//! The environment check on the device page.
//!
//! Exists to tell the user, before anything fails, whether the system is set
//! up for the dongle: read and write access to its USB device node (the udev
//! rule), whether a kernel driver is bound to interface 0 (`usbfs` means this
//! app holds it), whether lircd is running (not needed by this app) and
//! whether the hotplug watcher runs. Linux only; the file system roots are
//! parameters so the checks are tested with fake trees.

use std::fs::OpenOptions;
use std::path::Path;

use crate::usb::{DeviceSummary, INTERFACE};

/// How a check turned out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Warning,
    Problem,
}

/// One line of the environment check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub level: Level,
    pub text: String,
}

/// Where the checks look; `Roots::system()` is the real system.
#[derive(Debug, Clone, Copy)]
pub struct Roots<'a> {
    /// usbfs device nodes, normally `/dev/bus/usb`.
    pub dev_usb: &'a Path,
    /// procfs, normally `/proc`.
    pub proc: &'a Path,
}

impl Roots<'static> {
    pub fn system() -> Self {
        Self { dev_usb: Path::new("/dev/bus/usb"), proc: Path::new("/proc") }
    }
}

/// All checks for the supported dongles that are plugged in.
pub fn run(devices: &[DeviceSummary], roots: Roots<'_>, hotplug_active: bool) -> Vec<Check> {
    let check = |level, text: &str| Check { level, text: text.to_string() };
    let mut checks = Vec::new();
    match devices.first() {
        None => checks.push(check(Level::Warning, "No supported dongle is plugged in")),
        Some(device) => {
            let node = roots.dev_usb.join(&device.bus_id).join(format!("{:03}", device.address));
            checks.push(match OpenOptions::new().read(true).write(true).open(&node) {
                Ok(_) => check(Level::Ok, "USB node is readable and writable by your user (udev)"),
                Err(e) => check(
                    Level::Problem,
                    &format!(
                        "No read and write access to {} ({e}); install the udev rule and replug the dongle",
                        node.display()
                    ),
                ),
            });
            let driver = device.sysfs_path.as_deref().and_then(interface_driver);
            checks.push(match driver.as_deref() {
                None => check(Level::Ok, &format!("Interface {INTERFACE} is free (no kernel driver attached)")),
                Some("usbfs") => check(Level::Ok, &format!("Interface {INTERFACE} is claimed by this app")),
                Some(name) => check(
                    Level::Warning,
                    &format!("Interface {INTERFACE} is bound to kernel driver {name}; it is detached when connecting"),
                ),
            });
        }
    }
    checks.push(if lircd_running(roots.proc) {
        check(Level::Warning, "lircd is running. Not needed by this app.")
    } else {
        check(Level::Ok, "lircd is not running")
    });
    checks.push(if hotplug_active {
        check(Level::Ok, "Hotplug watcher active")
    } else {
        check(Level::Warning, "Hotplug watcher not running; connect by hand after plugging in")
    });
    checks
}

/// True when a process named `lircd` runs.
pub fn lircd_running(proc: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(proc) else { return false };
    entries.flatten().filter(|e| e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit())).any(|e| {
        std::fs::read_to_string(e.path().join("comm")).is_ok_and(|comm| comm.trim() == "lircd")
    })
}

