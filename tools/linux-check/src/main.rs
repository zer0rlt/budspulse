//! Not a real binary: pulls the platform-neutral + Linux modules of the app in via #[path] so
//! they can be type-checked for x86_64-unknown-linux-gnu. (state.rs / lib.rs need Tauri.)
#![allow(dead_code)]

#[path = "../../../src-tauri/src/audio_eq/mod.rs"]
pub mod audio_eq;
#[path = "../../../src-tauri/src/automation/mod.rs"]
pub mod automation;
#[path = "../../../src-tauri/src/bt/mod.rs"]
pub mod bt;
#[path = "../../../src-tauri/src/protocols/mod.rs"]
pub mod protocols;
#[path = "../../../src-tauri/src/tray.rs"]
pub mod tray;

fn main() {}
