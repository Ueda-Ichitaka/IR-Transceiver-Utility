//! Four-byte commands sent to the dongle on the bulk OUT endpoint.
//!
//! Exists so the magic byte sequences are named once and shared by the
//! parsers and the transport layer.

/// Header of a transmit message.
pub const TRANSMIT: [u8; 4] = [0xFF; 4];
/// Arms the IR receiver; the dongle replies once with the next code it sees.
pub const LEARN: [u8; 4] = [0xFE; 4];
/// Sent after a learn completes.
pub const STOP: [u8; 4] = [0xFD; 4];
/// Asks the dongle which protocol variant it speaks.
pub const IDENTIFY: [u8; 4] = [0xFC; 4];
