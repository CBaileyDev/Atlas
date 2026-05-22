//! Export IPC commands. Plan §10.
//!
//! The frontend picks a template and a selection (symbol id list), the
//! backend builds the Tera context, renders, and either returns the
//! string for live preview or writes a paired (`<file>`, `_atlas.json`)
//! to disk.

use std::path::PathBuf;

use atlas_core::export::{
    available_templates, build_context, load_template, render_to_string, AtlasSidecar, Selection,
    SelectionRules, TemplateInfo,
};
use atlas_core::storage::Db;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
pub struct ExportRequest {
    pub dump_id: i64,
    pub symbol_ids_hex: Vec<String>,
    pub template_name: String,
    pub project_name: String,
    pub trainer_class_name: String,
    pub process_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WriteResult {
    pub rendered_path: String,
    pub sidecar_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedSymbol {
    pub fqn: String,
    pub id_hex: Option<String>,
    pub kind_i: Option<i64>,
}

#[tauri::command]
pub async fn resolve_fqns(dump_id: i64, fqns: Vec<String>) -> AppResult<Vec<ResolvedSymbol>> {
    tokio::task::spawn_blocking(move || -> Result<_, AppError> {
        let db_path = atlas_core::paths::db_path().map_err(AppError::Atlas)?;
        let db = Db::open(&db_path).map_err(AppError::Atlas)?;
        let mut stmt = db
            .conn
            .prepare("SELECT id, kind FROM symbols WHERE dump_id = ? AND fqn = ? LIMIT 1")
            .map_err(|e| AppError::Internal(format!("prepare: {e}")))?;
        let mut out = Vec::with_capacity(fqns.len());
        for fqn in fqns {
            let row: Result<(Vec<u8>, i64), _> = stmt
                .query_row(rusqlite::params![dump_id, &fqn], |r| {
                    Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, i64>(1)?))
                });
            match row {
                Ok((bytes, kind)) => out.push(ResolvedSymbol {
                    fqn,
                    id_hex: Some(hex(&bytes)),
                    kind_i: Some(kind),
                }),
                Err(_) => out.push(ResolvedSymbol {
                    fqn,
                    id_hex: None,
                    kind_i: None,
                }),
            }
        }
        Ok(out)
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[tauri::command]
pub async fn list_templates() -> AppResult<Vec<TemplateInfo>> {
    tokio::task::spawn_blocking(|| -> Result<_, AppError> {
        available_templates().map_err(AppError::Atlas)
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

#[tauri::command]
pub async fn render_export_preview(req: ExportRequest) -> AppResult<String> {
    tokio::task::spawn_blocking(move || -> Result<_, AppError> {
        let db_path = atlas_core::paths::db_path().map_err(AppError::Atlas)?;
        let db = Db::open(&db_path).map_err(AppError::Atlas)?;
        let (src, _overridden) = load_template(&req.template_name).map_err(AppError::Atlas)?;
        let ctx = build_context(
            &db,
            req.dump_id,
            &req.symbol_ids_hex,
            &req.project_name,
            &req.trainer_class_name,
            &req.process_name,
        )
        .map_err(AppError::Atlas)?;
        render_to_string(&req.template_name, &src, ctx).map_err(AppError::Atlas)
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

#[derive(Debug, Clone, Deserialize)]
pub struct WriteRequest {
    #[serde(flatten)]
    pub req: ExportRequest,
    pub dest_dir: String,
    pub output_filename: String,
}

#[tauri::command]
pub async fn write_export(req: WriteRequest) -> AppResult<WriteResult> {
    tokio::task::spawn_blocking(move || -> Result<_, AppError> {
        let dest_dir = PathBuf::from(&req.dest_dir);
        if !dest_dir.is_dir() {
            return Err(AppError::Internal(format!(
                "dest_dir is not a directory: {}",
                req.dest_dir
            )));
        }

        let db_path = atlas_core::paths::db_path().map_err(AppError::Atlas)?;
        let db = Db::open(&db_path).map_err(AppError::Atlas)?;
        let (src, _overridden) = load_template(&req.req.template_name).map_err(AppError::Atlas)?;
        let ctx = build_context(
            &db,
            req.req.dump_id,
            &req.req.symbol_ids_hex,
            &req.req.project_name,
            &req.req.trainer_class_name,
            &req.req.process_name,
        )
        .map_err(AppError::Atlas)?;
        let rendered =
            render_to_string(&req.req.template_name, &src, ctx).map_err(AppError::Atlas)?;

        let (game_id, game_version) = db
            .conn
            .query_row(
                "SELECT game_id, game_version FROM dumps WHERE id = ?",
                [req.req.dump_id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .map_err(|e| AppError::Internal(format!("dump lookup: {e}")))?;

        let sel = Selection {
            symbol_ids_hex: req.req.symbol_ids_hex.clone(),
            rules: SelectionRules::default(),
        };
        let sidecar = AtlasSidecar::build(
            env!("CARGO_PKG_VERSION"),
            game_id,
            game_version,
            req.req.dump_id,
            &req.req.template_name,
            &src,
            &sel,
        );

        let rendered_path = dest_dir.join(&req.output_filename);
        std::fs::write(&rendered_path, rendered)
            .map_err(|e| AppError::Internal(format!("write {}: {e}", rendered_path.display())))?;

        let sidecar_path = dest_dir.join("_atlas.json");
        let sidecar_json = serde_json::to_string_pretty(&sidecar)
            .map_err(|e| AppError::Internal(format!("sidecar json: {e}")))?;
        std::fs::write(&sidecar_path, sidecar_json)
            .map_err(|e| AppError::Internal(format!("write {}: {e}", sidecar_path.display())))?;

        Ok(WriteResult {
            rendered_path: rendered_path.to_string_lossy().into_owned(),
            sidecar_path: sidecar_path.to_string_lossy().into_owned(),
        })
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}
