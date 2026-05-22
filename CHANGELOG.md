# Changelog

All notable changes to Codex Atlas land here. Format loosely follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning is SemVer-ish until 1.0 (anything before 0.1 may break).

## [0.1.0] — 2026-05-22

### Added
- Phase 1 — Ingest. End-to-end parse → SQLite for Dumper-7 Unreal Engine SDK dumps.
- `atlas-parser-trait`: full `SymbolGraph` data model from plan §4.1 — `Symbol`, `SymbolKind`, `Relation`, `RelationKind`, `TypeRef` (Local/Builtin/Unresolved), `TypeModifiers`, `SymbolFlags`, `SourceLoc`. `Reporter` trait with `NullReporter` and `CollectingReporter`.
- `atlas-parser-ue`: hand-written lexer + recursive-descent parser (~1100 lines). Handles `namespace SDK { ... }` wrapper, `alignas(N)`, `final`, `class`/`struct` type prefixes, `Foo::Bar::Baz` namespace-qualified types, inline method bodies (`{ STATIC_CLASS_IMPL(...) }`), top-level macro statements (`DUMPER7_ASSERTS_X;`), `_classes.hpp` / `_structs.hpp` packaging, `_SDKInfo.json` and legacy `SDKInfo.json`, FQN extraction from `// Class Pkg.Name` header comments.
- `atlas-core::storage::ingest`: writes a `SymbolGraph` into SQLite. 16-byte BLAKE3 IDs scoped by `(fqn, kind, dump_id)`. Idempotent re-ingest. `IngestReport` carries counts and warnings.
- `src-tauri::commands::ingest_dump`: Tauri IPC command running parse + ingest on a `spawn_blocking` thread. `TauriReporter` translates the parser's progress callbacks into `ingest:started` / `ingest:progress` / `ingest:warn` / `ingest:finished` events for the frontend.
- Synthetic fixtures at `fixtures/synthetic/tiny-game-v{1,2}/` covering all five v1→v2 change categories from plan §14.1 (added class, removed field, offset shift, rename, type substitution).
- Real-fixture verification against Borderlands 4 OakGame v5.5.4 (1538 .hpp files): 148,211 symbols, 204,487 relations, parse 900 ms, ingest 1.3 s — both well under the §12.2 budgets.

### Acceptance gate (plan §7)
- `cargo test --workspace`: **41 tests** pass; two real-fixture tests pass under `--ignored`.
- `cargo clippy --workspace -- -D warnings`: clean.
- `cargo fmt --check`: clean.
- Re-ingest is idempotent (verified against the real fixture; 0 inserts on second run).

## [0.2.0] — 2026-05-22

### Added
- Phase 2 — Browse + Search. End-to-end "open dump → fuzzy search → symbol detail" flow on a Tantivy 0.22 index.
- `atlas-core::search::DumpIndex` with a per-dump Tantivy index, schema versioned via a `.schema-version` marker file (auto-rebuilds on mismatch). Fields: `id` (16-byte BLOB), `fqn` (TEXT + STORED), `name` (TEXT), `kind_i` (FAST + INDEXED + STORED), `module` (STRING + FAST + STORED), `parent_name` (TEXT). Free-text query is AND-combined with `SearchFacets { kinds, modules }`.
- `atlas-core::search::lookup_symbol` for hydrating a hit into a full `SymbolRow`.
- Tauri IPC commands: `list_dumps`, `open_dump` (idempotently builds the index), `search_symbols`, `get_symbol`, `list_members`.
- React Browse route (`src/routes/browse.tsx`) wired to the new commands: dump selector, kind + module facets, debounced search input, hit list with selection, symbol detail panel with size/offset/vtable + member table.
- `src/ipc/client.ts` and `src/ipc/types.ts` extended with the full search surface.

### Acceptance gate (plan §8 partial — see TASKS.md for deferred items)
- `cargo test --workspace`: **44 tests** pass (added 3 search tests in `atlas-core`).
- `cargo clippy --workspace -- -D warnings`: clean.
- `pnpm typecheck`, `pnpm test`, `pnpm build`: clean.

