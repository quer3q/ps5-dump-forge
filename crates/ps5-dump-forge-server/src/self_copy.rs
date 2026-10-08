//! The PS5 payload saves a copy of its own ELF as `/data/ps5-dump-forge/ps5-dump-forge.elf`,
//! so the home-screen tile's page can ask elfldr to start it after a console restart. A running
//! payload can't read its own file, so `ps5/build.sh` builds twice: stage 1 (no copy inside),
//! then stage 2 with stage 1 embedded by `build.rs` as a blob:
//!
//! `PDFGSELF`, u64 LE raw length, u64 LE compressed length, SHA-256 of the raw ELF (32 bytes),
//! then the zlib stream. `scripts/release-ps5.sh` reads the same layout.

use std::cmp::Ordering;
use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use flate2::read::ZlibDecoder;
use sha2::{Digest, Sha256};

const MAGIC: &[u8; 8] = b"PDFGSELF";
const HEADER: usize = 8 + 8 + 8 + 32;
/// More than any payload this will ever be; a bad descriptor can't ask for more memory.
const MAX_ELF: u64 = 256 << 20;

/// The ELF a blob carries: decompressed to exactly its declared length, its SHA-256 checked,
/// and an ELF of this `version` (its `ps5-dump-forge-version:` marker).
pub(crate) fn unpack(blob: &[u8], version: &str) -> Result<Vec<u8>, String> {
    if blob.len() < HEADER || &blob[..8] != MAGIC {
        return Err("no PDFGSELF descriptor".into());
    }
    let u64_at = |at: usize| u64::from_le_bytes(blob[at..at + 8].try_into().unwrap());
    let (raw_len, packed_len) = (u64_at(8), u64_at(16));
    if packed_len != (blob.len() - HEADER) as u64 {
        return Err(format!(
            "descriptor says {packed_len} compressed bytes, blob holds {}",
            blob.len() - HEADER
        ));
    }
    if raw_len > MAX_ELF {
        return Err(format!("descriptor says {raw_len} bytes, over {MAX_ELF}"));
    }
    let mut elf = Vec::with_capacity(raw_len as usize);
    ZlibDecoder::new(&blob[HEADER..])
        .take(raw_len + 1)
        .read_to_end(&mut elf)
        .map_err(|e| format!("decompressing: {e}"))?;
    if elf.len() as u64 != raw_len {
        return Err(format!(
            "decompressed to {}{} bytes, descriptor says {raw_len}",
            elf.len(),
            if elf.len() as u64 > raw_len { "+" } else { "" }
        ));
    }
    if Sha256::digest(&elf).as_slice() != &blob[24..HEADER] {
        return Err("SHA-256 mismatch".into());
    }
    if !elf.starts_with(b"\x7fELF") {
        return Err("not an ELF".into());
    }
    match version_of(&elf) {
        Some(v) if v == version => Ok(elf),
        found => Err(format!("version marker {found:?}, want {version:?}")),
    }
}

/// The version in the first `ps5-dump-forge-version:<v>\0` marker.
pub(crate) fn version_of(elf: &[u8]) -> Option<&str> {
    // Sliced from the one marker literal, so no second copy of its prefix sits in the ELF
    // (scripts/release-ps5.sh wants exactly one).
    let marker = crate::VERSION_MARKER;
    let prefix = &marker[..marker.len() - 1 - env!("CARGO_PKG_VERSION").len()];
    let at = elf.windows(prefix.len()).position(|w| w == prefix)? + prefix.len();
    let rest = &elf[at..elf.len().min(at + 64)];
    let end = rest.iter().position(|&b| b == 0)?;
    std::str::from_utf8(&rest[..end]).ok()
}

/// `a` against `b`: `x.y.z` numerically, then a release above its pre-releases
/// (`0.0.1` > `0.0.1-pre3`), and pre-releases by their digit runs as numbers (`pre10` > `pre3`).
pub(crate) fn compare(a: &str, b: &str) -> Ordering {
    /// A run of digits `(false, n, "")` or of anything else `(true, 0, text)`: numbers sort
    /// below text.
    type Run<'a> = (bool, u64, &'a str);
    fn runs(s: &str) -> Vec<Run<'_>> {
        let mut out = Vec::new();
        let mut rest = s;
        while let Some(c) = rest.chars().next() {
            let digit = c.is_ascii_digit();
            let end = rest
                .find(|c: char| c.is_ascii_digit() != digit)
                .unwrap_or(rest.len());
            let (run, tail) = rest.split_at(end);
            out.push(match digit {
                true => (false, run.parse().unwrap_or(u64::MAX), ""),
                false => (true, 0, run),
            });
            rest = tail;
        }
        out
    }
    fn split(v: &str) -> (Vec<u64>, Option<Vec<Run<'_>>>) {
        let (core, pre) = match v.split_once('-') {
            Some((core, pre)) => (core, Some(runs(pre))),
            None => (v, None),
        };
        (
            core.split('.').map(|n| n.parse().unwrap_or(0)).collect(),
            pre,
        )
    }
    let ((a_core, a_pre), (b_core, b_pre)) = (split(a), split(b));
    a_core.cmp(&b_core).then(match (a_pre, b_pre) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(a), Some(b)) => a.cmp(&b),
    })
}

