//! Qt-free view models for the GUI pages.
//!
//! Exists so page logic is tested without Qt (design decision on GUI tests):
//! `app` holds the whole state and applies batches of session events,
//! `device` turns session events into the connection status and the actions
//! that are enabled, `capture` keeps the received signals and USB transfers
//! for the two capture tabs within the configured memory limit and
//! `capture_export` writes them as JSON, CSV or raw bytes, `display`
//! maps a view's settings to its encoding switch, `send` turns the Send form
//! into a session command, `library` describes and edits saved codes, `text`
//! formats messages, `exchange` converts to and from LIRC, Flipper and Pronto,
//! `files` picks export contents and recognises imported files, `settings_form`
//! edits the settings, `replay` turns an elk-probe log into session events.

pub mod app;
pub mod capture;
pub mod capture_export;
pub mod device;
pub mod display;
pub mod exchange;
pub mod files;
pub mod library;
pub mod replay;
pub mod send;
pub mod settings_form;
pub mod text;
