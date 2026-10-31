//! PS5 Dump Forge: the Tauri shell over `ps5-dump-forge-core`. No settings file: what the UI
//! remembers lives in memory until the app closes, except its language (WebView localStorage).
//!
//! All file access stays in these commands; the webview only gets event listening
//! and the app's own commands (see `build.rs` and `capabilities/default.json`).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use ps5_dump_forge_core::{
    ArchivePassword, ConvertRequest, EditCheck, Event, Format, HostTree, Inspection, JobId, Jobs,
    LibraryScan, Listing, folder_refusal,
};
use ps5_dump_forge_server::connect::{self, Connect};
use ps5_dump_forge_server::payload;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};

/// Emitted to the UI when a close or quit was held back because jobs are running or a paired
/// device is using files here (payload `{"peer": bool}`, `QuitHeld`). The UI asks the user and
/// calls `quit_app` on confirmation.
const CLOSE_REQUESTED: &str = "close-requested";

/// Why a close or quit waits for the user's answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct QuitHeld {
    /// A paired device is reading or writing files here: quitting cuts its transfer short.
    peer: bool,
}

/// Our own Quit menu item (macOS), so Cmd+Q goes through the running-jobs check.
#[cfg(target_os = "macos")]
const QUIT_MENU_ID: &str = "quit";

struct AppState {
    /// Created on the first `start_job`, so startup never depends on the job runner.
    jobs: OnceLock<Jobs>,
    /// Started minus finished jobs (queued ones included). Can dip below zero for a
    /// moment when a job finishes before `start` returns; only `> 0` matters.
    running: Arc<AtomicI64>,
    /// `true` once the app is quitting. Held while a job is admitted, so admitting and
    /// deciding to quit never interleave.
    quitting: Mutex<bool>,
    /// What each finished job published: the only paths `reveal` will show.
    outputs: Arc<Mutex<HashMap<JobId, PathBuf>>>,
    /// Connect's service (FLINK 9883 + discovery on UDP 8094; no HTTP listener), started at
    /// setup and shut down at exit; `Err` says why it isn't running.
    connect: Result<Arc<Connect>, String>,
    /// Whether a paired device is reading or writing files here (`Connect::activity_hook`).
    peers_active: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl AppState {
    fn jobs(&self, app: &AppHandle) -> &Jobs {
        self.jobs.get_or_init(|| {
            let app = app.clone();
            let running = self.running.clone();
            let outputs = self.outputs.clone();
            // Mirrored to the paired peers a job uses (Connect holds no strong link back).
            let connect = self.connect.as_ref().ok().cloned();
            let jobs = Jobs::new(move |event| {
                if let Some(c) = &connect {
                    c.job_event(&event);
                }
                let name = match &event {
                    Event::Stages { .. } => "job://stages",
                    Event::Progress { .. } => "job://progress",
                    Event::Log { .. } => "job://log",
                    Event::Done { job, result } => {
                        running.fetch_sub(1, Ordering::SeqCst);
                        if let Ok(report) = result {
                            lock(&outputs).insert(*job, report.output.clone());
                        }
                        "job://done"
                    }
                };
                let _ = app.emit(name, &event);
            });
            // No job starts on what a peer is writing here.
            if let Ok(connect) = &self.connect {
                jobs.set_external_busy(connect.writing_hook());
            }
            // An extract or transfer that replaces (the user confirmed it) never takes the home
            // folder, as Delete and Move keep it.
            jobs.set_protected(Arc::new(|| std::env::home_dir().into_iter().collect()));
            jobs
        })
    }

    fn quitting(&self) -> std::sync::MutexGuard<'_, bool> {
        // A panic while holding it (core's `start`) leaves the flag intact.
        self.quitting.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Quit when idle (no job of this app's, no paired device using files here): closes
    /// admission; else what holds it back.
    fn try_quit(&self) -> Result<(), QuitHeld> {
        let mut quitting = self.quitting();
        let peer = (self.peers_active)();
        if peer || self.running.load(Ordering::SeqCst) > 0 {
            return Err(QuitHeld { peer });
        }
        *quitting = true;
        Ok(())
    }

    /// Cancel every job and wait for its cleanup (a no-op before the first job).
    fn stop_jobs(&self) {
        if let Some(jobs) = self.jobs.get() {
            jobs.cancel_all_and_wait();
        }
    }
}

/// A lock that survives a panic elsewhere (the data is a plain map).
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Runs `f` off the main thread. A panic in core (or a slow scan) then fails only this
/// call instead of the event loop.
async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppHandle, &AppState) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || f(&app, &app.state::<AppState>()))
        .await
        .map_err(|e| format!("internal error: {e}"))?
}

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

/// `password`: for an archive that needs one (else core's `ARCHIVE_PASSWORD_REQUIRED`).
#[tauri::command]
async fn inspect(
    app: AppHandle,
    path: PathBuf,
    password: Option<ArchivePassword>,
) -> Result<Inspection, String> {
    blocking(app, move |_, _| {
        ps5_dump_forge_core::inspect_with(&path, password.as_ref().map(ArchivePassword::as_str))
            .map_err(err)
    })
    .await
}

/// The Library: game dumps under the folder the user chose, on any filesystem. Reads only.
/// System and well-known folders are refused.
#[tauri::command]
async fn library_scan(app: AppHandle, root: PathBuf) -> Result<LibraryScan, String> {
    blocking(app, move |_, _| {
        ps5_dump_forge_core::scan_library_folder(&root).map_err(err)
    })
    .await
}

