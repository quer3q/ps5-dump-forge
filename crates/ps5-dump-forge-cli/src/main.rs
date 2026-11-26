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
    ArchivePassword, ConvertRequest, Edit, Event, Format, Jobs, KrakenLevel, Lz4Mode, Lz4Runtime,
    MetadataRecovery, VerifyMode,
};

const USAGE: &str = "\
usage: ps5-dump-forge inspect <path> [--json] [--password-stdin]
       ps5-dump-forge convert <source> --to folder|exfat|ffpkg|ffpfs|ffpfsc|pkg|lz4
                         [--password-stdin]
                         [--inner exfat|ffpkg|ffpfs] [-o <output>] [--threads N]
                         [--remove-backport] [--full-verify] [--delete-source] [--restore-metadata]
                         [--kraken fast|balanced|smallest|stored]
                         [--level 0..9] [--lz4-unpack | --lz4-pack]
                         [--lz4-level 0|3..12] [--keep-ampr-emu]
       ps5-dump-forge convert <source> --to folder|exfat|ffpkg|ffpfs|ffpfsc --zstd-pack
                         [--zstd-level 1..22]
                         (with --to lz4, --lz4-pack or --zstd-pack: [--pack-profile <toml>])
       ps5-dump-forge convert <zstd_packed> --to <format> --zstd-unpack
       ps5-dump-forge edit <image> [--add <from>:<to>]... [--delete <path>]...
                         [--rename <from>:<to>]... [--mkdir <path>]... [--rebuild] [--check]
                         [--restore-metadata [--metadata-url <url>] [--original-runtime]]
       ps5-dump-forge serve [--port N] --root <dir> [--root <dir>]...
       ps5-dump-forge update [--install]

  inspect   describe a game folder, image (.exfat, .ffpkg, .ffpfs, .ffpfsc, .pkg) or archive
            holding one game folder (.zip, .7z, .rar; the first part of a multi-part set)
  convert   convert a source; progress is printed as JSON lines on stdout. An archive is
            read in place, with nothing unpacked to disk
  edit      change a game folder's or image's files without mounting it: --add copies a file or
            folder from this computer to a game path, --delete removes one, --rename moves one,
            --mkdir makes an empty one; --rebuild writes a new image beside the source instead
            of editing in place; --check only reports whether either fits (JSON on stdout)
  --password-stdin
            an archive's password: one line read from stdin (default: the
            FORGE_ARCHIVE_PASSWORD environment variable; never an argument)
  --to      folder, exfat, ffpkg (what ShadowMountPlus recommends), ffpfs (PFS image),
            ffpfsc (compressed PFS container, mounted read-only), or pkg = debug FPKG
            (shown as .fpkg in the app; the file still ends in .pkg), or lz4 = an LZ4 packed
            folder (AMPR asset packs and ampr-emu-ng, for any title; named like a folder)
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
  --delete-source
            once the output is verified and in place, delete the source (every part of a
            multi-part archive) as the app's Delete does; if it can't, the source stays and
            the job still succeeds (a source kept: <reason> check line)
  --restore-metadata
            with convert: the output gets Sony's metadata for this version where the game's
            differs (needs the internet); the source is not changed. When that fails, a warning
            and the conversion goes on as it is
  --level   with --to ffpfsc: zlib level, 0 (store) to 9 (smallest), default 6
  --kraken  with --to pkg: compression level (default fast). balanced and smallest make a
            package ~2.6% smaller but take ~6x and ~9x longer, every core busy throughout;
            stored compresses nothing (a far larger package), for a console that refuses
            to start a compressed one
  --lz4-pack
            pack the assets into LZ4 packs inside the target: --to folder (the same as
            --to lz4), exfat, ffpkg, ffpfs or ffpfsc (not pkg). Into an image the packs go
            straight in, with no temporary folder; the packed files are read twice. The
            runtime is ampr-emu-ng (packs everything except system files): turn off
            ShadowMount+'s fakelib update, or it replaces the runtime
  --zstd-pack
            pack the assets into zstd packs (ampr_zstd.index + ampr_zstd-NNN.pak, zstd in 256 KiB
            chunks) and install the zstd runtime at fakelib/libSceAmpr.sprx: --to folder,
            exfat, ffpkg, ffpfs or ffpfsc (not pkg; into an image straight in, the packed
            files read twice), any title (ampr-emu-ng injected); no LZ4 option. The runtime is
            ampr-emu-ng: turn off ShadowMount+'s fakelib update, or it replaces the runtime
  --zstd-level
            with --zstd-pack: zstd level, 1 (fastest) to 22 (smallest), default 5
  --zstd-unpack
            decode a zstd-packed game's packs into plain files, for any target but lz4;
            ampr_emu.index is written anew (into a pkg the builder writes it)
  --lz4-level
            with --to lz4 or --lz4-pack: 0 fast (default), 3..12 LZ4 HC (9 medium, 12 max)
  --keep-ampr-emu
            keep an ampr_emu build at fakelib/libSceAmpr.sprx as it is (by default the bundled
            ampr-emu-ng replaces it). Every conversion, pack and unpack installs ampr-emu-ng
            where an AMPR title has none and replaces an older one or Forge's trace build;
            a pkg carries no AMPR runtime either way (the one there is left out)
  --pack-profile
            with --to lz4, --lz4-pack or --zstd-pack: a TOML pack profile (ampr_pack.py's
            format, as Lazy_AMPR's): its [[rule]]s pick the packed files instead of the
            built-in packing strategy (the last matching rule wins; keys Forge doesn't use are logged)
  --lz4-unpack
            unpack LZ4 packs to plain files first, for any target but lz4 (which always
            unpacks first), pkg included
  --add     with edit: a file or folder on this computer, then a game path ('/'-separated):
            <from>:<to>; splits at the first ':' (a Windows drive, C:\\x, stays with the
            from side)
  --delete  with edit: a game path, and everything in it
  --rename  with edit: a game path, then the new game path: <from>:<to>
  --mkdir   with edit: a new empty game folder (parents created too)
  --rebuild with edit on an image: write a new image from the source plus the edits beside it,
            then rename it over the source, instead of editing the image in place
  --check   with edit: report whether the edits fit, in place or rebuilt (EditCheck JSON on
            stdout); exits 1 when the mode the command would run doesn't. Nothing is edited,
            but an interrupted edit's undo journal beside the image is settled first
  --restore-metadata
            with edit: also restore the game's sce_sys metadata (param.json, icon and pictures,
            snd0.at9, PlayGo tables) from Sony's update server, as one edit with any other
            change given (alone too): only the same version's metadata piece is downloaded
            (needs the internet); an image that can't take it in place (.ffpfs, .ffpfsc, .pkg)
            is rebuilt; a failed download changes nothing. Protected and sealed records (npbind,
            trophy and UDS containers) are left alone. With convert: the output gets Sony's
            files instead, the source is not changed, and a failure is a warning
  --metadata-url
            with edit --restore-metadata: the version XML, manifest or _sc.pkg URL (default:
            param.json's versionFileUri, which may offer only a newer version)
  --original-runtime
            with edit --restore-metadata: take Sony's sdkVersion, requiredSystemSoftwareVersion
            and applicationDrmType too (by default the game's own stay: a backport lowers them)
  serve     the web UI on http://<this computer>:<port> (default 8095); the file browser
            starts at the --root folders (on the PS5: /data and /mnt/usb0..7, /mnt/ext0..1
            while mounted, no --root; nothing else is listed). No protections: anyone on the
            network can use it
  update    ask GitHub whether a newer release has this computer's CLI and print its link
            (the one command that goes online); --install downloads it, checks it against the
            release's SHA256SUMS and puts it in place of this one
  -h, --help";

// One value per run (over clippy's limit on Windows, whose PathBuf is larger).
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
    Edit {
        request: ConvertRequest,
        check: bool,
    },
    Serve {
        port: u16,
        roots: Vec<PathBuf>,
    },
    Update {
        install: bool,
    },
    Help,
}

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
    if command == "edit" {
        return parse_edit(args);
    }
    if command == "update" {
        let mut install = false;
        for arg in args {
            match arg.to_str() {
                Some("--install") => install = true,
                Some("-h" | "--help") => return Ok(Cli::Help),
                _ => return Err(format!("unknown option {arg:?}")),
            }
        }
        return Ok(Cli::Update { install });
    }
    let mut positional = Vec::new();
    let (mut json, mut to, mut output) = (false, None, None);
    let (mut threads, mut inner, mut remove_backport) = (None, None, false);
    let mut full_verify = false;
    let mut delete_source = false;
    let mut restore_metadata = false;
    let mut kraken_level = None;
    let mut ffpfsc_level = None;
    let (mut lz4_unpack, mut lz4_pack) = (false, false);
    let (mut lz4_level, mut keep_ampr_emu) = (None, false);
    let (mut zstd_pack, mut zstd_level, mut zstd_unpack) = (false, None, false);
    let mut pack_profile = None;
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
            "--delete-source" => delete_source = true,
            "--restore-metadata" => restore_metadata = true,
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
            "--keep-ampr-emu" => keep_ampr_emu = true,
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
            "--pack-profile" => pack_profile = Some(PathBuf::from(value("--pack-profile")?)),
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
            if pack_profile.is_some() && !(packing || zstd_pack) {
                return Err(
                    "--pack-profile only goes with --to lz4, --lz4-pack or --zstd-pack".into(),
                );
            }
            if zstd_level.is_some() && !zstd_pack {
                return Err("--zstd-level only goes with --zstd-pack".into());
            }
            if zstd_pack && matches!(format, Format::Lz4 | Format::Pkg) {
                return Err(
                    "--zstd-pack goes with --to folder, exfat, ffpkg, ffpfs or ffpfsc".into(),
                );
            }
            let any_lz4 = lz4_unpack || lz4_pack || lz4_level.is_some();
            if zstd_pack && any_lz4 {
                return Err("--zstd-pack takes no LZ4 option".into());
            }
            if zstd_unpack && zstd_pack {
                return Err("--zstd-unpack and --zstd-pack exclude each other".into());
            }
            if zstd_unpack && (any_lz4 || format == Format::Lz4) {
                return Err("--zstd-unpack takes no LZ4 option and not --to lz4".into());
            }
            // No -o: named in `main` once the archive password is known.
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
                    lz4_runtime: Lz4Runtime::Ng,
                    replace_ampr_emu: !keep_ampr_emu,
                    zstd_pack,
                    zstd_level: zstd_level.unwrap_or(ps5_dump_forge_core::DEFAULT_ZSTD_LEVEL),
                    zstd_unpack,
                    edits: Vec::new(),
                    extract: Vec::new(),
                    transfer: Vec::new(),
                    replace: false,
                    edit_rebuild: false,
                    install: None,
                    recover_metadata: None,
                    restore_metadata,
                    delete_source,
                    pack_profile,
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
    if cfg!(target_env = "ps5") {
        if !roots.is_empty() {
            return Err("--root is not used on the PS5".into());
        }
    } else if roots.is_empty() {
        return Err("serve needs at least one --root".into());
    }
    Ok(Cli::Serve { port, roots })
}

fn parse_edit(mut args: impl Iterator<Item = OsString>) -> Result<Cli, String> {
    let (mut rebuild, mut check, mut edits, mut positional) =
        (false, false, Vec::new(), Vec::new());
    let (mut restore, mut url, mut original_runtime) = (false, None, false);
    while let Some(arg) = args.next() {
        let mut value = |flag: &str| args.next().ok_or(format!("{flag} needs a value"));
        let text = |v: OsString| v.into_string().map_err(|_| "option values must be text");
        match arg.to_str().unwrap_or("") {
            "-h" | "--help" => return Ok(Cli::Help),
            // Events are JSON lines already.
            "--json" => {}
            "--rebuild" => rebuild = true,
            "--check" => check = true,
            "--restore-metadata" => restore = true,
            "--metadata-url" => url = Some(text(value("--metadata-url")?)?),
            "--original-runtime" => original_runtime = true,
            "--add" => {
                let pair = text(value("--add")?)?;
                let (from, to) =
                    split_from_to(&pair).ok_or("--add needs from:to, with neither side empty")?;
                edits.push(Edit::Add {
                    from: PathBuf::from(from),
                    to: to.to_string(),
                });
            }
            "--delete" => {
                let path = text(value("--delete")?)?;
                if path.is_empty() {
                    return Err("--delete needs a path".into());
                }
                edits.push(Edit::Delete { path });
            }
            "--rename" => {
                let pair = text(value("--rename")?)?;
                let (from, to) = split_from_to(&pair)
                    .ok_or("--rename needs from:to, with neither side empty")?;
                edits.push(Edit::Rename {
                    from: from.to_string(),
                    to: to.to_string(),
                });
            }
            "--mkdir" => {
                let path = text(value("--mkdir")?)?;
                if path.is_empty() {
                    return Err("--mkdir needs a path".into());
                }
                edits.push(Edit::Mkdir { path });
            }
            flag if flag.starts_with('-') && flag != "-" => {
                return Err(format!("unknown option {flag:?}"));
            }
            _ => positional.push(PathBuf::from(arg)),
        }
    }
    let [image] = <[PathBuf; 1]>::try_from(positional)
        .map_err(|p| format!("expected one image, got {}", p.len()))?;
    if edits.is_empty() && !restore {
        return Err(
            "edit needs at least one of --add, --delete, --rename, --mkdir or --restore-metadata"
                .into(),
        );
    }
    if (url.is_some() || original_runtime) && !restore {
        return Err("--metadata-url and --original-runtime go with --restore-metadata".into());
    }
    if check && restore {
        return Err("--check doesn't download: run it without --restore-metadata".into());
    }
    let format = format_of(&image);
    Ok(Cli::Edit {
        request: ConvertRequest {
            edits,
            edit_rebuild: rebuild,
            recover_metadata: restore.then_some(MetadataRecovery {
                url,
                original_runtime,
            }),
            ..base_request(image, format)
        },
        check,
    })
}

fn base_request(source: PathBuf, format: Format) -> ConvertRequest {
    ConvertRequest {
        source,
        format,
        output: PathBuf::new(),
        compression_threads: None,
        inner: None,
        remove_backport: false,
        full_verify: false,
        kraken_level: KrakenLevel::default(),
        ffpfsc_level: ps5_dump_forge_core::DEFAULT_FFPFSC_LEVEL,
        lz4: None,
        lz4_level: 0,
        lz4_runtime: Lz4Runtime::default(),
        replace_ampr_emu: true,
        zstd_pack: false,
        zstd_level: ps5_dump_forge_core::DEFAULT_ZSTD_LEVEL,
        zstd_unpack: false,
        pack_profile: None,
        edits: Vec::new(),
        edit_rebuild: false,
        extract: Vec::new(),
        transfer: Vec::new(),
        replace: false,
        install: None,
        recover_metadata: None,
        restore_metadata: false,
        delete_source: false,
        archive_password: None,
    }
}

/// Split `from:to` at the first `:`, keeping a Windows drive prefix (`C:\x:/y`) with `from`.
fn split_from_to(s: &str) -> Option<(&str, &str)> {
    let bytes = s.as_bytes();
    let start = if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
    {
        2
    } else {
        0
    };
    let at = start + s[start..].find(':')?;
    let (from, to) = (&s[..at], &s[at + 1..]);
    (!from.is_empty() && !to.is_empty()).then_some((from, to))
}

/// Core ignores it for an image edit; the request only needs one.
fn format_of(path: &Path) -> Format {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("exfat") => Format::Exfat,
        Some("ffpkg") => Format::Ffpkg,
        Some("ffpfs") => Format::Ffpfs,
        Some("ffpfsc") => Format::Ffpfsc,
        Some("pkg") => Format::Pkg,
        _ => Format::Folder,
    }
}

