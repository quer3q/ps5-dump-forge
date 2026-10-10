//! The LZ4 traces zip (`*-amprtrace.zip`), both ways, so writer and reader share one layout.
//!
//! Writing: one pass, STORED entries (no compression), each local header with flag bit 3 (its
//! CRC-32 and sizes follow the data in a data descriptor, the CRC computed while the bytes go
//! out), then the central directory and the end record. Every length is known up front
//! ([`length`]), so a download carries an exact `Content-Length`. ZIP64 fields only where a size
//! or offset reaches 0xFFFFFFFF (a folder's journal can pass 4 GiB). ASCII names only.
//!
//! Reading ([`directory`], [`data_at`]): the central directory (ZIP64 included) of any zip, then
//! a STORED entry's byte range; every offset and size is checked against the file's length.

use std::io::{self, Read, Seek, SeekFrom, Write};
use std::time::SystemTime;

/// From here on a size or offset takes its ZIP64 field.
const LIMIT: u64 = 0xFFFF_FFFF;
const STORED: u16 = 0;
/// Bit 3: the CRC and sizes are in the data descriptor.
const FLAGS: u16 = 1 << 3;
/// The largest piece of an entry read and written at once.
const CHUNK: usize = 1 << 20;
const LOCAL: u32 = 0x0403_4b50;
const DESCRIPTOR: u32 = 0x0807_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const END: u32 = 0x0605_4b50;
const END64: u32 = 0x0606_4b50;
const LOCATOR: u32 = 0x0706_4b50;
const ZIP64_EXTRA: u16 = 0x0001;

pub(crate) struct Entry {
    pub name: &'static str,
    pub size: u64,
}

/// The archive's byte count, for `Content-Length`.
pub(crate) fn length(entries: &[Entry]) -> u64 {
    layout(entries, LIMIT).total
}

/// Writes the archive: each entry's bytes come from `read(i, buf)` (0 only at its end; exactly
/// `size` bytes in all, else an error). The time is now, as DOS time (UTC).
pub(crate) fn write(
    out: &mut dyn Write,
    entries: &[Entry],
    read: &mut dyn FnMut(usize, &mut [u8]) -> io::Result<usize>,
) -> io::Result<()> {
    write_with(out, entries, read, dos_time(SystemTime::now()), LIMIT)
}

/// Where each local header starts, and the central directory.
struct Layout {
    offsets: Vec<u64>,
    cd_offset: u64,
    cd_size: u64,
    total: u64,
}

fn layout(entries: &[Entry], limit: u64) -> Layout {
    let mut at = 0u64;
    let mut offsets = Vec::new();
    for e in entries {
        offsets.push(at);
        at += local_header(e, (0, 0), limit).len() as u64 + e.size;
        at += descriptor(e, 0, limit).len() as u64;
    }
    let cd_size: u64 = entries
        .iter()
        .zip(&offsets)
        .map(|(e, &off)| central_entry(e, 0, off, (0, 0), limit).len() as u64)
        .sum();
    let total = at + cd_size + end_records(entries.len(), at, cd_size, limit).len() as u64;
    Layout {
        offsets,
        cd_offset: at,
        cd_size,
        total,
    }
}

fn write_with(
    out: &mut dyn Write,
    entries: &[Entry],
    read: &mut dyn FnMut(usize, &mut [u8]) -> io::Result<usize>,
    time: (u16, u16),
    limit: u64,
) -> io::Result<()> {
    let plan = layout(entries, limit);
    let biggest = entries.iter().map(|e| e.size).max().unwrap_or(0);
    let mut buf = vec![0u8; CHUNK.min(usize::try_from(biggest).unwrap_or(usize::MAX))];
    let mut crcs = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        out.write_all(&local_header(e, time, limit))?;
        let mut crc = crc32fast::Hasher::new();
        let mut left = e.size;
        while left > 0 {
            let want = buf.len().min(usize::try_from(left).unwrap_or(usize::MAX));
            let n = match read(i, &mut buf[..want])? {
                0 => return Err(io::ErrorKind::UnexpectedEof.into()),
                n => n.min(want),
            };
            crc.update(&buf[..n]);
            out.write_all(&buf[..n])?;
            left -= n as u64;
        }
        let crc = crc.finalize();
        out.write_all(&descriptor(e, crc, limit))?;
        crcs.push(crc);
    }
    for (i, e) in entries.iter().enumerate() {
        out.write_all(&central_entry(e, crcs[i], plan.offsets[i], time, limit))?;
    }
    out.write_all(&end_records(
        entries.len(),
        plan.cd_offset,
        plan.cd_size,
        limit,
    ))
}

