//! `ping` IPC command — the Phase 0 sanity check.
//!
//! The frontend calls `invoke('ping', { message })`. On success the
//! backend returns a `PingResponse` containing `pong: "pong"` and a
//! UTC timestamp so the frontend can prove the call actually reached
//! the backend (rather than being satisfied by a cache or a stale
//! invoke result).

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingResponse {
    pub pong: String,
    pub echoed: Option<String>,
    pub timestamp: String,
    pub version: String,
}

#[tauri::command]
pub fn ping(message: Option<String>) -> AppResult<PingResponse> {
    tracing::debug!(?message, "ping received");
    Ok(PingResponse {
        pong: "pong".to_string(),
        echoed: message,
        timestamp: Utc::now().to_rfc3339(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_returns_pong_and_timestamp() {
        let r = ping(Some("hello".to_string())).unwrap();
        assert_eq!(r.pong, "pong");
        assert_eq!(r.echoed.as_deref(), Some("hello"));
        assert!(!r.timestamp.is_empty());
        assert!(!r.version.is_empty());
    }

    #[test]
    fn ping_handles_empty_message() {
        let r = ping(None).unwrap();
        assert_eq!(r.pong, "pong");
        assert!(r.echoed.is_none());
    }
}
