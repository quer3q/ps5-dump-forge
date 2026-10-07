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

// ponytail: v1 ships for macOS (`open -R` selects the file in Finder); the Windows and Linux
// branches compile but are untested: Explorer's `/select,` and the folder in `xdg-open`.
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

fn main() {
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
        .build(tauri::generate_context!())
        .expect("failed to build the app");

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
