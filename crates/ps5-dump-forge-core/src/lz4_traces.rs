//! A source's LZ4 traces (the journal and its path index at the game's root), read off the
//! console for packing elsewhere: front to back in pieces, never whole in memory (a journal
//! can be GiBs), each read refused once its source has changed.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::AtomicBool;

use ps5upload_fpkg::source::SourceTree;

use crate::LZ4_TRACE_FILES;
use crate::convert::{Kind, SourceStamp, open_source};

/// The two trace files of a source, in [`LZ4_TRACE_FILES`] order, and the name their
/// download takes. A game relaunched on a writable mounted image truncates its journal while
/// an image reader goes on returning the old blocks, so every read checks that the source is
/// unchanged since opening (an image: the image file; a folder: both trace files) and fails
/// otherwise: a copy never mixes two sessions.
pub struct Lz4Traces {
    zip_name: String,
    sizes: [u64; 2],
    done: [u64; 2],
    from: Source,
    stamps: Vec<SourceStamp>,
}

enum Source {
    /// A folder's two files, opened directly: no scan of the whole game.
    Files([File; 2]),
    Tree(Box<dyn SourceTree>),
}

impl Lz4Traces {
    /// `[GAME_TITLE]-[TITLE_ID]-amprtrace.zip` (see [`crate::lz4_traces`]).
    pub fn zip_name(&self) -> &str {
        &self.zip_name
    }

    /// Each file's name and size, journal first.
    pub fn files(&self) -> [(&'static str, u64); 2] {
        [
            (LZ4_TRACE_FILES[0], self.sizes[0]),
            (LZ4_TRACE_FILES[1], self.sizes[1]),
        ]
    }

    /// The bytes of [`Lz4Traces::write_zip`]'s archive.
    pub fn zip_len(&self) -> u64 {
        crate::zip::length(&self.entries())
    }

    /// Both files as one zip (STORED, the CRC-32 computed as the bytes go out) into `out`,
    /// exactly [`Lz4Traces::zip_len`] bytes, or an error part way (the source changed, a read
    /// failed, `out` refused).
    pub fn write_zip(&mut self, out: &mut dyn io::Write) -> io::Result<()> {
        let entries = self.entries();
        crate::zip::write(out, &entries, &mut |i, buf| self.read(i, buf))
    }

    fn entries(&self) -> [crate::zip::Entry; 2] {
        self.files()
            .map(|(name, size)| crate::zip::Entry { name, size })
    }

    /// The next bytes of file `i` (0: journal, 1: index), up to `buf.len()`; 0 only at its end.
    /// Fails when the file ends early or the source has changed since it was opened.
    pub fn read(&mut self, i: usize, buf: &mut [u8]) -> io::Result<usize> {
        let (name, pos) = (LZ4_TRACE_FILES[i], self.done[i]);
        let want = (self.sizes[i] - pos).min(buf.len() as u64) as usize;
        if want == 0 {
            return Ok(0);
        }
        let n = match &mut self.from {
            Source::Files(files) => files[i].read(&mut buf[..want])?,
            Source::Tree(tree) => {
                let bytes = tree.read_range(name, pos, want).map_err(io::Error::other)?;
                let n = bytes.len().min(want);
                buf[..n].copy_from_slice(&bytes[..n]);
                n
            }
        };
        // After the read, so these bytes are known to predate any change.
        if let Some(stamp) = self.stamps.iter().find(|s| !s.unchanged()) {
            return Err(io::Error::other(format!(
                "{} changed during the download (the game or a sync wrote to it); \
                 close the game and download again",
                stamp.path().display()
            )));
        }
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("{name}: ended at {pos} of {} bytes", self.sizes[i]),
            ));
        }
        self.done[i] += n as u64;
        Ok(n)
    }
}

