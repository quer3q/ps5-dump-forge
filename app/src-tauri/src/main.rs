//! PS5 Dump Forge: the Tauri shell over `ps5-dump-forge-core`. No settings file: what the UI
//! remembers lives in memory until the app closes.
//!
//! All file access stays in these commands; the webview only gets the dialog plugin
//! and the app's own commands (see `build.rs` and `capabilities/default.json`).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use ps5_dump_forge_core::{ConvertRequest, Event, Format, Inspection, JobId, Jobs};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};

/// Emitted to the UI when a close or quit was held back because jobs are running.
/// The UI asks the user and calls `quit_app` on confirmation.
const CLOSE_REQUESTED: &str = "close-requested";

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
}

impl AppState {
    fn jobs(&self, app: &AppHandle) -> &Jobs {
        self.jobs.get_or_init(|| {
            let app = app.clone();
            let running = self.running.clone();
            let outputs = self.outputs.clone();
            Jobs::new(move |event| {
                let name = match &event {
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
            })
        })
    }

    fn quitting(&self) -> std::sync::MutexGuard<'_, bool> {
        // A panic while holding it (core's `start`) leaves the flag intact.
        self.quitting.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Quit when idle: closes admission and returns `true`; else `false`.
    fn try_quit(&self) -> bool {
        let mut quitting = self.quitting();
        if self.running.load(Ordering::SeqCst) > 0 {
            return false;
        }
        *quitting = true;
        true
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

#[tauri::command]
async fn inspect(app: AppHandle, path: PathBuf) -> Result<Inspection, String> {
    blocking(app, move |_, _| {
        ps5_dump_forge_core::inspect(&path).map_err(err)
    })
    .await
}

#[tauri::command]
async fn default_output(
    app: AppHandle,
    source: PathBuf,
    format: Format,
    dir: PathBuf,
) -> Result<PathBuf, String> {
    blocking(app, move |_, _| {
        ps5_dump_forge_core::default_output(&source, format, &dir).map_err(err)
    })
    .await
}

#[tauri::command]
async fn generated_output(
    app: AppHandle,
    source: PathBuf,
    format: Format,
    dir: PathBuf,
    taken: Vec<PathBuf>,
) -> Result<PathBuf, String> {
    blocking(app, move |_, _| {
        ps5_dump_forge_core::generated_output(&source, format, &dir, &taken).map_err(err)
    })
    .await
}

#[tauri::command]
async fn start_job(app: AppHandle, request: ConvertRequest) -> Result<JobId, String> {
    blocking(app, move |app, state| {
        let quitting = state.quitting();
        if *quitting {
            return Err("PS5 Dump Forge is quitting".to_string());
        }
        let id = catch_unwind(AssertUnwindSafe(|| state.jobs(app).start(request)))
            .map_err(|_| "the job runner failed to start the job".to_string())?;
        state.running.fetch_add(1, Ordering::SeqCst);
        drop(quitting);
        Ok(id)
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

/// Leftover `.part` files in `dirs` (missing ones skip), each folder listed once.
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

// macOS (`open -R`) and Windows (Explorer's `/select,`) select the file; Linux has no portable
// way to, so `xdg-open` opens its folder. ponytail: the Windows and Linux branches are untested
// on real machines.
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
    let idle = app.try_state::<AppState>().is_none_or(|s| s.try_quit());
    if !idle {
        let _ = app.emit(CLOSE_REQUESTED, ());
    }
    idle
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
/// `-webview2.zip`), else the installed Evergreen one. Every failure ends in a native message
/// box and exit code 1: a release build has no console to print to.
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
    fn bundled() -> Option<PathBuf> {
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
        format!(
            "PS5 Dump Forge needs the Microsoft Edge WebView2 Runtime, and it is not installed.\n\n\
             Install it from\n{EVERGREEN}\n\nor use ps5-dump-forge-{}-windows-x64-webview2.zip \
             instead, which carries the runtime in its WebView2 folder.",
            env!("CARGO_PKG_VERSION")
        )
    }

    fn bundled_broken(dir: &Path, detail: &str) -> String {
        format!(
            "The WebView2 runtime bundled in\n{}\nfailed to start:\n\n{detail}\n\n\
             Delete the WebView2 folder and extract it again from the zip, or use \
             ps5-dump-forge-{}-windows-x64.zip with the installed WebView2 Runtime ({EVERGREEN}).\n\n\
             The app's folder must also be writable (its WebView data lives in the \"data\" folder \
             beside it) and on a local drive.",
            dir.display(),
            env!("CARGO_PKG_VERSION")
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
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // No exe path (or a panic in core) leaves WebView data in the OS default place.
            #[cfg(not(target_os = "macos"))]
            let webview_dir = std::env::current_exe()
                .ok()
                .and_then(|exe| catch_unwind(|| ps5_dump_forge_core::DataDirs::resolve(&exe)).ok())
                .map(|d| d.webview());
            app.manage(AppState {
                jobs: OnceLock::new(),
                running: Arc::default(),
                quitting: Mutex::new(false),
                outputs: Arc::default(),
            });

            let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title("PS5 Dump Forge")
                .inner_size(1200.0, 820.0)
                .min_inner_size(900.0, 620.0);
            // WebView data goes to `<app dir>/data/webview`, except on macOS where
            // WKWebView storage can't be moved, so it stays in memory (the UI keeps no state).
            #[cfg(target_os = "macos")]
            let window = window.incognito(true);
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
            default_output,
            generated_output,
            start_job,
            cancel_job,
            stale_parts,
            reveal,
            quit_app,
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
            }
        }
        _ => {}
    });
}