### Deferred to TASKS.md
- Virtualized table (current flat render is fine at 200-row limit but doesn't future-proof).
- Cmd-K command palette (the inline search input covers the data path).
- Type-ref hyperlinks in the detail panel.
- Keyboard navigation (↑/↓/Enter/Esc) in the hit list.
- Cross-reload state persistence via Zustand.

## [0.3.0] — 2026-05-22

### Added
- Phase 3 — Diff engine. Pure-function structural diff between two `SymbolGraph`s of the same game (plan §9 invariant 3).
- `atlas-core::diff::diff(base, head, config, overrides) -> Diff` with the three documented passes:
  - Pass 1: exact match by `(kind, fqn)`.
  - Pass 2: fingerprint rename detection (Jaccard on member names + types, scaled by same-module / same-parent bonuses, padding fields filtered out via `fingerprint_ignore_prefixes`).
  - Pass 3: field-level classification — `OffsetChanged`, `SizeChanged`, `VtableShift`, `ParentClassChanged`, `FieldAdded`, `FieldRemoved`, `FunctionSignatureChanged`, `FieldTypeSubstituted`.
- `DiffConfig` with the plan's documented defaults (0.70 suggestion / 0.90 confidence thresholds, 0.6 name / 0.3 type / +0.1 module weights).
- `RenameOverride` with `Decision::Match` / `Decision::Reject` — `Match` short-circuits scoring, `Reject` excludes the pair from suggestions.
- 10 diff unit tests covering all seven change categories plus override behavior and JSON round-trip.
- IPC commands `diff_dumps` and `diff_dumps_with_overrides`, both running on `spawn_blocking`. Dumps are rehydrated from SQLite into `SymbolGraph` (avoids re-parsing the SDK every diff).
- React Diff route with base/head selectors, filter chips for matches / added / removed / renamed / fields, and a flat change list that decodes `ChangeKind` to readable lines.

### Acceptance gate (plan §9 — partial; deferred items in TASKS.md)
- 54 workspace tests pass. `cargo clippy --workspace -- -D warnings` clean. `pnpm typecheck/test/build` clean.
- All seven change categories covered by unit tests against the synthetic v1/v2 fixtures.
- Diff output round-trips through JSON.

### Deferred to TASKS.md
- `insta` snapshot files (current behavior is asserted with field-by-field unit tests; snapshots are nice-to-have).
- `cargo bench` for diff against real-fixture-scale dumps.
- Side-by-side detail panel for matched pairs.
- Inline rename Confirm/Reject UI (the IPC command + `RenameOverride` shape are ready).
- Diff JSON caching at `diffs/<base>-<head>.json`.

## [0.4.0] — 2026-05-22

### Added
- Phase 4 — Export. Symbol selection → Tera template → on-disk scaffold with `_atlas.json` sidecar (plan §10).
- `atlas-core::export` module:
  - `Project`, `Selection`, `SelectionRules` shape (selection rules wired through; transitive closure rules not yet applied — see TASKS.md).
  - `ExportContext` + `SymbolView` view-model; both decimal and `0xHEX` forms surfaced for offsets and vtable slots so templates don't have to format themselves.
  - `build_context` hydrates the symbol set from SQLite and computes each field's parent FQN by walking `Contains` relations.
  - `render_to_string` runs Tera against a single template by source text.
  - `AtlasSidecar` (`_atlas.json`): atlas version, exported_at, game/version/dump ids, template name, BLAKE3-hashed template version, selection rules. Round-trips through JSON.
  - Template registry (`templates::available_templates` / `load_template`): bundled via `include_str!`, override via `<data>/templates/<name>.tera`.
- Bundled templates:
  - `Trainer.cs.tera` — single-file C# console trainer matching the 2HighInternal reference style (P/Invoke OpenProcess/ReadProcessMemory/WriteProcessMemory, `CheatEntry` registry, freeze threads, console UI).
  - `Offsets.h.tera` — flat C++ header of `static constexpr` offsets, grouped by class with vtable slots.
  - `IDA-Mapping.txt.tera` — tab-separated fqn / offset / size triples.
  - `Sigscan.txt.tera` — placeholder signature stubs by fqn.
- IPC commands: `list_templates`, `resolve_fqns` (FQN → 16-byte id), `render_export_preview`, `write_export`.
- React Export route (`src/routes/export.tsx`): dump selector, template picker, project/class/process inputs, paste-FQNs textarea with live resolve feedback, debounced 200 ms preview, write-to-disk via `@tauri-apps/plugin-dialog` open(directory).
- 4 export integration tests (Trainer / Offsets / IDA mapping render correctness, sidecar JSON round-trip).
- Trainer reference at `fixtures/real/trainer-reference/2HighInternal/` (source files only, build artifacts excluded).

### Acceptance gate (plan §10 — partial)
- 60 Rust workspace tests + 2 Vitest tests pass.
- `cargo clippy --workspace -- -D warnings`, `cargo fmt --check`, `pnpm typecheck/test/build` all clean.
- Trainer scaffold contains the expected structural marks (P/Invoke block, `CheatEntry`, `cheats.Add` calls with the BaseOffset TODO).
- Sidecar JSON round-trips and includes a `blake3:` template hash.
- `_atlas.json` written next to the rendered artifact, both paths returned to the frontend.

### Deferred (in TASKS.md)
- `dotnet build` compile-check in CI for the rendered C# (plan §10 lists this as optional).
- Right-click "Copy as…" submenu in the Browse route.
- Selection-rules transitive closure (`include_parents`, `type_depth`) — the wire format is ready, the resolver isn't.
- `Snippets/csharp_struct.tera` and `Snippets/cheat_engine_chain.tera` — not yet authored.

## [Unreleased]

### Added
- Phase 0 — Foundation. App launches, calls the `ping` IPC command, renders a connection badge plus four placeholder routes.
- Cargo workspace with four crates: `atlas-core`, `atlas-parser-trait`, `atlas-parser-ue`, `atlas-parser-unity`.
- SQLite migration `V0001` (six tables — `dumps`, `symbols`, `relations`, `symbol_links`, `rename_overrides`, `projects`) with WAL mode and `synchronous=NORMAL`.
- Tauri 2 shell with the `ping` command, layered tracing (pretty stderr + JSON daily-rotated file), and a locked-down CSP.
- React 19 + Vite 6 + Tailwind v4 frontend, dark by default, with `@/*` path alias.
- 14 Rust tests + 2 Vitest tests, all green.
- ADRs 0001 (MADR adoption), 0002 (locked stack), 0003 (per-dump symbol identity), 0004 (`AppError` wraps `AtlasError`).
- GitHub Actions CI: `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, `pnpm typecheck`, `pnpm test`, `pnpm build`.

## [0.0.0] — 2026-05-22

Initial commit. Plan and meta files.
