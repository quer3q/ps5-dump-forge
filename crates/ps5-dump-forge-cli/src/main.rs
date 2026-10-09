//! `ps5-dump-forge`: the conversion core for scripts and CI. Progress goes to stdout as one JSON
//! `Event` per line; a human summary goes to stderr.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use ps5_dump_forge_core::{ConvertRequest, Event, Format, Jobs, KrakenLevel, VerifyMode};

const USAGE: &str = "\
usage: ps5-dump-forge inspect <path> [--json]
       ps5-dump-forge convert <source> --to folder|exfat|ffpkg|ffpfs|ffpfsc|pkg
                         [--inner exfat|ffpkg|ffpfs] [-o <output>] [--threads N]
                         [--remove-backport] [--full-verify] [--kraken fast|balanced|smallest]
                         [--level 0..9]
       ps5-dump-forge serve [--port N] --root <dir> [--root <dir>]...

  inspect   describe a game folder or image (.exfat, .ffpkg, .ffpfs, .ffpfsc, .pkg)
  convert   convert a source; progress is printed as JSON lines on stdout
  --to      folder, exfat, ffpkg (what ShadowMountPlus recommends), ffpfs (PFS image),
            ffpfsc (compressed PFS container, mounted read-only), or pkg = debug FPKG
            (shown as .fpkg in the app; the file still ends in .pkg)
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
  serve     the web UI on http://<this computer>:<port> (default 8095); the file browser
            starts at the --root folders (on the PS5: /data, /mnt/usb0..7, /mnt/ext0..1,
            no --root). No protections: anyone on the network can use it
  -h, --help";

#[derive(Debug)]
enum Cli {
    Inspect { path: PathBuf, json: bool },
    Convert(ConvertRequest),
    Serve { port: u16, roots: Vec<PathBuf> },
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
            }))
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
    serde_json::from_value(serde_json::Value::String(s.to_ascii_lowercase()))
        .map_err(|_| format!("unknown format {s:?} (folder, exfat, ffpkg, ffpfs, ffpfsc, pkg)"))
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
