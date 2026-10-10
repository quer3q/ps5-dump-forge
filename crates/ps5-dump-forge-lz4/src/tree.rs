//! The packed output as a tree, without writing it: the loose files of the logical tree plus
//! the volumes, manifest, CRC sidecar and profile `pack` would write, with their exact sizes
//! from a [`crate::writer::measure`] pass. A volume's bytes are made on demand: each chunk is
//! re-read from its source block, compressed again by the one encoder `pack` uses, checked
//! against what the measure pass recorded (stored length, codec, CRC, and an LZ4 chunk's
//! decoded CRC) and put at its recorded offset; the header carries the final build ID and
//! everything else is zeros. So an image writer can take the packs straight from the source.
//!
//! Memory: the [`Measured`] metadata (16 bytes per chunk), one cached chunk, and at most
//! `threads * DEPTH_PER_WORKER` blocks in flight (twice that right after a seek).

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use ps5upload_fpkg::source::{SourceFile, SourceTree};
use ps5upload_fpkg::{Error, Result};

use crate::format::*;
use crate::writer::{
    DEPTH_PER_WORKER, Encoder, Measured, PackFile, check_cancel, encode, read_block,
};
use crate::{CRC_SIDECAR, MANIFEST, PROFILE};

fn bad(msg: impl Into<String>) -> Error {
    Error::Format(msg.into())
}

/// What the measure pass recorded for one chunk.
#[derive(Debug, Clone, Copy)]
struct Want {
    codec: Codec,
    stored: u32,
    crc: u32,
}

type Reply = std::result::Result<Vec<u8>, String>;
type Job = (Vec<u8>, bool, Want, Encoder, Sender<Reply>);

/// One chunk made again and checked against `want`; the stored bytes.
fn regenerate(raw: Vec<u8>, store: bool, want: Want, encoder: Encoder) -> Reply {
    let raw_len = raw.len();
    let e = encoder(raw, store);
    if e.codec != want.codec || e.data.len() != want.stored as usize || e.crc != want.crc {
        return Err(format!(
            "it came out {:?}, {} bytes, CRC {:#010x}; the measure pass recorded {:?}, {} bytes, \
             CRC {:#010x}",
            e.codec,
            e.data.len(),
            e.crc,
            want.codec,
            want.stored,
            want.crc
        ));
    }
    if e.codec == Codec::Lz4 {
        let mut out = vec![0; raw_len];
        match lz4_flex::block::decompress_into(&e.data, &mut out) {
            Ok(n) if n == raw_len && crc32(&out) == want.crc => {}
            _ => return Err("its LZ4 data does not decode to the recorded CRC".into()),
        }
    }
    Ok(e.data)
}

/// Compression threads owned by the tree; dropping it closes the queue and joins them.
struct Workers {
    work: Option<SyncSender<Job>>,
    handles: Vec<JoinHandle<()>>,
}

impl Workers {
    fn start(threads: usize) -> Result<Self> {
        let (work, queue) = mpsc::sync_channel::<Job>(threads * DEPTH_PER_WORKER);
        let queue = Arc::new(Mutex::new(queue));
        let mut handles = Vec::with_capacity(threads);
        for _ in 0..threads {
            let queue = Arc::clone(&queue);
            handles.push(
                std::thread::Builder::new()
                    .name("lz4-regenerate".into())
                    .spawn(move || {
                        loop {
                            let job = queue.lock().unwrap_or_else(|p| p.into_inner()).recv();
                            let Ok((raw, store, want, encoder, reply)) = job else {
                                return;
                            };
                            // The tree may have moved on (a seek); nothing to do then.
                            let _ = reply.send(regenerate(raw, store, want, encoder));
                        }
                    })?,
            );
        }
        Ok(Self {
            work: Some(work),
            handles,
        })
    }
}

impl Drop for Workers {
    fn drop(&mut self) {
        self.work.take();
        for h in self.handles.drain(..) {
            let _ = h.join();
        }
    }
}

