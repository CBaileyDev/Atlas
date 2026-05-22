//! Ingest a `SymbolGraph` into SQLite.
//!
//! Identity rules (plan §2 / ADR 0003):
//!
//! - Each symbol's persistent 16-byte id is
//!   `BLAKE3(fqn + ":" + kind_i64 + ":" + dump_id_i64)[..16]`.
//! - Re-ingesting the same dump (same `game_id`, `game_version`,
//!   `parser`) reuses the same `dump_id`, hashes again, and skips
//!   existing rows. Net result: idempotent.
//! - Relations carry both ends as the 16-byte ids. Local-id resolution
//!   happens here in the ingest step; the parser never sees a DB id.

use std::collections::HashMap;

use atlas_parser_trait::{Symbol, SymbolGraph};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::error::{AtlasError, AtlasResult};
use crate::storage::Db;

const ID_LEN: usize = 16;

/// Result of an ingest call. Returned to callers; serializable so it can
/// be handed straight back over the IPC boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestReport {
    pub dump_id: i64,
    pub game_id: String,
    pub game_version: String,
    pub parser: String,
    pub symbols_inserted: u64,
    pub symbols_skipped: u64,
    pub relations_inserted: u64,
    pub relations_skipped: u64,
    pub warnings: Vec<String>,
}

impl Db {
    /// Ingest a parsed graph. Idempotent: re-ingesting the same dump
    /// (same `game_id` + `game_version` + `parser`) updates the
    /// existing dump's `ingested_at` and `symbol_count`, then INSERT
    /// OR IGNOREs each symbol/relation row.
    pub fn ingest(&mut self, graph: &SymbolGraph) -> AtlasResult<IngestReport> {
        let tx = self.conn.transaction()?;

        // 1) Upsert the dumps row.
        let dump_id = upsert_dump(&tx, graph)?;

        // 2) Compute persistent ids for every symbol and write them.
        let mut id_map: HashMap<u32, [u8; ID_LEN]> = HashMap::with_capacity(graph.symbols.len());
        let mut symbols_inserted = 0u64;
        let mut symbols_skipped = 0u64;
        let mut warnings: Vec<String> = Vec::new();

        {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO symbols (
                    id, dump_id, fqn, name, kind, module,
                    size, align, offset, vtable_slot,
                    type_ref_json, flags, source_file, source_line
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )?;

            for sym in &graph.symbols {
                let id = symbol_id(dump_id, sym);
                id_map.insert(sym.local_id, id);

                let type_ref_json = sym
                    .type_ref
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()
                    .map_err(|e| AtlasError::Storage(format!("type_ref json: {e}")))?;
                let (source_file, source_line) = sym
                    .source_loc
                    .as_ref()
                    .map(|sl| (Some(sl.file.clone()), Some(sl.line)))
                    .unwrap_or((None, None));

                let changed = stmt.execute(params![
                    &id[..],
                    dump_id,
                    &sym.fqn,
                    &sym.name,
                    sym.kind.as_i64(),
                    &sym.module,
                    sym.size,
                    sym.align,
                    sym.offset,
                    sym.vtable_slot,
                    type_ref_json,
                    sym.flags.to_packed(),
                    source_file,
                    source_line,
                ])?;
                if changed == 1 {
                    symbols_inserted += 1;
                } else {
                    symbols_skipped += 1;
                }
            }
        }

        // 3) Write relations using the id map.
        let mut relations_inserted = 0u64;
        let mut relations_skipped = 0u64;
        {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO relations (from_symbol, to_symbol, kind)
                 VALUES (?, ?, ?)",
            )?;
            for rel in &graph.relations {
                let Some(from) = id_map.get(&rel.from) else {
                    warnings.push(format!(
                        "relation skipped: from local_id {} not found in graph",
                        rel.from
                    ));
                    continue;
                };
                let Some(to) = id_map.get(&rel.to) else {
                    warnings.push(format!(
                        "relation skipped: to local_id {} not found in graph",
                        rel.to
                    ));
                    continue;
                };
                let changed = stmt.execute(params![&from[..], &to[..], rel.kind.as_i64()])?;
                if changed == 1 {
                    relations_inserted += 1;
                } else {
                    relations_skipped += 1;
                }
            }
        }

        // 4) Final symbol_count update.
        tx.execute(
            "UPDATE dumps SET symbol_count = ?, ingested_at = ? WHERE id = ?",
            params![
                graph.symbols.len() as i64,
                graph.source.ingested_at.to_rfc3339(),
                dump_id,
            ],
        )?;

        tx.commit()?;

        Ok(IngestReport {
            dump_id,
            game_id: graph.source.game_id.clone(),
            game_version: graph.source.game_version.clone(),
            parser: graph.source.parser.clone(),
            symbols_inserted,
            symbols_skipped,
            relations_inserted,
            relations_skipped,
            warnings,
        })
    }
}

