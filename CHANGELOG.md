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
