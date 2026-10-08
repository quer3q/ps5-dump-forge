//! `POST /api/debug {path}`: what the platform's file calls return for `path`, raw and as std
//! decodes them. Only in builds made with `FORGE_DEBUG_API` set, to any value (`FORGE_DEBUG_API=1
//! ps5/build.sh`), for bringing up the PS5 payload; release builds have no such route.

use std::ffi::{CStr, CString};
use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::time::Instant;

use serde_json::{Value, json};

pub(crate) const ENABLED: bool = option_env!("FORGE_DEBUG_API").is_some();

pub(crate) fn probe(path: &str) -> Value {
    let meta = |m: io::Result<fs::Metadata>| match m {
        Ok(m) => json!({ "dir": m.is_dir(), "file": m.is_file(), "symlink": m.is_symlink(),
            "len": m.len(), "mode": format!("{:o}", mode(&m)) }),
        Err(e) => json!({ "error": e.to_string(), "errno": e.raw_os_error() }),
    };
    let listed: Vec<Value> = match fs::read_dir(path) {
        Ok(dir) => dir
            .take(12)
            .map(|e| match e {
                Ok(e) => json!({
                    "name": e.file_name().to_string_lossy(),
                    "file_type": e.file_type().map(|t| format!("{t:?}")).map_err(|e| e.to_string()),
                    "metadata": meta(e.metadata()),
                }),
                Err(e) => json!({ "error": e.to_string() }),
            })
            .collect(),
        Err(e) => vec![json!({ "error": e.to_string() })],
    };
    json!({
        "path": path,
        "std_metadata": meta(fs::metadata(path)),
        "std_symlink_metadata": meta(fs::symlink_metadata(path)),
        "std_canonicalize": fs::canonicalize(path).map(|p| p.display().to_string())
            .map_err(|e| e.to_string()),
        "std_read_dir": listed,
        "raw": raw::probe(path),
    })
}

/// `POST /api/debug_bench {source, dir, mib}`: the drive's ceiling next to the reader's pattern.
/// Reads `source` at 10/20/30 GiB in (far from what a job read last, so not cached), writes and
/// removes two scratch files in `dir`.
pub(crate) fn bench(source: &str, dir: &str, mib: u64) -> Value {
    let run = || -> io::Result<Value> {
        let bytes = mib << 20;
        let rate = |b: u64, t: Instant| {
            json!({ "mib": b >> 20, "secs": t.elapsed().as_secs_f64(),
            "mb_per_s": b as f64 / t.elapsed().as_secs_f64() / 1e6 })
        };
        let mut src = fs::File::open(source)?;
        // One 8 MiB read at a time.
        let mut big = vec![0u8; 8 << 20];
        src.seek(SeekFrom::Start(10 << 30))?;
        let t = Instant::now();
        for _ in 0..bytes / big.len() as u64 {
            src.read_exact(&mut big)?;
        }
        let sequential_8m = rate(bytes, t);
        // The UFS2 reader today: per 64 KiB block, two 8-byte pointer reads, then the block,
        // each after a seek.
        let mut block = vec![0u8; 64 << 10];
        let mut ptr = [0u8; 8];
        let base = 20u64 << 30;
        let t = Instant::now();
        for i in 0..bytes / block.len() as u64 {
            for _ in 0..2 {
                src.seek(SeekFrom::Start(4096 + (i % 512) * 8))?;
                src.read_exact(&mut ptr)?;
            }
            src.seek(SeekFrom::Start(base + i * block.len() as u64))?;
            src.read_exact(&mut block)?;
        }
        let reader_pattern = rate(bytes, t);
        // 64 KiB blocks, one seek each, no pointer reads.
        let base = 30u64 << 30;
        let t = Instant::now();
        for i in 0..bytes / block.len() as u64 {
            src.seek(SeekFrom::Start(base + i * block.len() as u64))?;
            src.read_exact(&mut block)?;
        }
        let blocks_64k = rate(bytes, t);
        let write = |sync_every: Option<u64>| -> io::Result<Value> {
            let path =
                std::path::Path::new(dir).join(format!("forge-bench-{}.part", std::process::id()));
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            let t = Instant::now();
            let mut since = 0;
            for _ in 0..bytes / big.len() as u64 {
                f.write_all(&big)?;
                since += big.len() as u64;
                if sync_every.is_some_and(|n| since >= n) {
                    f.sync_all()?;
                    since = 0;
                }
            }
            let before_sync = t.elapsed().as_secs_f64();
            f.sync_all()?;
            let mut r = rate(bytes, t);
            r["secs_before_final_sync"] = json!(before_sync);
            drop(f);
            fs::remove_file(&path)?;
            Ok(r)
        };
        Ok(json!({
            "read_sequential_8m": sequential_8m,
            "read_reader_pattern": reader_pattern,
            "read_64k_blocks": blocks_64k,
            "write_8m_no_sync": write(None)?,
            "write_8m_sync_64m": write(Some(64 << 20))?,
            "write_8m_sync_256m": write(Some(256 << 20))?,
        }))
    };
    run().unwrap_or_else(|e| json!({ "error": e.to_string() }))
}

