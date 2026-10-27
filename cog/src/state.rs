//! The time of the last sync, kept between runs.

use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::{Error, IoContext, Result};
use crate::fsutil::write_atomic;

/// Environment variable that overrides the state file location.
pub const STATE_ENV: &str = "COG_INFO_PATH";

/// Same layout as the Python version's `info.toml`, so either can be used.
#[derive(Debug, Default, Deserialize, Serialize)]
struct StateFile {
    #[serde(default)]
    last_sync: f64,
}

/// Path from `COG_INFO_PATH`, else `<state dir>/cog/state.toml`.
pub fn default_path() -> PathBuf {
    match env::var_os(STATE_ENV) {
        Some(path) => PathBuf::from(path),
        None => dirs::state_dir()
            .or_else(dirs::data_local_dir)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("cog/state.toml"),
    }
}

/// Reads the last sync time, or the Unix epoch if there is no state file.
pub fn read_last_sync(path: &Path) -> Result<SystemTime> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(UNIX_EPOCH),
        Err(e) => return Err(e).ctx("read state file", path),
    };
    let state: StateFile = toml::from_str(&contents).map_err(|source| Error::State {
        path: path.to_owned(),
        source,
    })?;
    // The value is all the error needs to say what's wrong with it
    Duration::try_from_secs_f64(state.last_sync)
        .ok()
        .and_then(|since| UNIX_EPOCH.checked_add(since))
        .ok_or_else(|| Error::LastSync {
            path: path.to_owned(),
            value: state.last_sync,
        })
}

/// Records `time` as the last sync time.
pub fn write_last_sync(path: &Path, time: SystemTime) -> Result<()> {
    let last_sync = time
        .duration_since(UNIX_EPOCH)
        .map_err(Error::Clock)?
        .as_secs_f64();
    let contents = toml::to_string(&StateFile { last_sync }).map_err(Error::StateEncode)?;
    write_atomic(path, contents.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_epoch() {
        let dir = tempfile::tempdir().unwrap();
        let time = read_last_sync(&dir.path().join("none.toml")).unwrap();
        assert_eq!(time, UNIX_EPOCH);
    }

    #[test]
    fn reads_python_info_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("info.toml");
        fs::write(&path, "last_sync = 1790753828.5\n").unwrap();
        let time = read_last_sync(&path).unwrap();
        assert_eq!(time, UNIX_EPOCH + Duration::from_secs_f64(1790753828.5));
    }

    #[test]
    fn rejects_times_that_are_not_valid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("info.toml");
        for value in ["inf", "nan", "-1.0", "1e300"] {
            fs::write(&path, format!("last_sync = {value}\n")).unwrap();
            assert!(
                matches!(read_last_sync(&path), Err(Error::LastSync { .. })),
                "{value}"
            );
        }
    }

    #[test]
    fn refuses_to_record_times_before_1970() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("info.toml");
        let time = UNIX_EPOCH.checked_sub(Duration::from_secs(1)).unwrap();
        assert!(matches!(write_last_sync(&path, time), Err(Error::Clock(_))));
        assert!(!path.exists());
    }

    #[test]
    fn round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cog/state.toml");
        let time = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        write_last_sync(&path, time).unwrap();
        assert_eq!(read_last_sync(&path).unwrap(), time);
    }
}
