//! ir-session - the device session of the IR Transceiver Utility.
//!
//! Exists to keep everything between the GUI and the USB transport testable
//! without Qt or hardware.
//!
//! Modules:
//! - [`budget`]: memory budget for captured data, from /proc/meminfo
//! - [`capture`]: capture buffer bounded by a byte budget

pub mod budget;
pub mod capture;
