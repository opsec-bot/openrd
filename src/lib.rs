//! # openrd
//!
//! A cross-platform Rust client for Bluetooth LE radar detectors.
//!
//! openrd scans for a detector, connects, streams decoded [`Alert`]s, and (for
//! Uniden) writes settings and mutes. It is built on
//! [`btleplug`](https://docs.rs/btleplug), so it runs on Windows, macOS and
//! Linux with no platform-specific code.
//!
//! Supported detectors:
//!
//! | Model | Alerts | Settings / mute | Protocol source |
//! |-------|:------:|:---------------:|-----------------|
//! | Uniden R4 / R8 / R9 | yes | yes | reverse-engineered (`PROTOCOL.md`) |
//! | Valentine One Gen2 | best-effort | — | public ESP spec |
//! | Radenso DS1 | raw frames | — | scaffold, PRs welcome |
//!
//! ## Example
//!
//! ```no_run
//! use futures::StreamExt;
//! use openrd::Event;
//! use std::time::Duration;
//!
//! #[tokio::main]
//! async fn main() -> openrd::Result<()> {
//!     // Find the first detector advertising nearby.
//!     let detector = openrd::scan(Duration::from_secs(5))
//!         .await?
//!         .into_iter()
//!         .next()
//!         .expect("no detector found");
//!
//!     println!("connecting to {}", detector.kind().name());
//!     let session = detector.connect().await?;
//!
//!     let mut events = Box::pin(session.events());
//!     while let Some(event) = events.next().await {
//!         if let Event::Alert(alert) = event {
//!             println!("{alert}");
//!         }
//!     }
//!     Ok(())
//! }
//! ```

mod alert;
mod detect;
mod error;
mod session;
mod transport;

pub use alert::{Alert, Band, Direction, Event};
pub use detect::DetectorKind;
pub use error::{Error, Result};
pub use session::Session;

use std::time::Duration;

use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter};
use btleplug::platform::{Manager, Peripheral};

/// A detector found during a [`scan`], not yet connected.
pub struct Discovered {
    kind: DetectorKind,
    name: String,
    peripheral: Peripheral,
}

impl Discovered {
    /// The detector model, inferred from its advertised name.
    pub fn kind(&self) -> DetectorKind {
        self.kind
    }

    /// The BLE name the detector advertised.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Connect and start a live [`Session`].
    pub async fn connect(self) -> Result<Session> {
        Session::start(self.peripheral, self.kind).await
    }
}

/// Scan for supported detectors for `duration`, then return everything found.
///
/// Uses the first Bluetooth adapter on the system. A detector is "supported" if
/// its advertised name matches a known model (see
/// [`DetectorKind::from_advertised_name`]).
pub async fn scan(duration: Duration) -> Result<Vec<Discovered>> {
    let manager = Manager::new().await?;
    let adapter = manager
        .adapters()
        .await?
        .into_iter()
        .next()
        .ok_or(Error::NoAdapter)?;

    adapter.start_scan(ScanFilter::default()).await?;
    tokio::time::sleep(duration).await;
    adapter.stop_scan().await?;

    let mut found = Vec::new();
    for peripheral in adapter.peripherals().await? {
        let name = match peripheral.properties().await? {
            Some(props) => props.local_name,
            None => None,
        };
        let Some(name) = name else { continue };
        let Some(kind) = DetectorKind::from_advertised_name(&name) else {
            continue;
        };
        found.push(Discovered {
            kind,
            name,
            peripheral,
        });
    }
    Ok(found)
}
