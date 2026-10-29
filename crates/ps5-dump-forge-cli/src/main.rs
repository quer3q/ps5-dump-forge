//! `ps5-dump-forge`: the conversion core for scripts and CI. Progress goes to stdout as one JSON
//! `Event` per line; a human summary goes to stderr.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use ps5_dump_forge_core::{
    ArchivePassword, ConvertRequest, Event, Format, Jobs, KrakenLevel, Lz4Mode, Lz4Runtime,
    VerifyMode,
};

const USAGE: &str = "\
usage: ps5-dump-forge inspect <path> [--json] [--password-stdin]
       ps5-dump-forge convert <source> --to folder|exfat|ffpkg|ffpfs|ffpfsc|pkg|lz4
                         [--password-stdin]
                         [--inner exfat|ffpkg|ffpfs] [-o <output>] [--threads N]
                         [--remove-backport] [--full-verify]
                         [--kraken fast|balanced|smallest|stored]
                         [--level 0..9] [--lz4-unpack | --lz4-pack]
                         [--lz4-level 0|3..12] [--lz4-runtime ng|ampr_emu]
       ps5-dump-forge convert <source> --to folder|exfat|ffpkg|ffpfs|ffpfsc --zstd-pack
                         [--zstd-level 1..22]
       ps5-dump-forge convert <zstd_packed> --to <format> --zstd-unpack
       ps5-dump-forge serve [--port N] --root <dir> [--root <dir>]...

  inspect   describe a game folder, image (.exfat, .ffpkg, .ffpfs, .ffpfsc, .pkg) or archive
            holding one game folder (.zip, .7z, .rar; the first part of a multi-part set)
  convert   convert a source; progress is printed as JSON lines on stdout. An archive is
            read in place, with nothing unpacked to disk
  --password-stdin
            an archive's password: one line read from stdin (default: the
            FORGE_ARCHIVE_PASSWORD environment variable; never an argument)
  --to      folder, exfat, ffpkg (what ShadowMountPlus recommends), ffpfs (PFS image),
            ffpfsc (compressed PFS container, mounted read-only), or pkg = debug FPKG
            (shown as .fpkg in the app; the file still ends in .pkg), or lz4 = an LZ4 packed
            folder (AMPR asset packs; titles that import libSceAmpr; named like a folder)
  --inner   with --to ffpfsc: the image inside (default ffpkg)
  -o        output path (default: named from the title id, next to the source); an
            output's name without its extension must stay within 63 bytes (58 for
            .ffpfsc), or ShadowMountPlus can't mount it
  --threads compression threads for pkg and ffpfsc (default: all cores)
  --remove-backport
            leave out fakelib's backport libraries, keeping its emulators (AMPR, DLC,
            PlayGo); refused when eboot.bin's SDK was lowered
  --full-verify
            re-read every byte of the output to verify it (takes longer); by default
            verification is fast: every structural check, small files whole and a random
            sample of the rest
  --level   with --to ffpfsc: zlib level, 0 (store) to 9 (smallest), default 6
  --kraken  with --to pkg: compression level (default fast). balanced and smallest make a
            package ~2.6% smaller but take ~6x and ~9x longer, every core busy throughout;
            stored compresses nothing (a far larger package), for a console that refuses
            to start a compressed one
  --lz4-pack
            pack the assets into LZ4 packs inside the target: --to folder (the same as
            --to lz4), exfat, ffpkg, ffpfs or ffpfsc (not pkg). Into an image the packs go
            straight in, with no temporary folder; the packed files are read twice
  --zstd-pack
            pack the assets into zstd packs (ampr_zstd.index + ampr_zstd-NNN.pak, zstd in 256 KiB
            chunks) and install the zstd runtime at fakelib/libSceAmpr.sprx: --to folder,
            exfat, ffpkg, ffpfs or ffpfsc (not pkg; into an image straight in, the packed
            files read twice), titles that import libSceAmpr; no LZ4 option. The runtime is
            ampr-emu-ng: turn off ShadowMount+'s fakelib update, or it replaces the runtime
  --zstd-level
            with --zstd-pack: zstd level, 1 (fastest) to 22 (smallest), default 5
  --zstd-unpack
            decode a zstd-packed game's packs into plain files, for any target but lz4;
            the release runtime replaces the zstd one and ampr_emu.index is written anew
  --lz4-level
            with --to lz4 or --lz4-pack: 0 fast (default), 3..12 LZ4 HC (9 medium, 12 max)
  --lz4-runtime
            with --to lz4 or --lz4-pack: the runtime installed, ng (ampr-emu-ng, default;
            reads LZ4 and zstd packs) or ampr_emu (the bundled upstream release build)
  --lz4-unpack
            unpack LZ4 packs to plain files first, for any target but lz4 (which always
            unpacks first)
  serve     the web UI on http://<this computer>:<port> (default 8095); the file browser
            starts at the --root folders (on the PS5: /data and /mnt/usb0..7, /mnt/ext0..1
            while mounted, no --root; nothing else is listed). No protections: anyone on the
            network can use it
  -h, --help";

