//! Crate-wide error type. See plan §12.4.
//!
//! `AtlasError` is intentionally string-shaped so it survives serde
//! serialization across the Tauri IPC boundary. We hand-curate messages
//! with `map_err` rather than using `#[from]` everywhere so that error
//! messages stay readable for end users.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(tag = "kind", content = "message")]
pub enum AtlasError {
    #[error("parser: {0}")]
    Parser(String),

    #[error("storage: {0}")]
    Storage(String),

    #[error("search: {0}")]
    Search(String),

    #[error("diff: {0}")]
    Diff(String),

    #[error("export: {0}")]
    Export(String),

    #[error("io: {0}")]
    Io(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("not found: {0}")]
    NotFound(String),
}

pub type AtlasResult<T> = Result<T, AtlasError>;

impl From<std::io::Error> for AtlasError {
    fn from(e: std::io::Error) -> Self {
        AtlasError::Io(e.to_string())
    }
}

impl From<rusqlite::Error> for AtlasError {
    fn from(e: rusqlite::Error) -> Self {
        AtlasError::Storage(e.to_string())
    }
}

impl From<refinery::Error> for AtlasError {
    fn from(e: refinery::Error) -> Self {
        AtlasError::Storage(format!("migration: {e}"))
    }
}

impl From<serde_json::Error> for AtlasError {
    fn from(e: serde_json::Error) -> Self {
        AtlasError::Storage(format!("json: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_includes_category() {
        let e = AtlasError::Storage("constraint violated".into());
        assert_eq!(e.to_string(), "storage: constraint violated");
    }

    #[test]
    fn serializes_with_tag() {
        let e = AtlasError::InvalidInput("path missing".into());
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("\"kind\""));
        assert!(json.contains("\"InvalidInput\""));
        assert!(json.contains("path missing"));
    }

    #[test]
    fn roundtrips_through_json() {
        let e = AtlasError::Diff("threshold not met".into());
        let json = serde_json::to_string(&e).unwrap();
        let back: AtlasError = serde_json::from_str(&json).unwrap();
        assert_eq!(e, back);
    }
}
