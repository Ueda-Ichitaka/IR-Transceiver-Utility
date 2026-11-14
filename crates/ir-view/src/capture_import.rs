//! Loading exported captures back (design decision 18 and the capture import
//! decisions).
//!
//! Exists to read the files written by `capture_export` into capture rows:
//! JSON (both tabs), CSV (the tab named by its header line) and `.bin`, which
//! becomes one learned code. Summaries and pulse timings are recomputed from
//! the data, so only data, times and states have to survive the file.

use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

use crate::capture::{Direction, SignalRow, TransferRow};

/// Rows read from a file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportedCapture {
    pub signals: Vec<SignalRow>,
    pub transfers: Vec<TransferRow>,
}

/// Reads a capture file; the format is taken from the `.bin` extension, a
/// JSON object, or the CSV header line.
pub fn import_capture(bytes: &[u8], path: &Path) -> Result<ImportedCapture, String> {
    let is_bin = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("bin"));
    if is_bin {
        if bytes.is_empty() {
            return Err("the file is empty".into());
        }
        return Ok(ImportedCapture {
            signals: vec![SignalRow::from_learned(Duration::ZERO, true, bytes.to_vec())],
            transfers: Vec::new(),
        });
    }
    let unknown = || "not a capture exported by this app (JSON, CSV or .bin)".to_string();
    let text = std::str::from_utf8(bytes).map_err(|_| unknown())?;
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') {
        return from_json(trimmed);
    }
    let records = csv_records(text);
    match records.first().map(|(_, fields)| fields.join(",")) {
        Some(header) if header == "time,status,length,summary,data" => signals_from_csv(&records[1..]),
        Some(header) if header == "time,direction,length,data" => transfers_from_csv(&records[1..]),
        _ => Err(unknown()),
    }
}

#[derive(Deserialize)]
struct JsonLog {
    signals: Vec<JsonSignal>,
    transfers: Vec<JsonTransfer>,
}

#[derive(Deserialize)]
struct JsonSignal {
    time_s: f64,
    complete: bool,
    data: String,
}

#[derive(Deserialize)]
struct JsonTransfer {
    time_s: f64,
    direction: String,
    data: String,
    error: Option<String>,
}

fn seconds(time_s: f64) -> Option<Duration> {
    (time_s.is_finite() && time_s >= 0.0).then(|| Duration::from_millis((time_s * 1000.0).round() as u64))
}

fn hex(text: &str) -> Result<Vec<u8>, String> {
    ir_codec::parse(text, ir_codec::Encoding::Hex).map_err(|e| e.to_string())
}

fn from_json(text: &str) -> Result<ImportedCapture, String> {
    let log: JsonLog = serde_json::from_str(text).map_err(|e| format!("JSON: {e}"))?;
    let json_err = |what: &str, i: usize, e: String| format!("JSON: {what} {}: {e}", i + 1);
    let signals = log
        .signals
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            let at = seconds(s.time_s).ok_or_else(|| json_err("signal", i, "invalid time".into()))?;
            let data = hex(&s.data).map_err(|e| json_err("signal", i, e))?;
            Ok(SignalRow::from_learned(at, s.complete, data))
        })
        .collect::<Result<_, String>>()?;
    let transfers = log
        .transfers
        .into_iter()
        .enumerate()
        .map(|(i, t)| {
            let at = seconds(t.time_s).ok_or_else(|| json_err("transfer", i, "invalid time".into()))?;
            let direction = match t.direction.as_str() {
                "out" => Direction::Out,
                "in" => Direction::In,
                "error" => Direction::Error,
                other => return Err(json_err("transfer", i, format!("{other:?} is not out, in or error"))),
            };
            let bytes = hex(&t.data).map_err(|e| json_err("transfer", i, e))?;
            Ok(TransferRow { at, direction, bytes, error: t.error })
        })
        .collect::<Result<_, String>>()?;
    Ok(ImportedCapture { signals, transfers })
}

/// RFC 4180 records with the line number each starts on.
fn csv_records(text: &str) -> Vec<(usize, Vec<String>)> {
    let mut records = Vec::new();
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut line = 1;
    let mut start = 1;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\n' {
            line += 1;
        }
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            ('"', true) => quoted = false,
            ('"', false) if field.is_empty() => quoted = true,
            (',', false) => fields.push(std::mem::take(&mut field)),
            ('\r', false) => {}
            ('\n', false) => {
                fields.push(std::mem::take(&mut field));
                records.push((start, std::mem::take(&mut fields)));
                start = line;
            }
            (c, _) => field.push(c),
        }
    }
    if !field.is_empty() || !fields.is_empty() {
        fields.push(field);
        records.push((start, fields));
    }
    records
}

fn csv_time(line: usize, text: &str) -> Result<Duration, String> {
    text.parse::<f64>().ok().and_then(seconds).ok_or_else(|| format!("line {line}: {text:?} is not a time"))
}

fn columns(line: usize, fields: &[String], count: usize) -> Result<&[String], String> {
    if fields.len() == count { Ok(fields) } else { Err(format!("line {line}: expected {count} fields")) }
}

fn signals_from_csv(records: &[(usize, Vec<String>)]) -> Result<ImportedCapture, String> {
    let signals = records
        .iter()
        .map(|(line, fields)| {
            let f = columns(*line, fields, 5)?;
            let complete = match f[1].as_str() {
                "complete" => true,
                "short" => false,
                other => return Err(format!("line {line}: {other:?} is not complete or short")),
            };
            let data = hex(&f[4]).map_err(|e| format!("line {line}: {e}"))?;
            Ok(SignalRow::from_learned(csv_time(*line, &f[0])?, complete, data))
        })
        .collect::<Result<_, String>>()?;
    Ok(ImportedCapture { signals, transfers: Vec::new() })
}

