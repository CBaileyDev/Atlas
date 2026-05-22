// Prevent the Windows console from popping up when running `codex-atlas.exe`
// in release. In dev we want the console for tracing output.
#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

fn main() {
    codex_atlas_lib::run();
}
