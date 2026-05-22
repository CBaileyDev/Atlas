# 2. Locked technology stack

- **Status:** Accepted
- **Date:** 2026-05-22
- **Deciders:** Carter Bailey

## Context

A single-user local desktop app with a 200k-symbol working set, fuzzy search, structural diffing, and a code-export pipeline has many plausible stacks. Picking the stack while implementing it costs days of churn; picking it up front and committing costs an hour of deliberation. The plan (`CODEX_ATLAS_PLAN.md` §2) locks the stack on purpose.

## Decision

Adopt the stack locked in the plan:

| Concern | Tech |
|---|---|
| Backend language | Rust stable, edition 2021 |
| Desktop framework | Tauri 2 |
| Frontend | React 19 + TypeScript 5 strict + Vite 6 |
| Styling | Tailwind CSS v4 |
| State | Zustand 5 |
| UI primitives | Radix UI primitives + hand-rolled components (no shadcn CLI) |
| Storage | SQLite via `rusqlite` 0.32 + `refinery` migrations |
| Search | `tantivy` 0.22 |
| Templating | `tera` 1.x |
| Async | `tokio` multi-thread |
| Errors | `thiserror` in libs, `anyhow` only at the binary edge |
| Logging | `tracing` + `tracing-subscriber` (JSON to file, pretty to stderr) |
| Hashing | `blake3` for IDs, `xxhash-rust` for fingerprints |
| Testing (Rust) | built-in `#[test]` + `insta` + `proptest` |
| Testing (TS) | `vitest` 3 + `@testing-library/react` 16 |

Local additions made during Phase 0:

- **`@tailwindcss/vite` plugin**, not the PostCSS path: Tailwind v4 prefers the dedicated Vite plugin.
- **`vite-tsconfig-paths`** for `@/*` path alias.
- **`vitest` >= 3.0**, not 2.x: Vitest 2 pulls in Vite 5 types and produces type-conflict noise against Vite 6.

## Consequences

**Positive**

- Every component has a known maintainer and a known release cadence.
- No bikeshedding mid-phase. Decisions like "use SQLite via rusqlite" or "use Tantivy" are settled.
- Plays to user strengths: Carter has trainer/game-tooling background; the stack reads as imperative-C#-adjacent (Rust + React + Tauri) rather than functional-experiment territory.

**Negative**

- Locking in costs flexibility. If, for example, Tantivy churns its API in Phase 2, we can't trivially swap to MeiliSearch without an ADR superseding this one.
- Tailwind v4 is new (2025); the ecosystem (Tailwind UI, Catalyst, etc.) is still catching up to v4-native APIs.
- React 19 is new enough that some libraries (notably some animation libraries) are still publishing alphas. We pin framer-motion 12 to dodge that.

## Alternatives considered

- **Electron + Node backend.** Rejected: heavier runtime, weaker SQLite/Tantivy interop, no Rust ecosystem at the storage layer.
- **Webview2-only + .NET backend (WPF-style).** Rejected: macOS becomes a second-class platform immediately; Rust's parser/SQLite/Tantivy story is stronger than .NET's for this workload.
- **`sqlx` instead of `rusqlite`.** Rejected: `sqlx`'s strength is compile-time-checked queries against a live DB, which we don't need for a local-only app; `rusqlite` with synchronous calls behind `spawn_blocking` is simpler.
- **Svelte/SolidJS instead of React.** Rejected: smaller hiring/AI-pair-programming pool, fewer Radix-equivalent primitives. Not enough upside for a personal-use app.

## Links

- Plan §2 (Locked decisions)
- [Tailwind v4 release notes](https://tailwindcss.com/blog/tailwindcss-v4)
- [Tauri 2 stable announcement](https://tauri.app/blog/tauri-20/)
