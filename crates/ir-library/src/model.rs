//! The library data model and its TOML form.
//!
//! Exists to define what a saved code is (design decisions 10, 12, 13, 22,
//! 23): a named, tagged code with one of four payload kinds, an optional
//! carrier frequency and repeat mode that override the global defaults, and
//! notes, and when it was added. Byte payloads are stored as spaced uppercase
//! hex and times as ISO 8601, so the file stays readable and editable by hand.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Version of the file format written by this code.
pub const FORMAT_VERSION: u32 = 1;

/// All saved codes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Library {
    pub version: u32,
    #[serde(default, rename = "code", skip_serializing_if = "Vec::is_empty")]
    pub codes: Vec<Code>,
}

/// One saved code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Code {
    /// Stable identifier, unique within the library.
    pub id: u64,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Carrier frequency; `None` uses the global default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carrier_hz: Option<u32>,
    /// Repeat handling; `None` uses the global default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat: Option<RepeatMode>,
    pub payload: Payload,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// When the code was added, as ISO 8601 UTC (`2026-11-14T20:15:03Z`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<String>,
}

/// What is sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Payload {
    /// Bytes as learned by the dongle (plain format).
    Learned {
        #[serde(with = "hex_bytes")]
        data: Vec<u8>,
    },
    /// On/off durations in microseconds.
    Pulses { pulses: Vec<[u32; 2]> },
    /// NEC address and command (standard bit order).
    Nec {
        address: u16,
        command: u8,
        #[serde(default)]
        extended: bool,
    },
    /// Bytes written to the USB OUT endpoint unchanged.
    Raw {
        #[serde(with = "hex_bytes")]
        data: Vec<u8>,
    },
}

/// How repeat frames of a learned code are sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum RepeatMode {
    /// Exactly as learned, including captured repeat frames.
    AsLearned,
    /// Only the first frame, sent `count` times.
    FirstFrame { count: u32 },
}

/// Why a library could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryError {
    /// Not valid TOML or not the expected structure.
    Parse(String),
    /// Written by a newer version of the app.
    UnsupportedVersion(u32),
    /// Two codes share an id.
    DuplicateId(u64),
}

impl fmt::Display for LibraryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LibraryError::Parse(msg) => write!(f, "the library file is damaged: {msg}"),
            LibraryError::UnsupportedVersion(v) => {
                write!(f, "the library file has format version {v}, this app reads up to {FORMAT_VERSION}")
            }
            LibraryError::DuplicateId(id) => write!(f, "the library file has two codes with id {id}"),
        }
    }
}

impl std::error::Error for LibraryError {}

impl Default for Library {
    fn default() -> Self {
        Self { version: FORMAT_VERSION, codes: Vec::new() }
    }
}

