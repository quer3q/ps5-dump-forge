use super::*;
use parts::Parts;
use sevenz_rust2::{Archive, ArchiveEntry, BlockDecoder, EncoderMethod as M, Error, Password};
use std::io::{Read, Seek, SeekFrom};

struct SecretPassword(Password);
impl fmt::Debug for SecretPassword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

fn map(error: Error, supplied: bool) -> ArchiveError {
    match error {
        Error::PasswordRequired => {
            if supplied {
                ArchiveError::PasswordWrong
            } else {
                ArchiveError::PasswordRequired
            }
        }
        Error::MaybeBadPassword(_) => {
            if supplied {
                ArchiveError::PasswordWrong
            } else {
                ArchiveError::PasswordRequired
            }
        }
        Error::UnsupportedCompressionMethod(m) => {
            ArchiveError::Unsupported(format!("Unsupported 7z method: {m}."))
        }
        Error::Unsupported(_) | Error::ExternalUnsupported | Error::UnsupportedVersion { .. } => {
            ArchiveError::Unsupported("Unsupported 7z archive encoding.".into())
        }
        Error::Io(e, _) | Error::FileOpen(e, _) => ArchiveError::Io(e),
        _ => corrupt("7z archive is damaged or its checksum does not match."),
    }
}
fn entry(e: &ArchiveEntry) -> Result<Entry, ArchiveError> {
    if e.is_anti_item || e.windows_attributes & 0x400 != 0 {
        return Err(ArchiveError::Unsupported(
            "Archive links and special files are not supported.".into(),
        ));
    }
    if e.has_windows_attributes {
        special(e.windows_attributes >> 16, e.is_directory)?;
    }
    Ok(Entry {
        path: safe_path(&e.name, e.is_directory)?,
        size: e.size,
        dir: e.is_directory,
        encrypted: false,
    })
}
// ponytail: 64 MiB caps the next header, an encoded header's declared sizes and its LZMA/LZMA2/PPMd
// memory; past it a 7z is refused, though no real archive of a game comes near it.
const HEADER_LIMIT: u64 = 64 << 20;
// ponytail: 1 GiB of LZMA/LZMA2/PPMd memory per block (summed over its coders, all alive at once while
// it decodes); 7-Zip's -mx9 uses 64 MiB and larger is rare. Raise it if a real game archive needs it.
const DATA_DICT_LIMIT: u64 = 1 << 30;
// ponytail: coders per block; 7-Zip writes at most ~7 (AES on each BCJ2 stream). sevenz-rust2 nests one
// reader per coder and rescans the bind pairs for each, so a crafted million-coder chain would spin and
// then overflow the stack.
const MAX_CODERS: u64 = 16;

