//! `_atlas.json` sidecar — written alongside every export so we can
//! recover (and round-trip) the selection later.
//!
//! Plan §10 schema. `template_version` is a BLAKE3 hash of the
//! template source so the sidecar can tell whether the user has
//! since edited it.

use blake3::Hasher;
use serde::{Deserialize, Serialize};

use crate::export::Selection;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtlasSidecar {
    pub atlas_version: String,
    pub exported_at: String,
    pub game_id: String,
    pub game_version: String,
    pub dump_id: i64,
    pub template: String,
    pub template_version: String,
    pub symbols: Vec<String>,
    pub selection_rules: SelectionRulesSidecar,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionRulesSidecar {
    pub include_parents: bool,
    pub type_depth: u32,
}

impl AtlasSidecar {
    pub fn build(
        atlas_version: impl Into<String>,
        game_id: impl Into<String>,
        game_version: impl Into<String>,
        dump_id: i64,
        template_name: impl Into<String>,
        template_source: &str,
        selection: &Selection,
    ) -> Self {
        let mut h = Hasher::new();
        h.update(template_source.as_bytes());
        let template_version = format!("blake3:{}", h.finalize().to_hex());
        Self {
            atlas_version: atlas_version.into(),
            exported_at: chrono::Utc::now().to_rfc3339(),
            game_id: game_id.into(),
            game_version: game_version.into(),
            dump_id,
            template: template_name.into(),
            template_version,
            symbols: selection.symbol_ids_hex.clone(),
            selection_rules: SelectionRulesSidecar {
                include_parents: selection.rules.include_parents,
                type_depth: selection.rules.type_depth,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::SelectionRules;

    #[test]
    fn sidecar_round_trips_through_json() {
        let sel = Selection {
            symbol_ids_hex: vec!["00".repeat(16), "ff".repeat(16)],
            rules: SelectionRules {
                include_parents: true,
                type_depth: 1,
            },
        };
        let s = AtlasSidecar::build("0.4.0", "TestGame", "1.0", 1, "Trainer.cs", "hello", &sel);
        let json = serde_json::to_string(&s).unwrap();
        let back: AtlasSidecar = serde_json::from_str(&json).unwrap();
        assert_eq!(back.template, s.template);
        assert_eq!(back.template_version, s.template_version);
        assert_eq!(back.symbols, s.symbols);
    }

    #[test]
    fn template_version_changes_with_source() {
        let sel = Selection::default();
        let a = AtlasSidecar::build("v", "g", "1", 1, "t", "alpha", &sel);
        let b = AtlasSidecar::build("v", "g", "1", 1, "t", "beta", &sel);
        assert_ne!(a.template_version, b.template_version);
    }
}