/// See the module docs. `inner` is the logical tree the plan was measured on.
pub struct PackedTree<'a> {
    inner: &'a mut dyn SourceTree,
    m: &'a Measured,
    plan: &'a [PackFile],
    files: Vec<SourceFile>,
    empty_dirs: Vec<String>,
    volumes: HashMap<String, usize>,
    /// (first chunk, index in `plan`) of every packed record, in chunk order.
    firsts: Vec<(u32, u32)>,
    /// First chunk of each volume, then the chunk count.
    vol_first: Vec<usize>,
    chunks_at: usize,
    threads: usize,
    cancel: &'a AtomicBool,
    encoder: Encoder,
    workers: Option<Workers>,
    pending: VecDeque<(usize, Receiver<Reply>)>,
    next: usize,
    cache: Option<(usize, Vec<u8>)>,
}

impl<'a> PackedTree<'a> {
    /// `m` must come from `measure` over `inner` with `plan`; `threads` workers (0 counts as
    /// 1) compress, started on the first volume read.
    pub fn new(
        inner: &'a mut dyn SourceTree,
        m: &'a Measured,
        plan: &'a [PackFile],
        threads: usize,
        cancel: &'a AtomicBool,
    ) -> Result<Self> {
        let chunk_count = u64_at(&m.manifest, 48)? as usize;
        let chunks_at = u64_at(&m.manifest, 80)? as usize;
        let mut firsts = Vec::new();
        let mut first = 0u64;
        for (i, f) in plan.iter().enumerate() {
            if let Some(s) = f.spec {
                firsts.push((first as u32, i as u32));
                first += f.size.div_ceil(1 << s.block_shift);
            }
        }
        if u64_at(&m.manifest, 40)? != plan.len() as u64 || first != chunk_count as u64 {
            return Err(bad("internal error: the pack list is not the one measured"));
        }
        let mut tree = Self {
            inner,
            m,
            plan,
            files: Vec::new(),
            empty_dirs: Vec::new(),
            volumes: HashMap::new(),
            firsts,
            vol_first: vec![0; m.volumes.len() + 1],
            chunks_at,
            threads: threads.max(1),
            cancel,
            encoder: encode,
            workers: None,
            pending: VecDeque::new(),
            next: 0,
            cache: None,
        };
        for c in 0..chunk_count {
            let pid = tree.chunk_rec(c)?.0 as usize;
            if pid >= m.volumes.len() {
                return Err(bad("internal error: a chunk names no volume"));
            }
            tree.vol_first[pid + 1] += 1;
        }
        for i in 1..tree.vol_first.len() {
            tree.vol_first[i] += tree.vol_first[i - 1];
        }
        let packed: std::collections::HashSet<&str> = plan
            .iter()
            .filter(|f| f.spec.is_some())
            .map(|f| f.path.as_str())
            .collect();
        let mut files: Vec<SourceFile> = tree
            .inner
            .files()
            .iter()
            .filter(|f| !packed.contains(f.path.as_str()))
            .cloned()
            .collect();
        files.extend(
            m.outputs()
                .into_iter()
                .map(|(path, size)| SourceFile { path, size }),
        );
        files.sort_by(|a, b| a.path.cmp(&b.path));
        if let Some(w) = files.windows(2).find(|w| w[0].path == w[1].path) {
            return Err(bad(format!(
                "{} is both a source file and a pack file",
                w[0].path
            )));
        }
        tree.files = files;
        tree.empty_dirs = tree.inner.empty_dirs().to_vec();
        tree.volumes = m
            .volumes
            .iter()
            .enumerate()
            .map(|(i, (n, _))| (n.clone(), i))
            .collect();
        Ok(tree)
    }

    /// Replaces the encoder, for tests that need a compressor that disagrees with itself.
    #[cfg(test)]
    pub(crate) fn with_encoder(mut self, encoder: Encoder) -> Self {
        self.encoder = encoder;
        self
    }

    fn chunk_rec(&self, c: usize) -> Result<(u16, u64, Descriptor)> {
        let at = self.chunks_at + CHUNK_RECORD * c;
        let (pid, off) = unpack_location(u64_at(&self.m.manifest, at)?);
        Ok((
            pid,
            off,
            unpack_descriptor(u32_at(&self.m.manifest, at + 8)?)?,
        ))
    }

    /// The packed record and block of chunk `c`: (index in `plan`, block number).
    fn block_of(&self, c: usize) -> (usize, u64) {
        let i = self
            .firsts
            .partition_point(|&(first, _)| first as usize <= c)
            - 1;
        let (first, fi) = self.firsts[i];
        (fi as usize, (c - first as usize) as u64)
    }

