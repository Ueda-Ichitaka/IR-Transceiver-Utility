//! Watching for supported dongles being plugged in and out.
//!
//! Exists so the app can connect when a dongle appears and notice when it
//! goes away while idle (no transfer would fail then). A thread waits on
//! nusb's hotplug stream (driven by `futures_lite::future::block_on`) and,
//! after every event, reports the supported dongles that are present. Only
//! exercised against the real system, not by unit tests; the decision what to
//! do lives in `ir_view::hotplug`.

use std::thread;

use futures_lite::StreamExt;
use futures_lite::future::block_on;

use crate::usb::{self, DeviceSummary};

/// Starts the watcher thread. `report` is called once at start and after
/// every hotplug event with the supported dongles present. The thread runs
/// until the process ends.
pub fn spawn(mut report: impl FnMut(Vec<DeviceSummary>) + Send + 'static) -> Result<(), String> {
    let mut watch = nusb::watch_devices().map_err(|e| format!("hotplug watcher: {e}"))?;
    thread::Builder::new()
        .name("ir-hotplug".into())
        .spawn(move || {
            report(usb::list().unwrap_or_default());
            while block_on(watch.next()).is_some() {
                report(usb::list().unwrap_or_default());
            }
        })
        .map_err(|e| format!("hotplug watcher: {e}"))?;
    Ok(())
}
