use thiserror::Error;

/// Errors returned by openrd.
#[derive(Debug, Error)]
pub enum Error {
    /// The underlying btleplug stack returned an error.
    #[error("bluetooth error: {0}")]
    Bluetooth(#[from] btleplug::Error),

    /// No Bluetooth adapter was found on this machine.
    #[error("no bluetooth adapter found")]
    NoAdapter,

    /// The peripheral did not expose the GATT characteristics this detector needs.
    ///
    /// Either the device is not the detector we think it is, or its firmware lays
    /// the service out differently. The message names what was missing.
    #[error("required GATT characteristic not found: {0}")]
    CharacteristicNotFound(&'static str),

    /// A command was sent but the detector did not acknowledge it in time.
    #[error("timed out waiting for detector to acknowledge `{0}`")]
    CommandTimeout(String),

    /// The detector replied, but not with the success acknowledgement we expected.
    #[error("detector rejected command `{sent}`: replied `{got}`")]
    CommandRejected { sent: String, got: String },

    /// This operation is not supported for the connected detector model.
    ///
    /// For example, writing settings is only implemented for Uniden.
    #[error("operation not supported for {0:?}")]
    Unsupported(crate::DetectorKind),
}

/// Convenience result type for openrd.
pub type Result<T> = std::result::Result<T, Error>;