// One value per run: Convert's size (over clippy's limit on Windows, whose PathBuf is larger) costs nothing.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
enum Cli {
    Inspect {
        path: PathBuf,
        json: bool,
        password_stdin: bool,
    },
    Convert {
        request: ConvertRequest,
        password_stdin: bool,
    },
    Serve {
        port: u16,
        roots: Vec<PathBuf>,
    },
    Help,
}

/// Paths stay `OsString`: a valid Unix path need not be UTF-8.
fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Cli, String> {
    let mut args = args.into_iter();
    let command = args.next().ok_or("missing command")?;
    let command = command.to_str().ok_or("unknown command")?.to_string();
    if matches!(command.as_str(), "-h" | "--help" | "help") {
        return Ok(Cli::Help);
    }
    if command == "serve" {
        return parse_serve(args);
    }
    let mut positional = Vec::new();
    let (mut json, mut to, mut output) = (false, None, None);
    let (mut threads, mut inner, mut remove_backport) = (None, None, false);
    let mut full_verify = false;
    let mut kraken_level = None;
    let mut ffpfsc_level = None;
    let (mut lz4_unpack, mut lz4_pack) = (false, false);
    let (mut lz4_level, mut lz4_runtime) = (None, None);
    let (mut zstd_pack, mut zstd_level, mut zstd_unpack) = (false, None, false);
    let mut password_stdin = false;
    while let Some(arg) = args.next() {
        let mut value = |flag: &str| args.next().ok_or(format!("{flag} needs a value"));
        let text = |v: OsString| v.into_string().map_err(|_| "option values must be text");
        match arg.to_str().unwrap_or("") {
            "-h" | "--help" => return Ok(Cli::Help),
            "--json" => json = true,
            "--password-stdin" => password_stdin = true,
            "--remove-backport" => remove_backport = true,
            "--full-verify" => full_verify = true,
            "--to" => to = Some(parse_format(&text(value("--to")?)?)?),
            "--inner" => inner = Some(parse_format(&text(value("--inner")?)?)?),
            "--level" => {
                ffpfsc_level = Some(
                    text(value("--level")?)?
                        .parse::<u32>()
                        .ok()
                        .filter(|l| *l <= 9)
                        .ok_or("--level takes 0 through 9")?,
                )
            }
            "--kraken" => {
                kraken_level = Some(match text(value("--kraken")?)?.as_str() {
                    "fast" => KrakenLevel::Fast,
                    "balanced" => KrakenLevel::Balanced,
                    "smallest" => KrakenLevel::Smallest,
                    "stored" => KrakenLevel::Stored,
                    _ => return Err("--kraken takes fast, balanced, smallest or stored".into()),
                })
            }
            "--lz4-unpack" => lz4_unpack = true,
            "--lz4-pack" => lz4_pack = true,
            "--lz4-level" => {
                lz4_level = Some(
                    text(value("--lz4-level")?)?
                        .parse::<u8>()
                        .ok()
                        .filter(|l| *l == 0 || (3..=12).contains(l))
                        .ok_or("--lz4-level takes 0 or 3 through 12")?,
                )
            }
            "--lz4-runtime" => {
                lz4_runtime = Some(match text(value("--lz4-runtime")?)?.as_str() {
                    "ng" => Lz4Runtime::Ng,
                    "ampr_emu" => Lz4Runtime::AmprEmu,
                    _ => return Err("--lz4-runtime takes ng or ampr_emu".into()),
                })
            }
            "--zstd-pack" => zstd_pack = true,
            "--zstd-unpack" => zstd_unpack = true,
            "--zstd-level" => {
                zstd_level = Some(
                    text(value("--zstd-level")?)?
                        .parse::<u8>()
                        .ok()
                        .filter(|l| (1..=22).contains(l))
                        .ok_or("--zstd-level takes 1 through 22")?,
                )
            }
            "-o" | "--output" => output = Some(PathBuf::from(value("-o")?)),
            "--threads" => {
                threads = Some(
                    text(value("--threads")?)?
                        .parse()
                        .map_err(|_| "--threads needs a number")?,
                )
            }
            flag if flag.starts_with('-') && flag != "-" => {
                return Err(format!("unknown option {flag:?}"));
            }
            _ => positional.push(PathBuf::from(arg)),
        }
    }
    let [path] = <[PathBuf; 1]>::try_from(positional)
        .map_err(|p| format!("expected one path, got {}", p.len()))?;
    match command.as_str() {
        "inspect" => Ok(Cli::Inspect {
            path,
            json,
            password_stdin,
        }),
        "convert" => {
            let format = to.ok_or("convert needs --to")?;
            match inner {
                Some(_) if format != Format::Ffpfsc => {
                    return Err("--inner only goes with --to ffpfsc".into());
                }
                Some(Format::Exfat | Format::Ffpkg | Format::Ffpfs) | None => {}
                Some(_) => return Err("--inner takes exfat, ffpkg or ffpfs".into()),
            }
            if kraken_level.is_some() && format != Format::Pkg {
                return Err("--kraken only goes with --to pkg".into());
            }
            if ffpfsc_level.is_some() && format != Format::Ffpfsc {
                return Err("--level only goes with --to ffpfsc".into());
            }
            if lz4_unpack && lz4_pack {
                return Err("--lz4-unpack and --lz4-pack exclude each other".into());
            }
            if lz4_pack && format == Format::Lz4 {
                return Err("--lz4-pack is redundant: --to lz4 packs".into());
            }
            if lz4_pack && format == Format::Pkg {
                return Err(
                    "--lz4-pack goes with --to folder, exfat, ffpkg, ffpfs or ffpfsc; \
                     packing into a pkg is not supported yet"
                        .into(),
                );
            }
            let packing = lz4_pack || format == Format::Lz4;
            if lz4_unpack && format == Format::Lz4 {
                return Err(
                    "--lz4-unpack is redundant: the LZ4 target always unpacks first".into(),
                );
            }
            if lz4_level.is_some() && !packing {
                return Err("--lz4-level only goes with --to lz4 or --lz4-pack".into());
            }
            if lz4_runtime.is_some() && !packing {
                return Err("--lz4-runtime only goes with --to lz4 or --lz4-pack".into());
            }
            if zstd_level.is_some() && !zstd_pack {
                return Err("--zstd-level only goes with --zstd-pack".into());
            }
            if zstd_pack && matches!(format, Format::Lz4 | Format::Pkg) {
                return Err(
                    "--zstd-pack goes with --to folder, exfat, ffpkg, ffpfs or ffpfsc".into(),
                );
            }
            let any_lz4 = lz4_unpack || lz4_pack || lz4_level.is_some() || lz4_runtime.is_some();
            if zstd_pack && any_lz4 {
                return Err("--zstd-pack takes no LZ4 option".into());
            }
            if zstd_unpack && zstd_pack {
                return Err("--zstd-unpack and --zstd-pack exclude each other".into());
            }
            if zstd_unpack && (any_lz4 || format == Format::Lz4) {
                return Err("--zstd-unpack takes no LZ4 option and not --to lz4".into());
            }
            // No -o: named in `main` once the archive password is known (an encrypted archive's
            // title is inside it).
            let output = output.unwrap_or_default();
            Ok(Cli::Convert {
                request: ConvertRequest {
                    source: path,
                    format,
                    output,
                    compression_threads: threads,
                    inner,
                    remove_backport,
                    full_verify,
                    kraken_level: kraken_level.unwrap_or_default(),
                    ffpfsc_level: ffpfsc_level.unwrap_or(ps5_dump_forge_core::DEFAULT_FFPFSC_LEVEL),
                    lz4: if lz4_unpack {
                        Some(Lz4Mode::Unpack)
                    } else if lz4_pack {
                        Some(Lz4Mode::Pack)
                    } else {
                        None
                    },
                    lz4_level: lz4_level.unwrap_or(0),
                    lz4_runtime: lz4_runtime.unwrap_or_default(),
                    zstd_pack,
                    zstd_level: zstd_level.unwrap_or(ps5_dump_forge_core::DEFAULT_ZSTD_LEVEL),
                    zstd_unpack,
                    edits: Vec::new(),
                    extract: Vec::new(),
                    transfer: Vec::new(),
                    replace: false,
                    edit_rebuild: false,
                    install: None,
                    archive_password: None,
                },
                password_stdin,
            })
        }
        other => Err(format!("unknown command {other:?}")),
    }
}