fn parse_format(s: &str) -> Result<Format, String> {
    serde_json::from_value(serde_json::Value::String(s.to_ascii_lowercase())).map_err(|_| {
        format!("unknown format {s:?} (folder, exfat, ffpkg, ffpfs, ffpfsc, pkg, lz4)")
    })
}

fn default_output(
    source: &Path,
    format: Format,
    password: Option<&str>,
) -> Result<PathBuf, String> {
    ps5_dump_forge_core::default_output_with(source, format, Path::new(""), password)
        .map_err(|e| format!("{e:#}"))
}

// Keeps the server's version marker linked in: scripts/release-ps5.sh checks it in the PS5 ELF.
#[used]
static VERSION_MARKER: &&[u8] = &ps5_dump_forge_server::VERSION_MARKER;

// ponytail: measured on the PS5 only (its libc serializes every malloc); musl's gain is assumed.
#[cfg(any(target_env = "ps5", target_env = "musl"))]
#[global_allocator]
static ALLOC: ps5_dump_forge_ps5::Pool = ps5_dump_forge_ps5::Pool;

// The PS5 kernel fills FreeBSD 11 structs (found on hardware: FreeBSD 12 layouts made every path
// neither a file nor a folder); these fail the build if ps5/build.sh's FreeBSD 11 ABI is lost.
#[cfg(target_env = "ps5")]
const _: () = {
    assert!(std::mem::size_of::<libc::stat>() == 120);
    assert!(std::mem::size_of::<libc::dirent>() == 264);
    // MNAMELEN 88 (FreeBSD 12+: 1024): free space, mount points, filesystem types.
    assert!(std::mem::size_of::<libc::statfs>() == 472);
};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
/// Windows' close handler waits for it.
static CLEANED_UP: AtomicBool = AtomicBool::new(false);
/// The first Ctrl-C (`INTERRUPTED` is also set by a dead stdout): a second one quits at once.
static SIGNALLED: AtomicBool = AtomicBool::new(false);

