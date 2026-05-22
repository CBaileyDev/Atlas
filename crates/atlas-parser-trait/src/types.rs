//! Core symbol-graph types. See plan §4.1 for the canonical definitions.
//!
//! Conventions:
//! - `local_id` is dump-scoped (u32) and only meaningful inside a single
//!   `SymbolGraph`. Storage assigns the persistent 16-byte symbol id.
//! - All types are `serde`-friendly so we can snapshot-test them with
//!   `insta` and round-trip them through JSON in tests.
//! - `TypeRef` keeps both "resolved" (linked to a local symbol) and
//!   "unresolved" (still a string) forms because Dumper-7 emits forward
//!   references and external types we can't always link locally.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::reporter::Reporter;

// ---------------------------------------------------------------------------
// ParserId
// ---------------------------------------------------------------------------

/// Stable identifier for a parser (`"dumper7-ue"`, `"il2cpp-unity"`, ...).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ParserId(pub String);

impl ParserId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// SourceMeta
// ---------------------------------------------------------------------------

/// Metadata about the dump that produced this graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMeta {
    pub parser: String,
    pub parser_version: String,
    pub game_id: String,
    pub game_version: String,
    pub ingested_at: DateTime<Utc>,
    pub sdk_root: PathBuf,
}

// ---------------------------------------------------------------------------
// SymbolKind / RelationKind
// ---------------------------------------------------------------------------

/// What kind of symbol a row represents.
///
/// The discriminant values are persisted to SQLite as the `kind` column,
/// so this enum is `#[repr(i64)]` and `#[non_exhaustive]` is intentionally
/// NOT used — adding a variant in the middle would break stored data.
/// New variants must be appended to the end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i64)]
pub enum SymbolKind {
    Module = 0,
    Class = 1,
    Struct = 2,
    Enum = 3,
    EnumValue = 4,
    Function = 5,
    Field = 6,
    Parameter = 7,
}

impl SymbolKind {
    pub const fn as_i64(self) -> i64 {
        self as i64
    }

    pub fn from_i64(v: i64) -> Option<Self> {
        Some(match v {
            0 => Self::Module,
            1 => Self::Class,
            2 => Self::Struct,
            3 => Self::Enum,
            4 => Self::EnumValue,
            5 => Self::Function,
            6 => Self::Field,
            7 => Self::Parameter,
            _ => return None,
        })
    }
}

/// Edges between symbols inside the same dump.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i64)]
pub enum RelationKind {
    Inherits = 0,
    Contains = 1,
    Returns = 2,
    TakesParam = 3,
    OfType = 4,
    Overrides = 5,
}

impl RelationKind {
    pub const fn as_i64(self) -> i64 {
        self as i64
    }

    pub fn from_i64(v: i64) -> Option<Self> {
        Some(match v {
            0 => Self::Inherits,
            1 => Self::Contains,
            2 => Self::Returns,
            3 => Self::TakesParam,
            4 => Self::OfType,
            5 => Self::Overrides,
            _ => return None,
        })
    }
}

// ---------------------------------------------------------------------------
// SymbolFlags
// ---------------------------------------------------------------------------

/// Bool flags carried with each symbol. Bool-field shape (rather than a
/// bitflags-style packed integer) keeps things explicit at the API level
/// and round-trips cleanly through JSON snapshots.
///
/// Packed into a u32 when written to SQLite; see `SymbolFlags::to_packed`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SymbolFlags {
    pub public: bool,
    pub private: bool,
    pub protected: bool,
    pub virtual_fn: bool,
    pub static_member: bool,
    pub const_member: bool,
    pub abstract_type: bool,
    pub deprecated: bool,
    pub pure_virtual: bool,
}

impl SymbolFlags {
    const BIT_PUBLIC: u32 = 1 << 0;
    const BIT_PRIVATE: u32 = 1 << 1;
    const BIT_PROTECTED: u32 = 1 << 2;
    const BIT_VIRTUAL: u32 = 1 << 3;
    const BIT_STATIC: u32 = 1 << 4;
    const BIT_CONST: u32 = 1 << 5;
    const BIT_ABSTRACT: u32 = 1 << 6;
    const BIT_DEPRECATED: u32 = 1 << 7;
    const BIT_PURE_VIRTUAL: u32 = 1 << 8;

    pub fn to_packed(self) -> u32 {
        let mut v = 0u32;
        if self.public {
            v |= Self::BIT_PUBLIC;
        }
        if self.private {
            v |= Self::BIT_PRIVATE;
        }
        if self.protected {
            v |= Self::BIT_PROTECTED;
        }
        if self.virtual_fn {
            v |= Self::BIT_VIRTUAL;
        }
        if self.static_member {
            v |= Self::BIT_STATIC;
        }
        if self.const_member {
            v |= Self::BIT_CONST;
        }
        if self.abstract_type {
            v |= Self::BIT_ABSTRACT;
        }
        if self.deprecated {
            v |= Self::BIT_DEPRECATED;
        }
        if self.pure_virtual {
            v |= Self::BIT_PURE_VIRTUAL;
        }
        v
    }

