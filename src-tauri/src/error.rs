use std::fmt;
use std::path::PathBuf;

/// Errors that reach the UI. `Display` is the calm, specific message the user sees.
#[derive(Debug)]
pub enum AppError {
    SteamNotFound,
    SteamPathInvalid(PathBuf),
    NoLibraries(PathBuf),
    Internal(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::SteamNotFound => {
                write!(f, "Couldn't find Steam. Set its folder in Settings.")
            }
            AppError::SteamPathInvalid(p) => write!(
                f,
                "The Steam folder in Settings ({}) doesn't look like a Steam install. Fix it or clear it to auto-detect.",
                p.display()
            ),
            AppError::NoLibraries(p) => write!(
                f,
                "Found Steam at {} but couldn't read any game libraries. Add a library folder in Settings.",
                p.display()
            ),
            AppError::Internal(msg) => write!(f, "Something went wrong: {msg}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<AppError> for String {
    fn from(e: AppError) -> Self {
        e.to_string()
    }
}
