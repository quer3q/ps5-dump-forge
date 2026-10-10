//! Save as profile: the pack plan a Pack job resolves, written as an editable TOML rules profile
//! (the subset `ps5_dump_forge_lz4::load_profile` honors) that selects exactly the same files.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use anyhow::{Context, bail};
use ps5_dump_forge_lz4::PackSpec;
use ps5_dump_forge_lz4::writer::PackFile;

use crate::jobs::Ctx;
use crate::preflight::GameInfo;
use crate::{ConvertRequest, Event, Lz4PlanProfile};

/// [`crate::lz4_plan_profile`].
pub(crate) fn plan_profile(req: &ConvertRequest) -> anyhow::Result<Lz4PlanProfile> {
    let log = Arc::new(Mutex::new(Vec::new()));
    let sink = log.clone();
    let emit: Box<dyn Fn(Event) + Send + Sync> = Box::new(move |e| {
        if let Event::Log { line, .. } = e {
            sink.lock().unwrap_or_else(|p| p.into_inner()).push(line);
        }
    });
    let cancel = AtomicBool::new(false);
    let ctx = Ctx::new(0, &emit, &cancel);
    let (plan, info) = crate::convert::lz4_plan(req, &ctx)?;
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let packed = plan.files.iter().filter(|f| f.spec.is_some()).count();
    let header = Header {
        game: crate::inspect::name_stem(&info),
        rules: &plan.rules,
        date: date(secs),
    };
    let toml = render(&plan.files, plan.runtime.as_ref(), &header);
    let log = std::mem::take(&mut *log.lock().unwrap_or_else(|p| p.into_inner()));
    Ok(Lz4PlanProfile {
        file_name: file_name(&info),
        toml,
        packed,
        loose: plan.files.len() - packed,
        log,
    })
}

pub(crate) fn file_name(info: &GameInfo) -> String {
    crate::inspect::download_name(info, "lz4profile.toml")
}

pub(crate) struct Header<'a> {
    pub game: Option<String>,
    pub rules: &'a str,
    pub date: String,
}

/// The profile: comments, `[pack]` (loose by default, auto-loose off: the plan already applied
/// it), one `[[rule]]` per distinct spec listing its exact paths, the job's `[runtime]` if any.
pub(crate) fn render(
    files: &[PackFile],
    runtime: Option<&ps5_dump_forge_lz4::RuntimeProfile>,
    h: &Header,
) -> String {
    let mut groups: BTreeMap<(u8, bool, bool, bool), Vec<&str>> = BTreeMap::new();
    for f in files {
        if let Some(PackSpec {
            block_shift,
            store,
            hot,
            random,
        }) = f.spec
        {
            let key = (block_shift, store, hot, random);
            groups.entry(key).or_default().push(&f.path);
        }
    }
    let packed: usize = groups.values().map(Vec::len).sum();
    let mut out = String::new();
    let mut line = |s: &str| {
        out.push_str(s);
        out.push('\n');
    };
    line(&format!(
        "# PS5 Dump Forge {} LZ4 rules profile",
        env!("CARGO_PKG_VERSION")
    ));
    line(&format!(
        "# Game: {}",
        comment(h.game.as_deref().unwrap_or("unknown"))
    ));
    line(&format!("# Rules from: {}", comment(h.rules)));
    line(&format!("# Made: {} (UTC)", h.date));
    line(&format!(
        "# {packed} files packed, {} loose; a file no rule lists stays loose.",
        files.len() - packed
    ));
    line("# Edit freely; load it with LZ4 -> Pack -> Use a rules profile (CLI --lz4-profile).");
    line("# Forge's keep-loose list (container indexes, configs, media, protected folders) still");
    line("# applies to whatever a profile packs.");
    line("");
    line("[pack]");
    line("default_action = \"loose\"");
    line("default_block_size = \"64KiB\"");
    line("# The plan already applied auto-loose: this list is final.");
    line("auto_loose_large_files = false");
    for ((shift, store, hot, random), paths) in &groups {
        line("");
        line("[[rule]]");
        line(if *store {
            "action = \"store\""
        } else {
            "action = \"compress\""
        });
        if *shift != 16 {
            line(&format!("block_size = {}", 1u64 << shift));
        }
        if *hot {
            line("hot = true");
        }
        // `auto` with `hot` means random: a hot sequential file says `mixed`.
        if *random {
            line("layout = \"random\"");
        } else if *hot {
            line("layout = \"mixed\"");
        }
        line("include = [");
        for p in paths {
            line(&format!("    {},", toml_string(&glob_escape(p))));
        }
        line("]");
    }
    if let Some(r) = runtime {
        line("");
        line("[runtime]");
        // Quoted: a size string holds every u64 the loader takes, a TOML integer only i64.
        line(&format!(
            "decoded_cache_bytes = \"{}\"",
            r.decoded_cache_bytes
        ));
        line(&format!(
            "physical_cache_bytes = \"{}\"",
            r.physical_cache_bytes
        ));
        line(&format!("workers = {}", r.workers));
        line(&format!(
            "latency_reserve_workers = {}",
            r.latency_reserve_workers
        ));
    }
    out
}