fn parse_serve(mut args: impl Iterator<Item = OsString>) -> Result<Cli, String> {
    let (mut port, mut roots) = (ps5_dump_forge_server::DEFAULT_PORT, Vec::new());
    while let Some(arg) = args.next() {
        let mut value = |flag: &str| args.next().ok_or(format!("{flag} needs a value"));
        match arg.to_str().unwrap_or("") {
            "-h" | "--help" => return Ok(Cli::Help),
            "--port" => {
                port = value("--port")?
                    .to_str()
                    .and_then(|p| p.parse().ok())
                    .ok_or("--port needs a number")?
            }
            "--root" => roots.push(PathBuf::from(value("--root")?)),
            other => return Err(format!("unknown argument {other:?} for serve")),
        }
    }
    // The PS5 serves its fixed folders; a computer only what it is told to.
    if cfg!(target_env = "ps5") {
        if !roots.is_empty() {
            return Err("--root is not used on the PS5".into());
        }
    } else if roots.is_empty() {
        return Err("serve needs at least one --root".into());
    }
    Ok(Cli::Serve { port, roots })
}

fn parse_format(s: &str) -> Result<Format, String> {
    serde_json::from_value(serde_json::Value::String(s.to_ascii_lowercase())).map_err(|_| {
        format!("unknown format {s:?} (folder, exfat, ffpkg, ffpfs, ffpfsc, pkg, lz4)")
    })
}