/// Checks the start header and an encoded header before sevenz-rust2 reads the archive: it decodes an
/// encoded header to its declared size with no memory limit, so a small crafted `.7z` could otherwise
/// expand to gigabytes before any check. Only what is clearly over is refused; anything odd is left to
/// sevenz-rust2. Also tells a byte-split set that lacks its last part.
fn check_header(source: &mut Parts, paths: &[PathBuf]) -> Result<(), ArchiveError> {
    let mut h = [0u8; 32];
    if source.len < 32 {
        return Ok(());
    }
    source.seek(SeekFrom::Start(0))?;
    source.read_exact(&mut h)?;
    if h[..6] != [b'7', b'z', 0xbc, 0xaf, 0x27, 0x1c] {
        return Ok(());
    }
    if h[8..].iter().all(|&b| b == 0) {
        // sevenz-rust2 would guess a header in the last 1 MiB and decode each guess unchecked.
        return Err(corrupt("7z archive is incomplete."));
    }
    if crc32fast::hash(&h[12..]) != u32::from_le_bytes(h[8..12].try_into().unwrap()) {
        return Ok(());
    }
    let offset = u64::from_le_bytes(h[12..20].try_into().unwrap());
    let size = u64::from_le_bytes(h[20..28].try_into().unwrap());
    let start = offset.checked_add(32);
    if start
        .and_then(|n| n.checked_add(size))
        .is_none_or(|n| n > source.len)
    {
        return Err(
            parts::missing_next(paths).unwrap_or_else(|| corrupt("7z archive is truncated."))
        );
    }
    if size > HEADER_LIMIT {
        return Err(too_large());
    }
    let mut header = vec![0u8; size as usize];
    source.seek(SeekFrom::Start(start.unwrap()))?;
    source.read_exact(&mut header)?;
    match header.split_first() {
        Some((0x17, rest)) if encoded_over(rest) == Some(true) => Err(too_large()),
        _ => Ok(()),
    }
}
fn too_large() -> ArchiveError {
    ArchiveError::Unsupported("The archive's header is too large.".into())
}
fn byte(b: &mut &[u8]) -> Option<u8> {
    let (&first, rest) = b.split_first()?;
    *b = rest;
    Some(first)
}
fn take<'a>(b: &mut &'a [u8], n: u64) -> Option<&'a [u8]> {
    let n = usize::try_from(n).ok().filter(|&n| n <= b.len())?;
    let (head, rest) = b.split_at(n);
    *b = rest;
    Some(head)
}
/// 7z's variable-length number: leading one bits in the first byte count the extra bytes.
fn number(b: &mut &[u8]) -> Option<u64> {
    let first = byte(b)?;
    let mut value = 0u64;
    for i in 0..8 {
        let mask = 0x80u8 >> i;
        if first & mask == 0 {
            return Some(value | (u64::from(first & (mask - 1)) << (8 * i)));
        }
        value |= u64::from(byte(b)?) << (8 * i);
    }
    Some(value)
}
/// The encoded header's StreamsInfo (after its 0x17): `Some(true)` when an unpack size or a coder's
/// dictionary is over the limit, `None` for a structure sevenz-rust2 should judge.
fn encoded_over(mut b: &[u8]) -> Option<bool> {
    let b = &mut b;
    let mut nid = byte(b)?;
    if nid == 0x06 {
        // PackInfo: position, count, then optional sizes and CRCs.
        number(b)?;
        let count = number(b)?;
        nid = byte(b)?;
        if nid == 0x09 {
            for _ in 0..count {
                number(b)?;
            }
            nid = byte(b)?;
        }
        if nid == 0x0a {
            let defined = if byte(b)? != 0 {
                count
            } else {
                let bits = take(b, count.div_ceil(8))?;
                (0..count)
                    .filter(|i| bits[(i / 8) as usize] & (0x80 >> (i % 8)) != 0)
                    .count() as u64
            };
            take(b, defined.checked_mul(4)?)?;
            nid = byte(b)?;
        }
        if nid != 0 {
            return None;
        }
        nid = byte(b)?;
    }
    if nid != 0x07 || byte(b)? != 0x0b {
        return None;
    }
    let blocks = number(b)?;
    if byte(b)? != 0 {
        return None;
    }
    let mut outs = Vec::new();
    for _ in 0..blocks {
        let (mut ins, mut out) = (0u64, 0u64);
        let coders = number(b)?;
        if coders > MAX_CODERS {
            return Some(true);
        }
        for _ in 0..coders {
            let flags = byte(b)?;
            let id = take(b, u64::from(flags & 0x0f))?;
            if flags & 0x10 == 0 {
                ins = ins.checked_add(1)?;
                out = out.checked_add(1)?;
            } else {
                ins = ins.checked_add(number(b)?)?;
                out = out.checked_add(number(b)?)?;
            }
            if flags & 0x20 != 0 {
                let n = number(b)?;
                let props = take(b, n)?;
                if dictionary(id, props).is_some_and(|d| d > HEADER_LIMIT) {
                    return Some(true);
                }
            }
            if flags & 0x80 != 0 {
                return None;
            }
        }
        let pairs = out.checked_sub(1)?;
        for _ in 0..pairs {
            number(b)?;
            number(b)?;
        }
        let packed = ins.checked_sub(pairs)?;
        if packed > 1 {
            for _ in 0..packed {
                number(b)?;
            }
        }
        outs.push(out);
    }
    if byte(b)? != 0x0c {
        return None;
    }
    for out in outs {
        for _ in 0..out {
            if number(b)? > HEADER_LIMIT {
                return Some(true);
            }
        }
    }
    Some(false)
}
/// The memory a coder asks for, as sevenz-rust2 reads its properties.
fn dictionary(id: &[u8], props: &[u8]) -> Option<u64> {
    match id {
        // LZMA and PPMd: a little-endian u32 after the first byte.
        [0x03, 0x01, 0x01] | [0x03, 0x04, 0x01] => props
            .get(1..5)
            .map(|d| u64::from(u32::from_le_bytes(d.try_into().unwrap()))),
        [0x21] => match *props.first()? {
            bits @ 0..40 => Some(u64::from(2 | (bits & 1)) << (bits / 2 + 11)),
            40 => Some(u64::from(u32::MAX)),
            _ => None,
        },
        _ => None,
    }
}
/// What one block's LZMA2 decode workers may hold together, so a job stays inside 512 MiB.
const DECODE_BUDGET: u64 = 256 << 20;
/// Larger packed blocks are not walked for [`largest_run`]: they decode on one thread.
// ponytail: the walk reads the block once before it decodes, so past this a solid block decodes
// on one thread (on a slow drive the extra read costs what the threads save). Upgrade: walk
// while decoding, from the reader's own chunk headers, if large solid blocks matter.
const WALK_MAX: u64 = 1 << 30;

