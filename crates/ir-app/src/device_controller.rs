//! `DeviceController`: the QML singleton for the dongle connection.
//!
//! Exists to give every page one object for the connection status and device
//! facts (read from `ir_view::device::DeviceView`), the capture counters and
//! status bar, the last send, the environment check and the session
//! commands. It starts the session worker on first connect and the hotplug
//! watcher on request; the worker's event batches and the watcher's reports
//! are queued onto the GUI thread, applied to the app state and announced
//! with `statusChanged`, `captureChanged` and `environmentChanged`.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
        include!("cxx-qt-lib/qlist.h");
        type QList_i32 = cxx_qt_lib::QList<i32>;
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
        #[qproperty(f64, last_send_wall_ms, READ = last_send_wall_ms, NOTIFY = status_changed)]
        #[qproperty(QString, last_send_text, READ = last_send_text, NOTIFY = status_changed)]
        #[qproperty(QString, sidebar_line, READ = sidebar_line, NOTIFY = status_changed)]
        #[qproperty(QString, location, READ = location, NOTIFY = status_changed)]
        #[qproperty(QStringList, fact_labels, READ = fact_labels, NOTIFY = status_changed)]
        #[qproperty(QStringList, fact_values, READ = fact_values, NOTIFY = status_changed)]
        #[qproperty(f64, connected_since_ms, READ = connected_since_ms, NOTIFY = status_changed)]
        #[qproperty(i32, error_count, READ = error_count, NOTIFY = status_changed)]
        #[qproperty(QList_i32, env_levels, READ = env_levels, NOTIFY = environment_changed)]
        #[qproperty(QStringList, env_texts, READ = env_texts, NOTIFY = environment_changed)]
        #[qproperty(bool, can_connect, READ = can_connect, NOTIFY = status_changed)]
        #[qproperty(bool, can_disconnect, READ = can_disconnect, NOTIFY = status_changed)]
        #[qproperty(bool, ready, READ = ready, NOTIFY = status_changed)]
        #[qproperty(bool, can_stop_capture, READ = can_stop_capture, NOTIFY = status_changed)]
        #[qproperty(i32, signal_count, READ = signal_count, NOTIFY = capture_changed)]
        #[qproperty(i32, transfer_count, READ = transfer_count, NOTIFY = capture_changed)]
        #[qproperty(bool, capture_full, READ = capture_full, NOTIFY = capture_changed)]
        #[qproperty(QString, capture_usage, READ = capture_usage, NOTIFY = capture_changed)]
        #[qproperty(QStringList, capture_status, READ = capture_status, NOTIFY = capture_changed)]
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
        fn last_send_wall_ms(self: &DeviceController) -> f64;
        fn last_send_text(self: &DeviceController) -> QString;
        fn sidebar_line(self: &DeviceController) -> QString;
        fn location(self: &DeviceController) -> QString;
        fn fact_labels(self: &DeviceController) -> QStringList;
        fn fact_values(self: &DeviceController) -> QStringList;
        fn connected_since_ms(self: &DeviceController) -> f64;
        fn error_count(self: &DeviceController) -> i32;
        fn env_levels(self: &DeviceController) -> QList_i32;
        fn env_texts(self: &DeviceController) -> QStringList;
        fn can_connect(self: &DeviceController) -> bool;
        fn can_disconnect(self: &DeviceController) -> bool;
        fn ready(self: &DeviceController) -> bool;
        fn can_stop_capture(self: &DeviceController) -> bool;
        fn signal_count(self: &DeviceController) -> i32;
        fn transfer_count(self: &DeviceController) -> i32;
        fn capture_full(self: &DeviceController) -> bool;
        fn capture_usage(self: &DeviceController) -> QString;
        fn capture_status(self: &DeviceController) -> QStringList;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qsignal]
        fn status_changed(self: Pin<&mut DeviceController>);
        #[qsignal]
        fn capture_changed(self: Pin<&mut DeviceController>);
        #[qsignal]
        fn environment_changed(self: Pin<&mut DeviceController>);

        /// Asks the dongle for its protocol again.
        #[qinvokable]
        fn identify(self: Pin<&mut DeviceController>);
        /// Runs the environment check again.
        #[qinvokable]
        fn refresh_environment(self: Pin<&mut DeviceController>);
        /// Starts watching for dongles being plugged in and out; connects
        /// when one is present. False when the watcher cannot start.
        #[qinvokable]
        fn start_hotplug(self: Pin<&mut DeviceController>) -> bool;

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
        /// Feeds the recorded TV remote fixture through the capture, as if
        /// the dongle had sent it (used by the smoke test).
        #[qinvokable]
        fn replay_fixture(self: Pin<&mut DeviceController>);
        /// Re-reads the connection status and last send (after a send).
        #[qinvokable]
        fn refresh_status(self: Pin<&mut DeviceController>);
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
use cxx_qt_lib::{QList, QString, QStringList};
use ir_session::session::{Command, Event, Session};
use ir_session::usb::UsbConnector;
use ir_transport::usb::{self, DeviceSummary};
use ir_transport::{env, hotplug};
use ir_session::worker;
use ir_library::model::{Code, Payload};
use ir_view::app::Update;
use ir_view::send::command_for;
use ir_view::text::capitalise;
use ir_view::capture::usage_text;
use ir_view::capture_view::status_parts;
use ir_transport::recorder::now_unix_ms;
use ir_view::replay::{TV_POWER_LOG, events_from_probe_log};

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

    fn last_send_wall_ms(&self) -> f64 {
        state::with(|s| s.core.last_send().map_or(-1.0, |l| l.wall_ms as f64))
    }

    fn last_send_text(&self) -> QString {
        state::with(|s| QString::from(&s.core.last_send().map(|l| l.text()).unwrap_or_default()))
    }

    fn refresh_status(self: Pin<&mut Self>) {
        self.status_changed();
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

    fn capture_status(&self) -> QStringList {
        let parts = state::with(|s| {
            status_parts(
                s.core.capturing_for(),
                s.core.capture.stats(),
                s.core.capture.used_bytes(),
                s.core.settings.capture.limit_mib,
            )
        });
        let parts: Vec<QString> = parts.iter().map(QString::from).collect();
        parts.iter().collect()
    }

    /// Starts the session worker if it is not running yet.
    fn ensure_session(&self) {
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
    }

    fn connect_device(self: Pin<&mut Self>) {
        self.ensure_session();
        self.send(Command::Connect);
    }

    fn identify(self: Pin<&mut Self>) {
        self.send(Command::Identify);
    }

    fn sidebar_line(&self) -> QString {
        state::with(|s| QString::from(&s.core.device.sidebar_line()))
    }

    fn location(&self) -> QString {
        state::with(|s| qs(s.core.device.location()))
    }

    fn fact_labels(&self) -> QStringList {
        let labels: Vec<QString> = state::with(|s| s.core.device.facts().iter().map(|(l, _)| QString::from(l)).collect());
        labels.iter().collect()
    }

    fn fact_values(&self) -> QStringList {
        let values: Vec<QString> = state::with(|s| s.core.device.facts().iter().map(|(_, v)| QString::from(v)).collect());
        values.iter().collect()
    }

    fn connected_since_ms(&self) -> f64 {
        state::with(|s| s.core.device.connected_since_ms().map_or(-1.0, |ms| ms as f64))
    }

    fn error_count(&self) -> i32 {
        state::with(|s| count(s.core.device.error_count()))
    }

    fn environment(&self) -> Vec<env::Check> {
        state::with(|s| env::run(&s.devices, env::Roots::system(), s.hotplug_active))
    }

    fn env_levels(&self) -> QList<i32> {
        self.environment()
            .iter()
            .map(|c| match c.level {
                env::Level::Ok => 0,
                env::Level::Warning => 1,
                env::Level::Problem => 2,
            })
            .collect()
    }

    fn env_texts(&self) -> QStringList {
        let texts: Vec<QString> = self.environment().iter().map(|c| QString::from(&c.text)).collect();
        texts.iter().collect()
    }

    fn refresh_environment(self: Pin<&mut Self>) {
        let hotplug = state::with(|s| s.hotplug_active);
        if !hotplug {
            // Without the watcher, list the dongles now.
            let devices = usb::list().unwrap_or_default();
            state::with(|s| s.devices = devices);
        }
        self.environment_changed();
    }

    fn start_hotplug(self: Pin<&mut Self>) -> bool {
        let thread = self.qt_thread();
        let started = hotplug::spawn(move |devices| {
            let _ = thread.queue(move |controller| controller.devices_changed(devices));
        });
        match started {
            Ok(()) => {
                state::with(|s| s.hotplug_active = true);
                true
            }
            Err(message) => {
                self.apply_events(vec![Event::Error(message)]);
                false
            }
        }
    }

    /// The hotplug watcher reported the dongles now present.
    fn devices_changed(mut self: Pin<&mut Self>, devices: Vec<DeviceSummary>) {
        let present = devices.len();
        let command = state::with(|s| {
            s.devices = devices;
            ir_view::hotplug::command(present, s.core.device.state())
        });
        match command {
            Some(Command::Connect) => self.as_mut().connect_device(),
            Some(command) => self.as_mut().send(command),
            None => {}
        }
        self.environment_changed();
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

    fn replay_fixture(self: Pin<&mut Self>) {
        self.apply_events(events_from_probe_log(TV_POWER_LOG, now_unix_ms()));
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
        let result = match command {
            Some(Ok(command)) => state::with(|s| s.send_command(command)),
            Some(Err(message)) => Err(message),
            None => return,
        };
        match result {
            Ok(()) => self.status_changed(),
            Err(message) => self.apply_events(vec![Event::Error(capitalise(&message))]),
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
