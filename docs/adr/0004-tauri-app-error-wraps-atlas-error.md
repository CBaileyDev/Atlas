# 4. The Tauri shell defines its own `AppError` that wraps `AtlasError`

- **Status:** Accepted
- **Date:** 2026-05-22
- **Deciders:** Carter Bailey

## Context

The plan (§12.4) defines `AtlasError` in `atlas-core` for everything the core engines can fail at: parser, storage, search, diff, export, io, invalid input, not found. Tauri IPC commands return `Result<T, _>`, and that `_` has to be serde-serializable.

We could either:

1. Use `AtlasError` directly as the IPC error type.
2. Wrap it in a thin shell-layer error that can also carry Tauri-runtime errors (failed `emit`, missing window label, etc.).

## Decision

Define `AppError` in `src-tauri/src/error.rs`:

```rust
#[derive(Error, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "message")]
pub enum AppError {
    #[error(transparent)] Atlas(#[from] AtlasError),
    #[error("tauri: {0}")] Tauri(String),
    #[error("internal: {0}")] Internal(String),
}
```

All IPC commands return `Result<T, AppError>`. The `From<AtlasError>` impl is automatic via `#[from]`, so commands that only fail in core ways can still write `core_call()?` and let `?` do the conversion.

## Consequences

**Positive**

- The IPC boundary owns its own error shape. If Tauri introduces a new failure mode (e.g. webview crash), we add a variant here without polluting `atlas-core`.
- The frontend's `AppError` TypeScript type stays in one place (`src/ipc/types.ts`) and maps 1:1 to the Rust enum because `#[serde(tag, content)]` produces the same shape both sides recognize.
- Core code stays portable. A future `atlas-cli` binary can use `AtlasError` directly without dragging in any Tauri concepts.

**Negative**

- Two error types to remember. Slightly more boilerplate in command signatures (`AppResult<T>` vs `AtlasResult<T>`).
- One extra `From` impl to maintain when `AtlasError` gets new variants.

## Alternatives considered

- **`AtlasError` everywhere, including IPC.** Rejected: forces every Tauri-runtime failure to be stuffed into one of `AtlasError`'s variants (probably `Io(String)`), which loses the "this isn't a core engine failure, it's a desktop-shell failure" distinction in logs and dev tools.
- **A single `anyhow::Error` at the IPC boundary.** Rejected: `anyhow` is great inside binaries but doesn't serialize cleanly, and we want the frontend to be able to discriminate on error kind to decide whether to retry, show a toast, or fall through to a generic dialog.

## Links

- Plan §12.4 (error model)
- `crates/atlas-core/src/error.rs`
- `src-tauri/src/error.rs`
