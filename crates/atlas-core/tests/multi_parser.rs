//! Validates the architectural promise of `SdkParser`: a second
//! parser implementation (Unity stub) ingests through the exact
//! same `atlas-core::storage::ingest` code path as the UE parser.

use atlas_core::storage::Db;
use atlas_parser_trait::{NullReporter, SdkParser};
use atlas_parser_unity::Il2CppStubParser;

#[test]
fn unity_stub_parser_ingests_via_same_pipeline_as_ue() {
    let tmp = tempfile::tempdir().unwrap();
    let marker = tmp.path().join("il2cpp-stub.json");
    std::fs::write(
        &marker,
        r#"{"game_id":"PipelineTest","game_version":"1.0","module_name":"Assembly-CSharp","placeholder_class":"PlayerController"}"#,
    )
    .unwrap();

    let parser = Il2CppStubParser::new();
    let graph = parser.parse(tmp.path(), &NullReporter).unwrap();
    assert_eq!(graph.source.parser, "il2cpp-unity");

    let mut db = Db::open_in_memory().unwrap();
    let report = db.ingest(&graph).unwrap();
    assert_eq!(report.parser, "il2cpp-unity");
    assert_eq!(report.symbols_inserted, 2);
    assert_eq!(report.relations_inserted, 1);
    assert_eq!(report.symbols_skipped, 0);

    // Re-ingest is idempotent for the Unity path too.
    let again = db.ingest(&graph).unwrap();
    assert_eq!(again.symbols_inserted, 0);
    assert_eq!(again.symbols_skipped, 2);
}
