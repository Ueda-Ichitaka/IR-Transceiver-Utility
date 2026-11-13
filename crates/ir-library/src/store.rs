//! Reading and writing the library file.
//!
//! Exists so saving can never leave a half-written library behind (write to a
//! temporary file, flush, then rename) and a damaged file never blocks the
//! app or gets overwritten: it is moved aside to a timestamped backup and the
//! app starts with an empty library plus a message saying why.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::model::{Library, LibraryError};

/// Directory name under the XDG data directory (the app id).
pub const APP_DIR: &str = "io.github.ueda.irtransceiver";
pub const FILE_NAME: &str = "library.toml";

/// Result of loading the library file.
#[derive(Debug)]
pub struct Loaded {
    pub library: Library,
    /// Set when the file was damaged or unsupported: where it was moved and
    /// why. The returned library is then empty.
    pub recovered: Option<(PathBuf, LibraryError)>,
}

/// `$XDG_DATA_HOME/<app>/library.toml`, falling back to
/// `$HOME/.local/share/<app>/library.toml`. `env` looks up variables.
pub fn default_path(env: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let base = match env("XDG_DATA_HOME").map(PathBuf::from) {
        Some(dir) if dir.is_absolute() => dir,
        _ => PathBuf::from(env("HOME").filter(|h| !h.is_empty())?).join(".local/share"),
    };
    Some(base.join(APP_DIR).join(FILE_NAME))
}

/// Writes the library atomically, creating the directory if needed.
pub fn save(path: &Path, library: &Library) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut tmp_name = path.as_os_str().to_owned();
    tmp_name.push(".tmp");
    let tmp = PathBuf::from(tmp_name);
    let mut file = fs::File::create(&tmp)?;
    file.write_all(library.to_toml().as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(&tmp, path)
}

/// Loads the library. A missing file gives an empty library; a damaged or
/// unsupported file is renamed to `<file>.damaged-<now_secs>`.
pub fn load(path: &Path, now_secs: u64) -> io::Result<Loaded> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Ok(Loaded { library: Library::default(), recovered: None });
        }
        Err(e) => return Err(e),
    };
    match Library::from_toml(&text) {
        Ok(library) => Ok(Loaded { library, recovered: None }),
        Err(reason) => {
            let mut backup = path.as_os_str().to_owned();
            backup.push(format!(".damaged-{now_secs}"));
            let backup = PathBuf::from(backup);
            fs::rename(path, &backup)?;
            Ok(Loaded { library: Library::default(), recovered: Some((backup, reason)) })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Payload;

    /// A fresh directory under target/test-tmp for one test.
    fn temp_dir(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-tmp/ir-library").join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| vars.iter().find(|(k, _)| *k == key).map(|(_, v)| v.to_string())
    }

    #[test]
    fn default_path_prefers_xdg_data_home() {
        assert_eq!(
            default_path(env(&[("XDG_DATA_HOME", "/data"), ("HOME", "/home/u")])),
            Some(PathBuf::from("/data/io.github.ueda.irtransceiver/library.toml"))
        );
    }

    #[test]
    fn default_path_falls_back_to_home() {
        let expected = Some(PathBuf::from("/home/u/.local/share/io.github.ueda.irtransceiver/library.toml"));
        assert_eq!(default_path(env(&[("HOME", "/home/u")])), expected);
        // The XDG spec says relative values are invalid and must be ignored.
        assert_eq!(default_path(env(&[("XDG_DATA_HOME", "rel"), ("HOME", "/home/u")])), expected);
        assert_eq!(default_path(env(&[("XDG_DATA_HOME", ""), ("HOME", "/home/u")])), expected);
    }

    #[test]
    fn default_path_without_home_is_none() {
        assert_eq!(default_path(env(&[])), None);
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = temp_dir("roundtrip").join("sub/library.toml");
        let mut lib = Library::default();
        lib.add("TV", Payload::Nec { address: 0, command: 0x59, extended: false });
        save(&path, &lib).unwrap();
        let loaded = load(&path, 0).unwrap();
        assert_eq!(loaded.library, lib);
        assert!(loaded.recovered.is_none());
    }

    #[test]
    fn save_leaves_no_temporary_file() {
        let dir = temp_dir("no-temp");
        save(&dir.join("library.toml"), &Library::default()).unwrap();
        let names: Vec<_> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, vec!["library.toml"]);
    }

    #[test]
    fn missing_file_is_an_empty_library() {
        let loaded = load(&temp_dir("missing").join("library.toml"), 0).unwrap();
        assert_eq!(loaded.library, Library::default());
        assert!(loaded.recovered.is_none());
    }

    #[test]
    fn damaged_file_is_moved_aside() {
        let dir = temp_dir("damaged");
        let path = dir.join("library.toml");
        fs::write(&path, "this is not toml [").unwrap();
        let loaded = load(&path, 1234).unwrap();
        assert_eq!(loaded.library, Library::default());
        let (backup, reason) = loaded.recovered.expect("recovered");
        assert_eq!(backup, dir.join("library.toml.damaged-1234"));
        assert!(matches!(reason, LibraryError::Parse(_)));
        assert_eq!(fs::read_to_string(&backup).unwrap(), "this is not toml [");
        assert!(!path.exists());
    }

    #[test]
    fn newer_version_is_moved_aside_too() {
        let path = temp_dir("newer").join("library.toml");
        fs::write(&path, "version = 9\n").unwrap();
        let (_, reason) = load(&path, 5).unwrap().recovered.expect("recovered");
        assert_eq!(reason, LibraryError::UnsupportedVersion(9));
    }
}
