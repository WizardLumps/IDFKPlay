//! JSON files in the config dir: missing or corrupt reads yield `None`, writes are atomic.

use serde::{de::DeserializeOwned, Serialize};
use std::fs;
use std::path::Path;

pub fn read<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let text = fs::read_to_string(path).ok()?;
    match serde_json::from_str(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            eprintln!("{} is unreadable ({e}); ignoring it", path.display());
            None
        }
    }
}

/// Writes via temp file + rename so a crash mid-write can't leave a half-written file.
pub fn write<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(value).map_err(std::io::Error::other)?;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, path)
}