/// The Edit tab's preview of a host file or folder the user adds or drops.
#[tauri::command]
async fn host_tree(app: AppHandle, path: PathBuf) -> Result<HostTree, String> {
    blocking(app, move |_, _| {
        ps5_dump_forge_core::host_tree(&path, &AtomicBool::new(false)).map_err(err)
    })
    .await
}

/// Which way the Edit tab's `request` can be applied, in place or by a rebuild (core's
/// `Jobs::edit_check`: it settles a leftover undo journal unless a job uses the image, then
/// only reads). Read errors come back as findings.
#[tauri::command]
async fn edit_check(app: AppHandle, request: ConvertRequest) -> Result<EditCheck, String> {
    blocking(app, move |app, state| {
        Ok(state.jobs(app).edit_check(&request))
    })
    .await
}

/// An extract's or transfer's targets that exist already (core's `Jobs::existing_targets`:
/// each selection's, top level, on a peer or NAS too), so the UI asks before it sends
/// `replace`. Reads only.
#[tauri::command]
async fn existing_targets(app: AppHandle, request: ConvertRequest) -> Result<Vec<PathBuf>, String> {
    blocking(app, move |app, state| {
        state.jobs(app).existing_targets(&request).map_err(err)
    })
    .await
}

/// One Library card's cover: a PNG data URL, or `None`.
#[tauri::command]
async fn library_cover(app: AppHandle, path: PathBuf) -> Result<Option<String>, String> {
    blocking(app, move |_, _| {
        ps5_dump_forge_core::library_cover(&path).map_err(err)
    })
    .await
}

/// `password`: an archive's, as `inspect` takes it.
#[tauri::command]
async fn default_output(
    app: AppHandle,
    source: PathBuf,
    format: Format,
    dir: PathBuf,
    password: Option<ArchivePassword>,
) -> Result<PathBuf, String> {
    blocking(app, move |_, _| {
        let password = password.as_ref().map(ArchivePassword::as_str);
        ps5_dump_forge_core::default_output_with(&source, format, &dir, password).map_err(err)
    })
    .await
}

/// `password`: an archive's, as `inspect` takes it.
#[tauri::command]
async fn generated_output(
    app: AppHandle,
    source: PathBuf,
    format: Format,
    dir: PathBuf,
    taken: Vec<PathBuf>,
    password: Option<ArchivePassword>,
) -> Result<PathBuf, String> {
    blocking(app, move |_, _| {
        let password = password.as_ref().map(ArchivePassword::as_str);
        ps5_dump_forge_core::generated_output_with(&source, format, &dir, &taken, password)
            .map_err(err)
    })
    .await
}

// An edit job's `edits` (and mode) come through as they are; core ignores its `output`.
#[tauri::command]
async fn start_job(app: AppHandle, request: ConvertRequest) -> Result<JobId, String> {
    blocking(app, move |app, state| {
        let quitting = state.quitting();
        if *quitting {
            return Err("PS5 Dump Forge is quitting".to_string());
        }
        let id = catch_unwind(AssertUnwindSafe(|| state.jobs(app).start(request.clone())))
            .map_err(|_| "the job runner failed to start the job".to_string())?;
        state.running.fetch_add(1, Ordering::SeqCst);
        if let Ok(connect) = &state.connect {
            connect.job_started(id, &request);
        }
        drop(quitting);
        Ok(id)
    })
    .await
}

/// Deletes a file or folder for good (core's `Jobs::delete_path`; the UI asks first): never a
/// drive root, the home folder or a folder holding it, nor anything an unfinished job uses,
/// nor anything in a folder the Library refuses (`folder_refusal`: `~/Library`, the OS's
/// own folders), which the File explorer could otherwise reach from the home folder.
/// Under the quit lock.
#[tauri::command]
async fn delete_path(app: AppHandle, path: PathBuf) -> Result<(), String> {
    blocking(app, move |app, state| {
        // A peer's path (`flink://`) is judged by the peer; a NAS share's is refused by core.
        if !ps5_dump_forge_core::net::is_network(&path)
            && let Some(why) = folder_refusal(&path)
        {
            return Err(why);
        }
        let quitting = state.quitting();
        if *quitting {
            return Err("PS5 Dump Forge is quitting".to_string());
        }
        let keep: Vec<PathBuf> = std::env::home_dir().into_iter().collect();
        let done = state.jobs(app).delete_path(&path, &keep);
        drop(quitting);
        done.map_err(|e| e.to_string())
    })
    .await
}

/// Moves or renames a file or folder on its drive (core's `Jobs::move_path`): `to` never
/// replaced unless `replace` (the user confirmed it: a file over a file, a folder merged into
/// a folder, the old file refused as `delete_path` refuses it), `from` refused as
/// `delete_path` refuses it (the home folder protected), and either in a folder the Library
/// refuses (`folder_refusal`). Under the quit lock, like `delete_path`.
#[tauri::command]
async fn move_path(
    app: AppHandle,
    from: PathBuf,
    to: PathBuf,
    replace: Option<bool>,
) -> Result<(), String> {
    let replace = replace.unwrap_or(false);
    blocking(app, move |app, state| {
        if let Some(why) = [from.as_path(), to.as_path()]
            .into_iter()
            .filter(|p| !ps5_dump_forge_core::net::is_network(p))
            .find_map(folder_refusal)
        {
            return Err(why);
        }
        let quitting = state.quitting();
        if *quitting {
            return Err("PS5 Dump Forge is quitting".to_string());
        }
        let keep: Vec<PathBuf> = std::env::home_dir().into_iter().collect();
        let done = state.jobs(app).move_path(&from, &to, replace, &keep);
        drop(quitting);
        done.map_err(|e| e.to_string())
    })
    .await
}

