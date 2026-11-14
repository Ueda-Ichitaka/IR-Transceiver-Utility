//! Choosing what an export writes and how an imported file is read.
//!
//! Exists to map the export menu entries to file contents (capture: JSON,
//! CSV of one tab, the received byte stream, LIRC, Flipper; library: LIRC,
//! Flipper) and to recognise imported files by their content, so the
//! cxx-qt objects only read and write files.

use std::path::Path;

use ir_library::model::Code;

use crate::capture::CaptureLog;
use crate::capture_export;
use crate::exchange::{self, Exported, NewCode};

/// Export menu entries of the capture page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureFormat {
    Json,
    SignalsCsv,
    TransfersCsv,
    Bin,
    Lirc,
    Flipper,
}

/// Export menu entries of the library page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryFormat {
    Lirc,
    Flipper,
}

/// File contents plus the names of entries that could not be exported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub bytes: Vec<u8>,
    pub skipped: Vec<String>,
}

impl CaptureFormat {
    /// Parses the name used by QML (`json`, `signals-csv`, `transfers-csv`,
    /// `bin`, `lirc`, `flipper`).
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "json" => CaptureFormat::Json,
            "signals-csv" => CaptureFormat::SignalsCsv,
            "transfers-csv" => CaptureFormat::TransfersCsv,
            "bin" => CaptureFormat::Bin,
            "lirc" => CaptureFormat::Lirc,
            "flipper" => CaptureFormat::Flipper,
            _ => return None,
        })
    }
}

impl LibraryFormat {
    /// Parses the name used by QML (`lirc`, `flipper`).
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "lirc" => Some(LibraryFormat::Lirc),
            "flipper" => Some(LibraryFormat::Flipper),
            _ => None,
        }
    }

    fn write(self, items: &[exchange::Item]) -> Result<String, String> {
        match self {
            LibraryFormat::Lirc => exchange::to_lirc(items),
            LibraryFormat::Flipper => exchange::to_flipper(items),
        }
    }
}

fn text(text: String) -> Output {
    Output { bytes: text.into_bytes(), skipped: Vec::new() }
}

pub fn export_capture(log: &CaptureLog, format: CaptureFormat, default_carrier_hz: u32) -> Result<Output, String> {
    let code_format = match format {
        CaptureFormat::Json => return Ok(text(capture_export::to_json(log))),
        CaptureFormat::SignalsCsv => return Ok(text(capture_export::signals_csv(log))),
        CaptureFormat::TransfersCsv => return Ok(text(capture_export::transfers_csv(log))),
        CaptureFormat::Bin => return Ok(Output { bytes: capture_export::in_stream(log), skipped: Vec::new() }),
        CaptureFormat::Lirc => LibraryFormat::Lirc,
        CaptureFormat::Flipper => LibraryFormat::Flipper,
    };
    let mut items = Vec::new();
    let mut skipped = Vec::new();
    for (i, row) in (0..log.signal_count()).filter_map(|i| Some((i, log.signal(i)?))) {
        match exchange::signal_item(row, default_carrier_hz) {
            Some(item) => items.push(item),
            None => skipped.push(format!("signal {}", i + 1)),
        }
    }
    Ok(Output { bytes: code_format.write(&items)?.into_bytes(), skipped })
}

pub fn export_library<'a>(
    codes: impl IntoIterator<Item = &'a Code>,
    format: LibraryFormat,
    default_carrier_hz: u32,
) -> Result<Output, String> {
    let Exported { text, skipped } = exchange::export_codes(codes, default_carrier_hz, |items| format.write(items))?;
    Ok(Output { bytes: text.into_bytes(), skipped })
}

/// Reads an imported file: Flipper (`Filetype:` header), LIRC (`begin
/// remote`) or else one Pronto code named after the file.
pub fn import(text: &str, path: &Path) -> Result<Vec<NewCode>, String> {
    let content = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#'));
    let first = content.clone().next().unwrap_or_default();
    if first.starts_with("Filetype:") {
        exchange::from_flipper(text)
    } else if content.clone().any(|l| l.starts_with("begin remote")) {
        exchange::from_lirc(text)
    } else {
        let name = path.file_stem().map_or_else(|| "Imported code".into(), |s| s.to_string_lossy().into_owned());
        exchange::from_pronto(text, &name).map_err(|e| format!("not a Flipper, LIRC or Pronto file ({e})"))
    }
}

/// Confirmation after an export, naming skipped entries.
pub fn export_message(path: &Path, skipped: &[String]) -> String {
    let mut message = format!("Exported to {}.", path.display());
    if !skipped.is_empty() {
        message.push_str(&format!(" Skipped because they have no pulse timings: {}.", skipped.join(", ")));
    }
    message
}

