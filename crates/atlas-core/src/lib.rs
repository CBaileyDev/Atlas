//! Codex Atlas — core engines.
//!
//! This crate owns the **storage**, **search**, **diff**, and **export**
//! sides of Atlas. It deliberately knows nothing about Tauri, the
//! filesystem layout of an SDK dump, or HTTP. Other crates (`src-tauri`,
//! `atlas-parser-*`) compose on top of it.
//!
//! See `CODEX_ATLAS_PLAN.md` at the repo root for the architectural
//! invariants this crate enforces (especially §3, the "five boundaries").
//!
//! ## Phase 0 status
//!
//! Phase 0 only ships:
//! - the `AtlasError` enum (§12.4)
//! - the `paths` module (locating per-platform data directories)
//! - the `storage` module's connection plumbing + migration 0001 (§4.2)
//!
//! Everything else (ingest, diff, search, export) lands in later phases.

pub mod diff;
pub mod error;
pub mod export;
pub mod paths;
pub mod search;
pub mod storage;

pub use error::{AtlasError, AtlasResult};

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_links() {
        // Phase 0 sanity test — confirms that the crate builds, that its
        // dependency on atlas-parser-trait resolves, and that tests run.
        assert_eq!(atlas_parser_trait::crate_smoke(), "atlas-parser-trait");
    }
}
