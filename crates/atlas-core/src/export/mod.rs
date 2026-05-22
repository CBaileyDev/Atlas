//! Symbol selection → Tera template → on-disk scaffold.
//!
//! Plan §10. The export module turns a selection of symbols (by id)
//! into a rendered text artifact (typically a C# trainer skeleton or
//! a C++ offsets header) plus an `_atlas.json` sidecar describing
//! exactly which dump and which template produced it.
//!
//! Templates are bundled into the binary via `include_str!` and can be
//! overridden by dropping a same-named file into the user's
//! `<data>/templates/` directory.

pub mod render;
pub mod sidecar;
pub mod templates;

pub use render::{build_context, render_to_string, ExportContext, SymbolView};
pub use sidecar::AtlasSidecar;
pub use templates::{available_templates, load_template, TemplateInfo};

use serde::{Deserialize, Serialize};

/// What gets persisted in the `projects` table: a saved selection +
/// template pairing the user can re-run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportProject {
    pub id: Option<i64>,
    pub name: String,
    pub dump_id: i64,
    pub template_name: String,
    pub selection: Selection,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Selection {
    /// Hex ids of the symbols the user explicitly picked.
    pub symbol_ids_hex: Vec<String>,
    pub rules: SelectionRules,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SelectionRules {
    /// Also include each picked symbol's parent class (transitively).
    pub include_parents: bool,
    /// Depth N: include types referenced by selected fields up to depth
    /// N. 0 = off. 1 = direct refs. 2 = refs-of-refs. Etc.
    pub type_depth: u32,
}