/// The Storage tab's file explorer: one folder's entries (core's `list_dir`). Reads only;
/// `parent` is the folder's own, the UI stops Up at its chosen root. A folder the Library
/// refuses (`folder_refusal`: `/`, the OS's own folders) is refused here too.
#[tauri::command]
async fn list_dir(app: AppHandle, path: PathBuf) -> Result<Listing, String> {
    blocking(app, move |_, _| {
        // A NAS share (`smb://`, the NAS source picker) or a peer (`flink://`) has no local
        // folder to refuse. A filesystem root (`/`, `C:\`) lists too: with no system dialog,
        // the picker reaches other folders (`/srv`, another drive) through it.
        if !ps5_dump_forge_core::net::is_network(&path)
            && path.parent().is_some()
            && let Some(why) = folder_refusal(&path)
        {
            return Err(why);
        }
        ps5_dump_forge_core::list_dir(&path).map_err(|e| format!("{}: {e}", path.display()))
    })
    .await
}

/// The File explorer's start before a folder is chosen: the user's home folder, if any.
#[tauri::command]
fn home_folder() -> Option<PathBuf> {
    std::env::home_dir().map(|h| dunce::simplified(&h).to_path_buf())
}

/// The picker's volume roots (Windows: drives C: to Z:; ponytail: A: and B: are left out, a
/// floppy probe can stall) that open and that `list_dir` doesn't refuse. Existence only: each
/// root is opened, not read, on a thread of its own, 1.5 s for all, so a stale mapped drive or
/// an optical drive without a disc is left out instead of holding the picker.
// ponytail: a root still stalled at the deadline leaves its thread running.
#[tauri::command]
async fn local_roots(app: AppHandle) -> Result<Vec<PathBuf>, String> {
    blocking(app, |_, _| {
        let candidates: Vec<PathBuf> = if cfg!(windows) {
            ('C'..='Z')
                .map(|d| PathBuf::from(format!("{d}:\\")))
                .collect()
        } else {
            ["/Volumes", "/media", "/run/media", "/mnt", "/"]
                .map(PathBuf::from)
                .into()
        };
        let (tx, rx) = std::sync::mpsc::channel();
        for (i, path) in candidates.iter().cloned().enumerate() {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let refused = path.parent().is_some() && folder_refusal(&path).is_some();
                let _ = tx.send((i, !refused && std::fs::read_dir(&path).is_ok()));
            });
        }
        drop(tx);
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
        let mut opens = vec![false; candidates.len()];
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            match rx.recv_timeout(left) {
                Ok((i, ok)) => opens[i] = ok,
                Err(_) => break,
            }
        }
        Ok(candidates
            .into_iter()
            .zip(opens)
            .filter_map(|(p, ok)| ok.then_some(p))
            .collect())
    })
    .await
}

#[tauri::command]
async fn cancel_job(app: AppHandle, id: JobId) -> Result<(), String> {
    blocking(app, move |_, state| {
        if let Some(jobs) = state.jobs.get() {
            jobs.cancel(id);
        }
        Ok(())
    })
    .await
}

/// Leftover `.part` files and archive staging folders in `dirs` (missing ones skip), each
/// folder listed once.
#[tauri::command]
async fn stale_parts(app: AppHandle, dirs: Vec<PathBuf>) -> Result<Vec<PathBuf>, String> {
    blocking(app, move |_, _| {
        let mut seen = Vec::new();
        let mut parts = Vec::new();
        for dir in dirs.into_iter().filter(|d| !d.as_os_str().is_empty()) {
            let key = dir.canonicalize().unwrap_or_else(|_| dir.clone());
            if !seen.contains(&key) {
                parts.extend(ps5_dump_forge_core::stale_parts(&key));
                seen.push(key);
            }
        }
        parts.sort();
        parts.dedup();
        Ok(parts)
    })
    .await
}

/// Shows a finished job's output in the file manager. The webview names a job, not a path,
/// so only what this session published can be revealed.
#[tauri::command]
async fn reveal(app: AppHandle, id: JobId) -> Result<(), String> {
    blocking(app, move |_, state| {
        let path = lock(&state.outputs)
            .get(&id)
            .cloned()
            .ok_or_else(|| "this job has no finished output to show".to_string())?;
        if !path.exists() {
            return Err(format!("{} is gone (moved or deleted)", path.display()));
        }
        reveal_path(&path).map_err(|e| format!("could not show {}: {e}", path.display()))
    })
    .await
}

/// The header's GitHub button: the project page in the default browser. A fixed URL, so the
/// webview can't have anything else opened.
#[tauri::command]
async fn open_repo() -> Result<(), String> {
    open_url("https://github.com/quer3q/ps5-dump-forge")
}

/// The running version and this build's release asset suffix (after `ps5-dump-forge-<v>-`),
/// for the update check (`app/src/update.ts`).
#[tauri::command]
fn update_target() -> HashMap<&'static str, String> {
    HashMap::from([
        ("version", env!("CARGO_PKG_VERSION").to_string()),
        ("asset", release_asset().to_string()),
    ])
}

/// The update toast's Update button: release `version`'s archive for this build, downloaded by
/// the default browser. Only a plain `X.Y.Z` makes it into the URL.
#[tauri::command]
async fn open_update(version: String) -> Result<(), String> {
    let parts: Vec<&str> = version.split('.').collect();
    let plain = parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    if !plain {
        return Err(format!("not a release version: {version}"));
    }
    open_url(&format!(
        "https://github.com/quer3q/ps5-dump-forge/releases/download/v{version}/\
         ps5-dump-forge-{version}-{}",
        release_asset()
    ))
}

