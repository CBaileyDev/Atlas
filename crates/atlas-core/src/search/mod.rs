//! Tantivy-backed search per dump (plan §8).
//!
//! Indexes live under `<data_dir>/index/<dump_id>/`. The index is a
//! pure derivative of the SQLite tables; deleting it always recovers
//! by calling `Index::rebuild`. Schema-version mismatches trigger an
//! automatic rebuild.
//!
//! Phase 2 schema:
//! - `id` — 16-byte BLOB, STORED.
//! - `fqn` — tokenized + stored. Doubles as the primary search target.
//! - `name` — tokenized (short, identifier-only).
//! - `kind_i` — i64, FAST + INDEXED + STORED. Filter facet.
//! - `module` — string keyword, FAST + STORED. Filter facet.
//! - `parent_name` — tokenized (for "subclasses of X" queries).

use std::path::{Path, PathBuf};

use rusqlite::params;
use serde::{Deserialize, Serialize};
use tantivy::collector::{Count, TopDocs};
use tantivy::query::{BooleanQuery, Occur, Query, QueryParser, TermQuery};
use tantivy::schema::{
    Field, IndexRecordOption, Schema, Value, FAST, INDEXED, STORED, STRING, TEXT,
};
use tantivy::{doc, Index, IndexReader, IndexSettings, IndexWriter, ReloadPolicy, Term};

use crate::error::{AtlasError, AtlasResult};
use crate::storage::Db;

/// Bumped whenever the schema or analyzer chain changes. The reader
/// notices a mismatch and triggers a rebuild rather than reading a
/// stale index.
pub const SCHEMA_VERSION: u32 = 1;

const SCHEMA_FILE_NAME: &str = ".schema-version";

#[derive(Debug, Clone)]
pub struct SearchFields {
    pub id: Field,
    pub fqn: Field,
    pub name: Field,
    pub kind_i: Field,
    pub module: Field,
    pub parent_name: Field,
}

impl SearchFields {
    pub fn schema() -> (Schema, Self) {
        let mut b = Schema::builder();
        let id = b.add_bytes_field("id", STORED);
        let fqn = b.add_text_field("fqn", TEXT | STORED);
        let name = b.add_text_field("name", TEXT);
        let kind_i = b.add_i64_field("kind_i", FAST | INDEXED | STORED);
        let module = b.add_text_field("module", STRING | FAST | STORED);
        let parent_name = b.add_text_field("parent_name", TEXT);
        (
            b.build(),
            Self {
                id,
                fqn,
                name,
                kind_i,
                module,
                parent_name,
            },
        )
    }
}

/// User-facing search hit returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    /// 16-byte symbol id encoded as lowercase hex (32 chars).
    pub id_hex: String,
    pub fqn: String,
    pub kind_i: i64,
    pub module: String,
    pub score: f32,
}

/// Aggregated result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub query: String,
    pub total_matched: usize,
    pub hits: Vec<SearchHit>,
}

/// Filter dialect accepted by `Index::query`. All facets are
/// AND-combined; multiple values within one facet are OR-combined.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchFacets {
    pub kinds: Vec<i64>,
    pub modules: Vec<String>,
}

/// Wraps a tantivy `Index` plus the resolved schema fields.
pub struct DumpIndex {
    index: Index,
    fields: SearchFields,
    reader: IndexReader,
    dir: PathBuf,
}

impl DumpIndex {
    /// Open an existing index, rebuilding if the on-disk schema
    /// version doesn't match.
    pub fn open_or_build(db: &Db, dump_id: i64, index_root: &Path) -> AtlasResult<Self> {
        let dir = index_root.join(dump_id.to_string());
        let needs_rebuild = !is_up_to_date(&dir)?;
        if needs_rebuild {
            std::fs::remove_dir_all(&dir).ok();
            std::fs::create_dir_all(&dir)?;
            let idx = Self::build_inner(db, dump_id, &dir)?;
            write_schema_marker(&dir)?;
            return Ok(idx);
        }
        Self::open_existing(&dir)
    }