// ponytail: a plain signal handler that only sets a flag; the main loop polls it.
#[cfg(unix)]
fn on_ctrl_c() {
    extern "C" fn handler(_: libc::c_int) {
        if SIGNALLED.swap(true, Ordering::Relaxed) {
            // SAFETY: `_exit` is async-signal-safe; the job's `.part` stays (`stale_parts`).
            unsafe { libc::_exit(130) };
        }
        INTERRUPTED.store(true, Ordering::Relaxed);
    }
    // SAFETY: the handler only touches atomics and `_exit`, both async-signal-safe.
    unsafe {
        libc::signal(libc::SIGINT, handler as *const () as libc::sighandler_t);
        libc::signal(libc::SIGTERM, handler as *const () as libc::sighandler_t);
    }
}

/// Closing the console, logging off or shutting down ends the process once the handler returns
/// (or ~5 s later), so it waits up to 4 s for the job's cleanup.
// ponytail: a slower cleanup is cut off; the `.part` survives and `stale_parts` lists it.
#[cfg(windows)]
fn on_ctrl_c() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetConsoleCtrlHandler(handler: extern "system" fn(u32) -> i32, add: i32) -> i32;
    }
    extern "system" fn handler(event: u32) -> i32 {
        const CTRL_BREAK_EVENT: u32 = 1;
        if event <= CTRL_BREAK_EVENT && SIGNALLED.swap(true, Ordering::Relaxed) {
            // A second Ctrl-C: not handled, so the default handler ends the process.
            return 0;
        }
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
    // What an earlier `update --install` moved aside: Windows can't remove a running exe.
    #[cfg(windows)]
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(old_exe(&exe));
    }
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
                run_request(request)
            }
            Err(e) => usage_error(&e),
        },
        Cli::Edit { request, check } => {
            if check {
                check_edits(&request)
            } else {
                run_request(request)
            }
        }
        Cli::Serve { port, roots } => serve(port, roots),
        Cli::Update { install } => update(install),
    }
}