/// Decode threads for block `b`. sevenz-rust2 decodes LZMA2 on several threads, one run of
/// chunks between dictionary resets per worker; with patch 0021 at most two runs per worker
/// are out, each decoded into a buffer that may have doubled past it, plus the input and the
/// run being served: a dictionary and six runs per worker. So the count follows the block's largest
/// run, found by walking its chunk headers just before it decodes (the pages stay cached for
/// the decoder). One thread unless the block is a lone LZMA2 coder, whose pack stream is then
/// the LZMA2 stream, starting at `start` (a filter's place in a chain isn't public, and every
/// LZMA2 coder of a chain would get the threads).
fn decode_threads(
    archive: &Archive,
    b: usize,
    start: Option<u64>,
    source: &mut Parts,
    cancel: &AtomicBool,
) -> u32 {
    let [lzma2] = archive.blocks[b].coders.as_slice() else {
        return 1;
    };
    let packed = archive.pack_sizes().get(b).copied().unwrap_or(u64::MAX);
    let (true, Some(start), Some(dict)) = (
        lzma2.encoder_method_id() == M::ID_LZMA2 && packed <= WALK_MAX,
        start,
        dictionary(M::ID_LZMA2, lzma2.properties()),
    ) else {
        return 1;
    };
    // An empty stream stays on one thread: sevenz-rust2's threaded reader waits forever on one.
    let Some(run) = largest_run(source, start, packed, cancel).filter(|&run| run > 0) else {
        return 1;
    };
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get()) as u64;
    let per_worker = dict.saturating_add(run.saturating_mul(6)).max(1);
    (DECODE_BUDGET / per_worker).clamp(1, cores) as u32
}

