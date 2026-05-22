//! Persistent user settings.
//!
//! Stored at `<data>/settings.json`. Read-on-demand, written via
//! `save()`. Backend-only settings live here; frontend-only
//! preferences (theme) live in `localStorage`.
//!
//! Forward-compatibility: unknown fields are ignored on load
//! (`#[serde(default)]` plus `Default` impls), so adding a new
//! setting in a future version is a non-breaking change.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{AtlasError, AtlasResult};
use crate::paths;

const FILENAME: &str = "settings.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtlasSettings {
    /// Directories the folder watcher should monitor for new
    /// Dumper-7-style SDK outputs.
    #[serde(default)]
    pub watcher_roots: Vec<PathBuf>,
    /// How long a watched path must stay unchanged before the
    /// watcher considers it "stable" and worth inspecting. The plan
    /// recommends 5 seconds (§11).
    #[serde(default = "default_debounce_ms")]
    pub watcher_debounce_ms: u64,
}

impl Default for AtlasSettings {
    fn default() -> Self {
        Self {
            watcher_roots: Vec::new(),
            watcher_debounce_ms: default_debounce_ms(),
        }
    }
}

const fn default_debounce_ms() -> u64 {
    5_000
}

impl AtlasSettings {
    pub fn load() -> AtlasResult<Self> {
        let path = paths::data_dir()?.join(FILENAME);
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(&path)?;
        let s: Self = serde_json::from_str(&raw)
            .map_err(|e| AtlasError::Storage(format!("settings: {e}")))?;
        Ok(s)
    }

    pub fn save(&self) -> AtlasResult<()> {
        let path = paths::data_dir()?.join(FILENAME);
        let raw = serde_json::to_string_pretty(self)
            .map_err(|e| AtlasError::Storage(format!("settings: {e}")))?;
        std::fs::write(&path, raw)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    // `ATLAS_DATA_DIR` is process-scoped, so any test that flips it has
    // to serialize against every other one. Cheaper than pulling in
    // `serial_test`; correctness is the same.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_data_dir<F: FnOnce()>(body: F) {
        let _g = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let prev = std::env::var("ATLAS_DATA_DIR").ok();
        // SAFETY: serialized by ENV_LOCK; restored before the lock is
        // dropped (which happens when this function returns).
        unsafe {
            std::env::set_var("ATLAS_DATA_DIR", tmp.path());
        }
        body();
        unsafe {
            match prev {
                Some(v) => std::env::set_var("ATLAS_DATA_DIR", v),
                None => std::env::remove_var("ATLAS_DATA_DIR"),
            }
        }
    }

    #[test]
    fn save_and_load_round_trips_settings() {
        with_data_dir(|| {
            let original = AtlasSettings {
                watcher_roots: vec![PathBuf::from(r"C:\Dumps"), PathBuf::from(r"D:\More\Dumps")],
                watcher_debounce_ms: 7_500,
            };
            original.save().unwrap();

            let back = AtlasSettings::load().unwrap();
            assert_eq!(back.watcher_roots, original.watcher_roots);
            assert_eq!(back.watcher_debounce_ms, original.watcher_debounce_ms);
        });
    }

    #[test]
    fn load_returns_default_when_file_missing() {
        with_data_dir(|| {
            let s = AtlasSettings::load().unwrap();
            assert!(s.watcher_roots.is_empty());
            assert_eq!(s.watcher_debounce_ms, 5_000);
        });
    }
}