/// Next to the source (core's empty `dir`), named from its title id.
fn default_output(
    source: &Path,
    format: Format,
    password: Option<&str>,
) -> Result<PathBuf, String> {
    ps5_dump_forge_core::default_output_with(source, format, Path::new(""), password)
        .map_err(|e| format!("{e:#}"))
}

// scripts/release-ps5.sh looks for the server's marker in the PS5 ELF to check that it is this
// version; this keeps it linked in every build (one literal: the self copy slices its prefix).
#[used]
static VERSION_MARKER: &&[u8] = &ps5_dump_forge_server::VERSION_MARKER;

// The PS5 libc serializes every malloc across threads (ps5-dump-forge-ps5's docs).
#[cfg(target_env = "ps5")]
#[global_allocator]
static ALLOC: ps5_dump_forge_ps5::Pool = ps5_dump_forge_ps5::Pool;

// The PS5 kernel fills FreeBSD 11 structs; libc's default FreeBSD 12 layouts read every field
// at the wrong offset (found on hardware: every path looked like neither a file nor a folder).
// ps5/build.sh selects the FreeBSD 11 ABI; these fail the build if that is ever lost.
#[cfg(target_env = "ps5")]
const _: () = {
    assert!(std::mem::size_of::<libc::stat>() == 120);
    assert!(std::mem::size_of::<libc::dirent>() == 264);
    // MNAMELEN 88 (FreeBSD 12+: 1024): free space, mount points, filesystem types.
    assert!(std::mem::size_of::<libc::statfs>() == 472);
};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
/// Set once the job is cancelled or done and its `.part` cleaned up; Windows' close
/// handler waits for it.
static CLEANED_UP: AtomicBool = AtomicBool::new(false);

// ponytail: a plain signal handler that only sets a flag; the main loop polls it.
#[cfg(unix)]
fn on_ctrl_c() {
    extern "C" fn handler(_: libc::c_int) {
        INTERRUPTED.store(true, Ordering::Relaxed);
    }
    // SAFETY: the handler only stores to an atomic, which is async-signal-safe.
    unsafe {
        libc::signal(libc::SIGINT, handler as *const () as libc::sighandler_t);
        libc::signal(libc::SIGTERM, handler as *const () as libc::sighandler_t);
    }
}

