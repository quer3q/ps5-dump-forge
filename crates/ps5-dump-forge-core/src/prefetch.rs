//! Read-ahead: the source is read on its own thread, one range ahead of the writer, so the
//! next read overlaps hashing and writing the last one.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::thread::JoinHandle;

use ps5upload_fpkg::source::{SourceFile, SourceTree};
use ps5upload_fpkg::{Error, Result};

/// Only ranges up to this size are read ahead; with the one the writer holds, at most twice
/// this is in memory.
const AHEAD_MAX: usize = 8 << 20;

enum Ask {
    Range(String, u64, usize),
    Whole(String),
}

/// A range read ahead and what reading it gave.
struct Ahead {
    path: String,
    offset: u64,
    len: usize,
    got: Result<Vec<u8>>,
}

/// A source whose reads run on a worker thread. After serving `read_range(path, offset, len)`
/// with `n` bytes the worker reads `(path, offset + n, len)` before the writer asks for it.
/// Any other request drops that guess, so the bytes served are always those a direct read
/// would give; a failed guess fails only the request that asks for that range.
///
/// The writer still checks its cancel flag between reads. Dropping this waits for the worker,
/// so a read already in a syscall finishes first.
// ponytail: the next file is not read ahead, so a game of many small files gains little.
// Upgrade: guess the next file in the writer's order (each writer has its own).
pub(crate) struct Prefetch {
    files: Vec<SourceFile>,
    empty_dirs: Vec<String>,
    describe: String,
    ask: Option<SyncSender<Ask>>,
    answer: Receiver<Result<Vec<u8>>>,
    worker: Option<JoinHandle<()>>,
}

impl Prefetch {
    pub(crate) fn new(tree: Box<dyn SourceTree>) -> std::io::Result<Self> {
        let (files, empty_dirs, describe) = (
            tree.files().to_vec(),
            tree.empty_dirs().to_vec(),
            tree.describe(),
        );
        // One request is outstanding at a time; the bounds only make that explicit.
        let (ask, asks) = sync_channel(1);
        let (answers, answer) = sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("read-ahead".into())
            .spawn(move || serve(tree, asks, answers))?;
        Ok(Self {
            files,
            empty_dirs,
            describe,
            ask: Some(ask),
            answer,
            worker: Some(worker),
        })
    }

    fn call(&mut self, ask: Ask) -> Result<Vec<u8>> {
        let sent = self.ask.as_ref().map(|a| a.send(ask).is_ok());
        if sent == Some(true)
            && let Ok(answer) = self.answer.recv()
        {
            return answer;
        }
        // The worker is gone: only a panic in the source ends it early. Rethrow it here, so
        // the job's `catch_unwind` sees it as it would without read-ahead.
        if let Some(Err(panic)) = self.worker.take().map(JoinHandle::join) {
            std::panic::resume_unwind(panic);
        }
        Err(Error::Format("the read-ahead thread stopped".into()))
    }
}

