//! ir-formats - import and export of IR codes in other tools' formats.
//!
//! Exists so codes can be exchanged with other IR tools. Every format is
//! converted to and from on/off pulse timings plus a carrier frequency.
//!
//! Modules:
//! - [`pronto`]: Pronto hex, learned (0000) type

pub mod pronto;
