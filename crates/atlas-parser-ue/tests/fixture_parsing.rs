//! Integration tests against the synthetic fixtures under
//! `fixtures/synthetic/tiny-game-v{1,2}/`.
//!
//! These exercise the parser end-to-end via the `SdkParser` trait so we
//! catch regressions in the public API, not just the internals.

use std::path::PathBuf;

use atlas_parser_trait::{NullReporter, SdkParser, SymbolKind};
use atlas_parser_ue::Dumper7Parser;

fn fixture_path(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates/
    p.pop(); // workspace root
    p.push("fixtures");
    p.push("synthetic");
    p.push(name);
    p
}

#[test]
fn tiny_game_v1_parses_with_expected_top_level_counts() {
    let parser = Dumper7Parser::new();
    let root = fixture_path("tiny-game-v1");
    assert!(parser.can_handle(&root), "parser should claim v1 fixture");
    let g = parser.parse(&root, &NullReporter).expect("parse");
    assert_eq!(g.source.game_id, "TinyGame");
    assert_eq!(g.source.game_version, "1.0.0");
    let classes: Vec<_> = g
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Class)
        .collect();
    assert_eq!(
        classes.len(),
        5,
        "expected 5 classes (UObject, AActor, APawn, AItem, APlayer), got {classes:?}"
    );

    let enums: Vec<_> = g
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Enum)
        .collect();
    assert_eq!(enums.len(), 1);

    let enum_values: Vec<_> = g
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::EnumValue)
        .collect();
    assert_eq!(enum_values.len(), 4);
}

#[test]
fn tiny_game_v1_captures_field_offsets() {
    let parser = Dumper7Parser::new();
    let g = parser
        .parse(&fixture_path("tiny-game-v1"), &NullReporter)
        .expect("parse");
    let speed = g
        .symbols
        .iter()
        .find(|s| s.fqn == "TinyGame.APawn.Speed")
        .expect("APawn.Speed");
    assert_eq!(speed.offset, Some(0x40));
    assert_eq!(speed.size, Some(4));
    assert_eq!(speed.kind, SymbolKind::Field);
}

#[test]
fn tiny_game_v1_captures_vtable_slots() {
    let parser = Dumper7Parser::new();
    let g = parser
        .parse(&fixture_path("tiny-game-v1"), &NullReporter)
        .expect("parse");
    let tick = g
        .symbols
        .iter()
        .find(|s| s.fqn == "TinyGame.AActor.Tick")
        .expect("AActor.Tick");
    assert_eq!(tick.vtable_slot, Some(0));
    assert!(tick.flags.virtual_fn, "Tick should be virtual");

    let begin_play = g
        .symbols
        .iter()
        .find(|s| s.fqn == "TinyGame.AActor.BeginPlay")
        .expect("BeginPlay");
    assert_eq!(begin_play.vtable_slot, Some(1));
}

#[test]
fn tiny_game_v1_captures_enum_values() {
    let parser = Dumper7Parser::new();
    let g = parser
        .parse(&fixture_path("tiny-game-v1"), &NullReporter)
        .expect("parse");
    let red = g
        .symbols
        .iter()
        .find(|s| s.fqn == "TinyGame.EColor.Red")
        .expect("EColor.Red");
    assert_eq!(red.offset, Some(0));
    let yellow = g
        .symbols
        .iter()
        .find(|s| s.fqn == "TinyGame.EColor.Yellow")
        .expect("EColor.Yellow");
    assert_eq!(yellow.offset, Some(3));
}

#[test]
fn tiny_game_v1_records_inheritance_relation() {
    use atlas_parser_trait::RelationKind;
    let parser = Dumper7Parser::new();
    let g = parser
        .parse(&fixture_path("tiny-game-v1"), &NullReporter)
        .expect("parse");

    let apawn = g
        .symbols
        .iter()
        .find(|s| s.fqn == "TinyGame.APawn")
        .expect("APawn");
    let aactor = g
        .symbols
        .iter()
        .find(|s| s.fqn == "TinyGame.AActor")
        .expect("AActor");

    let found = g.relations.iter().any(|r| {
        r.from == apawn.local_id && r.to == aactor.local_id && r.kind == RelationKind::Inherits
    });
    assert!(found, "APawn should inherit from AActor");
}

#[test]
fn tiny_game_v2_has_new_class_and_renamed_class() {
    let parser = Dumper7Parser::new();
    let g = parser
        .parse(&fixture_path("tiny-game-v2"), &NullReporter)
        .expect("parse");
    assert!(
        g.symbols.iter().any(|s| s.fqn == "TinyGame.ATrap"),
        "v2 should add ATrap"
    );
    assert!(
        g.symbols.iter().any(|s| s.fqn == "TinyGame.APickup"),
        "v2 should have APickup (renamed from AItem)"
    );
    assert!(
        !g.symbols.iter().any(|s| s.fqn == "TinyGame.AItem"),
        "v2 should NOT have AItem (renamed)"
    );
}

#[test]
fn tiny_game_v2_has_speed_type_substituted_to_float() {
    let parser = Dumper7Parser::new();
    let g = parser
        .parse(&fixture_path("tiny-game-v2"), &NullReporter)
        .expect("parse");
    let speed = g
        .symbols
        .iter()
        .find(|s| s.fqn == "TinyGame.APawn.Speed")
        .expect("Speed");
    match speed.type_ref.as_ref().expect("type_ref") {
        atlas_parser_trait::TypeRef::Builtin { name, .. } => assert_eq!(name, "float"),
        other => panic!("expected Builtin(float), got {other:?}"),
    }
}

#[test]
fn tiny_game_v1_no_warnings_on_clean_fixture() {
    let parser = Dumper7Parser::new();
    let r = atlas_parser_trait::CollectingReporter::new();
    let _ = parser
        .parse(&fixture_path("tiny-game-v1"), &r)
        .expect("parse");
    let warnings = r.take_warnings();
    // We expect zero "real" warnings (the final "ran with N warnings"
    // summary line counts as zero when N is zero, so it shouldn't fire).
    assert!(
        warnings.is_empty(),
        "expected no parser warnings, got: {warnings:?}"
    );
}