/// [`crate::write_lz4_plan_profile`].
pub(crate) fn write(source: &Path, dest: &Path, toml: &str, replace: bool) -> anyhow::Result<()> {
    let Some(name) = dest.file_name() else {
        bail!("{}: not a file name", dest.display());
    };
    let parent = dest.parent().filter(|p| !p.as_os_str().is_empty());
    let parent = parent
        .unwrap_or(Path::new("."))
        .canonicalize()
        .with_context(|| format!("{}", dest.display()))?;
    let target = parent.join(name);
    let src = source
        .canonicalize()
        .with_context(|| format!("{}", source.display()))?;
    let inside = if src.is_dir() {
        parent.ancestors().any(|a| same_entry(a, &src))
    } else {
        std::fs::symlink_metadata(&target).is_ok() && same_entry(&target, &src)
    };
    if inside {
        bail!(
            "{}: a profile is never saved into the source; choose a folder outside it",
            dest.display()
        );
    }
    match std::fs::symlink_metadata(&target) {
        Ok(m) if !m.is_file() => bail!("{}: exists and is not a regular file", dest.display()),
        Ok(_) if !replace => bail!("{}: already exists", dest.display()),
        _ => {}
    }
    let put = |path: &Path| -> std::io::Result<()> {
        // `create_new` (O_EXCL) never follows a symlink at `path`.
        let mut f = OpenOptions::new().write(true).create_new(true).open(path)?;
        f.write_all(toml.as_bytes())?;
        f.sync_all()
    };
    let shown = |e: std::io::Error| anyhow::anyhow!("{}: {e}", dest.display());
    if !replace {
        return put(&target).map_err(shown);
    }
    let mut tmp_name = std::ffi::OsString::from(".");
    tmp_name.push(name);
    tmp_name.push(format!(".forge-{}.tmp", std::process::id()));
    let tmp = parent.join(tmp_name);
    let _ = std::fs::remove_file(&tmp);
    put(&tmp)
        .and_then(|()| std::fs::rename(&tmp, &target))
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            shown(e)
        })
}

/// Whether `a` and `b` name the same file or folder: device and inode on Unix (aliases through
/// case or links included), the canonical paths elsewhere.
fn same_entry(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (std::fs::metadata(a), std::fs::metadata(b)) {
            (Ok(x), Ok(y)) => x.dev() == y.dev() && x.ino() == y.ino(),
            _ => false,
        }
    }
    // ponytail: Windows compares canonical paths, ignoring case; no file-id check.
    #[cfg(not(unix))]
    {
        let c = |p: &Path| p.canonicalize().map(|p| p.to_string_lossy().to_lowercase());
        matches!((c(a), c(b)), (Ok(x), Ok(y)) if x == y)
    }
}

/// `path` as an fnmatch pattern that matches only itself.
fn glob_escape(path: &str) -> String {
    let mut s = String::with_capacity(path.len());
    for c in path.chars() {
        match c {
            '[' => s.push_str("[[]"),
            '*' => s.push_str("[*]"),
            '?' => s.push_str("[?]"),
            c => s.push(c),
        }
    }
    s
}

