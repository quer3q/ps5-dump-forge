//! Validated zstd pack view over any `SourceTree`.

use std::collections::{HashMap, HashSet};

use ps5upload_fpkg::source::{SourceFile, SourceTree};
use ps5upload_fpkg::{Error, Result};
use rayon::prelude::*;

use crate::{
    CHUNK_SIZE, Codec, INDEX_HEADER_SIZE, Index, VOLUME_HEADER_SIZE, decode_index,
    decode_volume_header, volume_name,
};

pub const INDEX: &str = crate::INDEX_NAME;
const MAX_INDEX_BYTES: u64 = 512 << 20;
/// Stored bytes read per volume request: 8 chunks of 256 KiB, decoded in parallel.
const BATCH_CHUNKS: usize = ((2 << 20) / CHUNK_SIZE) as usize;

fn bad(message: impl Into<String>) -> Error {
    Error::Format(message.into())
}

fn exact<T: SourceTree + ?Sized>(
    tree: &mut T,
    path: &str,
    offset: u64,
    len: usize,
) -> Result<Vec<u8>> {
    let bytes = tree.read_range(path, offset, len)?;
    if bytes.len() != len {
        return Err(bad(format!(
            "{path}: short read at {offset}: {} of {len}",
            bytes.len()
        )));
    }
    Ok(bytes)
}

/// True when the source has a zstd pack index at its root. A damaged index is left
/// for `unpack` to report rather than being mistaken for a loose game.
pub fn detect<T: SourceTree + ?Sized>(tree: &T) -> bool {
    tree.files().iter().any(|file| file.path == INDEX)
}

/// A game tree with packed records decoded and pack artifacts hidden.
pub struct Unpacked<T: SourceTree + ?Sized> {
    inner: Box<T>,
    index: Index,
    files: Vec<SourceFile>,
    empty_dirs: Vec<String>,
    packed: HashMap<String, usize>,
    hidden: HashSet<String>,
}

/// Validate the whole index and each volume header and advertised physical size.
pub fn unpack<T: SourceTree + ?Sized>(mut inner: Box<T>) -> Result<Unpacked<T>> {
    let mut sizes = HashMap::with_capacity(inner.files().len());
    for file in inner.files() {
        if sizes.insert(file.path.clone(), file.size).is_some() {
            return Err(bad(format!("duplicate source path: {}", file.path)));
        }
    }
    let size = *sizes
        .get(INDEX)
        .ok_or_else(|| bad(format!("no {INDEX} at the root")))?;
    if !(INDEX_HEADER_SIZE as u64..=MAX_INDEX_BYTES).contains(&size) {
        return Err(bad(format!("{INDEX}: invalid size {size}")));
    }
    let bytes = exact(&mut *inner, INDEX, 0, size as usize)?;
    let index = decode_index(&bytes).map_err(|error| bad(error.to_string()))?;
    let mut hidden = HashSet::with_capacity(index.volume_sizes.len() + 1);
    hidden.insert(INDEX.to_owned());
    for (id, &expected) in index.volume_sizes.iter().enumerate() {
        let name = volume_name(id as u32);
        let actual = sizes
            .get(name.as_str())
            .ok_or_else(|| bad(format!("missing {name}")))?;
        if *actual != expected {
            return Err(bad(format!("{name}: size {actual}, expected {expected}")));
        }
        let bytes = exact(&mut *inner, &name, 0, VOLUME_HEADER_SIZE)?;
        let header =
            decode_volume_header(&bytes).map_err(|error| bad(format!("{name}: {error}")))?;
        if header.id != id as u32
            || header.count != index.volume_sizes.len() as u32
            || header.archive_id != index.archive_id
            || header.payload_bytes != expected - VOLUME_HEADER_SIZE as u64
        {
            return Err(bad(format!("{name}: header disagrees with {INDEX}")));
        }
        hidden.insert(name);
    }
    let mut packed = HashMap::with_capacity(index.files.len());
    for (i, file) in index.files.iter().enumerate() {
        if sizes.contains_key(file.path.as_str()) || hidden.contains(&file.path) {
            return Err(bad(format!(
                "{}: packed path also exists physically",
                file.path
            )));
        }
        packed.insert(file.path.clone(), i);
    }
    let mut files: Vec<_> = inner
        .files()
        .iter()
        .filter(|file| !hidden.contains(&file.path))
        .cloned()
        .collect();
    files.extend(index.files.iter().map(|file| SourceFile {
        path: file.path.clone(),
        size: file.logical_size,
    }));
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let empty_dirs = inner.empty_dirs().to_vec();
    Ok(Unpacked {
        inner,
        index,
        files,
        empty_dirs,
        packed,
        hidden,
    })
}