fn u16le(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn u32le(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn u64le(v: &mut Vec<u8>, x: u64) {
    v.extend_from_slice(&x.to_le_bytes());
}

/// A 32-bit field, or 0xFFFFFFFF when the value is in a ZIP64 field.
fn small(x: u64, limit: u64) -> u32 {
    if x >= limit { u32::MAX } else { x as u32 }
}

/// 4.5 for ZIP64, else 2.0.
fn version(zip64: bool) -> u16 {
    if zip64 { 45 } else { 20 }
}

/// With a ZIP64 extra field (sizes placeholder 0xFFFFFFFF, values 0: they follow the data), the
/// descriptor's sizes are 8 bytes each.
fn local_header(e: &Entry, (time, date): (u16, u16), limit: u64) -> Vec<u8> {
    let zip64 = e.size >= limit;
    let mut v = Vec::with_capacity(30 + e.name.len() + 20);
    u32le(&mut v, LOCAL);
    u16le(&mut v, version(zip64));
    u16le(&mut v, FLAGS);
    u16le(&mut v, STORED);
    u16le(&mut v, time);
    u16le(&mut v, date);
    u32le(&mut v, 0); // CRC-32: in the descriptor
    let size = if zip64 { u32::MAX } else { 0 };
    u32le(&mut v, size);
    u32le(&mut v, size);
    u16le(&mut v, e.name.len() as u16);
    u16le(&mut v, if zip64 { 20 } else { 0 });
    v.extend_from_slice(e.name.as_bytes());
    if zip64 {
        u16le(&mut v, ZIP64_EXTRA);
        u16le(&mut v, 16);
        u64le(&mut v, 0);
        u64le(&mut v, 0);
    }
    v
}

fn descriptor(e: &Entry, crc: u32, limit: u64) -> Vec<u8> {
    let mut v = Vec::with_capacity(24);
    u32le(&mut v, DESCRIPTOR);
    u32le(&mut v, crc);
    if e.size >= limit {
        u64le(&mut v, e.size);
        u64le(&mut v, e.size);
    } else {
        u32le(&mut v, e.size as u32);
        u32le(&mut v, e.size as u32);
    }
    v
}

fn central_entry(
    e: &Entry,
    crc: u32,
    offset: u64,
    (time, date): (u16, u16),
    limit: u64,
) -> Vec<u8> {
    let (big_size, big_offset) = (e.size >= limit, offset >= limit);
    let mut extra = Vec::new();
    if big_size {
        u64le(&mut extra, e.size); // uncompressed, then compressed: STORED, the same
        u64le(&mut extra, e.size);
    }
    if big_offset {
        u64le(&mut extra, offset);
    }
    let zip64 = !extra.is_empty();
    let mut v = Vec::with_capacity(46 + e.name.len() + 28);
    u32le(&mut v, CENTRAL);
    u16le(&mut v, version(zip64)); // made by: MS-DOS attributes
    u16le(&mut v, version(zip64));
    u16le(&mut v, FLAGS);
    u16le(&mut v, STORED);
    u16le(&mut v, time);
    u16le(&mut v, date);
    u32le(&mut v, crc);
    u32le(&mut v, small(e.size, limit));
    u32le(&mut v, small(e.size, limit));
    u16le(&mut v, e.name.len() as u16);
    u16le(&mut v, if zip64 { 4 + extra.len() as u16 } else { 0 });
    u16le(&mut v, 0); // comment
    u16le(&mut v, 0); // disk
    u16le(&mut v, 0); // internal attributes
    u32le(&mut v, 0); // external attributes
    u32le(&mut v, small(offset, limit));
    v.extend_from_slice(e.name.as_bytes());
    if zip64 {
        u16le(&mut v, ZIP64_EXTRA);
        u16le(&mut v, extra.len() as u16);
        v.extend_from_slice(&extra);
    }
    v
}

/// The end of central directory record, after a ZIP64 end record and its locator when the
/// directory starts or ends at or past the limit.
fn end_records(count: usize, cd_offset: u64, cd_size: u64, limit: u64) -> Vec<u8> {
    let zip64 = cd_offset >= limit || cd_size >= limit;
    let mut v = Vec::with_capacity(98);
    if zip64 {
        let at = cd_offset + cd_size;
        u32le(&mut v, END64);
        u64le(&mut v, 44); // the record's size after this field
        u16le(&mut v, 45);
        u16le(&mut v, 45);
        u32le(&mut v, 0); // this disk
        u32le(&mut v, 0); // the directory's disk
        u64le(&mut v, count as u64);
        u64le(&mut v, count as u64);
        u64le(&mut v, cd_size);
        u64le(&mut v, cd_offset);
        u32le(&mut v, LOCATOR);
        u32le(&mut v, 0); // the ZIP64 end record's disk
        u64le(&mut v, at);
        u32le(&mut v, 1); // disks
    }
    u32le(&mut v, END);
    u16le(&mut v, 0);
    u16le(&mut v, 0);
    u16le(&mut v, count as u16);
    u16le(&mut v, count as u16);
    u32le(&mut v, small(cd_size, if zip64 { 0 } else { limit }));
    u32le(&mut v, small(cd_offset, if zip64 { 0 } else { limit }));
    u16le(&mut v, 0); // comment
    v
}

/// One central directory record, as far as reading a STORED entry needs.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Found {
    pub name: Vec<u8>,
    pub flags: u16,
    pub method: u16,
    pub crc: u32,
    pub compressed: u64,
    pub size: u64,
    pub offset: u64,
}

/// The most central directory read (a traces zip's is about 150 bytes).
const MAX_DIRECTORY: u64 = 16 << 20;

fn get<const N: usize>(b: &[u8], at: usize) -> Option<[u8; N]> {
    b.get(at..at.checked_add(N)?)?.try_into().ok()
}

fn get16(b: &[u8], at: usize) -> Option<u16> {
    get(b, at).map(u16::from_le_bytes)
}

fn get32(b: &[u8], at: usize) -> Option<u32> {
    get(b, at).map(u32::from_le_bytes)
}

fn get64(b: &[u8], at: usize) -> Option<u64> {
    get(b, at).map(u64::from_le_bytes)
}

/// `n` bytes at `at`, all inside a file of `len` bytes.
fn read_at<F: Read + Seek>(f: &mut F, len: u64, at: u64, n: u64) -> Result<Vec<u8>, String> {
    if at.checked_add(n).is_none_or(|end| end > len) {
        return Err(format!("{n} bytes at {at} pass the end ({len} bytes)"));
    }
    let mut buf = vec![0u8; n as usize];
    f.seek(SeekFrom::Start(at))
        .and_then(|_| f.read_exact(&mut buf))
        .map_err(|e| format!("reading {n} bytes at {at}: {e}"))?;
    Ok(buf)
}

/// Every record of the central directory of a zip `len` bytes long (ZIP64 included).
pub(crate) fn directory<F: Read + Seek>(f: &mut F, len: u64) -> Result<Vec<Found>, String> {
    let tail_len = len.min(22 + 0xFFFF);
    let tail = read_at(f, len, len - tail_len, tail_len)?;
    let at = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&i| get32(&tail, i) == Some(END))
        .ok_or("no end of central directory record (not a zip, or cut off)")?;
    let end = len - tail_len + at as u64;
    let short = || "the end record is cut off".to_string();
    let mut count = u64::from(get16(&tail, at + 10).ok_or_else(short)?);
    let mut cd_size = u64::from(get32(&tail, at + 12).ok_or_else(short)?);
    let mut cd_offset = u64::from(get32(&tail, at + 16).ok_or_else(short)?);
    let mut cd_end = end;
    if end >= 20 {
        let locator = read_at(f, len, end - 20, 20)?;
        if get32(&locator, 0) == Some(LOCATOR) {
            let at64 = get64(&locator, 8).ok_or_else(short)?;
            if at64.checked_add(56).is_none_or(|e| e > end - 20) {
                return Err("the ZIP64 end record is out of place".into());
            }
            let rec = read_at(f, len, at64, 56)?;
            if get32(&rec, 0) != Some(END64) {
                return Err("no ZIP64 end record where its locator points".into());
            }
            count = get64(&rec, 32).ok_or_else(short)?;
            cd_size = get64(&rec, 40).ok_or_else(short)?;
            cd_offset = get64(&rec, 48).ok_or_else(short)?;
            cd_end = at64;
        }
    }
    if cd_size > MAX_DIRECTORY || cd_offset.checked_add(cd_size).is_none_or(|e| e > cd_end) {
        return Err(format!(
            "the central directory ({cd_size} bytes at {cd_offset}) is out of place"
        ));
    }
    let cd = read_at(f, len, cd_offset, cd_size)?;
    let bad = |what: &str| format!("central directory record {what}");
    let mut found = Vec::new();
    let mut at = 0usize;
    for _ in 0..count.min(cd_size / 46) {
        if get32(&cd, at) != Some(CENTRAL) {
            return Err(bad("missing"));
        }
        let field = |off: usize| get32(&cd, at + off).map(u64::from);
        let (Some(flags), Some(method), Some(crc)) =
            (get16(&cd, at + 8), get16(&cd, at + 10), get32(&cd, at + 16))
        else {
            return Err(bad("cut off"));
        };
        let (Some(mut compressed), Some(mut size), Some(mut offset)) =
            (field(20), field(24), field(42))
        else {
            return Err(bad("cut off"));
        };
        let sizes = |off| get16(&cd, at + off).map(usize::from);
        let (Some(name_len), Some(extra_len), Some(comment_len)) =
            (sizes(28), sizes(30), sizes(32))
        else {
            return Err(bad("cut off"));
        };
        let name = cd
            .get(at + 46..at + 46 + name_len)
            .ok_or_else(|| bad("cut off"))?
            .to_vec();
        let extra = cd
            .get(at + 46 + name_len..at + 46 + name_len + extra_len)
            .ok_or_else(|| bad("cut off"))?;
        // ZIP64: the 8-byte values of the fields set to 0xFFFFFFFF, in this order.
        let mut e = 0usize;
        while let (Some(id), Some(n)) = (get16(extra, e), get16(extra, e + 2)) {
            let data = extra
                .get(e + 4..e + 4 + usize::from(n))
                .ok_or_else(|| bad("extra cut off"))?;
            if id == ZIP64_EXTRA {
                let mut values = data
                    .as_chunks::<8>()
                    .0
                    .iter()
                    .map(|c| u64::from_le_bytes(*c));
                for v in [&mut size, &mut compressed, &mut offset] {
                    if *v == LIMIT {
                        *v = values.next().ok_or_else(|| bad("ZIP64 field missing"))?;
                    }
                }
            }
            e += 4 + usize::from(n);
        }
        found.push(Found {
            name,
            flags,
            method,
            crc,
            compressed,
            size,
            offset,
        });
        at += 46 + name_len + extra_len + comment_len;
    }
    if (found.len() as u64) < count {
        return Err(format!("{count} entries announced, {} found", found.len()));
    }
    Ok(found)
}

