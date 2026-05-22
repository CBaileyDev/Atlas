//! Tera rendering. Builds an `ExportContext`, hands it to Tera, returns
//! the rendered string.
//!
//! View-model design (see `SymbolView`): keep fields flat and string-
//! shaped where possible. Templates run inside Tera which doesn't have
//! to know about Rust enums. Offsets surface as both decimal and
//! hex-formatted strings since a template author shouldn't have to
//! re-derive `format!("0x{:X}", offset)` in every cheat entry.

use std::collections::HashMap;

use atlas_parser_trait::SymbolKind;
use serde::{Deserialize, Serialize};
use tera::{Context, Tera};

use crate::error::{AtlasError, AtlasResult};
use crate::storage::Db;

/// Minimal view-model of a single symbol surfaced to templates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolView {
    pub id_hex: String,
    pub fqn: String,
    pub name: String,
    pub kind: String,
    pub kind_i: i64,
    pub module: String,
    pub size: Option<u32>,
    pub align: Option<u32>,
    pub offset: Option<u32>,
    pub offset_hex: Option<String>,
    pub vtable_slot: Option<u32>,
    pub vtable_slot_hex: Option<String>,
    /// For fields: which class this field lives under.
    pub parent_fqn: Option<String>,
    /// For fields: the rendered type expression (`int32_t`, `class UFoo*`).
    pub type_display: Option<String>,
    /// For fields: a TODO marker text the template can include where
    /// the user needs to supply a pointer chain at runtime.
    pub todo_chain: bool,
}

/// Full context passed to every template render. Templates can branch
/// on `template_name` (the chosen file) but the context shape is
/// stable across templates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportContext {
    pub atlas_version: String,
    pub generated_at: String,
    pub game_id: String,
    pub game_version: String,
    pub dump_id: i64,
    pub project_name: String,
    pub trainer_class_name: String,
    pub process_name: String,
    pub symbols: Vec<SymbolView>,
}

impl ExportContext {
    pub fn into_tera(self) -> AtlasResult<Context> {
        Context::from_serialize(self)
            .map_err(|e| AtlasError::Export(format!("context serialize: {e}")))
    }
}

/// Build an `ExportContext` from a set of symbol ids in the given dump.
/// Order of `symbol_ids_hex` is preserved.
pub fn build_context(
    db: &Db,
    dump_id: i64,
    symbol_ids_hex: &[String],
    project_name: &str,
    trainer_class_name: &str,
    process_name: &str,
) -> AtlasResult<ExportContext> {
    let (game_id, game_version) = db
        .conn
        .query_row(
            "SELECT game_id, game_version FROM dumps WHERE id = ?",
            [dump_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .map_err(|e| AtlasError::Export(format!("dump lookup: {e}")))?;

    // Walk fqn-by-fqn building parent maps for the few rows we need.
    let mut id_to_view: HashMap<String, SymbolView> = HashMap::new();
    let mut id_to_parent_fqn: HashMap<String, String> = HashMap::new();

    // Hydrate the picked symbols.
    let mut stmt = db
        .conn
        .prepare(
            "SELECT id, fqn, name, kind, module, size, align, offset, vtable_slot
             FROM symbols WHERE dump_id = ? AND id = ?",
        )
        .map_err(|e| AtlasError::Export(format!("prepare: {e}")))?;

    for id_hex in symbol_ids_hex {
        let id_bytes = decode_hex(id_hex)
            .ok_or_else(|| AtlasError::Export(format!("bad id_hex: {id_hex}")))?;
        let row = stmt.query_row(rusqlite::params![dump_id, &id_bytes], |r| {
            Ok((
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<u32>>(5)?,
                r.get::<_, Option<u32>>(6)?,
                r.get::<_, Option<u32>>(7)?,
                r.get::<_, Option<u32>>(8)?,
            ))
        });
        let Ok((fqn, name, kind, module, size, align, offset, vtable_slot)) = row else {
            continue; // silently skip unknown ids
        };
        let kind_e = SymbolKind::from_i64(kind).unwrap_or(SymbolKind::Module);
        let parent_fqn = parent_fqn_of(db, dump_id, &id_bytes)?;
        let view = SymbolView {
            id_hex: id_hex.clone(),
            fqn: fqn.clone(),
            name,
            kind: kind_label(kind_e).into(),
            kind_i: kind,
            module,
            size,
            align,
            offset,
            offset_hex: offset.map(|o| format!("0x{o:X}")),
            vtable_slot,
            vtable_slot_hex: vtable_slot.map(|v| format!("0x{v:02X}")),
            parent_fqn: parent_fqn.clone(),
            type_display: None,
            todo_chain: matches!(kind_e, SymbolKind::Field),
        };
        if let Some(pfqn) = parent_fqn {
            id_to_parent_fqn.insert(id_hex.clone(), pfqn);
        }
        id_to_view.insert(id_hex.clone(), view);
    }

    // Preserve the order the user picked.
    let symbols: Vec<SymbolView> = symbol_ids_hex
        .iter()
        .filter_map(|h| id_to_view.get(h).cloned())
        .collect();

    Ok(ExportContext {
        atlas_version: env!("CARGO_PKG_VERSION").to_string(),
        generated_at: chrono::Utc::now().to_rfc3339(),
        game_id,
        game_version,
        dump_id,
        project_name: project_name.to_string(),
        trainer_class_name: trainer_class_name.to_string(),
        process_name: process_name.to_string(),
        symbols,
    })
}

fn parent_fqn_of(db: &Db, dump_id: i64, id_bytes: &[u8]) -> AtlasResult<Option<String>> {
    // A Field's parent class is the source of a Contains relation.
    let mut stmt = db
        .conn
        .prepare(
            "SELECT s.fqn
             FROM relations r
             JOIN symbols s ON s.id = r.from_symbol
             WHERE r.to_symbol = ? AND r.kind = 1 AND s.dump_id = ?
             LIMIT 1",
        )
        .map_err(|e| AtlasError::Export(format!("prepare parent: {e}")))?;
    let row: Result<String, _> = stmt.query_row(rusqlite::params![id_bytes, dump_id], |r| r.get(0));
    match row {
        Ok(s) => Ok(Some(s)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AtlasError::Export(format!("parent query: {e}"))),
    }
}

const fn kind_label(k: SymbolKind) -> &'static str {
    match k {
        SymbolKind::Module => "module",
        SymbolKind::Class => "class",
        SymbolKind::Struct => "struct",
        SymbolKind::Enum => "enum",
        SymbolKind::EnumValue => "enum_value",
        SymbolKind::Function => "function",
        SymbolKind::Field => "field",
        SymbolKind::Parameter => "parameter",
    }
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() != 32 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..16)
        .map(|i| {
            let lo = nibble(s.as_bytes()[i * 2])?;
            let hi = nibble(s.as_bytes()[i * 2 + 1])?;
            Some((lo << 4) | hi)
        })
        .collect()
}

fn nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Render a single template by source text against the given context.
pub fn render_to_string(
    template_name: &str,
    source: &str,
    ctx: ExportContext,
) -> AtlasResult<String> {
    let mut tera = Tera::default();
    tera.add_raw_template(template_name, source)
        .map_err(|e| AtlasError::Export(format!("add_template: {e}")))?;
    let context = ctx.into_tera()?;
    tera.render(template_name, &context)
        .map_err(|e| AtlasError::Export(format!("render: {e}")))
}
