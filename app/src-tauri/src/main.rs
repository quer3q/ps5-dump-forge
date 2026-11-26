//! PS5 Dump Forge: the Tauri shell over `ps5-dump-forge-core`; all file access stays in these commands (`build.rs`, `capabilities/default.json`).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use ps5_dump_forge_core::{
    ArchivePassword, ConvertRequest, EditCheck, Event, Format, HostTree, Inspection, JobId, Jobs,
    Listing, folder_refusal,
};
use ps5_dump_forge_server::connect::{self, Connect};
use ps5_dump_forge_server::payload;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};

/// Emitted when a close or quit was held back (payload `{"peer": bool}`); the UI asks, then calls `quit_app`.
const CLOSE_REQUESTED: &str = "close-requested";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct QuitHeld {
    /// A paired device is reading or writing files here.
    peer: bool,
}

/// Our own Quit menu item (macOS), so Cmd+Q goes through the running-jobs check.
#[cfg(target_os = "macos")]
const QUIT_MENU_ID: &str = "quit";

struct AppState {
    /// Created on the first `start_job`, so startup never depends on the job runner.
    jobs: OnceLock<Jobs>,
    /// Started minus finished jobs; can dip below zero for a moment, only `> 0` matters.
    running: Arc<AtomicI64>,
    /// Held while a job is admitted, so admitting and deciding to quit never interleave.
    quitting: Mutex<bool>,
    /// The only paths `reveal` will show.
    outputs: Arc<Mutex<HashMap<JobId, PathBuf>>>,
    /// Started at setup, shut down at exit; `Err` says why it isn't running.
    connect: Result<Arc<Connect>, String>,
    peers_active: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl AppState {
    fn jobs(&self, app: &AppHandle) -> &Jobs {
        self.jobs.get_or_init(|| {
            let app = app.clone();
            let running = self.running.clone();
            let outputs = self.outputs.clone();
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
            if let Ok(connect) = &self.connect {
                jobs.set_external_busy(connect.writing_hook());
            }
            // A confirmed replace never takes the home folder, as Delete and Move keep it.
            jobs.set_protected(Arc::new(|| std::env::home_dir().into_iter().collect()));
            jobs
        })
    }

    fn quitting(&self) -> std::sync::MutexGuard<'_, bool> {
        // A panic while holding it (core's `start`) leaves the flag intact.
        self.quitting.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Quit when idle: closes admission; else what holds it back.
    fn try_quit(&self) -> Result<(), QuitHeld> {
        let mut quitting = self.quitting();
        let peer = (self.peers_active)();
        if peer || self.running.load(Ordering::SeqCst) > 0 {
            return Err(QuitHeld { peer });
        }
        *quitting = true;
        Ok(())
    }

    fn stop_jobs(&self) {
        if let Some(jobs) = self.jobs.get() {
            jobs.cancel_all_and_wait();
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Runs `f` off the main thread, so a panic in core (or a slow scan) fails only this call.
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

#[tauri::command]
async fn host_tree(app: AppHandle, path: PathBuf) -> Result<HostTree, String> {
    blocking(app, move |_, _| {
        ps5_dump_forge_core::host_tree(&path, &AtomicBool::new(false)).map_err(err)
    })
    .await
}

#[tauri::command]
async fn pack_estimate(
    app: AppHandle,
    source: PathBuf,
    kind: ps5_dump_forge_core::EstimateKind,
    level: u8,
    password: Option<ArchivePassword>,
    unpack: Option<bool>,
) -> Result<ps5_dump_forge_core::PackEstimate, String> {
    blocking(app, move |_, _| {
        let unpack = unpack.unwrap_or(false);
        ps5_dump_forge_core::pack_estimate(&source, kind, level, password.as_ref(), unpack)
            .map_err(err)
    })
    .await
}

/// Settles a leftover undo journal unless a job uses the image, then only reads; read errors come back as findings.
#[tauri::command]
async fn edit_check(app: AppHandle, request: ConvertRequest) -> Result<EditCheck, String> {
    blocking(app, move |app, state| {
        Ok(state.jobs(app).edit_check(&request))
    })
    .await
}

#[tauri::command]
async fn existing_targets(app: AppHandle, request: ConvertRequest) -> Result<Vec<PathBuf>, String> {
    blocking(app, move |app, state| {
        state.jobs(app).existing_targets(&request).map_err(err)
    })
    .await
}

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

/// `naming`: `None` the default template; an invalid one is an error.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn generated_output(
    app: AppHandle,
    source: PathBuf,
    format: Format,
    dir: PathBuf,
    taken: Vec<PathBuf>,
    password: Option<ArchivePassword>,
    pack: Option<ps5_dump_forge_core::PackKind>,
    naming: Option<String>,
) -> Result<PathBuf, String> {
    blocking(app, move |_, _| {
        let password = password.as_ref().map(ArchivePassword::as_str);
        ps5_dump_forge_core::generated_output_with(
            &source,
            format,
            &dir,
            &taken,
            password,
            pack,
            naming.as_deref(),
        )
        .map_err(err)
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

/// Under the quit lock; also refuses a refused folder (`folder_refusal`), which the File explorer could reach from home.
#[tauri::command]
async fn delete_path(app: AppHandle, path: PathBuf) -> Result<(), String> {
    blocking(app, move |app, state| {
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

/// `to` replaced only with `replace`; refuses as `delete_path` does. Under the quit lock.
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

/// `parent` is the folder's own; the UI stops Up at its root. A refused folder is refused here too.
#[tauri::command]
async fn list_dir(app: AppHandle, path: PathBuf) -> Result<Listing, String> {
    blocking(app, move |_, _| {
        // A NAS or peer path has no local folder to refuse; `/` and `C:\` list too (no system dialog, so the picker reaches other folders through them).
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

#[tauri::command]
async fn free_space(app: AppHandle, path: PathBuf) -> Result<Option<u64>, String> {
    blocking(app, move |_, _| Ok(ps5_dump_forge_core::free_space(&path))).await
}

#[tauri::command]
fn bundled_ng_version() -> &'static str {
    ps5_dump_forge_lz4::runtime::NG_VERSION
}

#[tauri::command]
fn home_folder() -> Option<PathBuf> {
    std::env::home_dir().map(|h| dunce::simplified(&h).to_path_buf())
}

/// Volume roots that open and that `list_dir` doesn't refuse, each once at its real path; a thread each, 1.5 s for all, so a stalled drive is left out.
// ponytail: a root still stalled at the deadline leaves its thread running.
#[tauri::command]
async fn local_roots(app: AppHandle) -> Result<Vec<PathBuf>, String> {
    blocking(app, |_, _| {
        let mut candidates: Vec<PathBuf> = if cfg!(windows) {
            ('A'..='Z')
                .map(|d| PathBuf::from(format!("{d}:\\")))
                .collect()
        } else {
            ["/Volumes", "/media", "/run/media", "/mnt"]
                .map(PathBuf::from)
                .into()
        };
        // Linux: every mounted disk is a root too (issue #52), a folder reached twice through a link listed once; read lossily.
        #[cfg(target_os = "linux")]
        if let Ok(info) = std::fs::read("/proc/self/mountinfo") {
            candidates.extend(disk_mounts(&String::from_utf8_lossy(&info)));
        }
        if !cfg!(windows) {
            candidates.push(PathBuf::from("/"));
        }
        let (tx, rx) = std::sync::mpsc::channel();
        for (i, path) in candidates.iter().cloned().enumerate() {
            let tx = tx.clone();
            std::thread::spawn(move || {
                #[cfg(windows)]
                quiet_drive_errors();
                let refused = path.parent().is_some() && folder_refusal(&path).is_some();
                let opens = !refused && std::fs::read_dir(&path).is_ok();
                let real = if cfg!(windows) {
                    path
                } else {
                    dunce::canonicalize(&path).unwrap_or(path)
                };
                let _ = tx.send((i, opens.then_some(real)));
            });
        }
        // A home that is a disk of its own isn't listed again.
        const HOME: usize = usize::MAX;
        if let Some(home) = home_folder() {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send((HOME, dunce::canonicalize(&home).ok()));
            });
        }
        drop(tx);
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
        let mut roots = vec![None; candidates.len()];
        let mut seen = std::collections::HashSet::new();
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            match rx.recv_timeout(left) {
                Ok((HOME, home)) => seen.extend(home),
                Ok((i, root)) => roots[i] = root,
                Err(_) => break,
            }
        }
        Ok(roots
            .into_iter()
            .flatten()
            .filter(|p| seen.insert(p.clone()))
            .collect())
    })
    .await
}

/// Mount points in `/proc/self/mountinfo` of disk filesystems: not `/`, no kernel, container or package-image ones.
#[cfg(target_os = "linux")]
fn disk_mounts(info: &str) -> Vec<PathBuf> {
    const DISK: [&str; 18] = [
        "ext2", "ext3", "ext4", "btrfs", "xfs", "f2fs", "bcachefs", "zfs", "jfs", "reiserfs",
        "vfat", "exfat", "ntfs", "ntfs3", "fuseblk", "hfsplus", "udf", "iso9660",
    ];
    info.lines()
        .filter_map(|line| {
            let (mount, fs) = line.split_once(" - ")?;
            let point = mount.split(' ').nth(4)?;
            let fs = fs.split(' ').next()?;
            (DISK.contains(&fs) && point != "/").then(|| PathBuf::from(unescape_mount(point)))
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn unescape_mount(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let oct = b
            .get(i + 1..i + 4)
            .filter(|o| o.iter().all(|c| (b'0'..=b'7').contains(c)));
        match oct {
            Some(o) if b[i] == b'\\' => {
                out.push(o.iter().fold(0u8, |n, c| n.wrapping_mul(8) + (c - b'0')));
                i += 4;
            }
            _ => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Opens of an empty drive fail at once instead of asking for a disk (`SEM_FAILCRITICALERRORS`).
#[cfg(windows)]
fn quiet_drive_errors() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetThreadErrorMode(new_mode: u32, old_mode: *mut u32) -> i32;
    }
    let mut old = 0;
    // SAFETY: plain Win32 call; `old` is a valid out pointer.
    unsafe { SetThreadErrorMode(0x0001, &mut old) };
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

/// The webview names a job, not a path, so only what this session published can be revealed.
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

/// A fixed URL, so the webview can't have anything else opened.
#[tauri::command]
async fn open_repo() -> Result<(), String> {
    open_url("https://github.com/quer3q/ps5-dump-forge")
}

/// The version and this build's release asset name (`{v}` the version), for `app/src/update.ts`.
#[tauri::command]
fn update_target() -> HashMap<&'static str, String> {
    HashMap::from([
        ("version", env!("CARGO_PKG_VERSION").to_string()),
        ("asset", release_asset().to_string()),
    ])
}

/// Only a plain `X.Y.Z` makes it into the URL.
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
        "https://github.com/quer3q/ps5-dump-forge/releases/download/v{version}/{}",
        release_asset().replace("{v}", &version)
    ))
}

fn release_asset() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "ps5-dump-forge-{v}-macos-arm64.dmg"
    } else if cfg!(target_os = "macos") {
        "ps5-dump-forge-{v}-macos-x86-64.dmg"
    } else if cfg!(all(windows, target_arch = "x86_64")) {
        "ps5-dump-forge-{v}-windows-x86-64.exe"
    } else if cfg!(windows) {
        "ps5-dump-forge-{v}-windows-arm64.exe"
    } else if cfg!(target_arch = "aarch64") {
        "ps5-dump-forge-{v}-linux-arm64.AppImage"
    } else {
        "ps5-dump-forge-{v}-linux-x86-64.AppImage"
    }
}

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

// Linux has no portable way to select the file, so `xdg-open` opens its folder.
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
    // Explorer exits non-zero even when it worked.
    if cfg!(not(target_os = "windows")) && !status.success() {
        return Err(std::io::Error::other(format!("{status}")));
    }
    Ok(())
}

/// AppImage (`APPDIR` set): `cmd` gets every variable with an entry under `APPDIR` without those entries, and the ones `appimage_start` saved in `FORGE_HOST_VARS` / `FORGE_HOST_<name>` set back (tauri-apps/tauri#10617).
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn host_env(cmd: &mut std::process::Command) {
    let Some(appdir) = std::env::var_os("APPDIR") else {
        return;
    };
    // shortcut: a variable AppRun's hooks also gave a host entry (XDG_DATA_DIRS gets /usr/share)
    // keeps that entry; saving every variable before AppRun would need a wrapper, as forge.sh was.
    for (name, value) in std::env::vars_os() {
        let all: Vec<_> = std::env::split_paths(&value).collect();
        let kept: Vec<_> = all.iter().filter(|p| !p.starts_with(&appdir)).collect();
        if kept.len() == all.len() {
            continue;
        }
        match std::env::join_paths(kept) {
            Ok(v) if !v.is_empty() => cmd.env(name, v),
            _ => cmd.env_remove(name),
        };
    }
    // AppRun's GTK hook's theme pick and linuxdeploy's Python switch: not the host's.
    // ponytail: a GTK_THEME the host had set itself is lost too; AppRun overwrote it already.
    cmd.env_remove("GTK_THEME")
        .env_remove("PYTHONDONTWRITEBYTECODE");
    let Some(vars) = std::env::var_os("FORGE_HOST_VARS") else {
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

/// AppImage: WebKitGTK's DMA-BUF renderer off unless the user set it (blank windows), and on Wayland the host's libwayland-client preloaded by a re-exec (the bundled one can crash against the host's driver stack); `FORGE_HOST_VARS` marks the re-exec'd run.
#[cfg(target_os = "linux")]
fn appimage_start() {
    use std::os::unix::process::CommandExt;
    use std::process::Command;
    let ours = ["WEBKIT_DISABLE_DMABUF_RENDERER", "LD_PRELOAD"];
    if std::env::var_os("APPDIR").is_none() || std::env::var_os("FORGE_HOST_VARS").is_some() {
        return;
    }
    // SAFETY: the first thing main does, while the process has one thread.
    unsafe {
        for name in ours {
            if let Some(v) = std::env::var_os(name) {
                std::env::set_var(format!("FORGE_HOST_{name}"), v);
            }
        }
        std::env::set_var("FORGE_HOST_VARS", ours.join(" "));
        if std::env::var_os(ours[0]).is_none() {
            std::env::set_var(ours[0], "1");
        }
    }
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty())
        || std::env::var_os("XDG_SESSION_TYPE").is_some_and(|v| v == "wayland");
    let tag = if cfg!(target_arch = "x86_64") {
        "x86-64"
    } else {
        "AArch64"
    };
    let ldconfig = |cmd: &str| Command::new(cmd).arg("-p").output();
    let Some(out) = wayland
        .then(|| {
            ldconfig("ldconfig")
                .or_else(|_| ldconfig("/sbin/ldconfig"))
                .ok()
        })
        .flatten()
    else {
        return;
    };
    // `\tlibwayland-client.so.0 (libc6,x86-64) => /lib/x86_64-linux-gnu/libwayland-client.so.0`
    let text = String::from_utf8_lossy(&out.stdout);
    let lib = text.lines().find_map(|line| {
        let named = line.split_whitespace().next() == Some("libwayland-client.so.0");
        let lib = line.rsplit(' ').next()?;
        (named && line.contains(tag) && Path::new(lib).exists()).then(|| lib.to_string())
    });
    let (Some(lib), Ok(exe)) = (lib, std::env::current_exe()) else {
        return;
    };
    let preload = match std::env::var("LD_PRELOAD") {
        Ok(old) if !old.is_empty() => format!("{lib}:{old}"),
        _ => lib,
    };
    // Returns only when the exec fails: then the app runs on without the preload.
    let err = Command::new(exe)
        .args(std::env::args_os().skip(1))
        .env("LD_PRELOAD", preload)
        .exec();
    eprintln!("PS5 Dump Forge: can't restart with the host's libwayland-client: {err}");
}

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

/// `true` to quit now (idle), else the UI asks first.
fn request_quit(app: &AppHandle) -> bool {
    match app.try_state::<AppState>().map_or(Ok(()), |s| s.try_quit()) {
        Ok(()) => true,
        Err(held) => {
            let _ = app.emit(CLOSE_REQUESTED, serde_json::json!({ "peer": held.peer }));
            false
        }
    }
}

/// The default macOS menu, but Quit is ours: the predefined one sends `terminate:`, which exits without asking (tauri-apps/tauri#9198).
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

/// WebView2 on Windows: a runtime in a `WebView2` folder next to the exe, else the installed Evergreen one; a failure ends in a message box and exit code 1 (no console).
#[cfg(windows)]
mod webview2 {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::ptr::null_mut;

    use webview2_com::Microsoft::Web::WebView2::Win32::GetAvailableCoreWebView2BrowserVersionString;
    use webview2_com::{CoTaskMemPWSTR, take_pwstr};

    const EVERGREEN: &str = "https://developer.microsoft.com/microsoft-edge/webview2/";
    /// The Windows 10 sandbox fix for fixed runtimes >= 120: read/execute for ALL RESTRICTED APPLICATION PACKAGES and ALL APPLICATION PACKAGES.
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

    /// Runs first in `main`, before any other thread.
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

    pub fn startup_failed(err: &dyn std::fmt::Display) -> ! {
        match bundled() {
            Some(dir) => fail(&bundled_broken(&dir, &err.to_string())),
            None if !available(None) => fail(&no_runtime()),
            None => fail(&format!(
                "PS5 Dump Forge could not open its window:\n\n{err}\n\n\
                 It keeps its WebView data in the \"data\" folder next to the app, so that \
                 folder must be writable: move the app to a folder of your own (not Program \
                 Files) and start it from there."
            )),
        }
    }

    /// No fallback to Evergreen once it exists: a broken bundled runtime is reported.
    pub fn bundled() -> Option<PathBuf> {
        let dir = std::env::current_exe().ok()?.parent()?.join("WebView2");
        dir.is_dir().then_some(dir)
    }

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

    /// A folder unpacked by hand may carry no ACLs, and without them its processes fail to start on Windows 10.
    // ponytail: runs on each Windows 10 launch (idempotent); skipping it would need a marker file.
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
             Install the Evergreen WebView2 Runtime from\n{EVERGREEN}"
        )
    }

    fn bundled_broken(dir: &Path, detail: &str) -> String {
        format!(
            "The WebView2 runtime in\n{}\nfailed to start:\n\n{detail}\n\n\
             Delete the WebView2 folder to use the installed Evergreen WebView2 Runtime \
             ({EVERGREEN}).\n\n\
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

// ---- Connect: the same calls as the server's `/api/connect/*` ----

fn connect(state: &AppState) -> Result<&Connect, String> {
    state.connect.as_deref().map_err(Clone::clone)
}

/// An error is `connect::error_text` (a refusal's code, which the UI translates, else the text).
async fn connect_call<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&Connect) -> std::io::Result<T> + Send + 'static,
) -> Result<T, String> {
    blocking(app, move |_, state| {
        f(connect(state)?).map_err(|e| connect::error_text(&e))
    })
    .await
}

#[tauri::command]
async fn connect_state(app: AppHandle) -> Result<serde_json::Value, String> {
    connect_call(app, |c| Ok(c.snapshot())).await
}

#[tauri::command(rename_all = "snake_case")]
async fn connect_cancel_job(app: AppHandle, key_id: String, job: JobId) -> Result<bool, String> {
    connect_call(app, move |c| Ok(c.cancel_remote_job(&key_id, job))).await
}

#[tauri::command]
async fn connect_install(app: AppHandle, key_id: String, path: PathBuf) -> Result<JobId, String> {
    blocking(app, move |_, state| connect(state)?.install(&key_id, &path)).await
}

/// 128 MiB each way over FLINK's lanes, nothing written to disk; `run` is the UI's id for this test.
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

/// A run not started yet ends `cancelled` as it starts.
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

#[tauri::command(rename_all = "snake_case")]
async fn connect_pair(app: AppHandle, address: String) -> Result<(), String> {
    connect_call(app, move |c| c.pair(&address)).await
}

#[tauri::command(rename_all = "snake_case")]
async fn connect_probe(
    app: AppHandle,
    address: String,
) -> Result<ps5_dump_forge_server::probe::Probe, String> {
    connect_call(app, move |c| c.probe(&address)).await
}

/// The ELF downloaded into memory here and streamed to elfldr; the console needs no internet.
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

/// Only here, in its own window: the app has no HTTP listener.
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

/// `user`/`password` omitted keep the saved ones.
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
    #[cfg(target_os = "linux")]
    appimage_start();
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
            // A peer's writes never land in what this device's unfinished jobs use.
            if let Ok(connect) = &connect {
                let handle = app.handle().clone();
                connect.set_busy(Arc::new(move |path| {
                    (handle.try_state::<AppState>())
                        .is_some_and(|s| s.jobs.get().is_some_and(|jobs| jobs.uses(path)))
                }));
                // A peer's replacing rename is refused as a job's replace is; the runner is made if none is yet.
                let handle = app.handle().clone();
                connect.set_replace_check(Arc::new(move |path| {
                    match handle.try_state::<AppState>() {
                        Some(s) => (s.jobs(&handle).replace_check(path)).map_err(|e| e.to_string()),
                        None => Err("PS5 Dump Forge is starting; try again".to_string()),
                    }
                }));
                // A peer's Delete and Move to here are this device's own, refused while quitting.
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

            // Tauri 2's drag and drop handler stays on: Edit hears host files through `onDragDropEvent`.
            let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title(concat!("PS5 Dump Forge v", env!("CARGO_PKG_VERSION")))
                .inner_size(1200.0, 820.0)
                .min_inner_size(900.0, 620.0);
            // WebView data goes to `<app dir>/data/webview`, except on macOS (WebKit's own per-app store).
            #[cfg(not(target_os = "macos"))]
            let window = match webview_dir {
                Some(dir) => window.data_directory(dir),
                None => window,
            };
            // Tauri would only panic on an error here, which a Windows release build never shows.
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
            host_tree,
            pack_estimate,
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
            free_space,
            home_folder,
            bundled_ng_version,
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