/// A TOML basic string.
fn toml_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// One comment line's text: line breaks and other controls become spaces.
fn comment(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// `YYYY-MM-DD` of a Unix time (UTC), by the days-to-civil arithmetic.
fn date(secs: u64) -> String {
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ps5_dump_forge_lz4::{Selection, load_profile, select};

    fn spec(block_shift: u8, store: bool, hot: bool, random: bool) -> Option<PackSpec> {
        Some(PackSpec {
            block_shift,
            store,
            hot,
            random,
        })
    }

    #[test]
    fn dates() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(951_782_400), "2000-02-29");
        assert_eq!(date(1_791_590_400), "2026-10-10");
    }

    #[test]
    fn round_trips_to_the_same_specs() {
        let files: Vec<PackFile> = [
            ("data/plain.bin", spec(16, false, false, false)),
            ("data/a[1].bin", spec(16, false, false, false)),
            ("data/*star?.bin", spec(17, false, false, false)),
            ("data/quo\"te\\x.bin", spec(16, true, false, false)),
            ("data/ünï cødé.bin", spec(16, false, true, true)),
            ("data/hot-seq.bin", spec(16, false, true, false)),
            ("data/rand.bin", spec(20, false, false, true)),
            ("data/[!x].bin", spec(14, true, true, true)),
            ("data/loose.bin", None),
            ("data/a1.bin", None),
            ("data/xstarx.bin", None),
            ("root.bin", None),
        ]
        .into_iter()
        .map(|(p, s)| PackFile {
            path: p.into(),
            size: 100,
            spec: s,
        })
        .collect();
        let runtime = ps5_dump_forge_lz4::RuntimeProfile {
            decoded_cache_bytes: 1 << 63,
            physical_cache_bytes: 32768,
            workers: 4,
            latency_reserve_workers: 1,
        };
        let h = Header {
            game: Some("[Gäme \"x\"]-[PPSA01234]".into()),
            rules: "the traces copied to /a\nb",
            date: date(0),
        };
        let toml = render(&files, Some(&runtime), &h);
        assert!(toml.starts_with("# PS5 Dump Forge "), "{toml}");
        assert!(
            toml.contains("# Rules from: the traces copied to /a b\n"),
            "{toml}"
        );
        assert!(toml.contains("# 8 files packed, 4 loose"), "{toml}");
        let dir = std::env::temp_dir().join(format!("forge-planprofile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("p.toml");
        std::fs::write(&path, &toml).unwrap();
        let p = load_profile(&path).unwrap_or_else(|e| panic!("{e:?}\n{toml}"));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(p.ignored.is_empty(), "{:?}", p.ignored);
        assert!(!p.auto_loose.enabled);
        assert_eq!(p.runtime, Some(runtime));
        for f in &files {
            let got = select(&f.path, f.size, &Selection::Profile(&p));
            assert_eq!(got, f.spec, "{}\n{toml}", f.path);
        }
    }

    #[test]
    fn never_writes_into_the_source() {
        let root = std::env::temp_dir().join(format!("forge-planwrite-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let game = root.join("game");
        std::fs::create_dir_all(game.join("data")).unwrap();
        std::fs::write(game.join("data/settings.toml"), "game data").unwrap();
        let image = root.join("g.ffpkg");
        std::fs::write(&image, "image").unwrap();
        let out = root.join("out");
        std::fs::create_dir_all(&out).unwrap();
        let refused = |source: &Path, dest: &Path, replace: bool| {
            let e = write(source, dest, "x", replace).unwrap_err().to_string();
            assert!(
                e.contains("never saved into the source"),
                "{}: {e}",
                dest.display()
            );
        };
        refused(&game, &game.join("data/settings.toml"), true);
        refused(&game, &game.join("new.toml"), false);
        refused(&game, &game.join("data/../p.toml"), true);
        refused(&image, &image, true);
        #[cfg(unix)]
        {
            // Aliases: a symlinked folder into the source, a symlink at the destination.
            std::os::unix::fs::symlink(&game, root.join("alias")).unwrap();
            refused(&game, &root.join("alias/data/p.toml"), true);
            std::os::unix::fs::symlink(&image, out.join("link.toml")).unwrap();
            refused(&image, &out.join("link.toml"), true);
            let e = write(&game, &out.join("link.toml"), "x", true).unwrap_err();
            assert!(e.to_string().contains("not a regular file"), "{e}");
            assert_eq!(std::fs::read_to_string(&image).unwrap(), "image");
        }
        assert_eq!(
            std::fs::read_to_string(game.join("data/settings.toml")).unwrap(),
            "game data"
        );
        // Outside: new, then replaced only with `replace`.
        let p = out.join("p.toml");
        write(&game, &p, "one", false).unwrap();
        let e = write(&game, &p, "two", false).unwrap_err();
        assert!(e.to_string().contains("already exists"), "{e}");
        write(&image, &p, "two", true).unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "two");
        let names: Vec<_> = std::fs::read_dir(&out)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert!(
            !names.iter().any(|n| n.to_string_lossy().ends_with(".tmp")),
            "{names:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn names() {
        let info = crate::preflight::parse_param(
            br#"{"titleId":"PPSA13197","localizedParameters":{"defaultLanguage":"en-US","en-US":{"titleName":"Stellar Blade"}}}"#,
        );
        assert_eq!(
            file_name(&info),
            "[Stellar Blade]-[PPSA13197]-lz4profile.toml"
        );
        assert_eq!(file_name(&GameInfo::default()), "lz4profile.toml");
    }
}