/// The most bytes an LZMA2 stream of `len` bytes at `at` unpacks between dictionary resets
/// (where sevenz-rust2's decoder splits work); `None` if it doesn't parse, or on cancel.
fn largest_run(
    source: &mut (impl Read + Seek),
    at: u64,
    len: u64,
    cancel: &AtomicBool,
) -> Option<u64> {
    source.seek(SeekFrom::Start(at)).ok()?;
    let (mut pos, mut run, mut most, mut chunks) = (0u64, 0u64, 0u64, 0u64);
    let mut head = [0u8; 6];
    while pos < len {
        chunks += 1;
        if chunks.is_multiple_of(4096) && check(cancel).is_err() {
            return None;
        }
        source.read_exact(&mut head[..1]).ok()?;
        let control = head[0];
        let (more, unpacked, packed, reset) = match control {
            0 => break,
            1 | 2 => {
                source.read_exact(&mut head[1..3]).ok()?;
                let n = u64::from(u16::from_be_bytes([head[1], head[2]])) + 1;
                (2, n, n, control == 1)
            }
            0x80.. => {
                let more = if control >= 0xC0 { 5 } else { 4 };
                source.read_exact(&mut head[1..=more]).ok()?;
                let low = u64::from(u16::from_be_bytes([head[1], head[2]]));
                let unpacked = (u64::from(control & 0x1F) << 16 | low) + 1;
                let packed = u64::from(u16::from_be_bytes([head[3], head[4]])) + 1;
                (more, unpacked, packed, control >= 0xE0)
            }
            _ => return None,
        };
        if reset {
            run = 0;
        }
        run += unpacked;
        most = most.max(run);
        source.seek(SeekFrom::Current(packed as i64)).ok()?;
        pos += 1 + more as u64 + packed;
    }
    Some(most)
}

