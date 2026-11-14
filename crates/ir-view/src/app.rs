//! The GUI's state as one value, updated by batches of session events.
//!
//! Exists so the cxx-qt objects only forward: `AppCore::apply` updates the
//! device view and the capture log from the events the session worker
//! delivers, and reports what changed and whether capture must stop because
//! the capture limit was reached.

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
        }
    }

    pub fn apply(&mut self, events: &[Event]) -> Update {
        let mut update = Update::default();
        for event in events {
            update.status_changed |= self.device.apply(event);
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
        Event::Transfer(Entry { at: Duration::ZERO, event: recorder::Event::In(vec![0; 64]) })
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
    fn limit_message_text() {
        assert_eq!(
            limit_message(4096),
            "Capture stopped: the capture limit of 4096 MiB is reached. Export or clear the capture to continue."
        );
    }
}
