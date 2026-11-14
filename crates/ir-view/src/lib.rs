//! Qt-free view models for the GUI pages.
//!
//! Exists so page logic is tested without Qt (design decision on GUI tests):
//! `device` turns session events into the connection status and the actions
//! that are enabled, `capture` keeps the received signals and USB transfers
//! for the two capture tabs within the configured memory limit.

pub mod capture;
pub mod device;
