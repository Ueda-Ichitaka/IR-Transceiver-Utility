//! Session events from an `elk-probe` log.
//!
//! Exists so a recorded hardware session can be fed through the GUI without
//! a dongle: the GUI smoke test replays the fixture of a learned TV remote to
//! check that capture rows really render. Transfer lines become transfer
//! events, each `# data:` line a completed learn.

use std::time::Duration;

use elk_proto::learn::LearnedCode;
use ir_session::session::Event;
use ir_transport::ops::LearnOutcome;
use ir_transport::recorder::{self, Entry};

/// The fixture recorded from a TV remote (three learned codes).
pub const TV_POWER_LOG: &str = include_str!("../../../fixtures/1794596116-learn-tv-power.log");

/// Transfer events of the log in order, followed by one learned event per
/// `# data:` line, with wall-clock times counted from `base_wall_ms`. Lines
/// that do not parse are skipped.
pub fn events_from_probe_log(text: &str, base_wall_ms: u64) -> Vec<Event> {
    let mut transfers = Vec::new();
    let mut learned = Vec::new();
    for line in text.lines() {
        if let Some(data) = line.strip_prefix("# data:") {
            if let Some(data) = hex_bytes(data) {
                learned.push(Event::Learned(LearnOutcome::Complete(LearnedCode { announced_len: data.len(), data })));
            }
        } else if let Some(entry) = transfer(line, base_wall_ms) {
            transfers.push(Event::Transfer(entry));
        }
    }
    transfers.extend(learned);
    transfers
}

/// `   0.010  OUT  FC FC FC FC` -> an entry.
fn transfer(line: &str, base_wall_ms: u64) -> Option<Entry> {
    let mut words = line.split_whitespace();
    let (secs, millis) = words.next()?.split_once('.')?;
    let at = Duration::from_secs(secs.parse().ok()?) + Duration::from_millis(millis.parse().ok()?);
    let direction = words.next()?;
    let bytes = hex_bytes(&words.collect::<Vec<_>>().join(" "))?;
    let event = match direction {
        "OUT" => recorder::Event::Out(bytes),
        "IN" => recorder::Event::In(bytes),
        _ => return None,
    };
    Some(Entry { at, wall_ms: base_wall_ms + at.as_millis() as u64, event })
}

/// Space-separated two-digit hex bytes; `None` if any word is not one.
fn hex_bytes(text: &str) -> Option<Vec<u8>> {
    text.split_whitespace()
        .map(|w| if w.len() == 2 { u8::from_str_radix(w, 16).ok() } else { None })
        .collect::<Option<Vec<u8>>>()
        .filter(|bytes| !bytes.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_replays_transfers_and_three_codes() {
        let events = events_from_probe_log(TV_POWER_LOG, 1_000_000);
        let transfers: Vec<&Entry> =
            events.iter().filter_map(|e| if let Event::Transfer(t) = e { Some(t) } else { None }).collect();
        let lines = TV_POWER_LOG.lines().filter(|l| l.contains("  OUT  ") || l.contains("  IN   ")).count();
        assert_eq!(transfers.len(), lines);
        assert_eq!(transfers[0], &Entry { wall_ms: 1_000_010, at: Duration::from_millis(10), event: recorder::Event::Out(vec![0xFC; 4]) });
        assert_eq!(transfers[1].event, recorder::Event::In(vec![0xFC, 0xFC, 0xFC, 0xFC, 0x70, 0x01]));
        assert_eq!(transfers[3].at, Duration::from_millis(19_708));
        let learned: Vec<&LearnedCode> = events
            .iter()
            .filter_map(|e| if let Event::Learned(LearnOutcome::Complete(c)) = e { Some(c) } else { None })
            .collect();
        assert_eq!(learned.len(), 3);
        assert_eq!(learned[0].data.len(), 216);
        assert_eq!(learned[0].announced_len, 216);
        assert_eq!(&learned[0].data[..4], &[0xFF, 0xFF, 0x37, 0xFF]);
        assert!(matches!(events.last(), Some(Event::Learned(_))));
    }

    #[test]
    fn junk_is_skipped() {
        assert!(events_from_probe_log("hello\n  x  IN   ZZ\n# data: 1\n", 0).is_empty());
    }
}