/// Compute the persistent 16-byte id for a symbol. The hash includes
/// `dump_id` so two dumps that both have `TinyGame.AActor` end up with
/// distinct rows (plan ADR 0003).
fn symbol_id(dump_id: i64, sym: &Symbol) -> [u8; ID_LEN] {
    let mut h = blake3::Hasher::new();
    h.update(sym.fqn.as_bytes());
    h.update(b":");
    h.update(&sym.kind.as_i64().to_le_bytes());
    h.update(b":");
    h.update(&dump_id.to_le_bytes());
    let full = h.finalize();
    let mut out = [0u8; ID_LEN];
    out.copy_from_slice(&full.as_bytes()[..ID_LEN]);
    out
}

/// INSERT-or-resolve the dumps row. Returns the dump id either way.
fn upsert_dump(tx: &rusqlite::Transaction<'_>, graph: &SymbolGraph) -> AtlasResult<i64> {
    let game_id = &graph.source.game_id;
    let game_version = &graph.source.game_version;
    let parser = &graph.source.parser;
    let parser_version = &graph.source.parser_version;
    let sdk_root = graph.source.sdk_root.to_string_lossy().into_owned();
    let ingested_at = graph.source.ingested_at.to_rfc3339();
    let symbol_count = graph.symbols.len() as i64;

    // First try INSERT; if it conflicts on the unique constraint, look up
    // and update the existing row's ingested_at + symbol_count.
    let changed = tx.execute(
        "INSERT INTO dumps (game_id, game_version, parser, parser_version,
                            sdk_root, ingested_at, symbol_count)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(game_id, game_version, parser) DO UPDATE
            SET parser_version = excluded.parser_version,
                sdk_root       = excluded.sdk_root,
                ingested_at    = excluded.ingested_at,
                symbol_count   = excluded.symbol_count",
        params![
            game_id,
            game_version,
            parser,
            parser_version,
            sdk_root,
            ingested_at,
            symbol_count,
        ],
    )?;
    if changed == 0 {
        // Nothing inserted or updated — shouldn't happen given ON CONFLICT
        // DO UPDATE, but defensive.
        return Err(AtlasError::Storage("dumps upsert produced no row".into()));
    }

    let id = tx.query_row(
        "SELECT id FROM dumps WHERE game_id = ? AND game_version = ? AND parser = ?",
        params![game_id, game_version, parser],
        |r| r.get::<_, i64>(0),
    )?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use atlas_parser_trait::{NullReporter, SdkParser};
    use atlas_parser_ue::Dumper7Parser;
    use std::path::PathBuf;

    use super::*;

    fn workspace_root() -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.pop(); // crates/
        p.pop(); // workspace root
        p
    }

    fn fixture(name: &str) -> PathBuf {
        workspace_root()
            .join("fixtures")
            .join("synthetic")
            .join(name)
    }

    fn parse_v1() -> SymbolGraph {
        let parser = Dumper7Parser::new();
        parser
            .parse(&fixture("tiny-game-v1"), &NullReporter)
            .unwrap()
    }

    fn parse_v2() -> SymbolGraph {
        let parser = Dumper7Parser::new();
        parser
            .parse(&fixture("tiny-game-v2"), &NullReporter)
            .unwrap()
    }

    #[test]
    fn ingest_writes_symbols_and_relations() {
        let mut db = Db::open_in_memory().unwrap();
        let g = parse_v1();
        let r = db.ingest(&g).unwrap();
        assert!(r.symbols_inserted > 0);
        assert!(r.relations_inserted > 0);
        assert_eq!(r.symbols_skipped, 0);

        // Confirm the dump row matches.
        let count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM symbols WHERE dump_id = ?",
                [r.dump_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count as u64, r.symbols_inserted);
    }

    #[test]
    fn ingest_is_idempotent() {
        let mut db = Db::open_in_memory().unwrap();
        let g = parse_v1();
        let r1 = db.ingest(&g).unwrap();
        let r2 = db.ingest(&g).unwrap();
        assert_eq!(r1.dump_id, r2.dump_id);
        assert_eq!(r2.symbols_inserted, 0, "second ingest inserts nothing");
        assert_eq!(r2.symbols_skipped, r1.symbols_inserted);
    }

    #[test]
    fn two_versions_yield_two_dumps_and_disjoint_symbol_rows() {
        let mut db = Db::open_in_memory().unwrap();
        let v1 = parse_v1();
        let v2 = parse_v2();
        let r1 = db.ingest(&v1).unwrap();
        let r2 = db.ingest(&v2).unwrap();
        assert_ne!(r1.dump_id, r2.dump_id);

        // Same FQN appears in both dumps as different rows.
        let count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM symbols WHERE fqn = 'TinyGame.AActor'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2, "AActor exists once per dump");
    }
}
