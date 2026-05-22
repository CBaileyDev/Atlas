# Changelog

All notable changes to Codex Atlas land here. Format loosely follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning is SemVer-ish until 1.0 (anything before 0.1 may break).

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
