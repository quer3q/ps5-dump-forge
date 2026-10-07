//! `ps5-dump-forge`: the conversion core for scripts and CI. Progress goes to stdout as one JSON
//! `Event` per line; a human summary goes to stderr.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use ps5_dump_forge_core::{ConvertRequest, Event, Format, Jobs};

const USAGE: &str = "\
usage: ps5-dump-forge inspect <path> [--json]
       ps5-dump-forge convert <source> --to folder|exfat|ffpkg|ffpfs|ffpfsc|pkg
                         [--inner exfat|ffpkg|ffpfs] [-o <output>] [--threads N]

  inspect   describe a game folder or image (.exfat, .ffpkg, .ffpfs, .ffpfsc, .pkg)
  convert   convert a source; progress is printed as JSON lines on stdout
  --to      folder, exfat, ffpkg (what ShadowMountPlus recommends), ffpfs (PFS image),
            ffpfsc (compressed PFS container, mounted read-only), or pkg = debug FPKG
            (shown as .fpkg in the app; the file still ends in .pkg)
  --inner   with --to ffpfsc: the image inside (default exfat)
  -o        output path (default: named from the title id, next to the source;
            .ffpfs/.ffpfsc names must stay within 63 bytes)
  --threads compression threads for pkg and ffpfsc (default: all cores)
  -h, --help";

#[derive(Debug)]
enum Cli {
    Inspect { path: PathBuf, json: bool },
    Convert(ConvertRequest),
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
    let mut positional = Vec::new();
    let (mut json, mut to, mut output) = (false, None, None);
    let (mut threads, mut inner) = (None, None);
    while let Some(arg) = args.next() {
        let mut value = |flag: &str| args.next().ok_or(format!("{flag} needs a value"));
        let text = |v: OsString| v.into_string().map_err(|_| "option values must be text");
        match arg.to_str().unwrap_or("") {
            "-h" | "--help" => return Ok(Cli::Help),
            "--json" => json = true,
            "--to" => to = Some(parse_format(&text(value("--to")?)?)?),
            "--inner" => inner = Some(parse_format(&text(value("--inner")?)?)?),
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
            }))
        }
        other => Err(format!("unknown command {other:?}")),
    }
}

fn parse_format(s: &str) -> Result<Format, String> {
    serde_json::from_value(serde_json::Value::String(s.to_ascii_lowercase()))
        .map_err(|_| format!("unknown format {s:?} (folder, exfat, ffpkg, ffpfs, ffpfsc, pkg)"))
}

/// Next to the source, named from its title id.
fn default_output(source: &Path, format: Format) -> Result<PathBuf, String> {
    let dir = source
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    ps5_dump_forge_core::default_output(source, format, dir).map_err(|e| format!("{e:#}"))
}

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

// ponytail: a plain signal handler that only sets a flag; the main loop polls it. Windows
// Ctrl-C ends the process without cleanup (the `.part` stays and `stale_parts` lists it).
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

#[cfg(not(unix))]
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
    }
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
    let backport = match found.backport.len() {
        0 => "no".to_string(),
        n => format!(
            "yes, firmware {} or later from its executables, {n} files (fakelib/)",
            or_none(&found.backport_firmware)
        ),
    };
    let mut text = format!(
        "{} ({})\ntitle:      {}\ntitle id:   {}\ncontent id: {}\nversion:    {}\n\
         firmware:   {} (sdk {})\nbackport:   {backport}\n\
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
                    eprintln!("ps5-dump-forge: cancelling...");
                    jobs.cancel(job);
                    cancelled = true;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break Err("job vanished".to_string()),
        }
    };
    jobs.cancel_all_and_wait();
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
        assert!(matches!(
            parse_args(args("inspect /g --json")),
            Ok(Cli::Inspect { path, json: true }) if path == Path::new("/g")
        ));
        assert!(matches!(parse_args(args("help")), Ok(Cli::Help)));
    }
}
