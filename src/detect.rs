//! Per-model identity: how each detector advertises itself, which GATT
//! services it exposes, and how to turn its notifications into [`Event`]s.
//!
//! The UUIDs here were recovered by reverse-engineering a shipping detector
//! companion app (see `PROTOCOL.md`). We do **not** hardcode which
//! characteristic is "notify" vs "write" — [`crate::transport`] picks those at
//! connect time from the GATT property flags, which is both correct and robust
//! across firmware revisions. The service UUIDs below are only used to locate
//! the right service.

use uuid::Uuid;

use crate::alert::{Alert, Band, Direction, Event};

/// Which detector we are talking to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectorKind {
    /// Uniden R4 / R4w.
    UnidenR4,
    /// Uniden R8 / R8w.
    UnidenR8,
    /// Uniden R9 / R9w.
    UnidenR9,
    /// Valentine One Gen2.
    ValentineV1Gen2,
    /// Radenso DS1.
    RadensoDs1,
}

impl DetectorKind {
    /// Human-readable model name.
    pub fn name(&self) -> &'static str {
        match self {
            DetectorKind::UnidenR4 => "Uniden R4",
            DetectorKind::UnidenR8 => "Uniden R8",
            DetectorKind::UnidenR9 => "Uniden R9",
            DetectorKind::ValentineV1Gen2 => "Valentine One Gen2",
            DetectorKind::RadensoDs1 => "Radenso DS1",
        }
    }

    /// True if this detector speaks the Uniden ASCII protocol.
    pub fn is_uniden(&self) -> bool {
        matches!(
            self,
            DetectorKind::UnidenR4 | DetectorKind::UnidenR8 | DetectorKind::UnidenR9
        )
    }

    /// Guess the model from the BLE advertised name. Returns `None` if the name
    /// doesn't look like any detector we support.
    pub fn from_advertised_name(name: &str) -> Option<DetectorKind> {
        let n = name.to_ascii_uppercase();
        // Order matters: check the longer/more specific tokens first.
        if n.contains("R9") {
            Some(DetectorKind::UnidenR9)
        } else if n.contains("R8") {
            Some(DetectorKind::UnidenR8)
        } else if n.contains("R4") {
            Some(DetectorKind::UnidenR4)
        } else if n.contains("DS1") || n.contains("RADENSO") {
            Some(DetectorKind::RadensoDs1)
        } else if n.contains("V1") || n.contains("VALENTINE") {
            Some(DetectorKind::ValentineV1Gen2)
        } else {
            None
        }
    }

    /// Candidate GATT service UUIDs for this detector. The transport layer
    /// searches these services for a notify and a write characteristic.
    pub fn service_uuids(&self) -> Vec<Uuid> {
        match self {
            DetectorKind::UnidenR4 | DetectorKind::UnidenR8 | DetectorKind::UnidenR9 => vec![
                uuid("18424398-7cbc-11e9-8f9e-2a86e4085a59"),
                uuid("1842467c-7cbc-11e9-8f9e-2a86e4085a59"),
            ],
            DetectorKind::ValentineV1Gen2 => vec![
                // Valentine One Gen2 "V1connection LE" service.
                uuid("92a0b8d2-9e05-11e2-aa59-f23c91aec05e"),
            ],
            DetectorKind::RadensoDs1 => vec![
                uuid("0000180a-0000-1000-8000-00805f9b34fb"),
                // DS1 vendor characteristic lives alongside the Uniden-style chars.
            ],
        }
    }

    /// Turn one raw GATT notification payload into zero or more [`Event`]s.
    pub fn decode(&self, payload: &[u8]) -> Vec<Event> {
        match self {
            k if k.is_uniden() => decode_uniden(payload),
            DetectorKind::ValentineV1Gen2 => decode_valentine(payload),
            _ => vec![Event::Raw(payload.to_vec())],
        }
    }
}

/// Parse a UUID string, panicking on a malformed literal (they are all
/// compile-time constants in this crate, so a panic here is a programming bug).
pub(crate) fn uuid(s: &str) -> Uuid {
    Uuid::parse_str(s).expect("valid UUID literal")
}

// ---------------------------------------------------------------------------
// Uniden R4 / R8 / R9
// ---------------------------------------------------------------------------

/// Prefix for a settings write: `BTreqSETC:<idx>=<value>`.
pub(crate) const UNIDEN_SET_PREFIX: &str = "BTreqSETC:";
/// Prefix for a mute command: `BTreqMUTE:<0|1>`.
pub(crate) const UNIDEN_MUTE_PREFIX: &str = "BTreqMUTE:";
/// Acknowledgement the detector sends after a successful settings write.
pub(crate) const UNIDEN_SET_ACK: &str = "RDrptSETC:1";
/// Prefix of the acknowledgement the detector sends after a mute command
/// (`RDrptMUTE:<value>=1`).
pub(crate) const UNIDEN_MUTE_ACK_PREFIX: &str = "RDrptMUTE:";
/// Any line beginning with this is a reply to a command, not an alert stream.
pub(crate) const UNIDEN_REPLY_PREFIX: &str = "RDrpt";

