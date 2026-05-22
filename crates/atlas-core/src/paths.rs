//! Locate per-platform application data directories.
//!
//! Plan §2 fixes the locations:
//! - Windows:  `%APPDATA%\CodexAtlas\`
//! - macOS:    `~/Library/Application Support/CodexAtlas/`
//! - Linux:    `~/.local/share/CodexAtlas/`  (not officially supported, but harmless)
//!
//! `directories::ProjectDirs` derives the right path on every platform
//! from a `(qualifier, organization, application)` triple.
//!
//! For tests, every path-returning function accepts an override via the
//! `ATLAS_DATA_DIR` environment variable so test runs don't litter the
//! user's real AppData.

use std::path::PathBuf;

use directories::ProjectDirs;

use crate::error::{AtlasError, AtlasResult};

const ORG_QUALIFIER: &str = "dev";
const ORG_NAME: &str = "CBaileyDev";
const APP_NAME: &str = "CodexAtlas";

/// Returns the root data directory for Codex Atlas.
///
/// Honors `ATLAS_DATA_DIR` for tests.
pub fn data_dir() -> AtlasResult<PathBuf> {
    if let Ok(over) = std::env::var("ATLAS_DATA_DIR") {
        let p = PathBuf::from(over);
        std::fs::create_dir_all(&p)?;
        return Ok(p);
    }

    let dirs = ProjectDirs::from(ORG_QUALIFIER, ORG_NAME, APP_NAME)
        .ok_or_else(|| AtlasError::Io("could not determine project dirs".into()))?;
    let p = dirs.data_dir().to_path_buf();
    std::fs::create_dir_all(&p)?;
    Ok(p)
}

/// Path to the main SQLite database file.
pub fn db_path() -> AtlasResult<PathBuf> {
    Ok(data_dir()?.join("atlas.sqlite"))
}

/// Path to the log directory.
pub fn log_dir() -> AtlasResult<PathBuf> {
    let p = data_dir()?.join("logs");
    std::fs::create_dir_all(&p)?;
    Ok(p)
}

/// Path to the tantivy index root.
pub fn index_dir() -> AtlasResult<PathBuf> {
    let p = data_dir()?.join("index");
    std::fs::create_dir_all(&p)?;
    Ok(p)
}

/// Path to the user-editable templates directory.
pub fn templates_dir() -> AtlasResult<PathBuf> {
    let p = data_dir()?.join("templates");
    std::fs::create_dir_all(&p)?;
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_dir_via_env() {
        let tmp = tempfile::tempdir().unwrap();
        let prev = std::env::var("ATLAS_DATA_DIR").ok();

        // SAFETY: tests run single-threaded in `cargo test`'s default mode
        // for env vars matters at process scope; this is fine for a one-off
        // smoke test of the override path.
        // SAFETY-justification: see Rust 1.82 std::env::set_var safety note.
        // We restore the previous value at the end of the test.
        unsafe {
            std::env::set_var("ATLAS_DATA_DIR", tmp.path());
        }

        let d = data_dir().unwrap();
        assert_eq!(d, tmp.path());

        let db = db_path().unwrap();
        assert!(db.ends_with("atlas.sqlite"));

        unsafe {
            match prev {
                Some(v) => std::env::set_var("ATLAS_DATA_DIR", v),
                None => std::env::remove_var("ATLAS_DATA_DIR"),
            }
        }
    }
}