/// Ctrl-C and Ctrl-Break only set the flag. Closing the console, logging off or shutting
/// down ends the process once the handler returns (or about 5 s later), so the handler
/// waits there, up to 4 s, for the main loop to cancel the job and clean up.
// ponytail: a cleanup slower than the 4 s wait is cut off by Windows; the `.part` survives
// and `stale_parts` lists it.
#[cfg(windows)]
fn on_ctrl_c() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetConsoleCtrlHandler(handler: extern "system" fn(u32) -> i32, add: i32) -> i32;
    }
    extern "system" fn handler(event: u32) -> i32 {
        const CTRL_BREAK_EVENT: u32 = 1;
        INTERRUPTED.store(true, Ordering::Relaxed);
        if event > CTRL_BREAK_EVENT {
            let start = std::time::Instant::now();
            while !CLEANED_UP.load(Ordering::Relaxed) && start.elapsed() < Duration::from_secs(4) {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        1
    }
    // SAFETY: the handler is a plain function for the life of the process; it runs on a
    // thread of its own and touches only atomics.
    if unsafe { SetConsoleCtrlHandler(handler, 1) } == 0 {
        let _ = writeln!(
            std::io::stderr(),
            "ps5-dump-forge: no Ctrl-C handler ({}); an interrupted job leaves its .part",
            std::io::Error::last_os_error()
        );
    }
}

#[cfg(not(any(unix, windows)))]
fn on_ctrl_c() {}

fn main() -> ExitCode {
    let cli = match parse_args(std::env::args_os().skip(1)) {
        Ok(cli) => cli,
        Err(e) => {
            eprintln!("ps5-dump-forge: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match cli {
        Cli::Help => print_out(&format!("{USAGE}\n")),
        Cli::Inspect {
            path,
            json,
            password_stdin,
        } => match archive_password(password_stdin) {
            Ok(password) => inspect(&path, password.as_ref(), json),
            Err(e) => usage_error(&e),
        },
        Cli::Convert {
            mut request,
            password_stdin,
        } => match archive_password(password_stdin) {
            Ok(password) => {
                request.archive_password = password;
                if request.output.as_os_str().is_empty() {
                    let password = request.archive_password.as_ref().map(|p| p.as_str());
                    match default_output(&request.source, request.format, password) {
                        Ok(o) => request.output = o,
                        Err(e) => return usage_error(&e),
                    }
                }
                convert(request)
            }
            Err(e) => usage_error(&e),
        },
        Cli::Serve { port, roots } => serve(port, roots),
    }
}

fn usage_error(e: &str) -> ExitCode {
    eprintln!("ps5-dump-forge: {e}");
    ExitCode::from(2)
}

/// An archive's password: one line of stdin with `--password-stdin`, else
/// `FORGE_ARCHIVE_PASSWORD`. Never an argument (argv is visible to other users). Empty = none.
fn archive_password(from_stdin: bool) -> Result<Option<ArchivePassword>, String> {
    let password = if from_stdin {
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .map_err(|e| format!("--password-stdin: {e}"))?;
        let line = line.strip_suffix('\n').unwrap_or(&line);
        line.strip_suffix('\r').unwrap_or(line).to_string()
    } else {
        match std::env::var("FORGE_ARCHIVE_PASSWORD") {
            Ok(p) => p,
            Err(std::env::VarError::NotPresent) => String::new(),
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err("FORGE_ARCHIVE_PASSWORD must be text".into());
            }
        }
    };
    Ok((!password.is_empty()).then(|| ArchivePassword::new(password)))
}

fn serve(port: u16, roots: Vec<PathBuf>) -> ExitCode {
    use ps5_dump_forge_server::{Options, PS5_ROOTS, Platform};
    // `parse_serve` refuses `--root` on the PS5, which serves its fixed folders.
    let (roots, platform) = if cfg!(target_env = "ps5") {
        (PS5_ROOTS.iter().map(PathBuf::from).collect(), Platform::Ps5)
    } else {
        (roots, Platform::Host)
    };
    let mut opts = Options::new(port, roots, platform, notify);
    opts.connect_dir = ps5_dump_forge_server::connect::default_dir();
    if platform == Platform::Ps5 {
        opts.settings_dir = Some(PathBuf::from(ps5_dump_forge_server::FORGE_DIR));
    }
    on_ctrl_c();
    opts.interrupt = Some(&INTERRUPTED);
    match ps5_dump_forge_server::serve(opts) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ps5-dump-forge: serve: {e}");
            ExitCode::FAILURE
        }
    }
}

/// A PS5 notification (`ps5/entry.c`).
#[cfg(target_env = "ps5")]
fn notify(line: &str) {
    unsafe extern "C" {
        fn ps5_notify(message: *const std::ffi::c_char) -> std::ffi::c_int;
    }
    eprintln!("{line}");
    let text = std::ffi::CString::new(line.replace('\0', "")).unwrap_or_default();
    // SAFETY: a NUL-terminated string that outlives the call; entry.c copies it.
    unsafe { ps5_notify(text.as_ptr()) };
}

#[cfg(not(target_env = "ps5"))]
fn notify(line: &str) {
    eprintln!("{line}");
}