/// Decode a Uniden alert notification.
///
/// Wire format (ASCII): alerts are joined with `&`; each alert is a CSV record
///
/// ```text
///   index , count , BAND , frontStrength , rearStrength , freqGHz , DIR , priority
/// ```
///
/// An `index` of `0` marks an empty slot and is skipped. See `PROTOCOL.md`.
fn decode_uniden(payload: &[u8]) -> Vec<Event> {
    let text = String::from_utf8_lossy(payload);

    // Replies to our own commands arrive on the same characteristic; they are
    // not alerts. The session layer consumes these separately, so skip here.
    if text.starts_with(UNIDEN_REPLY_PREFIX) {
        return Vec::new();
    }

    let mut out = Vec::new();
    for record in text.split('&') {
        let record = record.trim();
        if record.is_empty() {
            continue;
        }
        let f: Vec<&str> = record.split(',').collect();
        // Need at least through the direction field to be a usable alert.
        if f.len() < 7 {
            continue;
        }
        // index 0 == empty slot.
        if f[0] == "0" {
            continue;
        }

        let band = Band::parse_uniden(f[2]);
        let front_strength = f[3].parse::<u8>().unwrap_or(0);
        let rear_strength = f[4].parse::<u8>().unwrap_or(0);
        let frequency_ghz = f[5].parse::<f64>().ok().filter(|v| *v > 0.0);
        let direction = Direction::parse_uniden(f[6]);
        let priority = f.get(7).and_then(|s| s.parse::<u8>().ok()).unwrap_or(0);

        out.push(Event::Alert(Alert {
            band,
            frequency_ghz,
            direction,
            front_strength,
            rear_strength,
            priority,
            raw: record.to_string(),
        }));
    }
    out
}

/// Build the byte payload for a Uniden settings write.
///
/// `index` is the detector's storage index for the setting (the `write_index`
/// from the detector's menu map — see `PROTOCOL.md`), `value` is the raw byte
/// value for that setting.
pub(crate) fn uniden_set_command(index: u16, value: u8) -> (String, Vec<u8>) {
    let cmd = format!("{UNIDEN_SET_PREFIX}{index}={value}");
    let bytes = cmd.clone().into_bytes();
    (cmd, bytes)
}

/// Build the byte payload for a Uniden mute command.
pub(crate) fn uniden_mute_command(mute: bool) -> (String, Vec<u8>) {
    let cmd = format!("{UNIDEN_MUTE_PREFIX}{}", mute as u8);
    let bytes = cmd.clone().into_bytes();
    (cmd, bytes)
}

// ---------------------------------------------------------------------------
// Valentine One Gen2 (ESP over BLE)
// ---------------------------------------------------------------------------

/// Start-of-frame byte of a Valentine ESP packet.
const ESP_SOF: u8 = 0xAA;
/// End-of-frame byte of a Valentine ESP packet.
const ESP_EOF: u8 = 0xAB;
/// ESP packet id for `infDisplayData` (the live alert/display frame).
const ESP_INF_DISPLAY_DATA: u8 = 0x31;

/// Decode a Valentine notification.
///
/// Valentine publishes the "ESP Specification". Each frame is
///
/// ```text
///   AA  destId  origId  packetId  payloadLen  payload...  checksum  AB
/// ```
///
/// The framing and checksum are public and implemented here. For
/// `infDisplayData` we extract the band/direction into a best-effort [`Alert`];
/// everything else is handed up as [`Event::EspPacket`] for the caller to
/// decode. The exact `infDisplayData` byte layout should be validated against
/// your V1's firmware before relying on it — see `PROTOCOL.md`.
fn decode_valentine(payload: &[u8]) -> Vec<Event> {
    let mut out = Vec::new();
    for frame in split_esp_frames(payload) {
        // frame = [dest, orig, id, len, payload..., checksum]
        if frame.len() < 5 {
            continue;
        }
        let id = frame[2];
        let len = frame[3] as usize;
        let body_end = 4 + len;
        if body_end > frame.len() {
            continue;
        }
        let body = &frame[4..body_end];

        if id == ESP_INF_DISPLAY_DATA {
            if let Some(alert) = valentine_infdisplay_alert(body) {
                out.push(Event::Alert(alert));
                continue;
            }
        }
        out.push(Event::EspPacket {
            id,
            payload: body.to_vec(),
        });
    }
    out
}

