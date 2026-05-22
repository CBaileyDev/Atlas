//! Shared types and the [`SdkParser`] trait used by every parser crate.
//!
//! Parsers in Codex Atlas produce a [`SymbolGraph`] and walk away. They
//! never touch the database or the search index. That separation is what
//! lets us snapshot-test parsers, swap in synthetic fixtures, and add
//! new parsers (Unity IL2CPP, etc.) without changing the storage layer.
//!
//! See the project plan (`CODEX_ATLAS_PLAN.md` §3) for the boundary
//! invariants this trait enforces.

pub mod reporter;
pub mod types;

pub use reporter::{CollectingReporter, NullReporter, Reporter};
pub use types::{
    ParseError, ParserId, Relation, RelationKind, SdkParser, SourceLoc, SourceMeta, Symbol,
    SymbolFlags, SymbolGraph, SymbolKind, TypeModifiers, TypeRef,
};

#[doc(hidden)]
pub fn crate_smoke() -> &'static str {
    "atlas-parser-trait"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_returns_crate_name() {
        assert_eq!(crate_smoke(), "atlas-parser-trait");
    }
}