/// The release archive this build came from (the release scripts' names).
fn release_asset() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos-universal.zip"
    } else if cfg!(all(windows, target_arch = "x86_64")) {
        #[cfg(windows)]
        if webview2::bundled().is_some() {
            return "windows-x86-64-webview2.zip";
        }
        "windows-x86-64.zip"
    } else if cfg!(windows) {
        "windows-arm64.zip"
    } else if cfg!(target_arch = "aarch64") {
        "linux-arm64.tar.gz"
    } else {
        "linux-x86-64.tar.gz"
    }
}

/// `url` in the default browser.
fn open_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("/usr/bin/open");
    #[cfg(target_os = "windows")]
    let mut cmd = std::process::Command::new("explorer");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        host_env(&mut c);
        c
    };
    run(cmd.arg(url)).map_err(|e| format!("could not open {url}: {e}"))
}

// macOS (`open -R`) and Windows (Explorer's `/select,`) select the file; Linux has no portable
// way to, so `xdg-open` opens its folder.
fn reveal_path(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("/usr/bin/open");
        c.arg("-R").arg(path);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        let mut c = std::process::Command::new("explorer");
        c.raw_arg(format!("/select,\"{}\"", path.display()));
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(path.parent().unwrap_or(path));
        host_env(&mut c);
        c
    };
    run(&mut cmd)
}

fn run(cmd: &mut std::process::Command) -> std::io::Result<()> {
    let status = cmd.status()?;
    // Explorer exits non-zero even when it worked; elsewhere a failure means it didn't show
    // (e.g. the output was moved or deleted since).
    if cfg!(not(target_os = "windows")) && !status.success() {
        return Err(std::io::Error::other(format!("{status}")));
    }
    Ok(())
}

/// Started by the release's forge.sh from its AppDir (`APPDIR` set), the app inherits library,
/// GTK and GLib paths into the AppDir, which break host tools (tauri-apps/tauri#10617). forge.sh
/// names them in `FORGE_HOST_VARS` and keeps each one's earlier value in `FORGE_HOST_<name>`
/// (absent: it was unset); `cmd` gets those back. Any other run leaves `cmd` as it is.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn host_env(cmd: &mut std::process::Command) {
    let Some(vars) = std::env::var_os("APPDIR").and(std::env::var_os("FORGE_HOST_VARS")) else {
        return;
    };
    for name in vars.to_string_lossy().split_whitespace() {
        let saved = format!("FORGE_HOST_{name}");
        match std::env::var_os(&saved) {
            Some(value) => cmd.env(name, value),
            None => cmd.env_remove(name),
        };
        cmd.env_remove(saved);
    }
    cmd.env_remove("FORGE_HOST_VARS");
}

/// The user confirmed closing while jobs run: cancel them, wait for their cleanup, exit.
#[tauri::command]
async fn quit_app(app: AppHandle) -> Result<(), String> {
    blocking(app, |app, state| {
        *state.quitting() = true;
        state.stop_jobs();
        app.exit(0);
        Ok(())
    })
    .await
}

/// Close, Cmd+Q or last window gone: `true` to quit now (idle), else the UI asks first.
fn request_quit(app: &AppHandle) -> bool {
    match app.try_state::<AppState>().map_or(Ok(()), |s| s.try_quit()) {
        Ok(()) => true,
        Err(held) => {
            let _ = app.emit(CLOSE_REQUESTED, serde_json::json!({ "peer": held.peer }));
            false
        }
    }
}

/// The default macOS menu, except that Quit is ours: the predefined one sends
/// `terminate:`, which exits without asking the app (tauri-apps/tauri#9198).
#[cfg(target_os = "macos")]
fn macos_menu(app: &AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem as P, Submenu};
    let quit = MenuItem::with_id(
        app,
        QUIT_MENU_ID,
        "Quit PS5 Dump Forge",
        true,
        Some("CmdOrCtrl+Q"),
    )?;
    let forge = Submenu::with_items(
        app,
        "PS5 Dump Forge",
        true,
        &[
            &P::about(app, None, None)?,
            &P::separator(app)?,
            &P::services(app, None)?,
            &P::separator(app)?,
            &P::hide(app, None)?,
            &P::hide_others(app, None)?,
            &P::show_all(app, None)?,
            &P::separator(app)?,
            &quit,
        ],
    )?;
    // Text fields need the Edit menu for Cmd+C / Cmd+V on macOS.
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &P::undo(app, None)?,
            &P::redo(app, None)?,
            &P::separator(app)?,
            &P::cut(app, None)?,
            &P::copy(app, None)?,
            &P::paste(app, None)?,
            &P::select_all(app, None)?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        "Window",
        true,
        &[&P::minimize(app, None)?, &P::close_window(app, None)?],
    )?;
    Menu::with_items(app, &[&forge, &edit, &window])
}

/// WebView2 on Windows: the runtime bundled in a `WebView2` folder next to the exe (the
/// x86-64 `-webview2.zip`), else the installed Evergreen one. Every failure ends in a native
/// message box and exit code 1: a release build has no console to print to.
#[cfg(windows)]
mod webview2 {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::ptr::null_mut;

    use webview2_com::Microsoft::Web::WebView2::Win32::GetAvailableCoreWebView2BrowserVersionString;
    use webview2_com::{CoTaskMemPWSTR, take_pwstr};