pub(crate) fn decode_chunk(frame: &[u8], codec: Codec, expected: usize) -> Result<Vec<u8>> {
    if codec == Codec::Raw {
        if frame.len() != expected {
            return Err(bad("raw chunk length changed"));
        }
        return Ok(frame.to_vec());
    }
    // A chunk must be exactly one ordinary frame, with declared content size,
    // no dictionary, a checksum and a window of at most one chunk (the runtime's
    // limit). zstd's one-shot decode verifies the checksum.
    if frame.len() < 6
        || frame[..4] != [0x28, 0xb5, 0x2f, 0xfd]
        || frame[4] & 0x04 == 0
        || frame[4] & 0x03 != 0
    {
        return Err(bad("zstd frame header"));
    }
    if frame[4] & 0x20 == 0 {
        // Window descriptor: 2^(10 + exponent) plus mantissa eighths of that.
        let base = 1u64 << (10 + (frame[5] >> 3));
        if base + base / 8 * u64::from(frame[5] & 7) > CHUNK_SIZE {
            return Err(bad("zstd window larger than a chunk"));
        }
    }
    let declared =
        zstd::zstd_safe::get_frame_content_size(frame).map_err(|_| bad("zstd content size"))?;
    if declared != Some(expected as u64) {
        return Err(bad("zstd content size"));
    }
    let consumed =
        zstd::zstd_safe::find_frame_compressed_size(frame).map_err(|_| bad("zstd frame size"))?;
    if consumed != frame.len() {
        return Err(bad("trailing bytes after zstd frame"));
    }
    let mut decoded = vec![0u8; expected];
    let written = zstd::zstd_safe::decompress(&mut decoded[..], frame)
        .map_err(|_| bad("zstd frame or checksum"))?;
    if written != expected {
        return Err(bad("decoded chunk length"));
    }
    Ok(decoded)
}

impl<T: SourceTree + ?Sized> Unpacked<T> {
    pub fn index(&self) -> &Index {
        &self.index
    }

    fn read_packed(&mut self, file_id: usize, offset: u64, len: usize) -> Result<Vec<u8>> {
        let file = &self.index.files[file_id];
        let end = offset.saturating_add(len as u64).min(file.logical_size);
        if offset >= end {
            return Ok(Vec::new());
        }
        let first = (offset / CHUNK_SIZE) as usize;
        let last = ((end - 1) / CHUNK_SIZE) as usize + 1;
        let base = file.first_chunk as usize;
        let mut out = Vec::with_capacity((end - offset) as usize);
        let mut local = first;
        while local < last {
            let start = local;
            let first_chunk = self.index.chunks[base + start];
            let volume = first_chunk.volume_id;
            let mut stored_end = first_chunk.offset as u64;
            while local < last && local - start < BATCH_CHUNKS {
                let chunk = self.index.chunks[base + local];
                if chunk.volume_id != volume {
                    break;
                }
                if u64::from(chunk.offset) != stored_end {
                    return Err(bad("noncontiguous chunk range"));
                }
                stored_end += u64::from(chunk.stored_size);
                local += 1;
            }
            let name = volume_name(u32::from(volume));
            let stored = exact(
                &mut *self.inner,
                &name,
                first_chunk.offset as u64,
                (stored_end - u64::from(first_chunk.offset)) as usize,
            )?;
            let chunks = &self.index.chunks[base + start..base + local];
            let decoded: Vec<Vec<u8>> = chunks
                .par_iter()
                .enumerate()
                .map(|(i, chunk)| {
                    let from = (u64::from(chunk.offset) - u64::from(first_chunk.offset)) as usize;
                    let to = from + chunk.stored_size as usize;
                    let logical = (file.logical_size - (start + i) as u64 * CHUNK_SIZE)
                        .min(CHUNK_SIZE) as usize;
                    decode_chunk(&stored[from..to], chunk.codec, logical)
                        .map_err(|error| bad(format!("{} chunk {}: {error}", file.path, start + i)))
                })
                .collect::<Result<_>>()?;
            for (i, bytes) in decoded.iter().enumerate() {
                let chunk_start = (start + i) as u64 * CHUNK_SIZE;
                let from = offset.saturating_sub(chunk_start) as usize;
                let to = (end - chunk_start).min(bytes.len() as u64) as usize;
                out.extend_from_slice(&bytes[from..to]);
            }
        }
        Ok(out)
    }
}

impl<T: SourceTree + ?Sized> SourceTree for Unpacked<T> {
    fn files(&self) -> &[SourceFile] {
        &self.files
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        if let Some(&file_id) = self.packed.get(path) {
            let size = usize::try_from(self.index.files[file_id].logical_size)
                .map_err(|_| bad(format!("{path}: too large to read at once")))?;
            return self.read_packed(file_id, 0, size);
        }
        if self.hidden.contains(path) || !self.files.iter().any(|file| file.path == path) {
            return Err(bad(format!("{path} is not in the unpacked tree")));
        }
        self.inner.read(path)
    }

    fn read_range(&mut self, path: &str, offset: u64, len: usize) -> Result<Vec<u8>> {
        if let Some(&file_id) = self.packed.get(path) {
            return self.read_packed(file_id, offset, len);
        }
        if self.hidden.contains(path) || !self.files.iter().any(|file| file.path == path) {
            return Err(bad(format!("{path} is not in the unpacked tree")));
        }
        self.inner.read_range(path, offset, len)
    }

    fn empty_dirs(&self) -> &[String] {
        &self.empty_dirs
    }

    fn describe(&self) -> String {
        format!("{} with zstd packs unpacked", self.inner.describe())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_frame_window_over_one_chunk() {
        // A streamed frame without a pledged size carries a window descriptor.
        let data = vec![5u8; 1000];
        let mut encoder = zstd::stream::Encoder::new(Vec::new(), 3).unwrap();
        encoder.window_log(20).unwrap();
        encoder.include_checksum(true).unwrap();
        std::io::Write::write_all(&mut encoder, &data).unwrap();
        let frame = encoder.finish().unwrap();
        assert_eq!(frame[4] & 0x20, 0);
        let error = decode_chunk(&frame, Codec::Zstd, data.len()).unwrap_err();
        assert!(error.to_string().contains("window"), "{error}");
    }
}
