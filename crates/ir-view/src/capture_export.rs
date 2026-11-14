//! Exporting the capture log (design decision 17 and the export format
//! decisions): JSON with both tabs, CSV of one tab, and the raw stream of
//! bytes received from the dongle.
//!
//! Exists so the export layouts are defined and tested in one place. Bytes
//! are written as uppercase spaced hex, times as seconds since connecting.
//! CSV follows RFC 4180 (CRLF line ends, quoted fields where needed).

use serde::Serialize;

use ir_codec::{Encoding, FormatOptions};

use crate::capture::{CaptureLog, Direction, TransferRow, format_time};

const HEX: FormatOptions = FormatOptions { uppercase: true, bytes_per_line: 0 };

fn hex(bytes: &[u8]) -> String {
    ir_codec::format(bytes, Encoding::Hex, &HEX)
}

#[derive(Serialize)]
struct JsonLog {
    signals: Vec<JsonSignal>,
    transfers: Vec<JsonTransfer>,
}

#[derive(Serialize)]
struct JsonSignal {
    time_s: f64,
    complete: bool,
    summary: String,
    data: String,
    pulses: Option<Vec<[u32; 2]>>,
}

#[derive(Serialize)]
struct JsonTransfer {
    time_s: f64,
    direction: &'static str,
    data: String,
    error: Option<String>,
}

fn transfers(log: &CaptureLog) -> impl Iterator<Item = &TransferRow> {
    (0..log.transfer_count()).filter_map(|i| log.transfer(i))
}

/// One CSV field, quoted when it contains a comma, quote or line break.
fn field(text: &str) -> String {
    if text.contains([',', '"', '\r', '\n']) { format!("\"{}\"", text.replace('"', "\"\"")) } else { text.to_string() }
}

fn csv_line(fields: &[&str]) -> String {
    let fields: Vec<String> = fields.iter().map(|f| field(f)).collect();
    format!("{}\r\n", fields.join(","))
}

/// Both tabs as pretty-printed JSON.
pub fn to_json(log: &CaptureLog) -> String {
    let signals = (0..log.signal_count())
        .filter_map(|i| log.signal(i))
        .map(|r| JsonSignal {
            time_s: r.at.as_secs_f64(),
            complete: r.complete,
            summary: r.summary.clone(),
            data: hex(&r.data),
            pulses: r.pulses.as_ref().map(|p| p.iter().map(|p| [p.on_us, p.off_us]).collect()),
        })
        .collect();
    let transfers = transfers(log)
        .map(|r| JsonTransfer {
            time_s: r.at.as_secs_f64(),
            direction: match r.direction {
                Direction::Out => "out",
                Direction::In => "in",
                Direction::Error => "error",
            },
            data: hex(&r.bytes),
            error: r.error.clone(),
        })
        .collect();
    serde_json::to_string_pretty(&JsonLog { signals, transfers }).expect("capture log is always serializable")
}

/// The Signals tab as CSV: `time,status,length,summary,data`.
pub fn signals_csv(log: &CaptureLog) -> String {
    let mut out = csv_line(&["time", "status", "length", "summary", "data"]);
    for r in (0..log.signal_count()).filter_map(|i| log.signal(i)) {
        let status = if r.complete { "complete" } else { "short" };
        out.push_str(&csv_line(&[&format_time(r.at), status, &r.data.len().to_string(), &r.summary, &hex(&r.data)]));
    }
    out
}

/// The Transfers tab as CSV: `time,direction,length,data` (the error text
/// in `data` for failed transfers).
pub fn transfers_csv(log: &CaptureLog) -> String {
    let mut out = csv_line(&["time", "direction", "length", "data"]);
    for r in transfers(log) {
        let direction = match r.direction {
            Direction::Out => "OUT",
            Direction::In => "IN",
            Direction::Error => "ERR",
        };
        let data = r.error.clone().unwrap_or_else(|| hex(&r.bytes));
        out.push_str(&csv_line(&[&format_time(r.at), direction, &r.bytes.len().to_string(), &data]));
    }
    out
}

/// All bytes received from the dongle (IN transfers), in order.
pub fn in_stream(log: &CaptureLog) -> Vec<u8> {
    transfers(log).filter(|r| r.direction == Direction::In).flat_map(|r| r.bytes.iter().copied()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use elk_proto::learn::LearnedCode;
    use ir_session::session::Event;
    use ir_transport::ops::LearnOutcome;
    use ir_transport::recorder::{self, Entry};
    use ir_transport::transport::TransportError;
    use std::time::Duration;

    fn transfer(ms: u64, event: recorder::Event) -> Event {
        Event::Transfer(Entry { wall_ms: 0, at: Duration::from_millis(ms), event })
    }

    fn sample() -> CaptureLog {
        let mut log = CaptureLog::new(1 << 20);
        log.apply(&transfer(10, recorder::Event::Out(vec![0xFE; 4])));
        log.apply(&transfer(1234, recorder::Event::In(vec![0xFE, 0xFE, 0xFE, 0xFE, 0x00, 0x02, 0x23, 0x37])));
        log.apply(&Event::Learned(LearnOutcome::Complete(LearnedCode { announced_len: 2, data: vec![0x23, 0x37] })));
        log.apply(&transfer(2000, recorder::Event::In(vec![0xAB])));
        log.apply(&transfer(2500, recorder::Event::Error(TransportError::Usb("pipe, \"stalled\"".into()))));
        log
    }

    #[test]
    fn json_holds_both_tabs() {
        let value: serde_json::Value = serde_json::from_str(&to_json(&sample())).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "signals": [{
                    "time_s": 1.234,
                    "complete": true,
                    "summary": "unknown",
                    "data": "23 37",
                    "pulses": [[560, 880]]
                }],
                "transfers": [
                    { "time_s": 0.01, "direction": "out", "data": "FE FE FE FE", "error": null },
                    { "time_s": 1.234, "direction": "in", "data": "FE FE FE FE 00 02 23 37", "error": null },
                    { "time_s": 2.0, "direction": "in", "data": "AB", "error": null },
                    { "time_s": 2.5, "direction": "error", "data": "", "error": TransportError::Usb("pipe, \"stalled\"".into()).to_string() }
                ]
            })
        );
    }

    #[test]
    fn empty_log_json() {
        let value: serde_json::Value = serde_json::from_str(&to_json(&CaptureLog::new(1))).unwrap();
        assert_eq!(value, serde_json::json!({ "signals": [], "transfers": [] }));
    }

    #[test]
    fn signals_csv_layout() {
        assert_eq!(signals_csv(&sample()), "time,status,length,summary,data\r\n1.234,complete,2,unknown,23 37\r\n");
    }

    #[test]
    fn transfers_csv_quotes_where_needed() {
        let error = TransportError::Usb("pipe, \"stalled\"".into()).to_string().replace('"', "\"\"");
        assert_eq!(
            transfers_csv(&sample()),
            format!(
                "time,direction,length,data\r\n0.010,OUT,4,FE FE FE FE\r\n1.234,IN,8,FE FE FE FE 00 02 23 37\r\n2.000,IN,1,AB\r\n2.500,ERR,0,\"{error}\"\r\n"
            )
        );
    }

    #[test]
    fn in_stream_joins_received_bytes() {
        assert_eq!(in_stream(&sample()), vec![0xFE, 0xFE, 0xFE, 0xFE, 0x00, 0x02, 0x23, 0x37, 0xAB]);
    }
}