#[derive(Debug, PartialEq)]
pub(crate) enum Outcome {
    Saved,
    /// The file is already these bytes.
    UpToDate,
    /// The file is a strictly newer version (this one), left as it is.
    KeptNewer(String),
}

/// Makes `target` hold `elf` (of `version`) unless it already does or holds a strictly newer
/// version. Anything else (missing, older, the same version built differently, no marker) is
/// replaced: a unique temporary file next to it, synced, renamed over it, the folder synced.
// ponytail: reads whatever file is at `target` (up to MAX_ELF) to find its version; another
// program swapping files there in between is out of scope (no protections, by decision).
pub(crate) fn ensure(target: &Path, elf: &[u8], version: &str) -> io::Result<Outcome> {
    match fs::symlink_metadata(target) {
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(e),
        Ok(meta) if !meta.is_file() => {
            let why = format!("{} is not a regular file; left alone", target.display());
            return Err(io::Error::other(why));
        }
        Ok(_) => {
            let mut old = Vec::new();
            fs::File::open(target)?
                .take(MAX_ELF)
                .read_to_end(&mut old)?;
            if old == elf {
                return Ok(Outcome::UpToDate);
            }
            if let Some(v) = version_of(&old)
                && compare(v, version) == Ordering::Greater
            {
                return Ok(Outcome::KeptNewer(v.to_string()));
            }
        }
    }
    let dir = target.parent().unwrap_or(Path::new("."));
    let name = target.file_name().and_then(|n| n.to_str()).unwrap_or("elf");
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    // Only a dead process with our pid can have left this one.
    let _ = fs::remove_file(&tmp);
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&tmp)?;
        file.write_all(elf)?;
        file.sync_all()?;
        fs::rename(&tmp, target)?;
        fs::File::open(dir)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map(|()| Outcome::Saved)
}

/// On `serve` start: the copy, in a background thread, logged to stdout (the serve log) and
/// shown as `self_copy` in `GET /api/session`.
#[cfg(target_env = "ps5")]
mod payload {
    use std::path::Path;
    use std::sync::Mutex;
    use std::thread::JoinHandle;

    use super::Outcome;

    // `static SELF_ELF: Option<&[u8]>`: stage 2's blob, `None` in stage 1 (build.rs).
    include!(concat!(env!("OUT_DIR"), "/self_elf.rs"));

    const TARGET: &str = "/data/ps5-dump-forge/ps5-dump-forge.elf";
    static STATUS: Mutex<String> = Mutex::new(String::new());
    static SAVING: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

    fn set(line: String) {
        *STATUS.lock().unwrap_or_else(|e| e.into_inner()) = line;
    }

