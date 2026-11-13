//! ir-library - the saved IR codes of the IR Transceiver Utility.
//!
//! Exists to keep the code library (a list of named, tagged codes stored as
//! TOML) independent of the GUI and fully tested.
//!
//! Modules:
//! - [`model`]: codes, payload kinds and the TOML file format
//! - [`store`]: file location, atomic saving and recovery from damaged files

pub mod model;
pub mod store;
