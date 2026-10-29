# Detector Bluetooth protocols

Reference for the wire protocols `openrd` implements. The Uniden section was
recovered by observing a shipping detector companion app; the Valentine section
is from Valentine Research's published ESP specification; Radenso is a scaffold.

GATT characteristic **roles are discovered at runtime** from property flags
(`NOTIFY` vs `WRITE`), so the "which UUID is which" question below is
informational: `openrd` does not depend on it.

---

## Uniden R4 / R8 / R9 (and `w` variants)

A text/ASCII protocol over GATT. No binary framing, no encryption.

### GATT services

Two vendor services are advertised; across firmware one carries the alert
notify stream and the other carries writes. `openrd` selects by property flag.

```
Service  18424398-7cbc-11e9-8f9e-2a86e4085a59
  char   2d86686a-53dc-25b3-0c4a-f0e10c8dee20
  char   6eb675ab-8bd1-1b9a-7444-621e52ec6823
Service  1842467c-7cbc-11e9-8f9e-2a86e4085a59
  char   2c86686a-53dc-25b3-0c4a-f0e10c8dee20
  char   5987b4ef-3bfa-76a8-e642-92933c31434f

Standard Device Information service 0000180a-… exposes the firmware string
via Software Revision 00002a28-….
```

### Alert stream (detector → app, via NOTIFY)

One notification carries all current alerts. Alerts are joined with `&`; each
alert is a comma-separated record:

```
index , count , BAND , frontStrength , rearStrength , freqGHz , DIR , priority
```

| Field | Meaning |
|-------|---------|
| `index` | Alert slot number. `0` = empty slot, skip it. |
| `count` | Number of active alerts. |
| `BAND` | `X`, `K`, `KA`, `LASER`, `POP`, `MRCD`, `MRCT`, or a user-relabelled band. |
| `frontStrength` | Front-antenna signal magnitude (integer, firmware scale). |
| `rearStrength` | Rear-antenna signal magnitude. |
| `freqGHz` | Centre frequency in GHz, e.g. `34.700`. `0` when not applicable. |
| `DIR` | `F` front, `S` side, `R` rear, `0` none. |
| `priority` | Priority / bogey rank, `1`..`6`, `1` = highest priority. |

Example notification payload:

```
1,2,KA,12,0,34.700,F,1&2,2,K,5,3,24.150,R,2
```

→ two alerts: a priority Ka threat ahead at 34.700 GHz, and a weaker K threat
behind at 24.150 GHz.

> The band/direction/frequency fields are confirmed from the app's parser. The
> two strength fields and `count`/`priority` are cross-checked against
> community documentation of the Uniden BT output; treat their exact numeric
> scale as firmware-defined.

### Commands (app → detector, via WRITE)

Written as ASCII strings to the write characteristic. The detector acknowledges
on the **notify** channel (same stream as alerts).

| Command written | Meaning | Acknowledgement |
|-----------------|---------|-----------------|
| `BTreqSETC:<index>=<value>` | Set one setting by storage index. | `RDrptSETC:1` |
| `BTreqMUTE:<0\|1>` | `1` mute current alert, `0` unmute. | `RDrptMUTE:<value>=1` |

Settings are written one at a time; send the next after the ack. `openrd`
serializes commands and matches each ack to the pending command by order
(`Session::set_setting`, `Session::mute`).

### Settings index map

`index`/`value` are the detector's own menu storage indices. The companion app
ships a full map (band sensitivities, city/highway mode, filters, Ka segments,
display colours, tones, etc.). `openrd` does not bundle that schema — it exposes
the raw `set_setting(index, value)` so you can drive any setting. A few common
ones (R8 menu):

| Setting | index | values |
|---------|------:|--------|
| Basic/Expert Mode | 0 | 0 Basic, 1 Expert |
| City Mode | 1 | 0 Highway, 1 City, 2 Auto, 3 Advanced |
| X Band Sensitivity | 2 | 0..7 (30%..100%) |

Dump your own detector's full map by walking the app's settings definitions, or
contribute one as `uniden_settings.rs`.

---

## Valentine One Gen2 (ESP over BLE)

Valentine publishes the **ESP Specification**. `openrd` implements the public
framing and a best-effort `infDisplayData` decode.

### GATT service

```
Service 92a0b8d2-9e05-11e2-aa59-f23c91aec05e   (V1connection LE)
  characteristics in the 92a0…-9e05-11e2-aa59-f23c91aec05e family
Legacy classic SPP 00001101-… is also present for V1connection (non-LE).
```

### ESP frame

```
AA  destId  origId  packetId  payloadLen  payload…  checksum  AB
```

- `AA` start of frame, `AB` end of frame.
- `packetId 0x31` = `infDisplayData`, the live alert/display frame.
- `checksum` is the 8-bit sum of the bytes from `destId` through the last
  payload byte.

`openrd` deframes on `AA`/`AB`, and for `infDisplayData` reads the band bitfield
and arrow (direction) bitfield into an `Alert`. **Validate the `infDisplayData`
byte offsets against your own V1's firmware before trusting them** — the decode
is intentionally conservative and non-`infDisplayData` packets are passed up raw
as `Event::EspPacket` for you to decode. Frequency is reported in a separate ESP
packet and is not yet merged into the alert.

---

## Radenso DS1

Scaffolded. The DS1 vendor characteristic observed:

```
258eafa5-e914-47da-95ca-c5ab0dc85b11
```

alongside the Uniden-style `2c86686a` / `2d86686a` characteristics. `openrd`
connects, subscribes, and delivers notifications as `Event::Raw(bytes)`. Decode
the DS1 payload and send a PR to turn these into `Alert`s.

---

## Contributing a detector

1. Add a `DetectorKind` variant and its name match in `src/detect.rs`.
2. List its candidate service UUIDs in `service_uuids()`.
3. Implement `decode(payload) -> Vec<Event>` for its notifications.

The scan, connect, characteristic-role detection and event stream are shared, so
a new read-only detector is usually just those three pieces.
