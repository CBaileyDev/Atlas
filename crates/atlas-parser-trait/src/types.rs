//! Placeholder type module. Real `SymbolGraph`, `Symbol`, `Relation`, and
//! `SdkParser` trait definitions land in Phase 1 (see plan §4.1).
//!
//! Phase 0 only needs the crate to compile and export something so that
//! downstream crates can link against it.

use serde::{Deserialize, Serialize};

/// Parser identification tag. Real parsers will set this to a stable
/// string like `"dumper7-ue"` or `"il2cpp-unity"`.
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
