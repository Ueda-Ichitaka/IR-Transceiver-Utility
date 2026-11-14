//! The GUI's state as one value, updated by batches of session events.
//!
//! Exists so the cxx-qt objects only forward: `AppCore::apply` updates the
//! device view and the capture log from the events the session worker
//! delivers, and reports what changed and whether capture must stop because
//! the capture limit was reached. It also tracks the capture start and the
//! outcome of the last send.

use std::time::Instant;

use ir_session::session::{Event, State};
use ir_settings::Settings;

use crate::capture::CaptureLog;
use crate::device::DeviceView;

/// Everything the pages show.
#[derive(Debug)]
pub struct AppCore {
    pub device: DeviceView,
    pub capture: CaptureLog,
    pub settings: Settings,
    capture_started: Option<Instant>,
    last_send: Option<LastSend>,
}

/// The last code handed to the session for sending.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastSend {
    /// When it was sent, in Unix milliseconds.
    pub wall_ms: u64,
    /// What went over USB, e.g. `82 bytes in 2 chunks`.
    pub wire: String,
    pub outcome: SendOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendOutcome {
    Pending,
    Ok,
    Failed(String),
}

impl LastSend {
    /// `82 bytes in 2 chunks · ok` (the time is formatted by the page).
    pub fn text(&self) -> String {
        let outcome = match &self.outcome {
            SendOutcome::Pending => "sending".to_string(),
            SendOutcome::Ok => "ok".to_string(),
            SendOutcome::Failed(reason) => format!("failed: {reason}"),
        };
        format!("{} · {outcome}", self.wire)
    }
}

/// What a batch of events changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Update {
    pub status_changed: bool,
    pub signals_added: usize,
    pub transfers_added: usize,
    /// The capture limit was reached while capturing: send `StopCapture`.
    pub stop_capture: bool,
}

/// Shown when the capture limit is reached.
pub fn limit_message(limit_mib: u64) -> String {
    format!(
        "Capture stopped: the capture limit of {limit_mib} MiB is reached. Export or clear the capture to continue."
    )
}

impl AppCore {
    pub fn new(settings: Settings) -> Self {
        Self {
            device: DeviceView::default(),
            capture: CaptureLog::new(settings.capture_limit_bytes()),
            settings,
            capture_started: None,
            last_send: None,
        }
    }

    /// Notes that a send with `wire` was handed to the session.
    pub fn sending(&mut self, wire: String, wall_ms: u64) {
        self.last_send = Some(LastSend { wall_ms, wire, outcome: SendOutcome::Pending });
    }

    pub fn last_send(&self) -> Option<&LastSend> {
        self.last_send.as_ref()
    }

    /// Resolves a pending send from a session event; true when it changed.
    fn resolve_send(&mut self, event: &Event) -> bool {
        let Some(send) = self.last_send.as_mut().filter(|s| s.outcome == SendOutcome::Pending) else {
            return false;
        };
        send.outcome = match event {
            Event::Transmitted { .. } | Event::RawSent { .. } => SendOutcome::Ok,
            Event::Error(reason) => SendOutcome::Failed(reason.clone()),
            _ => return false,
        };
        true
    }

    /// How long the current capture has been running.
    pub fn capturing_for(&self) -> Option<std::time::Duration> {
        self.capture_started.map(|start| start.elapsed())
    }

