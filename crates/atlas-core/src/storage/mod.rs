//! SQLite storage layer.
//!
//! Phase 0 ships only:
//! - the connection bootstrap (`open` / `open_in_memory`)
//! - the migration runner (refinery, embedded at compile time)
//! - the schema-0001 set from plan §4.2
//!
//! Real read/write APIs (`ingest`, `list_dumps`, `get_symbol`, etc.) land
//! in Phase 1 and later.

pub mod ingest;
pub mod migrations;

pub use ingest::IngestReport;

use std::path::Path;

use rusqlite::Connection;

use crate::error::{AtlasError, AtlasResult};

/// Wrapper around a single SQLite connection. Atlas is single-user and
/// single-process, so we don't need a pool; a single connection with WAL
/// mode handles our concurrency comfortably.
pub struct Db {
    /// Public so the Tauri command layer in `src-tauri` can issue
    /// project-specific queries without having to wrap every shape
    /// in a method here. Plan §3 invariant 5 (UI never opens SQLite)
    /// still holds — the **frontend** stays behind the IPC boundary;
    /// this exposure is only visible to other Rust crates.
    pub conn: Connection,
}

impl Db {
    /// Open the on-disk database at the given path, run migrations, and
    /// apply the runtime PRAGMAs the plan calls for.
    pub fn open(path: impl AsRef<Path>) -> AtlasResult<Self> {
        let conn = Connection::open(path.as_ref())
            .map_err(|e| AtlasError::Storage(format!("open: {e}")))?;
        let mut db = Self { conn };
        db.apply_pragmas()?;
        db.migrate()?;
        Ok(db)
    }

    /// Open an in-memory database (used for tests).
    pub fn open_in_memory() -> AtlasResult<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| AtlasError::Storage(format!("open_in_memory: {e}")))?;
        let mut db = Self { conn };
        // WAL has no effect on :memory: but the other pragmas still apply.
        db.apply_pragmas()?;
        db.migrate()?;
        Ok(db)
    }

    fn apply_pragmas(&mut self) -> AtlasResult<()> {
        // Plan §7: WAL + synchronous=NORMAL after migrations.
        // For :memory:, WAL is a no-op but harmless.
        self.conn
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| AtlasError::Storage(format!("pragma journal_mode: {e}")))?;
        self.conn
            .pragma_update(None, "synchronous", "NORMAL")
            .map_err(|e| AtlasError::Storage(format!("pragma synchronous: {e}")))?;
        self.conn
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| AtlasError::Storage(format!("pragma foreign_keys: {e}")))?;
        // 64 MB cache.  Symbol joins benefit from a generous page cache.
        self.conn
            .pragma_update(None, "cache_size", -64_000_i64)
            .map_err(|e| AtlasError::Storage(format!("pragma cache_size: {e}")))?;
        self.conn
            .pragma_update(None, "temp_store", "MEMORY")
            .map_err(|e| AtlasError::Storage(format!("pragma temp_store: {e}")))?;
        Ok(())
    }

    fn migrate(&mut self) -> AtlasResult<()> {
        migrations::runner()
            .run(&mut self.conn)
            .map_err(|e| AtlasError::Storage(format!("migrate: {e}")))?;
        Ok(())
    }

    /// Borrow the underlying rusqlite connection. Inside-crate use only.
    /// Phase 1 will move query callers here; tagged `dead_code`-allowed
    /// while only tests use it.
    #[allow(dead_code)]
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    /// List the names of all tables defined by the user schema (excluding
    /// SQLite/refinery bookkeeping tables). Used in tests.
    pub fn user_tables(&self) -> AtlasResult<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT name FROM sqlite_master
                 WHERE type='table'
                   AND name NOT LIKE 'sqlite_%'
                   AND name NOT LIKE 'refinery_%'
                 ORDER BY name",
            )
            .map_err(|e| AtlasError::Storage(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| AtlasError::Storage(e.to_string()))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| AtlasError::Storage(e.to_string()))?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_runs_migrations_and_creates_all_phase0_tables() {
        let db = Db::open_in_memory().unwrap();
        let tables = db.user_tables().unwrap();
        let expected = [
            "dumps",
            "projects",
            "relations",
            "rename_overrides",
            "symbol_links",
            "symbols",
        ];
        for t in expected {
            assert!(
                tables.iter().any(|x| x == t),
                "expected table {t} in {tables:?}"
            );
        }
    }

    #[test]
    fn migration_is_idempotent_on_reopen() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("atlas.sqlite");
        {
            let _db = Db::open(&db_path).unwrap();
        }
        // Second open shouldn't error or re-run migrations destructively.
        let db = Db::open(&db_path).unwrap();
        let tables = db.user_tables().unwrap();
        assert!(tables.contains(&"symbols".to_string()));
    }

    #[test]
    fn foreign_keys_are_enabled() {
        let db = Db::open_in_memory().unwrap();
        let fk: i32 = db
            .conn()
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1, "foreign_keys pragma must be on");
    }
}