    /// Microsoft's Evergreen bootstrapper.
    const EVERGREEN: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";
    /// The Windows 10 sandbox fix for fixed runtimes >= 120 (Microsoft's WebView2 distribution
    /// docs): read/execute for ALL RESTRICTED APPLICATION PACKAGES and ALL APPLICATION PACKAGES.
    const GRANTS: [&str; 2] = ["*S-1-15-2-2:(OI)(CI)(RX)", "*S-1-15-2-1:(OI)(CI)(RX)"];

    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(
            hwnd: *mut std::ffi::c_void,
            text: *const u16,
            caption: *const u16,
            kind: u32,
        ) -> i32;
    }

    #[repr(C)]
    struct OsVersionInfoW {
        size: u32,
        major: u32,
        minor: u32,
        build: u32,
        platform: u32,
        csd: [u16; 128],
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetVolumePathNameW(path: *const u16, volume: *mut u16, len: u32) -> i32;
        fn GetVolumeInformationW(
            root: *const u16,
            name: *mut u16,
            name_len: u32,
            serial: *mut u32,
            max_component: *mut u32,
            flags: *mut u32,
            fs_name: *mut u16,
            fs_name_len: u32,
        ) -> i32;
    }

    #[link(name = "ntdll")]
    unsafe extern "system" {
        /// Unlike `GetVersionExW`, not shimmed by the app manifest.
        fn RtlGetVersion(info: *mut OsVersionInfoW) -> i32;
    }

    /// Runs first in `main`, before Tauri or any other thread: picks the runtime, fixes its
    /// ACLs on Windows 10, and stops here when no runtime can start.
    pub fn prepare() {
        let Some(dir) = bundled() else {
            if !available(None) {
                fail(&no_runtime());
            }
            return;
        };
        // SAFETY: the first thing `main` does; no other thread exists yet.
        unsafe { std::env::set_var("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", &dir) };
        if windows_10() {
            grant_sandbox_access(&dir);
        }
        if !available(Some(&dir)) {
            fail(&bundled_broken(&dir, "no usable runtime found in it"));
        }
    }

    /// The window (or the app) could not be created even though a runtime was found.
    pub fn startup_failed(err: &dyn std::fmt::Display) -> ! {
        match bundled() {
            Some(dir) => fail(&bundled_broken(&dir, &err.to_string())),
            None if !available(None) => fail(&no_runtime()),
            None => fail(&format!(
                "PS5 Dump Forge could not open its window:\n\n{err}\n\n\
                 It keeps its WebView data in the \"data\" folder next to PS5 Dump Forge.exe, so \
                 that folder must be writable: extract the zip to a folder of your own (not \
                 Program Files) and start it from there."
            )),
        }
    }

    /// `WebView2` next to the exe. No fallback to Evergreen once it exists: a broken bundled
    /// runtime is reported, not silently replaced.
    pub fn bundled() -> Option<PathBuf> {
        let dir = std::env::current_exe().ok()?.parent()?.join("WebView2");
        dir.is_dir().then_some(dir)
    }

    /// A runtime is installed (`folder: None`) or `folder` holds one.
    fn available(folder: Option<&Path>) -> bool {
        // An empty string makes a null pointer: "look for an installed runtime".
        let folder = folder.map(|f| f.to_string_lossy()).unwrap_or_default();
        let wide = CoTaskMemPWSTR::from(&*folder);
        let folder = wide.as_ref();
        let mut version = CoTaskMemPWSTR::default().take();
        // SAFETY: `folder` is null or a NUL-terminated wide string that outlives the call;
        // `version` receives a CoTaskMem string (or stays null) that `take_pwstr` frees.
        let found = unsafe {
            GetAvailableCoreWebView2BrowserVersionString(*folder.as_pcwstr(), &mut version)
        };
        let version = take_pwstr(version);
        // No runtime: an error, or (older loaders) success with no version string.
        found.is_ok() && !version.is_empty()
    }

    /// Windows 10 (build < 22000; Windows 11 shares major version 10).
    fn windows_10() -> bool {
        let mut info = OsVersionInfoW {
            size: size_of::<OsVersionInfoW>() as u32,
            major: 0,
            minor: 0,
            build: 0,
            platform: 0,
            csd: [0; 128],
        };
        // SAFETY: `info` is a valid OSVERSIONINFOW with its size set.
        unsafe { RtlGetVersion(&mut info) == 0 && info.major == 10 && info.build < 22000 }
    }

    /// Grants the sandbox read/execute on the bundled runtime: a zip carries no ACLs, and
    /// without them its processes fail to start on Windows 10.
    // ponytail: runs on each Windows 10 launch (idempotent, re-applying the same entries);
    // remembering that it was done would need a marker file.
    fn grant_sandbox_access(dir: &Path) {
        // FAT32 and exFAT keep no ACLs (icacls fails there), and need none.
        if persistent_acls(dir) == Some(false) {
            return;
        }
        // System32's icacls, not whatever a search of the exe's own folder finds first.
        let icacls = std::env::var_os("SystemRoot")
            .map(|root| PathBuf::from(root).join(r"System32\icacls.exe"))
            .unwrap_or_else(|| "icacls.exe".into());
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let out = Command::new(icacls)
            .arg(dir)
            .arg("/grant")
            .args(GRANTS)
            .arg("/Q")
            .stdin(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let detail = match out {
            Ok(out) if out.status.success() => return,
            Ok(out) => {
                String::from_utf8_lossy(&out.stdout).trim().to_string()
                    + "\n"
                    + String::from_utf8_lossy(&out.stderr).trim()
            }
            Err(e) => e.to_string(),
        };
        fail(&format!(
            "PS5 Dump Forge could not give the WebView2 runtime in\n{}\nthe permissions it needs on \
             Windows 10:\n\n{}\n\nRun this in a Command Prompt, then start the app again:\n\n\
             icacls \"{}\" /grant \"{}\" \"{}\"\n\n\
             The folder must be on a local drive (not a network share) and one you can change.",
            dir.display(),
            detail.trim(),
            dir.display(),
            GRANTS[0],
            GRANTS[1],
        ));
    }

    /// Whether `dir`'s volume stores ACLs (`FILE_PERSISTENT_ACLS`); `None` if the query fails.
    fn persistent_acls(dir: &Path) -> Option<bool> {
        const FILE_PERSISTENT_ACLS: u32 = 0x8;
        let path: Vec<u16> = dir.as_os_str().encode_wide().chain([0]).collect();
        let mut root = [0u16; 1024];
        let mut flags = 0;
        // SAFETY: `path` is NUL-terminated, `root` is writable for its length (the call
        // NUL-terminates it), and the unused out-parameters are null with zero lengths.
        let ok = unsafe {
            GetVolumePathNameW(path.as_ptr(), root.as_mut_ptr(), root.len() as u32) != 0
                && GetVolumeInformationW(
                    root.as_ptr(),
                    null_mut(),
                    0,
                    null_mut(),
                    null_mut(),
                    &mut flags,
                    null_mut(),
                    0,
                ) != 0
        };
        ok.then_some(flags & FILE_PERSISTENT_ACLS != 0)
    }

    fn no_runtime() -> String {
        // Only the x86-64 release has a zip with the runtime in it.
        let bundled_zip = if cfg!(target_arch = "x86_64") {
            format!(
                "\n\nor use ps5-dump-forge-{}-windows-x86-64-webview2.zip instead, which carries \
                 the runtime in its WebView2 folder.",
                env!("CARGO_PKG_VERSION")
            )
        } else {
            String::new()
        };
        format!(
            "PS5 Dump Forge needs the Microsoft Edge WebView2 Runtime, and it is not installed.\n\n\
             Install it from\n{EVERGREEN}{bundled_zip}"
        )
    }

    fn bundled_broken(dir: &Path, detail: &str) -> String {
        // Only the x86-64 webview2 zip ships the folder; anywhere else it was added by hand.
        let fix = if cfg!(target_arch = "x86_64") {
            format!(
                "Delete the WebView2 folder and extract it again from the zip, or use \
                 ps5-dump-forge-{}-windows-x86-64.zip with the installed WebView2 Runtime \
                 ({EVERGREEN}).",
                env!("CARGO_PKG_VERSION")
            )
        } else {
            format!(
                "Delete the WebView2 folder to use the installed WebView2 Runtime ({EVERGREEN})."
            )
        };
        format!(
            "The WebView2 runtime bundled in\n{}\nfailed to start:\n\n{detail}\n\n{fix}\n\n\
             The app's folder must also be writable (its WebView data lives in the \"data\" folder \
             beside it) and on a local drive.",
            dir.display()
        )
    }

    fn fail(text: &str) -> ! {
        let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain([0]).collect() };
        let (text, caption) = (wide(text), wide("PS5 Dump Forge"));
        const MB_ICONERROR: u32 = 0x10;
        // SAFETY: both are valid NUL-terminated wide strings; no owner window.
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                caption.as_ptr(),
                MB_ICONERROR,
            )
        };
        std::process::exit(1);
    }
}

