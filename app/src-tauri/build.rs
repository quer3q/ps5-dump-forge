use tauri_build::{AppManifest, Attributes};

/// The app's own commands; each gets an `allow-<command>` permission that
/// `capabilities/default.json` grants. Anything not listed is denied.
const COMMANDS: &[&str] = &[
    "inspect",
    "library_scan",
    "library_cover",
    "host_tree",
    "edit_check",
    "existing_targets",
    "default_output",
    "generated_output",
    "start_job",
    "cancel_job",
    "stale_parts",
    "delete_path",
    "move_path",
    "list_dir",
    "home_folder",
    "local_roots",
    "reveal",
    "open_repo",
    "update_target",
    "open_update",
    "quit_app",
    "connect_state",
    "connect_pair",
    "connect_probe",
    "connect_pair_cancel",
    "connect_approve",
    "connect_forget",
    "connect_discover_start",
    "connect_discover_stop",
    "connect_send",
    "connect_fetch",
    "connect_cancel",
    "connect_remote_list",
    "connect_nas_save",
    "connect_nas_forget",
    "connect_cancel_job",
    "connect_install",
    "connect_payload_install",
    "connect_payload_send",
    "connect_speed_test",
    "connect_speed_test_cancel",
];

fn main() {
    tauri_build::try_build(Attributes::new().app_manifest(AppManifest::new().commands(COMMANDS)))
        .expect("tauri build script failed");
}
