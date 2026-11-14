//! User settings and their TOML file.
//!
//! Exists to keep every user-adjustable default in one typed place (design
//! decisions 12, 13, 15, 19, 20 and the settings-store decision): the global
//! carrier frequency and repeat mode for sending, the encoding switch options
//! of each view, the capture limit and the preferred dongle. Missing fields
//! take their defaults, so older files keep loading; a damaged file is moved
//! aside like the library file.

use std::io;
use std::path::{Path, PathBuf};

use ir_codec::{Encoding, FormatOptions};
use ir_library::model::RepeatMode;
use ir_library::send::Defaults;
use ir_library::store;
use serde::{Deserialize, Serialize};

/// Version of the file format written by this code.
pub const FORMAT_VERSION: u32 = 1;
pub const FILE_NAME: &str = "settings.toml";

/// All settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub send: SendSettings,
    pub capture: CaptureSettings,
    pub views: Views,
    pub device: DeviceSettings,
}

/// Global defaults for sending; a code's own values override them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SendSettings {
    pub carrier_hz: u32,
    pub repeat: RepeatMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptureSettings {
    /// Capture stops when the captured data reaches this size.
    pub limit_mib: u64,
}

/// The encoding switch options of each view (the capture page has one per
/// tab: received signals and USB transfers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Views {
    pub capture: ViewSettings,
    pub transfers: ViewSettings,
    pub send: ViewSettings,
    pub library: ViewSettings,
}

/// One view's encoding switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewSettings {
    #[serde(with = "encoding_name")]
    pub encoding: Encoding,
    pub uppercase: bool,
    /// Bytes per line; 0 keeps everything on one line.
    pub bytes_per_line: usize,
    /// Whether the switch also offers decoded pulse timings.
    pub offer_pulses: bool,
    /// Pulse timings are selected instead of `encoding` (only while
    /// `offer_pulses` is set).
    pub show_pulses: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceSettings {
    /// Serial number of the dongle to select; the first one found is used
    /// when it is unset or not connected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_serial: Option<String>,
}

/// Why a settings file could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsError {
    Parse(String),
    UnsupportedVersion(u32),
}

impl std::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SettingsError::Parse(msg) => write!(f, "the settings file is damaged: {msg}"),
            SettingsError::UnsupportedVersion(v) => {
                write!(f, "the settings file has format version {v}, this app reads up to {FORMAT_VERSION}")
            }
        }
    }
}

impl std::error::Error for SettingsError {}

/// Result of loading the settings file.
#[derive(Debug)]
pub struct Loaded {
    pub settings: Settings,
    /// Set when the file was damaged or unsupported: where it was moved and
    /// why. The returned settings are then the defaults.
    pub recovered: Option<(PathBuf, SettingsError)>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: FORMAT_VERSION,
            send: SendSettings::default(),
            capture: CaptureSettings::default(),
            views: Views::default(),
            device: DeviceSettings::default(),
        }
    }
}

impl Default for SendSettings {
    fn default() -> Self {
        Self { carrier_hz: 38_000, repeat: RepeatMode::AsLearned }
    }
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self { limit_mib: 4096 }
    }
}

impl Default for Views {
    fn default() -> Self {
        let view = ViewSettings::default();
        Self { capture: view, transfers: view, send: view, library: view }
    }
}

impl Default for ViewSettings {
    fn default() -> Self {
        Self { encoding: Encoding::Hex, uppercase: true, bytes_per_line: 16, offer_pulses: true, show_pulses: false }
    }
}

impl Settings {
    /// Parses TOML text; missing fields take their defaults.
    pub fn from_toml(text: &str) -> Result<Self, SettingsError> {
        let settings: Settings = toml::from_str(text).map_err(|e| SettingsError::Parse(e.to_string()))?;
        if settings.version > FORMAT_VERSION {
            return Err(SettingsError::UnsupportedVersion(settings.version));
        }
        Ok(settings)
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).expect("settings are always serializable")
    }

    /// The send defaults in the form `ir_library::send::prepare` takes.
    pub fn send_defaults(&self) -> Defaults {
        Defaults { carrier_hz: self.send.carrier_hz, repeat: self.send.repeat }
    }

    pub fn capture_limit_bytes(&self) -> u64 {
        self.capture.limit_mib.saturating_mul(1024 * 1024)
    }
}

impl ViewSettings {
    pub fn format_options(&self) -> FormatOptions {
        FormatOptions { uppercase: self.uppercase, bytes_per_line: self.bytes_per_line }
    }
}

