//! Smoke test: the real `ir-transceiver` binary loads its QML.
//!
//! Exists to catch broken QML, missing QML modules (Kirigami) and bridge
//! registration errors that unit tests cannot see. The app is started on
//! Qt's offscreen platform with `--smoke-test`; the main window then logs a
//! marker and quits. A QML error, a missing marker or a hang fails the test.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MARKER: &str = "ir-transceiver: main window ready";
const TIMEOUT: Duration = Duration::from_secs(60);

#[test]
fn main_window_loads_offscreen() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ir-transceiver"))
        .arg("--smoke-test")
        .env("QT_QPA_PLATFORM", "offscreen")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start ir-transceiver");
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait for ir-transceiver") {
            break status;
        }
        if start.elapsed() > TIMEOUT {
            let _ = child.kill();
            panic!("ir-transceiver did not quit within {TIMEOUT:?}");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut stderr = String::new();
    child.stderr.take().unwrap().read_to_string(&mut stderr).unwrap();
    assert!(status.success(), "exit status {status}, stderr:\n{stderr}");
    assert!(stderr.contains(MARKER), "marker missing, stderr:\n{stderr}");
}
