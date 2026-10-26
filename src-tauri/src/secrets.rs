//! Steam Web API key storage: Windows Credential Manager first, a file in the
//! config dir as fallback (e.g. if Credential Manager is unavailable).

use serde::Serialize;
use std::fs;
use std::path::Path;

const SERVICE: &str = "IDFKPlay";
/// Credential Manager service name used before the rename; migrated on first read.
const LEGACY_SERVICE: &str = "gamepicker";
const ACCOUNT: &str = "steam-web-api-key";
const FALLBACK_FILE: &str = "steam-api-key.txt";

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum KeyStorage {
    CredentialManager,
    File,
}

fn entry() -> keyring::Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, ACCOUNT)
}

/// Moves a key saved under the old service name to the current one.
fn migrate_legacy_key() -> Option<String> {
    let legacy = keyring::Entry::new(LEGACY_SERVICE, ACCOUNT).ok()?;
    let key = legacy.get_password().ok()?;
    if entry().and_then(|e| e.set_password(&key)).is_ok() {
        let _ = legacy.delete_credential();
    }
    Some(key)
}

pub fn load_api_key(config_dir: &Path) -> Option<String> {
    let from_vault = entry().and_then(|e| e.get_password()).ok().or_else(migrate_legacy_key);
    from_vault
        .or_else(|| fs::read_to_string(config_dir.join(FALLBACK_FILE)).ok())
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
}

/// Saves (or with `None`, removes) the key. Returns where it was stored.
pub fn store_api_key(config_dir: &Path, key: Option<&str>) -> Result<Option<KeyStorage>, String> {
    let fallback = config_dir.join(FALLBACK_FILE);
    let key = key.map(str::trim).filter(|k| !k.is_empty());

    let Some(key) = key else {
        if let Ok(e) = entry() {
            // NoEntry just means there was nothing to delete.
            match e.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(err) => return Err(format!("Couldn't remove the API key from Credential Manager: {err}")),
            }
        }
        if fallback.exists() {
            fs::remove_file(&fallback).map_err(|e| format!("Couldn't remove the saved API key: {e}"))?;
        }
        return Ok(None);
    };

    if !key.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("That doesn't look like a Steam Web API key (it should be 32 letters and numbers).".into());
    }

    match entry().and_then(|e| e.set_password(key)) {
        Ok(()) => {
            // Don't leave a stale copy behind in the fallback file.
            let _ = fs::remove_file(&fallback);
            Ok(Some(KeyStorage::CredentialManager))
        }
        Err(err) => {
            eprintln!("secrets: Credential Manager unavailable ({err}); using file fallback");
            fs::create_dir_all(config_dir).map_err(|e| e.to_string())?;
            fs::write(&fallback, key).map_err(|e| format!("Couldn't save the API key: {e}"))?;
            Ok(Some(KeyStorage::File))
        }
    }
}
