//! Open PS5 packages the way Convert does and read them back: every file of every package listed,
//! a sample read whole (or every byte with `--all`), image headers checked. A package that fails
//! says why; nothing is written.
//!
//! `cargo run --release -p ps5upload-fpkg --example pkg_check -- [--all] <pkg>...`
//! `... --example pkg_check -- --head <file in the package> <pkg>` prints that file's size and
//! first 64 bytes.

use std::path::Path;
use std::time::Instant;

use ps5upload_fpkg::{crypto, pkg_source, source::SourceTree};

/// Read whole every this-many-th file in sample mode.
const SAMPLE_EVERY: usize = 400;
/// A sampled file is read up to this much.
const SAMPLE_CAP: u64 = 64 << 20;
/// One read's size.
const CHUNK: u64 = 8 << 20;

fn main() {
    let mut all = false;
    let mut head = None;
    let mut paths = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--all" {
            all = true;
        } else if a == "--head" {
            head = args.next();
        } else {
            paths.push(a);
        }
    }
    if let Some(name) = head {
        for p in &paths {
            show_head(Path::new(p), &name);
        }
        return;
    }
    let mut failed = 0;
    for p in &paths {
        if !check(Path::new(p), all) {
            failed += 1;
        }
    }
    println!("{} of {} packages failed", failed, paths.len());
    if failed > 0 {
        std::process::exit(1);
    }
}

fn check(path: &Path, all: bool) -> bool {
    let t = Instant::now();
    println!("{}", path.display());
    let mut src = match pkg_source::open(path, crypto::DEFAULT_PASSCODE) {
        Ok(s) => s,
        Err(e) => {
            println!("  OPEN FAILED: {e}");
            return false;
        }
    };
    let files = src.files().to_vec();
    let total: u64 = files.iter().map(|f| f.size).sum();
    println!(
        "  opened in {:.1?}: {} files, {} bytes",
        t.elapsed(),
        files.len(),
        total
    );
    let (mut read, mut bad) = (0u64, 0);
    let step = if all { 1 } else { SAMPLE_EVERY };
    for f in files.iter().step_by(step) {
        let want = if all { f.size } else { f.size.min(SAMPLE_CAP) };
        let mut off = 0;
        while off < want {
            let n = (want - off).min(CHUNK) as usize;
            match src.read_range(&f.path, off, n) {
                Ok(b) if b.len() == n => {
                    if off == 0 && f.path.ends_with(".png") && !b.starts_with(b"\x89PNG")
                        // Games ship JPEGs under a .png name too (Minecraft does).
                        && !b.starts_with(&[0xFF, 0xD8, 0xFF])
                    {
                        println!("  NOT A PNG: {}", f.path);
                        bad += 1;
                    }
                }
                Ok(b) => {
                    println!("  SHORT READ: {} at {off}: {} of {n}", f.path, b.len());
                    bad += 1;
                    break;
                }
                Err(e) => {
                    println!("  READ FAILED: {} at {off}: {e}", f.path);
                    bad += 1;
                    break;
                }
            }
            off += n as u64;
            read += n as u64;
        }
        if bad > 20 {
            println!("  stopping after {bad} failures");
            break;
        }
    }
    println!(
        "  read {} bytes of {} files in {:.1?}: {}",
        read,
        files.len().div_ceil(step),
        t.elapsed(),
        if bad == 0 { "ok" } else { "FAILED" }
    );
    bad == 0
}

fn show_head(path: &Path, name: &str) {
    let mut src = match pkg_source::open(path, crypto::DEFAULT_PASSCODE) {
        Ok(s) => s,
        Err(e) => return println!("OPEN FAILED: {e}"),
    };
    let Some(size) = src.files().iter().find(|f| f.path == name).map(|f| f.size) else {
        return println!("{name}: not in the package");
    };
    match src.read_range(name, 0, size.min(64) as usize) {
        Ok(b) => println!("{name}: {size} bytes, starts {b:02x?}"),
        Err(e) => println!("{name}: READ FAILED: {e}"),
    }
}
