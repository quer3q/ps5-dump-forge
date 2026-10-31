//! Checked AMPRIDX3 path-index reader. Stricter than the runtime's own parser: a forged index
//! must not be able to attach file ids to the wrong files.

use std::collections::HashSet;

use crate::format::{
    APP0, IDX_HEADER, IDX_MAGIC, IDX_RECORD, IDX_SLOT, IDX_VERSION, check_magic, idx_hash,
    logical_to_rel, u32_at, u64_at,
};
use ps5upload_fpkg::{Error, Result};

pub const MAX_RECORDS: u64 = 2_000_000;
pub const MAX_PATH_BLOB: u64 = 256 << 20;
/// Slot flag: another path shares this slot's hash.
const SLOT_SHARED: u32 = 1;

/// The records of a path index, in on-disk record order (file id `n` is `records[n - 1]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathIndex {
    /// (path relative to `/app0/`, spelling kept; size).
    pub records: Vec<(String, u64)>,
}

fn bad(msg: impl Into<String>) -> Error {
    Error::Format(format!("path index: {}", msg.into()))
}

fn folded(path: &str) -> Vec<u8> {
    path.bytes()
        .map(|b| match b {
            b'\\' => b'/',
            b'A'..=b'Z' => b + 32,
            _ => b,
        })
        .collect()
}

