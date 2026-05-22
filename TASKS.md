# TASKS

This file tracks **deferred work** — anything we deliberately did not finish during a phase, that needs to be picked up later. The plan's `CODEX_ATLAS_PLAN.md` describes the *intended* roadmap; this file describes the *outstanding* items inside it.

Format: one line per deferred task. Add a phase tag and a one-sentence reason. Remove the line when the work lands.

## Outstanding

- `[Phase 1]` Property tests for the ingest pipeline (proptest round-trip
  of random `SymbolGraph` through SQLite). Nice-to-have; the
  real-fixture integration test exercises the path end-to-end.
- `[Phase 1]` Snapshot tests for parser output using `insta`. Useful
  once the synthetic fixture's expected shape stops drifting.
- `[Phase 1]` `cargo bench` target for ingest. Real-fixture timing
  (parse 900 ms, ingest 1.3 s) already proves we're well inside §12.2;
  formal `cargo bench` would lock in regression detection.
- `[Phase 1]` Parser still emits ~12k internal warnings on the
  148k-symbol Borderlands 4 dump (mostly `peek_decl_shape == Unknown`
  on shapes we skip rather than parse). Within tolerance per plan §7
  ("tolerant of minor format drift — log, skip, continue") but worth
  bucketing by cause once the diff engine surfaces real impact.
- ⚠ Frontend has no ingest UI yet. The `ingest_dump` IPC command is
  wired but only invokable from devtools or a test harness. The Browse
  route gets that UI in Phase 2.

## Conventions

- Use `[Phase N]` tag at the start of each entry.
- If a task is blocked on user input, prefix with `🛑`.
- If a task is a known limitation rather than a TODO, prefix with `⚠`.
- Remove the line on completion. The git history is the audit trail.
