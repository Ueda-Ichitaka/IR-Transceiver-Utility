//! How capture rows look on the Capture page (as in the mockup).
//!
//! Exists to define the compact list (a kind chip per row with a tone for its
//! colour, a one-line byte preview), the detail panel of the selected row
//! (title, formatted bytes, pulse count, duration, protocol guess, the pulse
//! timings for the waveform) and the status bar (capture duration, frames,
//! IR codes, short reads, buffer use). Transfer kinds follow the dongle
//! protocol: FC identify, FE learn, FD stop, FF send header.

use std::time::Duration;

use ir_codec::{Encoding, FormatOptions};

use crate::capture::{Direction, Shown, SignalRow, TransferRow, signal_data_text, transfer_data_text};

/// Colour class of a kind chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Infrared data.
    Ir,
    /// Commands and replies.
    Control,
    /// Short reads and errors.
    Problem,
}

/// The selected row's detail panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detail {
    pub title: String,
    pub bytes: String,
    /// Label and value pairs.
    pub facts: Vec<(String, String)>,
    /// Pulse timings as on, off, on, off, ... in microseconds (empty when
    /// there is nothing to draw).
    pub waveform: Vec<u32>,
}

/// Counts for the status bar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    pub frames: usize,
    pub codes: usize,
    pub short_reads: usize,
}

pub fn signal_kind(row: &SignalRow) -> (String, Tone) {
    if row.complete {
        (format!("IR {} B", row.data.len()), Tone::Ir)
    } else {
        (format!("short {} of {}", row.data.len(), row.announced_len), Tone::Problem)
    }
}

pub fn transfer_kind(row: &TransferRow) -> (String, Tone) {
    let b = row.bytes.as_slice();
    let repeated = |byte: u8| b.len() >= 4 && b[..4].iter().all(|&x| x == byte);
    let control = |name: &str| (name.to_string(), Tone::Control);
    match row.direction {
        Direction::Error => ("error".to_string(), Tone::Problem),
        Direction::Out if repeated(0xFC) => control("identify"),
        Direction::Out if repeated(0xFE) => control("learn"),
        Direction::Out if repeated(0xFD) => control("stop"),
        Direction::Out if repeated(0xFF) => control("send"),
        Direction::Out => control("send data"),
        Direction::In if repeated(0xFC) => control("identify reply"),
        Direction::In if b == [0xFF; 4] => control("ack"),
        Direction::In if repeated(0xFE) && b.len() >= 6 => {
            (format!("IR {} B", u16::from_be_bytes([b[4], b[5]])), Tone::Ir)
        }
        Direction::In => ("IR data".to_string(), Tone::Ir),
    }
}

/// `text` on one line, cut to at most `max_chars` characters with ` …`.
pub fn preview(text: &str, max_chars: usize) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= max_chars {
        return line;
    }
    let cut: String = line.chars().take(max_chars.saturating_sub(2)).collect();
    format!("{} …", cut.trim_end())
}

/// Detail of signal number `number` (1-based).
pub fn signal_detail(number: usize, row: &SignalRow, shown: Shown, options: &FormatOptions) -> Detail {
    let pulses = row.pulses.as_deref().unwrap_or_default();
    let decoded = row.pulses.is_some();
    let total_us: u64 = pulses.iter().map(|p| u64::from(p.on_us) + u64::from(p.off_us)).sum();
    let facts = vec![
        ("Length".to_string(), format!("{} of {} bytes", row.data.len(), row.announced_len)),
        ("Pulses".to_string(), if decoded { pulses.len().to_string() } else { "-".into() }),
        ("Duration".to_string(), if decoded { format!("{:.1} ms", total_us as f64 / 1000.0) } else { "-".into() }),
        ("Guess".to_string(), row.summary.clone()),
    ];
    Detail {
        title: format!("Signal #{number}"),
        bytes: signal_data_text(row, shown, options),
        facts,
        waveform: pulses.iter().flat_map(|p| [p.on_us, p.off_us]).collect(),
    }
}

/// Detail of transfer number `number` (1-based).
pub fn transfer_detail(number: usize, row: &TransferRow, encoding: Encoding, options: &FormatOptions) -> Detail {
    let direction = match row.direction {
        Direction::Out => "OUT",
        Direction::In => "IN",
        Direction::Error => "error",
    };
    Detail {
        title: format!("Transfer #{number} · {direction}"),
        bytes: transfer_data_text(row, encoding, options),
        facts: vec![
            ("Kind".to_string(), transfer_kind(row).0),
            ("Length".to_string(), format!("{} bytes", row.bytes.len())),
        ],
        waveform: Vec::new(),
    }
}

