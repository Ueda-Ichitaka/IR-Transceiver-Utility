//! The capture log behind the two capture tabs.
//!
//! Exists to keep everything received during a session for the Signals tab
//! (one row per learned code, decoded and described) and the Transfers tab
//! (one row per USB transfer), within the configured capture limit (design
//! decision 15: capture stops when the limit is reached). Both tabs share one
//! budget; rows are only appended, so row indices stay valid until `clear`.

use std::time::Duration;

use elk_proto::protocols::describe::describe_pulses;
use elk_proto::pulse::{Pulse, decode_learned};
use ir_codec::{Encoding, FormatOptions};
use ir_session::capture::{CaptureBuffer, OverflowPolicy, Pushed, Weighted};
use ir_session::session::Event;
use ir_transport::ops::LearnOutcome;
use ir_transport::recorder;

/// Shown instead of a description or timings when the data does not decode.
pub const UNDECODABLE: &str = "cannot be decoded";

/// Direction of a USB transfer row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Out,
    In,
    Error,
}

/// One USB transfer (or transfer error).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferRow {
    /// Offset from connecting.
    pub at: Duration,
    /// Wall-clock time in Unix milliseconds, when known.
    pub wall_ms: Option<u64>,
    pub direction: Direction,
    pub bytes: Vec<u8>,
    /// Description for `Direction::Error`.
    pub error: Option<String>,
}

/// One learned code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalRow {
    /// Time of the last transfer that belongs to the code.
    pub at: Duration,
    /// Wall-clock time in Unix milliseconds, when known.
    pub wall_ms: Option<u64>,
    /// False when the reply stopped before the announced length.
    pub complete: bool,
    /// Length announced by the dongle (equals the data length when complete).
    pub announced_len: usize,
    pub data: Vec<u8>,
    /// `None` when the data does not decode into pulses.
    pub pulses: Option<Vec<Pulse>>,
    /// Protocol description, e.g. `NEC addr 0x00 cmd 0x59, NEC repeat x5`.
    pub summary: String,
}

/// How a byte column is shown: in an encoding, or as decoded pulse timings
/// (design decision 20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    Encoded(Encoding),
    Pulses,
}

/// What `CaptureLog::apply` changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Applied {
    pub signal_added: bool,
    pub transfer_added: bool,
    /// The limit was reached by this event; capture should stop.
    pub became_full: bool,
}

#[derive(Debug)]
enum Item {
    Signal(SignalRow),
    Transfer(TransferRow),
}

/// Fixed cost per row on top of its byte data (row struct, index entry).
const ROW_OVERHEAD: usize = 96;

impl Weighted for Item {
    fn weight(&self) -> usize {
        ROW_OVERHEAD
            + match self {
                Item::Signal(r) => {
                    r.data.len() + r.pulses.as_ref().map_or(0, |p| p.len() * size_of::<Pulse>()) + r.summary.len()
                }
                Item::Transfer(r) => r.bytes.len() + r.error.as_ref().map_or(0, String::len),
            }
    }
}

/// Signals and transfers of one app run, kept in memory until exported.
#[derive(Debug)]
pub struct CaptureLog {
    items: CaptureBuffer<Item>,
    signals: Vec<u64>,
    transfers: Vec<u64>,
    last_at: Duration,
    last_wall_ms: Option<u64>,
    full: bool,
}

impl CaptureLog {
    pub fn new(limit_bytes: u64) -> Self {
        let budget = usize::try_from(limit_bytes).unwrap_or(usize::MAX);
        Self {
            items: CaptureBuffer::new(budget, OverflowPolicy::Reject),
            signals: Vec::new(),
            transfers: Vec::new(),
            last_at: Duration::ZERO,
            last_wall_ms: None,
            full: false,
        }
    }