/// Name of the kernel driver bound to interface `INTERFACE` of the device in
/// `sysfs_path` (configuration 1), if any.
pub fn interface_driver(sysfs_path: &Path) -> Option<String> {
    let name = sysfs_path.file_name()?.to_string_lossy();
    let interface = sysfs_path.with_file_name(format!("{name}:1.{INTERFACE}"));
    let target = std::fs::read_link(interface.join("driver")).ok()?;
    Some(target.file_name()?.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-tmp/ir-transport-env").join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A fake system: one dongle node with `mode`, its sysfs directory with an
    /// optional interface driver, and processes named `processes`.
    fn system(name: &str, mode: u32, driver: Option<&str>, processes: &[&str]) -> (PathBuf, DeviceSummary) {
        let root = temp_dir(name);
        let node_dir = root.join("dev/bus/usb/003");
        fs::create_dir_all(&node_dir).unwrap();
        let node = node_dir.join("014");
        fs::write(&node, b"").unwrap();
        fs::set_permissions(&node, fs::Permissions::from_mode(mode)).unwrap();
        let sysfs = root.join("sys/bus/usb/devices/3-2");
        fs::create_dir_all(&sysfs).unwrap();
        let iface = root.join("sys/bus/usb/devices/3-2:1.0");
        fs::create_dir_all(&iface).unwrap();
        if let Some(driver) = driver {
            let target = root.join("sys/bus/usb/drivers").join(driver);
            fs::create_dir_all(&target).unwrap();
            symlink(&target, iface.join("driver")).unwrap();
        }
        for (pid, process) in processes.iter().enumerate() {
            let dir = root.join("proc").join((100 + pid).to_string());
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("comm"), format!("{process}\n")).unwrap();
        }
        fs::create_dir_all(root.join("proc/self")).unwrap();
        let summary = DeviceSummary {
            product_id: 0x0132,
            bus_id: "003".into(),
            address: 14,
            serial: None,
            product: None,
            sysfs_path: Some(sysfs),
        };
        (root, summary)
    }

    fn roots(root: &Path) -> (PathBuf, PathBuf) {
        (root.join("dev/bus/usb"), root.join("proc"))
    }

    fn texts(checks: &[Check]) -> Vec<(Level, &str)> {
        checks.iter().map(|c| (c.level, c.text.as_str())).collect()
    }

    #[test]
    fn everything_fine() {
        let (root, dev) = system("fine", 0o600, None, &["bash", "plasmashell"]);
        let (dev_usb, proc) = roots(&root);
        let checks = run(&[dev], Roots { dev_usb: &dev_usb, proc: &proc }, true);
        assert_eq!(
            texts(&checks),
            vec![
                (Level::Ok, "USB node is readable and writable by your user (udev)"),
                (Level::Ok, "Interface 0 is free (no kernel driver attached)"),
                (Level::Ok, "lircd is not running"),
                (Level::Ok, "Hotplug watcher active"),
            ]
        );
    }

    #[test]
    fn problems_and_warnings() {
        let (root, dev) = system("problems", 0o000, Some("usbhid"), &["lircd"]);
        let (dev_usb, proc) = roots(&root);
        let checks = run(&[dev], Roots { dev_usb: &dev_usb, proc: &proc }, false);
        let node = dev_usb.join("003/014");
        assert_eq!(checks[0].level, Level::Problem);
        assert!(
            checks[0].text.starts_with(&format!("No read and write access to {}", node.display())),
            "{}",
            checks[0].text
        );
        assert!(checks[0].text.ends_with("install the udev rule and replug the dongle"), "{}", checks[0].text);
        assert_eq!(
            texts(&checks[1..]),
            vec![
                (Level::Warning, "Interface 0 is bound to kernel driver usbhid; it is detached when connecting"),
                (Level::Warning, "lircd is running. Not needed by this app."),
                (Level::Warning, "Hotplug watcher not running; connect by hand after plugging in"),
            ]
        );
    }

    #[test]
    fn usbfs_means_this_app_holds_the_interface() {
        let (root, dev) = system("usbfs", 0o600, Some("usbfs"), &[]);
        let (dev_usb, proc) = roots(&root);
        let checks = run(&[dev], Roots { dev_usb: &dev_usb, proc: &proc }, true);
        assert_eq!(texts(&checks)[1], (Level::Ok, "Interface 0 is claimed by this app"));
    }

    #[test]
    fn no_dongle_plugged_in() {
        let root = temp_dir("none");
        let (dev_usb, proc) = roots(&root);
        let checks = run(&[], Roots { dev_usb: &dev_usb, proc: &proc }, true);
        assert_eq!(texts(&checks)[0], (Level::Warning, "No supported dongle is plugged in"));
        assert_eq!(checks.len(), 3, "no interface check without a dongle");
    }

    #[test]
    fn helpers() {
        let (root, dev) = system("helpers", 0o600, Some("usbfs"), &["lircd"]);
        assert!(lircd_running(&root.join("proc")));
        assert_eq!(interface_driver(dev.sysfs_path.as_deref().unwrap()), Some("usbfs".to_string()));
        assert!(!lircd_running(&root.join("missing")));
    }
}