#[cfg(unix)]
fn mode(m: &fs::Metadata) -> u32 {
    use std::os::unix::fs::MetadataExt;
    m.mode()
}

#[cfg(not(unix))]
fn mode(_: &fs::Metadata) -> u32 {
    0
}

#[cfg(unix)]
mod raw {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// `stat`/`lstat` into a buffer of `0xaa`: the untouched tail shows how much the call wrote.
    #[allow(clippy::unnecessary_cast)] // field widths differ between targets
    fn stat(c: &CStr, link: bool) -> Value {
        let mut buf = [0xaau8; 512];
        let ptr = buf.as_mut_ptr().cast::<libc::stat>();
        // SAFETY: `buf` is 512 bytes, larger than any `struct stat`, and suitably aligned for
        // the reads below (`read_unaligned`).
        let rc = unsafe {
            if link {
                libc::lstat(c.as_ptr(), ptr)
            } else {
                libc::stat(c.as_ptr(), ptr)
            }
        };
        let errno = io::Error::last_os_error().raw_os_error();
        let written = buf.iter().rposition(|&b| b != 0xaa).map_or(0, |i| i + 1);
        // SAFETY: the buffer holds a whole `libc::stat`, initialized (0xaa where untouched).
        let st = unsafe { ptr.read_unaligned() };
        json!({ "rc": rc, "errno": errno, "bytes_written": written,
            "hex": hex(&buf[..written.clamp(64, 256)]),
            "decoded": { "st_mode": format!("{:o}", st.st_mode), "st_size": st.st_size,
                "st_ino": st.st_ino as u64, "st_dev": st.st_dev as u64 } })
    }

    fn realpath(c: &CStr) -> Value {
        // SAFETY: a null buffer asks realpath to allocate; freed below.
        let p = unsafe { libc::realpath(c.as_ptr(), std::ptr::null_mut()) };
        if p.is_null() {
            return json!({ "error": io::Error::last_os_error().to_string() });
        }
        // SAFETY: realpath returned a NUL-terminated malloc'd string.
        let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
        unsafe { libc::free(p.cast()) };
        json!(s)
    }

    /// The first records `readdir` returns: their header bytes and the name at `d_name`.
    fn readdir(c: &CStr) -> Value {
        // SAFETY: plain opendir/readdir/closedir on a C string; each record is read only within
        // its header and its NUL-terminated name.
        unsafe {
            let dir = libc::opendir(c.as_ptr());
            if dir.is_null() {
                return json!({ "error": io::Error::last_os_error().to_string() });
            }
            let mut out = Vec::new();
            for _ in 0..12 {
                let e = libc::readdir(dir);
                if e.is_null() {
                    break;
                }
                let head = std::slice::from_raw_parts(e.cast::<u8>(), 32);
                let name = CStr::from_ptr((*e).d_name.as_ptr())
                    .to_string_lossy()
                    .into_owned();
                out.push(json!({ "hex32": hex(head), "d_reclen": (*e).d_reclen,
                    "d_type": (*e).d_type, "d_namlen": (*e).d_namlen, "name": name }));
            }
            libc::closedir(dir);
            json!(out)
        }
    }

    pub(super) fn probe(path: &str) -> Value {
        let Ok(c) = CString::new(path) else {
            return json!({ "error": "path holds a NUL" });
        };
        json!({
            "size_of_stat": std::mem::size_of::<libc::stat>(),
            "size_of_dirent": std::mem::size_of::<libc::dirent>(),
            "stat": stat(&c, false),
            "lstat": stat(&c, true),
            "realpath": realpath(&c),
            "readdir": readdir(&c),
        })
    }
}

#[cfg(not(unix))]
mod raw {
    use super::*;

    pub(super) fn probe(_: &str) -> Value {
        json!({ "error": "unix only" })
    }
}
