//! Unity IL2CPP parser — Phase 5 stub.
//!
//! Purpose for now is **architectural**: prove that the `SdkParser`
//! trait in `atlas-parser-trait` supports more than one concrete
//! parser, that the trait's interface is enough to produce a
//! `SymbolGraph` from a non-UE source, and that the resulting graph
//! ingests through the same `atlas-core::storage::ingest` path
//! without any UE-specific assumptions.
//!
//! The stub recognizes a directory containing a `dump.cs` file (the
//! file Il2CppDumper / Cpp2IL emit) or an `il2cpp-stub.json` marker
//! we generate in tests. It returns a tiny graph with one module
//! plus one placeholder class so the surrounding pipeline gets to
//! exercise the multi-parser path.
//!
//! A real Unity parser lands in a future phase.

use std::path::{Path, PathBuf};

use atlas_parser_trait::{
    ParseError, Relation, RelationKind, Reporter, SdkParser, SourceLoc, SourceMeta, Symbol,
    SymbolFlags, SymbolGraph, SymbolKind,
};
use chrono::Utc;

pub const PARSER_NAME: &str = "il2cpp-unity";
pub const PARSER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Marker file used by tests / synthetic fixtures so the stub can
/// claim a folder without needing a real Il2CppDumper dump on disk.
const STUB_MARKER: &str = "il2cpp-stub.json";

#[derive(Debug, Default, Clone, Copy)]
pub struct Il2CppStubParser;

impl Il2CppStubParser {
    pub fn new() -> Self {
        Self
    }
}

impl SdkParser for Il2CppStubParser {
    fn name(&self) -> &str {
        PARSER_NAME
    }

    fn version(&self) -> &str {
        PARSER_VERSION
    }

    fn can_handle(&self, root: &Path) -> bool {
        if !root.is_dir() {
            return false;
        }
        root.join(STUB_MARKER).exists() || root.join("dump.cs").exists()
    }

    fn parse(&self, root: &Path, reporter: &dyn Reporter) -> Result<SymbolGraph, ParseError> {
        if !self.can_handle(root) {
            return Err(ParseError::UnsupportedFormat {
                parser: PARSER_NAME.to_string(),
                reason: format!(
                    "{} is not a recognized IL2CPP dump (no {STUB_MARKER}, no dump.cs)",
                    root.display()
                ),
            });
        }

        let info = read_marker(root).unwrap_or_default();
        reporter.started(Some(1));
        reporter.progress(1, "stub: emitting placeholder symbols");

        let module_id = 0u32;
        let class_id = 1u32;
        let symbols = vec![
            Symbol {
                local_id: module_id,
                fqn: info.module_name.clone(),
                name: info.module_name.clone(),
                kind: SymbolKind::Module,
                module: info.module_name.clone(),
                size: None,
                align: None,
                offset: None,
                vtable_slot: None,
                type_ref: None,
                flags: SymbolFlags::default(),
                source_loc: Some(SourceLoc {
                    file: STUB_MARKER.into(),
                    line: 1,
                }),
            },
            Symbol {
                local_id: class_id,
                fqn: format!("{}.{}", info.module_name, info.placeholder_class),
                name: info.placeholder_class.clone(),
                kind: SymbolKind::Class,
                module: info.module_name.clone(),
                size: None,
                align: None,
                offset: None,
                vtable_slot: None,
                type_ref: None,
                flags: SymbolFlags {
                    public: true,
                    ..Default::default()
                },
                source_loc: Some(SourceLoc {
                    file: STUB_MARKER.into(),
                    line: 1,
                }),
            },
        ];
        let relations = vec![Relation {
            from: module_id,
            to: class_id,
            kind: RelationKind::Contains,
        }];

        reporter.finished();

        Ok(SymbolGraph {
            source: SourceMeta {
                parser: PARSER_NAME.to_string(),
                parser_version: PARSER_VERSION.to_string(),
                game_id: info.game_id.clone(),
                game_version: info.game_version.clone(),
                ingested_at: Utc::now(),
                sdk_root: PathBuf::from(root),
            },
            symbols,
            relations,
        })
    }
}

#[derive(Debug)]
struct StubInfo {
    game_id: String,
    game_version: String,
    module_name: String,
    placeholder_class: String,
}

impl Default for StubInfo {
    fn default() -> Self {
        Self {
            game_id: "UnknownUnityGame".into(),
            game_version: "unknown".into(),
            module_name: "Assembly-CSharp".into(),
            placeholder_class: "Placeholder".into(),
        }
    }
}

fn read_marker(root: &Path) -> Option<StubInfo> {
    let p = root.join(STUB_MARKER);
    let raw = std::fs::read_to_string(&p).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    Some(StubInfo {
        game_id: v.get("game_id")?.as_str()?.to_string(),
        game_version: v
            .get("game_version")
            .and_then(|x| x.as_str())
            .unwrap_or("unknown")
            .to_string(),
        module_name: v
            .get("module_name")
            .and_then(|x| x.as_str())
            .unwrap_or("Assembly-CSharp")
            .to_string(),
        placeholder_class: v
            .get("placeholder_class")
            .and_then(|x| x.as_str())
            .unwrap_or("Placeholder")
            .to_string(),
    })
}

#[cfg(test)]
mod tests {
    use atlas_parser_trait::NullReporter;
    use tempfile::TempDir;

    use super::*;

    fn fixture() -> TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let marker = tmp.path().join(STUB_MARKER);
        std::fs::write(
            &marker,
            serde_json::json!({
                "game_id": "TestUnityGame",
                "game_version": "2026.1.0f1",
                "module_name": "Assembly-CSharp",
                "placeholder_class": "PlayerController"
            })
            .to_string(),
        )
        .unwrap();
        tmp
    }

    #[test]
    fn parser_name_is_stable() {
        assert_eq!(PARSER_NAME, "il2cpp-unity");
    }

    #[test]
    fn rejects_directory_without_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let p = Il2CppStubParser::new();
        assert!(!p.can_handle(tmp.path()));
        assert!(p.parse(tmp.path(), &NullReporter).is_err());
    }

    #[test]
    fn accepts_marker_and_emits_module_plus_class() {
        let tmp = fixture();
        let p = Il2CppStubParser::new();
        assert!(p.can_handle(tmp.path()));
        let g = p.parse(tmp.path(), &NullReporter).unwrap();
        assert_eq!(g.source.parser, "il2cpp-unity");
        assert_eq!(g.source.game_id, "TestUnityGame");
        assert_eq!(g.source.game_version, "2026.1.0f1");
        assert_eq!(g.symbols.len(), 2);
        assert!(g
            .symbols
            .iter()
            .any(|s| s.kind == SymbolKind::Class && s.name == "PlayerController"));
        assert_eq!(g.relations.len(), 1);
    }
}