/// Where a STORED entry's bytes start: past its local header's name and extra field, the whole
/// range inside the file.
pub(crate) fn data_at<F: Read + Seek>(f: &mut F, len: u64, e: &Found) -> Result<u64, String> {
    let name = String::from_utf8_lossy(&e.name);
    if e.method != STORED || e.flags & 1 != 0 {
        return Err(format!(
            "{name} is compressed or encrypted (method {}): unzip it and pick the folder",
            e.method
        ));
    }
    if e.compressed != e.size {
        return Err(format!("{name}: stored with two different sizes"));
    }
    let header = read_at(f, len, e.offset, 30)?;
    if get32(&header, 0) != Some(LOCAL) {
        return Err(format!("{name}: no local header at {}", e.offset));
    }
    let skip = 30
        + u64::from(get16(&header, 26).unwrap_or(0))
        + u64::from(get16(&header, 28).unwrap_or(0));
    let data = e.offset + skip;
    if data.checked_add(e.size).is_none_or(|end| end > len) {
        return Err(format!(
            "{name}: its {} bytes pass the end of the zip",
            e.size
        ));
    }
    Ok(data)
}

/// An entry's bytes as a stream that checks its CRC-32 at the end: a mismatch, or an early end,
/// is an error.
pub(crate) struct Checked<R> {
    inner: R,
    name: String,
    left: u64,
    want: u32,
    crc: crc32fast::Hasher,
}

