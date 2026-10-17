use tauri_build::{AppManifest, Attributes};

/// The app's own commands; each gets an `allow-<command>` permission that
/// `capabilities/default.json` grants. Anything not listed is denied.
const COMMANDS: &[&str] = &[
    "inspect",
    "library_scan",
    "library_cover",
    "host_tree",
    "edit_check",
    "default_output",
    "generated_output",
    "start_job",
    "cancel_job",
    "stale_parts",
    "lz4_patch",
    "lz4_unpatch",
    "delete_path",
    "reveal",
    "open_repo",
    "update_target",
    "open_update",
    "quit_app",
];

fn main() {
    tauri_build::try_build(Attributes::new().app_manifest(AppManifest::new().commands(COMMANDS)))
        .expect("tauri build script failed");
}
