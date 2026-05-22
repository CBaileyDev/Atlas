//! Tauri IPC command registrations.
//!
//! Each submodule contributes one or more `#[tauri::command]` functions.
//! Phase 0 ships only `ping` so the frontend can prove the bridge works;
//! commands for dumps, symbols, search, diff, and export land in later
//! phases under `dumps.rs`, `symbols.rs`, `search.rs`, `diff.rs`,
//! `export.rs` respectively.

pub mod dumps;
pub mod ping;
pub mod search;
