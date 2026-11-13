//! ir-session - the device session of the IR Transceiver Utility.
//!
//! Exists to keep everything between the GUI and the USB transport testable
//! without Qt or hardware.
//!
//! Modules:
//! - [`budget`]: capture limit and low-memory guard, from /proc/meminfo
//! - [`capture`]: capture buffer bounded by a byte budget
//! - [`session`]: the state machine over one dongle connection
//! - [`worker`]: runs a session on its own thread
//! - [`usb`]: the connector for real dongles

pub mod budget;
pub mod capture;
pub mod session;
pub mod usb;
pub mod worker;
