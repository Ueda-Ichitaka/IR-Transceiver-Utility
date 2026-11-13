//! ir-transport - USB transport for the ELK Smart IR dongle (045c:0132).
//!
//! Exists to separate raw bulk I/O from protocol logic: operations are written
//! against the [`transport::Transport`] trait so they can be tested with a
//! scripted mock.
//!
//! Modules:
//! - [`transport`]: the trait and its error type
//! - [`mock`]: scripted transport for tests
//! - [`recorder`]: wrapper that logs every transfer (fixtures, USB log)
//! - [`endpoints`]: picks bulk OUT/IN endpoints from descriptors
//! - [`ops`]: identify and learn sequences
//! - [`usb`]: the nusb-backed transport for the real dongle
//! - [`probe`]: testable parts of the elk-probe tool

pub mod endpoints;
pub mod mock;
pub mod ops;
pub mod probe;
pub mod recorder;
pub mod transport;
pub mod usb;