/// The status bar, e.g. `Capturing 00:03:12`, `48 frames`, `3 IR codes`,
/// `1 short read`, `Buffer 0.2 % of 4096 MiB`.
pub fn status_parts(capturing_for: Option<Duration>, stats: Stats, used_bytes: u64, limit_mib: u64) -> Vec<String> {
    let plural = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let state = match capturing_for {
        Some(d) => {
            let s = d.as_secs();
            format!("Capturing {:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
        }
        None => "Not capturing".to_string(),
    };
    let limit_bytes = limit_mib.saturating_mul(1024 * 1024).max(1);
    let percent = used_bytes as f64 * 100.0 / limit_bytes as f64;
    vec![
        state,
        plural(stats.frames, "frame", "frames"),
        plural(stats.codes, "IR code", "IR codes"),
        plural(stats.short_reads, "short read", "short reads"),
        format!("Buffer {percent:.1} % of {limit_mib} MiB"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;


    const OPTS: FormatOptions = FormatOptions { uppercase: true, bytes_per_line: 0 };

    fn t(direction: Direction, bytes: &[u8]) -> TransferRow {
        TransferRow { at: Duration::ZERO, wall_ms: None, direction, bytes: bytes.to_vec(), error: None }
    }

    fn kind(direction: Direction, bytes: &[u8]) -> (String, Tone) {
        transfer_kind(&t(direction, bytes))
    }

    #[test]
    fn transfer_kinds_follow_the_protocol() {
        use Direction::{In, Out};
        let c = |s: &str| (s.to_string(), Tone::Control);
        assert_eq!(kind(Out, &[0xFC; 4]), c("identify"));
        assert_eq!(kind(Out, &[0xFE; 4]), c("learn"));
        assert_eq!(kind(Out, &[0xFD; 4]), c("stop"));
        assert_eq!(kind(Out, &[0xFF, 0xFF, 0xFF, 0xFF, 0x7F, 0x03]), c("send"));
        assert_eq!(kind(Out, &[0x12, 0x34]), c("send data"));
        assert_eq!(kind(In, &[0xFC, 0xFC, 0xFC, 0xFC, 0x70, 0x01]), c("identify reply"));
        assert_eq!(kind(In, &[0xFF; 4]), c("ack"));
        assert_eq!(kind(In, &[0xFE, 0xFE, 0xFE, 0xFE, 0x00, 0xD8, 0xFF]), ("IR 216 B".to_string(), Tone::Ir));
        assert_eq!(kind(In, &[0x26, 0x65]), ("IR data".to_string(), Tone::Ir));
        let err = TransferRow { error: Some("gone".into()), ..t(Direction::Error, &[]) };
        assert_eq!(transfer_kind(&err), ("error".to_string(), Tone::Problem));
    }

    fn signal(complete: bool, announced_len: usize, data: Vec<u8>) -> SignalRow {
        SignalRow::from_learned(Duration::ZERO, None, complete, announced_len, data)
    }

    #[test]
    fn signal_kinds() {
        assert_eq!(signal_kind(&signal(true, 2, vec![0x23, 0x37])), ("IR 2 B".to_string(), Tone::Ir));
        assert_eq!(signal_kind(&signal(false, 68, vec![0x23; 31])), ("short 31 of 68".to_string(), Tone::Problem));
    }

    #[test]
    fn previews_are_one_line_and_cut() {
        assert_eq!(preview("FE FE\nFE FE", 40), "FE FE FE FE");
        assert_eq!(preview("FE FE FE FE 00 44", 11), "FE FE FE …");
        assert_eq!(preview("short", 5), "short");
    }

    #[test]
    fn signal_detail_lists_facts_and_waveform() {
        let row = signal(true, 4, vec![0x23, 0x37, 0x23, 0x00]);
        let d = signal_detail(3, &row, Shown::Encoded(Encoding::Hex), &OPTS);
        assert_eq!(d.title, "Signal #3");
        assert_eq!(d.bytes, "23 37 23 00");
        assert_eq!(
            d.facts,
            vec![
                ("Length".to_string(), "4 of 4 bytes".to_string()),
                ("Pulses".to_string(), "2".to_string()),
                ("Duration".to_string(), "2.0 ms".to_string()),
                ("Guess".to_string(), row.summary.clone()),
            ]
        );
        assert_eq!(d.waveform, vec![560, 880, 560, 0]);
        let pulses = signal_detail(3, &row, Shown::Pulses, &OPTS);
        assert_eq!(pulses.bytes, "560/880 560/0");
    }

    #[test]
    fn undecodable_signal_detail() {
        let row = signal(false, 9, vec![0x23, 0xFF]);
        let d = signal_detail(1, &row, Shown::Encoded(Encoding::Hex), &OPTS);
        assert_eq!(d.facts[1], ("Pulses".to_string(), "-".to_string()));
        assert_eq!(d.facts[2], ("Duration".to_string(), "-".to_string()));
        assert!(d.waveform.is_empty());
        assert_eq!(d.facts[0], ("Length".to_string(), "2 of 9 bytes".to_string()));
    }

    #[test]
    fn transfer_detail_names_direction_and_kind() {
        let d = transfer_detail(12, &t(Direction::In, &[0xFC, 0xFC, 0xFC, 0xFC, 0x70, 0x01]), Encoding::Hex, &OPTS);
        assert_eq!(d.title, "Transfer #12 · IN");
        assert_eq!(d.bytes, "FC FC FC FC 70 01");
        assert_eq!(
            d.facts,
            vec![("Kind".to_string(), "identify reply".to_string()), ("Length".to_string(), "6 bytes".to_string())]
        );
        assert!(d.waveform.is_empty());
        let err = TransferRow { error: Some("gone".into()), ..t(Direction::Error, &[]) };
        assert_eq!(transfer_detail(1, &err, Encoding::Hex, &OPTS).bytes, "gone");
        assert_eq!(transfer_detail(1, &err, Encoding::Hex, &OPTS).title, "Transfer #1 · error");
    }

    #[test]
    fn status_bar() {
        let stats = Stats { frames: 48, codes: 3, short_reads: 1 };
        assert_eq!(
            status_parts(Some(Duration::from_secs(192)), stats, 8_589_935, 4096),
            ["Capturing 00:03:12", "48 frames", "3 IR codes", "1 short read", "Buffer 0.2 % of 4096 MiB"]
        );
        let one = Stats { frames: 1, codes: 1, short_reads: 2 };
        assert_eq!(
            status_parts(None, one, 0, 64),
            ["Not capturing", "1 frame", "1 IR code", "2 short reads", "Buffer 0.0 % of 64 MiB"]
        );
        assert_eq!(status_parts(Some(Duration::from_secs(3 * 3600 + 5)), one, 0, 64)[0], "Capturing 03:00:05");
    }
}
