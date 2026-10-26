use crate::vdf::{self, ParseError};
use serde::Serialize;

/// SteamID64 of account ID 0; subtracting gives the 32-bit ID used for `userdata\<id>`.
pub const STEAMID64_BASE: u64 = 76561197960265728;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SteamAccount {
    /// SteamID64 as a string: it doesn't fit in a JS number.
    pub steam_id: String,
    pub account_id: u32,
    pub persona_name: String,
    pub account_name: String,
    pub most_recent: bool,
    pub timestamp: u64,
}

pub fn parse_login_users(text: &str) -> Result<Vec<SteamAccount>, ParseError> {
    let doc = vdf::parse(text)?;
    let Some(users) = doc.get_obj("users") else {
        return Ok(Vec::new());
    };
    Ok(users
        .iter()
        .filter_map(|(id, v)| {
            let id64: u64 = id.trim().parse().ok()?;
            let account_id = u32::try_from(id64.checked_sub(STEAMID64_BASE)?).ok()?;
            let user = v.as_obj()?;
            let account_name = user.get_str("AccountName").unwrap_or_default().to_string();
            let persona_name = user
                .get_str("PersonaName")
                .filter(|s| !s.is_empty())
                .unwrap_or(&account_name)
                .to_string();
            Some(SteamAccount {
                steam_id: id64.to_string(),
                account_id,
                persona_name,
                account_name,
                most_recent: user.get_str("MostRecent").is_some_and(|s| s.trim() == "1"),
                timestamp: user.get_str("Timestamp").and_then(|s| s.trim().parse().ok()).unwrap_or(0),
            })
        })
        .collect())
}

/// The preferred account if present, else MostRecent, else the latest login.
pub fn pick_account<'a>(accounts: &'a [SteamAccount], preferred: Option<&str>) -> Option<&'a SteamAccount> {
    preferred
        .and_then(|id| accounts.iter().find(|a| a.steam_id == id))
        .or_else(|| accounts.iter().find(|a| a.most_recent))
        .or_else(|| accounts.iter().max_by_key(|a| a.timestamp))
}