    pub fn apply(&mut self, events: &[Event]) -> Update {
        let mut update = Update::default();
        for event in events {
            update.status_changed |= self.device.apply(event);
            update.status_changed |= self.resolve_send(event);
            if let Event::State(state) = event {
                self.capture_started = match (state, self.capture_started) {
                    (State::Capturing, Some(start)) => Some(start),
                    (State::Capturing, None) => Some(Instant::now()),
                    _ => None,
                };
            }
            let applied = self.capture.apply(event);
            update.signals_added += usize::from(applied.signal_added);
            update.transfers_added += usize::from(applied.transfer_added);
            if applied.became_full {
                let message = limit_message(self.settings.capture.limit_mib);
                update.status_changed |= self.device.apply(&Event::Error(message));
                update.stop_capture |= self.device.state() == State::Capturing;
            }
        }
        update
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elk_proto::identify::Protocol;
    use ir_transport::recorder::{self, Entry};
    use std::time::Duration;

    fn chunk() -> Event {
        Event::Transfer(Entry { wall_ms: 0, at: Duration::ZERO, event: recorder::Event::In(vec![0; 64]) })
    }

    #[test]
    fn capture_limit_comes_from_the_settings() {
        let mut settings = Settings::default();
        settings.capture.limit_mib = 1;
        let core = AppCore::new(settings);
        assert_eq!(core.capture.used_bytes(), 0);
        assert_eq!(core.settings.capture.limit_mib, 1);
    }

    #[test]
    fn batch_updates_status_and_counts_rows() {
        let mut core = AppCore::new(Settings::default());
        let update = core.apply(&[
            Event::Connected { device: "dongle".into(), protocol: Protocol::Plain },
            Event::State(State::Ready),
            chunk(),
            chunk(),
        ]);
        assert_eq!(update, Update { status_changed: true, signals_added: 0, transfers_added: 2, stop_capture: false });
        assert_eq!(core.device.device(), Some("dongle"));
        assert_eq!(core.capture.transfer_count(), 2);
    }

    #[test]
    fn rows_alone_do_not_change_the_status() {
        let mut core = AppCore::new(Settings::default());
        assert!(!core.apply(&[chunk()]).status_changed);
    }

    #[test]
    fn reaching_the_limit_while_capturing_stops_capture_with_a_message() {
        let mut settings = Settings::default();
        settings.capture.limit_mib = 0;
        let mut core = AppCore::new(settings);
        core.apply(&[Event::State(State::Capturing)]);
        let update = core.apply(&[chunk()]);
        assert!(update.stop_capture);
        assert!(update.status_changed);
        assert_eq!(core.device.last_error(), Some(limit_message(0).as_str()));
        assert!(core.capture.is_full());
    }

    #[test]
    fn reaching_the_limit_outside_capture_only_reports() {
        let mut settings = Settings::default();
        settings.capture.limit_mib = 0;
        let mut core = AppCore::new(settings);
        let update = core.apply(&[Event::State(State::Ready), chunk()]);
        assert!(!update.stop_capture);
        assert_eq!(core.device.last_error(), Some(limit_message(0).as_str()));
    }

    #[test]
    fn capture_start_is_tracked() {
        let mut core = AppCore::new(Settings::default());
        assert!(core.capturing_for().is_none());
        core.apply(&[Event::State(State::Capturing)]);
        assert!(core.capturing_for().is_some());
        core.apply(&[Event::State(State::Capturing)]);
        assert!(core.capturing_for().unwrap() < std::time::Duration::from_secs(1), "kept from the first event");
        core.apply(&[Event::State(State::Recovering)]);
        assert!(core.capturing_for().is_none());
    }

    #[test]
    fn last_send_is_resolved_by_the_session() {
        let mut core = AppCore::new(Settings::default());
        assert_eq!(core.last_send(), None);
        core.sending("82 bytes in 2 chunks".into(), 1000);
        assert_eq!(core.last_send().unwrap().text(), "82 bytes in 2 chunks · sending");
        assert!(core.apply(&[Event::Transmitted { pulses: 34 }]).status_changed);
        let sent = core.last_send().unwrap();
        assert_eq!((sent.wall_ms, sent.outcome.clone()), (1000, SendOutcome::Ok));
        assert_eq!(sent.text(), "82 bytes in 2 chunks · ok");

        core.sending("4 bytes in 1 chunk".into(), 2000);
        core.apply(&[Event::Error("transmit failed: USB error".into())]);
        assert_eq!(core.last_send().unwrap().text(), "4 bytes in 1 chunk · failed: transmit failed: USB error");

        core.sending("4 bytes in 1 chunk".into(), 3000);
        core.apply(&[Event::RawSent { bytes: 4 }]);
        core.apply(&[Event::Error("unrelated later error".into())]);
        assert_eq!(core.last_send().unwrap().outcome, SendOutcome::Ok, "only a pending send can fail");
    }

    #[test]
    fn limit_message_text() {
        assert_eq!(
            limit_message(4096),
            "Capture stopped: the capture limit of 4096 MiB is reached. Export or clear the capture to continue."
        );
    }
}
