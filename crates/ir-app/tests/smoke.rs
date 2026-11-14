//! Smoke test: the real `ir-transceiver` binary loads its QML.
//!
//! Exists to catch broken QML, missing QML modules (Kirigami) and bridge
//! registration errors that unit tests cannot see. The app is started on
//! Qt's offscreen platform with `--smoke-test`; the main window then opens
//! every page, logs a marker per page and one when done, and quits. A QML
//! warning or error, a missing marker or a hang fails the test. In smoke-test
//! mode the app does not connect to a dongle.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MARKER: &str = "ir-transceiver: main window ready";
const PAGES: [&str; 4] = ["Capture", "Send", "Library", "Device"];
/// Text Qt prints for QML problems.
const QML_PROBLEMS: [&str; 7] =
    ["qrc:/", "qt.qml.", "TypeError", "ReferenceError", "is not a type", "Cannot assign", "is not defined"];
const TIMEOUT: Duration = Duration::from_secs(60);

/// An empty directory under target/test-tmp.
fn test_dir(name: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-tmp/ir-app").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

#[test]
fn main_window_loads_offscreen() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ir-transceiver"))
        .arg("--smoke-test")
        .env("QT_QPA_PLATFORM", "offscreen")
        // Qt logs to the systemd journal when stderr is not a terminal.
        .env("QT_FORCE_STDERR_LOGGING", "1")
        // Keep the user's settings and library out of the test.
        .env("XDG_CONFIG_HOME", test_dir("config"))
        .env("XDG_DATA_HOME", test_dir("data"))
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
    for page in PAGES {
        let marker = format!("ir-transceiver: page {page} ready");
        assert!(stderr.contains(&marker), "{marker:?} missing, stderr:\n{stderr}");
    }
    for problem in QML_PROBLEMS {
        assert!(!stderr.contains(problem), "QML problem {problem:?}, stderr:\n{stderr}");
    }
}
