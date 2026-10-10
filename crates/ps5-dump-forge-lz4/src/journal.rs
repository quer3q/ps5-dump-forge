//! Streaming AMPRCMD1 journal scanner: which files a traced run read, plus damage counters.
//! Memory is bounded by one record's payload; nothing is kept per event.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::format::{CMD_HEADER, CMD_MAGIC, CMD_VERSION, cmd_hash, u16_at, u32_at, u64_at};
use ps5upload_fpkg::{Error, Result};

/// Largest `recordBytes` taken as a real record; the emulator's own queue is 4 MiB.
/// A bigger claim has no trusted boundary.
pub const MAX_RECORD_BYTES: u32 = 64 << 20;
const DOMAIN_APR: u32 = 1;
/// Priorities kept for inherited state; more distinct ones reset the table.
const MAX_PRIORITIES: usize = 64;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct JournalStats {
    /// 1-based ids of files with a valid ReadFile (record `id - 1` of the path index).
    pub observed: BTreeSet<u32>,
    /// Records framed, hash-checked and decoded (domain 1).
    pub records: u64,
    /// Records skipped because the payload hash did not match.
    pub bad_hash: u64,
    /// Records that ended early on an unknown, invalid or overrunning packet.
    pub unknown: u64,
    /// Sequence gaps or backward jumps (including a first record other than 1).
    pub gaps: u64,
    /// Decoded records whose non-zero `commandCount` differed from the packets found.
    pub count_mismatch: u64,
    /// Framed records of other domains, skipped.
    pub other_domain: u64,
    /// Read-type packets mapped to a file (ReadFile, Gather, Scatter, GatherScatter).
    pub reads: u64,
    /// Sum of their requested lengths (ranges are summed, not unioned), saturating.
    pub requested_bytes: u64,
    /// ReadFile packets whose file id was 0 or beyond `file_count`.
    pub missing_ids: u64,
    /// Gather/Scatter/GatherScatter packets with no inherited ReadFile state.
    pub stateless_reads: u64,
    /// Bytes of a final record cut short by the end of the stream.
    pub truncated_tail: u64,
    /// Bytes from an unframeable header (bad magic/version/lengths) to the end of the stream.
    pub unparsed_tail: u64,
}

/// Reads up to `buf.len()` bytes; fewer only at end of stream.
fn fill<R: Read>(r: &mut R, buf: &mut [u8], cancel: &AtomicBool) -> Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        match r.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(n)
}

/// Discards up to `n` bytes, returning how many were there.
fn skip<R: Read>(r: &mut R, n: u64, cancel: &AtomicBool) -> Result<u64> {
    let mut buf = [0u8; 8192];
    let mut left = n;
    while left > 0 {
        let k = fill(r, &mut buf[..left.min(8192) as usize], cancel)?;
        left -= k as u64;
        if k == 0 {
            break;
        }
    }
    Ok(n - left)
}

/// Reads up to `want` bytes into `payload` (cleared first), growing it only as bytes arrive in
/// bounded chunks, so a lying length costs nothing. Fewer bytes only at end of stream.
fn read_payload<R: Read>(
    r: &mut R,
    payload: &mut Vec<u8>,
    want: usize,
    cancel: &AtomicBool,
) -> Result<usize> {
    const CHUNK: usize = 1 << 16;
    payload.clear();
    while payload.len() < want {
        let at = payload.len();
        let step = CHUNK.min(want - at);
        payload.resize(at + step, 0);
        let got = fill(r, &mut payload[at..], cancel)?;
        payload.truncate(at + got);
        if got < step {
            break;
        }
    }
    Ok(payload.len())
}

