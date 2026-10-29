//! Model-independent alert types.
//!
//! Every detector reports threats differently on the wire, but they all boil
//! down to "a signal on some band, from some direction, at some strength".
//! [`Alert`] is that common shape. Each detector module is responsible for
//! turning its own wire format into one or more of these.

use std::fmt;

/// Radar / laser band of a threat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Band {
    /// X band (~10.5 GHz).
    X,
    /// K band (~24.1 GHz).
    K,
    /// Ka band (~33.4–36 GHz).
    Ka,
    /// Laser (lidar).
    Laser,
    /// K/Ka "POP" quick-trigger.
    Pop,
    /// MultaRadar CD (MRCD).
    Mrcd,
    /// MultaRadar CT (MRCT).
    Mrct,
    /// Any band label the firmware sent that we don't have a variant for
    /// (e.g. a user-relabelled band). The raw string is preserved.
    Other(String),
}

impl Band {
    /// Parse a Uniden-style band token (`"X"`, `"K"`, `"KA"`, `"LASER"`, ...).
    pub fn parse_uniden(token: &str) -> Band {
        match token {
            "X" => Band::X,
            "K" => Band::K,
            "KA" => Band::Ka,
            "LASER" => Band::Laser,
            "POP" => Band::Pop,
            "MRCD" => Band::Mrcd,
            "MRCT" => Band::Mrct,
            other => Band::Other(other.to_string()),
        }
    }
}

impl fmt::Display for Band {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Band::X => write!(f, "X"),
            Band::K => write!(f, "K"),
            Band::Ka => write!(f, "Ka"),
            Band::Laser => write!(f, "Laser"),
            Band::Pop => write!(f, "POP"),
            Band::Mrcd => write!(f, "MRCD"),
            Band::Mrct => write!(f, "MRCT"),
            Band::Other(s) => write!(f, "{s}"),
        }
    }
}

/// Direction of a threat relative to the vehicle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Front,
    Side,
    Rear,
    /// Not reported, or the detector has no directional antennas.
    Unknown,
}

impl Direction {
    /// Parse a Uniden-style direction token (`"F"`, `"S"`, `"R"`).
    pub fn parse_uniden(token: &str) -> Direction {
        match token {
            "F" => Direction::Front,
            "S" => Direction::Side,
            "R" => Direction::Rear,
            _ => Direction::Unknown,
        }
    }
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Direction::Front => "front",
            Direction::Side => "side",
            Direction::Rear => "rear",
            Direction::Unknown => "?",
        };
        write!(f, "{s}")
    }
}

/// A single threat as reported by a detector at one instant.
#[derive(Debug, Clone, PartialEq)]
pub struct Alert {
    /// Band of the signal.
    pub band: Band,
    /// Centre frequency in GHz, when the detector reports one (radar only).
    pub frequency_ghz: Option<f64>,
    /// Direction of the strongest lobe.
    pub direction: Direction,
    /// Front-antenna signal strength as sent by the firmware (0 = none).
    ///
    /// Uniden sends this as an integer; its scale is firmware-defined. Treat it
    /// as a relative magnitude, not a calibrated value.
    pub front_strength: u8,
    /// Rear-antenna signal strength as sent by the firmware (0 = none).
    pub rear_strength: u8,
    /// Priority / bogey rank, 1 = highest-priority alert. Uniden sends 1..=6.
    pub priority: u8,
    /// The raw per-alert wire record this was decoded from, for debugging.
    pub raw: String,
}

impl fmt::Display for Alert {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:<5} ", self.band.to_string())?;
        if let Some(ghz) = self.frequency_ghz {
            write!(f, "{ghz:7.3} GHz ")?;
        } else {
            write!(f, "{:11} ", "")?;
        }
        write!(
            f,
            "{:<5} front={:<3} rear={:<3} prio={}",
            self.direction.to_string(),
            self.front_strength,
            self.rear_strength,
            self.priority
        )
    }
}

/// An event delivered on a [`crate::Session`] stream.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A decoded threat (Uniden, or a best-effort decode for Valentine).
    Alert(Alert),
    /// A raw Valentine ESP packet that framed cleanly but we did not decode
    /// into an [`Alert`]. `id` is the ESP packet id.
    EspPacket { id: u8, payload: Vec<u8> },
    /// Raw notification bytes from a detector whose payload decoding is not yet
    /// implemented (currently Radenso). Decode these and send a PR.
    Raw(Vec<u8>),
}
