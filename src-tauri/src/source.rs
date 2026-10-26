//! Launcher-agnostic game model. Steam is the only source today; others
//! (Xbox, Epic, GOG) implement `GameSource` later.

use crate::error::AppError;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    /// Which `GameSource` produced this game, e.g. "steam".
    pub source: &'static str,
    pub app_id: u32,
    pub name: String,
    pub installed: bool,
    pub install_path: Option<String>,
    pub size_on_disk: Option<u64>,
    pub playtime_minutes: u32,
    /// Unix seconds, when known.
    pub last_played: Option<u64>,
}

#[derive(Debug, Default)]
pub struct SourceScan {
    pub games: Vec<Game>,
    /// Non-fatal problems worth telling the user about.
    pub warnings: Vec<String>,
}

pub trait GameSource {
    fn id(&self) -> &'static str;
    fn scan(&self) -> Result<SourceScan, AppError>;
}