    pub fn from_packed(v: u32) -> Self {
        Self {
            public: v & Self::BIT_PUBLIC != 0,
            private: v & Self::BIT_PRIVATE != 0,
            protected: v & Self::BIT_PROTECTED != 0,
            virtual_fn: v & Self::BIT_VIRTUAL != 0,
            static_member: v & Self::BIT_STATIC != 0,
            const_member: v & Self::BIT_CONST != 0,
            abstract_type: v & Self::BIT_ABSTRACT != 0,
            deprecated: v & Self::BIT_DEPRECATED != 0,
            pure_virtual: v & Self::BIT_PURE_VIRTUAL != 0,
        }
    }
}

// ---------------------------------------------------------------------------
// TypeRef
// ---------------------------------------------------------------------------

/// Reference to a type, used by fields, function returns, function
/// parameters, and template arguments.
///
/// The parser tries to resolve to a `Local` ref (linked to a symbol in
/// the same graph). If the type is a primitive (`int32`, `float`,
/// `bool`) it becomes `Builtin`. If it's an external/forward reference
/// we couldn't resolve at parse time, it stays as `Unresolved` with the
/// original textual form.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TypeRef {
    Local {
        local_id: u32,
        #[serde(default)]
        modifiers: TypeModifiers,
    },
    Builtin {
        name: String,
        #[serde(default)]
        modifiers: TypeModifiers,
    },
    Unresolved {
        name: String,
        #[serde(default)]
        modifiers: TypeModifiers,
    },
}

impl TypeRef {
    pub fn modifiers(&self) -> &TypeModifiers {
        match self {
            Self::Local { modifiers, .. }
            | Self::Builtin { modifiers, .. }
            | Self::Unresolved { modifiers, .. } => modifiers,
        }
    }

    /// Display name suitable for human-readable rendering. Stable across
    /// the three variants.
    pub fn display_name(&self) -> String {
        let base = match self {
            Self::Local { local_id, .. } => format!("#{local_id}"),
            Self::Builtin { name, .. } | Self::Unresolved { name, .. } => name.clone(),
        };
        self.modifiers().decorate(&base)
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TypeModifiers {
    #[serde(default)]
    pub pointer_depth: u8,
    #[serde(default)]
    pub is_reference: bool,
    #[serde(default)]
    pub is_const: bool,
    #[serde(default)]
    pub array_dim: Option<u32>,
    #[serde(default)]
    pub template_args: Vec<TypeRef>,
}

impl TypeModifiers {
    fn decorate(&self, base: &str) -> String {
        let mut out = String::new();
        if self.is_const {
            out.push_str("const ");
        }
        out.push_str(base);
        if !self.template_args.is_empty() {
            out.push('<');
            for (i, t) in self.template_args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&t.display_name());
            }
            out.push('>');
        }
        for _ in 0..self.pointer_depth {
            out.push('*');
        }
        if self.is_reference {
            out.push('&');
        }
        if let Some(dim) = self.array_dim {
            out.push_str(&format!("[{dim}]"));
        }
        out
    }
}

// ---------------------------------------------------------------------------
// SourceLoc
// ---------------------------------------------------------------------------

/// Where a symbol came from in the source dump (file + line).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceLoc {
    pub file: String,
    pub line: u32,
}

// ---------------------------------------------------------------------------
// Symbol
// ---------------------------------------------------------------------------

/// One row of the symbol graph. `local_id` is unique inside this graph
/// only — storage assigns the persistent 16-byte id at insert time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    pub local_id: u32,
    pub fqn: String,
    pub name: String,
    pub kind: SymbolKind,
    pub module: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vtable_slot: Option<u32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_ref: Option<TypeRef>,

    #[serde(default)]
    pub flags: SymbolFlags,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_loc: Option<SourceLoc>,
}

// ---------------------------------------------------------------------------
// Relation
// ---------------------------------------------------------------------------

/// Edge between two symbols inside the same graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Relation {
    pub from: u32,
    pub to: u32,
    pub kind: RelationKind,
}

// ---------------------------------------------------------------------------
// SymbolGraph
// ---------------------------------------------------------------------------

/// The whole graph produced by one parser run. Plain data — no DB
/// handles, no logger, no Tauri imports — so it can be snapshot-tested
/// and shipped between threads freely.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolGraph {
    pub source: SourceMeta,
    pub symbols: Vec<Symbol>,
    pub relations: Vec<Relation>,
}

impl SymbolGraph {
    /// Locate a symbol by its `local_id`. O(n); used for tests and
    /// reporting, not hot paths.
    pub fn lookup(&self, local_id: u32) -> Option<&Symbol> {
        self.symbols.iter().find(|s| s.local_id == local_id)
    }

    pub fn symbol_count(&self) -> usize {
        self.symbols.len()
    }

    pub fn relation_count(&self) -> usize {
        self.relations.len()
    }
}

