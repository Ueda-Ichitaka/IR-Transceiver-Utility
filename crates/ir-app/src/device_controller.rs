//! `DeviceController`: the QML singleton for the dongle connection.
//!
//! Exists to give every page one object for the connection status (read from
//! `ir_view::device::DeviceView`), the capture counters and the session
//! commands. It starts the session worker on first connect; the worker's
//! event batches are queued onto the GUI thread, applied to the app state
//! and announced with `statusChanged` and `captureChanged`.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, state_label, READ = state_label, NOTIFY = status_changed)]
        #[qproperty(QString, device, READ = device, NOTIFY = status_changed)]
        #[qproperty(QString, protocol, READ = protocol, NOTIFY = status_changed)]
        #[qproperty(QString, banner, READ = banner, NOTIFY = status_changed)]
        #[qproperty(QString, last_error, READ = last_error, NOTIFY = status_changed)]
        #[qproperty(QString, last_sent, READ = last_sent, NOTIFY = status_changed)]
        #[qproperty(bool, can_connect, READ = can_connect, NOTIFY = status_changed)]
        #[qproperty(bool, can_disconnect, READ = can_disconnect, NOTIFY = status_changed)]
        #[qproperty(bool, ready, READ = ready, NOTIFY = status_changed)]
        #[qproperty(bool, can_stop_capture, READ = can_stop_capture, NOTIFY = status_changed)]
        #[qproperty(i32, signal_count, READ = signal_count, NOTIFY = capture_changed)]
        #[qproperty(i32, transfer_count, READ = transfer_count, NOTIFY = capture_changed)]
        #[qproperty(bool, capture_full, READ = capture_full, NOTIFY = capture_changed)]
        #[qproperty(QString, capture_usage, READ = capture_usage, NOTIFY = capture_changed)]
        type DeviceController = super::DeviceControllerRust;
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        fn state_label(self: &DeviceController) -> QString;
        fn device(self: &DeviceController) -> QString;
        fn protocol(self: &DeviceController) -> QString;
        fn banner(self: &DeviceController) -> QString;
        fn last_error(self: &DeviceController) -> QString;
        fn last_sent(self: &DeviceController) -> QString;
        fn can_connect(self: &DeviceController) -> bool;
        fn can_disconnect(self: &DeviceController) -> bool;
        fn ready(self: &DeviceController) -> bool;
        fn can_stop_capture(self: &DeviceController) -> bool;
        fn signal_count(self: &DeviceController) -> i32;
        fn transfer_count(self: &DeviceController) -> i32;
        fn capture_full(self: &DeviceController) -> bool;
        fn capture_usage(self: &DeviceController) -> QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qsignal]
        fn status_changed(self: Pin<&mut DeviceController>);
        #[qsignal]
        fn capture_changed(self: Pin<&mut DeviceController>);

        /// Connects to the first supported dongle.
        #[qinvokable]
        fn connect_device(self: Pin<&mut DeviceController>);
        #[qinvokable]
        fn disconnect_device(self: Pin<&mut DeviceController>);
        /// Waits up to `wait_s` seconds for one code.
        #[qinvokable]
        fn learn_once(self: Pin<&mut DeviceController>, wait_s: i32);
        #[qinvokable]
        fn start_capture(self: Pin<&mut DeviceController>);
        #[qinvokable]
        fn stop_capture(self: Pin<&mut DeviceController>);
        #[qinvokable]
        fn clear_capture(self: Pin<&mut DeviceController>);
        #[qinvokable]
        fn clear_error(self: Pin<&mut DeviceController>);
        /// Re-reads the capture counters and usage (after the limit changed).
        #[qinvokable]
        fn refresh_capture(self: Pin<&mut DeviceController>);
        /// Sends captured signal `row` again with the global defaults.
        #[qinvokable]
        fn send_signal(self: Pin<&mut DeviceController>, row: i32);
    }

    impl cxx_qt::Threading for DeviceController {}
}

use std::pin::Pin;
use cxx_qt::Threading;
use cxx_qt_lib::QString;
use ir_session::session::{Command, Event, Session};
use ir_session::usb::UsbConnector;
use ir_session::worker;
use ir_library::model::{Code, Payload};
use ir_view::app::Update;
use ir_view::send::command_for;
use ir_view::capture::usage_text;

use crate::state;

/// Rust side of `DeviceController`; all data lives in `state`.
#[derive(Default)]
pub struct DeviceControllerRust;

fn qs(text: Option<&str>) -> QString {
    QString::from(text.unwrap_or(""))
}

