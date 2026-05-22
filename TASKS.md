# TASKS

This file tracks **deferred work** — anything we deliberately did not finish during a phase, that needs to be picked up later. The plan's `CODEX_ATLAS_PLAN.md` describes the *intended* roadmap; this file describes the *outstanding* items inside it.

Format: one line per deferred task. Add a phase tag and a one-sentence reason. Remove the line when the work lands.

## Outstanding

- `[Phase 1]` Property tests for the ingest pipeline (proptest round-trip
  of random `SymbolGraph` through SQLite). Plan §7 acceptance lists this;
  deferred to keep Phase 1's STOP cadence tight.
- `[Phase 1]` Snapshot tests for parser output using `insta`. Add once
  the parser is locked against a real fixture so the snapshots reflect
  verified ground truth, not a guess.
- `[Phase 1]` `cargo bench` target for ingest. Plan §7 asks for <100ms
  on the synthetic fixture; deferred — we haven't measured yet.
- `[Phase 1]` 🛑 Real Dumper-7 fixture from Carter required at
  `fixtures/real/<game>-<version>/` before merging Phase 1 to main and
  cutting v0.1.0. See plan §13 STOP #1.
- ⚠ Frontend has no ingest UI yet. The `ingest_dump` IPC command is
  wired but only invokable from devtools or a test harness. The Browse
  route gets that UI in Phase 2.
- ⚠ Cross-module parent-class linkage isn't implemented. The synthetic
  fixture only has one module, so this hasn't surfaced; will need to
  handle multi-module inheritance when real Fortnite-style dumps arrive
  (AActor in Engine.hpp, AFortPlayerController in FortniteGame.hpp,
  etc.).

## Conventions

- Use `[Phase N]` tag at the start of each entry.
- If a task is blocked on user input, prefix with `🛑`.
- If a task is a known limitation rather than a TODO, prefix with `⚠`.
- Remove the line on completion. The git history is the audit trail.
