//! Tests against the real Dumper-7 fixture(s) under `fixtures/real/`.
//!
//! These tests are `#[ignore]` by default because:
//! - `fixtures/real/` is gitignored — CI machines don't have the data.
//! - The fixture is large (~hundreds of MB across 1500+ files) and the
//!   parse can take longer than a typical unit-test budget.
//!
//! Run with: `cargo test -p atlas-parser-ue -- --ignored --nocapture`.

use std::path::PathBuf;
use std::time::Instant;

use atlas_parser_trait::{CollectingReporter, SdkParser, SymbolKind};
use atlas_parser_ue::Dumper7Parser;

fn fixtures_real_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("fixtures");
    p.push("real");
    p
}

/// Pick the first subdirectory of `fixtures/real/`. We don't hard-code
/// a game name so a future Carter can drop a different dump in without
/// editing the test.
fn first_real_fixture() -> Option<PathBuf> {
    let dir = fixtures_real_dir();
    if !dir.is_dir() {
        return None;
    }
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().is_dir())
        .collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    entries.first().map(std::fs::DirEntry::path)
}

#[test]
#[ignore]
fn real_dumper7_parses_with_reasonable_counts() {
    let Some(root) = first_real_fixture() else {
        eprintln!("No real fixture found under fixtures/real/; skipping.");
        return;
    };
    eprintln!("Real fixture: {}", root.display());

    let parser = Dumper7Parser::new();
    assert!(
        parser.can_handle(&root),
        "parser should claim the real fixture as a Dumper-7 dump"
    );

    let reporter = CollectingReporter::new();
    let started = Instant::now();
    let graph = parser
        .parse(&root, &reporter)
        .expect("real fixture should parse");
    let elapsed = started.elapsed();

    let warnings = reporter.take_warnings();

    let modules = graph
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Module)
        .count();
    let classes = graph
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Class)
        .count();
    let structs = graph
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Struct)
        .count();
    let enums = graph
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Enum)
        .count();
    let enum_values = graph
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::EnumValue)
        .count();
    let fields = graph
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Field)
        .count();
    let functions = graph
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Function)
        .count();

    eprintln!("game_id        = {}", graph.source.game_id);
    eprintln!("game_version   = {}", graph.source.game_version);
    eprintln!("modules        = {modules}");
    eprintln!("classes        = {classes}");
    eprintln!("structs        = {structs}");
    eprintln!("enums          = {enums}");
    eprintln!("enum_values    = {enum_values}");
    eprintln!("fields         = {fields}");
    eprintln!("functions      = {functions}");
    eprintln!("relations      = {}", graph.relations.len());
    eprintln!("warnings       = {}", warnings.len());
    eprintln!("elapsed        = {elapsed:?}");
    if !warnings.is_empty() {
        eprintln!("first 10 warnings:");
        for w in warnings.iter().take(10) {
            eprintln!("  - {w}");
        }
    }

    // Sanity: a Fortnite-scale UE5 dump should have at least these orders
    // of magnitude. We assert generous lower bounds rather than exact
    // counts so the test stays stable across game updates.
    assert!(modules > 10, "expected >10 modules, got {modules}");
    assert!(classes > 100, "expected >100 classes, got {classes}");
    assert!(enums > 10, "expected >10 enums, got {enums}");
    assert!(
        fields > 1000,
        "expected >1000 fields across the whole dump, got {fields}"
    );

    // Sanity: plan §7 budget is <30s for a 200k-symbol Fortnite-scale dump.
    // OakGame is roughly the same order. Hold the line loosely (60s) to
    // account for CI / debug builds; release builds should beat <30s.
    assert!(
        elapsed.as_secs() < 60,
        "real-fixture parse took {elapsed:?} — way over plan §7 budget"
    );
}