// ---- Connect: the same calls as the server's `/api/connect/*` (snake_case arguments) ------

/// Connect's service, or why it isn't running.
fn connect(state: &AppState) -> Result<&Connect, String> {
    state.connect.as_deref().map_err(Clone::clone)
}

/// Runs `f` on Connect off the main thread; an error is `connect::error_text` (a refusal's
/// code, which the UI translates, else the text).
async fn connect_call<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&Connect) -> std::io::Result<T> + Send + 'static,
) -> Result<T, String> {
    blocking(app, move |_, state| {
        f(connect(state)?).map_err(|e| connect::error_text(&e))
    })
    .await
}

/// This device, saved devices and the last search.
#[tauri::command]
async fn connect_state(app: AppHandle) -> Result<serde_json::Value, String> {
    connect_call(app, |c| Ok(c.snapshot())).await
}

/// Asks a paired peer to cancel its job shown here; false when it isn't listed.
#[tauri::command(rename_all = "snake_case")]
async fn connect_cancel_job(app: AppHandle, key_id: String, job: JobId) -> Result<bool, String> {
    connect_call(app, move |c| Ok(c.cancel_remote_job(&key_id, job))).await
}

/// Asks the paired PS5 `key_id` to install the `.pkg` at `path` on this computer (read over
/// FLINK; the job shows here mirrored); the PS5's job id.
#[tauri::command]
async fn connect_install(app: AppHandle, key_id: String, path: PathBuf) -> Result<JobId, String> {
    blocking(app, move |_, state| connect(state)?.install(&key_id, &path)).await
}

/// Times the link to the paired device `key_id`: 256 MiB sent, then fetched, over FLINK's
/// lanes, nothing written to disk on either side. `run` is the UI's id for this test.
#[tauri::command]
async fn connect_speed_test(
    app: AppHandle,
    key_id: String,
    run: String,
) -> Result<ps5_dump_forge_server::connect_fs::SpeedTest, String> {
    blocking(app, move |_, state| {
        connect(state)?.speed_test(&key_id, &run)
    })
    .await
}

/// Stops the speed test `run` with the paired device `key_id` (it ends `cancelled`; a run not
/// started yet ends so as it starts).
#[tauri::command]
async fn connect_speed_test_cancel(
    app: AppHandle,
    key_id: String,
    run: String,
) -> Result<(), String> {
    blocking(app, move |_, state| {
        connect(state)?.speed_test_cancel(&key_id, &run);
        Ok(())
    })
    .await
}

/// Asks the device at `address` to pair; `connect_state`'s `outgoing` follows it.
#[tauri::command(rename_all = "snake_case")]
async fn connect_pair(app: AppHandle, address: String) -> Result<(), String> {
    connect_call(app, move |c| c.pair(&address)).await
}