    pub fn apply(&mut self, event: &Event) -> Applied {
        let item = match event {
            Event::Transfer(entry) => {
                self.last_at = entry.at;
                self.last_wall_ms = Some(entry.wall_ms);
                Item::Transfer(transfer_row(entry))
            }
            Event::Learned(outcome) => match signal_row(outcome, self.last_at, self.last_wall_ms) {
                Some(row) => Item::Signal(row),
                None => return Applied::default(),
            },
            _ => return Applied::default(),
        };
        if self.full {
            return Applied::default();
        }
        let is_signal = matches!(item, Item::Signal(_));
        match self.items.push(item) {
            Pushed::Stored { seq, .. } => {
                if is_signal {
                    self.signals.push(seq);
                } else {
                    self.transfers.push(seq);
                }
                Applied { signal_added: is_signal, transfer_added: !is_signal, became_full: false }
            }
            Pushed::Rejected => {
                self.full = true;
                Applied { became_full: true, ..Applied::default() }
            }
        }
    }

    pub fn signal_count(&self) -> usize {
        self.signals.len()
    }

    pub fn transfer_count(&self) -> usize {
        self.transfers.len()
    }

    pub fn signal(&self, index: usize) -> Option<&SignalRow> {
        match self.items.get(*self.signals.get(index)?)? {
            Item::Signal(row) => Some(row),
            Item::Transfer(_) => None,
        }
    }

    pub fn transfer(&self, index: usize) -> Option<&TransferRow> {
        match self.items.get(*self.transfers.get(index)?)? {
            Item::Transfer(row) => Some(row),
            Item::Signal(_) => None,
        }
    }

    /// Changes the limit. A larger limit lets a full log store again; rows
    /// already stored are kept under a smaller one.
    pub fn set_limit(&mut self, limit_bytes: u64) {
        let budget = usize::try_from(limit_bytes).unwrap_or(usize::MAX);
        self.items.set_budget(budget);
        if self.items.used() < budget {
            self.full = false;
        }
    }

    /// Appends imported rows within the limit; returns how many were stored.
    /// Stops at the first row that does not fit and marks the log full.
    pub fn import(&mut self, signals: Vec<SignalRow>, transfers: Vec<TransferRow>) -> usize {
        let items = signals.into_iter().map(Item::Signal).chain(transfers.into_iter().map(Item::Transfer));
        let mut stored = 0;
        for item in items {
            if self.full {
                break;
            }
            let is_signal = matches!(item, Item::Signal(_));
            match self.items.push(item) {
                Pushed::Stored { seq, .. } => {
                    if is_signal { self.signals.push(seq) } else { self.transfers.push(seq) }
                    stored += 1;
                }
                Pushed::Rejected => self.full = true,
            }
        }
        stored
    }

    /// True once the limit was reached; cleared by `clear`.
    pub fn is_full(&self) -> bool {
        self.full
    }

    pub fn used_bytes(&self) -> u64 {
        self.items.used() as u64
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.signals.clear();
        self.transfers.clear();
        self.full = false;
    }
}

fn transfer_row(entry: &recorder::Entry) -> TransferRow {
    let (direction, bytes, error) = match &entry.event {
        recorder::Event::Out(b) => (Direction::Out, b.clone(), None),
        recorder::Event::In(b) => (Direction::In, b.clone(), None),
        recorder::Event::Error(e) => (Direction::Error, Vec::new(), Some(e.to_string())),
    };
    TransferRow { at: entry.at, wall_ms: Some(entry.wall_ms), direction, bytes, error }
}

impl SignalRow {
    /// A row for learned `data`, decoded and described.
    pub fn from_learned(at: Duration, wall_ms: Option<u64>, complete: bool, announced_len: usize, data: Vec<u8>) -> Self {
        let pulses = decode_learned(&data).ok();
        let summary = pulses.as_deref().map_or_else(|| UNDECODABLE.to_string(), describe_pulses);
        SignalRow { at, wall_ms, complete, announced_len, data, pulses, summary }
    }
}

fn signal_row(outcome: &LearnOutcome, at: Duration, wall_ms: Option<u64>) -> Option<SignalRow> {
    let (code, complete) = match outcome {
        LearnOutcome::Complete(code) => (code, true),
        LearnOutcome::Short(code) => (code, false),
        LearnOutcome::NothingReceived => return None,
    };
    Some(SignalRow::from_learned(at, wall_ms, complete, code.announced_len, code.data.clone()))
}