    fn what(&self, c: usize) -> String {
        let (fi, b) = self.block_of(c);
        format!("{} block {b}", self.plan[fi].path)
    }

    fn submit(&mut self, c: usize) -> Result<()> {
        check_cancel(self.cancel)?;
        let (fi, b) = self.block_of(c);
        let plan = self.plan;
        let f = &plan[fi];
        let spec = f.spec.expect("only packed records have chunks");
        let block = 1u64 << spec.block_shift;
        let raw = read_block(
            &mut *self.inner,
            &f.path,
            b * block,
            block.min(f.size - b * block),
        )?;
        let d = self.chunk_rec(c)?.2;
        let want = Want {
            codec: d.codec,
            stored: d.stored,
            crc: u32_at(&self.m.crc, CRC_HEADER + 4 * c)?,
        };
        if self.workers.is_none() {
            self.workers = Some(Workers::start(self.threads)?);
        }
        let (reply, rx) = mpsc::channel();
        let work = self.workers.as_ref().and_then(|w| w.work.as_ref());
        work.expect("started above")
            .send((raw, spec.store, want, self.encoder, reply))
            .map_err(|_| bad("a compression worker failed"))?;
        self.pending.push_back((c, rx));
        Ok(())
    }

    /// The stored bytes of chunk `c`: cached, next in the pipeline, or after a seek.
    fn chunk(&mut self, c: usize) -> Result<&[u8]> {
        if !matches!(&self.cache, Some((k, _)) if *k == c) {
            if self.pending.front().map(|p| p.0) != Some(c) {
                self.pending.clear();
                self.next = c;
            }
            let end = self.vol_first[self.chunk_rec(c)?.0 as usize + 1];
            while self.pending.len() < self.threads * DEPTH_PER_WORKER && self.next < end {
                self.submit(self.next)?;
                self.next += 1;
            }
            let (_, rx) = self.pending.pop_front().expect("chunk c is submitted");
            let data = match rx.recv() {
                Ok(Ok(data)) => data,
                Ok(Err(why)) => {
                    self.pending.clear();
                    return Err(bad(format!(
                        "{}: the regenerated LZ4 chunk {c} differs from the measure pass: {why} \
                         (the source changed between its two reads, or the compressor is not \
                         deterministic)",
                        self.what(c)
                    )));
                }
                Err(_) => return Err(bad("a compression worker failed")),
            };
            self.cache = Some((c, data));
        }
        Ok(&self.cache.as_ref().expect("filled above").1)
    }