impl Library {
    /// Parses and validates TOML text.
    pub fn from_toml(text: &str) -> Result<Self, LibraryError> {
        #[derive(Deserialize)]
        struct VersionOnly {
            version: u32,
        }
        let parse_err = |e: toml::de::Error| LibraryError::Parse(e.to_string());
        let VersionOnly { version } = toml::from_str(text).map_err(parse_err)?;
        if version > FORMAT_VERSION {
            return Err(LibraryError::UnsupportedVersion(version));
        }
        let library: Library = toml::from_str(text).map_err(parse_err)?;
        let mut ids = std::collections::HashSet::new();
        for code in &library.codes {
            if !ids.insert(code.id) {
                return Err(LibraryError::DuplicateId(code.id));
            }
        }
        Ok(library)
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).expect("library is always serializable")
    }

    /// Adds a code and returns its new id (one more than the highest id).
    pub fn add(&mut self, name: impl Into<String>, payload: Payload) -> u64 {
        let id = self.codes.iter().map(|c| c.id).max().unwrap_or(0) + 1;
        self.codes.push(Code {
            id,
            name: name.into(),
            tags: Vec::new(),
            carrier_hz: None,
            repeat: None,
            payload,
            notes: String::new(),
            added: None,
        });
        id
    }

    pub fn get(&self, id: u64) -> Option<&Code> {
        self.codes.iter().find(|c| c.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Code> {
        self.codes.iter_mut().find(|c| c.id == id)
    }

    pub fn remove(&mut self, id: u64) -> Option<Code> {
        let index = self.codes.iter().position(|c| c.id == id)?;
        Some(self.codes.remove(index))
    }

    /// All tags in use, sorted, without duplicates.
    pub fn all_tags(&self) -> Vec<String> {
        let tags: std::collections::BTreeSet<&String> = self.codes.iter().flat_map(|c| &c.tags).collect();
        tags.into_iter().cloned().collect()
    }

    /// Codes whose name or notes contain `query` (case-insensitive) and, if
    /// given, that carry `tag`. An empty query matches everything.
    pub fn search(&self, query: &str, tag: Option<&str>) -> Vec<&Code> {
        let query = query.to_lowercase();
        self.codes
            .iter()
            .filter(|c| tag.is_none_or(|t| c.tags.iter().any(|ct| ct == t)))
            .filter(|c| {
                query.is_empty() || c.name.to_lowercase().contains(&query) || c.notes.to_lowercase().contains(&query)
            })
            .collect()
    }
}

/// ISO 8601 UTC time for Unix seconds, e.g. `2026-11-14T20:15:03Z`.
pub fn iso_utc(unix_secs: u64) -> String {
    let days = unix_secs / 86_400;
    let secs = unix_secs % 86_400;
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm,
    // "chrono-Compatible Low-Level Date Algorithms").
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", secs / 3600, secs / 60 % 60, secs % 60)
}

/// Serde helpers for bytes stored as hex text.
mod hex_bytes {
    use ir_codec::{Encoding, FormatOptions};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        let options = FormatOptions { uppercase: true, bytes_per_line: 0 };
        s.serialize_str(&ir_codec::format(bytes, Encoding::Hex, &options))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(d)?;
        ir_codec::parse(&text, Encoding::Hex).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Library {
        let mut lib = Library::default();
        let tv = lib.add("TV power", Payload::Learned { data: vec![0xFF, 0x23, 0x37] });
        let code = lib.get_mut(tv).unwrap();
        code.tags = vec!["living room".into(), "tv".into()];
        code.carrier_hz = Some(38000);
        code.repeat = Some(RepeatMode::FirstFrame { count: 2 });
        code.notes = "Learned from the original remote".into();
        lib.add("TV mute", Payload::Nec { address: 0x00, command: 0x59, extended: false });
        lib.add("Strip red", Payload::Pulses { pulses: vec![[9000, 4500], [560, 0]] });
        let raw = lib.add("Raw test", Payload::Raw { data: vec![0xFC, 0xFC, 0xFC, 0xFC] });
        lib.get_mut(raw).unwrap().tags = vec!["tv".into(), "debug".into()];
        lib
    }

    #[test]
    fn all_payload_kinds_round_trip_through_toml() {
        let lib = sample();
        assert_eq!(Library::from_toml(&lib.to_toml()), Ok(lib));
    }

    #[test]
    fn documented_file_format_parses() {
        let text = r#"
version = 1

[[code]]
id = 1
name = "TV power"
tags = ["tv"]
carrier_hz = 38000
repeat = { mode = "first_frame", count = 2 }
payload = { kind = "learned", data = "FF 23 37" }

[[code]]
id = 2
name = "TV mute"
payload = { kind = "nec", address = 0, command = 89 }
"#;
        let lib = Library::from_toml(text).unwrap();
        assert_eq!(lib.codes.len(), 2);
        assert_eq!(lib.codes[0].payload, Payload::Learned { data: vec![0xFF, 0x23, 0x37] });
        assert_eq!(lib.codes[0].repeat, Some(RepeatMode::FirstFrame { count: 2 }));
        assert_eq!(lib.codes[1].payload, Payload::Nec { address: 0, command: 0x59, extended: false });
        assert_eq!(lib.codes[1].carrier_hz, None);
    }

    #[test]
    fn iso_times() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(iso_utc(1_794_596_116), "2026-11-13T18:55:16Z");
        assert_eq!(iso_utc(4_107_542_399), "2100-02-28T23:59:59Z");
    }

    #[test]
    fn added_time_round_trips() {
        let mut lib = Library::default();
        let id = lib.add("x", Payload::Raw { data: vec![1] });
        lib.get_mut(id).unwrap().added = Some(iso_utc(0));
        assert!(lib.to_toml().contains("added = \"1970-01-01T00:00:00Z\""));
        assert_eq!(Library::from_toml(&lib.to_toml()), Ok(lib));
    }

    #[test]
    fn bytes_are_written_as_spaced_hex() {
        let mut lib = Library::default();
        lib.add("x", Payload::Raw { data: vec![0x0A, 0xFF] });
        assert!(lib.to_toml().contains("\"0A FF\""), "{}", lib.to_toml());
    }

    #[test]
    fn empty_library_round_trips() {
        let lib = Library::default();
        assert_eq!(Library::from_toml(&lib.to_toml()), Ok(lib));
    }

    #[test]
    fn invalid_hex_is_a_parse_error() {
        let text = "version = 1\n[[code]]\nid = 1\nname = \"x\"\npayload = { kind = \"raw\", data = \"F\" }\n";
        assert!(matches!(Library::from_toml(text), Err(LibraryError::Parse(_))));
    }

    #[test]
    fn unknown_payload_kind_is_a_parse_error() {
        let text = "version = 1\n[[code]]\nid = 1\nname = \"x\"\npayload = { kind = \"magic\" }\n";
        assert!(matches!(Library::from_toml(text), Err(LibraryError::Parse(_))));
    }

    #[test]
    fn newer_format_version_is_rejected() {
        assert_eq!(Library::from_toml("version = 2\n"), Err(LibraryError::UnsupportedVersion(2)));
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let text = "version = 1\n[[code]]\nid = 3\nname = \"a\"\npayload = { kind = \"raw\", data = \"\" }\n[[code]]\nid = 3\nname = \"b\"\npayload = { kind = \"raw\", data = \"\" }\n";
        assert_eq!(Library::from_toml(text), Err(LibraryError::DuplicateId(3)));
    }

    #[test]
    fn ids_continue_after_the_highest() {
        let mut lib = sample();
        lib.remove(2);
        assert_eq!(lib.add("new", Payload::Raw { data: vec![] }), 5);
        assert!(lib.get(2).is_none());
        assert_eq!(Library::default().add("first", Payload::Raw { data: vec![] }), 1);
    }

    #[test]
    fn remove_returns_the_code() {
        let mut lib = sample();
        assert_eq!(lib.remove(1).map(|c| c.name), Some("TV power".to_string()));
        assert_eq!(lib.remove(1), None);
    }

    #[test]
    fn tags_are_sorted_and_unique() {
        assert_eq!(sample().all_tags(), vec!["debug", "living room", "tv"]);
    }

    #[test]
    fn search_by_text_and_tag() {
        let lib = sample();
        let names = |v: Vec<&Code>| v.into_iter().map(|c| c.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(lib.search("tv", None)), vec!["TV power", "TV mute"]);
        assert_eq!(names(lib.search("ORIGINAL", None)), vec!["TV power"], "matches notes");
        assert_eq!(names(lib.search("", Some("tv"))), vec!["TV power", "Raw test"]);
        assert_eq!(names(lib.search("mute", Some("tv"))), Vec::<String>::new());
        assert_eq!(lib.search("", None).len(), 4);
    }
}