impl Drop for Prefetch {
    fn drop(&mut self) {
        // Closing the channel ends the worker's loop once its current read returns.
        self.ask = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn serve(mut tree: Box<dyn SourceTree>, asks: Receiver<Ask>, answers: SyncSender<Result<Vec<u8>>>) {
    let sizes: HashMap<String, u64> = tree
        .files()
        .iter()
        .map(|f| (f.path.clone(), f.size))
        .collect();
    let mut ahead: Option<Ahead> = None;
    for ask in asks {
        let (answer, next) = match ask {
            Ask::Whole(path) => (tree.read(&path), None),
            Ask::Range(path, offset, len) => {
                let answer = match ahead.take() {
                    // A prefix of a longer read at the same offset is what a read of `len` gives.
                    Some(a) if a.path == path && a.offset == offset && len <= a.len => {
                        a.got.map(|mut bytes| {
                            bytes.truncate(len);
                            bytes
                        })
                    }
                    _ => tree.read_range(&path, offset, len),
                };
                let next = match &answer {
                    Ok(bytes) if !bytes.is_empty() && len <= AHEAD_MAX => offset
                        .checked_add(bytes.len() as u64)
                        .filter(|&at| sizes.get(&path).is_some_and(|&size| at < size))
                        .map(|at| (path, at, len)),
                    _ => None,
                };
                (answer, next)
            }
        };
        if answers.send(answer).is_err() {
            return;
        }
        if let Some((path, offset, len)) = next {
            let got = tree.read_range(&path, offset, len);
            ahead = Some(Ahead {
                path,
                offset,
                len,
                got,
            });
        }
    }
}

impl SourceTree for Prefetch {
    fn files(&self) -> &[SourceFile] {
        &self.files
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        self.call(Ask::Whole(path.to_string()))
    }

    fn read_range(&mut self, path: &str, offset: u64, len: usize) -> Result<Vec<u8>> {
        self.call(Ask::Range(path.to_string(), offset, len))
    }

    fn empty_dirs(&self) -> &[String] {
        &self.empty_dirs
    }

    fn describe(&self) -> String {
        self.describe.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use super::*;

    /// What the inner tree was asked, in order: (path, offset, len).
    type Log = Arc<Mutex<Vec<(String, u64, usize)>>>;

    /// An in-memory tree that logs its range reads, fails at `bad` offsets, can be held at a
    /// gate, and says when it is dropped.
    struct Mem {
        files: Vec<SourceFile>,
        data: Vec<Vec<u8>>,
        log: Log,
        bad: Vec<u64>,
        gate: Option<Receiver<()>>,
        dropped: Arc<Mutex<bool>>,
    }

    impl Mem {
        fn new(files: &[(&str, Vec<u8>)]) -> Self {
            Self {
                files: files
                    .iter()
                    .map(|(p, d)| SourceFile {
                        path: p.to_string(),
                        size: d.len() as u64,
                    })
                    .collect(),
                data: files.iter().map(|(_, d)| d.clone()).collect(),
                log: Log::default(),
                bad: Vec::new(),
                gate: None,
                dropped: Arc::default(),
            }
        }
    }

    impl Drop for Mem {
        fn drop(&mut self) {
            *self.dropped.lock().unwrap() = true;
        }
    }

    impl SourceTree for Mem {
        fn files(&self) -> &[SourceFile] {
            &self.files
        }
        fn read(&mut self, path: &str) -> Result<Vec<u8>> {
            let i = self.files.iter().position(|f| f.path == path).unwrap();
            Ok(self.data[i].clone())
        }
        fn read_range(&mut self, path: &str, offset: u64, len: usize) -> Result<Vec<u8>> {
            self.log.lock().unwrap().push((path.into(), offset, len));
            if let Some(gate) = &self.gate {
                let _ = gate.recv();
            }
            if path == "boom" {
                panic!("the source panicked");
            }
            if self.bad.contains(&offset) {
                return Err(Error::Format(format!("bad read at {offset}")));
            }
            let i = self.files.iter().position(|f| f.path == path).unwrap();
            let data = &self.data[i];
            let start = (offset as usize).min(data.len());
            Ok(data[start..(start + len).min(data.len())].to_vec())
        }
        fn empty_dirs(&self) -> &[String] {
            &[]
        }
        fn describe(&self) -> String {
            "mem".into()
        }
    }

    fn bytes(len: usize, salt: u8) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8 ^ salt).collect()
    }

    /// Waits until `log` holds `n` reads, then a little longer to see that no more come.
    fn settled(log: &Log, n: usize) -> Vec<(String, u64, usize)> {
        let start = Instant::now();
        while log.lock().unwrap().len() < n && start.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(1));
        }
        std::thread::sleep(Duration::from_millis(50));
        log.lock().unwrap().clone()
    }

    fn at(path: &str, offset: u64, len: usize) -> (String, u64, usize) {
        (path.into(), offset, len)
    }

    #[test]
    fn sequential_reads_are_read_once_and_one_ahead() {
        let data = bytes(1000, 1);
        let mem = Mem::new(&[("f", data.clone()), ("g", bytes(10, 2))]);
        let log = mem.log.clone();
        let mut tree = Prefetch::new(Box::new(mem)).unwrap();
        assert_eq!(tree.files().len(), 2);
        assert_eq!(tree.describe(), "mem");
        assert_eq!(tree.read_range("f", 0, 300).unwrap(), data[..300]);
        // Only the one range after it is read ahead, however long the writer takes.
        assert_eq!(settled(&log, 2), [at("f", 0, 300), at("f", 300, 300)]);
        assert_eq!(tree.read_range("f", 300, 300).unwrap(), data[300..600]);
        assert_eq!(tree.read_range("f", 600, 300).unwrap(), data[600..900]);
        // The tail asks for less than the guess read: its prefix, read once.
        assert_eq!(tree.read_range("f", 900, 100).unwrap(), data[900..]);
        // Nothing is read past the end of the file.
        assert_eq!(
            settled(&log, 4),
            [
                at("f", 0, 300),
                at("f", 300, 300),
                at("f", 600, 300),
                at("f", 900, 300)
            ]
        );
        assert_eq!(tree.read("g").unwrap(), bytes(10, 2));
    }

    #[test]
    fn other_requests_drop_the_guess() {
        let (f, g) = (bytes(1000, 3), bytes(1000, 4));
        let mem = Mem::new(&[("f", f.clone()), ("g", g.clone())]);
        let log = mem.log.clone();
        let mut tree = Prefetch::new(Box::new(mem)).unwrap();
        assert_eq!(tree.read_range("f", 0, 100).unwrap(), f[..100]);
        // A gap, another file, a longer read at the guessed offset, a read backwards.
        assert_eq!(tree.read_range("f", 500, 100).unwrap(), f[500..600]);
        assert_eq!(tree.read_range("g", 0, 100).unwrap(), g[..100]);
        assert_eq!(tree.read_range("g", 100, 200).unwrap(), g[100..300]);
        assert_eq!(tree.read_range("g", 0, 50).unwrap(), g[..50]);
        assert_eq!(tree.read_range("g", 50, 20).unwrap(), g[50..70], "a hit");
        let reads = settled(&log, 11);
        assert_eq!(
            reads,
            [
                at("f", 0, 100),
                at("f", 100, 100),
                at("f", 500, 100),
                at("f", 600, 100),
                at("g", 0, 100),
                at("g", 100, 100),
                at("g", 100, 200),
                at("g", 300, 200),
                at("g", 0, 50),
                at("g", 50, 50),
                at("g", 70, 20),
            ]
        );
    }

    #[test]
    fn a_failed_guess_fails_only_the_read_that_asks_for_it() {
        let data = bytes(1000, 5);
        let mut mem = Mem::new(&[("f", data.clone())]);
        mem.bad = vec![100, 300];
        let mut tree = Prefetch::new(Box::new(mem)).unwrap();
        // The guess at 100 failed; nobody asks for it.
        assert_eq!(tree.read_range("f", 0, 100).unwrap(), data[..100]);
        assert_eq!(tree.read_range("f", 200, 100).unwrap(), data[200..300]);
        // The guess at 300 failed; this read asks for it.
        let err = tree.read_range("f", 300, 100).unwrap_err().to_string();
        assert_eq!(err, "bad read at 300");
    }

    #[test]
    fn dropping_waits_for_a_read_in_progress_and_frees_the_source() {
        let (open, gate) = sync_channel(0);
        let mut mem = Mem::new(&[("f", bytes(1000, 6))]);
        mem.gate = Some(gate);
        let (log, dropped) = (mem.log.clone(), mem.dropped.clone());
        let mut tree = Prefetch::new(Box::new(mem)).unwrap();
        let reader = std::thread::spawn(move || {
            tree.read_range("f", 0, 100).unwrap();
            // A cancelled writer stops reading and drops its source mid-guess.
            drop(tree);
        });
        open.send(()).unwrap(); // the read asked for
        assert_eq!(settled(&log, 2).len(), 2, "the guess is in its read");
        assert!(!reader.is_finished(), "dropping waits for it");
        assert!(!*dropped.lock().unwrap());
        open.send(()).unwrap();
        reader.join().unwrap();
        assert!(*dropped.lock().unwrap());
    }

    #[test]
    fn a_panic_in_the_source_reaches_the_reader() {
        let mem = Mem::new(&[("boom", bytes(10, 7))]);
        let mut tree = Prefetch::new(Box::new(mem)).unwrap();
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = tree.read_range("boom", 0, 10);
        }))
        .unwrap_err();
        assert_eq!(panic.downcast_ref::<&str>(), Some(&"the source panicked"));
        let err = tree.read_range("boom", 0, 10).unwrap_err().to_string();
        assert!(err.contains("read-ahead thread stopped"), "{err}");
    }
}