    pub(crate) fn status() -> String {
        STATUS.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub(crate) fn start() {
        let Some(blob) = SELF_ELF else {
            set("none".into());
            return;
        };
        set("saving".into());
        let version = env!("CARGO_PKG_VERSION");
        let save = move || {
            let line = match super::unpack(blob, version).and_then(|elf| {
                super::ensure(Path::new(TARGET), &elf, version).map_err(|e| e.to_string())
            }) {
                Ok(Outcome::Saved) => "saved".to_string(),
                Ok(Outcome::UpToDate) => "up to date".into(),
                Ok(Outcome::KeptNewer(v)) => format!("kept newer {v}"),
                Err(e) => format!("failed: {e}"),
            };
            println!("ps5-dump-forge serve: self copy {TARGET}: {line}");
            set(line);
        };
        match std::thread::Builder::new()
            .name("forge-self-copy".into())
            .spawn(save)
        {
            Ok(handle) => *SAVING.lock().unwrap_or_else(|e| e.into_inner()) = Some(handle),
            Err(e) => set(format!("failed: {e}")),
        }
    }

    /// Before the process exits (another copy already serves, or quit): the copy finishes.
    pub(crate) fn wait() {
        let handle = SAVING.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
}

#[cfg(target_env = "ps5")]
pub(crate) use payload::{start, status, wait};

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::ZlibEncoder;

    const V: &str = env!("CARGO_PKG_VERSION");

    fn elf(version: &str, filler: &[u8]) -> Vec<u8> {
        let prefix = &crate::VERSION_MARKER[..crate::VERSION_MARKER.len() - 1 - V.len()];
        [b"\x7fELF", filler, prefix, version.as_bytes(), b"\0"].concat()
    }

    /// build.rs's layout, with the lengths given.
    fn blob(raw: &[u8], raw_len: u64, hash: &[u8]) -> Vec<u8> {
        let mut z = ZlibEncoder::new(Vec::new(), Compression::best());
        z.write_all(raw).unwrap();
        let packed = z.finish().unwrap();
        let len = (packed.len() as u64).to_le_bytes();
        [MAGIC, &raw_len.to_le_bytes()[..], &len, hash, &packed].concat()
    }

    fn good(raw: &[u8]) -> Vec<u8> {
        blob(raw, raw.len() as u64, &Sha256::digest(raw))
    }

    #[test]
    fn unpacks_only_a_checked_blob() {
        let a = elf(V, &[7; 5000]);
        assert_eq!(unpack(&good(&a), V).unwrap(), a);
        let err = |b: &[u8]| unpack(b, V).unwrap_err();
        assert!(err(b"PDFGSELF").contains("no PDFGSELF"));
        let mut b = good(&a);
        b[0] = b'X';
        assert!(err(&b).contains("no PDFGSELF"));
        let b = good(&a);
        assert!(err(&b[..b.len() - 1]).contains("compressed bytes"));
        assert!(err(&[&b[..], b"x"].concat()).contains("compressed bytes"));
        // Lengths are checked before memory is set aside, and the stream is read only one
        // byte past what is declared.
        let hash = Sha256::digest(&a);
        assert!(err(&blob(&a, MAX_ELF + 1, &hash)).contains("over"));
        assert!(err(&blob(&a, 100, &hash)).contains("decompressed to 101+ bytes"));
        let short = format!("decompressed to {} bytes", a.len());
        assert!(err(&blob(&a, a.len() as u64 + 1, &hash)).contains(&short));
        assert!(err(&blob(&a, a.len() as u64, &[0; 32])).contains("SHA-256"));
        let mut c = good(&a);
        let at = c.len() - 10;
        c[at] ^= 0xff;
        assert!(unpack(&c, V).is_err(), "a corrupt stream");
        assert!(err(&good(&a[1..])).contains("not an ELF"));
        assert!(err(&good(b"\x7fELF no marker")).contains("version marker None"));
        assert!(err(&good(&elf("9.9.9", b""))).contains("\"9.9.9\""));
    }

    #[test]
    fn versions_order() {
        let order = [
            "0.0.1-pre3",
            "0.0.1-pre10",
            "0.0.1-rc1",
            "0.0.1",
            "0.0.2",
            "0.1.0",
            "1.0.0",
        ];
        for (i, a) in order.iter().enumerate() {
            for (j, b) in order.iter().enumerate() {
                assert_eq!(compare(a, b), i.cmp(&j), "{a} vs {b}");
            }
        }
        assert_eq!(version_of(&elf("0.0.2", b"x")), Some("0.0.2"));
        assert_eq!(version_of(b"no marker"), None);
    }

    #[test]
    fn copy_policy() {
        let dir = std::env::temp_dir().join(format!("forge-self-copy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("ps5-dump-forge.elf");
        let ours = elf(V, b"ours");
        let run = || ensure(&target, &ours, V).unwrap();

        assert_eq!(run(), Outcome::Saved, "missing");
        assert_eq!(fs::read(&target).unwrap(), ours);
        assert_eq!(run(), Outcome::UpToDate, "identical");
        fs::write(&target, elf("0.0.0", b"old")).unwrap();
        assert_eq!(run(), Outcome::Saved, "older");
        assert_eq!(fs::read(&target).unwrap(), ours);
        fs::write(&target, elf(V, b"another build")).unwrap();
        assert_eq!(run(), Outcome::Saved, "same version, other bytes");
        assert_eq!(fs::read(&target).unwrap(), ours);
        fs::write(&target, b"not ours").unwrap();
        assert_eq!(run(), Outcome::Saved, "no marker");
        let newer = elf("999.0.0", b"new");
        fs::write(&target, &newer).unwrap();
        assert_eq!(run(), Outcome::KeptNewer("999.0.0".into()));
        assert_eq!(fs::read(&target).unwrap(), newer);

        // A symlink there is neither followed nor replaced.
        let other = dir.join("other");
        fs::write(&other, b"theirs").unwrap();
        fs::remove_file(&target).unwrap();
        std::os::unix::fs::symlink(&other, &target).unwrap();
        assert!(ensure(&target, &ours, V).is_err());
        assert_eq!(fs::read(&other).unwrap(), b"theirs");

        // Nor is a folder.
        fs::remove_file(&target).unwrap();
        fs::create_dir(&target).unwrap();
        assert!(ensure(&target, &ours, V).is_err());
        // No temporary file left behind.
        let mut names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        names.sort();
        assert_eq!(names, ["other", "ps5-dump-forge.elf"]);
        fs::remove_dir_all(&dir).unwrap();
    }
}
