//! Block-size benchmark for the LZ4 packs, on a sample of a real game.
//!
//! ```sh
//! cargo run --release -p ps5-dump-forge-lz4 --example lz4_bench -- survey <game_dir>
//! cargo run --release -p ps5-dump-forge-lz4 --example lz4_bench -- bench <threads> <file[@offset+len]>...
//! ```
//! `survey` lists what the built-in fallback packs (sizes only, nothing is read). `bench` loads the
//! slices into RAM, then for every block shift 14..=20 runs the real `writer::pack` into memory
//! (1 thread and `<threads>`), reads the result back through `reader::unpack`, and times plain
//! `lz4_flex` block decompression. Each time is the median of 3 runs after one warm-up.

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use ps5_dump_forge_lz4::format::Codec;
use ps5_dump_forge_lz4::reader::{open_manifest, unpack};
use ps5_dump_forge_lz4::writer::{PackFile, PackOutput, pack};
use ps5_dump_forge_lz4::{MANIFEST, PackSpec, Selection, select};
use ps5upload_fpkg::source::{FolderSource, SourceFile, SourceTree};
use ps5upload_fpkg::{Error, Result};

#[derive(Default)]
struct Mem {
    files: Vec<SourceFile>,
    data: HashMap<String, Vec<u8>>,
}

impl Mem {
    fn add(&mut self, path: String, data: Vec<u8>) {
        self.files.push(SourceFile {
            path: path.clone(),
            size: data.len() as u64,
        });
        self.files.sort_by(|a, b| a.path.cmp(&b.path));
        self.data.insert(path, data);
    }
}

impl SourceTree for Mem {
    fn files(&self) -> &[SourceFile] {
        &self.files
    }
    fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        self.data
            .get(path)
            .cloned()
            .ok_or_else(|| Error::Format(path.into()))
    }
    fn read_range(&mut self, path: &str, offset: u64, len: usize) -> Result<Vec<u8>> {
        let d = self
            .data
            .get(path)
            .ok_or_else(|| Error::Format(path.into()))?;
        let s = (offset as usize).min(d.len());
        Ok(d[s..(s + len).min(d.len())].to_vec())
    }
    fn describe(&self) -> String {
        "memory sample".into()
    }
}

/// Packs into memory: no disk in the timings.
#[derive(Default)]
struct MemOut {
    done: HashMap<String, Vec<u8>>,
}

impl PackOutput for MemOut {
    type W = Cursor<Vec<u8>>;
    fn create(&mut self, _: &str) -> Result<Self::W> {
        Ok(Cursor::new(Vec::new()))
    }
    fn reopen(&mut self, name: &str) -> Result<Self::W> {
        Ok(Cursor::new(self.done.remove(name).expect("reopen")))
    }
    fn finish(&mut self, name: &str, w: Self::W) -> Result<()> {
        self.done.insert(name.into(), w.into_inner());
        Ok(())
    }
}