/// What answers at `address` (Forge, SMB, NFS or FTP): Pair by address decides by it.
#[tauri::command(rename_all = "snake_case")]
async fn connect_probe(
    app: AppHandle,
    address: String,
) -> Result<ps5_dump_forge_server::probe::Probe, String> {
    connect_call(app, move |c| c.probe(&address)).await
}

/// Install on PS5: v`version`'s PS5 ELF downloaded from its GitHub release into memory here and
/// streamed to elfldr at `address` (port 9021 by default), which starts it; the console needs no
/// internet.
#[tauri::command(rename_all = "snake_case")]
async fn connect_payload_install(
    app: AppHandle,
    address: String,
    version: String,
) -> Result<(), String> {
    blocking(app, move |_, _| {
        let addr = payload::address(&address).map_err(|e| e.to_string())?;
        ps5_dump_forge_updater::install(addr, &version, None)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
}

/// Send a payload: the ELF file at `path` streamed to elfldr at `address` (port 9021 by
/// default); the bytes sent.
#[tauri::command(rename_all = "snake_case")]
async fn connect_payload_send(
    app: AppHandle,
    address: String,
    path: PathBuf,
) -> Result<u64, String> {
    blocking(app, move |_, _| {
        let addr = payload::address(&address).map_err(|e| e.to_string())?;
        ps5_dump_forge_updater::send(addr, &path).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
async fn connect_pair_cancel(app: AppHandle) -> Result<(), String> {
    connect_call(app, |c| {
        c.pair_cancel();
        Ok(())
    })
    .await
}

/// This device's user answers another device's pairing request `id` (only here, in its own
/// window: the app has no HTTP listener).
#[tauri::command(rename_all = "snake_case")]
async fn connect_approve(app: AppHandle, id: String, allow: bool) -> Result<bool, String> {
    connect_call(app, move |c| Ok(c.approve(&id, allow))).await
}

#[tauri::command(rename_all = "snake_case")]
async fn connect_forget(app: AppHandle, key_id: String) -> Result<bool, String> {
    connect_call(app, move |c| c.forget(&key_id)).await
}

#[tauri::command]
async fn connect_discover_start(app: AppHandle) -> Result<(), String> {
    connect_call(app, |c| {
        c.discover_start();
        Ok(())
    })
    .await
}

#[tauri::command]
async fn connect_discover_stop(app: AppHandle) -> Result<(), String> {
    connect_call(app, |c| {
        c.discover_stop();
        Ok(())
    })
    .await
}

/// Saves a NAS (one entry per host + port, updated in place; `id`: the entry an edit
/// changes); its id. `user`/`password` omitted keep the saved ones.
// Tauri takes the request's fields as the command's arguments.
#[allow(clippy::too_many_arguments)]
#[tauri::command(rename_all = "snake_case")]
async fn connect_nas_save(
    app: AppHandle,
    id: Option<String>,
    name: String,
    host: String,
    port: Option<u16>,
    protocol: Option<ps5_dump_forge_server::nas::Protocol>,
    user: Option<String>,
    password: Option<String>,
) -> Result<String, String> {
    let req = ps5_dump_forge_server::nas::NasSave {
        id,
        name,
        host,
        port,
        protocol,
        user,
        password,
    };
    connect_call(app, move |c| c.nas_save(&req)).await
}

#[tauri::command(rename_all = "snake_case")]
async fn connect_nas_forget(app: AppHandle, id: String) -> Result<bool, String> {
    connect_call(app, move |c| c.nas_forget(&id)).await
}

fn main() {
    #[cfg(windows)]
    webview2::prepare();
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.menu(macos_menu).on_menu_event(|app, event| {
        if event.id() == QUIT_MENU_ID && request_quit(app) {
            app.exit(0);
        }
    });
    let app = builder
        .setup(|app| {
            // No exe path (or a panic in core) leaves WebView data in the OS default place.
            #[cfg(not(target_os = "macos"))]
            let webview_dir = std::env::current_exe()
                .ok()
                .and_then(|exe| catch_unwind(|| ps5_dump_forge_core::DataDirs::resolve(&exe)).ok())
                .map(|d| d.webview());
            let connect = match app.path().app_data_dir() {
                Ok(dir) => {
                    let dir = dir.join("connect");
                    let port = std::env::var("FORGE_FLINK_PORT")
                        .ok()
                        .and_then(|p| p.parse().ok())
                        .unwrap_or(connect::PORT);
                    Connect::start(&dir, port)
                        .map(Arc::new)
                        .map_err(|e| format!("Connect couldn't start: {}: {e}", dir.display()))
                }
                Err(e) => Err(format!("Connect couldn't start: no app data folder: {e}")),
            };
            // A peer's writes never land in what this device's unfinished jobs use (none
            // before the first job creates the runner).
            if let Ok(connect) = &connect {
                let handle = app.handle().clone();
                connect.set_busy(Arc::new(move |path| {
                    (handle.try_state::<AppState>())
                        .is_some_and(|s| s.jobs.get().is_some_and(|jobs| jobs.uses(path)))
                }));
                // A peer's replacing rename is refused as a job's replace is (the home folder
                // kept): the runner is made if none is yet, which sets what it protects.
                let handle = app.handle().clone();
                connect.set_replace_check(Arc::new(move |path| {
                    match handle.try_state::<AppState>() {
                        Some(s) => (s.jobs(&handle).replace_check(path)).map_err(|e| e.to_string()),
                        None => Err("PS5 Dump Forge is starting; try again".to_string()),
                    }
                }));
                // A peer's Delete and Move to here are this device's own (the home folder
                // kept), refused while quitting.
                use ps5_dump_forge_core::DeleteError;
                let keep = || -> Vec<PathBuf> { std::env::home_dir().into_iter().collect() };
                let starting = || DeleteError::Refused("PS5 Dump Forge is starting".into());
                let handle = app.handle().clone();
                let delete: ps5_dump_forge_server::connect_fs::DeleteHook = Arc::new(move |path| {
                    let state = handle.try_state::<AppState>().ok_or_else(starting)?;
                    let quitting = state.quitting();
                    if *quitting {
                        return Err(DeleteError::Refused("PS5 Dump Forge is quitting".into()));
                    }
                    // The folders this app's own Delete refuses (`~/Library`, the OS's), too.
                    if let Some(why) = folder_refusal(path) {
                        return Err(DeleteError::Refused(why));
                    }
                    state.jobs(&handle).delete_path(path, &keep())
                });
                let handle = app.handle().clone();
                let move_: ps5_dump_forge_server::connect_fs::MoveHook =
                    Arc::new(move |from, to, replace| {
                        let state = handle.try_state::<AppState>().ok_or_else(starting)?;
                        let quitting = state.quitting();
                        if *quitting {
                            return Err(DeleteError::Refused("PS5 Dump Forge is quitting".into()));
                        }
                        if let Some(why) = [from, to].into_iter().find_map(folder_refusal) {
                            return Err(DeleteError::Refused(why));
                        }
                        state.jobs(&handle).move_path(from, to, replace, &keep())
                    });
                connect.set_delete_move(delete, move_);
                // A peer shown one of this device's jobs may cancel it.
                let handle = app.handle().clone();
                connect.set_cancel(Arc::new(move |id| {
                    if let Some(state) = handle.try_state::<AppState>()
                        && let Some(jobs) = state.jobs.get()
                    {
                        jobs.cancel(id);
                    }
                }));
            }
            let peers_active = match &connect {
                Ok(c) => c.activity_hook(),
                Err(_) => Arc::new(|| false),
            };
            app.manage(AppState {
                jobs: OnceLock::new(),
                running: Arc::default(),
                quitting: Mutex::new(false),
                outputs: Arc::default(),
                connect,
                peers_active,
            });

            // The drag and drop handler stays on (Tauri 2's default): the Edit tab hears
            // drops of host files through `onDragDropEvent` (`core:event:allow-listen`).
            let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                // The web build's page title says the same (vite.config.ts).
                .title(concat!("PS5 Dump Forge v", env!("CARGO_PKG_VERSION")))
                .inner_size(1200.0, 820.0)
                .min_inner_size(900.0, 620.0);
            // WebView data (the UI language's localStorage) goes to `<app dir>/data/webview`,
            // except on macOS where WKWebView storage can't be moved: WebKit's own per-app store.
            #[cfg(not(target_os = "macos"))]
            let window = match webview_dir {
                Some(dir) => window.data_directory(dir),
                None => window,
            };
            // Tauri runs this hook from its event loop and would only panic on an error,
            // which a Windows release build never shows.
            #[cfg(windows)]
            if let Err(err) = window.build() {
                webview2::startup_failed(&err);
            }
            #[cfg(not(windows))]
            window.build()?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event
                && !request_quit(window.app_handle())
            {
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            inspect,
            library_scan,
            library_cover,
            host_tree,
            edit_check,
            existing_targets,
            default_output,
            generated_output,
            start_job,
            cancel_job,
            stale_parts,
            delete_path,
            move_path,
            list_dir,
            home_folder,
            local_roots,
            reveal,
            open_repo,
            update_target,
            open_update,
            quit_app,
            connect_state,
            connect_pair,
            connect_probe,
            connect_pair_cancel,
            connect_approve,
            connect_forget,
            connect_discover_start,
            connect_discover_stop,
            connect_nas_save,
            connect_nas_forget,
            connect_cancel_job,
            connect_install,
            connect_speed_test,
            connect_speed_test_cancel,
            connect_payload_install,
            connect_payload_send,
        ])
        .build(tauri::generate_context!());
    #[cfg(windows)]
    let app = app.unwrap_or_else(|err| webview2::startup_failed(&err));
    #[cfg(not(windows))]
    let app = app.expect("failed to build the app");

    app.run(|app, event| match event {
        // Last window gone (`code: None`); `quit_app` and our Quit exit with `Some(0)`.
        RunEvent::ExitRequested {
            code: None, api, ..
        } => {
            if !request_quit(app) {
                api.prevent_exit();
            }
        }
        // ponytail: Dock "Quit", logout and shutdown send `terminate:` and can't be held
        // back without a custom app delegate; we at least cancel and wait for cleanup.
        RunEvent::Exit => {
            if let Some(state) = app.try_state::<AppState>() {
                state.stop_jobs();
                if let Ok(c) = &state.connect {
                    c.shutdown();
                }
            }
        }
        _ => {}
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The quit check hears the peers' hook as it hears this app's jobs.
    #[test]
    fn a_peer_using_files_here_holds_a_quit_back() {
        let busy = Arc::new(AtomicBool::new(true));
        let hook = busy.clone();
        let state = AppState {
            jobs: OnceLock::new(),
            running: Arc::default(),
            quitting: Mutex::new(false),
            outputs: Arc::default(),
            connect: Err("not started".into()),
            peers_active: Arc::new(move || hook.load(Ordering::SeqCst)),
        };
        assert_eq!(state.try_quit(), Err(QuitHeld { peer: true }));
        assert!(!*state.quitting());
        state.running.store(1, Ordering::SeqCst);
        busy.store(false, Ordering::SeqCst);
        assert_eq!(state.try_quit(), Err(QuitHeld { peer: false }));
        state.running.store(0, Ordering::SeqCst);
        assert_eq!(state.try_quit(), Ok(()));
        assert!(*state.quitting());
    }
}
