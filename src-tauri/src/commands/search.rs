//! Search and dump-listing IPC commands. Plan §8.
//!
//! The frontend never opens the SQLite file or the tantivy index
//! directly; everything comes through these commands.

use std::path::PathBuf;
use std::sync::Mutex;

use atlas_core::search::{DumpIndex, SearchFacets, SearchResult, SymbolRow};
use atlas_core::storage::Db;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DumpListItem {
    pub id: i64,
    pub game_id: String,
    pub game_version: String,
    pub parser: String,
    pub symbol_count: i64,
    pub ingested_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenDumpInfo {
    pub id: i64,
    pub game_id: String,
    pub game_version: String,
    pub symbol_count: i64,
    pub modules: Vec<String>,
}

#[tauri::command]
pub async fn list_dumps() -> AppResult<Vec<DumpListItem>> {
    tokio::task::spawn_blocking(|| -> Result<_, AppError> {
        let db_path = atlas_core::paths::db_path().map_err(AppError::Atlas)?;
        let db = Db::open(&db_path).map_err(AppError::Atlas)?;
        let mut stmt = db
            .conn
            .prepare(
                "SELECT id, game_id, game_version, parser, symbol_count, ingested_at
                 FROM dumps ORDER BY ingested_at DESC",
            )
            .map_err(|e| AppError::Internal(format!("prepare: {e}")))?;
        let rows = stmt
            .query_map([], |r| {
                Ok(DumpListItem {
                    id: r.get(0)?,
                    game_id: r.get(1)?,
                    game_version: r.get(2)?,
                    parser: r.get(3)?,
                    symbol_count: r.get(4)?,
                    ingested_at: r.get(5)?,
                })
            })
            .map_err(|e| AppError::Internal(format!("query: {e}")))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| AppError::Internal(format!("row: {e}")))?);
        }
        Ok(out)
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

#[tauri::command]
pub async fn open_dump(dump_id: i64) -> AppResult<OpenDumpInfo> {
    tokio::task::spawn_blocking(move || -> Result<_, AppError> {
        let db_path = atlas_core::paths::db_path().map_err(AppError::Atlas)?;
        let db = Db::open(&db_path).map_err(AppError::Atlas)?;

        let (game_id, game_version, symbol_count) = db
            .conn
            .query_row(
                "SELECT game_id, game_version, symbol_count FROM dumps WHERE id = ?",
                [dump_id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                },
            )
            .map_err(|e| AppError::Internal(format!("dump lookup: {e}")))?;

        let mut stmt = db
            .conn
            .prepare("SELECT DISTINCT module FROM symbols WHERE dump_id = ? ORDER BY module")
            .map_err(|e| AppError::Internal(format!("prepare: {e}")))?;
        let rows = stmt
            .query_map([dump_id], |r| r.get::<_, String>(0))
            .map_err(|e| AppError::Internal(format!("query: {e}")))?;
        let mut modules = Vec::new();
        for r in rows {
            modules.push(r.map_err(|e| AppError::Internal(format!("row: {e}")))?);
        }

        // Force the index to exist; rebuild if schema version is off.
        let index_root = atlas_core::paths::index_dir().map_err(AppError::Atlas)?;
        let _ = DumpIndex::open_or_build(&db, dump_id, &index_root).map_err(AppError::Atlas)?;

        Ok(OpenDumpInfo {
            id: dump_id,
            game_id,
            game_version,
            symbol_count,
            modules,
        })
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

#[tauri::command]
pub async fn search_symbols(
    dump_id: i64,
    query: String,
    kinds: Vec<i64>,
    modules: Vec<String>,
    limit: Option<u32>,
) -> AppResult<SearchResult> {
    let limit = limit.unwrap_or(100).clamp(1, 1000) as usize;
    tokio::task::spawn_blocking(move || -> Result<_, AppError> {
        let db_path = atlas_core::paths::db_path().map_err(AppError::Atlas)?;
        let db = Db::open(&db_path).map_err(AppError::Atlas)?;
        let index_root = atlas_core::paths::index_dir().map_err(AppError::Atlas)?;
        let idx = DumpIndex::open_or_build(&db, dump_id, &index_root).map_err(AppError::Atlas)?;
        let facets = SearchFacets { kinds, modules };
        idx.query(&query, &facets, limit).map_err(AppError::Atlas)
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

#[tauri::command]
pub async fn get_symbol(id_hex: String) -> AppResult<Option<SymbolRow>> {
    let id_bytes = decode_hex(&id_hex)
        .ok_or_else(|| AppError::Internal(format!("invalid id_hex: {id_hex}")))?;
    tokio::task::spawn_blocking(move || -> Result<_, AppError> {
        let db_path = atlas_core::paths::db_path().map_err(AppError::Atlas)?;
        let db = Db::open(&db_path).map_err(AppError::Atlas)?;
        atlas_core::search::lookup_symbol(&db, &id_bytes).map_err(AppError::Atlas)
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

#[tauri::command]
pub async fn list_members(class_id_hex: String) -> AppResult<Vec<SymbolRow>> {
    let id_bytes = decode_hex(&class_id_hex)
        .ok_or_else(|| AppError::Internal(format!("invalid id_hex: {class_id_hex}")))?;
    tokio::task::spawn_blocking(move || -> Result<_, AppError> {
        let db_path = atlas_core::paths::db_path().map_err(AppError::Atlas)?;
        let db = Db::open(&db_path).map_err(AppError::Atlas)?;
        let mut stmt = db
            .conn
            .prepare(
                "SELECT s.id, s.dump_id, s.fqn, s.name, s.kind, s.module,
                        s.size, s.align, s.offset, s.vtable_slot,
                        s.type_ref_json, s.flags, s.source_file, s.source_line
                 FROM relations r
                 JOIN symbols s ON s.id = r.to_symbol
                 WHERE r.from_symbol = ? AND r.kind = 1
                 ORDER BY s.offset ASC NULLS LAST, s.fqn",
            )
            .map_err(|e| AppError::Internal(format!("prepare: {e}")))?;
        let rows = stmt
            .query_map([id_bytes.as_slice()], |r| {
                Ok(SymbolRow {
                    id: r.get(0)?,
                    dump_id: r.get(1)?,
                    fqn: r.get(2)?,
                    name: r.get(3)?,
                    kind: r.get(4)?,
                    module: r.get(5)?,
                    size: r.get(6)?,
                    align: r.get(7)?,
                    offset: r.get(8)?,
                    vtable_slot: r.get(9)?,
                    type_ref_json: r.get(10)?,
                    flags: r.get(11)?,
                    source_file: r.get(12)?,
                    source_line: r.get(13)?,
                })
            })
            .map_err(|e| AppError::Internal(format!("query: {e}")))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| AppError::Internal(format!("row: {e}")))?);
        }
        Ok(out)
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() != 32 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = Vec::with_capacity(16);
    let mut iter = s.as_bytes().chunks_exact(2);
    for ch in &mut iter {
        let hi = nibble(ch[0])?;
        let lo = nibble(ch[1])?;
        out.push((hi << 4) | lo);
    }
    Some(out)
}

fn nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

// Silences "field never used" — kept for future single-connection
// reuse rather than re-opening per command.
#[allow(dead_code)]
struct DbCell {
    inner: Mutex<Option<Db>>,
    path: PathBuf,
}
