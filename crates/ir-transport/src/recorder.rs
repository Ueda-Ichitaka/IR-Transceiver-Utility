//! `Recorder`: a `Transport` wrapper that logs every transfer.
//!
//! Exists to capture raw dongle traffic as fixtures and as the app's USB log.
//! Every successful write and read is logged with its offset from the start;
//! read timeouts are counted instead of logged, other errors are logged.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use crate::transport::{Transport, TransportError};

/// Direction or kind of a logged event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Out(Vec<u8>),
    In(Vec<u8>),
    Error(TransportError),
}

/// One logged event with its time offset from the recorder's start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub at: Duration,
    pub event: Event,
}

/// Wraps a transport and logs its traffic.
pub struct Recorder<T> {
    inner: T,
    start: Instant,
    entries: Vec<Entry>,
    read_timeouts: u64,
}

impl<T: Transport> Recorder<T> {
    pub fn new(inner: T) -> Self {
        Self { inner, start: Instant::now(), entries: Vec::new(), read_timeouts: 0 }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Number of reads that timed out (not logged as entries).
    pub fn read_timeouts(&self) -> u64 {
        self.read_timeouts
    }

    pub fn into_inner(self) -> T {
        self.inner
    }

    fn log(&mut self, event: Event) {
        self.entries.push(Entry { at: self.start.elapsed(), event });
    }
}

impl<T: Transport> Transport for Recorder<T> {
    fn write(&mut self, data: &[u8], timeout: Duration) -> Result<(), TransportError> {
        let result = self.inner.write(data, timeout);
        match &result {
            Ok(()) => self.log(Event::Out(data.to_vec())),
            Err(e) => self.log(Event::Error(e.clone())),
        }
        result
    }

    fn read(&mut self, timeout: Duration) -> Result<Vec<u8>, TransportError> {
        let result = self.inner.read(timeout);
        match &result {
            Ok(bytes) => self.log(Event::In(bytes.clone())),
            Err(TransportError::Timeout) => self.read_timeouts += 1,
            Err(e) => self.log(Event::Error(e.clone())),
        }
        result
    }
}

/// Formats bytes as uppercase hex separated by spaces.
pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let _ = write!(out, "{b:02X}");
    }
    out
}

/// Formats one entry as a log line: `   1.234  OUT  FC FC FC FC`.
pub fn format_entry(entry: &Entry) -> String {
    let (tag, body) = match &entry.event {
        Event::Out(b) => ("OUT", hex(b)),
        Event::In(b) => ("IN ", hex(b)),
        Event::Error(e) => ("ERR", e.to_string()),
    };
    let millis = entry.at.as_millis();
    format!("{:>4}.{:03}  {tag}  {body}", millis / 1000, millis % 1000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::{MockTransport, Step};

    const T: Duration = Duration::from_millis(1);

    #[test]
    fn logs_writes_and_reads_in_order() {
        let mock = MockTransport::new([Step::ExpectWrite(vec![0xFC; 4]), Step::Reply(vec![0xAB])]);
        let mut r = Recorder::new(mock);
        r.write(&[0xFC; 4], T).unwrap();
        r.read(T).unwrap();
        let events: Vec<_> = r.entries().iter().map(|e| e.event.clone()).collect();
        assert_eq!(events, vec![Event::Out(vec![0xFC; 4]), Event::In(vec![0xAB])]);
        assert!(r.into_inner().is_done());
    }

    #[test]
    fn read_timeouts_are_counted_not_logged() {
        let mut r = Recorder::new(MockTransport::new([Step::Timeout, Step::Timeout]));
        assert_eq!(r.read(T), Err(TransportError::Timeout));
        assert_eq!(r.read(T), Err(TransportError::Timeout));
        assert!(r.entries().is_empty());
        assert_eq!(r.read_timeouts(), 2);
    }

    #[test]
    fn entry_offsets_are_monotonic() {
        let mock = MockTransport::new([Step::Reply(vec![1]), Step::Reply(vec![2])]);
        let mut r = Recorder::new(mock);
        r.read(T).unwrap();
        r.read(T).unwrap();
        assert!(r.entries()[0].at <= r.entries()[1].at);
    }

    #[test]
    fn hex_is_uppercase_space_separated() {
        assert_eq!(hex(&[0x0A, 0xFF, 0x00]), "0A FF 00");
        assert_eq!(hex(&[]), "");
    }

    #[test]
    fn format_entry_lines() {
        let out = Entry { at: Duration::from_millis(1234), event: Event::Out(vec![0xFC, 0xFC]) };
        let inn = Entry { at: Duration::from_millis(5), event: Event::In(vec![0x01]) };
        let err = Entry { at: Duration::from_secs(12), event: Event::Error(TransportError::Disconnected) };
        assert_eq!(format_entry(&out), "   1.234  OUT  FC FC");
        assert_eq!(format_entry(&inn), "   0.005  IN   01");
        assert_eq!(format_entry(&err), "  12.000  ERR  device disconnected");
    }
}
