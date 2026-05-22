# Codex Atlas

Single-user desktop app for ingesting game SDK dumper output, browsing the resulting symbol graph, diffing versions, and exporting trainer scaffolds.

Local-only. No accounts. No network.

## Stack

- Rust (stable) + Tauri 2
- React 19 + TypeScript 5 + Vite 6 + Tailwind v4
- SQLite via rusqlite + refinery migrations
- Tantivy for search, Tera for export templates

The full plan is in [`CODEX_ATLAS_PLAN.md`](./CODEX_ATLAS_PLAN.md). Architecture decisions are recorded under [`docs/adr/`](./docs/adr/).

## Run locally

Prerequisites: Rust stable, Node 20+, pnpm 9+, and on Windows the MSVC build tools plus WebView2 runtime (preinstalled on Windows 11).

```
pnpm install
pnpm tauri dev
```

## Tests and checks

```
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
pnpm typecheck
pnpm test
pnpm build
```

## Build

```
pnpm tauri build
```

Produces an MSI on Windows under `src-tauri/target/release/bundle/`.

## Layout

```
crates/             Rust workspace (atlas-core, parsers)
src-tauri/          Tauri shell + IPC commands
src/                React frontend
fixtures/           Test fixtures (synthetic checked in, real gitignored)
docs/adr/           Architecture decision records
scripts/            Dev tooling
```

## Phases

- Phase 0: Foundation
- Phase 1: Ingest (Dumper-7 parser)
- Phase 2: Browse + Search
- Phase 3: Diff engine
- Phase 4: Export
- Phase 5: Polish + folder watcher (optional)
