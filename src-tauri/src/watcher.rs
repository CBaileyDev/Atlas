//! Folder watcher (plan §11).
//!
//! Background task that watches the user's configured roots for new
//! Dumper-7-style SDK folders. When a path under a watched root
//! stays stable for `watcher_debounce_ms`, we check whether it has
//! the shape of an SDK dump (`CppSDK/SDK/` directory or `SDK.hpp`
//! at the root). If so, we emit a `watcher:dump-detected` event and
//! the frontend toasts it.
//!
//! The watcher is created once at app setup. To pick up changes to
//! the configured roots, the user has to restart the app — making
//! this hot is a future-Phase-5 improvement.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use atlas_core::settings::AtlasSettings;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

/// Event payload the frontend listens on `watcher:dump-detected`.
#[derive(Debug, Clone, Serialize)]
struct DumpDetectedEvent {
    path: String,
    watched_root: String,
}

/// Spawn the watcher in a Tokio task tied to the app handle. Returns
/// immediately. Errors (missing notify backend, no roots configured)
/// are logged via `tracing` and the task exits cleanly.
pub fn spawn(app: AppHandle) {
    tokio::spawn(async move {
        if let Err(e) = run(app).await {
            tracing::warn!(error = %e, "watcher exited with error");
        }
    });
}

async fn run(app: AppHandle) -> Result<(), String> {
    let settings = AtlasSettings::load().map_err(|e| e.to_string())?;
    if settings.watcher_roots.is_empty() {
        tracing::debug!("watcher: no roots configured; idle");
        return Ok(());
    }

    let debounce = Duration::from_millis(settings.watcher_debounce_ms.max(500));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<notify::Event>();
    let tx = Arc::new(tx);

    // notify gives us a sync API. Bridge it to our async mpsc.
    let tx_for_watcher = Arc::clone(&tx);
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res {
            let _ = tx_for_watcher.send(ev);
        }
    })
    .map_err(|e| format!("create watcher: {e}"))?;

    use notify::Watcher;
    let mut watched_roots = Vec::new();
    for root in &settings.watcher_roots {
        if !root.is_dir() {
            tracing::warn!(root = %root.display(), "watcher: root is not a directory; skipping");
            continue;
        }
        match watcher.watch(root, notify::RecursiveMode::Recursive) {
            Ok(()) => {
                tracing::info!(root = %root.display(), "watcher: now watching");
                watched_roots.push(root.clone());
            }
            Err(e) => {
                tracing::warn!(root = %root.display(), error = %e, "watcher: failed to attach");
            }
        }
    }

    if watched_roots.is_empty() {
        return Ok(());
    }

    // Pending paths get re-armed every time a new event lands. When
    // `debounce` elapses with no new event, we classify and emit.
    let pending: Arc<Mutex<HashSet<PathBuf>>> = Arc::new(Mutex::new(HashSet::new()));
    let already_emitted: Arc<Mutex<HashSet<PathBuf>>> = Arc::new(Mutex::new(HashSet::new()));

    while let Some(ev) = rx.recv().await {
        for raw_path in ev.paths {
            // Map every event to the nearest directory we care about
            // (skip ephemeral temp files etc.).
            let Some(dir) = nearest_dir(&raw_path) else {
                continue;
            };
            // Walk up to find the "dump root" — the first directory
            // among the path's ancestors that itself contains either
            // a `CppSDK/SDK` dir or an `_SDKInfo.json` / `SDKInfo.json`
            // marker.
            let Some(candidate) = pick_candidate(&dir, &watched_roots) else {
                continue;
            };

            // Skip ones we've already announced.
            {
                let g = already_emitted.lock().await;
                if g.contains(&candidate) {
                    continue;
                }
            }
            // Arm / re-arm the debounce timer for this candidate.
            {
                let mut p = pending.lock().await;
                p.insert(candidate.clone());
            }

            let pending = Arc::clone(&pending);
            let emitted = Arc::clone(&already_emitted);
            let app = app.clone();
            let watched_roots = watched_roots.clone();
            tokio::spawn(async move {
                tokio::time::sleep(debounce).await;
                // Still pending? Then nothing else has touched it; emit.
                let still_pending = {
                    let mut p = pending.lock().await;
                    p.remove(&candidate)
                };
                if !still_pending {
                    return;
                }
                if !looks_like_dump(&candidate) {
                    return;
                }
                let watched_root = watched_roots
                    .iter()
                    .find(|r| candidate.starts_with(r))
                    .cloned()
                    .unwrap_or_else(|| candidate.clone());
                {
                    let mut e = emitted.lock().await;
                    e.insert(candidate.clone());
                }
                let payload = DumpDetectedEvent {
                    path: candidate.to_string_lossy().into_owned(),
                    watched_root: watched_root.to_string_lossy().into_owned(),
                };
                let _ = app.emit("watcher:dump-detected", payload);
                tracing::info!(path = %candidate.display(), "watcher: new dump detected");
            });
        }
    }
    Ok(())
}

fn nearest_dir(p: &Path) -> Option<PathBuf> {
    if p.is_dir() {
        return Some(p.to_path_buf());
    }
    p.parent().map(Path::to_path_buf)
}

fn looks_like_dump(p: &Path) -> bool {
    if !p.is_dir() {
        return false;
    }
    p.join("CppSDK").join("SDK").is_dir()
        || p.join("_SDKInfo.json").is_file()
        || p.join("SDKInfo.json").is_file()
        || p.join("SDK.hpp").is_file()
}

/// Given a path under one of the watched roots, walk up the tree
/// looking for the first ancestor that looks like a dump root.
/// Returns `None` if nothing in the ancestor chain qualifies.
fn pick_candidate(start: &Path, watched_roots: &[PathBuf]) -> Option<PathBuf> {
    let in_a_root = |p: &Path| watched_roots.iter().any(|r| p.starts_with(r));
    let mut cur: Option<&Path> = Some(start);
    while let Some(p) = cur {
        if !in_a_root(p) {
            return None;
        }
        if looks_like_dump(p) {
            return Some(p.to_path_buf());
        }
        cur = p.parent();
    }
    None
}