/// Seconds with milliseconds, e.g. `12.034`.
pub fn format_time(at: Duration) -> String {
    let millis = at.as_millis();
    format!("{}.{:03}", millis / 1000, millis % 1000)
}

/// Memory used by the capture against its limit, e.g. `1.5 KiB of 4096 MiB`.
pub fn usage_text(used_bytes: u64, limit_mib: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * 1024;
    let used = match used_bytes {
        n if n < KIB => format!("{n} B"),
        n if n < MIB => format!("{:.1} KiB", n as f64 / KIB as f64),
        n => format!("{:.1} MiB", n as f64 / MIB as f64),
    };
    format!("{used} of {limit_mib} MiB")
}

/// Pulse timings as `on/off` pairs in microseconds, separated by spaces.
pub fn pulses_text(pulses: &[Pulse]) -> String {
    let pairs: Vec<String> = pulses.iter().map(|p| format!("{}/{}", p.on_us, p.off_us)).collect();
    pairs.join(" ")
}

/// The data column of a signal row.
pub fn signal_data_text(row: &SignalRow, shown: Shown, options: &FormatOptions) -> String {
    match shown {
        Shown::Encoded(encoding) => ir_codec::format(&row.data, encoding, options),
        Shown::Pulses => row.pulses.as_deref().map_or_else(|| UNDECODABLE.to_string(), pulses_text),
    }
}