/// Dwords of one packet (`None`: unknown or invalid), per the upstream decoder's order.
fn packet_len(w0: u32) -> Option<usize> {
    let op8 = w0 & 0xff;
    let op12 = w0 & 0xfff;
    let n8 = ((w0 >> 8) & 15) as usize + 1;
    let ok = |d: usize, lo: usize, hi: usize| (lo..=hi).contains(&d).then_some(d);
    match op8 {
        1 => return ok(n8, 2, 4),
        2 => return ok(n8, 1, 5).filter(|d| *d != 4),
        5 | 0x75 => return ok(((w0 >> 8) & 3) as usize + 1, 2, 4),
        6 | 0x76 => return ok(n8, 1, 3),
        _ => {}
    }
    if op12 == 0x408 || op12 == 0x478 {
        return Some(5);
    }
    if w0 & 0xffff_000f == 0x5452_000f {
        let ty = (w0 >> 12) & 15;
        return ok(n8, if ty == 5 || ty == 6 { 2 } else { 1 }, 16);
    }
    match op8 {
        40 => return ok(((w0 >> 8) & 7) as usize + 1, 5, 6),
        41 => return ok(((w0 >> 8) & 3) as usize + 1, 2, 3),
        43 => return ok(((w0 >> 8) & 7) as usize + 1, 4, 5),
        _ => {}
    }
    match op12 {
        0x22a | 0x22d => return Some(3),
        0x32d => return Some(4),
        _ => {}
    }
    if w0 == 47 || w0 == 46 {
        return Some(1);
    }
    // AMM packets: lengths only; not profiled.
    match op12 {
        0x221 | 0x222 | 0x228 => Some(3),
        0x321 | 0x325 | 0x323 | 0x324 | 0x327 | 0x326 => Some(4),
        0x423..=0x426 => Some(5),
        _ => None,
    }
}

struct State {
    file_count: u32,
    /// Per priority: (file id, next source offset).
    inherited: BTreeMap<u32, (u32, u64)>,
}

impl State {
    fn set(&mut self, priority: u32, v: (u32, u64)) {
        if self.inherited.len() >= MAX_PRIORITIES && !self.inherited.contains_key(&priority) {
            self.inherited.clear();
        }
        self.inherited.insert(priority, v);
    }

    /// Decodes one verified domain-1 payload into `st`.
    fn record(&mut self, payload: &[u8], priority: u32, command_count: u32, st: &mut JournalStats) {
        let mut at = 0;
        let mut packets = 0u32;
        let mut clean = true;
        while at < payload.len() {
            let dwords = payload.len() / 4 - at / 4;
            let w = |i: usize| u32_at(payload, at + 4 * i).unwrap_or(0);
            let Some(d) = (dwords > 0)
                .then(|| packet_len(w(0)))
                .flatten()
                .filter(|d| *d <= dwords)
            else {
                clean = false;
                break;
            };
            self.packet(d, [w(0), w(1), w(2), w(3), w(4), w(5)], priority, st);
            packets += 1;
            at += 4 * d;
        }
        if !clean {
            st.unknown += 1;
            self.inherited.clear();
        } else if command_count != 0 && command_count != packets {
            st.count_mismatch += 1;
        }
        st.records += 1;
    }

    fn packet(&mut self, d: usize, w: [u32; 6], priority: u32, st: &mut JournalStats) {
        let w0 = w[0];
        let len = u64::from(w[1]) + 1;
        let low = u64::from((w0 >> 12) & 0x3ffff);
        if w0 == 47 {
            self.inherited.remove(&priority);
            return;
        }
        if w0 & 0xff == 40 {
            let mut off = low | u64::from(w[4] & 0xfffc_0000);
            if d == 6 {
                off |= u64::from(w[5] & 0xff) << 32;
            }
            let id = w[2] & 0x7fff_ffff;
            if id == 0 || id > self.file_count {
                st.missing_ids += 1;
                self.inherited.remove(&priority);
            } else {
                st.observed.insert(id);
                self.account(priority, id, off, len, st);
            }
            return;
        }
        let kind = match w0 & 0xff {
            41 => 1,
            43 => 2,
            _ if w0 & 0xfff == 0x22a => 3,
            _ => return,
        };
        let Some(&(id, next)) = self.inherited.get(&priority) else {
            st.stateless_reads += 1;
            return;
        };
        let off = match kind {
            1 if d == 3 => low | u64::from(w[2] & 0x3f_ffff) << 18,
            1 => low,
            2 => {
                let hi = if d == 5 {
                    u64::from(w[4] & 0xff) << 32
                } else {
                    0
                };
                low | u64::from(w[3] & 0xfffc_0000) | hi
            }
            _ => next,
        };
        self.account(priority, id, off, len, st);
    }

