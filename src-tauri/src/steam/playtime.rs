use crate::vdf::{self, ParseError};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LocalStats {
    pub playtime_minutes: u32,
    pub last_played: Option<u64>,
}

pub fn localconfig_path(steam: &Path, account_id: u32) -> PathBuf {
    steam
        .join("userdata")
        .join(account_id.to_string())
        .join("config")
        .join("localconfig.vdf")
}

/// Per-app stats from `UserLocalConfigStore > Software > Valve > Steam > apps`.
/// Steam's key casing varies ("apps"/"Apps", "Playtime"/"playtime"), so lookups ignore case.
pub fn parse_local_config(text: &str) -> Result<HashMap<u32, LocalStats>, ParseError> {
    let doc = vdf::parse(text)?;
    let Some(apps) = doc
        .get_path(&["UserLocalConfigStore", "Software", "Valve", "Steam"])
        .and_then(|steam| steam.get_obj("apps"))
    else {
        return Ok(HashMap::new());
    };

    Ok(apps
        .iter()
        .filter_map(|(id, v)| {
            let app_id = id.trim().parse().ok()?;
            let app = v.as_obj()?;
            let num = |key| app.get_str(key).and_then(|s| s.trim().parse::<u64>().ok());
            Some((
                app_id,
                LocalStats {
                    playtime_minutes: num("Playtime").map_or(0, |m| m.min(u32::MAX as u64) as u32),
                    last_played: num("LastPlayed").filter(|&t| t > 0),
                },
            ))
        })
        .collect())
}
