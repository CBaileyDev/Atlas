//! Shared types and the `SdkParser` trait used by every parser crate.
//!
//! Parsers in Codex Atlas produce a `SymbolGraph` and walk away. They never
//! touch the database or the search index. That separation is what lets us
//! snapshot-test parsers, swap in synthetic fixtures, and add new parsers
//! (Unity IL2CPP, etc.) without changing the storage layer.
//!
//! The concrete types defined here live in `crate::types`. Phase 0 ships
//! only the placeholders needed for the workspace to compile; the full
//! types in plan §4.1 land in Phase 1 alongside the Dumper-7 parser.

pub mod types;

pub use types::*;

/// Marker function used by Phase 0 acceptance tests to confirm the crate
/// links and exports. Removed once real symbols are added in Phase 1.
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