    fn read_volume(&mut self, id: usize, off: u64, len: usize) -> Result<Vec<u8>> {
        let size = self.m.volumes[id].1;
        let end = off.saturating_add(len as u64).min(size);
        if off >= end {
            return Ok(Vec::new());
        }
        let mut out = vec![0u8; (end - off) as usize];
        let head = DAT_HEADER as u64;
        if off < head {
            let h = self.m.volume_header(id)?;
            let to = end.min(head);
            out[..(to - off) as usize].copy_from_slice(&h[off as usize..to as usize]);
        }
        // The first chunk that ends after `off`, then every chunk that starts before `end`.
        let (mut lo, mut hi) = (self.vol_first[id], self.vol_first[id + 1]);
        let last = hi;
        while lo < hi {
            let mid = (lo + hi) / 2;
            let (_, o, d) = self.chunk_rec(mid)?;
            if o + u64::from(d.stored) <= off {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        for c in lo..last {
            let (_, o, d) = self.chunk_rec(c)?;
            if o >= end {
                break;
            }
            let data = self.chunk(c)?;
            let (s, e) = (o.max(off), (o + u64::from(d.stored)).min(end));
            out[(s - off) as usize..(e - off) as usize]
                .copy_from_slice(&data[(s - o) as usize..(e - o) as usize]);
        }
        Ok(out)
    }
}

fn slice(b: &[u8], off: u64, len: usize) -> Vec<u8> {
    let start = usize::try_from(off).unwrap_or(usize::MAX).min(b.len());
    b[start..start.saturating_add(len).min(b.len())].to_vec()
}

impl SourceTree for PackedTree<'_> {
    fn files(&self) -> &[SourceFile] {
        &self.files
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        let size = self
            .files
            .binary_search_by(|f| f.path.as_str().cmp(path))
            .map(|i| self.files[i].size)
            .map_err(|_| bad(format!("{path} is not in the packed tree")))?;
        self.read_range(path, 0, usize::try_from(size).unwrap_or(usize::MAX))
    }

    fn read_range(&mut self, path: &str, offset: u64, len: usize) -> Result<Vec<u8>> {
        if let Some(&id) = self.volumes.get(path) {
            return self.read_volume(id, offset, len);
        }
        match path {
            MANIFEST => return Ok(slice(&self.m.manifest, offset, len)),
            CRC_SIDECAR => return Ok(slice(&self.m.crc, offset, len)),
            PROFILE if self.m.profile.is_some() => {
                return Ok(slice(self.m.profile.as_deref().unwrap_or(&[]), offset, len));
            }
            _ => {}
        }
        if self
            .files
            .binary_search_by(|f| f.path.as_str().cmp(path))
            .is_err()
        {
            return Err(bad(format!("{path} is not in the packed tree")));
        }
        self.inner.read_range(path, offset, len)
    }

    fn empty_dirs(&self) -> &[String] {
        &self.empty_dirs
    }

    fn describe(&self) -> String {
        format!(
            "{} as LZ4 packs ({} volumes, made on demand)",
            self.inner.describe(),
            self.m.volumes.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writer::testkit::*;
    use crate::writer::{PAGE, VOLUME_CAP, measure_with_cap};
    use crate::{PackSpec, RuntimeProfile};

    type Entries = Vec<(&'static str, Vec<u8>, Option<PackSpec>)>;

    fn entries() -> Entries {
        let store = Some(PackSpec {
            block_shift: 15,
            store: true,
            hot: true,
            random: false,
        });
        vec![
            ("g/a", text(700_000, 1), spec(16)),
            ("g/b", noise(300_000, 2), spec(14)),
            ("g/c", noise(65, 3), spec(20)),
            ("g/d", text(1 << 20, 4), spec(20)),
            ("g/e", text(90_000, 5), store),
            ("g/loose", noise(1000, 6), None),
            ("g/f", noise(2 << 20, 7), spec(17)),
        ]
    }

    fn profile() -> RuntimeProfile {
        RuntimeProfile {
            decoded_cache_bytes: 1 << 20,
            physical_cache_bytes: 16384,
            workers: 4,
            latency_reserve_workers: 1,
        }
    }

    fn measured(
        e: &Entries,
        p: Option<&RuntimeProfile>,
        cap: u64,
    ) -> (Mem, Vec<PackFile>, Measured) {
        let mut src = Mem::new(e.iter().map(|(p, d, _)| (p.to_string(), d.clone())));
        let files: Vec<PackFile> = e
            .iter()
            .map(|(p, d, s)| PackFile {
                path: p.to_string(),
                size: d.len() as u64,
                spec: *s,
            })
            .collect();
        let cancel = AtomicBool::new(false);
        let m = measure_with_cap(
            &mut src,
            &files,
            1_700_000_000,
            p,
            3,
            &mut |_| {},
            &cancel,
            cap,
        )
        .unwrap();
        (src, files, m)
    }

    /// The tree serves exactly the deployed folder `pack` writes: same listing, same bytes
    /// whole, sequentially in odd pieces, and out of order.
    #[test]
    fn serves_what_pack_writes() {
        let p = profile();
        let e = entries();
        for (cap, prof) in [(VOLUME_CAP, None), (PAGE + (1 << 20), Some(&p))] {
            let (_, _, out, report) = pack_mem(&e, prof, 2, cap).unwrap();
            let (mut src, files, m) = measured(&e, prof, cap);
            assert_eq!(m.report, report);
            let mut want = deployed(&e, &out);
            let cancel = AtomicBool::new(false);
            let mut tree = PackedTree::new(&mut src, &m, &files, 3, &cancel).unwrap();
            assert_eq!(tree.files(), want.files(), "cap {cap}");
            for f in want.files().to_vec() {
                let whole = want.read(&f.path).unwrap();
                assert!(tree.read(&f.path).unwrap() == whole, "{}", f.path);
                let mut got = Vec::new();
                while (got.len() as u64) < f.size {
                    got.extend(tree.read_range(&f.path, got.len() as u64, 100_003).unwrap());
                }
                assert!(got == whole, "{} in pieces", f.path);
                for (off, len) in [(f.size / 2, 70_000), (0, 10), (f.size.saturating_sub(5), 9)] {
                    let end = (off as usize + len).min(whole.len());
                    let want = &whole[(off as usize).min(end)..end];
                    assert_eq!(tree.read_range(&f.path, off, len).unwrap(), want);
                }
            }
        }
    }

    fn appends_a_byte(raw: Vec<u8>, store: bool) -> crate::writer::Encoded {
        let mut e = encode(raw, store);
        e.data.push(0);
        e
    }

    fn other_codec(raw: Vec<u8>, _: bool) -> crate::writer::Encoded {
        encode(raw, true)
    }

    #[test]
    fn a_regenerated_chunk_that_differs_is_an_error() {
        let e = entries();
        let (mut src, files, m) = measured(&e, None, VOLUME_CAP);
        for encoder in [appends_a_byte as Encoder, other_codec] {
            let cancel = AtomicBool::new(false);
            let mut tree = PackedTree::new(&mut src, &m, &files, 2, &cancel)
                .unwrap()
                .with_encoder(encoder);
            let err = tree.read("ampr_assets-000.pak").unwrap_err().to_string();
            assert!(err.contains("differs from the measure pass"), "{err}");
            assert!(err.contains("g/a block 0"), "{err}");
        }
    }

    #[test]
    fn a_source_changed_between_the_passes_is_an_error() {
        let e = entries();
        let (_, files, m) = measured(&e, None, VOLUME_CAP);
        let mut changed = e.clone();
        changed[0].1[123_456] ^= 1;
        let mut src = Mem::new(changed.iter().map(|(p, d, _)| (p.to_string(), d.clone())));
        let cancel = AtomicBool::new(false);
        let mut tree = PackedTree::new(&mut src, &m, &files, 2, &cancel).unwrap();
        let err = tree.read("ampr_assets-000.pak").unwrap_err().to_string();
        assert!(err.contains("g/a block 1"), "{err}");
        // A shorter file fails its read instead.
        let mut short = e.clone();
        short[1].1.truncate(1000);
        let mut src = Mem::new(short.iter().map(|(p, d, _)| (p.to_string(), d.clone())));
        let mut tree = PackedTree::new(&mut src, &m, &files, 2, &cancel).unwrap();
        assert!(tree.read("ampr_assets-000.pak").is_err());
    }

    #[test]
    fn cancel_stops_both_passes() {
        let e = entries();
        let (mut src, files, m) = measured(&e, None, VOLUME_CAP);
        let cancel = AtomicBool::new(true);
        let r = measure_with_cap(
            &mut src,
            &files,
            0,
            None,
            2,
            &mut |_| {},
            &cancel,
            VOLUME_CAP,
        );
        assert!(matches!(r, Err(Error::Cancelled)));
        let mut tree = PackedTree::new(&mut src, &m, &files, 2, &cancel).unwrap();
        assert!(matches!(
            tree.read_range("ampr_assets-000.pak", PAGE, 10),
            Err(Error::Cancelled)
        ));
        // The header needs no source read.
        assert_eq!(
            tree.read_range("ampr_assets-000.pak", 0, 8).unwrap(),
            DAT_MAGIC
        );
    }

    /// The metadata is 16 bytes per chunk (a 12-byte manifest record and a 4-byte CRC) plus
    /// the per-file records, paths and volumes; no chunk data is kept.
    #[test]
    fn metadata_is_sixteen_bytes_per_chunk() {
        let n = 4096usize;
        let e: Entries = vec![("m/big", text(n << 14, 1), spec(14))];
        let (_, _, m) = measured(&e, None, VOLUME_CAP);
        let fixed = PAK_HEADER + FILE_RECORD + PACK_RECORD + CRC_HEADER;
        let strings = "/app0/m/big".len() + 1 + "ampr_assets-000.pak".len() + 1;
        assert_eq!(m.manifest.len() + m.crc.len(), fixed + strings + 16 * n);
        let per_chunk = (m.metadata_bytes() - fixed - strings) as f64 / n as f64;
        eprintln!(
            "measure metadata: {} bytes for {n} chunks ({per_chunk:.1} B/chunk); 1.4 M chunks: \
             ~{:.1} MB",
            m.metadata_bytes(),
            per_chunk * 1.4e6 / 1e6
        );
        assert!(per_chunk < 17.0, "{per_chunk}");
    }
}
