//! Error types for the RFM95 driver.

use std::io;

/// Errors that can occur when communicating with the RFM95 module.
#[derive(Debug, thiserror::Error)]
pub enum Rfm95Error {
    /// SPI bus communication error.
    #[error("SPI error: {0}")]
    Spi(#[from] io::Error),

    /// GPIO error from gpio-cdev.
    #[error("GPIO error: {0}")]
    Gpio(#[from] gpio_cdev::errors::Error),

    /// The chip version register returned an unexpected value.
    #[error("unexpected chip version: expected 0x{expected:#04x}, got 0x{actual:#04x}")]
    UnexpectedVersion { expected: u8, actual: u8 },

    /// Attempted to transmit a payload exceeding the FIFO size.
    #[error("payload too large: {size} bytes exceeds maximum of {max} bytes")]
    PayloadTooLarge { size: usize, max: usize },

    /// A timeout occurred waiting for an operation to complete.
    #[error("operation timed out after {0:?}")]
    Timeout(std::time::Duration),

    /// The radio is in an unexpected state for the requested operation.
    #[error("invalid radio state for this operation")]
    InvalidState,

    /// CRC error detected in the received packet.
    #[error("CRC error in received packet")]
    CrcError,

    /// Received an invalid or out-of-range parameter.
    #[error("invalid parameter: {0}")]
    InvalidParameter(String),
}

/// Convenience alias for `Result<T, Rfm95Error>`.
pub type Result<T> = std::result::Result<T, Rfm95Error>;