impl<R: Read> Checked<R> {
    pub(crate) fn new(inner: R, name: &str, size: u64, crc: u32) -> Self {
        Self {
            inner,
            name: name.to_string(),
            left: size,
            want: crc,
            crc: crc32fast::Hasher::new(),
        }
    }
}

impl<R: Read> Read for Checked<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 {
            return Ok(0);
        }
        let want = buf
            .len()
            .min(usize::try_from(self.left).unwrap_or(usize::MAX));
        let n = self.inner.read(&mut buf[..want])?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("the traces zip is damaged: {} ends early", self.name),
            ));
        }
        self.crc.update(&buf[..n]);
        self.left -= n as u64;
        if self.left == 0 && self.crc.clone().finalize() != self.want {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("the traces zip is damaged: {} fails its CRC-32", self.name),
            ));
        }
        Ok(n)
    }
}

/// (time, date) in DOS form, UTC; 1980-01-01 for anything earlier.
fn dos_time(now: SystemTime) -> (u16, u16) {
    let secs = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
        .max(315_532_800); // 1980-01-01
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Days to a civil date (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let time = ((rem / 3600) << 11) | ((rem % 3600 / 60) << 5) | ((rem % 60) / 2);
    let date = ((year - 1980).clamp(0, 127) << 9 | month << 5 | day) as u16;
    (time as u16, date)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn archive(files: &[(&'static str, &[u8])], limit: u64) -> Vec<u8> {
        let entries: Vec<Entry> = files
            .iter()
            .map(|(name, b)| Entry {
                name,
                size: b.len() as u64,
            })
            .collect();
        let mut pos = vec![0usize; files.len()];
        let mut read = |i: usize, buf: &mut [u8]| {
            let data = &files[i].1[pos[i]..];
            let n = data.len().min(buf.len()).min(3); // small pieces
            buf[..n].copy_from_slice(&data[..n]);
            pos[i] += n;
            Ok(n)
        };
        let mut out = Vec::new();
        write_with(&mut out, &entries, &mut read, (0x6000, 0x5949), limit).unwrap();
        assert_eq!(
            out.len() as u64,
            layout(&entries, limit).total,
            "the planned length"
        );
        out
    }

    /// Python's zipfile (when installed) reads the archive, checks every CRC and gets the
    /// bytes back; macOS also checks it with `unzip -t` and `ditto`.
    fn independent_check(zip: &[u8], files: &[(&str, &[u8])], tag: &str) {
        let dir = std::env::temp_dir().join(format!("forge-zip-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.zip");
        std::fs::write(&path, zip).unwrap();
        let script = "import zipfile,sys,json\n\
            z=zipfile.ZipFile(sys.argv[1]); assert z.testzip() is None\n\
            print(json.dumps({i.filename: z.read(i).hex() for i in z.infolist()}))";
        match std::process::Command::new("python3")
            .args(["-c", script])
            .arg(&path)
            .output()
        {
            Ok(out) if out.status.success() => {
                let got: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
                for (name, bytes) in files {
                    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
                    assert_eq!(got[name], hex, "{name}");
                }
                assert_eq!(got.as_object().unwrap().len(), files.len());
            }
            Ok(out) => panic!("python3 zipfile: {}", String::from_utf8_lossy(&out.stderr)),
            Err(e) => eprintln!("python3 not run ({e}); the archive is checked by layout only"),
        }
        if cfg!(target_os = "macos") {
            let ok = |cmd: &mut std::process::Command| cmd.output().unwrap().status.success();
            assert!(ok(std::process::Command::new("unzip")
                .arg("-tq")
                .arg(&path)));
            let x = dir.join("x");
            assert!(ok(std::process::Command::new("ditto")
                .args(["-x", "-k"])
                .arg(&path)
                .arg(&x)));
            for (name, bytes) in files {
                assert_eq!(std::fs::read(x.join(name)).unwrap(), *bytes, "ditto {name}");
            }
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_plain_archive_reads_back() {
        let journal: Vec<u8> = (0..100_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let files: [(&'static str, &[u8]); 2] =
            [("ampr_commands.bin", &journal), ("ampr_emu.index", b"idx")];
        let zip = archive(&files, LIMIT);
        // 2 × (30 + name) headers, 2 × 16 descriptors, 2 × (46 + name), 22.
        let names = 17 + 14;
        assert_eq!(
            zip.len(),
            2 * 30 + names + 100_003 + 32 + 2 * 46 + names + 22
        );
        assert!(
            !zip.windows(4).any(|w| w == [0x50, 0x4b, 6, 6]),
            "no ZIP64 end"
        );
        independent_check(&zip, &files, "plain");
        // An empty entry too.
        let files: [(&'static str, &[u8]); 2] = [("a", b""), ("b", b"x")];
        independent_check(&archive(&files, LIMIT), &files, "empty");
    }

    /// Every ZIP64 field, on small data: the limit lowered to 0 puts every size and offset
    /// past it. Readers take the 8-byte values.
    #[test]
    fn zip64_fields_read_back() {
        let files: [(&'static str, &[u8]); 2] = [
            ("ampr_commands.bin", b"journal bytes"),
            ("ampr_emu.index", b"index"),
        ];
        let zip = archive(&files, 0);
        assert!(zip.windows(4).any(|w| w == [0x50, 0x4b, 6, 6]), "ZIP64 end");
        assert!(
            zip.windows(4).any(|w| w == [0x50, 0x4b, 6, 7]),
            "its locator"
        );
        independent_check(&zip, &files, "zip64");
    }

    /// The real limit with sizes past 4 GiB: which fields switch to ZIP64, and the lengths.
    #[test]
    fn zip64_layout_past_4_gib() {
        let big = Entry {
            name: "ampr_commands.bin",
            size: LIMIT + 10,
        };
        let small_one = Entry {
            name: "ampr_emu.index",
            size: 100,
        };
        let header = local_header(&big, (0, 0), LIMIT);
        assert_eq!(header.len(), 30 + 17 + 20);
        assert_eq!(&header[4..6], &45u16.to_le_bytes());
        assert_eq!(&header[18..26], &[0xff; 8]);
        let desc = descriptor(&big, 7, LIMIT);
        assert_eq!(desc.len(), 24);
        assert_eq!(&desc[8..16], &(LIMIT + 10).to_le_bytes());
        assert_eq!(descriptor(&small_one, 7, LIMIT).len(), 16);
        // The index's header starts past 4 GiB: only its offset is ZIP64.
        let off = (30 + 17 + 20) + LIMIT + 10 + 24;
        let cd = central_entry(&small_one, 7, off, (0, 0), LIMIT);
        assert_eq!(cd.len(), 46 + 14 + 4 + 8);
        assert_eq!(&cd[20..28], &[100, 0, 0, 0, 100, 0, 0, 0]);
        assert_eq!(&cd[42..46], &[0xff; 4]);
        assert_eq!(&cd[cd.len() - 8..], &off.to_le_bytes());
        let cd_big = central_entry(&big, 7, 0, (0, 0), LIMIT);
        assert_eq!(cd_big.len(), 46 + 17 + 4 + 16);
        let entries = [big, small_one];
        let plan = layout(&entries, LIMIT);
        assert_eq!(plan.offsets, [0, off]);
        let end_at = off + (30 + 14) + 100 + 16;
        assert_eq!(plan.cd_offset, end_at);
        assert_eq!(plan.total, end_at + plan.cd_size + 56 + 20 + 22);
        assert_eq!(
            length(&[Entry { name: "a", size: 1 }]),
            31 + 1 + 16 + 47 + 22
        );
    }

    /// Both files back through the reader: directory, data range, CRC-checked stream.
    fn read_back(zip: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
        let mut f = io::Cursor::new(zip);
        let len = zip.len() as u64;
        let mut out = Vec::new();
        for e in directory(&mut f, len)? {
            let at = data_at(&mut f, len, &e)?;
            f.seek(SeekFrom::Start(at)).unwrap();
            let name = String::from_utf8_lossy(&e.name).into_owned();
            let mut bytes = Vec::new();
            Checked::new((&mut f).take(e.size), &name, e.size, e.crc)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            out.push((name, bytes));
        }
        Ok(out)
    }

    #[test]
    fn the_reader_reads_what_the_writer_wrote() {
        let files: [(&'static str, &[u8]); 2] = [
            ("ampr_commands.bin", b"journal bytes"),
            ("ampr_emu.index", b"index"),
        ];
        let want: Vec<(String, Vec<u8>)> = files
            .iter()
            .map(|(n, b)| (n.to_string(), b.to_vec()))
            .collect();
        for limit in [LIMIT, 0] {
            assert_eq!(
                read_back(&archive(&files, limit)).unwrap(),
                want,
                "limit {limit}"
            );
        }
    }

    #[test]
    fn a_damaged_zip_is_refused_clearly() {
        let files: [(&'static str, &[u8]); 2] = [
            ("ampr_commands.bin", b"journal bytes"),
            ("ampr_emu.index", b"index"),
        ];
        let zip = archive(&files, LIMIT);
        let err = |zip: &[u8]| read_back(zip).unwrap_err();
        // A flipped data byte: the CRC-32.
        let mut bad = zip.clone();
        bad[30 + 17] ^= 1;
        assert!(
            err(&bad).contains("ampr_commands.bin fails its CRC-32"),
            "{}",
            err(&bad)
        );
        // Cut off: no end record.
        assert!(err(&zip[..zip.len() - 30]).contains("no end of central directory"));
        assert!(err(&zip[..10]).contains("no end of central directory"));
        assert!(err(b"").contains("no end of central directory"));
        // Deflated (method 8) in the central directory.
        let cd = zip
            .windows(4)
            .position(|w| w == CENTRAL.to_le_bytes())
            .unwrap();
        let mut deflated = zip.clone();
        deflated[cd + 10] = 8;
        assert!(
            err(&deflated).contains("unzip it and pick the folder"),
            "{}",
            err(&deflated)
        );
        // A local header offset past the end, and a size past it.
        let mut far = zip.clone();
        far[cd + 42..cd + 46].copy_from_slice(&0x7fff_0000u32.to_le_bytes());
        assert!(err(&far).contains("pass the end"), "{}", err(&far));
        let mut big = zip.clone();
        big[cd + 20..cd + 28].copy_from_slice(&[0, 0, 0, 0x7f, 0, 0, 0, 0x7f]);
        assert!(
            err(&big).contains("pass the end of the zip"),
            "{}",
            err(&big)
        );
        // A directory out of place, an entry count it can't hold.
        let mut lost = zip.clone();
        let end = lost.len() - 22;
        lost[end + 16..end + 20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(err(&lost).contains("out of place"), "{}", err(&lost));
        let mut many = zip.clone();
        many[end + 10..end + 12].copy_from_slice(&9u16.to_le_bytes());
        assert!(err(&many).contains("9 entries announced"), "{}", err(&many));
        // Random bytes never panic.
        let mut x = 0x1234_5678u32;
        for n in 0..2000usize {
            let mut junk = zip.clone();
            for _ in 0..1 + n % 7 {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                let at = x as usize % junk.len();
                junk[at] = (x >> 8) as u8;
            }
            junk.truncate(junk.len() - n % 40);
            let _ = read_back(&junk);
        }
    }

    /// The ZIP64 fields of an entry past 4 GiB read back (no 4 GiB of data: a directory and
    /// its end records alone, the size then refused as passing the end).
    #[test]
    fn the_reader_takes_zip64_sizes() {
        let big = Entry {
            name: "ampr_commands.bin",
            size: LIMIT + 10,
        };
        let cd = central_entry(&big, 7, LIMIT + 3, (0, 0), LIMIT);
        let mut zip = cd.clone();
        zip.extend(end_records(1, 0, cd.len() as u64, 0));
        let mut f = io::Cursor::new(&zip);
        let found = directory(&mut f, zip.len() as u64).unwrap();
        assert_eq!(
            (
                found[0].size,
                found[0].compressed,
                found[0].offset,
                found[0].crc
            ),
            (LIMIT + 10, LIMIT + 10, LIMIT + 3, 7)
        );
        let err = data_at(&mut f, zip.len() as u64, &found[0]).unwrap_err();
        assert!(err.contains("pass the end"), "{err}");
    }

    #[test]
    fn dos_times() {
        let at = |s| dos_time(SystemTime::UNIX_EPOCH + Duration::from_secs(s));
        // 2026-10-10 01:07:42 UTC.
        assert_eq!(
            at(1_791_594_462),
            (1 << 11 | 7 << 5 | 21, (46 << 9 | 10 << 5 | 10) as u16)
        );
        assert_eq!(at(0), (0, 1 << 5 | 1)); // clamped to 1980-01-01
        // 2000-02-29 23:59:58.
        assert_eq!(
            at(951_868_798),
            (23 << 11 | 59 << 5 | 29, (20 << 9 | 2 << 5 | 29) as u16)
        );
    }
}