fn transfers_from_csv(records: &[(usize, Vec<String>)]) -> Result<ImportedCapture, String> {
    let transfers = records
        .iter()
        .map(|(line, fields)| {
            let f = columns(*line, fields, 4)?;
            let at = csv_time(*line, &f[0])?;
            let (direction, bytes, error) = match f[1].as_str() {
                "OUT" => (Direction::Out, hex(&f[3]).map_err(|e| format!("line {line}: {e}"))?, None),
                "IN" => (Direction::In, hex(&f[3]).map_err(|e| format!("line {line}: {e}"))?, None),
                "ERR" => (Direction::Error, Vec::new(), Some(f[3].clone())),
                other => return Err(format!("line {line}: {other:?} is not OUT, IN or ERR")),
            };
            Ok(TransferRow { at, direction, bytes, error })
        })
        .collect::<Result<_, String>>()?;
    Ok(ImportedCapture { signals: Vec::new(), transfers })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::CaptureLog;
    use crate::capture_export;
    use elk_proto::learn::LearnedCode;
    use ir_session::session::Event;
    use ir_transport::ops::LearnOutcome;
    use ir_transport::recorder::{self, Entry};
    use ir_transport::transport::TransportError;

    fn transfer(ms: u64, event: recorder::Event) -> Event {
        Event::Transfer(Entry { wall_ms: 0, at: Duration::from_millis(ms), event })
    }

    fn sample() -> CaptureLog {
        let mut log = CaptureLog::new(1 << 20);
        log.apply(&transfer(10, recorder::Event::Out(vec![0xFE; 4])));
        log.apply(&transfer(1234, recorder::Event::In(vec![0xFE, 0x00, 0x02, 0x23, 0x37])));
        log.apply(&Event::Learned(LearnOutcome::Complete(LearnedCode { announced_len: 2, data: vec![0x23, 0x37] })));
        log.apply(&Event::Learned(LearnOutcome::Short(LearnedCode { announced_len: 9, data: vec![0x23, 0xFF] })));
        log.apply(&transfer(2500, recorder::Event::Error(TransportError::Usb("pipe, \"stalled\"\nx".into()))));
        log
    }

    fn rows(log: &CaptureLog) -> ImportedCapture {
        ImportedCapture {
            signals: (0..log.signal_count()).filter_map(|i| log.signal(i).cloned()).collect(),
            transfers: (0..log.transfer_count()).filter_map(|i| log.transfer(i).cloned()).collect(),
        }
    }

    #[test]
    fn json_round_trip() {
        let log = sample();
        let json = capture_export::to_json(&log);
        assert_eq!(import_capture(json.as_bytes(), Path::new("a.json")), Ok(rows(&log)));
    }

    #[test]
    fn csv_round_trips_per_tab() {
        let log = sample();
        let all = rows(&log);
        let signals = import_capture(capture_export::signals_csv(&log).as_bytes(), Path::new("s.csv")).unwrap();
        assert_eq!(signals, ImportedCapture { signals: all.signals.clone(), transfers: vec![] });
        let transfers = import_capture(capture_export::transfers_csv(&log).as_bytes(), Path::new("t.csv")).unwrap();
        assert_eq!(transfers, ImportedCapture { signals: vec![], transfers: all.transfers });
    }

    #[test]
    fn bin_is_one_learned_code() {
        let imported = import_capture(&[0x23, 0x37], Path::new("dump.BIN")).unwrap();
        assert_eq!(imported.signals, vec![SignalRow::from_learned(Duration::ZERO, true, vec![0x23, 0x37])]);
        assert!(imported.transfers.is_empty());
        assert_eq!(import_capture(&[], Path::new("e.bin")), Err("the file is empty".into()));
    }

    #[test]
    fn unknown_and_broken_files() {
        assert_eq!(
            import_capture(b"hello", Path::new("x.txt")),
            Err("not a capture exported by this app (JSON, CSV or .bin)".into())
        );
        assert!(import_capture(b"{\"signals\": 3}", Path::new("x.json")).unwrap_err().starts_with("JSON:"));
        let bad_csv = "time,direction,length,data\r\nsoon,IN,1,AB\r\n";
        assert_eq!(import_capture(bad_csv.as_bytes(), Path::new("x.csv")), Err("line 2: \"soon\" is not a time".into()));
        let bad_dir = "time,direction,length,data\r\n1.000,UP,1,AB\r\n";
        assert_eq!(import_capture(bad_dir.as_bytes(), Path::new("x.csv")), Err("line 2: \"UP\" is not OUT, IN or ERR".into()));
        let bad_hex = "time,status,length,summary,data\r\n1.000,complete,1,x,ZZ\r\n";
        assert!(import_capture(bad_hex.as_bytes(), Path::new("x.csv")).unwrap_err().starts_with("line 2:"));
    }

    #[test]
    fn json_times_are_rounded_to_milliseconds() {
        let json = r#"{"signals":[],"transfers":[{"time_s":1.2339999,"direction":"in","data":"AB","error":null}]}"#;
        let imported = import_capture(json.as_bytes(), Path::new("x.json")).unwrap();
        assert_eq!(imported.transfers[0].at, Duration::from_millis(1234));
    }
}