/// Opens the traces of `source` (a folder or any image this app reads). The inner `Err` names
/// what is missing (no journal, or no index beside it); the outer one is a source that can't be
/// read.
pub(crate) fn open(source: &Path) -> anyhow::Result<Result<Lz4Traces, String>> {
    use anyhow::Context;
    let [journal, index] = LZ4_TRACE_FILES;
    let missing = |name: &str| {
        Ok(Err(match name == journal {
            true => format!("{}: no {journal} at its root", source.display()),
            false => format!(
                "{}: no {index} beside {journal}; Pack needs it, the journal names files by it",
                source.display()
            ),
        }))
    };
    let kind = Kind::of(source)?;
    let param_path = "sce_sys/param.json";
    if kind == Kind::Folder {
        let mut files = Vec::new();
        let mut stamps = Vec::new();
        let mut sizes = [0u64; 2];
        for (i, name) in LZ4_TRACE_FILES.into_iter().enumerate() {
            let path = source.join(name);
            let file = match crate::scan::open_nofollow(&path) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => return missing(name),
                r => r.with_context(|| path.display().to_string())?,
            };
            let meta = file
                .metadata()
                .with_context(|| path.display().to_string())?;
            if !meta.is_file() {
                anyhow::bail!("{}: not a regular file", path.display());
            }
            // From the handle, then checked against the path: a file replaced between the
            // open and here is never paired with the other one's stamp.
            let stamp = SourceStamp::of_handle(&path, &file)?;
            if !stamp.unchanged() {
                anyhow::bail!(
                    "{} changed while it was being opened (the game or a sync wrote to it); \
                     close the game and try again",
                    path.display()
                );
            }
            stamps.push(stamp);
            sizes[i] = meta.len();
            files.push(file);
        }
        let param = small_file(&source.join(param_path));
        let files: [File; 2] = files.try_into().map_err(|_| anyhow::anyhow!("two files"))?;
        return Ok(Ok(Lz4Traces {
            zip_name: crate::inspect::traces_zip_name(param.as_deref()),
            sizes,
            done: [0; 2],
            from: Source::Files(files),
            stamps,
        }));
    }
    // Before the open: an image replaced after this point fails the first read.
    let stamp = SourceStamp::of_file(source)?;
    let mut tree = open_source(source, kind, &AtomicBool::new(false))?;
    let size = |tree: &dyn SourceTree, path: &str| {
        tree.files().iter().find(|f| f.path == path).map(|f| f.size)
    };
    let mut sizes = [0u64; 2];
    for (i, name) in LZ4_TRACE_FILES.into_iter().enumerate() {
        match size(&*tree, name) {
            Some(s) => sizes[i] = s,
            None => return missing(name),
        }
    }
    let param = size(&*tree, param_path)
        .filter(|&s| s <= crate::preflight::MAX_PARAM_JSON)
        .and_then(|_| tree.read(param_path).ok());
    Ok(Ok(Lz4Traces {
        zip_name: crate::inspect::traces_zip_name(param.as_deref()),
        sizes,
        done: [0; 2],
        from: Source::Tree(tree),
        stamps: vec![stamp],
    }))
}

