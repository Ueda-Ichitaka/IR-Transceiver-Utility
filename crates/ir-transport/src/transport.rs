//! The `Transport` trait: raw bulk OUT writes and bulk IN reads.
//!
//! Exists so protocol operations depend on a small interface instead of a USB
//! library; the nusb implementation and the test mock both implement it.

use std::fmt;
use std::time::Duration;

/// Failure of a single bulk transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    /// Nothing arrived (or could be sent) within the timeout.
    Timeout,
    /// The device was unplugged.
    Disconnected,
    /// Any other USB failure, with a description.
    Usb(String),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::Timeout => write!(f, "timeout"),
            TransportError::Disconnected => write!(f, "device disconnected"),
            TransportError::Usb(msg) => write!(f, "USB error: {msg}"),
        }
    }
}

impl std::error::Error for TransportError {}

/// Bulk transfers to and from the dongle. Reads return one transfer
/// (at most one 64-byte packet for this device).
pub trait Transport {
    fn write(&mut self, data: &[u8], timeout: Duration) -> Result<(), TransportError>;
    fn read(&mut self, timeout: Duration) -> Result<Vec<u8>, TransportError>;
}