    fn account(&mut self, priority: u32, id: u32, offset: u64, len: u64, st: &mut JournalStats) {
        st.reads += 1;
        st.requested_bytes = st.requested_bytes.saturating_add(len);
        match offset.checked_add(len) {
            Some(next) => self.set(priority, (id, next)),
            None => {
                self.inherited.remove(&priority);
            }
        }
    }
}

/// Scans a journal stream. `file_count` is the path index's record count (ids are 1-based).
/// Damage is counted, never an error; only I/O failure and `cancel` (checked per record and per read) are.
pub fn scan<R: Read>(mut r: R, file_count: u32, cancel: &AtomicBool) -> Result<JournalStats> {
    let mut st = JournalStats::default();
    let mut state = State {
        file_count,
        inherited: BTreeMap::new(),
    };
    let mut last_seq = 0u64;
    let mut head = [0u8; CMD_HEADER];
    let mut payload = Vec::new();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let got = fill(&mut r, &mut head, cancel)?;
        if got == 0 {
            return Ok(st);
        }
        if got < CMD_HEADER {
            st.truncated_tail += got as u64;
            return Ok(st);
        }
        let header_bytes = u32::from(u16_at(&head, 10)?);
        let record_bytes = u32_at(&head, 12)?;
        let payload_bytes = u32_at(&head, 64)?;
        let framed = &head[..8] == CMD_MAGIC
            && u16_at(&head, 8)? == CMD_VERSION
            && header_bytes as usize >= CMD_HEADER
            && record_bytes <= MAX_RECORD_BYTES
            && record_bytes.checked_sub(header_bytes) == Some(payload_bytes);
        if !framed {
            st.unparsed_tail = CMD_HEADER as u64 + skip(&mut r, u64::MAX, cancel)?;
            return Ok(st);
        }
        let total = u64::from(record_bytes);
        let extra = u64::from(header_bytes) - CMD_HEADER as u64;
        let seq = u64_at(&head, 16)?;
        let (priority, domain) = (u32_at(&head, 76)?, u32_at(&head, 80)?);

        if last_seq.checked_add(1) != Some(seq) {
            st.gaps += 1;
            state.inherited.clear();
        }
        last_seq = seq;

        let got_extra = skip(&mut r, extra, cancel)?;
        if got_extra < extra {
            st.truncated_tail += CMD_HEADER as u64 + got_extra;
            return Ok(st);
        }
        if domain != DOMAIN_APR {
            let n = skip(&mut r, u64::from(payload_bytes), cancel)?;
            if n < u64::from(payload_bytes) {
                st.truncated_tail += CMD_HEADER as u64 + extra + n;
                return Ok(st);
            }
            st.other_domain += 1;
            continue;
        }
        let n = read_payload(&mut r, &mut payload, payload_bytes as usize, cancel)?;
        if n < payload_bytes as usize {
            st.truncated_tail += total - u64::from(payload_bytes) + n as u64;
            return Ok(st);
        }
        if cmd_hash(&payload) != u64_at(&head, 56)? {
            st.bad_hash += 1;
            state.inherited.clear();
            continue;
        }
        state.record(&payload, priority, u32_at(&head, 72)?, &mut st);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{put_u16, put_u32, put_u64};

    /// One record: a 96-byte header (plus `extra` header bytes) and the payload dwords.
    struct Rec {
        seq: u64,
        domain: u32,
        priority: u32,
        count: u32,
        extra: usize,
        words: Vec<u32>,
    }

    fn rec(seq: u64, words: &[u32]) -> Rec {
        Rec {
            seq,
            domain: 1,
            priority: 0,
            count: 0,
            extra: 0,
            words: words.to_vec(),
        }
    }

    impl Rec {
        fn bytes(&self) -> Vec<u8> {
            let payload: Vec<u8> = self.words.iter().flat_map(|w| w.to_le_bytes()).collect();
            let mut b = vec![0u8; CMD_HEADER + self.extra];
            b[..8].copy_from_slice(CMD_MAGIC);
            put_u16(&mut b, 8, 1).unwrap();
            put_u16(&mut b, 10, (CMD_HEADER + self.extra) as u16).unwrap();
            let total = (b.len() + payload.len()) as u32;
            put_u32(&mut b, 12, total).unwrap();
            put_u64(&mut b, 16, self.seq).unwrap();
            put_u64(&mut b, 56, cmd_hash(&payload)).unwrap();
            put_u32(&mut b, 64, payload.len() as u32).unwrap();
            put_u32(&mut b, 72, self.count).unwrap();
            put_u32(&mut b, 76, self.priority).unwrap();
            put_u32(&mut b, 80, self.domain).unwrap();
            b.extend_from_slice(&payload);
            b
        }
    }

    fn read_file(id: u32, len: u64, off: u64) -> Vec<u32> {
        vec![
            40 | 4 << 8 | ((off & 0x3ffff) as u32) << 12,
            (len - 1) as u32,
            id,
            0,
            (off & 0xfffc_0000) as u32,
        ]
    }
    fn read_file6(id: u32, len: u64, off: u64) -> Vec<u32> {
        let mut w = read_file(id, len, off);
        w[0] += 1 << 8;
        w.push((off >> 32) as u32 & 0xff);
        w
    }
    fn gather(len: u64) -> Vec<u32> {
        vec![41 | 1 << 8, (len - 1) as u32]
    }
    fn scatter(len: u64) -> Vec<u32> {
        vec![0x22a, (len - 1) as u32, 0]
    }
    fn gs(len: u64) -> Vec<u32> {
        vec![43 | 3 << 8, (len - 1) as u32, 0, 0]
    }
    const RESET: u32 = 47;

    fn run(parts: &[Vec<u8>], files: u32) -> JournalStats {
        let all: Vec<u8> = parts.concat();
        scan(&all[..], files, &AtomicBool::new(false)).unwrap()
    }
    fn cat(rs: &[Vec<u32>]) -> Vec<u32> {
        rs.concat()
    }

    #[test]
    fn good_records_observe_files() {
        let mut r1 = rec(1, &cat(&[read_file(2, 100, 0), read_file6(3, 50, 1 << 35)]));
        r1.count = 2;
        let st = run(&[r1.bytes(), rec(2, &read_file(2, 8, 0)).bytes()], 3);
        assert_eq!(st.observed, BTreeSet::from([2, 3]));
        assert_eq!((st.records, st.reads, st.requested_bytes), (2, 3, 158));
        assert_eq!(
            (
                st.bad_hash,
                st.unknown,
                st.gaps,
                st.count_mismatch,
                st.missing_ids
            ),
            (0, 0, 0, 0, 0)
        );
        assert_eq!((st.unparsed_tail, st.truncated_tail), (0, 0));
        assert_eq!(run(&[], 3), JournalStats::default());
    }

    #[test]
    fn length_is_u64_plus_one() {
        let st = run(&[rec(1, &read_file(1, 1 << 32, 0)).bytes()], 1);
        assert_eq!(st.requested_bytes, 1 << 32);
        let st = run(&[rec(1, &[40 | 4 << 8, u32::MAX, 1, 0, 0]).bytes()], 1);
        assert_eq!(st.requested_bytes, 1 << 32);
    }

    #[test]
    fn missing_and_out_of_range_ids_are_counted_not_mapped() {
        let w = cat(&[
            read_file(0, 1, 0),
            read_file(4, 1, 0),
            read_file(0x8000_0002, 1, 0),
        ]);
        let st = run(&[rec(1, &w).bytes()], 3);
        // 0x80000002 masks to id 2: bit 31 is not part of the id.
        assert_eq!(st.missing_ids, 2);
        assert_eq!(st.observed, BTreeSet::from([2]));
        assert_eq!((st.reads, st.requested_bytes), (1, 1));
        // A bad id also drops inherited state.
        let w = cat(&[read_file(1, 1, 0), read_file(9, 1, 0), gather(5)]);
        let st = run(&[rec(1, &w).bytes()], 3);
        assert_eq!((st.reads, st.stateless_reads), (1, 1));
    }

    #[test]
    fn bad_hash_is_skipped_and_the_next_record_parses() {
        let mut bad = rec(1, &read_file(1, 10, 0)).bytes();
        *bad.last_mut().unwrap() ^= 1;
        let st = run(&[bad, rec(2, &read_file(2, 10, 0)).bytes()], 3);
        assert_eq!(st.bad_hash, 1);
        assert_eq!(st.observed, BTreeSet::from([2]));
        assert_eq!((st.records, st.unparsed_tail), (1, 0));
    }

    #[test]
    fn truncation_stops_cleanly() {
        let a = rec(1, &read_file(1, 10, 0)).bytes();
        let b = rec(2, &read_file(2, 10, 0)).bytes();
        let whole = [a.clone(), b.clone()].concat();
        for cut in [1, 50, 95, 96, 100, b.len() - 1] {
            let all = &whole[..a.len() + cut];
            let st = scan(all, 3, &AtomicBool::new(false)).unwrap();
            assert_eq!(st.observed, BTreeSet::from([1]), "{cut}");
            assert_eq!(st.truncated_tail, cut as u64, "{cut}");
            assert_eq!(st.unparsed_tail, 0, "{cut}");
        }
        // Cut inside an other-domain payload and inside an extended header too.
        let mut o = rec(1, &[0; 8]);
        o.domain = 2;
        o.extra = 16;
        let ob = o.bytes();
        for cut in [100, 112, ob.len() - 1] {
            let st = scan(&ob[..cut], 1, &AtomicBool::new(false)).unwrap();
            assert_eq!(
                (st.truncated_tail, st.other_domain),
                (cut as u64, 0),
                "{cut}"
            );
        }
    }

    #[test]
    fn untrusted_framing_stops_and_counts_the_tail() {
        let good = rec(1, &read_file(1, 10, 0)).bytes();
        let tail = rec(2, &read_file(2, 10, 0)).bytes();
        type Damage = Box<dyn Fn(&mut Vec<u8>)>;
        let cases: Vec<(&str, Damage)> = vec![
            ("magic", Box::new(|b| b[0] = b'X')),
            ("version", Box::new(|b| put_u16(b, 8, 2).unwrap())),
            ("header length", Box::new(|b| put_u16(b, 10, 95).unwrap())),
            (
                "record below header",
                Box::new(|b| put_u32(b, 12, 90).unwrap()),
            ),
            (
                "record over cap",
                Box::new(|b| put_u32(b, 12, u32::MAX).unwrap()),
            ),
            ("payload mismatch", Box::new(|b| put_u32(b, 64, 3).unwrap())),
        ];
        for (name, f) in cases {
            let mut b = tail.clone();
            f(&mut b);
            let all = [good.clone(), b.clone()].concat();
            let st = scan(&all[..], 3, &AtomicBool::new(false)).unwrap();
            assert_eq!(st.observed, BTreeSet::from([1]), "{name}");
            assert_eq!(st.unparsed_tail, b.len() as u64, "{name}");
            assert_eq!(st.records, 1, "{name}");
        }
    }

    #[test]
    fn unknown_opcode_ends_the_record_and_clears_state() {
        let w = cat(&[read_file(1, 10, 0), vec![0xdead_beef], read_file(2, 10, 0)]);
        let tail = cat(&[gather(4), read_file(3, 1, 0)]);
        let st = run(&[rec(1, &w).bytes(), rec(2, &tail).bytes()], 3);
        assert_eq!(st.unknown, 1);
        // File 2 sits after the unknown packet: not decoded. Gather has no state left.
        assert_eq!(st.observed, BTreeSet::from([1, 3]));
        assert_eq!(st.stateless_reads, 1);
        assert_eq!(st.records, 2);
    }

    #[test]
    fn invalid_lengths_are_unknown() {
        // WaitOnCounter of 4 dwords, a ReadFile claiming 3, a packet running past the payload,
        // a trailing partial dword.
        for payload in [
            vec![2 | 3 << 8, 0, 0, 0],
            vec![40 | 2 << 8, 0, 1],
            vec![40 | 5 << 8, 0, 1, 0, 0],
            vec![1 | 3 << 8, 0, 0],
        ] {
            let st = run(&[rec(1, &payload).bytes()], 1);
            assert_eq!((st.unknown, st.observed.len()), (1, 0), "{payload:x?}");
        }
        let mut r = rec(1, &[RESET]).bytes();
        r.extend_from_slice(&[1, 2]);
        put_u32(&mut r, 12, 96 + 6).unwrap();
        put_u32(&mut r, 64, 6).unwrap();
        let hash = cmd_hash(&r[96..]);
        put_u64(&mut r, 56, hash).unwrap();
        assert_eq!(run(&[r], 1).unknown, 1);
    }

    #[test]
    fn known_non_read_packets_are_skipped_by_length() {
        let nops = cat(&[
            vec![1 | 1 << 8, 0],                     // WaitOnAddress, 2
            vec![2],                                 // WaitOnCounter, 1
            vec![5 | 1 << 8, 0],                     // WriteAddress, 2
            vec![0x75 | 3 << 8, 0, 0, 0],            // WriteAddress, 4
            vec![6 | 2 << 8, 0, 0],                  // WriteCounter, 3
            vec![0x408, 0, 0, 0, 0],                 // WriteKernelEventQueue
            vec![0x5452_000f | 5 << 12 | 1 << 8, 7], // colour marker needs a dword
            vec![0x5452_300f],                       // MarkerPop
            vec![0x22d, 0, 0],                       // MapBegin
            vec![0x32d, 0, 0, 0],                    // MapDirectBegin
            vec![46],                                // MapEnd
            vec![0x221, 0, 0],                       // AMM Map
            vec![0x425, 0, 0, 0, 0],                 // AMM MapDirect
            read_file(1, 5, 0),
        ]);
        let mut r = rec(1, &nops);
        r.count = 14;
        let st = run(&[r.bytes()], 1);
        assert_eq!((st.unknown, st.count_mismatch), (0, 0));
        assert_eq!(st.observed, BTreeSet::from([1]));
        let mut r = rec(1, &nops);
        r.count = 3;
        assert_eq!(run(&[r.bytes()], 1).count_mismatch, 1);
        // The colour marker with no payload dword is invalid.
        assert_eq!(
            run(&[rec(1, &[0x5452_000f | 5 << 12]).bytes()], 1).unknown,
            1
        );
    }

    #[test]
    fn sequence_gaps_and_backward_jumps_clear_state() {
        let rf = read_file(1, 10, 0);
        // Normal flow: state carries across records.
        let st = run(&[rec(1, &rf).bytes(), rec(2, &gather(5)).bytes()], 1);
        assert_eq!((st.reads, st.requested_bytes, st.gaps), (2, 15, 0));
        // A gap drops it (and is counted); so does going backwards; so does a first record not 1.
        for seqs in [[1, 3], [2, 1]] {
            let st = run(
                &[rec(seqs[0], &rf).bytes(), rec(seqs[1], &gather(5)).bytes()],
                1,
            );
            assert_eq!(st.stateless_reads, 1, "{seqs:?}");
            assert!(st.gaps >= 1, "{seqs:?}");
        }
        assert_eq!(run(&[rec(5, &rf).bytes()], 1).gaps, 1);
        // The record that follows a gap still decodes.
        let st = run(&[rec(1, &rf).bytes(), rec(9, &rf).bytes()], 1);
        assert_eq!((st.gaps, st.reads), (1, 2));
    }

    #[test]
    fn inherited_state_is_per_priority() {
        let mut a = rec(1, &read_file(1, 10, 0));
        a.priority = 3;
        let mut b = rec(2, &cat(&[gather(7), scatter(5), gs(2)]));
        b.priority = 4; // no state for priority 4
        let mut c = rec(3, &cat(&[gather(7), scatter(5), gs(2)]));
        c.priority = 3;
        let mut d = rec(4, &cat(&[vec![RESET], gather(1)]));
        d.priority = 3;
        let mut e = rec(5, &gather(1)); // after reset: stateless
        e.priority = 3;
        let st = run(&[a.bytes(), b.bytes(), c.bytes(), d.bytes(), e.bytes()], 1);
        assert_eq!(st.stateless_reads, 3 + 1 + 1);
        assert_eq!(st.reads, 1 + 3);
        assert_eq!(st.requested_bytes, 10 + 7 + 5 + 2);
        assert_eq!(st.observed, BTreeSet::from([1]));
    }

    #[test]
    fn bad_hash_clears_state() {
        let mut bad = rec(2, &[RESET]).bytes();
        *bad.last_mut().unwrap() ^= 1;
        let st = run(
            &[
                rec(1, &read_file(1, 10, 0)).bytes(),
                bad,
                rec(3, &gather(5)).bytes(),
            ],
            1,
        );
        assert_eq!((st.bad_hash, st.stateless_reads, st.gaps), (1, 1, 0));
    }

    #[test]
    fn other_domains_are_skipped_by_framing() {
        // The payload would be a ReadFile if it were read as APR.
        let mut o = rec(2, &read_file(2, 10, 0));
        o.domain = 2;
        let mut junk = rec(3, &[0xdead_beef]);
        junk.domain = 7;
        let st = run(
            &[
                rec(1, &read_file(1, 10, 0)).bytes(),
                o.bytes(),
                junk.bytes(),
                rec(4, &gather(5)).bytes(),
            ],
            2,
        );
        assert_eq!(st.observed, BTreeSet::from([1]));
        assert_eq!(
            (st.other_domain, st.records, st.gaps, st.unknown),
            (2, 2, 0, 0)
        );
        // State survives them.
        assert_eq!(st.reads, 2);
    }

    #[test]
    fn extended_header_is_skipped() {
        let mut r = rec(1, &read_file(1, 10, 0));
        r.extra = 24;
        let st = run(&[r.bytes(), rec(2, &read_file(2, 1, 0)).bytes()], 2);
        assert_eq!(st.observed, BTreeSet::from([1, 2]));
        assert_eq!((st.unparsed_tail, st.truncated_tail), (0, 0));
    }

    #[test]
    fn cancel_stops_with_cancelled() {
        let b = rec(1, &read_file(1, 10, 0)).bytes();
        let err = scan(&b[..], 1, &AtomicBool::new(true)).unwrap_err();
        assert!(matches!(err, Error::Cancelled));
    }

    #[test]
    fn io_errors_propagate_and_short_reads_work() {
        struct OneByte<'a>(&'a [u8]);
        impl Read for OneByte<'_> {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let n = self.0.len().min(1).min(buf.len());
                buf[..n].copy_from_slice(&self.0[..n]);
                self.0 = &self.0[n..];
                Ok(n)
            }
        }
        let b = [
            rec(1, &read_file(1, 10, 0)).bytes(),
            rec(2, &read_file(2, 10, 0)).bytes(),
        ]
        .concat();
        let st = scan(OneByte(&b), 2, &AtomicBool::new(false)).unwrap();
        assert_eq!(st.observed, BTreeSet::from([1, 2]));
        struct Fail;
        impl Read for Fail {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("boom"))
            }
        }
        assert!(matches!(
            scan(Fail, 1, &AtomicBool::new(false)),
            Err(Error::Io(_))
        ));
    }

    #[test]
    fn a_huge_claimed_payload_on_a_short_stream_does_not_allocate_it() {
        let mut b = rec(1, &[RESET]).bytes();
        put_u32(&mut b, 12, MAX_RECORD_BYTES).unwrap();
        put_u32(&mut b, 64, MAX_RECORD_BYTES - 96).unwrap();
        let st = scan(&b[..], 1, &AtomicBool::new(false)).unwrap();
        assert_eq!(st.truncated_tail, b.len() as u64);
    }

    #[test]
    fn payload_buffer_grows_with_arrived_bytes_only() {
        // 64 MiB claimed, 100 bytes delivered: the buffer stays chunk-sized.
        let mut p = Vec::new();
        let n = read_payload(
            &mut &[7u8; 100][..],
            &mut p,
            64 << 20,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(n, 100);
        assert!(p.capacity() <= 1 << 17, "capacity {}", p.capacity());
        // A full delivery still reads everything.
        let n = read_payload(
            &mut &vec![1u8; 200_000][..],
            &mut p,
            200_000,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!((n, p.len()), (200_000, 200_000));
        // Through scan: 1 MiB delivered of a 64 MiB claim.
        let mut b = rec(1, &[RESET]).bytes();
        put_u32(&mut b, 12, MAX_RECORD_BYTES).unwrap();
        put_u32(&mut b, 64, MAX_RECORD_BYTES - 96).unwrap();
        b.resize(96 + (1 << 20), 0);
        let st = scan(&b[..], 1, &AtomicBool::new(false)).unwrap();
        assert_eq!(st.truncated_tail, b.len() as u64);
    }

    #[test]
    fn sequence_exhaustion_is_a_discontinuity() {
        let st = run(
            &[
                rec(u64::MAX, &read_file(1, 8, 0)).bytes(),
                rec(0, &gather(8)).bytes(),
            ],
            1,
        );
        // Both records are gaps; the wrapped 0 must not inherit state.
        assert_eq!((st.gaps, st.stateless_reads, st.reads), (2, 1, 1));
    }

    /// Endless zeros that raise `cancel` after a few reads and fail if read too long.
    struct Endless<'a> {
        reads: usize,
        head: Vec<u8>,
        cancel: &'a AtomicBool,
    }
    impl Read for Endless<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.reads += 1;
            if self.reads == 5 {
                self.cancel.store(true, Ordering::Relaxed);
            }
            if self.reads > 10_000 {
                return Err(std::io::Error::other("cancel ignored"));
            }
            let k = self.head.len().min(buf.len());
            buf[..k].copy_from_slice(&self.head[..k]);
            self.head.drain(..k);
            buf[k..].fill(0);
            Ok(buf.len())
        }
    }

    #[test]
    fn cancel_stops_draining_a_damaged_tail() {
        let cancel = AtomicBool::new(false);
        let r = Endless {
            reads: 0,
            head: vec![0xAA; CMD_HEADER],
            cancel: &cancel,
        };
        assert!(matches!(scan(r, 1, &cancel), Err(Error::Cancelled)));
    }
}