fn median<T: PartialOrd + Copy>(mut v: Vec<T>) -> T {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn survey(dir: &str) -> Result<()> {
    let tree = FolderSource::open(Path::new(dir))?;
    let (mut packed, mut loose) = (BTreeMap::new(), BTreeMap::new());
    for f in tree.files() {
        let ext = f
            .path
            .rsplit_once('.')
            .map_or("", |x| x.1)
            .to_ascii_lowercase();
        let m = if select(&f.path, f.size, &Selection::Fallback).is_some() {
            &mut packed
        } else {
            &mut loose
        };
        let e: &mut (u64, u64) = m.entry(ext).or_default();
        e.0 += 1;
        e.1 += f.size;
        if select(&f.path, f.size, &Selection::Fallback).is_some() {
            println!("PACK {:>14} {}", f.size, f.path);
        }
    }
    for (name, m) in [("packed", &packed), ("loose", &loose)] {
        for (ext, (n, b)) in m {
            println!("{name:7} .{ext:6} {n:5} files {b:>15} bytes");
        }
        println!(
            "{name} total: {} bytes",
            m.values().map(|v| v.1).sum::<u64>()
        );
    }
    Ok(())
}

fn load(arg: &str) -> std::io::Result<(String, Vec<u8>)> {
    let (path, range) = match arg.split_once('@') {
        Some((p, r)) => (p, Some(r)),
        None => (arg, None),
    };
    let mut f = File::open(path)?;
    let size = f.metadata()?.len();
    let (off, len) = match range.map(|r| r.split_once('+').expect("offset+len")) {
        Some((o, l)) => (o.parse().unwrap(), l.parse::<u64>().unwrap()),
        None => (0, size),
    };
    f.seek(SeekFrom::Start(off))?;
    let mut buf = Vec::new();
    f.take(len).read_to_end(&mut buf)?;
    let base = Path::new(path).file_name().unwrap().to_string_lossy();
    Ok((format!("{off}_{base}"), buf))
}

/// The writer's RAW/LZ4 decision (private there), copied: 64 KiB pages.
fn keep_lz4(raw: usize, compressed: usize) -> bool {
    if compressed >= raw {
        return false;
    }
    let (r, saved) = (raw as u64, (raw - compressed) as u64);
    if (compressed as u64).div_ceil(65536) >= r.div_ceil(65536) {
        saved >= 8192 && saved * 8 >= r
    } else {
        saved >= 64 && saved * 100 >= r
    }
}

fn run_pack(mem: &mut Mem, files: &[PackFile], shift: u8, threads: usize) -> (f64, MemOut) {
    let files: Vec<PackFile> = files
        .iter()
        .map(|f| PackFile {
            spec: Some(PackSpec {
                block_shift: shift,
                ..f.spec.unwrap()
            }),
            ..f.clone()
        })
        .collect();
    let mut out = MemOut::default();
    let t = Instant::now();
    pack(
        mem,
        &files,
        0,
        None,
        threads,
        &mut out,
        &mut |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    (t.elapsed().as_secs_f64(), out)
}

fn bench(threads: usize, args: &[&str]) -> Result<()> {
    let mut mem = Mem::default();
    let t = Instant::now();
    for a in args {
        let (name, data) = load(a)?;
        mem.add(name, data);
    }
    let total: u64 = mem.files.iter().map(|f| f.size).sum();
    println!(
        "sample: {} files, {total} bytes, read in {:.1}s ({:.0} MB/s)",
        mem.files.len(),
        t.elapsed().as_secs_f64(),
        total as f64 / 1e6 / t.elapsed().as_secs_f64()
    );
    let files: Vec<PackFile> = mem
        .files
        .iter()
        .map(|f| PackFile {
            path: f.path.clone(),
            size: f.size,
            spec: Some(PackSpec {
                block_shift: 16,
                store: false,
                hot: false,
                random: false,
            }),
        })
        .collect();
    println!(
        "shift block  stored/raw  disk/raw  lz4%blk  lz4%bytes  c1 MB/s  c{threads} MB/s  d_block MB/s  d_unpack MB/s"
    );
    for shift in 14..=20u8 {
        run_pack(&mut mem, &files, shift, threads); // warm-up
        let mut t1 = vec![];
        let mut tn = vec![];
        let mut last = None;
        for _ in 0..3 {
            t1.push(run_pack(&mut mem, &files, shift, 1).0);
            let (t, out) = run_pack(&mut mem, &files, shift, threads);
            tn.push(t);
            last = Some(out);
        }
        let out = last.unwrap();
        let disk: usize = out.done.values().map(Vec::len).sum();
        let m = open_manifest(&out.done[MANIFEST])?;
        let n_lz4 = m.chunks.iter().filter(|c| c.codec == Codec::Lz4).count();
        let stored: u64 = m.chunks.iter().map(|c| u64::from(c.stored)).sum();
        // Plain block decode: compress each block once, time decompression of the kept ones.
        let bs = 1usize << shift;
        let mut kept: Vec<(Vec<u8>, usize)> = vec![];
        let mut lz4_raw = 0u64;
        for d in mem.data.values() {
            for b in d.chunks(bs) {
                let c = lz4_flex::block::compress(b);
                if keep_lz4(b.len(), c.len()) {
                    lz4_raw += b.len() as u64;
                    kept.push((c, b.len()));
                }
            }
        }
        let mut dt = vec![];
        for _ in 0..4 {
            let t = Instant::now();
            let mut sink = 0usize;
            for (c, n) in &kept {
                sink += lz4_flex::block::decompress(c, *n).unwrap().len();
            }
            assert_eq!(sink as u64, lz4_raw);
            dt.push(t.elapsed().as_secs_f64());
        }
        dt.remove(0);
        // Sequential reads through the real reader.
        let mut ut = vec![];
        for _ in 0..4 {
            let mut inner = Mem::default();
            for (k, v) in &out.done {
                inner.add(k.clone(), v.clone());
            }
            let mut u = unpack(Box::new(inner))?;
            let t = Instant::now();
            for f in &files {
                let mut off = 0;
                while off < f.size {
                    let n = u.read_range(&f.path, off, 1 << 20)?.len();
                    off += n as u64;
                }
            }
            ut.push(t.elapsed().as_secs_f64());
        }
        ut.remove(0);
        let mb = |s: f64, b: u64| b as f64 / 1e6 / s;
        println!(
            "{shift:5} {:>4}K  {:9.4}  {:8.4}  {:7.1}  {:9.1}  {:7.0}  {:8.0}  {:12.0}  {:13.0}",
            bs >> 10,
            stored as f64 / total as f64,
            disk as f64 / total as f64,
            100.0 * n_lz4 as f64 / m.chunks.len() as f64,
            100.0 * lz4_raw as f64 / total as f64,
            mb(median(t1), total),
            mb(median(tn), total),
            mb(median(dt), lz4_raw),
            mb(median(ut), total),
        );
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["survey", dir] => survey(dir),
        ["bench", threads, ref files @ ..] => bench(threads.parse().expect("threads"), files),
        _ => {
            eprintln!("usage: lz4_bench survey <dir> | bench <threads> <file[@offset+len]>...");
            std::process::exit(2);
        }
    }
}
