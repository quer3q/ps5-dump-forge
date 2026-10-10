//! `ps5-dump-forge`: the conversion core for scripts and CI. Progress goes to stdout as one JSON
//! `Event` per line; a human summary goes to stderr.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use ps5_dump_forge_core::{ConvertRequest, Event, Format, Jobs, KrakenLevel, Lz4Mode, VerifyMode};

const USAGE: &str = "\
usage: ps5-dump-forge inspect <path> [--json]
       ps5-dump-forge convert <source> --to folder|exfat|ffpkg|ffpfs|ffpfsc|pkg|lz4
                         [--inner exfat|ffpkg|ffpfs] [-o <output>] [--threads N]
                         [--remove-backport] [--full-verify] [--kraken fast|balanced|smallest]
                         [--level 0..9] [--lz4-profile <toml> | --lz4-traces <zip|dir|journal>]
                         [--lz4-trace [--lz4-trace-space <MiB>] | --lz4-unpack | --lz4-unpatch
                          | --lz4-pack]
       ps5-dump-forge convert <image.exfat|image.ffpkg> --lz4-in-place
                         (--lz4-trace [--lz4-trace-space <MiB>] | --lz4-unpatch) [--full-verify]
       ps5-dump-forge lz4-patch <folder>
       ps5-dump-forge lz4-unpatch <folder>
       ps5-dump-forge lz4-profile <source> [--lz4-traces <zip|dir|journal> | --lz4-profile <toml>]
                         [-o <out.toml>]
       ps5-dump-forge serve [--port N] --root <dir> [--root <dir>]...

  inspect   describe a game folder or image (.exfat, .ffpkg, .ffpfs, .ffpfsc, .pkg)
  convert   convert a source; progress is printed as JSON lines on stdout
  --to      folder, exfat, ffpkg (what ShadowMountPlus recommends), ffpfs (PFS image),
            ffpfsc (compressed PFS container, mounted read-only), or pkg = debug FPKG
            (shown as .fpkg in the app; the file still ends in .pkg), or lz4 = an LZ4 packed
            folder (AMPR asset packs; titles that import libSceAmpr; named like a folder)
  --inner   with --to ffpfsc: the image inside (default exfat)
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
            package ~2.6% smaller but take ~6x and ~9x longer, every core busy throughout
  --lz4-pack
            pack the assets into LZ4 packs inside the target: --to folder (the same as
            --to lz4), exfat, ffpkg, ffpfs or ffpfsc (not pkg). Into an image the packs go
            straight in, with no temporary folder; the packed files are read twice
  --lz4-profile
            with --to lz4 or --lz4-pack: a TOML pack-rules file (default: this dump's traces, else a
            built-in guess, which is less reliable)
  --lz4-traces
            with --to lz4 or --lz4-pack: traces from the console: the *-amprtrace.zip from Download traces, a
            folder holding ampr_commands.bin and ampr_emu.index, or that ampr_commands.bin
            with its ampr_emu.index beside it; picks the files to pack instead of this dump's
            own traces. Exclusive with --lz4-profile
  --lz4-trace
            record LZ4 traces: install the trace runtime, so a play session writes a
            journal; convert the dump again with --to lz4 to pack what it read. Goes with
            --to folder, exfat or ffpkg only; exclusive with --lz4-unpack
  --lz4-trace-space
            with --lz4-trace: MiB of room kept in an image for the journal, 64 to 1024
            in steps of 64 (default 256)
  --lz4-unpack
            unpack LZ4 packs to plain files first, for any target but lz4 (which always
            unpacks first)
  --lz4-unpatch
            undo --lz4-trace: install the release runtime (whatever runtime is there), leave
            out the journal and logs, write a fresh ampr_emu.index. Not for a packed source
            (unpack it first) or --to lz4 (which always installs the release runtime)
  --lz4-in-place
            with --lz4-trace or --lz4-unpatch: REPLACE the source .exfat/.ffpkg image with a
            patched copy of the same format (no --to, no -o). The copy is written beside it
            (needs about the image's size of free space) and verified; only then is it
            renamed over the source, if the source is unchanged. On any failure the source
            stays as it was
  lz4-patch change a game folder IN PLACE to record LZ4 traces: install the trace runtime
            at fakelib/libSceAmpr.sprx (replacing any runtime there), delete the last
            session's journal and logs, and write a fresh ampr_emu.index. Folders only; not a
            packed one; titles that import libSceAmpr. Play, then copy ampr_commands.bin and
            ampr_emu.index to your computer and pack the original dump with --lz4-traces
  lz4-unpatch
            undo lz4-patch IN PLACE: install the release runtime at
            fakelib/libSceAmpr.sprx (replacing any runtime there; no backup exists), delete the
            journal and logs, and write a fresh ampr_emu.index. Folders only; not a packed one
  lz4-profile
            save the LZ4 pack plan --to lz4 would use (traces, profile or built-in guess, the
            keep-loose list, auto-loose) as an editable TOML rules profile; reads the source,
            writes only the TOML (default: [Game]-[TITLE_ID]-lz4profile.toml next to the
            source; never overwrites). Load it back with --lz4-profile
  serve     the web UI on http://<this computer>:<port> (default 8095); the file browser
            starts at the --root folders (on the PS5: /data, /mnt/shadowmnt, /mnt/usb0..7,
            /mnt/ext0..1, no --root). No protections: anyone on the network can use it
  -h, --help";

#[derive(Debug)]
enum Cli {
    Inspect {
        path: PathBuf,
        json: bool,
    },
    Convert(ConvertRequest),
    Lz4Patch {
        path: PathBuf,
        unpatch: bool,
    },
    Lz4Profile {
        request: Box<ConvertRequest>,
        out: Option<PathBuf>,
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
    if command == "lz4-patch" || command == "lz4-unpatch" {
        let mut paths = Vec::new();
        for arg in args {
            match arg.to_str().unwrap_or("") {
                "-h" | "--help" => return Ok(Cli::Help),
                flag if flag.starts_with('-') && flag != "-" => {
                    return Err(format!("unknown option {flag:?}"));
                }
                _ => paths.push(PathBuf::from(arg)),
            }
        }
        let [path] = <[PathBuf; 1]>::try_from(paths)
            .map_err(|p| format!("expected one folder, got {}", p.len()))?;
        return Ok(Cli::Lz4Patch {
            path,
            unpatch: command == "lz4-unpatch",
        });
    }
    if command == "lz4-profile" {
        return parse_lz4_profile(args);
    }
    let mut positional = Vec::new();
    let (mut json, mut to, mut output) = (false, None, None);
    let (mut threads, mut inner, mut remove_backport) = (None, None, false);
    let mut full_verify = false;
    let mut kraken_level = None;
    let mut ffpfsc_level = None;
    let (mut lz4_profile, mut lz4_trace, mut lz4_unpack, mut lz4_space) =
        (None, false, false, None);
    let mut lz4_traces = None;
    let (mut lz4_unpatch, mut lz4_in_place, mut lz4_pack) = (false, false, false);
    while let Some(arg) = args.next() {
        let mut value = |flag: &str| args.next().ok_or(format!("{flag} needs a value"));
        let text = |v: OsString| v.into_string().map_err(|_| "option values must be text");
        match arg.to_str().unwrap_or("") {
            "-h" | "--help" => return Ok(Cli::Help),
            "--json" => json = true,
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
                    _ => return Err("--kraken takes fast, balanced or smallest".into()),
                })
            }
            "--lz4-profile" => lz4_profile = Some(PathBuf::from(value("--lz4-profile")?)),
            "--lz4-traces" => lz4_traces = Some(PathBuf::from(value("--lz4-traces")?)),
            "--lz4-trace" => lz4_trace = true,
            "--lz4-unpack" => lz4_unpack = true,
            "--lz4-pack" => lz4_pack = true,
            "--lz4-unpatch" => lz4_unpatch = true,
            "--lz4-in-place" => lz4_in_place = true,
            "--lz4-trace-space" => {
                lz4_space = Some(
                    text(value("--lz4-trace-space")?)?
                        .parse::<u32>()
                        .ok()
                        .filter(|g| (64..=1024).contains(g) && g % 64 == 0)
                        .ok_or("--lz4-trace-space takes 64 through 1024 (MiB), in steps of 64")?,
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
        "inspect" => Ok(Cli::Inspect { path, json }),
        "convert" => {
            if lz4_in_place {
                if to.is_some() || output.is_some() {
                    return Err(
                        "--lz4-in-place keeps the image's format and name: no --to, no -o".into(),
                    );
                }
                if !(lz4_trace || lz4_unpatch) {
                    return Err("--lz4-in-place goes with --lz4-trace or --lz4-unpatch".into());
                }
                // Core refuses anything else, read-only images included, with the reason.
                to = Some(match path.extension().and_then(|e| e.to_str()) {
                    Some(e)
                        if e.eq_ignore_ascii_case("ffpkg") || e.eq_ignore_ascii_case("ufs2") =>
                    {
                        Format::Ffpkg
                    }
                    Some(e) if e.eq_ignore_ascii_case("exfat") => Format::Exfat,
                    _ => {
                        return Err("--lz4-in-place replaces an .exfat or .ffpkg image (a game \
                             folder: lz4-patch or lz4-unpatch)"
                            .into());
                    }
                });
                output = Some(path.clone());
            }
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
            if [lz4_trace, lz4_unpack, lz4_unpatch, lz4_pack]
                .iter()
                .filter(|&&f| f)
                .count()
                > 1
            {
                return Err(
                    "--lz4-trace, --lz4-unpack, --lz4-unpatch and --lz4-pack exclude each other"
                        .into(),
                );
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
            if lz4_unpatch && format == Format::Lz4 {
                return Err(
                    "--lz4-unpatch is redundant: the LZ4 target always installs the release \
                     runtime"
                        .into(),
                );
            }
            if lz4_unpack && format == Format::Lz4 {
                return Err(
                    "--lz4-unpack is redundant: the LZ4 target always unpacks first".into(),
                );
            }
            if lz4_profile.is_some() && !packing {
                return Err("--lz4-profile only goes with --to lz4 or --lz4-pack".into());
            }
            if lz4_traces.is_some() && !packing {
                return Err("--lz4-traces only goes with --to lz4 or --lz4-pack".into());
            }
            if lz4_traces.is_some() && lz4_profile.is_some() {
                return Err("choose --lz4-profile or --lz4-traces, not both".into());
            }
            if lz4_trace && !matches!(format, Format::Folder | Format::Exfat | Format::Ffpkg) {
                return Err("--lz4-trace only goes with --to folder, exfat or ffpkg".into());
            }
            if lz4_space.is_some() && !lz4_trace {
                return Err("--lz4-trace-space only goes with --lz4-trace".into());
            }
            let output = match output {
                Some(o) => o,
                None => default_output(&path, format)?,
            };
            Ok(Cli::Convert(ConvertRequest {
                source: path,
                format,
                output,
                compression_threads: threads,
                inner,
                remove_backport,
                full_verify,
                kraken_level: kraken_level.unwrap_or_default(),
                ffpfsc_level: ffpfsc_level.unwrap_or(ps5_dump_forge_core::DEFAULT_FFPFSC_LEVEL),
                lz4: if lz4_trace {
                    Some(Lz4Mode::Trace)
                } else if lz4_unpack {
                    Some(Lz4Mode::Unpack)
                } else if lz4_unpatch {
                    Some(Lz4Mode::Unpatch)
                } else if lz4_pack {
                    Some(Lz4Mode::Pack)
                } else {
                    None
                },
                lz4_profile,
                lz4_traces,
                lz4_trace_space_mib: lz4_space
                    .unwrap_or(ps5_dump_forge_core::DEFAULT_LZ4_TRACE_SPACE_MIB),
                lz4_in_place,
            }))
        }
        other => Err(format!("unknown command {other:?}")),
    }
}

fn parse_lz4_profile(mut args: impl Iterator<Item = OsString>) -> Result<Cli, String> {
    let (mut paths, mut out, mut traces, mut profile) = (Vec::new(), None, None, None);
    while let Some(arg) = args.next() {
        let mut value = |flag: &str| args.next().ok_or(format!("{flag} needs a value"));
        match arg.to_str().unwrap_or("") {
            "-h" | "--help" => return Ok(Cli::Help),
            "-o" | "--output" => out = Some(PathBuf::from(value("-o")?)),
            "--lz4-traces" => traces = Some(PathBuf::from(value("--lz4-traces")?)),
            "--lz4-profile" => profile = Some(PathBuf::from(value("--lz4-profile")?)),
            flag if flag.starts_with('-') && flag != "-" => {
                return Err(format!("unknown option {flag:?}"));
            }
            _ => paths.push(PathBuf::from(arg)),
        }
    }
    let [source] = <[PathBuf; 1]>::try_from(paths)
        .map_err(|p| format!("expected one source, got {}", p.len()))?;
    if traces.is_some() && profile.is_some() {
        return Err("choose --lz4-profile or --lz4-traces, not both".into());
    }
    Ok(Cli::Lz4Profile {
        request: Box::new(ConvertRequest {
            source,
            format: Format::Lz4,
            output: PathBuf::new(),
            compression_threads: None,
            inner: None,
            remove_backport: false,
            full_verify: false,
            kraken_level: KrakenLevel::default(),
            ffpfsc_level: ps5_dump_forge_core::DEFAULT_FFPFSC_LEVEL,
            lz4: None,
            lz4_profile: profile,
            lz4_traces: traces,
            lz4_trace_space_mib: ps5_dump_forge_core::DEFAULT_LZ4_TRACE_SPACE_MIB,
            lz4_in_place: false,
        }),
        out,
    })
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
fn default_output(source: &Path, format: Format) -> Result<PathBuf, String> {
    ps5_dump_forge_core::default_output(source, format, Path::new("")).map_err(|e| format!("{e:#}"))
}

// scripts/release-ps5.sh looks for the server's marker in the PS5 ELF to check that it is this
// version; this keeps it linked in every build (one literal: the self copy slices its prefix).
#[used]
static VERSION_MARKER: &&[u8] = &ps5_dump_forge_server::VERSION_MARKER;

// The PS5 kernel fills FreeBSD 11 structs; libc's default FreeBSD 12 layouts read every field
// at the wrong offset (found on hardware: every path looked like neither a file nor a folder).
// ps5/build.sh selects the FreeBSD 11 ABI; these fail the build if that is ever lost.
#[cfg(target_env = "ps5")]
const _: () = {
    assert!(std::mem::size_of::<libc::stat>() == 120);
    assert!(std::mem::size_of::<libc::dirent>() == 264);
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
        Cli::Inspect { path, json } => inspect(&path, json),
        Cli::Convert(request) => convert(request),
        Cli::Lz4Patch { path, unpatch } => lz4_patch(&path, unpatch),
        Cli::Lz4Profile { request, out } => lz4_profile(&request, out),
        Cli::Serve { port, roots } => serve(port, roots),
    }
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

fn inspect(path: &Path, json: bool) -> ExitCode {
    let found = match ps5_dump_forge_core::inspect(path) {
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
    if let Some(line) = found.lz4.as_ref().map(lz4_line) {
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

/// `1.2 GB` (decimal units, one decimal above a kilobyte).
fn human_bytes(n: u64) -> String {
    let mut value = n as f64;
    for unit in ["B", "KB", "MB", "GB"] {
        if value < 1000.0 {
            return if unit == "B" {
                format!("{n} B")
            } else {
                format!("{value:.1} {unit}")
            };
        }
        value /= 1000.0;
    }
    format!("{value:.1} TB")
}

/// The one `inspect` line for a source's LZ4 facts.
fn lz4_line(f: &ps5_dump_forge_core::Lz4Facts) -> String {
    let runtime = match f.runtime.as_str() {
        "forge_release" => "Forge release",
        "forge_trace" => "Forge trace",
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
    } else if let Some(j) = f.journal_bytes {
        format!("traced, journal {}", human_bytes(j))
    } else if f.runtime == "forge_trace" {
        "traced, no journal yet".to_string()
    } else {
        format!("plain, runtime {runtime}")
    };
    let ampr = if f.imports_ampr {
        ""
    } else {
        " (eboot.bin does not import libSceAmpr)"
    };
    format!("LZ4 (AMPR): {state}{ampr}")
}

fn lz4_patch(path: &Path, unpatch: bool) -> ExitCode {
    if unpatch {
        return match ps5_dump_forge_core::lz4_unpatch(path) {
            Ok(done) => {
                eprintln!("ps5-dump-forge: {}", done.warning);
                let mut text = format!(
                    "unpatched {}: the release runtime is at fakelib/libSceAmpr.sprx, \
                     ampr_emu.index lists {} files\n",
                    path.display(),
                    done.indexed
                );
                for removed in &done.removed {
                    text += &format!("  deleted {removed}\n");
                }
                print_out(&text)
            }
            Err(e) => {
                eprintln!("ps5-dump-forge: {e:#}");
                ExitCode::FAILURE
            }
        };
    }
    match ps5_dump_forge_core::lz4_patch(path) {
        Ok(done) => {
            eprintln!("ps5-dump-forge: {}", done.warning);
            let mut text = format!(
                "patched {} for LZ4 traces: the trace runtime is at fakelib/libSceAmpr.sprx, \
                 ampr_emu.index lists {} files\n",
                path.display(),
                done.indexed
            );
            for removed in &done.removed {
                text += &format!("  deleted the last session's {removed}\n");
            }
            text += "play the game, then copy ampr_commands.bin and ampr_emu.index from this \
                     folder to your computer and pack the original dump with --to lz4 \
                     --lz4-traces <ampr_commands.bin>; each launch overwrites the previous \
                     session's trace\n";
            print_out(&text)
        }
        Err(e) => {
            eprintln!("ps5-dump-forge: {e:#}");
            ExitCode::FAILURE
        }
    }
}

/// Saves the pack plan as a TOML profile: at `out`, else next to the source; never over a file.
fn lz4_profile(request: &ConvertRequest, out: Option<PathBuf>) -> ExitCode {
    let fail = |e: anyhow::Error| {
        eprintln!("ps5-dump-forge: {e:#}");
        ExitCode::FAILURE
    };
    let saved = match ps5_dump_forge_core::lz4_plan_profile(request) {
        Ok(saved) => saved,
        Err(e) => return fail(e),
    };
    for line in &saved.log {
        eprintln!("ps5-dump-forge: {line}");
    }
    let path = match out {
        Some(p) => p,
        // The folder holding the source, as core names outputs there.
        None => match ps5_dump_forge_core::default_output(
            &request.source,
            Format::Folder,
            Path::new(""),
        ) {
            Ok(p) => p.with_file_name(&saved.file_name),
            Err(e) => return fail(e),
        },
    };
    if let Err(e) =
        ps5_dump_forge_core::write_lz4_plan_profile(&request.source, &path, &saved.toml, false)
    {
        return fail(e);
    }
    print_out(&format!(
        "saved {}: {} files packed, {} loose; edit it and load it with --lz4-profile\n",
        path.display(),
        saved.packed,
        saved.loose
    ))
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
        let Ok(Cli::Convert(r)) =
            parse_args(args("convert /g --to exfat -o /o/x.exfat --threads 2"))
        else {
            panic!("not a convert");
        };
        assert_eq!(r.format, Format::Exfat);
        assert_eq!(r.output, PathBuf::from("/o/x.exfat"));
        assert_eq!(r.compression_threads, Some(2));
        let Ok(Cli::Convert(r)) = parse_args(args("convert /g --to PKG -o /o/x.pkg")) else {
            panic!("not a convert");
        };
        assert_eq!(r.format, Format::Pkg);
        assert_eq!(r.inner, None);
        let Ok(Cli::Convert(r)) = parse_args(args("convert /g --to ffpfsc --inner ffpkg -o /o/x"))
        else {
            panic!("not a convert");
        };
        assert_eq!((r.format, r.inner), (Format::Ffpfsc, Some(Format::Ffpkg)));
        assert!(!r.remove_backport);
        assert!(!r.full_verify, "fast by default");
        let Ok(Cli::Convert(r)) = parse_args(args("convert /g --to folder --remove-backport -o x"))
        else {
            panic!("not a convert");
        };
        assert!(r.remove_backport);
        let Ok(Cli::Convert(r)) = parse_args(args("convert /g --to ffpkg --full-verify -o x"))
        else {
            panic!("not a convert");
        };
        assert!(r.full_verify);
        assert_eq!(r.kraken_level, KrakenLevel::Fast, "fast by default");
        let Ok(Cli::Convert(r)) = parse_args(args("convert /g --to pkg --kraken smallest -o x"))
        else {
            panic!("not a convert");
        };
        assert_eq!(r.kraken_level, KrakenLevel::Smallest);
        let Ok(Cli::Convert(r)) = parse_args(args("convert /g --to ffpfsc --level 0 -o x")) else {
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
            Ok(Cli::Inspect { path, json: true }) if path == Path::new("/g")
        ));
        assert!(matches!(parse_args(args("help")), Ok(Cli::Help)));
    }

    fn convert(s: &str) -> Result<ConvertRequest, String> {
        match parse_args(args(s)) {
            Ok(Cli::Convert(r)) => Ok(r),
            Ok(_) => panic!("not a convert"),
            Err(e) => Err(e),
        }
    }

    #[test]
    fn parses_lz4_workflows() {
        let r = convert("convert /g --to lz4 -o x").unwrap();
        assert_eq!((r.format, r.lz4, r.lz4_profile), (Format::Lz4, None, None));
        let r = convert("convert /g --to lz4 --lz4-profile /p/rules.toml -o x").unwrap();
        assert_eq!(r.lz4_profile, Some(PathBuf::from("/p/rules.toml")));
        for to in ["folder", "exfat", "ffpkg"] {
            let r = convert(&format!("convert /g --to {to} --lz4-trace -o x")).unwrap();
            assert_eq!(r.lz4, Some(Lz4Mode::Trace));
            assert_eq!(
                r.lz4_trace_space_mib,
                ps5_dump_forge_core::DEFAULT_LZ4_TRACE_SPACE_MIB
            );
        }
        let r = convert("convert /g --to ffpkg --lz4-trace --lz4-trace-space 1024 -o x").unwrap();
        assert_eq!(r.lz4_trace_space_mib, 1024);
        let r = convert("convert /g --to exfat --lz4-trace --lz4-trace-space 64 -o x").unwrap();
        assert_eq!(r.lz4_trace_space_mib, 64);
        for to in ["folder", "exfat", "ffpkg", "ffpfs", "ffpfsc", "pkg"] {
            let r = convert(&format!("convert /g --to {to} --lz4-unpack -o x")).unwrap();
            assert_eq!(r.lz4, Some(Lz4Mode::Unpack), "{to}");
        }
        let r = convert("convert /g --to lz4 --lz4-traces /t/ampr_commands.bin -o x").unwrap();
        assert_eq!(
            (r.lz4_traces, r.lz4_profile),
            (Some(PathBuf::from("/t/ampr_commands.bin")), None)
        );
        for to in ["folder", "exfat", "ffpkg", "ffpfs", "ffpfsc"] {
            let r = convert(&format!(
                "convert /g --to {to} --lz4-pack --lz4-traces /t/x-amprtrace.zip -o x"
            ))
            .unwrap();
            assert_eq!(r.lz4, Some(Lz4Mode::Pack), "{to}");
            assert_eq!(r.lz4_traces, Some(PathBuf::from("/t/x-amprtrace.zip")));
        }
        let r = convert("convert /g --to ffpfsc --inner ffpkg --lz4-pack --lz4-profile p.toml")
            .unwrap();
        assert_eq!((r.inner, r.lz4), (Some(Format::Ffpkg), Some(Lz4Mode::Pack)));
        assert_eq!(
            r.output.extension().and_then(|e| e.to_str()),
            Some("ffpfsc")
        );
        // Old arguments are unchanged.
        let r = convert("convert /g --to exfat -o x").unwrap();
        assert_eq!((r.lz4, r.lz4_profile, r.lz4_traces), (None, None, None));
    }

    #[test]
    fn parses_lz4_patch() {
        assert!(matches!(
            parse_args(args("lz4-patch /data/game")),
            Ok(Cli::Lz4Patch { path, unpatch: false }) if path == Path::new("/data/game")
        ));
        assert!(matches!(
            parse_args(args("lz4-unpatch /data/game")),
            Ok(Cli::Lz4Patch { path, unpatch: true }) if path == Path::new("/data/game")
        ));
        assert!(matches!(
            parse_args(args("lz4-patch --help")),
            Ok(Cli::Help)
        ));
        for bad in [
            "lz4-patch",
            "lz4-patch /a /b",
            "lz4-patch /a --to lz4",
            "lz4-unpatch",
            "lz4-unpatch /a --lz4-in-place",
        ] {
            assert!(parse_args(args(bad)).is_err(), "{bad}");
        }
        assert!(USAGE.contains("lz4-patch <folder>"));
        assert!(USAGE.contains("lz4-unpatch <folder>"));
    }

    #[test]
    fn parses_lz4_profile() {
        match parse_args(args("lz4-profile /d/game --lz4-traces /t.zip -o /x.toml")) {
            Ok(Cli::Lz4Profile { request, out }) => {
                assert_eq!(request.source, Path::new("/d/game"));
                assert_eq!(request.format, Format::Lz4);
                assert_eq!(request.lz4_traces.as_deref(), Some(Path::new("/t.zip")));
                assert_eq!(request.lz4_profile, None);
                assert_eq!(out.as_deref(), Some(Path::new("/x.toml")));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            parse_args(args("lz4-profile /d/game --lz4-profile /p.toml")),
            Ok(Cli::Lz4Profile { request, out: None }) if request.lz4_profile.is_some()
        ));
        for bad in [
            "lz4-profile",
            "lz4-profile /a /b",
            "lz4-profile /a --to lz4",
            "lz4-profile /a --lz4-traces /t --lz4-profile /p",
        ] {
            assert!(parse_args(args(bad)).is_err(), "{bad}");
        }
        assert!(USAGE.contains("lz4-profile <source>"));
    }

    #[test]
    fn parses_lz4_in_place() {
        let r = convert("convert /d/game.ffpkg --lz4-trace --lz4-in-place --lz4-trace-space 128")
            .unwrap();
        assert_eq!(
            (r.format, r.lz4, r.lz4_in_place, r.lz4_trace_space_mib),
            (Format::Ffpkg, Some(Lz4Mode::Trace), true, 128)
        );
        assert_eq!(r.output, Path::new("/d/game.ffpkg"));
        let r = convert("convert /d/game.EXFAT --lz4-unpatch --lz4-in-place").unwrap();
        assert_eq!(
            (r.format, r.lz4, r.lz4_in_place),
            (Format::Exfat, Some(Lz4Mode::Unpatch), true)
        );
        let r = convert("convert /g --to folder --lz4-unpatch -o x").unwrap();
        assert_eq!((r.lz4, r.lz4_in_place), (Some(Lz4Mode::Unpatch), false));
        for bad in [
            "convert /d/game.exfat --lz4-in-place",
            "convert /d/game.exfat --lz4-unpack --lz4-in-place",
            "convert /d/game.exfat --lz4-trace --lz4-unpatch --lz4-in-place",
            "convert /d/game.exfat --lz4-trace --lz4-in-place --to exfat",
            "convert /d/game.exfat --lz4-trace --lz4-in-place -o y.exfat",
            "convert /d/game.ffpfs --lz4-trace --lz4-in-place",
            "convert /d/game --lz4-unpatch --lz4-in-place",
            "convert /g --to lz4 --lz4-unpatch -o x",
            "convert /g --to folder --lz4-unpack --lz4-unpatch -o x",
        ] {
            assert!(convert(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn rejects_bad_lz4_combinations() {
        for bad in [
            "convert /g --to lz4 --lz4-unpack -o x",
            "convert /g --to folder --lz4-trace --lz4-unpack -o x",
            "convert /g --to folder --lz4-profile r.toml -o x",
            "convert /g --to lz4 --lz4-trace -o x",
            "convert /g --to ffpfs --lz4-trace -o x",
            "convert /g --to ffpfsc --lz4-trace -o x",
            "convert /g --to pkg --lz4-trace -o x",
            "convert /g --to folder --lz4-trace-space 8 -o x",
            "convert /g --to lz4 --lz4-trace-space 8 -o x",
            "convert /g --to folder --lz4-trace --lz4-trace-space 0 -o x",
            "convert /g --to folder --lz4-trace --lz4-trace-space 100 -o x",
            "convert /g --to folder --lz4-trace --lz4-trace-space 1088 -o x",
            "convert /g --to folder --lz4-trace --lz4-trace-space 5000 -o x",
            "convert /g --to folder --lz4-trace --lz4-trace-space x -o x",
            "convert /g --to folder --lz4-trace --lz4-trace-space -o",
            "convert /g --to lz4 --lz4-profile -o x",
            "convert /g --to folder --lz4-traces t.bin -o x",
            "convert /g --to exfat --lz4-traces t.bin -o x",
            "convert /g --to lz4 --lz4-profile r.toml --lz4-traces t.bin -o x",
            "convert /g --to lz4 --lz4-traces",
            "convert /g --to lz4 --inner exfat -o x",
            "convert /g --to lz4 --kraken fast -o x",
            "convert /g --to lz4 --level 5 -o x",
            "convert /g --to lz4 --lz4-pack -o x",
            "convert /g --to pkg --lz4-pack -o x",
            "convert /g --to exfat --lz4-pack --lz4-trace -o x",
            "convert /g --to exfat --lz4-pack --lz4-unpack -o x",
            "convert /g --lz4-trace",
        ] {
            assert!(convert(bad).is_err(), "{bad}");
        }
        let e = convert("convert /g --to lz4 --lz4-unpack -o x").unwrap_err();
        assert!(
            e.contains("redundant: the LZ4 target always unpacks first"),
            "{e}"
        );
    }

    #[test]
    fn lz4_default_name_has_no_extension() {
        let root = std::env::temp_dir().join(format!("forge-cli-lz4-name-{}", std::process::id()));
        let game = root.join("game");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(game.join("sce_sys")).unwrap();
        std::fs::write(game.join("eboot.bin"), b"x").unwrap();
        std::fs::write(
            game.join("sce_sys/param.json"),
            r#"{"titleId":"PPSA01234"}"#,
        )
        .unwrap();
        let r = convert(&format!("convert {} --to lz4", game.display())).unwrap();
        assert_eq!(r.output, root.join("PPSA01234"));
        assert_ne!(r.output.extension().and_then(|e| e.to_str()), Some("lz4"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn lz4_inspect_line() {
        use ps5_dump_forge_core::{Lz4Facts, Lz4Packed};
        let facts = |packed, journal_bytes, runtime: &str| Lz4Facts {
            imports_ampr: true,
            packed,
            manifest_error: None,
            runtime: runtime.into(),
            journal_bytes,
            traces_zip: None,
        };
        let packed = Lz4Packed {
            files: 12431,
            packed_files: Some(12000),
            volumes: 3,
            stored_percent: Some(52),
        };
        assert_eq!(
            lz4_line(&facts(Some(packed), None, "forge_release")),
            "LZ4 (AMPR): packed, 12,431 files in 3 volumes, 52% of size"
        );
        assert_eq!(
            lz4_line(&facts(None, Some(1_200_000_000), "forge_trace")),
            "LZ4 (AMPR): traced, journal 1.2 GB"
        );
        assert_eq!(
            lz4_line(&facts(None, None, "forge_release")),
            "LZ4 (AMPR): plain, runtime Forge release"
        );
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1000), "1,000");
    }

    #[cfg(not(target_env = "ps5"))]
    #[test]
    fn parses_serve() {
        assert!(matches!(
            parse_args(args("serve --port 9000 --root /a --root /b")),
            Ok(Cli::Serve { port: 9000, roots }) if roots == [Path::new("/a"), Path::new("/b")]
        ));
        assert!(matches!(
            parse_args(args("serve --root /a")),
            Ok(Cli::Serve { port: 8095, .. })
        ));
        assert!(parse_args(args("serve")).is_err());
        assert!(parse_args(args("serve --root")).is_err());
        assert!(parse_args(args("serve --port x --root /a")).is_err());
        assert!(parse_args(args("serve /a")).is_err());
    }
}