fn open(
    paths: &[PathBuf],
    password: Option<&str>,
) -> Result<(Parts, Archive, SecretPassword, bool), ArchiveError> {
    let mut source = Parts::open(paths)?;
    check_header(&mut source, paths)?;
    source.seek(SeekFrom::Start(0))?;
    let pw = SecretPassword(Password::from(password.unwrap_or("")));
    let archive = Archive::read(&mut source, &pw.0).map_err(|e| map(e, password.is_some()))?;
    let mut encrypted = false;
    // Every block is checked before anything decodes: sevenz-rust2 gives file data no memory limit.
    // A multi-input coder other than BCJ2 is refused by sevenz-rust2 itself before it allocates.
    for block in &archive.blocks {
        if block.coders.len() as u64 > MAX_CODERS {
            return Err(ArchiveError::Unsupported(format!(
                "Unsupported 7z method chain: {} coders in one block.",
                block.coders.len()
            )));
        }
        let mut memory = 0u64;
        for coder in &block.coders {
            let id = coder.encoder_method_id();
            encrypted |= id == M::ID_AES256_SHA256;
            if ![
                M::ID_COPY,
                M::ID_LZMA,
                M::ID_LZMA2,
                M::ID_PPMD,
                M::ID_BZIP2,
                M::ID_AES256_SHA256,
                M::ID_DELTA,
                M::ID_BCJ_X86,
                M::ID_BCJ2,
                M::ID_BCJ_PPC,
                M::ID_BCJ_IA64,
                M::ID_BCJ_ARM,
                M::ID_BCJ_ARM64,
                M::ID_BCJ_ARM_THUMB,
                M::ID_BCJ_SPARC,
                M::ID_BCJ_RISCV,
            ]
            .contains(&id)
            {
                let method = M::by_id(id)
                    .map(|m| m.name().to_owned())
                    .unwrap_or_else(|| format!("{id:02x?}"));
                return Err(ArchiveError::Unsupported(format!(
                    "Unsupported 7z method: {method}."
                )));
            }
            memory = memory.saturating_add(dictionary(id, coder.properties()).unwrap_or(0));
        }
        if memory > DATA_DICT_LIMIT {
            return Err(ArchiveError::Unsupported(format!(
                "The archive needs a {} MiB decompression dictionary (over 1 GiB); re-pack it with a smaller one.",
                memory.div_ceil(1 << 20)
            )));
        }
    }
    if encrypted && password.is_none() {
        return Err(ArchiveError::PasswordRequired);
    }
    Ok((source, archive, pw, encrypted))
}
pub(super) fn list(paths: &[PathBuf], password: Option<&str>) -> Result<Listing, ArchiveError> {
    let (_, archive, _, encrypted) = open(paths, password)?;
    // A file's data is encrypted when its block has an AES coder.
    let aes = |b: usize| {
        (archive.blocks[b].coders.iter()).any(|c| c.encoder_method_id() == M::ID_AES256_SHA256)
    };
    let entries = (archive.files.iter().enumerate())
        .map(|(i, e)| {
            let block = archive.stream_map.file_block_index[i];
            Ok(Entry {
                encrypted: block.is_some_and(aes),
                ..entry(e)?
            })
        })
        .collect::<Result<Vec<_>, ArchiveError>>()?;
    Ok(Listing {
        format: ArchiveFormat::SevenZ,
        entries,
        volumes: paths.to_vec(),
        archive_bytes: 0,
        solid: archive.is_solid,
        encrypted,
    })
}
pub(super) fn walk(
    paths: &[PathBuf],
    password: Option<&str>,
    sink: &mut dyn Sink,
    cancel: &AtomicBool,
) -> Result<(), ArchiveError> {
    let (mut source, archive, pw, encrypted) = open(paths, password)?;
    let mut streams = vec![0usize; archive.blocks.len()];
    for (f, block) in archive
        .files
        .iter()
        .zip(&archive.stream_map.file_block_index)
    {
        if let Some(b) = block.filter(|_| f.has_stream) {
            streams[b] += 1;
        }
    }
    // Where each block's pack stream starts, when each block has exactly one (no BCJ2).
    let one_each = archive.pack_sizes().len() == archive.blocks.len()
        && !(archive.blocks.iter())
            .any(|b| b.coders.iter().any(|c| c.encoder_method_id() == M::ID_BCJ2));
    let starts: Vec<u64> = archive
        .pack_sizes()
        .iter()
        .scan(
            sevenz_rust2::SIGNATURE_HEADER_SIZE + archive.pack_pos(),
            |at, &n| {
                let start = *at;
                *at = at.saturating_add(n);
                Some(start)
            },
        )
        .collect();
    for (i, &expected) in streams.iter().enumerate() {
        check(cancel)?;
        if sink.done() {
            return Ok(());
        }
        let decoder = BlockDecoder::new(1, i, &archive, &pw.0, &mut source);
        // sevenz-rust2 walks a block's first `num_unpack_sub_streams` entries, so an empty file
        // listed between two of its streams pushes the last stream out of the walk (and out of
        // `entries`): refuse that rather than leave a file out.
        if decoder.entries().iter().filter(|e| e.has_stream).count() != expected {
            return Err(ArchiveError::Unsupported(
                "This 7z lists an empty file between packed ones, which can't be unpacked; \
                 re-pack it with 7-Zip."
                    .into(),
            ));
        }
        if !decoder.entries().iter().any(|e| sink.wants(&e.name)) {
            continue;
        }
        // The check's one-thread decoder ends here (NLL), freeing `source` for the walk.
        let start = starts.get(i).copied().filter(|_| one_each);
        let threads = if sink.streams() {
            decode_threads(&archive, i, start, &mut source, cancel)
        } else {
            1
        };
        check(cancel)?;
        let decoder = BlockDecoder::new(threads, i, &archive, &pw.0, &mut source);
        let mut failure = None;
        let result = decoder.for_each_entries(&mut |e, reader| {
            let result = (|| {
                check(cancel)?;
                let e = entry(e)?;
                let selected = sink.begin(&e)?;
                let mut buf = [0u8; 64 * 1024];
                let mut size = 0u64;
                loop {
                    check(cancel)?;
                    let n = reader.read(&mut buf).map_err(|error| {
                        // A wrong key decodes garbage; an OS error (a drive gone) is still I/O.
                        if encrypted && error.raw_os_error().is_none() {
                            ArchiveError::PasswordWrong
                        } else {
                            ArchiveError::Io(error)
                        }
                    })?;
                    if n == 0 {
                        break;
                    }
                    size += n as u64;
                    if selected {
                        sink.chunk(&buf[..n])?;
                    }
                }
                if size != e.size {
                    return Err(corrupt("7z entry size does not match."));
                }
                if selected {
                    sink.end()?;
                }
                Ok(!sink.done())
            })();
            match result {
                Ok(more) => Ok(more),
                Err(e) => {
                    failure = Some(e);
                    Err(Error::Other("Archive operation stopped.".into()))
                }
            }
        });
        if let Some(e) = failure {
            return Err(e);
        }
        result.map_err(|e| map(e, password.is_some()))?;
    }
    for (i, e) in archive.files.iter().enumerate() {
        check(cancel)?;
        if sink.done() {
            break;
        }
        if archive.stream_map.file_block_index[i].is_none() && sink.begin(&entry(e)?)? {
            sink.end()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn threaded_lzma2_decodes_the_runs_largest_run_measures() {
        use lzma_rust2::{Lzma2Options, Lzma2ReaderMt, Lzma2WriterMt};
        use std::io::Write;
        let data: Vec<u8> = (0..3u32 << 20)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8 & 0x3F)
            .collect();
        let mut options = Lzma2Options::with_preset(1);
        options.lzma_options.dict_size = 64 << 10;
        options.chunk_size = std::num::NonZeroU64::new(256 << 10);
        let mut writer = Lzma2WriterMt::new(Vec::new(), options, 4).unwrap();
        writer.write_all(&data).unwrap();
        let packed = writer.finish().unwrap();
        let go = AtomicBool::new(false);
        let mut cursor = std::io::Cursor::new(&packed);
        assert_eq!(
            largest_run(&mut cursor, 0, packed.len() as u64, &go),
            Some(256 << 10)
        );
        let mut out = Vec::new();
        Lzma2ReaderMt::new(&packed[..], 64 << 10, None, 4)
            .read_to_end(&mut out)
            .unwrap();
        assert!(out == data, "threaded decode differs");
        // Dropped part way, the reader waits for its workers (patch 0021) without hanging.
        let (done, finished) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = Lzma2ReaderMt::new(&packed[..], 64 << 10, None, 4);
            let mut first = [0u8; 4096];
            reader.read_exact(&mut first).unwrap();
            drop(reader);
            done.send(()).unwrap();
        });
        finished
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("dropping a threaded reader returns");
    }

    #[test]
    fn memory_limits() {
        // An encoded header of one LZMA2 coder: property `dict`, unpack size `size` (7z number).
        let encoded = |dict: u8, size: &[u8]| {
            let h = [
                0x06, 0, 1, 0x09, 0, 0, 0x07, 0x0b, 1, 0, 1, 0x21, 0x21, 1, dict, 0x0c,
            ];
            [&h[..], size, &[0, 0]].concat()
        };
        // 4 GiB declared, a 4 GiB dictionary, then within the limits.
        assert_eq!(encoded_over(&encoded(16, &[0xf1, 0, 0, 0, 0])), Some(true));
        assert_eq!(encoded_over(&encoded(40, &[100])), Some(true));
        assert_eq!(encoded_over(&encoded(16, &[100])), Some(false));
        // 17 coders: over MAX_CODERS before any is read.
        let chain = [0x06, 0, 1, 0x09, 16, 0, 0x07, 0x0b, 1, 0, 17];
        assert_eq!(encoded_over(&chain), Some(true));
        assert_eq!(dictionary(&[0x21], &[36]), Some(1 << 30));
        let lzma = [0x5d, 0, 0, 0, 0x80];
        assert_eq!(dictionary(&[3, 1, 1], &lzma), Some(2048 << 20));
    }
}
