//! IPC-facing error type.
//!
//! `atlas-core` defines `AtlasError`, which is already serde-friendly.
//! The Tauri shell uses `AppError` as the type returned from IPC
//! commands so we can layer shell-specific errors (e.g. Tauri runtime
//! errors) on top of the core ones.

use atlas_core::AtlasError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "kind", content = "message")]
pub enum AppError {
    #[error(transparent)]
    Atlas(#[from] AtlasError),

    #[error("tauri: {0}")]
    Tauri(String),

    #[error("internal: {0}")]
    Internal(String),
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        AppError::Tauri(e.to_string())
    }
}

/// Convenient alias for command return types.
pub type AppResult<T> = Result<T, AppError>;