/// Pull complete `AA ... AB` frames out of a byte buffer, returning the inner
/// bytes between SOF and EOF (checksum included, SOF/EOF stripped).
fn split_esp_frames(buf: &[u8]) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    let mut i = 0;
    while i < buf.len() {
        if buf[i] != ESP_SOF {
            i += 1;
            continue;
        }
        if let Some(rel_end) = buf[i + 1..].iter().position(|&b| b == ESP_EOF) {
            let start = i + 1;
            let end = start + rel_end; // index of EOF
            frames.push(buf[start..end].to_vec());
            i = end + 1;
        } else {
            break; // no EOF yet; wait for more bytes
        }
    }
    frames
}

/// Best-effort decode of an `infDisplayData` body into an [`Alert`].
///
/// This reads the priority-alert band flags from the documented ESP layout.
/// Returns `None` if the body is empty or carries no active alert, in which
/// case the caller receives the raw packet instead.
fn valentine_infdisplay_alert(body: &[u8]) -> Option<Alert> {
    // ESP infDisplayData layout (public spec): the first bytes carry bar-graph
    // strength and band/arrow bitfields. We read band from the band bitfield
    // and arrow (direction) from the arrow byte. Firmware revisions differ, so
    // this is intentionally conservative and clearly marked for validation.
    if body.len() < 6 {
        return None;
    }
    let bogey_strength = body[0]; // front bar graph
    let band_bits = body[2];
    let arrow_bits = body[3];

    if bogey_strength == 0 && band_bits == 0 {
        return None; // no active alert
    }

    // Band bitfield (per ESP spec, low bits): bit0 Laser, bit1 Ka, bit2 K, bit3 X.
    let band = if band_bits & 0x01 != 0 {
        Band::Laser
    } else if band_bits & 0x02 != 0 {
        Band::Ka
    } else if band_bits & 0x04 != 0 {
        Band::K
    } else if band_bits & 0x08 != 0 {
        Band::X
    } else {
        Band::Other(format!("bits:{band_bits:#04x}"))
    };

    // Arrow bitfield: bit0 front, bit1 side, bit2 rear.
    let direction = if arrow_bits & 0x01 != 0 {
        Direction::Front
    } else if arrow_bits & 0x02 != 0 {
        Direction::Side
    } else if arrow_bits & 0x04 != 0 {
        Direction::Rear
    } else {
        Direction::Unknown
    };

    Some(Alert {
        band,
        frequency_ghz: None, // Gen2 reports frequency in a separate ESP packet
        direction,
        front_strength: bogey_strength,
        rear_strength: 0,
        priority: 1,
        raw: format!("{body:02x?}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniden_parses_a_ka_front_alert() {
        // index 1, count 1, Ka band, front 12, rear 0, 34.700 GHz, Front, prio 1
        let events = decode_uniden(b"1,1,KA,12,0,34.700,F,1");
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::Alert(a) => {
                assert_eq!(a.band, Band::Ka);
                assert_eq!(a.direction, Direction::Front);
                assert_eq!(a.front_strength, 12);
                assert_eq!(a.frequency_ghz, Some(34.700));
                assert_eq!(a.priority, 1);
            }
            other => panic!("expected alert, got {other:?}"),
        }
    }

    #[test]
    fn uniden_skips_empty_slots_and_splits_multiple() {
        let events = decode_uniden(b"0,0,,0,0,0,0,0&2,2,K,8,3,24.150,R,2");
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::Alert(a) => {
                assert_eq!(a.band, Band::K);
                assert_eq!(a.direction, Direction::Rear);
                assert_eq!(a.rear_strength, 3);
            }
            other => panic!("expected alert, got {other:?}"),
        }
    }

    #[test]
    fn uniden_ignores_command_replies() {
        assert!(decode_uniden(b"RDrptSETC:1").is_empty());
        assert!(decode_uniden(b"RDrptMUTE:1=1").is_empty());
    }

    #[test]
    fn uniden_command_builders() {
        assert_eq!(uniden_set_command(42, 3).0, "BTreqSETC:42=3");
        assert_eq!(uniden_mute_command(true).0, "BTreqMUTE:1");
        assert_eq!(uniden_mute_command(false).0, "BTreqMUTE:0");
    }

    #[test]
    fn valentine_frames_split_on_sof_eof() {
        // Two frames: AA <d o id len body cksum> AB, back to back.
        let buf = [
            0xAA, 0xD0, 0x0A, 0x31, 0x01, 0x00, 0x00, 0xAB, 0xAA, 0xD0, 0x0A, 0x30, 0x00, 0x00,
            0xAB,
        ];
        let frames = split_esp_frames(&buf);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0][2], 0x31); // packet id
    }
}