/// Traces copied off the console, as `--lz4-traces` names them: a `*-amprtrace.zip` (STORED
/// entries, as Download traces writes it), a folder holding both files, or `ampr_commands.bin` with
/// its index beside it. The index whole (more than `max_index` bytes is refused) and the
/// journal as a stream, a zip's checked against its CRC-32 as it is read. `Err`: why not, for
/// the findings.
pub(crate) fn open_copied(path: &Path, max_index: u64) -> Result<(Vec<u8>, Box<dyn Read>), String> {
    let [journal, index] = LZ4_TRACE_FILES;
    let is_zip = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    if is_zip && !path.is_dir() {
        return open_zip(path, max_index)
            .map_err(|e| format!("the LZ4 traces zip {}: {e}", path.display()));
    }
    let (dir, journal_path) = if path.is_dir() {
        (path.to_path_buf(), path.join(journal))
    } else {
        (
            path.parent().unwrap_or(Path::new("")).to_path_buf(),
            path.to_path_buf(),
        )
    };
    let shown = || match path.is_dir() {
        true => format!(
            "the LZ4 traces folder {} (with {journal} and {index})",
            path.display()
        ),
        false => format!("the LZ4 traces {} (with {index} beside it)", path.display()),
    };
    let open = |p: &Path| File::open(p).map_err(|e| format!("{}: {}: {e}", shown(), p.display()));
    let file = open(&journal_path)?;
    let mut bytes = Vec::new();
    open(&dir.join(index))?
        .take(max_index + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{}: {e}", shown()))?;
    if bytes.len() as u64 > max_index {
        return Err(format!("{}: {index} is too big to be one", shown()));
    }
    Ok((bytes, Box::new(file)))
}

fn open_zip(path: &Path, max_index: u64) -> Result<(Vec<u8>, Box<dyn Read>), String> {
    use crate::zip::{Checked, data_at, directory};
    let damaged = |e: String| format!("the traces zip is damaged: {e}");
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    let entries = directory(&mut file, len).map_err(damaged)?;
    let find = |name: &str| {
        entries
            .iter()
            .find(|e| e.name == name.as_bytes())
            .cloned()
            .ok_or_else(|| format!("no {name} in it (a zip from Download traces holds both files)"))
    };
    let [journal, index] = LZ4_TRACE_FILES;
    let (j, i) = (find(journal)?, find(index)?);
    let (j_at, i_at) = (data_at(&mut file, len, &j), data_at(&mut file, len, &i));
    let (j_at, i_at) = (j_at.map_err(damaged)?, i_at.map_err(damaged)?);
    if i.size > max_index {
        return Err(format!(
            "its {index} is {} bytes, too big to be one",
            i.size
        ));
    }
    let mut bytes = Vec::new();
    file.seek(SeekFrom::Start(i_at))
        .and_then(|_| {
            Checked::new((&file).take(i.size), index, i.size, i.crc).read_to_end(&mut bytes)
        })
        .map_err(|e| e.to_string())?;
    file.seek(SeekFrom::Start(j_at))
        .map_err(|e| e.to_string())?;
    let stream = Checked::new(file.take(j.size), journal, j.size, j.crc);
    Ok((bytes, Box::new(stream)))
}

/// A folder's `param.json`, when it is a regular file of a plausible size.
fn small_file(path: &Path) -> Option<Vec<u8>> {
    let file = crate::scan::open_nofollow(path).ok()?;
    let meta = file.metadata().ok()?;
    (meta.is_file() && meta.len() <= crate::preflight::MAX_PARAM_JSON).then_some(())?;
    let mut bytes = Vec::new();
    file.take(crate::preflight::MAX_PARAM_JSON)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_all(t: &mut Lz4Traces, i: usize) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut buf = [0u8; 4];
        loop {
            match t.read(i, &mut buf)? {
                0 => return Ok(out),
                n => out.extend_from_slice(&buf[..n]),
            }
        }
    }

    /// Both files read whole; either missing names it; a file rewritten while it is read (a
    /// game relaunched truncates its journal) fails the next read.
    #[test]
    fn folder_traces() {
        let dir = std::env::temp_dir().join(format!("forge-traces-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sce_sys")).unwrap();
        let [journal, index] = LZ4_TRACE_FILES;
        std::fs::write(dir.join(journal), b"first session").unwrap();
        let err = open(&dir).unwrap().err().unwrap();
        assert!(err.contains("no ampr_emu.index beside"), "{err}");
        std::fs::write(dir.join(index), b"idx").unwrap();
        std::fs::write(
            dir.join("sce_sys/param.json"),
            br#"{"titleId":"PPSA01234","titleName":"Game: Part 2"}"#,
        )
        .unwrap();
        let mut t = open(&dir).unwrap().unwrap();
        assert_eq!(t.zip_name(), "[Game Part 2]-[PPSA01234]-amprtrace.zip");
        assert_eq!(t.files(), [(journal, 13), (index, 3)]);
        assert_eq!(read_all(&mut t, 0).unwrap(), b"first session");
        assert_eq!(read_all(&mut t, 1).unwrap(), b"idx");

        let mut t = open(&dir).unwrap().unwrap();
        let mut buf = [0u8; 5];
        assert_eq!(t.read(0, &mut buf).unwrap(), 5);
        std::fs::write(dir.join(journal), b"second!").unwrap();
        let err = t.read(0, &mut buf).unwrap_err();
        assert!(
            err.to_string().contains("changed during the download"),
            "{err}"
        );
        // A stamp taken from a handle describes that file: once the path names another (a
        // sync replaced it after the open), it no longer matches.
        let path = dir.join(index);
        let held = File::open(&path).unwrap();
        assert!(SourceStamp::of_handle(&path, &held).unwrap().unchanged());
        std::fs::write(dir.join("new"), b"idx").unwrap();
        std::fs::rename(dir.join("new"), &path).unwrap();
        assert!(!SourceStamp::of_handle(&path, &held).unwrap().unchanged());

        std::fs::remove_file(dir.join(journal)).unwrap();
        let err = open(&dir).unwrap().err().unwrap();
        assert!(err.contains("no ampr_commands.bin at its root"), "{err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
