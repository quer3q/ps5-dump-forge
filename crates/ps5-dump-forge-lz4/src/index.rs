//! Checked AMPRIDX3 path-index reader. Stricter than the runtime's own parser: a forged index
//! must not be able to attach trace ids to the wrong files.

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
    use crate::format::{put_u32, put_u64};

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

    fn slots_at(b: &[u8]) -> usize {
        u64_at(b, 32).unwrap() as usize
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
    fn single_record_and_many() {
        assert_eq!(read_index(&built(&["only"])).unwrap().records.len(), 1);
        let names: Vec<String> = (0..500).map(|i| format!("d{}/f{i}.bin", i % 7)).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        assert_eq!(read_index(&built(&refs)).unwrap().records.len(), 500);
    }

    #[test]
    fn hash_vectors_agree_with_the_builder() {
        let b = built(&["toc"]);
        let s = slots_at(&b);
        let slot = (0..2)
            .map(|n| u64_at(&b, s + n * 16).unwrap())
            .max()
            .unwrap();
        assert_eq!(slot, 0x348925d202b64400);
        assert_eq!(idx_hash(b"/app0/toc"), 0x348925d202b64400);
    }

    /// Hand-made: two records in non-sorted order, 4 slots, the first record's hash collides
    /// on its home slot so the second must be found by linear probing.
    fn hand_made(paths: [&str; 2]) -> Vec<u8> {
        let full: Vec<String> = paths.iter().map(|p| format!("/app0/{p}")).collect();
        let mut blob = Vec::new();
        let mut recs = Vec::new();
        for (i, p) in full.iter().enumerate() {
            recs.extend_from_slice(&(blob.len() as u32).to_le_bytes());
            recs.extend_from_slice(&(p.len() as u32).to_le_bytes());
            recs.extend_from_slice(&(10 * (i as u64 + 1)).to_le_bytes());
            recs.extend_from_slice(&0i64.to_le_bytes());
            blob.extend_from_slice(p.as_bytes());
            blob.push(0);
        }
        let slots_at = (48 + recs.len() + blob.len()).next_multiple_of(16);
        let mut out = vec![0u8; 48];
        out[..8].copy_from_slice(b"AMPRIDX3");
        put_u32(&mut out, 8, 3).unwrap();
        put_u32(&mut out, 12, 24).unwrap();
        put_u64(&mut out, 16, 2).unwrap();
        put_u64(&mut out, 24, blob.len() as u64).unwrap();
        put_u64(&mut out, 32, slots_at as u64).unwrap();
        put_u32(&mut out, 40, 16).unwrap();
        put_u32(&mut out, 44, 4).unwrap();
        out.extend_from_slice(&recs);
        out.extend_from_slice(&blob);
        out.resize(slots_at + 64, 0);
        let mut table = [(0u64, 0u32); 4];
        for (i, p) in full.iter().enumerate() {
            let h = idx_hash(p.as_bytes());
            let mut at = h as usize & 3;
            while table[at].1 != 0 {
                at = (at + 1) & 3;
            }
            table[at] = (h, i as u32 + 1);
        }
        for (n, (h, r)) in table.iter().enumerate() {
            put_u64(&mut out, slots_at + n * 16, *h).unwrap();
            put_u32(&mut out, slots_at + n * 16 + 8, *r).unwrap();
        }
        out
    }

    #[test]
    fn unsorted_valid_index_is_accepted() {
        let idx = read_index(&hand_made(["zeta/Last.bin", "alpha.bin"])).unwrap();
        assert_eq!(
            idx.records,
            [
                ("zeta/Last.bin".to_string(), 10),
                ("alpha.bin".to_string(), 20)
            ]
        );
    }

    #[test]
    fn slot_probe_wrap_is_accepted() {
        // Find two names whose home slot is the last of 4, so the second wraps to slot 0.
        let mut names = Vec::new();
        for i in 0..200 {
            let n = format!("f{i}");
            if idx_hash(format!("/app0/{n}").as_bytes()) & 3 == 3 {
                names.push(n);
            }
            if names.len() == 2 {
                break;
            }
        }
        let idx = read_index(&hand_made([&names[0], &names[1]])).unwrap();
        assert_eq!(idx.records.len(), 2);
    }

    #[test]
    fn rejects_header_damage() {
        let good = built(&["a", "b", "c"]);
        let mut cases: Vec<(&str, Vec<u8>)> = Vec::new();
        let mut patch = |name, f: &dyn Fn(&mut Vec<u8>)| {
            let mut b = good.clone();
            f(&mut b);
            cases.push((name, b));
        };
        patch("magic", &|b| b[7] = b'2');
        patch("version", &|b| put_u32(b, 8, 4).unwrap());
        patch("record size", &|b| put_u32(b, 12, 32).unwrap());
        patch("slot size", &|b| put_u32(b, 40, 8).unwrap());
        patch("zero count", &|b| put_u64(b, 16, 0).unwrap());
        patch("count overflow", &|b| put_u64(b, 16, u64::MAX).unwrap());
        patch("count over limit", &|b| put_u64(b, 16, 2_000_001).unwrap());
        patch("count past file", &|b| put_u64(b, 16, 1_000_000).unwrap());
        patch("path bytes huge", &|b| put_u64(b, 24, u64::MAX).unwrap());
        patch("path bytes zero", &|b| put_u64(b, 24, 0).unwrap());
        patch("slot offset overflow", &|b| {
            put_u64(b, 32, u64::MAX - 8).unwrap()
        });
        patch("slot offset misaligned", &|b| {
            let v = u64_at(b, 32).unwrap() + 8;
            put_u64(b, 32, v).unwrap()
        });
        patch("slot offset in blob", &|b| put_u64(b, 32, 48).unwrap());
        patch("slot count not pow2", &|b| put_u32(b, 44, 12).unwrap());
        patch("slot count below records", &|b| put_u32(b, 44, 2).unwrap());
        patch("slot count huge", &|b| put_u32(b, 44, 1 << 31).unwrap());
        for (name, b) in cases {
            assert!(read_index(&b).is_err(), "{name}");
        }
    }

    #[test]
    fn rejects_truncation_at_every_length() {
        let good = built(&["a", "b/c", "d"]);
        for n in 0..good.len() {
            assert!(read_index(&good[..n]).is_err(), "{n}");
        }
        assert!(read_index(&good).is_ok());
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

    #[test]
    fn rejects_folded_duplicate_paths() {
        // Same length, same path up to ASCII case.
        let b = hand_made(["Dup.bin", "dup.bin"]);
        assert!(read_index(&b).is_err());
        assert!(read_index(&hand_made(["dup.bin", "other.b"])).is_ok());
    }

    #[test]
    fn rejects_slot_damage() {
        let good = built(&["a", "b", "c"]);
        let s = slots_at(&good);
        let count = u32_at(&good, 44).unwrap() as usize;
        let filled: Vec<usize> = (0..count)
            .filter(|n| u32_at(&good, s + n * 16 + 8).unwrap() != 0)
            .collect();
        let empty = (0..count).find(|n| !filled.contains(n)).unwrap();
        let f = filled[0];
        let at = |n: usize| s + n * 16;
        let mut cases: Vec<(&str, Vec<u8>)> = Vec::new();
        let mut patch = |name, f: &dyn Fn(&mut Vec<u8>)| {
            let mut b = good.clone();
            f(&mut b);
            cases.push((name, b));
        };
        patch("wrong hash", &|b| b[at(f)] ^= 1);
        patch("record past end", &|b| put_u32(b, at(f) + 8, 99).unwrap());
        patch("unknown flag", &|b| put_u32(b, at(f) + 12, 2).unwrap());
        patch("referenced twice", &|b| {
            let (h, r) = (u64_at(b, at(f)).unwrap(), u32_at(b, at(f) + 8).unwrap());
            put_u64(b, at(empty), h).unwrap();
            put_u32(b, at(empty) + 8, r).unwrap()
        });
        patch("dirty empty slot", &|b| put_u64(b, at(empty), 5).unwrap());
        patch("record without slot", &|b| {
            put_u64(b, at(f), 0).unwrap();
            put_u32(b, at(f) + 8, 0).unwrap()
        });
        patch("swapped records", &|b| {
            let (g, h) = (filled[0], filled[1]);
            let (a, c) = (u32_at(b, at(g) + 8).unwrap(), u32_at(b, at(h) + 8).unwrap());
            put_u32(b, at(g) + 8, c).unwrap();
            put_u32(b, at(h) + 8, a).unwrap()
        });
        for (name, b) in cases {
            assert!(read_index(&b).is_err(), "{name}");
        }
    }

    /// `n` records, record `i` in slot `i` (a full table), with hand-set correct hashes.
    fn full_table(n: usize) -> Vec<u8> {
        let paths: Vec<String> = (0..n).map(|i| format!("f{i}")).collect();
        let mut blob = Vec::new();
        let mut recs = Vec::new();
        for p in &paths {
            let full = format!("/app0/{p}");
            recs.extend_from_slice(&(blob.len() as u32).to_le_bytes());
            recs.extend_from_slice(&(full.len() as u32).to_le_bytes());
            recs.extend_from_slice(&[0u8; 16]);
            blob.extend_from_slice(full.as_bytes());
            blob.push(0);
        }
        let slots_at = (48 + recs.len() + blob.len()).next_multiple_of(16);
        let mut out = vec![0u8; 48];
        out[..8].copy_from_slice(IDX_MAGIC);
        put_u32(&mut out, 8, 3).unwrap();
        put_u32(&mut out, 12, 24).unwrap();
        put_u64(&mut out, 16, n as u64).unwrap();
        put_u64(&mut out, 24, blob.len() as u64).unwrap();
        put_u64(&mut out, 32, slots_at as u64).unwrap();
        put_u32(&mut out, 40, 16).unwrap();
        put_u32(&mut out, 44, n as u32).unwrap();
        out.extend_from_slice(&recs);
        out.extend_from_slice(&blob);
        out.resize(slots_at, 0);
        for (i, p) in paths.iter().enumerate() {
            let h = idx_hash(format!("/app0/{p}").as_bytes());
            out.extend_from_slice(&h.to_le_bytes());
            out.extend_from_slice(&(i as u32 + 1).to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
        }
        out
    }

    #[test]
    fn a_large_full_table_validates_in_linear_time() {
        let b = full_table(1 << 16);
        let t = std::time::Instant::now();
        assert_eq!(read_index(&b).unwrap().records.len(), 1 << 16);
        assert!(t.elapsed().as_secs() < 5, "{:?}", t.elapsed());
    }

    #[test]
    fn rejects_a_broken_probe_chain() {
        // Two names with the same home slot: the second sits one slot after the first.
        let home = |n: &str| idx_hash(format!("/app0/{n}").as_bytes()) as usize & 3;
        let names: Vec<String> = (0..64).map(|i| format!("f{i}")).collect();
        let a = &names[0];
        let b = names.iter().skip(1).find(|n| home(n) == home(a)).unwrap();
        let good = hand_made([a, b]);
        assert!(read_index(&good).is_ok());
        let s = slots_at(&good);
        let at = |n: usize| s + n * 16;
        let h = home(a);
        // Empty the home slot and put its record two slots on: the chain from home now dies.
        let mut bad = good.clone();
        let (hash, rec) = (
            u64_at(&bad, at(h)).unwrap(),
            u32_at(&bad, at(h) + 8).unwrap(),
        );
        let far = (h + 2) & 3;
        assert_eq!(u32_at(&bad, at(far) + 8).unwrap(), 0);
        put_u64(&mut bad, at(h), 0).unwrap();
        put_u32(&mut bad, at(h) + 8, 0).unwrap();
        put_u64(&mut bad, at(far), hash).unwrap();
        put_u32(&mut bad, at(far) + 8, rec).unwrap();
        assert!(read_index(&bad).is_err());
    }
}