fn update(install: bool) -> ExitCode {
    use ps5_dump_forge_updater as up;
    let current = env!("CARGO_PKG_VERSION");
    let Some(asset) = up::cli_asset() else {
        eprintln!("ps5-dump-forge: no release has a CLI for this platform; update it by hand");
        return ExitCode::FAILURE;
    };
    let latest = match up::latest(asset) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("ps5-dump-forge: {e}");
            return ExitCode::FAILURE;
        }
    };
    let new = latest.version;
    if !up::is_newer(&new, current) {
        return print_out(&format!("ps5-dump-forge {current} is up to date\n"));
    }
    if !latest.listed {
        eprintln!("ps5-dump-forge: no ps5-dump-forge-cli asset for this platform in v{new}");
        return ExitCode::FAILURE;
    }
    if !install {
        let url = up::asset_url(&new, asset);
        return print_out(&format!("ps5-dump-forge {new} is available: {url}\n"));
    }
    match up::download_cli(&new, asset).and_then(|bytes| replace_exe(&bytes)) {
        Ok(()) => print_out(&format!("updated to {new}\n")),
        Err(e) => {
            eprintln!("ps5-dump-forge: couldn't update to {new}: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(windows)]
fn old_exe(exe: &Path) -> PathBuf {
    let mut old = exe.as_os_str().to_owned();
    old.push(".old");
    PathBuf::from(old)
}

fn replace_exe(bytes: &[u8]) -> std::io::Result<()> {
    // unix: resolve a link (e.g. one in /usr/local/bin) so the file it names is replaced; Windows'
    // current_exe is already absolute and canonicalize fails on some volumes (RAM disks, SUBST).
    let exe = std::env::current_exe()?;
    #[cfg(unix)]
    let exe = std::fs::canonicalize(exe)?;
    let dir = exe.parent().unwrap_or(Path::new("."));
    let name = exe.file_name().unwrap_or_default().to_string_lossy();
    let tmp = dir.join(format!(".{name}.update-{}.tmp", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o755);
    let written = options.open(&tmp).and_then(|mut f| {
        f.write_all(bytes)?;
        // The create mode went through the umask; the CLI must stay runnable by everyone.
        #[cfg(unix)]
        f.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o755))?;
        f.sync_all()
    });
    if let Err(e) = written.and_then(|()| put_in_place(&tmp, &exe)) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    // In place already: a folder that can't be synced (FUSE, network) isn't a failure.
    #[cfg(unix)]
    let _ = std::fs::File::open(dir).and_then(|d| d.sync_all());
    Ok(())
}

#[cfg(not(windows))]
fn put_in_place(tmp: &Path, exe: &Path) -> std::io::Result<()> {
    std::fs::rename(tmp, exe)
}

#[cfg(windows)]
fn put_in_place(tmp: &Path, exe: &Path) -> std::io::Result<()> {
    let old = old_exe(exe);
    let _ = std::fs::remove_file(&old);
    std::fs::rename(exe, &old)?;
    // shortcut: two renames, not one atomic swap; a crash between them leaves only `<exe>.old`,
    // which the error below names (a separate updater process would close that window).
    std::fs::rename(tmp, exe).map_err(|e| match std::fs::rename(&old, exe) {
        Ok(()) => e,
        Err(back) => std::io::Error::other(format!(
            "{e}; putting the old CLI back failed too ({back}): rename {} to {} by hand",
            old.display(),
            exe.display()
        )),
    })
}

fn usage_error(e: &str) -> ExitCode {
    eprintln!("ps5-dump-forge: {e}");
    ExitCode::from(2)
}

/// Never an argument (argv is visible to other users). Empty = none.
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
    if let Some(line) = found.lz4.as_ref().map(lz4_line) {
        text += &format!("{line}\n");
    }
    if !found.unsigned.is_empty() {
        let n = found.unsigned.len();
        let more = if n > 5 { ", ..." } else { "" };
        text += &format!(
            "unsigned:   {n} executables (a conversion signs them): {}{more}\n",
            found.unsigned[..n.min(5)].join(", ")
        );
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

fn lz4_line(f: &ps5_dump_forge_core::Lz4Facts) -> String {
    let named = |name: &str| match &f.runtime_version {
        Some(v) => format!("{name} {v}"),
        None => name.to_owned(),
    };
    let runtime = match f.runtime.as_str() {
        "forge_release" | "other" => named("ampr_emu"),
        // Forge's old trace runtime: still told apart, so a conversion replaces it.
        "forge_trace" => "Forge trace".to_owned(),
        "zstd" => named("ampr-emu-ng"),
        _ => "none".to_owned(),
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
    // `runtime_action` assumes `--keep-ampr-emu` is off (core's `inspect`).
    let mut action = match (f.runtime_action, f.runtime.as_str()) {
        (Some("install"), _) => ", a conversion installs ampr-emu-ng".to_owned(),
        (Some("replace"), _) if f.keep_runtime_action == Some("keep") => {
            format!(", a conversion replaces {runtime} with ampr-emu-ng (--keep-ampr-emu keeps it)")
        }
        (Some("replace"), _) => format!(", a conversion replaces {runtime} with ampr-emu-ng"),
        (Some("keep"), _) => format!(", {runtime} kept"),
        _ => String::new(),
    };
    if f.pkg_runtime_action == Some("omit") {
        action += ", a pkg carries none";
    }
    format!("LZ4 (AMPR): {state}{action}{index}{ampr}")
}

/// Without the panic `print!` has on a closed pipe.
fn print_out(text: &str) -> ExitCode {
    match std::io::stdout().lock().write_all(text.as_bytes()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

/// Fails when the mode the same command would run has findings.
fn check_edits(request: &ConvertRequest) -> ExitCode {
    let check = Jobs::new(|_| {}).edit_check(request);
    let text = match serde_json::to_string_pretty(&check) {
        Ok(text) => text + "\n",
        Err(e) => {
            eprintln!("ps5-dump-forge: {e}");
            return ExitCode::FAILURE;
        }
    };
    let findings = if request.edit_rebuild && !request.source.is_dir() {
        &check.rebuild
    } else {
        &check.in_place
    };
    let printed = print_out(&text);
    if findings.is_empty() {
        return printed;
    }
    for f in findings {
        eprintln!("ps5-dump-forge: {f}");
    }
    ExitCode::FAILURE
}

fn run_request(request: ConvertRequest) -> ExitCode {
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
    if request.output.as_os_str().is_empty() {
        eprintln!("ps5-dump-forge: {}", request.source.display());
    } else {
        eprintln!(
            "ps5-dump-forge: {} -> {}",
            request.source.display(),
            request.output.display()
        );
    }
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
                    let _ = writeln!(
                        std::io::stderr(),
                        "ps5-dump-forge: cancelling... (press Ctrl-C again to quit now)"
                    );
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

    fn edit(s: &str) -> Result<(ConvertRequest, bool), String> {
        match parse_args(args(s)) {
            Ok(Cli::Edit { request, check }) => Ok((request, check)),
            Ok(_) => panic!("not an edit"),
            Err(e) => Err(e),
        }
    }

    #[test]
    fn edit_flags_become_edits_in_order() {
        let (r, check) = edit(
            "edit game.exfat --add /tmp/a.sprx:fakelib/libSceAmpr.sprx \
             --delete data/old.bin --rename data/a.bin:data/b.bin --mkdir data/new --rebuild",
        )
        .unwrap();
        assert_eq!(r.format, Format::Exfat);
        assert!(r.output.as_os_str().is_empty());
        assert!(r.edit_rebuild);
        assert!(!check);
        assert_eq!(
            r.edits,
            vec![
                Edit::Add {
                    from: PathBuf::from("/tmp/a.sprx"),
                    to: "fakelib/libSceAmpr.sprx".into(),
                },
                Edit::Delete {
                    path: "data/old.bin".into(),
                },
                Edit::Rename {
                    from: "data/a.bin".into(),
                    to: "data/b.bin".into(),
                },
                Edit::Mkdir {
                    path: "data/new".into(),
                },
            ]
        );
    }

    #[test]
    fn edit_add_keeps_a_windows_drive_with_from() {
        let (r, _) =
            edit(r"edit game.ffpkg --add C:\games\a.sprx:fakelib/libSceAmpr.sprx").unwrap();
        assert_eq!(
            r.edits,
            vec![Edit::Add {
                from: PathBuf::from(r"C:\games\a.sprx"),
                to: "fakelib/libSceAmpr.sprx".into(),
            }]
        );
        assert_eq!(r.format, Format::Ffpkg);
    }
}
