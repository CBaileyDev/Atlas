//! Ingest a Dumper-7 SDK folder and write it to the application
//! database. Phase 1's only externally-visible operation.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use atlas_core::storage::{Db, IngestReport};
use atlas_parser_trait::{Reporter, SdkParser};
use atlas_parser_ue::Dumper7Parser;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::error::{AppError, AppResult};

/// Frontend-visible progress event.
#[derive(Debug, Clone, Serialize)]
struct ProgressEvent<'a> {
    current: u64,
    total: Option<u64>,
    label: &'a str,
}

/// Bridges the parser-trait `Reporter` to Tauri events. Each method
/// pushes an event onto the main window's channel. We don't keep a
/// strong reference back to the window — if the window has gone away
/// the emit just silently fails, which is fine for progress events.
struct TauriReporter {
    app: AppHandle,
    total: Mutex<Option<u64>>,
}

impl TauriReporter {
    fn new(app: AppHandle) -> Self {
        Self {
            app,
            total: Mutex::new(None),
        }
    }
}

impl Reporter for TauriReporter {
    fn started(&self, total_estimate: Option<u64>) {
        *self.total.lock().expect("total mutex poisoned") = total_estimate;
        let _ = self.app.emit(
            "ingest:started",
            ProgressEvent {
                current: 0,
                total: total_estimate,
                label: "starting",
            },
        );
    }

    fn progress(&self, current: u64, label: &str) {
        let total = *self.total.lock().expect("total mutex poisoned");
        let _ = self.app.emit(
            "ingest:progress",
            ProgressEvent {
                current,
                total,
                label,
            },
        );
    }

    fn warn(&self, message: &str) {
        let _ = self.app.emit("ingest:warn", message);
    }

    fn finished(&self) {
        let _ = self.app.emit("ingest:finished", ());
    }
}

/// Ingest the SDK folder at `path` and return the resulting report.
///
/// The command is offloaded to `tokio::task::spawn_blocking` because
/// both the parser and SQLite calls are synchronous.
#[tauri::command]
pub async fn ingest_dump(app: AppHandle, path: String) -> AppResult<IngestReport> {
    let root = PathBuf::from(&path);
    if !root.exists() {
        return Err(AppError::Internal(format!("path not found: {path}")));
    }

    let reporter: Arc<dyn Reporter> = Arc::new(TauriReporter::new(app));
    let parser = Dumper7Parser::new();

    if !parser.can_handle(&root) {
        return Err(AppError::Internal(format!(
            "no parser claims {path} (looked for SDK.hpp or SDKInfo.json)"
        )));
    }

    let reporter_for_thread = Arc::clone(&reporter);
    let root_for_thread = root.clone();
    let graph = tokio::task::spawn_blocking(move || -> Result<_, AppError> {
        parser
            .parse(&root_for_thread, reporter_for_thread.as_ref())
            .map_err(|e| AppError::Internal(format!("parse: {e}")))
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))??;

    let report = tokio::task::spawn_blocking(move || -> Result<_, AppError> {
        let db_path = atlas_core::paths::db_path().map_err(AppError::Atlas)?;
        let mut db = Db::open(&db_path).map_err(AppError::Atlas)?;
        db.ingest(&graph).map_err(AppError::Atlas)
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))??;

    Ok(report)
}
