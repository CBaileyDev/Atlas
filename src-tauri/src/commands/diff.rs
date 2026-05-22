//! Diff IPC command. Pure Rust diff engine wraps two dumps re-read
//! from SQLite into `SymbolGraph`s.

use atlas_core::diff::{diff, Diff, DiffConfig, RenameOverride};
use atlas_core::storage::Db;
use atlas_parser_trait::{
    Relation, RelationKind, SourceMeta, Symbol, SymbolFlags, SymbolGraph, SymbolKind, TypeRef,
};
use chrono::Utc;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::{AppError, AppResult};

#[tauri::command]
pub async fn diff_dumps(base_dump_id: i64, head_dump_id: i64) -> AppResult<Diff> {
    tokio::task::spawn_blocking(move || -> Result<_, AppError> {
        let db_path = atlas_core::paths::db_path().map_err(AppError::Atlas)?;
        let db = Db::open(&db_path).map_err(AppError::Atlas)?;
        let base = load_graph(&db, base_dump_id)?;
        let head = load_graph(&db, head_dump_id)?;
        let d = diff(&base, &head, &DiffConfig::default(), &[]);
        Ok(d)
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

#[tauri::command]
pub async fn diff_dumps_with_overrides(
    base_dump_id: i64,
    head_dump_id: i64,
    overrides: Vec<RenameOverride>,
) -> AppResult<Diff> {
    tokio::task::spawn_blocking(move || -> Result<_, AppError> {
        let db_path = atlas_core::paths::db_path().map_err(AppError::Atlas)?;
        let db = Db::open(&db_path).map_err(AppError::Atlas)?;
        let base = load_graph(&db, base_dump_id)?;
        let head = load_graph(&db, head_dump_id)?;
        let d = diff(&base, &head, &DiffConfig::default(), &overrides);
        Ok(d)
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

/// Read a dump back into the parser-trait `SymbolGraph` shape. The diff
/// engine wants the in-memory graph; we reconstruct it from SQLite so
/// we don't have to re-parse the SDK every time someone runs a diff.
fn load_graph(db: &Db, dump_id: i64) -> Result<SymbolGraph, AppError> {
    let meta = db
        .conn
        .query_row(
            "SELECT game_id, game_version, parser, parser_version, sdk_root, ingested_at
             FROM dumps WHERE id = ?",
            [dump_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                ))
            },
        )
        .map_err(|e| AppError::Internal(format!("dump lookup: {e}")))?;

    // First pass: pull every symbol and assign it a fresh u32 local_id.
    let mut stmt = db
        .conn
        .prepare(
            "SELECT id, fqn, name, kind, module, size, align, offset,
                    vtable_slot, type_ref_json, flags, source_file, source_line
             FROM symbols WHERE dump_id = ? ORDER BY rowid",
        )
        .map_err(|e| AppError::Internal(format!("prepare symbols: {e}")))?;
    let rows = stmt
        .query_map([dump_id], |r| {
            Ok(SymbolRowRaw {
                id_bytes: r.get(0)?,
                fqn: r.get(1)?,
                name: r.get(2)?,
                kind: r.get(3)?,
                module: r.get(4)?,
                size: r.get(5)?,
                align: r.get(6)?,
                offset: r.get(7)?,
                vtable_slot: r.get(8)?,
                type_ref_json: r.get(9)?,
                flags: r.get(10)?,
                source_file: r.get(11)?,
                source_line: r.get(12)?,
            })
        })
        .map_err(|e| AppError::Internal(format!("symbols: {e}")))?;

    let mut symbols = Vec::new();
    let mut id_to_local: HashMap<Vec<u8>, u32> = HashMap::new();
    for (local_id_usize, r) in rows.enumerate() {
        let local_id: u32 = local_id_usize
            .try_into()
            .map_err(|_| AppError::Internal("dump has more than u32::MAX symbols".into()))?;
        let r = r.map_err(|e| AppError::Internal(format!("row: {e}")))?;
        let kind = SymbolKind::from_i64(r.kind).ok_or_else(|| {
            AppError::Internal(format!("invalid symbol kind {} for {}", r.kind, r.fqn))
        })?;
        let type_ref: Option<TypeRef> = match r.type_ref_json {
            Some(s) if !s.is_empty() => Some(
                serde_json::from_str(&s)
                    .map_err(|e| AppError::Internal(format!("type_ref json: {e}")))?,
            ),
            _ => None,
        };
        id_to_local.insert(r.id_bytes.clone(), local_id);
        symbols.push(Symbol {
            local_id,
            fqn: r.fqn,
            name: r.name,
            kind,
            module: r.module,
            size: r.size,
            align: r.align,
            offset: r.offset,
            vtable_slot: r.vtable_slot,
            type_ref,
            flags: SymbolFlags::from_packed(r.flags as u32),
            source_loc: match (r.source_file, r.source_line) {
                (Some(f), Some(l)) => Some(atlas_parser_trait::SourceLoc { file: f, line: l }),
                _ => None,
            },
        });
    }

    // Second pass: relations.
    let mut rel_stmt = db
        .conn
        .prepare(
            "SELECT r.from_symbol, r.to_symbol, r.kind
             FROM relations r
             JOIN symbols sf ON sf.id = r.from_symbol AND sf.dump_id = ?
             JOIN symbols st ON st.id = r.to_symbol   AND st.dump_id = ?",
        )
        .map_err(|e| AppError::Internal(format!("prepare relations: {e}")))?;
    let rel_rows = rel_stmt
        .query_map([dump_id, dump_id], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })
        .map_err(|e| AppError::Internal(format!("relations: {e}")))?;
    let mut relations = Vec::new();
    for r in rel_rows {
        let (from, to, kind_i) = r.map_err(|e| AppError::Internal(format!("rel row: {e}")))?;
        let Some(&from_local) = id_to_local.get(&from) else {
            continue;
        };
        let Some(&to_local) = id_to_local.get(&to) else {
            continue;
        };
        let Some(kind) = RelationKind::from_i64(kind_i) else {
            continue;
        };
        relations.push(Relation {
            from: from_local,
            to: to_local,
            kind,
        });
    }

    Ok(SymbolGraph {
        source: SourceMeta {
            parser: meta.2,
            parser_version: meta.3,
            game_id: meta.0,
            game_version: meta.1,
            ingested_at: chrono::DateTime::parse_from_rfc3339(&meta.5)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            sdk_root: PathBuf::from(meta.4),
        },
        symbols,
        relations,
    })
}

struct SymbolRowRaw {
    id_bytes: Vec<u8>,
    fqn: String,
    name: String,
    kind: i64,
    module: String,
    size: Option<u32>,
    align: Option<u32>,
    offset: Option<u32>,
    vtable_slot: Option<u32>,
    type_ref_json: Option<String>,
    flags: i64,
    source_file: Option<String>,
    source_line: Option<u32>,
}
