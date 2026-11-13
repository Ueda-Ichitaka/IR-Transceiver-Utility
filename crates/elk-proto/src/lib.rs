//! elk-proto - wire protocol of the ELK Smart USB IR dongle (045c:0132).
//!
//! Exists to keep every byte-level rule of the dongle in one place that has no
//! I/O, so it can be tested without hardware. The rules follow the reference
//! driver `elksmart-comm.c` (usb-drivers, 0BSD).
//!
//! Modules:
//! - [`command`]: the four-byte commands sent to the dongle
//! - [`identify`]: classifying the identify reply into a protocol variant
//! - [`learn`]: reassembling a learned IR code from bulk IN transfers
//! - [`pulse`]: decoding learned bytes into on/off pulse durations

pub mod command;
pub mod identify;
pub mod learn;
pub mod pulse;
