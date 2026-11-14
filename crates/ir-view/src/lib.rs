//! Qt-free view models for the GUI pages.
//!
//! Exists so page logic is tested without Qt (design decision on GUI tests):
//! `app` holds the whole state and applies batches of session events,
//! `device` turns session events into the connection status and the actions
//! that are enabled, `capture` keeps the received signals and USB transfers
//! for the two capture tabs within the configured memory limit, `display`
//! maps a view's settings to its encoding switch, `send` turns the Send form
//! into a session command, `library` describes and edits saved codes.

pub mod app;
pub mod capture;
pub mod device;
pub mod display;
pub mod library;
pub mod send;