fn count(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

impl qobject::DeviceController {
    fn state_label(&self) -> QString {
        state::with(|s| QString::from(s.core.device.state_label()))
    }

    fn device(&self) -> QString {
        state::with(|s| qs(s.core.device.device()))
    }

    fn protocol(&self) -> QString {
        state::with(|s| qs(s.core.device.protocol_label()))
    }

    fn banner(&self) -> QString {
        state::with(|s| qs(s.core.device.banner()))
    }

    fn last_error(&self) -> QString {
        state::with(|s| qs(s.core.device.last_error()))
    }

    fn last_sent(&self) -> QString {
        state::with(|s| qs(s.core.device.last_sent()))
    }

    fn can_connect(&self) -> bool {
        state::with(|s| s.core.device.can_connect())
    }

    fn can_disconnect(&self) -> bool {
        state::with(|s| s.core.device.can_disconnect())
    }

    fn ready(&self) -> bool {
        state::with(|s| s.core.device.is_ready())
    }

    fn can_stop_capture(&self) -> bool {
        state::with(|s| s.core.device.can_stop_capture())
    }

    fn signal_count(&self) -> i32 {
        state::with(|s| count(s.core.capture.signal_count()))
    }

    fn transfer_count(&self) -> i32 {
        state::with(|s| count(s.core.capture.transfer_count()))
    }

    fn capture_full(&self) -> bool {
        state::with(|s| s.core.capture.is_full())
    }

    fn capture_usage(&self) -> QString {
        state::with(|s| QString::from(&usage_text(s.core.capture.used_bytes(), s.core.settings.capture.limit_mib)))
    }

    fn connect_device(self: Pin<&mut Self>) {
        let thread = self.qt_thread();
        state::with(|s| {
            if s.session.is_none() {
                let session = Session::new(UsbConnector, Box::new(std::thread::sleep));
                s.session = Some(worker::spawn(session, move |events| {
                    // Fails only when the controller is gone (app quitting).
                    let _ = thread.queue(move |controller| controller.apply_events(events));
                }));
            }
        });
        self.send(Command::Connect);
    }

    fn disconnect_device(self: Pin<&mut Self>) {
        self.send(Command::Disconnect);
    }

    fn learn_once(self: Pin<&mut Self>, wait_s: i32) {
        self.send(Command::LearnOnce { wait_s: u32::try_from(wait_s).unwrap_or(0) });
    }

    fn start_capture(self: Pin<&mut Self>) {
        self.send(Command::StartCapture);
    }

    fn stop_capture(self: Pin<&mut Self>) {
        self.send(Command::StopCapture);
    }

    fn clear_capture(self: Pin<&mut Self>) {
        state::with(|s| s.core.capture.clear());
        self.capture_changed();
    }

    fn refresh_capture(self: Pin<&mut Self>) {
        self.capture_changed();
    }

    fn clear_error(self: Pin<&mut Self>) {
        state::with(|s| s.core.device.clear_error());
        self.status_changed();
    }

    fn send_signal(self: Pin<&mut Self>, row: i32) {
        let command = state::with(|s| {
            let signal = usize::try_from(row).ok().and_then(|r| s.core.capture.signal(r))?;
            let code = Code {
                id: 0,
                name: String::new(),
                tags: Vec::new(),
                carrier_hz: None,
                repeat: None,
                payload: Payload::Learned { data: signal.data.clone() },
                notes: String::new(),
            };
            Some(command_for(&code, &s.core.settings.send_defaults()))
        });
        match command {
            Some(Ok(command)) => self.send(command),
            Some(Err(message)) => self.apply_events(vec![Event::Error(message)]),
            None => {}
        }
    }

    /// Sends a command to the session worker; reports a missing worker.
    fn send(self: Pin<&mut Self>, command: Command) {
        let sent = state::with(|s| s.session.as_ref().is_some_and(|session| session.send(command)));
        if !sent {
            self.apply_events(vec![Event::Error("The dongle is not connected.".into())]);
        }
    }

    /// Applies a batch of session events on the GUI thread.
    fn apply_events(mut self: Pin<&mut Self>, events: Vec<Event>) {
        let update: Update = state::with(|s| {
            let update = s.core.apply(&events);
            if update.stop_capture {
                if let Some(session) = &s.session {
                    session.send(Command::StopCapture);
                }
            }
            update
        });
        if update.status_changed {
            self.as_mut().status_changed();
        }
        if update.signals_added > 0 || update.transfers_added > 0 || update.status_changed {
            self.as_mut().capture_changed();
        }
    }
}