/// `$XDG_CONFIG_HOME/<app>/settings.toml`, falling back to
/// `$HOME/.config/<app>/settings.toml`. `env` looks up variables.
pub fn default_path(env: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let base = match env("XDG_CONFIG_HOME").map(PathBuf::from) {
        Some(dir) if dir.is_absolute() => dir,
        _ => PathBuf::from(env("HOME").filter(|h| !h.is_empty())?).join(".config"),
    };
    Some(base.join(store::APP_DIR).join(FILE_NAME))
}

/// Writes the settings atomically, creating the directory if needed.
pub fn save(path: &Path, settings: &Settings) -> io::Result<()> {
    store::write_atomic(path, &settings.to_toml())
}

/// Loads the settings. A missing file gives the defaults; a damaged or
/// unsupported file is renamed to `<file>.damaged-<now_secs>`.
pub fn load(path: &Path, now_secs: u64) -> io::Result<Loaded> {
    let Some(text) = store::read_optional(path)? else {
        return Ok(Loaded { settings: Settings::default(), recovered: None });
    };
    match Settings::from_toml(&text) {
        Ok(settings) => Ok(Loaded { settings, recovered: None }),
        Err(reason) => {
            let backup = store::move_aside(path, now_secs)?;
            Ok(Loaded { settings: Settings::default(), recovered: Some((backup, reason)) })
        }
    }
}

/// Serde helpers storing an encoding as its lowercase name.
mod encoding_name {
    use ir_codec::Encoding;
    use serde::{Deserialize, Deserializer, Serializer};

    const NAMES: [(Encoding, &str); 4] = [
        (Encoding::Hex, "hex"),
        (Encoding::Binary, "binary"),
        (Encoding::Decimal, "decimal"),
        (Encoding::Base64, "base64"),
    ];