/// Parses and validates an `ampr_emu.index` image.
pub fn read_index(bytes: &[u8]) -> Result<PathIndex> {
    check_magic(bytes, IDX_MAGIC, "a path index")?;
    if bytes.len() < IDX_HEADER {
        return Err(bad("shorter than its header"));
    }
    let version = u32_at(bytes, 8)?;
    let record_size = u32_at(bytes, 12)?;
    let count = u64_at(bytes, 16)?;
    let path_bytes = u64_at(bytes, 24)?;
    let slots_at = u64_at(bytes, 32)?;
    let slot_size = u32_at(bytes, 40)?;
    let slot_count = u32_at(bytes, 44)?;
    if version != IDX_VERSION {
        return Err(bad(format!("version {version}, expected {IDX_VERSION}")));
    }
    if record_size as usize != IDX_RECORD || slot_size as usize != IDX_SLOT {
        return Err(bad(format!(
            "record size {record_size} / slot size {slot_size}, expected {IDX_RECORD} / {IDX_SLOT}"
        )));
    }
    if count == 0 || count > MAX_RECORDS {
        return Err(bad(format!("{count} records (1..={MAX_RECORDS} allowed)")));
    }
    if path_bytes == 0 || path_bytes > MAX_PATH_BLOB {
        return Err(bad(format!("{path_bytes} path bytes")));
    }
    if slot_count < 2 || !slot_count.is_power_of_two() || u64::from(slot_count) < count {
        return Err(bad(format!(
            "{slot_count} slots for {count} records (power of two, at least 2 and the record count)"
        )));
    }
    // Both fit u64: count <= 2e6, path_bytes <= 256 MiB, slot_count < 2^32.
    let blob_at = IDX_HEADER as u64 + count * IDX_RECORD as u64;
    let blob_end = blob_at + path_bytes;
    let table_end = slots_at
        .checked_add(u64::from(slot_count) * IDX_SLOT as u64)
        .ok_or_else(|| bad("slot table overflows"))?;
    if slots_at % IDX_SLOT as u64 != 0 || slots_at < blob_end {
        return Err(bad(format!(
            "slot table at {slots_at} is misaligned or overlaps the path blob (ends {blob_end})"
        )));
    }
    if table_end > bytes.len() as u64 {
        return Err(bad(format!(
            "truncated: slot table ends at {table_end}, file is {} bytes",
            bytes.len()
        )));
    }
    // Everything below is within bytes.len(), so it fits usize.
    let blob = &bytes[blob_at as usize..blob_end as usize];
    let mut records = Vec::with_capacity(count as usize);
    let mut seen = HashSet::with_capacity(count as usize);
    for i in 0..count as usize {
        let at = IDX_HEADER + i * IDX_RECORD;
        let off = u64::from(u32_at(bytes, at)?);
        let len = u64::from(u32_at(bytes, at + 4)?);
        let size = u64_at(bytes, at + 8)?;
        let end = off + len;
        if end >= path_bytes || blob[end as usize] != 0 {
            return Err(bad(format!(
                "record {i}: path {off}+{len} is outside the blob or not NUL-terminated"
            )));
        }
        let full = std::str::from_utf8(&blob[off as usize..end as usize])
            .map_err(|_| bad(format!("record {i}: path is not UTF-8")))?;
        let rel = logical_to_rel(full).map_err(|e| bad(format!("record {i}: {e}")))?;
        if !seen.insert(folded(full)) {
            return Err(bad(format!("record {i}: duplicate path {full:?}")));
        }
        debug_assert!(full.len() == APP0.len() + rel.len());
        records.push((rel.to_owned(), size));
    }

    // Slot pass: every filled slot hashes its own record's path, and no record is referenced twice.
    let slot = |n: usize| -> Result<(u64, u32, u32)> {
        let at = slots_at as usize + n * IDX_SLOT;
        Ok((
            u64_at(bytes, at)?,
            u32_at(bytes, at + 8)?,
            u32_at(bytes, at + 12)?,
        ))
    };
    let mut referenced = vec![false; records.len()];
    let mut hashes = vec![0u64; records.len()];
    for n in 0..slot_count as usize {
        let (hash, rec, flags) = slot(n)?;
        if rec == 0 {
            if hash != 0 || flags != 0 {
                return Err(bad(format!("slot {n}: empty slot is not zero")));
            }
            continue;
        }
        let i = rec as usize - 1;
        if i >= records.len() {
            return Err(bad(format!("slot {n}: record {rec} does not exist")));
        }
        if flags & !SLOT_SHARED != 0 {
            return Err(bad(format!("slot {n}: unknown flags {flags:#x}")));
        }
        if std::mem::replace(&mut referenced[i], true) {
            return Err(bad(format!("slot {n}: record {rec} is referenced twice")));
        }
        let want = idx_hash(format!("{APP0}{}", records[i].0).as_bytes());
        if hash != want {
            return Err(bad(format!("slot {n}: hash does not match record {rec}")));
        }
        hashes[i] = hash;
    }
    if let Some(i) = referenced.iter().position(|r| !r) {
        return Err(bad(format!("record {i} has no slot")));
    }

    // Probe pass, linear: a lookup from a record's home slot must cross only filled slots to reach
    // it, i.e. the run of filled slots ending at its slot (cyclic) covers the distance from home.
    let mask = slot_count as usize - 1;
    let mut slot_of = vec![0usize; records.len()];
    let mut filled = vec![false; slot_count as usize];
    for (n, f) in filled.iter_mut().enumerate() {
        let (_, rec, _) = slot(n)?;
        if rec != 0 {
            slot_of[rec as usize - 1] = n;
            *f = true;
        }
    }
    // Run length ending at each slot, starting just after an empty slot (none: all reachable).
    if let Some(empty) = filled.iter().position(|f| !f) {
        let mut run = vec![0usize; filled.len()];
        let mut len = 0usize;
        for step in 1..=mask + 1 {
            let n = (empty + step) & mask;
            len = if filled[n] { len + 1 } else { 0 };
            run[n] = len;
        }
        for (i, &h) in hashes.iter().enumerate() {
            let at = slot_of[i];
            if (at.wrapping_sub(h as usize & mask) & mask) >= run[at] {
                return Err(bad(format!("record {i} is unreachable by its probe chain")));
            }
        }
    }
    Ok(PathIndex { records })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::put_u32;

    fn files(names: &[&str]) -> Vec<(String, u64)> {
        names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.to_string(), 100 + i as u64))
            .collect()
    }

    fn built(names: &[&str]) -> Vec<u8> {
        ps5upload_fpkg::ampr_index::build(&files(names), 7).unwrap()
    }

    #[test]
    fn round_trips_the_builder_in_its_sorted_order() {
        let src = files(&[
            "z.bin",
            "Data/Mixed_Case.PAK",
            "a/b/c.txt",
            "sce_sys/param.json",
        ]);
        let idx = read_index(&ps5upload_fpkg::ampr_index::build(&src, 0).unwrap()).unwrap();
        assert_eq!(idx.records.len(), 4);
        // Sorted by folded key; spelling preserved.
        let names: Vec<_> = idx.records.iter().map(|r| r.0.as_str()).collect();
        assert_eq!(
            names,
            [
                "a/b/c.txt",
                "Data/Mixed_Case.PAK",
                "sce_sys/param.json",
                "z.bin"
            ]
        );
        for (p, s) in &src {
            assert!(idx.records.contains(&(p.clone(), *s)), "{p}");
        }
    }

    #[test]
    fn rejects_record_and_path_damage() {
        let good = built(&["alpha", "beta"]);
        let rec = |i: usize| 48 + i * 24;
        let blob = 48 + 2 * 24;
        let mut cases: Vec<(&str, Vec<u8>)> = Vec::new();
        let mut patch = |name, f: &dyn Fn(&mut Vec<u8>)| {
            let mut b = good.clone();
            f(&mut b);
            cases.push((name, b));
        };
        patch("path past blob", &|b| put_u32(b, rec(0), 9999).unwrap());
        patch("length overflow", &|b| {
            put_u32(b, rec(0) + 4, u32::MAX).unwrap()
        });
        patch("offset overflow", &|b| {
            put_u32(b, rec(0), u32::MAX).unwrap();
            put_u32(b, rec(0) + 4, u32::MAX).unwrap()
        });
        patch("no NUL", &|b| b[blob + "/app0/alpha".len()] = b'x');
        patch("short length", &|b| put_u32(b, rec(0) + 4, 5).unwrap());
        patch("embedded NUL", &|b| b[blob + 8] = 0);
        patch("not app0", &|b| b[blob + 4] = b'1');
        patch("backslash", &|b| b[blob + 8] = b'\\');
        patch("bad utf8", &|b| b[blob + 8] = 0xff);
        patch("empty component", &|b| b[blob + 7] = b'/');
        for (name, b) in cases {
            assert!(read_index(&b).is_err(), "{name}");
        }
    }
}