// ---------------------------------------------------------------------------
// ParseError
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("io: {0}")]
    Io(String),
    #[error("not a {parser} dump: {reason}")]
    UnsupportedFormat { parser: String, reason: String },
    #[error("parse at {file}:{line}: {message}")]
    Syntax {
        file: String,
        line: u32,
        message: String,
    },
    #[error("{0}")]
    Other(String),
}

impl From<std::io::Error> for ParseError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// SdkParser trait
// ---------------------------------------------------------------------------

/// What every game-SDK parser implements.
///
/// `name()` is the stable identifier persisted into the `dumps.parser`
/// column. `can_handle()` is a cheap heuristic the application uses to
/// pick a parser (or auto-detect). `parse()` does the work.
pub trait SdkParser: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn can_handle(&self, root: &Path) -> bool;
    fn parse(&self, root: &Path, reporter: &dyn Reporter) -> Result<SymbolGraph, ParseError>;
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_graph() -> SymbolGraph {
        SymbolGraph {
            source: SourceMeta {
                parser: "test-parser".into(),
                parser_version: "0.0.0".into(),
                game_id: "TestGame".into(),
                game_version: "1.0".into(),
                ingested_at: DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
                sdk_root: PathBuf::from("/tmp/sdk"),
            },
            symbols: vec![
                Symbol {
                    local_id: 1,
                    fqn: "TestGame.Foo".into(),
                    name: "Foo".into(),
                    kind: SymbolKind::Class,
                    module: "TestGame".into(),
                    size: Some(64),
                    align: Some(8),
                    offset: None,
                    vtable_slot: None,
                    type_ref: None,
                    flags: SymbolFlags::default(),
                    source_loc: Some(SourceLoc {
                        file: "TestGame.hpp".into(),
                        line: 42,
                    }),
                },
                Symbol {
                    local_id: 2,
                    fqn: "TestGame.Foo.bar".into(),
                    name: "bar".into(),
                    kind: SymbolKind::Field,
                    module: "TestGame".into(),
                    size: Some(4),
                    align: Some(4),
                    offset: Some(8),
                    vtable_slot: None,
                    type_ref: Some(TypeRef::Builtin {
                        name: "int32".into(),
                        modifiers: TypeModifiers::default(),
                    }),
                    flags: SymbolFlags {
                        public: true,
                        ..Default::default()
                    },
                    source_loc: None,
                },
            ],
            relations: vec![Relation {
                from: 1,
                to: 2,
                kind: RelationKind::Contains,
            }],
        }
    }

    #[test]
    fn graph_round_trips_through_json() {
        let g = sample_graph();
        let json = serde_json::to_string(&g).unwrap();
        let back: SymbolGraph = serde_json::from_str(&json).unwrap();
        assert_eq!(g, back);
    }

    #[test]
    fn symbol_kind_round_trip() {
        for variant in [
            SymbolKind::Module,
            SymbolKind::Class,
            SymbolKind::Struct,
            SymbolKind::Enum,
            SymbolKind::EnumValue,
            SymbolKind::Function,
            SymbolKind::Field,
            SymbolKind::Parameter,
        ] {
            let i = variant.as_i64();
            let back = SymbolKind::from_i64(i).unwrap();
            assert_eq!(variant, back, "round trip at {i}");
        }
    }

    #[test]
    fn relation_kind_round_trip() {
        for variant in [
            RelationKind::Inherits,
            RelationKind::Contains,
            RelationKind::Returns,
            RelationKind::TakesParam,
            RelationKind::OfType,
            RelationKind::Overrides,
        ] {
            let i = variant.as_i64();
            let back = RelationKind::from_i64(i).unwrap();
            assert_eq!(variant, back, "round trip at {i}");
        }
    }

    #[test]
    fn symbol_flags_pack_round_trip() {
        let f = SymbolFlags {
            public: true,
            virtual_fn: true,
            pure_virtual: true,
            ..Default::default()
        };
        let packed = f.to_packed();
        let back = SymbolFlags::from_packed(packed);
        assert_eq!(f, back);
    }

    #[test]
    fn type_ref_display_name() {
        let t = TypeRef::Builtin {
            name: "int32".into(),
            modifiers: TypeModifiers {
                pointer_depth: 2,
                is_const: true,
                ..Default::default()
            },
        };
        assert_eq!(t.display_name(), "const int32**");
    }

    #[test]
    fn type_ref_template_args_render() {
        let t = TypeRef::Unresolved {
            name: "TArray".into(),
            modifiers: TypeModifiers {
                template_args: vec![TypeRef::Builtin {
                    name: "FString".into(),
                    modifiers: TypeModifiers::default(),
                }],
                ..Default::default()
            },
        };
        assert_eq!(t.display_name(), "TArray<FString>");
    }

    #[test]
    fn graph_lookup_finds_existing_and_misses_missing() {
        let g = sample_graph();
        assert!(g.lookup(1).is_some());
        assert!(g.lookup(999).is_none());
        assert_eq!(g.symbol_count(), 2);
        assert_eq!(g.relation_count(), 1);
    }
}
