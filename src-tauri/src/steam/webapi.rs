//! Steam Web API: owned games (including uninstalled ones).

use serde::{Deserialize, Serialize};
use std::fmt;

const OWNED_GAMES_URL: &str = "https://api.steampowered.com/IPlayerService/GetOwnedGames/v1/";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OwnedGame {
    pub appid: u32,
    #[serde(default)]
    pub name: String,
    /// Minutes.
    #[serde(default)]
    pub playtime_forever: u32,
    #[serde(default)]
    pub rtime_last_played: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WebApiError {
    BadKey,
    /// The call worked but returned no games: private profile or wrong account.
    NoGames,
    Network(String),
    Http(u16),
    Parse(String),
}

impl fmt::Display for WebApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WebApiError::BadKey => write!(f, "Steam rejected the Web API key. Check it in Settings."),
            WebApiError::NoGames => write!(
                f,
                "Steam returned no games for this account. Set your profile's \"Game details\" privacy to Public, or check the account and API key in Settings."
            ),
            WebApiError::Network(e) => write!(f, "Couldn't reach Steam to list your games ({e})."),
            WebApiError::Http(code) => write!(f, "Steam's API answered with an error (HTTP {code}). Try again later."),
            WebApiError::Parse(e) => write!(f, "Steam's API sent something unexpected ({e})."),
        }
    }
}

impl std::error::Error for WebApiError {}

#[derive(Deserialize)]
struct Envelope {
    response: OwnedResponse,
}

#[derive(Deserialize)]
struct OwnedResponse {
    #[serde(default)]
    games: Vec<OwnedGame>,
}

pub fn parse_owned_games(json: &str) -> Result<Vec<OwnedGame>, WebApiError> {
    let env: Envelope = serde_json::from_str(json).map_err(|e| WebApiError::Parse(e.to_string()))?;
    if env.response.games.is_empty() {
        return Err(WebApiError::NoGames);
    }
    Ok(env.response.games)
}

pub async fn fetch_owned_games(
    client: &reqwest::Client,
    key: &str,
    steam_id: &str,
) -> Result<Vec<OwnedGame>, WebApiError> {
    let url = reqwest::Url::parse_with_params(
        OWNED_GAMES_URL,
        &[
            ("key", key),
            ("steamid", steam_id),
            ("include_appinfo", "1"),
            ("include_played_free_games", "1"),
            ("format", "json"),
        ],
    )
    .map_err(|e| WebApiError::Parse(e.to_string()))?;
    let resp = client
        .get(url)
        .send()
        .await
        // without_url: never leak the key into error messages.
        .map_err(|e| WebApiError::Network(e.without_url().to_string()))?;
    match resp.status().as_u16() {
        200 => {}
        401 | 403 => return Err(WebApiError::BadKey),
        code => return Err(WebApiError::Http(code)),
    }
    let text = resp
        .text()
        .await
        .map_err(|e| WebApiError::Network(e.without_url().to_string()))?;
    parse_owned_games(&text)
}
