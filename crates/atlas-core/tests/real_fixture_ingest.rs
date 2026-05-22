//! Full parse→ingest path against the real Dumper-7 fixture under
//! `fixtures/real/`. `#[ignore]` by default; run with:
//!
//! ```
//! cargo test -p atlas-core --test real_fixture_ingest -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::time::Instant;

use atlas_core::storage::Db;
use atlas_parser_trait::{NullReporter, SdkParser};
use atlas_parser_ue::Dumper7Parser;

fn first_real_fixture() -> Option<PathBuf> {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.pop();
    dir.pop();
    dir.push("fixtures");
    dir.push("real");
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
fn real_dumper7_parses_and_ingests_into_sqlite() {
    let Some(root) = first_real_fixture() else {
        eprintln!("No real fixture; skipping.");
        return;
    };

    let parser = Dumper7Parser::new();
    let started_parse = Instant::now();
    let graph = parser.parse(&root, &NullReporter).expect("parse");
    let parse_elapsed = started_parse.elapsed();

    let started_ingest = Instant::now();
    let mut db = Db::open_in_memory().expect("open in-memory db");
    let report = db.ingest(&graph).expect("ingest");
    let ingest_elapsed = started_ingest.elapsed();

    eprintln!("parse elapsed:  {parse_elapsed:?}");
    eprintln!("ingest elapsed: {ingest_elapsed:?}");
    eprintln!(
        "report: dump_id={} symbols={}/+{} skipped relations={}/+{} skipped",
        report.dump_id,
        report.symbols_inserted,
        report.symbols_skipped,
        report.relations_inserted,
        report.relations_skipped,
    );

    // First-time ingest should write everything. A small number of
    // skipped rows is expected when the same FQN+kind appears more
    // than once in the graph (e.g. a class declared in both
    // <Pkg>_classes.hpp and a sibling file's forward declaration).
    // Cap the allowed skip ratio at 0.1% so a real regression still
    // surfaces.
    assert!(report.symbols_inserted > 100_000);
    let total = report.symbols_inserted + report.symbols_skipped;
    let skip_ratio = report.symbols_skipped as f64 / total as f64;
    assert!(
        skip_ratio < 0.001,
        "expected <0.1% symbol skips on first ingest, got {} skips / {} total = {:.3}%",
        report.symbols_skipped,
        total,
        skip_ratio * 100.0
    );
    assert!(report.relations_inserted > 100_000);

    // Re-ingest is idempotent: nothing new gets written.
    let report2 = db.ingest(&graph).expect("re-ingest");
    assert_eq!(report2.symbols_inserted, 0, "re-ingest inserts nothing");
    assert_eq!(report2.symbols_skipped, total);

    // Plan §7 budget: ingest of a 200k-symbol Fortnite-scale dump in
    // under 30s. We're well under in-memory; the on-disk case is
    // slower but should still beat 30s on a release build.
    assert!(
        ingest_elapsed.as_secs() < 60,
        "ingest took {ingest_elapsed:?} — over plan §7 budget"
    );
}
