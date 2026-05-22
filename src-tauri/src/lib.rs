//! Tauri 2 desktop shell for Codex Atlas.
//!
//! Tauri 2 conventionally puts the application entry point in `lib.rs`
//! (so it can be shared with mobile targets later). `main.rs` is a thin
//! shim that just calls into `run()`.
//!
//! Phase 0 ships a single IPC command (`ping`) so the frontend can prove
//! the bridge is wired correctly. Real commands land in later phases
//! under `commands/`.

mod commands;
mod error;
mod observability;
mod watcher;

pub use error::AppError;

/// Entry point. Initializes tracing, builds the Tauri runtime, and runs
/// the application.
///
/// # Panics
///
/// Panics if Tauri fails to start the application (which is the standard
/// pattern in Tauri 2 — the application can't recover from this).
pub fn run() {
    let _guard = observability::init();

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "Codex Atlas starting");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::ping::ping,
            commands::dumps::ingest_dump,
            commands::search::list_dumps,
            commands::search::open_dump,
            commands::search::search_symbols,
            commands::search::get_symbol,
            commands::search::list_members,
            commands::diff::diff_dumps,
            commands::diff::diff_dumps_with_overrides,
            commands::export::list_templates,
            commands::export::resolve_fqns,
            commands::export::render_export_preview,
            commands::export::write_export,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::add_watcher_root,
            commands::settings::remove_watcher_root,
        ])
        .setup(|app| {
            tracing::info!("application setup complete");
            watcher::spawn(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Codex Atlas");
}