/// Confirmation after an import.
pub fn import_message(count: usize, path: &Path) -> String {
    let codes = if count == 1 { "code" } else { "codes" };
    format!("Imported {count} {codes} from {}.", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exchange::Item;
    use elk_proto::learn::LearnedCode;
    use elk_proto::pulse::Pulse;
    use ir_library::model::Payload;
    use ir_session::session::Event;
    use ir_transport::ops::LearnOutcome;
    use ir_transport::recorder::{self, Entry};
    use std::time::Duration;

    fn log() -> CaptureLog {
        let mut log = CaptureLog::new(1 << 20);
        log.apply(&Event::Transfer(Entry { at: Duration::ZERO, event: recorder::Event::In(vec![0xAB, 0xCD]) }));
        log.apply(&Event::Learned(LearnOutcome::Complete(LearnedCode { announced_len: 2, data: vec![0x23, 0x37] })));
        log.apply(&Event::Learned(LearnOutcome::Short(LearnedCode { announced_len: 9, data: vec![0x23, 0xFF] })));
        log
    }

    fn item() -> Item {
        Item { name: "A".into(), pulses: vec![Pulse { on_us: 560, off_us: 560 }], carrier_hz: 38_000, nec: None }
    }

    #[test]
    fn messages() {
        assert_eq!(export_message(Path::new("/x/a.ir"), &[]), "Exported to /x/a.ir.");
        assert_eq!(
            export_message(Path::new("/x/a.ir"), &["Raw".into(), "signal 2".into()]),
            "Exported to /x/a.ir. Skipped because they have no pulse timings: Raw, signal 2."
        );
        assert_eq!(import_message(1, Path::new("/x/tv.conf")), "Imported 1 code from /x/tv.conf.");
        assert_eq!(import_message(3, Path::new("/x/tv.conf")), "Imported 3 codes from /x/tv.conf.");
    }

    #[test]
    fn format_names() {
        for (name, format) in [
            ("json", CaptureFormat::Json),
            ("signals-csv", CaptureFormat::SignalsCsv),
            ("transfers-csv", CaptureFormat::TransfersCsv),
            ("bin", CaptureFormat::Bin),
            ("lirc", CaptureFormat::Lirc),
            ("flipper", CaptureFormat::Flipper),
        ] {
            assert_eq!(CaptureFormat::from_name(name), Some(format));
        }
        assert_eq!(CaptureFormat::from_name("xml"), None);
        assert_eq!(LibraryFormat::from_name("lirc"), Some(LibraryFormat::Lirc));
        assert_eq!(LibraryFormat::from_name("flipper"), Some(LibraryFormat::Flipper));
        assert_eq!(LibraryFormat::from_name("json"), None);
    }

    #[test]
    fn capture_exports() {
        let log = log();
        let bin = export_capture(&log, CaptureFormat::Bin, 38_000).unwrap();
        assert_eq!(bin, Output { bytes: vec![0xAB, 0xCD], skipped: vec![] });
        let csv = export_capture(&log, CaptureFormat::TransfersCsv, 38_000).unwrap();
        assert!(String::from_utf8(csv.bytes).unwrap().starts_with("time,direction"));
        let csv = export_capture(&log, CaptureFormat::SignalsCsv, 38_000).unwrap();
        assert!(String::from_utf8(csv.bytes).unwrap().starts_with("time,status"));
        let json = export_capture(&log, CaptureFormat::Json, 38_000).unwrap();
        assert!(String::from_utf8(json.bytes).unwrap().contains("\"signals\""));
    }

    #[test]
    fn capture_to_code_formats_skips_undecodable_signals() {
        let out = export_capture(&log(), CaptureFormat::Flipper, 36_000).unwrap();
        assert_eq!(out.skipped, vec!["signal 2".to_string()]);
        let text = String::from_utf8(out.bytes).unwrap();
        assert!(text.contains("frequency: 36000"), "{text}");
        let out = export_capture(&log(), CaptureFormat::Lirc, 36_000).unwrap();
        assert!(String::from_utf8(out.bytes).unwrap().contains("frequency 36000"));
    }

    #[test]
    fn library_exports() {
        let codes = [ir_library::model::Code {
            id: 1,
            name: "P".into(),
            tags: vec![],
            carrier_hz: None,
            repeat: None,
            payload: Payload::Pulses { pulses: vec![[560, 0]] },
            notes: String::new(),
        }];
        let out = export_library(&codes, LibraryFormat::Flipper, 38_000).unwrap();
        assert!(String::from_utf8(out.bytes).unwrap().starts_with("Filetype: IR signals file"));
        let out = export_library(&codes, LibraryFormat::Lirc, 38_000).unwrap();
        assert!(String::from_utf8(out.bytes).unwrap().starts_with("begin remote"));
    }

    #[test]
    fn imports_are_recognised_by_content() {
        let flipper = exchange::to_flipper(&[item()]).unwrap();
        assert_eq!(import(&flipper, Path::new("x.txt")).unwrap()[0].name, "A");
        let lirc = format!("# comment\n{}", exchange::to_lirc(&[item()]).unwrap());
        assert_eq!(import(&lirc, Path::new("remote.conf")).unwrap()[0].tags, vec!["ir-transceiver".to_string()]);
        let pronto = exchange::to_pronto(&item()).unwrap();
        assert_eq!(import(&pronto, Path::new("/tmp/TV power.txt")).unwrap()[0].name, "TV power");
        assert!(import("nothing useful", Path::new("a")).is_err());
    }
}
