//! Settings IPC. Read / write the user's persistent `<data>/settings.json`.

use std::path::PathBuf;

use atlas_core::settings::AtlasSettings;

use crate::error::{AppError, AppResult};

#[tauri::command]
pub async fn get_settings() -> AppResult<AtlasSettings> {
    tokio::task::spawn_blocking(|| AtlasSettings::load().map_err(AppError::Atlas))
        .await
        .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

#[tauri::command]
pub async fn save_settings(settings: AtlasSettings) -> AppResult<()> {
    tokio::task::spawn_blocking(move || settings.save().map_err(AppError::Atlas))
        .await
        .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

#[tauri::command]
pub async fn add_watcher_root(root: String) -> AppResult<AtlasSettings> {
    tokio::task::spawn_blocking(move || -> Result<AtlasSettings, AppError> {
        let p = PathBuf::from(&root);
        if !p.is_dir() {
            return Err(AppError::Internal(format!("not a directory: {root}")));
        }
        let mut s = AtlasSettings::load().map_err(AppError::Atlas)?;
        if !s.watcher_roots.iter().any(|r| r == &p) {
            s.watcher_roots.push(p);
        }
        s.save().map_err(AppError::Atlas)?;
        Ok(s)
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

#[tauri::command]
pub async fn remove_watcher_root(root: String) -> AppResult<AtlasSettings> {
    tokio::task::spawn_blocking(move || -> Result<AtlasSettings, AppError> {
        let p = PathBuf::from(&root);
        let mut s = AtlasSettings::load().map_err(AppError::Atlas)?;
        s.watcher_roots.retain(|r| r != &p);
        s.save().map_err(AppError::Atlas)?;
        Ok(s)
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}