/// The data column of a transfer row (the error text for errors).
pub fn transfer_data_text(row: &TransferRow, encoding: Encoding, options: &FormatOptions) -> String {
    match &row.error {
        Some(error) => error.clone(),
        None => ir_codec::format(&row.bytes, encoding, options),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elk_proto::learn::LearnedCode;
    use ir_transport::recorder::Entry;
    use ir_transport::transport::TransportError;

    const LEARN_LOG: &str = include_str!("../../../fixtures/1794596116-learn-tv-power.log");

    fn fixture_data() -> Vec<u8> {
        let line = LEARN_LOG.lines().find_map(|l| l.strip_prefix("# data:")).expect("data line");
        line.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    fn transfer(ms: u64, event: recorder::Event) -> Event {
        Event::Transfer(Entry { wall_ms: 0, at: Duration::from_millis(ms), event })
    }

    fn learned(data: Vec<u8>) -> Event {
        Event::Learned(LearnOutcome::Complete(LearnedCode { announced_len: data.len(), data }))
    }

    const OPTS: FormatOptions = FormatOptions { uppercase: true, bytes_per_line: 0 };

    #[test]
    fn transfers_become_rows_in_order() {
        let mut log = CaptureLog::new(1 << 20);
        let a = log.apply(&transfer(10, recorder::Event::Out(vec![0xFC; 4])));
        assert_eq!(a, Applied { transfer_added: true, ..Applied::default() });
        log.apply(&transfer(20, recorder::Event::In(vec![0xFC, 0xFC, 0xFC, 0xFC, 0x70, 0x01])));
        log.apply(&transfer(30, recorder::Event::Error(TransportError::Disconnected)));
        assert_eq!(log.transfer_count(), 3);
        assert_eq!(log.signal_count(), 0);
        let first = log.transfer(0).unwrap();
        assert_eq!((first.at, first.direction, first.bytes.clone()), (Duration::from_millis(10), Direction::Out, vec![0xFC; 4]));
        assert_eq!(log.transfer(1).unwrap().direction, Direction::In);
        let err = log.transfer(2).unwrap();
        assert_eq!(err.direction, Direction::Error);
        assert_eq!(err.error.as_deref(), Some(TransportError::Disconnected.to_string().as_str()));
        assert!(log.transfer(3).is_none());
    }

    #[test]
    fn learned_code_becomes_a_described_signal() {
        let mut log = CaptureLog::new(1 << 20);
        log.apply(&transfer(1234, recorder::Event::In(vec![0xFE; 4])));
        let a = log.apply(&learned(fixture_data()));
        assert_eq!(a, Applied { signal_added: true, ..Applied::default() });
        let row = log.signal(0).unwrap();
        assert_eq!(row.at, Duration::from_millis(1234));
        assert!(row.complete);
        assert_eq!(row.data, fixture_data());
        assert!(row.pulses.is_some());
        assert_eq!(row.summary, "NEC addr 0x00 cmd 0x59, NEC repeat x5");
    }

    #[test]
    fn signals_take_the_wall_time_of_their_last_transfer() {
        let mut log = CaptureLog::new(1 << 20);
        let entry = Entry { wall_ms: 1_700_000_000_500, at: Duration::from_millis(500), event: recorder::Event::In(vec![1]) };
        log.apply(&Event::Transfer(entry));
        log.apply(&learned(vec![0x23, 0x37]));
        assert_eq!(log.transfer(0).unwrap().wall_ms, Some(1_700_000_000_500));
        assert_eq!(log.signal(0).unwrap().wall_ms, Some(1_700_000_000_500));
        assert_eq!(log.signal(0).unwrap().announced_len, 2);
    }

    #[test]
    fn short_and_undecodable_codes() {
        let mut log = CaptureLog::new(1 << 20);
        log.apply(&Event::Learned(LearnOutcome::Short(LearnedCode { announced_len: 10, data: vec![0x23, 0xFF] })));
        let row = log.signal(0).unwrap();
        assert!(!row.complete);
        assert_eq!(row.pulses, None);
        assert_eq!(row.summary, "cannot be decoded");
    }

    #[test]
    fn nothing_received_and_other_events_add_no_rows() {
        let mut log = CaptureLog::new(1 << 20);
        for event in [
            Event::Learned(LearnOutcome::NothingReceived),
            Event::Transmitted { pulses: 2 },
            Event::Error("x".into()),
        ] {
            assert_eq!(log.apply(&event), Applied::default());
        }
        assert_eq!((log.signal_count(), log.transfer_count()), (0, 0));
    }

    #[test]
    fn limit_stops_storing_and_reports_full_once() {
        let mut log = CaptureLog::new(2 * (ROW_OVERHEAD as u64 + 64));
        let chunk = || transfer(0, recorder::Event::In(vec![0; 64]));
        assert!(log.apply(&chunk()).transfer_added);
        assert!(log.apply(&chunk()).transfer_added);
        assert!(!log.is_full());
        assert_eq!(log.apply(&chunk()), Applied { became_full: true, ..Applied::default() });
        assert!(log.is_full());
        assert_eq!(log.apply(&chunk()), Applied::default(), "full is reported once");
        assert_eq!(log.transfer_count(), 2);
        assert_eq!(log.used_bytes(), 2 * (ROW_OVERHEAD as u64 + 64));
    }

    #[test]
    fn raising_the_limit_resumes_storing() {
        let mut log = CaptureLog::new(ROW_OVERHEAD as u64 + 4);
        log.apply(&transfer(0, recorder::Event::Out(vec![0xFC; 4])));
        log.apply(&transfer(0, recorder::Event::Out(vec![0xFC; 4])));
        assert!(log.is_full());
        log.set_limit(1 << 20);
        assert!(!log.is_full());
        assert!(log.apply(&transfer(0, recorder::Event::Out(vec![0xFC; 4]))).transfer_added);
        log.set_limit(1);
        assert_eq!(log.transfer_count(), 2, "rows are kept");
        assert!(log.apply(&transfer(0, recorder::Event::Out(vec![0xFC; 4]))).became_full);
    }

    #[test]
    fn signal_rows_from_learned_data() {
        let row = SignalRow::from_learned(Duration::from_millis(5), Some(7), false, 300, fixture_data());
        assert_eq!((row.at, row.wall_ms, row.complete, row.announced_len), (Duration::from_millis(5), Some(7), false, 300));
        assert_eq!(row.summary, "NEC addr 0x00 cmd 0x59, NEC repeat x5");
        assert!(row.pulses.is_some());
    }

    #[test]
    fn import_appends_rows_within_the_limit() {
        let mut log = CaptureLog::new(1 << 20);
        log.apply(&transfer(1, recorder::Event::Out(vec![1])));
        let t = TransferRow { at: Duration::from_millis(2), wall_ms: None, direction: Direction::In, bytes: vec![2], error: None };
        let s = SignalRow::from_learned(Duration::from_millis(3), None, true, 2, vec![0x23, 0x37]);
        assert_eq!(log.import(vec![s.clone()], vec![t.clone()]), 2);
        assert_eq!(log.transfer_count(), 2);
        assert_eq!(log.transfer(1), Some(&t));
        assert_eq!(log.signal(0), Some(&s));

        let mut small = CaptureLog::new(ROW_OVERHEAD as u64 + 1);
        assert_eq!(small.import(vec![], vec![t.clone(), t]), 1);
        assert!(small.is_full());
    }

    #[test]
    fn clear_empties_both_tabs_and_resets_full() {
        let mut log = CaptureLog::new(ROW_OVERHEAD as u64 + 4);
        log.apply(&transfer(0, recorder::Event::Out(vec![0xFC; 4])));
        log.apply(&transfer(0, recorder::Event::Out(vec![0xFC; 4])));
        assert!(log.is_full());
        log.clear();
        assert!(!log.is_full());
        assert_eq!((log.signal_count(), log.transfer_count(), log.used_bytes()), (0, 0, 0));
        assert!(log.apply(&transfer(0, recorder::Event::Out(vec![0xFC; 4]))).transfer_added);
        assert_eq!(log.transfer(0).unwrap().bytes, vec![0xFC; 4]);
    }

    #[test]
    fn signals_and_transfers_share_the_budget() {
        let mut log = CaptureLog::new(1 << 20);
        log.apply(&transfer(0, recorder::Event::In(vec![1, 2, 3])));
        log.apply(&learned(vec![0x23, 0x37]));
        assert_eq!(log.used_bytes(), 2 * ROW_OVERHEAD as u64 + 3 + 2 + 8 + "unknown".len() as u64);
    }

    #[test]
    fn time_format() {
        assert_eq!(format_time(Duration::from_millis(0)), "0.000");
        assert_eq!(format_time(Duration::from_millis(12_034)), "12.034");
    }

    #[test]
    fn usage() {
        assert_eq!(usage_text(0, 4096), "0 B of 4096 MiB");
        assert_eq!(usage_text(1023, 4096), "1023 B of 4096 MiB");
        assert_eq!(usage_text(1536, 4096), "1.5 KiB of 4096 MiB");
        assert_eq!(usage_text(3 * 1024 * 1024 + 104_858, 10), "3.1 MiB of 10 MiB");
    }

    #[test]
    fn pulse_timings() {
        let p = [Pulse { on_us: 9000, off_us: 4500 }, Pulse { on_us: 560, off_us: 0 }];
        assert_eq!(pulses_text(&p), "9000/4500 560/0");
    }

    #[test]
    fn data_columns() {
        let row = SignalRow {
            at: Duration::ZERO,
            wall_ms: None,
            complete: true,
            announced_len: 2,
            data: vec![0x0A, 0xFF],
            pulses: Some(vec![Pulse { on_us: 160, off_us: 4080 }]),
            summary: String::new(),
        };
        assert_eq!(signal_data_text(&row, Shown::Encoded(Encoding::Hex), &OPTS), "0A FF");
        assert_eq!(signal_data_text(&row, Shown::Encoded(Encoding::Decimal), &OPTS), "10 255");
        assert_eq!(signal_data_text(&row, Shown::Pulses, &OPTS), "160/4080");
        let undecodable = SignalRow { pulses: None, ..row };
        assert_eq!(signal_data_text(&undecodable, Shown::Pulses, &OPTS), "cannot be decoded");

        let out = TransferRow { at: Duration::ZERO, wall_ms: None, direction: Direction::Out, bytes: vec![0xFC, 0xFC], error: None };
        assert_eq!(transfer_data_text(&out, Encoding::Hex, &OPTS), "FC FC");
        let err = TransferRow { direction: Direction::Error, bytes: vec![], error: Some("gone".into()), ..out };
        assert_eq!(transfer_data_text(&err, Encoding::Hex, &OPTS), "gone");
    }
}
