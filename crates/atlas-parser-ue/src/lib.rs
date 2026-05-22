//! Dumper-7 parser for Unreal Engine SDK dumps.
//!
//! Format notes live in `docs/parser-format-notes.md`. The
//! parser is intentionally **NOT** a full C++ parser — Dumper-7
//! emits predictable shapes and a hand-written recursive-descent
//! routine over those shapes is much smaller and faster than
//! pulling in `tree-sitter-cpp` or `clang-sys`.
//!
//! Phase 1 ships against a synthetic fixture under
//! `fixtures/synthetic/tiny-game-v{1,2}/`. Plan §13 STOP #1
//! mandates verification against a real Dumper-7 fixture before
//! locking the grammar.

use std::path::{Path, PathBuf};

use atlas_parser_trait::{ParseError, Reporter, SdkParser, SourceMeta, SymbolGraph};
use chrono::Utc;

mod lexer;
mod parser;

pub const PARSER_NAME: &str = "dumper7-ue";
pub const PARSER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The Dumper-7 SDK parser.
#[derive(Debug, Default, Clone, Copy)]
pub struct Dumper7Parser;

impl Dumper7Parser {
    pub fn new() -> Self {
        Self
    }
}

impl SdkParser for Dumper7Parser {
    fn name(&self) -> &str {
        PARSER_NAME
    }

    fn version(&self) -> &str {
        PARSER_VERSION
    }

    /// Cheap heuristic: looks for either an `SDKInfo.json` file or an
    /// `SDK.hpp` at the root. Either is enough to classify the folder.
    fn can_handle(&self, root: &Path) -> bool {
        root.is_dir() && (root.join("SDK.hpp").exists() || root.join("SDKInfo.json").exists())
    }

    fn parse(&self, root: &Path, reporter: &dyn Reporter) -> Result<SymbolGraph, ParseError> {
        if !root.is_dir() {
            return Err(ParseError::UnsupportedFormat {
                parser: PARSER_NAME.to_string(),
                reason: format!("{} is not a directory", root.display()),
            });
        }

        let info = SdkInfo::read(root).ok();
        let game_id = info
            .as_ref()
            .map(|i| i.game_name.clone())
            .unwrap_or_else(|| {
                root.file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("UnknownGame")
                    .to_string()
            });
        let game_version = info
            .as_ref()
            .map(|i| i.game_version.clone())
            .unwrap_or_else(|| "unknown".to_string());

        let hpp_files = discover_hpp_files(root)?;
        reporter.started(Some(hpp_files.len() as u64));

        let mut builder = parser::GraphBuilder::new();
        let mut warnings = 0u64;

        for (i, file) in hpp_files.iter().enumerate() {
            reporter.progress(
                (i + 1) as u64,
                &format!(
                    "parsing {}",
                    file.file_name().and_then(|s| s.to_str()).unwrap_or("?")
                ),
            );

            // Module name is the filename without `.hpp` extension.
            let module = file
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown")
                .to_string();

            // SDK.hpp is a manifest of #includes — skip it; the real
            // content is in per-module headers.
            if module.eq_ignore_ascii_case("SDK") {
                continue;
            }

            let src = match std::fs::read_to_string(file) {
                Ok(s) => s,
                Err(e) => {
                    reporter.warn(&format!("read {}: {e}", file.display()));
                    warnings += 1;
                    continue;
                }
            };

            let relative_file = file
                .strip_prefix(root)
                .unwrap_or(file)
                .to_string_lossy()
                .replace('\\', "/");

            let tokens = lexer::tokenize(&src);
            match parser::parse_module(&module, &relative_file, &tokens, &mut builder) {
                Ok(warns) => warnings += warns,
                Err(e) => {
                    reporter.warn(&format!("parse {}: {e}", file.display()));
                    warnings += 1;
                }
            }
        }

        reporter.finished();
        if warnings > 0 {
            reporter.warn(&format!("parser ran with {warnings} warning(s)"));
        }

        let symbols = builder.symbols;
        let relations = builder.relations;

        Ok(SymbolGraph {
            source: SourceMeta {
                parser: PARSER_NAME.to_string(),
                parser_version: PARSER_VERSION.to_string(),
                game_id,
                game_version,
                ingested_at: Utc::now(),
                sdk_root: root.to_path_buf(),
            },
            symbols,
            relations,
        })
    }
}

/// Walk the SDK root and return all `.hpp` files in deterministic order.
fn discover_hpp_files(root: &Path) -> Result<Vec<PathBuf>, ParseError> {
    let mut out = Vec::new();
    visit(root, &mut out)?;
    out.sort();
    Ok(out)
}

fn visit(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), ParseError> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            visit(&path, out)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("hpp") {
            out.push(path);
        }
    }
    Ok(())
}

/// Optional metadata file written by Dumper-7. We parse the small JSON
/// subset we care about with `serde_json::Value` rather than a typed
/// struct, since the real format isn't fully nailed down yet (see plan
/// §13 STOP #1).
#[derive(Debug, Clone)]
struct SdkInfo {
    game_name: String,
    game_version: String,
}

impl SdkInfo {
    fn read(root: &Path) -> Result<Self, ParseError> {
        let p = root.join("SDKInfo.json");
        let raw = std::fs::read_to_string(&p)?;
        let v: serde_json::Value = serde_json::from_str(&raw).map_err(|e| ParseError::Syntax {
            file: p.to_string_lossy().into_owned(),
            line: 0,
            message: format!("invalid SDKInfo.json: {e}"),
        })?;
        let game_name = v
            .get("GameName")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("UnknownGame")
            .to_string();
        let game_version = v
            .get("GameVersion")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        Ok(Self {
            game_name,
            game_version,
        })
    }
}
