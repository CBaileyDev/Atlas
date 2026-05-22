//! Dumper-7 parser for Unreal Engine SDK dumps.
//!
//! Format notes (verified against a real Borderlands 4 / OakGame
//! Dumper-7 output, 2026-05-22):
//!
//! - The SDK root may either be the dump folder itself (synthetic
//!   fixtures live this way) **or** a parent that contains a
//!   `CppSDK/SDK/` subdirectory full of per-package `.hpp` files
//!   (the real Dumper-7 layout).
//! - Metadata lives at `_SDKInfo.json` (Dumper-7) or `SDKInfo.json`
//!   (legacy / synthetic).
//! - Each package emits up to four files:
//!   - `<Pkg>_classes.hpp`     — `class` declarations
//!   - `<Pkg>_structs.hpp`     — `struct` / `enum class` declarations
//!   - `<Pkg>_parameters.hpp`  — `struct` per function (skipped, Phase 1)
//!   - `<Pkg>_functions.cpp`   — function bodies (skipped — `.cpp`)
//! - All real content sits in `namespace SDK { ... }` (or
//!   `namespace SDK::Params { ... }`). The wrapper is **not** part of
//!   the FQN.
//! - Canonical FQN lives in a comment header above the declaration:
//!   `// Class Engine.Actor`
//!   `// ScriptStruct CoreUObject.Vector`
//!   `// Enum Engine.EWorldType`
//!   `// Function Engine.Actor.K2_DestroyActor`
//! - `Basic.hpp` is template-/macro-heavy infrastructure with no real
//!   game symbols; we skip it by name.
//!
//! The parser is intentionally **NOT** a full C++ parser — we recognize
//! just the shapes Dumper-7 emits.

use std::path::{Path, PathBuf};

use atlas_parser_trait::{ParseError, Reporter, SdkParser, SourceMeta, SymbolGraph};
use chrono::Utc;

mod lexer;
mod parser;

pub const PARSER_NAME: &str = "dumper7-ue";
pub const PARSER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Files we always skip, by basename.
const SKIP_FILENAMES: &[&str] = &[
    "Basic.hpp",
    "PropertyFixup.hpp",
    "UnrealContainers.hpp",
    "UtfN.hpp",
];

/// Filename suffixes (before `.hpp`) we always skip in Phase 1.
const SKIP_SUFFIXES: &[&str] = &["_parameters"];

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

    fn can_handle(&self, root: &Path) -> bool {
        if !root.is_dir() {
            return false;
        }
        find_sdk_dir(root).is_some() || root.join("SDK.hpp").exists()
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

        // Find the directory that actually holds the per-package .hpp
        // files. For real Dumper-7 this is `<root>/CppSDK/SDK`; for
        // the synthetic fixture it's just `<root>` itself.
        let sdk_dir = find_sdk_dir(root).unwrap_or_else(|| root.to_path_buf());

        let hpp_files = discover_hpp_files(&sdk_dir)?;
        reporter.started(Some(hpp_files.len() as u64));

        let mut builder = parser::GraphBuilder::new();
        let mut warnings = 0u64;

        for (i, file) in hpp_files.iter().enumerate() {
            let stem = file
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown");
            let basename = file
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown");

            // Filename filters.
            if SKIP_FILENAMES
                .iter()
                .any(|n| n.eq_ignore_ascii_case(basename))
            {
                continue;
            }
            if SKIP_SUFFIXES.iter().any(|suf| stem.ends_with(suf)) {
                continue;
            }
            // SDK.hpp is a manifest of #includes.
            if stem.eq_ignore_ascii_case("SDK") {
                continue;
            }

            reporter.progress((i + 1) as u64, &format!("parsing {basename}"));

            let module = module_name_from_stem(stem);

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
            match parser::parse_file(&module, &relative_file, &tokens, &mut builder) {
                Ok(warns) => warnings += warns,
                Err(e) => {
                    reporter.warn(&format!("parse {}: {e}", file.display()));
                    warnings += 1;
                }
            }
        }

        builder.resolve_references();

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

/// Locate the directory holding the per-package `.hpp` files.
/// Order: `<root>/CppSDK/SDK`, then `<root>/SDK`, else `None`.
fn find_sdk_dir(root: &Path) -> Option<PathBuf> {
    let candidate = root.join("CppSDK").join("SDK");
    if candidate.is_dir() {
        return Some(candidate);
    }
    let candidate = root.join("SDK");
    if candidate.is_dir() {
        return Some(candidate);
    }
    None
}

/// `ACLPlugin_classes` -> `ACLPlugin`. `Engine_structs` -> `Engine`.
/// Identity if no recognized suffix.
fn module_name_from_stem(stem: &str) -> String {
    for suf in ["_classes", "_structs", "_functions", "_parameters"] {
        if let Some(prefix) = stem.strip_suffix(suf) {
            return prefix.to_string();
        }
    }
    stem.to_string()
}

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

/// Optional metadata file written by Dumper-7. Both `_SDKInfo.json`
/// (Dumper-7) and `SDKInfo.json` (legacy / synthetic) layouts are
/// supported.
#[derive(Debug, Clone)]
struct SdkInfo {
    game_name: String,
    game_version: String,
}

impl SdkInfo {
    fn read(root: &Path) -> Result<Self, ParseError> {
        for candidate in ["_SDKInfo.json", "SDKInfo.json"] {
            let p = root.join(candidate);
            if let Ok(raw) = std::fs::read_to_string(&p) {
                let v: serde_json::Value =
                    serde_json::from_str(&raw).map_err(|e| ParseError::Syntax {
                        file: p.to_string_lossy().into_owned(),
                        line: 0,
                        message: format!("invalid {candidate}: {e}"),
                    })?;

                // Try the Dumper-7 nested shape first:
                //   { "game": { "name": "...", "version": "..." } }
                if let Some(game) = v.get("game") {
                    let name = game
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("UnknownGame")
                        .to_string();
                    let version = game
                        .get("version")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("unknown")
                        .to_string();
                    return Ok(Self {
                        game_name: name,
                        game_version: version,
                    });
                }

                // Legacy / synthetic flat shape:
                //   { "GameName": "...", "GameVersion": "..." }
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
                return Ok(Self {
                    game_name,
                    game_version,
                });
            }
        }
        Err(ParseError::Io(format!(
            "no SDKInfo.json or _SDKInfo.json under {}",
            root.display()
        )))
    }
}