    pub fn serialize<S: Serializer>(e: &Encoding, s: S) -> Result<S::Ok, S::Error> {
        let name = NAMES.iter().find(|(enc, _)| enc == e).map(|(_, n)| *n).expect("every encoding has a name");
        s.serialize_str(name)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Encoding, D::Error> {
        let text = String::deserialize(d)?;
        NAMES.iter().find(|(_, n)| *n == text).map(|(enc, _)| *enc).ok_or_else(|| {
            serde::de::Error::custom(format!("unknown encoding \"{text}\", expected hex, binary, decimal or base64"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-tmp/ir-settings").join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| vars.iter().find(|(k, _)| *k == key).map(|(_, v)| v.to_string())
    }

    #[test]
    fn defaults_follow_the_design_decisions() {
        let s = Settings::default();
        assert_eq!(s.version, FORMAT_VERSION);
        // 38000 Hz is also elksmart-comm's default.
        assert_eq!(s.send.carrier_hz, 38_000);
        assert_eq!(s.send.repeat, RepeatMode::AsLearned);
        assert_eq!(s.capture.limit_mib, 4096);
        assert_eq!(s.capture_limit_bytes(), 4096 * 1024 * 1024);
        for view in [s.views.capture, s.views.transfers, s.views.send, s.views.library] {
            assert_eq!(view.encoding, Encoding::Hex);
            assert!(view.uppercase);
            assert_eq!(view.bytes_per_line, 16);
            assert!(view.offer_pulses);
            assert!(!view.show_pulses);
        }
        assert_eq!(s.device.preferred_serial, None);
    }

    #[test]
    fn round_trips_through_toml() {
        let mut s = Settings::default();
        s.send.carrier_hz = 36_000;
        s.send.repeat = RepeatMode::FirstFrame { count: 3 };
        s.capture.limit_mib = 512;
        s.views.capture.encoding = Encoding::Base64;
        s.views.send.encoding = Encoding::Binary;
        s.views.library.encoding = Encoding::Decimal;
        s.views.library.uppercase = false;
        s.views.library.bytes_per_line = 0;
        s.views.send.offer_pulses = false;
        s.views.capture.show_pulses = true;
        s.views.transfers.encoding = Encoding::Decimal;
        s.device.preferred_serial = Some("030820250001".into());
        assert_eq!(Settings::from_toml(&s.to_toml()), Ok(s));
    }

    #[test]
    fn documented_file_format_parses() {
        let text = r#"
version = 1

[send]
carrier_hz = 40000
repeat = { mode = "first_frame", count = 2 }

[capture]
limit_mib = 100

[views.capture]
encoding = "binary"
bytes_per_line = 8

[device]
preferred_serial = "030820250001"
"#;
        let s = Settings::from_toml(text).unwrap();
        assert_eq!(s.send.carrier_hz, 40_000);
        assert_eq!(s.send.repeat, RepeatMode::FirstFrame { count: 2 });
        assert_eq!(s.capture.limit_mib, 100);
        assert_eq!(s.views.capture.encoding, Encoding::Binary);
        assert_eq!(s.views.capture.bytes_per_line, 8);
        assert!(s.views.capture.uppercase, "unset fields keep their defaults");
        assert_eq!(s.views.send, ViewSettings::default());
        assert_eq!(s.device.preferred_serial.as_deref(), Some("030820250001"));
    }

    #[test]
    fn empty_file_gives_defaults() {
        assert_eq!(Settings::from_toml(""), Ok(Settings::default()));
    }

    #[test]
    fn encodings_are_stored_by_name() {
        let mut s = Settings::default();
        s.views.capture.encoding = Encoding::Base64;
        let text = s.to_toml();
        assert!(text.contains("encoding = \"base64\""), "{text}");
        assert!(text.contains("encoding = \"hex\""), "{text}");
        for (name, enc) in
            [("hex", Encoding::Hex), ("binary", Encoding::Binary), ("decimal", Encoding::Decimal), ("base64", Encoding::Base64)]
        {
            let parsed = Settings::from_toml(&format!("[views.send]\nencoding = \"{name}\"\n")).unwrap();
            assert_eq!(parsed.views.send.encoding, enc);
        }
    }

    #[test]
    fn unknown_encoding_and_wrong_types_are_parse_errors() {
        assert!(matches!(Settings::from_toml("[views.send]\nencoding = \"octal\"\n"), Err(SettingsError::Parse(_))));
        assert!(matches!(Settings::from_toml("[send]\ncarrier_hz = \"fast\"\n"), Err(SettingsError::Parse(_))));
        assert!(matches!(Settings::from_toml("not toml ["), Err(SettingsError::Parse(_))));
    }

    #[test]
    fn errors_read_as_sentences() {
        assert_eq!(
            SettingsError::UnsupportedVersion(3).to_string(),
            "the settings file has format version 3, this app reads up to 1"
        );
        assert_eq!(SettingsError::Parse("bad".into()).to_string(), "the settings file is damaged: bad");
    }

    #[test]
    fn newer_version_is_rejected() {
        assert_eq!(Settings::from_toml("version = 2\n"), Err(SettingsError::UnsupportedVersion(2)));
    }

    #[test]
    fn send_defaults_and_format_options() {
        let mut s = Settings::default();
        s.send.carrier_hz = 36_000;
        s.send.repeat = RepeatMode::FirstFrame { count: 1 };
        assert_eq!(s.send_defaults(), Defaults { carrier_hz: 36_000, repeat: RepeatMode::FirstFrame { count: 1 } });
        let view = ViewSettings { uppercase: false, bytes_per_line: 4, ..ViewSettings::default() };
        assert_eq!(view.format_options(), FormatOptions { uppercase: false, bytes_per_line: 4 });
    }

    #[test]
    fn default_path_prefers_xdg_config_home() {
        assert_eq!(
            default_path(env(&[("XDG_CONFIG_HOME", "/cfg"), ("HOME", "/home/u")])),
            Some(PathBuf::from("/cfg/io.github.ueda.irtransceiver/settings.toml"))
        );
    }

    #[test]
    fn default_path_falls_back_to_home() {
        let expected = Some(PathBuf::from("/home/u/.config/io.github.ueda.irtransceiver/settings.toml"));
        assert_eq!(default_path(env(&[("HOME", "/home/u")])), expected);
        assert_eq!(default_path(env(&[("XDG_CONFIG_HOME", "rel"), ("HOME", "/home/u")])), expected);
        assert_eq!(default_path(env(&[])), None);
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = temp_dir("roundtrip").join("sub").join(FILE_NAME);
        let mut s = Settings::default();
        s.capture.limit_mib = 1;
        save(&path, &s).unwrap();
        let loaded = load(&path, 0).unwrap();
        assert_eq!(loaded.settings, s);
        assert!(loaded.recovered.is_none());
    }

    #[test]
    fn missing_file_gives_defaults() {
        let loaded = load(&temp_dir("missing").join(FILE_NAME), 0).unwrap();
        assert_eq!(loaded.settings, Settings::default());
        assert!(loaded.recovered.is_none());
    }

    #[test]
    fn damaged_file_is_moved_aside() {
        let dir = temp_dir("damaged");
        let path = dir.join(FILE_NAME);
        fs::write(&path, "[send]\ncarrier_hz = \"x\"\n").unwrap();
        let loaded = load(&path, 77).unwrap();
        assert_eq!(loaded.settings, Settings::default());
        let (backup, reason) = loaded.recovered.expect("recovered");
        assert_eq!(backup, dir.join("settings.toml.damaged-77"));
        assert!(matches!(reason, SettingsError::Parse(_)));
        assert!(!path.exists());
    }
}
