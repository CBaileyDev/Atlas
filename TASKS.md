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
  wired but only invokable from devtools or a test harness. A small
  "ingest button" UI is the cheapest add for the Settings route.
- `[Phase 2]` Virtualized table for the Browse hit list. Current
  implementation renders flat — fine at the 200-row default limit,
  but the plan §8 budget asks for sub-50 ms keystroke response on
  <1000 rows, and a virtualizer (tanstack-react-virtual is already
  installed) is the cleanest way to keep that under any limit.
- `[Phase 2]` Cmd-K command palette using `cmdk`. Search input
  already handles the data flow; this is a UX add, not new plumbing.
- `[Phase 2]` Type-ref hyperlinks in the detail panel — click a
  field's type to jump to that symbol. Requires resolving the
  `type_ref_json` blob in the frontend and dispatching to the same
  selection state.
- `[Phase 2]` Keyboard navigation in the hit list (↑/↓/Enter/Esc).
- `[Phase 2]` Zustand store wiring: persist last selected dump,
  query, and symbol across reloads.

## Conventions

- Use `[Phase N]` tag at the start of each entry.
- If a task is blocked on user input, prefix with `🛑`.
- If a task is a known limitation rather than a TODO, prefix with `⚠`.
- Remove the line on completion. The git history is the audit trail.