    /// Force a full rebuild of the index for a dump.
    pub fn rebuild(db: &Db, dump_id: i64, index_root: &Path) -> AtlasResult<Self> {
        let dir = index_root.join(dump_id.to_string());
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir)?;
        let idx = Self::build_inner(db, dump_id, &dir)?;
        write_schema_marker(&dir)?;
        Ok(idx)
    }

    fn open_existing(dir: &Path) -> AtlasResult<Self> {
        let index =
            Index::open_in_dir(dir).map_err(|e| AtlasError::Search(format!("open index: {e}")))?;
        let (_schema, fields) = SearchFields::schema();
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e| AtlasError::Search(format!("reader: {e}")))?;
        Ok(Self {
            index,
            fields,
            reader,
            dir: dir.to_path_buf(),
        })
    }

    fn build_inner(db: &Db, dump_id: i64, dir: &Path) -> AtlasResult<Self> {
        let (schema, fields) = SearchFields::schema();
        let index = Index::builder()
            .schema(schema)
            .settings(IndexSettings::default())
            .create_in_dir(dir)
            .map_err(|e| AtlasError::Search(format!("create index: {e}")))?;

        // 50 MB write buffer; tantivy recommends >=15 MB for batch
        // writes. Bigger means fewer segment merges during build.
        let mut writer: IndexWriter = index
            .writer(50 * 1024 * 1024)
            .map_err(|e| AtlasError::Search(format!("writer: {e}")))?;

        // Pull every symbol for this dump.
        let mut stmt = db
            .conn
            .prepare("SELECT id, fqn, name, kind, module FROM symbols WHERE dump_id = ?")
            .map_err(|e| AtlasError::Search(format!("prepare: {e}")))?;

        let rows = stmt
            .query_map([dump_id], |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })
            .map_err(|e| AtlasError::Search(format!("query: {e}")))?;

        for row in rows {
            let (id, fqn, name, kind, module) =
                row.map_err(|e| AtlasError::Search(format!("row: {e}")))?;
            writer
                .add_document(doc!(
                    fields.id => id,
                    fields.fqn => fqn,
                    fields.name => name,
                    fields.kind_i => kind,
                    fields.module => module,
                ))
                .map_err(|e| AtlasError::Search(format!("add_document: {e}")))?;
        }

        writer
            .commit()
            .map_err(|e| AtlasError::Search(format!("commit: {e}")))?;

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e| AtlasError::Search(format!("reader: {e}")))?;
        Ok(Self {
            index,
            fields,
            reader,
            dir: dir.to_path_buf(),
        })
    }

    /// Execute a free-text query plus optional facet filters and
    /// return a ranked, limited list of hits.
    pub fn query(
        &self,
        query: &str,
        facets: &SearchFacets,
        limit: usize,
    ) -> AtlasResult<SearchResult> {
        let searcher = self.reader.searcher();

        // Free-text component — searches both fqn and name fields.
        let parser = QueryParser::for_index(&self.index, vec![self.fields.fqn, self.fields.name]);
        let text_q: Box<dyn Query> = if query.trim().is_empty() {
            Box::new(tantivy::query::AllQuery)
        } else {
            parser
                .parse_query(query)
                .map_err(|e| AtlasError::Search(format!("parse_query: {e}")))?
        };

        // Facet component.
        let mut sub_queries: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        sub_queries.push((Occur::Must, text_q));

        if !facets.kinds.is_empty() {
            let mut kind_subs: Vec<(Occur, Box<dyn Query>)> = Vec::new();
            for k in &facets.kinds {
                let term = Term::from_field_i64(self.fields.kind_i, *k);
                kind_subs.push((
                    Occur::Should,
                    Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
                ));
            }
            sub_queries.push((Occur::Must, Box::new(BooleanQuery::new(kind_subs))));
        }

        if !facets.modules.is_empty() {
            let mut mod_subs: Vec<(Occur, Box<dyn Query>)> = Vec::new();
            for m in &facets.modules {
                let term = Term::from_field_text(self.fields.module, m);
                mod_subs.push((
                    Occur::Should,
                    Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
                ));
            }
            sub_queries.push((Occur::Must, Box::new(BooleanQuery::new(mod_subs))));
        }

        let combined: Box<dyn Query> = if sub_queries.len() == 1 {
            sub_queries.pop().expect("nonempty").1
        } else {
            Box::new(BooleanQuery::new(sub_queries))
        };

        let (top_docs, total_matched) = searcher
            .search(&combined, &(TopDocs::with_limit(limit), Count))
            .map_err(|e| AtlasError::Search(format!("search: {e}")))?;

        let mut hits = Vec::with_capacity(top_docs.len());
        for (score, doc_addr) in top_docs {
            let retrieved: tantivy::TantivyDocument = searcher
                .doc(doc_addr)
                .map_err(|e| AtlasError::Search(format!("doc fetch: {e}")))?;
            let id_bytes = retrieved
                .get_first(self.fields.id)
                .and_then(|v| v.as_bytes())
                .map(<[u8]>::to_vec)
                .unwrap_or_default();
            let fqn = retrieved
                .get_first(self.fields.fqn)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let kind_i = retrieved
                .get_first(self.fields.kind_i)
                .and_then(|v| v.as_i64())
                .unwrap_or(-1);
            let module = retrieved
                .get_first(self.fields.module)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            hits.push(SearchHit {
                id_hex: hex_encode(&id_bytes),
                fqn,
                kind_i,
                module,
                score,
            });
        }

        Ok(SearchResult {
            query: query.to_string(),
            total_matched,
            hits,
        })
    }

    pub fn index_dir(&self) -> &Path {
        &self.dir
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

fn is_up_to_date(dir: &Path) -> AtlasResult<bool> {
    if !dir.exists() {
        return Ok(false);
    }
    let marker = dir.join(SCHEMA_FILE_NAME);
    if !marker.exists() {
        return Ok(false);
    }
    let raw = std::fs::read_to_string(&marker)?;
    Ok(raw.trim() == SCHEMA_VERSION.to_string())
}

fn write_schema_marker(dir: &Path) -> AtlasResult<()> {
    let marker = dir.join(SCHEMA_FILE_NAME);
    std::fs::write(&marker, SCHEMA_VERSION.to_string())?;
    Ok(())
}

/// Convenience: look up a single symbol by its 16-byte id.
pub fn lookup_symbol(db: &Db, id: &[u8]) -> AtlasResult<Option<SymbolRow>> {
    let mut stmt = db.conn.prepare(
        "SELECT id, dump_id, fqn, name, kind, module, size, align, offset,
                vtable_slot, type_ref_json, flags, source_file, source_line
         FROM symbols WHERE id = ?",
    )?;
    let mut rows = stmt
        .query_map(params![id], |r| {
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
        .map_err(|e| AtlasError::Storage(e.to_string()))?;
    if let Some(r) = rows.next() {
        return Ok(Some(r.map_err(|e| AtlasError::Storage(e.to_string()))?));
    }
    Ok(None)
}

/// Hydrated row from the `symbols` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolRow {
    pub id: Vec<u8>,
    pub dump_id: i64,
    pub fqn: String,
    pub name: String,
    pub kind: i64,
    pub module: String,
    pub size: Option<u32>,
    pub align: Option<u32>,
    pub offset: Option<u32>,
    pub vtable_slot: Option<u32>,
    pub type_ref_json: Option<String>,
    pub flags: i64,
    pub source_file: Option<String>,
    pub source_line: Option<u32>,
}

#[cfg(test)]
mod tests {
    use atlas_parser_trait::{NullReporter, SdkParser};
    use atlas_parser_ue::Dumper7Parser;
    use std::path::PathBuf;

    use super::*;

    fn fixture_v1() -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.pop();
        p.pop();
        p.push("fixtures");
        p.push("synthetic");
        p.push("tiny-game-v1");
        p
    }

    fn build_db_with_v1() -> (tempfile::TempDir, Db, i64) {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("atlas.sqlite");
        let mut db = Db::open(&db_path).unwrap();
        let g = Dumper7Parser::new()
            .parse(&fixture_v1(), &NullReporter)
            .unwrap();
        let r = db.ingest(&g).unwrap();
        (tmp, db, r.dump_id)
    }

    #[test]
    fn build_index_then_query_finds_symbol() {
        let (tmp, db, dump_id) = build_db_with_v1();
        let idx_root = tmp.path().join("index");
        let idx = DumpIndex::rebuild(&db, dump_id, &idx_root).unwrap();

        let result = idx.query("APawn", &SearchFacets::default(), 10).unwrap();
        assert!(
            result.hits.iter().any(|h| h.fqn == "TinyGame.APawn"),
            "expected APawn in hits, got {:?}",
            result.hits
        );
    }

    #[test]
    fn facet_filter_restricts_to_kind() {
        let (tmp, db, dump_id) = build_db_with_v1();
        let idx_root = tmp.path().join("index");
        let idx = DumpIndex::rebuild(&db, dump_id, &idx_root).unwrap();

        let facets = SearchFacets {
            kinds: vec![3], // Enum
            modules: vec![],
        };
        let r = idx.query("", &facets, 10).unwrap();
        assert!(r.total_matched > 0);
        assert!(r.hits.iter().all(|h| h.kind_i == 3));
    }

    #[test]
    fn schema_version_mismatch_triggers_rebuild() {
        let (tmp, db, dump_id) = build_db_with_v1();
        let idx_root = tmp.path().join("index");

        // Build once.
        let _ = DumpIndex::open_or_build(&db, dump_id, &idx_root).unwrap();

        // Corrupt the marker.
        let marker = idx_root.join(dump_id.to_string()).join(SCHEMA_FILE_NAME);
        std::fs::write(&marker, "999").unwrap();

        // open_or_build should see the mismatch and rebuild.
        let idx2 = DumpIndex::open_or_build(&db, dump_id, &idx_root).unwrap();
        let r = idx2.query("APawn", &SearchFacets::default(), 5).unwrap();
        assert!(!r.hits.is_empty());
    }
}
