//! Thin BLE transport over btleplug.
//!
//! The important bit is [`pick_characteristics`]: instead of hardcoding which
//! vendor UUID is the "notify" channel and which is the "write" channel (that
//! split differs across detector models and firmware), we discover the
//! services and choose by GATT property flags. The notify channel is whichever
//! characteristic advertises NOTIFY/INDICATE; the write channel is whichever
//! advertises WRITE/WRITE_WITHOUT_RESPONSE.

use btleplug::api::{CharPropFlags, Characteristic, Peripheral as _, WriteType};
use btleplug::platform::Peripheral;
use uuid::Uuid;

use crate::error::{Error, Result};

/// The two characteristics openrd needs on a detector.
#[derive(Debug, Clone)]
pub struct Channels {
    /// Characteristic we subscribe to for the alert / reply stream.
    pub notify: Characteristic,
    /// Characteristic we write commands to (absent for read-only detectors).
    pub write: Option<Characteristic>,
    /// Preferred write type for the write characteristic.
    pub write_type: WriteType,
}

/// Connect, discover services, and locate the notify/write characteristics
/// within the given candidate service UUIDs.
pub async fn open(peripheral: &Peripheral, service_uuids: &[Uuid]) -> Result<Channels> {
    if !peripheral.is_connected().await? {
        peripheral.connect().await?;
    }
    peripheral.discover_services().await?;
    pick_characteristics(peripheral, service_uuids)
}

/// Choose the notify and write characteristics by property flags.
pub fn pick_characteristics(peripheral: &Peripheral, service_uuids: &[Uuid]) -> Result<Channels> {
    let in_scope = |c: &Characteristic| {
        service_uuids.is_empty() || service_uuids.contains(&c.service_uuid)
    };

    let chars = peripheral.characteristics();

    let notify = chars
        .iter()
        .filter(|c| in_scope(c))
        .find(|c| {
            c.properties
                .intersects(CharPropFlags::NOTIFY | CharPropFlags::INDICATE)
        })
        .cloned()
        .ok_or(Error::CharacteristicNotFound("notify/indicate"))?;

    let write = chars
        .iter()
        .filter(|c| in_scope(c))
        .find(|c| {
            c.properties
                .intersects(CharPropFlags::WRITE | CharPropFlags::WRITE_WITHOUT_RESPONSE)
        })
        .cloned();

    // Prefer WITHOUT_RESPONSE when the characteristic supports it: the Uniden
    // command/ack handshake rides the notify channel, not a GATT write-response.
    let write_type = match &write {
        Some(c) if c.properties.contains(CharPropFlags::WRITE_WITHOUT_RESPONSE) => {
            WriteType::WithoutResponse
        }
        _ => WriteType::WithResponse,
    };

    Ok(Channels {
        notify,
        write,
        write_type,
    })
}