fn inspect(path: &Path, password: Option<&ArchivePassword>, json: bool) -> ExitCode {
    let found = match ps5_dump_forge_core::inspect_with(path, password.map(|p| p.as_str())) {
        Ok(found) => found,
        Err(e) => {
            eprintln!("ps5-dump-forge: {e:#}");
            return ExitCode::FAILURE;
        }
    };
    if json {
        return match serde_json::to_string_pretty(&found) {
            Ok(text) => print_out(&(text + "\n")),
            Err(e) => {
                eprintln!("ps5-dump-forge: {e}");
                ExitCode::FAILURE
            }
        };
    }
    let or_none = |v: &Option<String>| v.clone().unwrap_or_else(|| "-".to_string());
    let backport = match (found.backport.len(), &found.backport_blocked) {
        (0, _) => "no".to_string(),
        (n, blocked) => format!(
            "yes, firmware {} or later from its executables, {n} {} (fakelib/); {}",
            or_none(&found.backport_firmware),
            if n == 1 { "library" } else { "libraries" },
            blocked
                .as_deref()
                .unwrap_or("removable (--remove-backport)")
        ),
    };
    let mut names: Vec<&str> = found.emulators.iter().map(|e| e.name.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    let emulators = match names.len() {
        0 => "none".to_string(),
        _ => format!(
            "{} ({} files in fakelib/)",
            names.join(", "),
            found.emulators.len()
        ),
    };
    let mut text = format!(
        "{} ({})\ntitle:      {}\ntitle id:   {}\ncontent id: {}\nversion:    {}\n\
         firmware:   {} (sdk {})\nbackport:   {backport}\nemulators:  {emulators}\n\
         files:      {} ({} bytes), {} empty dirs\n",
        found.describe,
        found.kind,
        or_none(&found.title_name),
        or_none(&found.title_id),
        or_none(&found.content_id),
        or_none(&found.version),
        or_none(&found.firmware),
        or_none(&found.sdk),
        found.files.len(),
        found.total_bytes,
        found.empty_dirs.len()
    );
    if let Some(v) = &found.forge_version {
        text += &format!("forge:      v{v}\n");
    }
    if let Some(line) = found.lz4.as_ref().map(|f| lz4_line(f, found.zstd_packed)) {
        text += &format!("{line}\n");
    }
    if !found.dlcs.is_empty() {
        text += &format!("dlc:        {}\n", found.dlcs.len());
        for d in &found.dlcs {
            let name = d.name.as_deref().unwrap_or(&d.label);
            let place = match (&d.folder, &d.emulated) {
                (Some(folder), _) => folder.clone(),
                (None, Some(status)) => format!("(dlc_emu.ini {status})"),
                (None, None) => "(merged into the game)".to_string(),
            };
            text += &format!("  {name}  {}  {place}\n", d.content_id);
        }
    }
    for line in &found.details {
        text += &format!("  {line}\n");
    }
    if found.findings.is_empty() {
        text += "findings:   none\n";
    } else {
        text += "findings:\n";
        for line in &found.findings {
            text += &format!("  {line}\n");
        }
    }
    print_out(&text)
}

/// `12431` as `12,431`.
fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The one `inspect` line for a source's LZ4 facts.
fn lz4_line(f: &ps5_dump_forge_core::Lz4Facts, zstd_packed: bool) -> String {
    let runtime = match f.runtime.as_str() {
        "forge_release" => "Forge release",
        // Forge's old trace runtime: still told apart, so a conversion replaces it.
        "forge_trace" => "Forge trace",
        "zstd" => "zstd",
        "other" => "other",
        _ => "none",
    };
    let state = if let Some(p) = &f.packed {
        let share = p
            .stored_percent
            .map_or(String::new(), |s| format!(", {s}% of size"));
        format!(
            "packed, {} files in {} volumes{share}",
            grouped(p.files),
            grouped(p.volumes)
        )
    } else if let Some(e) = &f.manifest_error {
        format!("damaged packs ({e})")
    } else {
        format!("plain, runtime {runtime}")
    };
    let ampr = if f.imports_ampr {
        ""
    } else {
        " (eboot.bin does not import libSceAmpr)"
    };
    let index = match f.index {
        Some(i @ ("stale" | "damaged" | "missing")) => {
            format!(", ampr_emu.index {i} (a conversion writes a new one)")
        }
        _ => String::new(),
    };
    let shipped = f.shipped_runtime_version;
    let action = match (f.runtime_action, f.runtime.as_str()) {
        // Beside zstd packs core installs its zstd runtime, not ampr_emu (`lz4::plain`).
        (Some("install" | "replace"), _) if zstd_packed => {
            ", a conversion installs the zstd runtime".to_owned()
        }
        (Some("install"), _) => format!(", a conversion installs ampr_emu {shipped}"),
        (Some("replace"), _) => {
            format!(", a conversion replaces the runtime with ampr_emu {shipped}")
        }
        // Core keeps another runtime only when it names a version at least the bundled one.
        (Some("keep"), "other") => {
            format!(
                ", ampr_emu {} kept",
                f.runtime_version.as_deref().unwrap_or_default()
            )
        }
        _ => String::new(),
    };
    format!("LZ4 (AMPR): {state}{action}{index}{ampr}")
}

/// stdout, without the panic `print!` has on a closed pipe.
fn print_out(text: &str) -> ExitCode {
    match std::io::stdout().lock().write_all(text.as_bytes()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

fn convert(request: ConvertRequest) -> ExitCode {
    on_ctrl_c();
    let (tx, rx) = mpsc::channel();
    let jobs = Jobs::new(move |event| {
        // Never `println!`: it panics on a closed pipe (`| head`). A dead stdout cancels
        // the job instead, and the result still reaches the main thread.
        if let Ok(line) = serde_json::to_string(&event)
            && writeln!(std::io::stdout().lock(), "{line}").is_err()
        {
            INTERRUPTED.store(true, Ordering::Relaxed);
        }
        if let Event::Done { result, .. } = event {
            let _ = tx.send(result);
        }
    });
    eprintln!(
        "ps5-dump-forge: {} -> {}",
        request.source.display(),
        request.output.display()
    );
    let job = jobs.start(request);
    let mut cancelled = false;
    let result = loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(result) => break result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if !cancelled && INTERRUPTED.load(Ordering::Relaxed) {
                    // Cancel first; a closing console can fail the write, which must not panic.
                    jobs.cancel(job);
                    cancelled = true;
                    let _ = writeln!(std::io::stderr(), "ps5-dump-forge: cancelling...");
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break Err("job vanished".to_string()),
        }
    };
    jobs.cancel_all_and_wait();
    CLEANED_UP.store(true, Ordering::Relaxed);
    match result {
        Ok(report) => {
            eprintln!(
                "ps5-dump-forge: wrote {} ({} files, {} bytes)",
                report.output.display(),
                report.files,
                report.bytes
            );
            for check in &report.checks {
                eprintln!("  ok: {check}");
            }
            let v = &report.verify;
            match v.mode {
                VerifyMode::Fast => eprintln!(
                    "ps5-dump-forge: Fast verification passed: {} of {} bytes in {} samples \
                     (seed {}); --full-verify re-reads every byte",
                    v.checked_bytes, v.total_bytes, v.samples, v.seed
                ),
                VerifyMode::Full => eprintln!(
                    "ps5-dump-forge: Full verification passed: {} bytes",
                    v.checked_bytes
                ),
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("ps5-dump-forge: failed: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<OsString> {
        s.split_whitespace().map(OsString::from).collect()
    }

    #[test]
    fn parses_convert() {
        let Ok(Cli::Convert { request: r, .. }) =
            parse_args(args("convert /g --to exfat -o /o/x.exfat --threads 2"))
        else {
            panic!("not a convert");
        };
        assert_eq!(r.format, Format::Exfat);
        assert_eq!(r.output, PathBuf::from("/o/x.exfat"));
        assert_eq!(r.compression_threads, Some(2));
        let Ok(Cli::Convert { request: r, .. }) =
            parse_args(args("convert /g --to PKG -o /o/x.pkg"))
        else {
            panic!("not a convert");
        };
        assert_eq!(r.format, Format::Pkg);
        assert_eq!(r.inner, None);
        let Ok(Cli::Convert { request: r, .. }) =
            parse_args(args("convert /g --to ffpfsc --inner ffpkg -o /o/x"))
        else {
            panic!("not a convert");
        };
        assert_eq!((r.format, r.inner), (Format::Ffpfsc, Some(Format::Ffpkg)));
        assert!(!r.remove_backport);
        assert!(!r.full_verify, "fast by default");
        let Ok(Cli::Convert { request: r, .. }) =
            parse_args(args("convert /g --to folder --remove-backport -o x"))
        else {
            panic!("not a convert");
        };
        assert!(r.remove_backport);
        let Ok(Cli::Convert { request: r, .. }) =
            parse_args(args("convert /g --to ffpkg --full-verify -o x"))
        else {
            panic!("not a convert");
        };
        assert!(r.full_verify);
        assert_eq!(r.kraken_level, KrakenLevel::Fast, "fast by default");
        let Ok(Cli::Convert { request: r, .. }) =
            parse_args(args("convert /g --to pkg --kraken smallest -o x"))
        else {
            panic!("not a convert");
        };
        assert_eq!(r.kraken_level, KrakenLevel::Smallest);
        let Ok(Cli::Convert { request: r, .. }) =
            parse_args(args("convert /g --to pkg --kraken stored -o x"))
        else {
            panic!("not a convert");
        };
        assert_eq!(r.kraken_level, KrakenLevel::Stored);
        let Ok(Cli::Convert { request: r, .. }) =
            parse_args(args("convert /g --to ffpfsc --level 0 -o x"))
        else {
            panic!("not a convert");
        };
        assert_eq!(r.ffpfsc_level, 0);
    }

    #[test]
    fn rejects_bad_args() {
        assert!(parse_args(args("convert /g")).is_err());
        assert!(parse_args(args("convert /g --to zip -o x")).is_err());
        assert!(parse_args(args("convert /g --to exfat --inner exfat -o x")).is_err());
        assert!(parse_args(args("convert /g --to ffpfsc --inner pkg -o x")).is_err());
        assert!(parse_args(args("convert /g --to ffpfsc --inner ffpfsc -o x")).is_err());
        assert!(parse_args(args("inspect")).is_err());
        assert!(parse_args(args("inspect a b")).is_err());
        assert!(parse_args(args("frob /g")).is_err());
        assert!(parse_args(args("inspect /g --bogus")).is_err());
        assert!(parse_args(args("convert /g --to exfat --full-verify=yes -o x")).is_err());
        assert!(parse_args(args("convert /g --to pkg --kraken medium -o x")).is_err());
        assert!(parse_args(args("convert /g --to exfat --kraken fast -o x")).is_err());
        assert!(parse_args(args("convert /g --to ffpfsc --level 10 -o x")).is_err());
        assert!(parse_args(args("convert /g --to ffpfsc --level -1 -o x")).is_err());
        assert!(parse_args(args("convert /g --to exfat --level 5 -o x")).is_err());
        assert!(matches!(
            parse_args(args("inspect /g --json")),
            Ok(Cli::Inspect { path, json: true, password_stdin: false }) if path == Path::new("/g")
        ));
        assert!(matches!(parse_args(args("help")), Ok(Cli::Help)));
    }

    fn convert(s: &str) -> Result<ConvertRequest, String> {
        match parse_args(args(s)) {
            Ok(Cli::Convert { request: r, .. }) => Ok(r),
            Ok(_) => panic!("not a convert"),
            Err(e) => Err(e),
        }
    }

    #[test]
    fn parses_lz4_level() {
        assert_eq!(convert("convert /g --to lz4 -o x").unwrap().lz4_level, 0);
        let r = convert("convert /g --to ffpkg --lz4-pack --lz4-level 12 -o x").unwrap();
        assert_eq!(r.lz4_level, 12);
        assert_eq!(r.lz4_runtime, Lz4Runtime::Ng);
        let r = convert("convert /g --to lz4 --lz4-runtime ampr_emu -o x").unwrap();
        assert_eq!(r.lz4_runtime, Lz4Runtime::AmprEmu);
        for bad in [
            "convert /g --to lz4 --lz4-level 2 -o x",
            "convert /g --to lz4 --lz4-level 13 -o x",
            "convert /g --to folder --lz4-level 9 -o x",
            "convert /g --to lz4 --lz4-runtime zstd -o x",
            "convert /g --to folder --lz4-runtime ng -o x",
        ] {
            assert!(convert(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn parses_zstd_pack() {
        let r = convert("convert /g --to folder --zstd-pack -o x").unwrap();
        assert_eq!((r.zstd_pack, r.zstd_level, r.lz4), (true, 5, None));
        let r = convert("convert /g --to folder --zstd-pack --zstd-level 22 -o x").unwrap();
        assert_eq!((r.zstd_pack, r.zstd_level), (true, 22));
        let r = convert("convert /g --to ffpkg --zstd-pack -o x").unwrap();
        assert!(r.zstd_pack && r.format == Format::Ffpkg);
        let r = convert("convert /g --to folder -o x").unwrap();
        assert!(!r.zstd_pack && !r.zstd_unpack);
        let r = convert("convert /g --to pkg --zstd-unpack -o x").unwrap();
        assert!(r.zstd_unpack && !r.zstd_pack);
        for bad in [
            "convert /g --to folder --zstd-level 3 -o x",
            "convert /g --to folder --zstd-pack --zstd-level 0 -o x",
            "convert /g --to folder --zstd-pack --zstd-level 23 -o x",
            "convert /g --to folder --zstd-pack --zstd-level x -o x",
            "convert /g --to pkg --zstd-pack -o x",
            "convert /g --to lz4 --zstd-pack -o x",
            "convert /g --to folder --zstd-pack --lz4-pack -o x",
            "convert /g --to folder --zstd-pack --lz4-unpack -o x",
            "convert /g --to folder --zstd-pack --lz4-level 9 -o x",
            "convert /g --to folder --zstd-pack --zstd-unpack -o x",
            "convert /g --to lz4 --zstd-unpack -o x",
            "convert /g --to folder --zstd-unpack --lz4-pack -o x",
        ] {
            assert!(convert(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn rejects_bad_lz4_combinations() {
        for bad in [
            "convert /g --to lz4 --lz4-unpack -o x",
            "convert /g --to lz4 --lz4-profile r.toml -o x",
            "convert /g --to lz4 --inner exfat -o x",
            "convert /g --to lz4 --kraken fast -o x",
            "convert /g --to lz4 --level 5 -o x",
            "convert /g --to lz4 --lz4-pack -o x",
            "convert /g --to pkg --lz4-pack -o x",
            "convert /g --to exfat --lz4-pack --lz4-unpack -o x",
        ] {
            assert!(convert(bad).is_err(), "{bad}");
        }
        let e = convert("convert /g --to lz4 --lz4-unpack -o x").unwrap_err();
        assert!(
            e.contains("redundant: the LZ4 target always unpacks first"),
            "{e}"
        );
    }
}
