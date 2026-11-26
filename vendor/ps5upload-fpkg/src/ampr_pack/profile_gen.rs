// Ported from drakmor's ampr_emu tools (https://github.com/drakmor/ampr_emu, GPL-3.0). This
// Rust port is part of ps5upload, GPL-3.0-or-later.
//! A profile measured from how a game reads its files: the port of drakmor's
//! `tools/ampr_pack_profile.py` (ampr_emu, GPL-3.0) that turns `AMPRCMD1` command journals
//! (what the debug build of ampr_emu writes to `/app0/ampr_commands.bin` while a game runs)
//! and the game's `AMPRIDX3` index into the pack profile [`super::config::Config`] reads.
//!
//! The reference tool's "exact" mode is ported line for line, with everything the engine
//! does not need left out: directory generalisation, content sampling, externalised include
//! lists, the markdown report and the batch mode. `tests/fixtures/ampr_trace` holds a
//! synthetic trace and the reference tool's output for it; the test compares byte for byte.
//!
//! What it decides, per file the game read: the block size (16 KiB to 1 MiB, scored on
//! read amplification and operations per read), the layout (`streaming`, `random`,
//! `mixed`), whether it is `hot` (kept decoded), and the lane group. Then a resident index
//! budget trades block sizes up where it costs the least, an LRU simulation over the reads
//! sizes the decoded cache, and the remaining runtime figures follow from the read mix.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet};
use std::path::Path;

const CHUNK_RECORD_BYTES: u64 = 12;
const FILE_RECORD_BYTES: u64 = 48;
const PACK_RECORD_BYTES: u64 = 32;
const DEFAULT_IO_PAGE: u64 = 64 * 1024;
const DEFAULT_INDEX_BUDGET: u64 = 96 * 1024 * 1024;
const RUNTIME_PIPELINE_SLOTS: u64 = 32;
const RUNTIME_PIPELINE_IO_BYTES: u64 = 2 * 1024 * 1024;
const RUNTIME_WORKER_SCRATCH_BYTES: u64 = 1024 * 1024;
const RUNTIME_POST_CACHE_RESERVE_BYTES: u64 = 32 * 1024 * 1024;
const DEFAULT_CACHE_MAX_TOUCHES: u64 = 5_000_000;
const DEFAULT_CACHE_CANDIDATES: [u64; 5] = [
    32 * 1024 * 1024,
    64 * 1024 * 1024,
    128 * 1024 * 1024,
    256 * 1024 * 1024,
    512 * 1024 * 1024,
];
const BLOCK_SIZES: [u64; 7] = [
    16 * 1024,
    32 * 1024,
    64 * 1024,
    128 * 1024,
    256 * 1024,
    512 * 1024,
    1024 * 1024,
];
const KIB64: u64 = 64 * 1024;
const MAX_RULE_FILES: usize = 256;
const KNOWN_LOOSE_BASENAMES: [&str; 7] = [
    "eboot.bin",
    "ampr_emu.index",
    "ampr_assets.index",
    "ampr_assets.index.crc",
    "ampr_assets.index.runtime",
    "param.sfo",
    "nptitle.dat",
];
const KNOWN_LOOSE_SUFFIXES: [&str; 4] = [".prx", ".sprx", ".self", ".elf"];
const KNOWN_STREAM_STORE_SUFFIXES: [&str; 15] = [
    ".bik", ".bk2", ".mp4", ".m4v", ".webm", ".m2v", ".mpg", ".mpeg", ".avi", ".wem", ".opus",
    ".ogg", ".mp3", ".aac", ".flac",
];
const STANDARD_LOOSE_PATTERNS: [&str; 12] = [
    "sce_module/**",
    "system/**",
    "mods/**",
    "save/**",
    "**/*.prx",
    "**/*.sprx",
    "eboot.bin",
    "ampr_emu.index",
    "ampr_assets.index",
    "ampr_assets.index.crc",
    "ampr_assets.index.runtime",
    "ampr_assets-*.pak",
];
/// The reference tool's version, named in the generated file's first line.
const TOOL_VERSION: &str = "4.1";

const COMMAND_LOG_MAGIC: &[u8; 8] = b"AMPRCMD1";
const COMMAND_LOG_HEADER_SIZE: usize = 96;

/// One trace: the journal and the index the game ran with.
#[derive(Debug, Clone)]
pub struct TraceSpec {
    pub name: String,
    pub commands: std::path::PathBuf,
    pub index: std::path::PathBuf,
}

/// What generating produced.
#[derive(Debug, Clone)]
pub struct Generated {
    pub toml: String,
    pub summary: Summary,
}

/// The figures a UI shows about a generated profile.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Summary {
    pub traces: usize,
    pub records: u64,
    pub reads: u64,
    /// Files the game was seen reading, and how many of them the profile packs.
    pub files_observed: usize,
    pub files_packed: usize,
    pub decoded_cache_bytes: u64,
    pub physical_cache_bytes: u64,
    pub runtime_workers: u32,
    pub latency_reserve_workers: u32,
    pub projected_index_bytes: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct ReadEvent {
    trace_ordinal: usize,
    sequence: u64,
    command_ordinal: u64,
    relative: String,
    file_size: u64,
    offset: u64,
    length: u64,
}

impl ReadEvent {
    fn end(&self) -> u64 {
        self.offset + self.length
    }
    fn order_key(&self) -> (usize, u64, u64) {
        (self.trace_ordinal, self.sequence, self.command_ordinal)
    }
}

#[derive(Debug, Clone)]
struct CandidateMetrics {
    repeat_ratio: f64,
    metadata_bytes: u64,
    local_score: f64,
}

#[derive(Debug, Clone)]
struct FileMetrics {
    relative: String,
    file_size: u64,
    read_count: usize,
    requested_bytes: u64,
    p50: f64,
    p90: f64,
    tiny_4k_ratio: f64,
    small_16k_ratio: f64,
    small_64k_ratio: f64,
    large_256k_ratio: f64,
    exact_sequential_ratio: f64,
    near_sequential_ratio: f64,
    random_seek_ratio: f64,
    candidates: BTreeMap<u64, CandidateMetrics>,
    confidence: f64,
    confidence_label: &'static str,
}

#[derive(Debug, Clone)]
struct Recommendation {
    metrics_index: usize,
    action: &'static str,
    layout: &'static str,
    block_size: u64,
    hot: bool,
    group: &'static str,
}

impl Recommendation {
    fn profile_key(&self) -> (String, String, u64, bool, String) {
        (
            self.action.to_string(),
            self.layout.to_string(),
            self.block_size,
            self.hot,
            self.group.to_string(),
        )
    }
    fn metadata_bytes(&self, files: &[FileMetrics]) -> u64 {
        if self.action == "loose" {
            return 0;
        }
        files[self.metrics_index]
            .file_size
            .div_ceil(self.block_size)
            * CHUNK_RECORD_BYTES
    }
}

#[derive(Debug, Clone)]
struct CacheSimulation {
    capacity: u64,
    requested_bytes: u64,
    hit_bytes: u64,
}

impl CacheSimulation {
    fn hit_ratio(&self) -> f64 {
        if self.requested_bytes == 0 {
            0.0
        } else {
            self.hit_bytes as f64 / self.requested_bytes as f64
        }
    }
}

#[derive(Debug, Clone)]
struct Group {
    pack_count: u64,
    max_pack_size: u64,
}

// ---------------------------------------------------------------- helpers --

fn percentile(values: &[u64], pct: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    if values.len() == 1 {
        return values[0] as f64;
    }
    let mut ordered = values.to_vec();
    ordered.sort_unstable();
    let position = (ordered.len() - 1) as f64 * pct / 100.0;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        return ordered[lower] as f64;
    }
    let fraction = position - lower as f64;
    ordered[lower] as f64 * (1.0 - fraction) + ordered[upper] as f64 * fraction
}

fn union_length(mut ranges: Vec<(u64, u64)>) -> u64 {
    ranges.sort_unstable();
    let mut total = 0u64;
    let mut current: Option<(u64, u64)> = None;
    for (start, end) in ranges {
        if end <= start {
            continue;
        }
        match current {
            None => current = Some((start, end)),
            Some((cs, ce)) => {
                if start <= ce {
                    current = Some((cs, ce.max(end)));
                } else {
                    total += ce - cs;
                    current = Some((start, end));
                }
            }
        }
    }
    if let Some((cs, ce)) = current {
        total += ce - cs;
    }
    total
}

fn human_size(value: f64) -> String {
    let mut size = value;
    for suffix in ["B", "KiB", "MiB", "GiB", "TiB"] {
        if size.abs() < 1024.0 || suffix == "TiB" {
            if suffix == "B" {
                return format!("{} B", round_half_even(size) as i64);
            }
            return format!("{size:.2} {suffix}");
        }
        size /= 1024.0;
    }
    format!("{size:.2} TiB")
}

/// Python's `round()`: half to even.
fn round_half_even(v: f64) -> f64 {
    let r = v.round();
    if (v - v.trunc()).abs() == 0.5 && r % 2.0 != 0.0 {
        r - v.signum()
    } else {
        r
    }
}

fn size_literal(value: u64) -> String {
    for (divisor, suffix) in [
        (1u64 << 30, "GiB"),
        (1u64 << 20, "MiB"),
        (1u64 << 10, "KiB"),
    ] {
        if value != 0 && value.is_multiple_of(divisor) {
            return format!("{}{suffix}", value / divisor);
        }
    }
    format!("{value}B")
}

fn toml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| format!("\"{value}\""))
}

fn canonical_relative(path: &str) -> Result<String, String> {
    let mut value = path.replace('\\', "/");
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("/app0/") {
        value = value[6..].to_string();
    } else if lower == "/app0" {
        value = String::new();
    }
    let value = value.trim_start_matches('/');
    let parts: Vec<&str> = value.split('/').collect();
    if parts
        .iter()
        .any(|p| p.is_empty() || *p == "." || *p == "..")
    {
        return Err(format!("unsafe indexed path: {path:?}"));
    }
    Ok(parts.join("/"))
}

fn fnv1a64(data: &[u8]) -> u64 {
    let mut h: u64 = 1469598103934665603;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    if h == 0 {
        1
    } else {
        h
    }
}

fn u32_at(d: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([d[off], d[off + 1], d[off + 2], d[off + 3]])
}

fn need(d: &[u8], off: usize, dwords: usize) -> bool {
    dwords > 0 && off + dwords * 4 <= d.len()
}

/// One decoded command: its size in dwords and, for an APR read, what it reads.
enum Cmd {
    ReadFile {
        dwords: usize,
        file_id: u32,
        file_offset: u64,
        length: u64,
    },
    ReadGather {
        dwords: usize,
        file_offset: u64,
        length: u64,
    },
    ReadScatter {
        dwords: usize,
        length: u64,
    },
    ReadGatherScatter {
        dwords: usize,
        file_offset: u64,
        length: u64,
    },
    ResetGatherScatter,
    Other {
        dwords: usize,
    },
}

/// `decode_command` of the reference parser, kept to what sizes the stream and what reads.
fn decode_command(d: &[u8], off: usize) -> Result<Cmd, String> {
    if off & 3 != 0 || off + 4 > d.len() {
        return Err("unaligned or truncated command".into());
    }
    let w0 = u32_at(d, off);
    let opcode8 = w0 & 0xFF;
    let opcode12 = w0 & 0xFFF;
    let other = |dwords: usize, lo: usize, hi: usize, what: &str| -> Result<Cmd, String> {
        if !(lo <= dwords && dwords <= hi && need(d, off, dwords)) {
            return Err(format!("invalid {what} length"));
        }
        Ok(Cmd::Other { dwords })
    };
    if opcode8 == 1 {
        let dwords = ((w0 >> 8) & 0xF) as usize + 1;
        return other(dwords, 2, 4, "WaitOnAddress");
    }
    if opcode8 == 2 {
        let dwords = ((w0 >> 8) & 0xF) as usize + 1;
        if dwords == 4 {
            return Err("invalid WaitOnCounter length".into());
        }
        return other(dwords, 1, 5, "WaitOnCounter");
    }
    if opcode8 == 5 || opcode8 == 117 {
        let dwords = ((w0 >> 8) & 3) as usize + 1;
        return other(dwords, 2, 4, "WriteAddress");
    }
    if opcode8 == 6 || opcode8 == 118 {
        let dwords = ((w0 >> 8) & 0xF) as usize + 1;
        return other(dwords, 1, 3, "WriteCounter");
    }
    if opcode12 == 1032 || opcode12 == 1144 {
        return other(5, 5, 5, "WriteKernelEventQueue");
    }
    if (w0 & 0xFFFF000F) == 0x5452000F {
        let dwords = ((w0 >> 8) & 0xF) as usize + 1;
        if !need(d, off, dwords) {
            return Err("truncated NOP/marker packet".into());
        }
        if w0 == 0x5452300F {
            return Ok(Cmd::Other { dwords: 1 });
        }
        let marker_type = (w0 >> 12) & 0xF;
        let payload_dwords = (w0 >> 8) & 0xF;
        if matches!(marker_type, 5 | 6) && payload_dwords < 1 {
            return Err("marker with color has no color dword".into());
        }
        return Ok(Cmd::Other { dwords });
    }
    if opcode8 == 40 {
        let dwords = ((w0 >> 8) & 7) as usize + 1;
        if !((5..=6).contains(&dwords) && need(d, off, dwords)) {
            return Err("invalid AprReadFile length".into());
        }
        let w4 = u32_at(d, off + 16);
        let mut file_offset = (((w0 >> 12) & 0x3FFFF) | (w4 & 0xFFFC0000)) as u64;
        if dwords >= 6 {
            file_offset |= ((u32_at(d, off + 20) & 0xFF) as u64) << 32;
        }
        return Ok(Cmd::ReadFile {
            dwords,
            length: u32_at(d, off + 4) as u64 + 1,
            file_id: u32_at(d, off + 8) & 0x7FFF_FFFF,
            file_offset,
        });
    }
    if opcode8 == 41 {
        let dwords = ((w0 >> 8) & 3) as usize + 1;
        if !((2..=3).contains(&dwords) && need(d, off, dwords)) {
            return Err("invalid AprReadGather length".into());
        }
        let mut file_offset = ((w0 >> 12) & 0x3FFFF) as u64;
        if dwords >= 3 {
            file_offset |= ((u32_at(d, off + 8) & 0x3FFFFF) as u64) << 18;
        }
        return Ok(Cmd::ReadGather {
            dwords,
            length: u32_at(d, off + 4) as u64 + 1,
            file_offset,
        });
    }
    if opcode12 == 0x22A {
        if !need(d, off, 3) {
            return Err("truncated AprReadScatter".into());
        }
        return Ok(Cmd::ReadScatter {
            dwords: 3,
            length: u32_at(d, off + 4) as u64 + 1,
        });
    }
    if opcode8 == 43 {
        let dwords = ((w0 >> 8) & 7) as usize + 1;
        if !((4..=5).contains(&dwords) && need(d, off, dwords)) {
            return Err("invalid AprReadGatherScatter length".into());
        }
        let w3 = u32_at(d, off + 12);
        let mut file_offset = (((w0 >> 12) & 0x3FFFF) | (w3 & 0xFFFC0000)) as u64;
        if dwords >= 5 {
            file_offset |= ((u32_at(d, off + 16) & 0xFF) as u64) << 32;
        }
        return Ok(Cmd::ReadGatherScatter {
            dwords,
            length: u32_at(d, off + 4) as u64 + 1,
            file_offset,
        });
    }
    if w0 == 47 {
        return Ok(Cmd::ResetGatherScatter);
    }
    if opcode12 == 557 {
        return other(3, 3, 3, "AprMapBegin");
    }
    if opcode12 == 813 {
        return other(4, 4, 4, "AprMapDirectBegin");
    }
    if w0 == 46 {
        return Ok(Cmd::Other { dwords: 1 });
    }
    // AMM commands: only their sizes matter here.
    let amm = match opcode12 {
        0x221 => Some(3),
        0x321 => Some(4),
        0x325 => Some(4),
        0x425 => Some(5),
        0x222 | 0x228 => Some(3),
        0x323 | 0x324 | 0x327 => Some(4),
        0x423 | 0x424 => Some(5),
        0x326 => Some(4),
        0x426 => Some(5),
        _ => None,
    };
    if let Some(dwords) = amm {
        if need(d, off, dwords) {
            return Ok(Cmd::Other { dwords });
        }
        // The reference decoder falls through to "Unknown" when an AMM command is cut.
    }
    let mut guessed = ((w0 >> 8) & 0xF) as usize + 1;
    if !need(d, off, guessed) {
        guessed = 1;
    }
    Ok(Cmd::Other { dwords: guessed })
}

struct GatherState {
    file_id: u32,
    next_file_offset: u64,
}

/// The index as the reference tool loads it: file id (1-based) → (relative path, size).
fn load_index(path: &Path) -> Result<HashMap<u32, (String, u64)>, String> {
    let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let entries = crate::ampr_index::parse(&data)
        .ok_or_else(|| format!("{} is not an AMPRIDX3 index", path.display()))?;
    Ok(entries
        .into_iter()
        .enumerate()
        .map(|(i, e)| (i as u32 + 1, (e.path, e.size)))
        .collect())
}

struct TraceLoad {
    events: Vec<ReadEvent>,
    records: u64,
    reads: u64,
    path_sizes: HashMap<String, u64>,
    warnings: Vec<String>,
}

fn stream_one_trace(
    spec: &TraceSpec,
    trace_ordinal: usize,
    index: &HashMap<u32, (String, u64)>,
) -> Result<TraceLoad, String> {
    let data =
        std::fs::read(&spec.commands).map_err(|e| format!("{}: {e}", spec.commands.display()))?;
    let mut events = Vec::new();
    let mut path_sizes: HashMap<String, u64> = HashMap::new();
    let mut warnings = Vec::new();
    let mut gather: HashMap<u32, GatherState> = HashMap::new();
    let mut records = 0u64;
    let mut reads = 0u64;
    let mut previous_sequence: Option<u64> = None;
    let mut file_offset = 0usize;
    let warn = |warnings: &mut Vec<String>, m: String| {
        if warnings.len() < 200 {
            warnings.push(m);
        }
    };
    while file_offset < data.len() {
        if file_offset + COMMAND_LOG_HEADER_SIZE > data.len() {
            warn(
                &mut warnings,
                format!(
                    "truncated header at 0x{file_offset:x}: {}/{COMMAND_LOG_HEADER_SIZE}",
                    data.len() - file_offset
                ),
            );
            break;
        }
        let h = &data[file_offset..file_offset + COMMAND_LOG_HEADER_SIZE];
        if &h[0..8] != COMMAND_LOG_MAGIC {
            return Err(format!(
                "{}: invalid command-log magic at 0x{file_offset:x}",
                spec.name
            ));
        }
        let version = u16::from_le_bytes([h[8], h[9]]);
        let header_bytes = u16::from_le_bytes([h[10], h[11]]) as usize;
        let record_bytes = u32_at(h, 12) as usize;
        let sequence = u64::from_le_bytes(h[16..24].try_into().unwrap());
        let payload_hash = u64::from_le_bytes(h[56..64].try_into().unwrap());
        let payload_bytes = u32_at(h, 64) as usize;
        let command_count = u32_at(h, 72);
        let priority = u32_at(h, 76);
        let domain = u32_at(h, 80);
        if version != 1 {
            return Err(format!(
                "{}: unsupported command-log version {version}",
                spec.name
            ));
        }
        if header_bytes < COMMAND_LOG_HEADER_SIZE {
            return Err(format!(
                "{}: invalid header size {header_bytes} at 0x{file_offset:x}",
                spec.name
            ));
        }
        if record_bytes < header_bytes || record_bytes - header_bytes != payload_bytes {
            return Err(format!(
                "{}: inconsistent record sizes at 0x{file_offset:x}",
                spec.name
            ));
        }
        let payload_start = file_offset + header_bytes;
        if payload_start + payload_bytes > data.len() {
            warn(
                &mut warnings,
                format!(
                    "truncated payload for seq={sequence}: {}/{payload_bytes}",
                    data.len().saturating_sub(payload_start)
                ),
            );
            break;
        }
        let payload = &data[payload_start..payload_start + payload_bytes];
        records += 1;
        match previous_sequence {
            None if sequence != 1 => warn(
                &mut warnings,
                format!("sequence starts at {sequence}, expected 1"),
            ),
            Some(p) if sequence != p + 1 => warn(
                &mut warnings,
                format!("sequence gap/non-monotonic: previous={p}, current={sequence}"),
            ),
            _ => {}
        }
        previous_sequence = Some(sequence);
        if fnv1a64(payload) != payload_hash {
            warn(
                &mut warnings,
                format!("payload hash mismatch for seq={sequence}"),
            );
        }
        if domain != 1 {
            file_offset += record_bytes;
            continue;
        }
        let mut off = 0usize;
        let mut ordinal = 0u64;
        while off < payload.len() {
            let cmd = match decode_command(payload, off) {
                Ok(c) => c,
                Err(e) => {
                    warn(
                        &mut warnings,
                        format!("seq={sequence} decode error at 0x{off:x}: {e}"),
                    );
                    break;
                }
            };
            let dwords = match &cmd {
                Cmd::ReadFile { dwords, .. }
                | Cmd::ReadGather { dwords, .. }
                | Cmd::ReadScatter { dwords, .. }
                | Cmd::ReadGatherScatter { dwords, .. }
                | Cmd::Other { dwords } => *dwords,
                Cmd::ResetGatherScatter => 1,
            };
            let command_bytes = dwords * 4;
            if command_bytes == 0 || off + command_bytes > payload.len() {
                warn(
                    &mut warnings,
                    format!("seq={sequence} decoder returned invalid size {command_bytes}"),
                );
                break;
            }
            let mut emit = |file_id: u32, read_offset: u64, mut length: u64| {
                let Some((path, size)) = index.get(&file_id) else {
                    warn(
                        &mut warnings,
                        format!("fileId {file_id} absent from AMPRIDX3 at seq={sequence}"),
                    );
                    return;
                };
                let relative = match canonical_relative(path) {
                    Ok(r) => r,
                    Err(e) => {
                        warn(&mut warnings, e);
                        return;
                    }
                };
                if length == 0 {
                    return;
                }
                if read_offset >= *size {
                    warn(
                        &mut warnings,
                        format!("read beyond {relative}: offset=0x{read_offset:x} size=0x{size:x}"),
                    );
                    return;
                }
                if length > size - read_offset {
                    warn(
                        &mut warnings,
                        format!(
                            "clamped read for {relative}: offset=0x{read_offset:x} \
                             length=0x{length:x} size=0x{size:x}"
                        ),
                    );
                    length = size - read_offset;
                }
                path_sizes.entry(relative.clone()).or_insert(*size);
                events.push(ReadEvent {
                    trace_ordinal,
                    sequence,
                    command_ordinal: ordinal,
                    relative,
                    file_size: *size,
                    offset: read_offset,
                    length,
                });
                reads += 1;
            };
            match cmd {
                Cmd::ReadFile {
                    file_id,
                    file_offset: fo,
                    length,
                    ..
                } => {
                    gather.insert(
                        priority,
                        GatherState {
                            file_id,
                            next_file_offset: fo + length,
                        },
                    );
                    emit(file_id, fo, length);
                }
                Cmd::ReadGather {
                    file_offset: fo,
                    length,
                    ..
                } => match gather.get_mut(&priority) {
                    None => warn(
                        &mut warnings,
                        format!("AprReadGather without active state at seq={sequence}"),
                    ),
                    Some(s) => {
                        let id = s.file_id;
                        s.next_file_offset = fo + length;
                        emit(id, fo, length);
                    }
                },
                Cmd::ReadScatter { length, .. } => match gather.get_mut(&priority) {
                    None => warn(
                        &mut warnings,
                        format!("AprReadScatter without active state at seq={sequence}"),
                    ),
                    Some(s) => {
                        let id = s.file_id;
                        let fo = s.next_file_offset;
                        s.next_file_offset = fo + length;
                        emit(id, fo, length);
                    }
                },
                Cmd::ReadGatherScatter {
                    file_offset: fo,
                    length,
                    ..
                } => match gather.get_mut(&priority) {
                    None => warn(
                        &mut warnings,
                        format!("AprReadGatherScatter without active state at seq={sequence}"),
                    ),
                    Some(s) => {
                        let id = s.file_id;
                        s.next_file_offset = fo + length;
                        emit(id, fo, length);
                    }
                },
                Cmd::ResetGatherScatter => {
                    gather.remove(&priority);
                }
                Cmd::Other { .. } => {}
            }
            off += command_bytes;
            ordinal += 1;
        }
        if command_count != 0 && ordinal != command_count as u64 {
            warn(
                &mut warnings,
                format!("seq={sequence} header command_count={command_count}, decoded={ordinal}"),
            );
        }
        file_offset += record_bytes;
    }
    Ok(TraceLoad {
        events,
        records,
        reads,
        path_sizes,
        warnings,
    })
}

fn candidate_metrics(
    events: &[ReadEvent],
    file_size: u64,
    block_size: u64,
    layout: &str,
    randomness: f64,
) -> CandidateMetrics {
    let requested: u64 = events.iter().map(|e| e.length).sum();
    let mut touched = 0u64;
    let mut total_touches = 0u64;
    let mut ranges = Vec::with_capacity(events.len());
    for e in events {
        let first = e.offset / block_size;
        let last_exclusive = e.end().div_ceil(block_size);
        let start = first * block_size;
        let end = (last_exclusive * block_size).min(file_size);
        touched += end.saturating_sub(start);
        total_touches += last_exclusive.saturating_sub(first);
        ranges.push((first, last_exclusive));
    }
    let unique_blocks = union_length(ranges);
    let amplification = if requested != 0 {
        touched as f64 / requested as f64
    } else {
        1.0
    };
    let average_blocks = if events.is_empty() {
        0.0
    } else {
        total_touches as f64 / events.len() as f64
    };
    let repeat_ratio = if total_touches != 0 {
        (1.0 - unique_blocks as f64 / total_touches as f64).max(0.0)
    } else {
        0.0
    };
    let metadata_bytes = file_size.div_ceil(block_size) * CHUNK_RECORD_BYTES;
    let log_amp = amplification.max(1.0).log2();
    let log_ops = average_blocks.max(1.0).log2();
    let size_log = (block_size as f64 / KIB64 as f64).log2();
    let bs = block_size as f64;
    let mut score = match layout {
        "random" => {
            let mut s = 3.8 * log_amp + 0.12 * log_ops;
            if block_size > 64 * 1024 {
                s += 0.55 * (bs / (64.0 * 1024.0)).log2();
            }
            if block_size < 16 * 1024 {
                s += 10.0;
            }
            s
        }
        "streaming" => {
            let mut s = 0.65 * log_amp + 0.95 * log_ops;
            if block_size < 128 * 1024 {
                s += 0.35 * ((128.0 * 1024.0) / bs).log2();
            }
            if block_size > 512 * 1024 {
                s += 0.15 * (bs / (512.0 * 1024.0)).log2();
            }
            s
        }
        _ => {
            let mut s = 2.15 * log_amp + 0.38 * log_ops;
            s += size_log.abs() * 0.06;
            if block_size > 128 * 1024 {
                s += 0.28 * (bs / (128.0 * 1024.0)).log2();
            }
            if block_size > 64 * 1024 {
                s += randomness * 0.42 * (bs / (64.0 * 1024.0)).log2();
            }
            if block_size < 32 * 1024 {
                s += 0.20 * ((32.0 * 1024.0) / bs).log2();
            }
            s
        }
    };
    score += (metadata_bytes as f64 / file_size.max(1) as f64) * 3.0;
    CandidateMetrics {
        repeat_ratio,
        metadata_bytes,
        local_score: score,
    }
}

#[allow(clippy::too_many_arguments)]
fn classify_layout(
    p50: f64,
    p90: f64,
    tiny_4k: f64,
    small_16k: f64,
    small_64k: f64,
    large_256k: f64,
    exact: f64,
    near: f64,
    random: f64,
    repeat_64k: f64,
    read_count: usize,
) -> &'static str {
    let sequential = exact + near;
    let micro_index =
        p50 <= 256.0 && read_count >= 64 && (repeat_64k >= 0.10 || sequential >= 0.70);
    if micro_index {
        return "random";
    }
    if p50 >= 64.0 * 1024.0 && sequential >= 0.76 && random <= 0.20 && repeat_64k <= 0.18 {
        return "streaming";
    }
    if p90 >= 256.0 * 1024.0 && large_256k >= 0.65 && sequential >= 0.86 && repeat_64k <= 0.12 {
        return "streaming";
    }
    if tiny_4k >= 0.50
        || small_16k >= 0.70
        || (small_64k >= 0.82 && random >= 0.30)
        || (repeat_64k >= 0.28 && p50 <= 64.0 * 1024.0)
    {
        return "random";
    }
    "mixed"
}

fn build_file_metrics(events: &[ReadEvent]) -> Vec<FileMetrics> {
    let mut grouped: BTreeMap<String, Vec<ReadEvent>> = BTreeMap::new();
    for e in events {
        grouped
            .entry(e.relative.clone())
            .or_default()
            .push(e.clone());
    }
    let mut results = Vec::new();
    for (relative, mut file_events) in grouped {
        file_events.sort_by_key(|e| e.order_key());
        let lengths: Vec<u64> = file_events.iter().map(|e| e.length).collect();
        let requested: u64 = lengths.iter().sum();
        let file_size = file_events.iter().map(|e| e.file_size).max().unwrap_or(0);
        let (mut transition, mut exact, mut near, mut random) = (0u64, 0u64, 0u64, 0u64);
        let mut previous_by_trace: HashMap<usize, ReadEvent> = HashMap::new();
        for e in &file_events {
            let previous = previous_by_trace.insert(e.trace_ordinal, e.clone());
            let Some(previous) = previous else { continue };
            let weight = e.length;
            transition += weight;
            if e.offset == previous.end() {
                exact += weight;
                continue;
            }
            let gap = e.offset as i128 - previous.end() as i128;
            let near_limit = DEFAULT_IO_PAGE.max(previous.length.min(512 * 1024)) as i128;
            if 0 <= gap && gap <= near_limit {
                near += weight;
            } else if e.offset < previous.offset {
                random += weight;
            } else if e.offset < previous.end() {
                // overlap: counted, not random
            } else {
                random += weight;
            }
        }
        let share = |pred: &dyn Fn(u64) -> bool| -> f64 {
            lengths.iter().filter(|l| pred(**l)).sum::<u64>() as f64 / requested as f64
        };
        let tiny_4k = share(&|l| l <= 4 * 1024);
        let small_16k = share(&|l| l <= 16 * 1024);
        let small_64k = share(&|l| l <= 64 * 1024);
        let large_256k = share(&|l| l >= 256 * 1024);
        let ratio = |v: u64| {
            if transition != 0 {
                v as f64 / transition as f64
            } else {
                0.0
            }
        };
        let (exact_r, near_r, random_r) = (ratio(exact), ratio(near), ratio(random));
        let p50 = percentile(&lengths, 50.0);
        let p90 = percentile(&lengths, 90.0);
        let provisional_64 = candidate_metrics(&file_events, file_size, KIB64, "mixed", random_r);
        let layout = classify_layout(
            p50,
            p90,
            tiny_4k,
            small_16k,
            small_64k,
            large_256k,
            exact_r,
            near_r,
            random_r,
            provisional_64.repeat_ratio,
            file_events.len(),
        );
        let candidates: BTreeMap<u64, CandidateMetrics> = BLOCK_SIZES
            .iter()
            .map(|&s| {
                (
                    s,
                    candidate_metrics(&file_events, file_size, s, layout, random_r),
                )
            })
            .collect();
        let evidence_reads = (((file_events.len() + 1) as f64).log10() / 3.0).min(1.0);
        let evidence_bytes = (requested as f64 / (64.0 * 1024.0 * 1024.0)).min(1.0);
        let trace_count = file_events
            .iter()
            .map(|e| e.trace_ordinal)
            .collect::<HashSet<_>>()
            .len();
        let multi_trace = ((trace_count.saturating_sub(1)) as f64 / 2.0).min(1.0);
        let confidence = 0.45 * evidence_reads + 0.35 * evidence_bytes + 0.20 * multi_trace;
        let confidence_label = if confidence >= 0.72 {
            "high"
        } else if confidence >= 0.42 {
            "medium"
        } else {
            "low"
        };
        results.push(FileMetrics {
            relative,
            file_size,
            read_count: file_events.len(),
            requested_bytes: requested,
            p50,
            p90,
            tiny_4k_ratio: tiny_4k,
            small_16k_ratio: small_16k,
            small_64k_ratio: small_64k,
            large_256k_ratio: large_256k,
            exact_sequential_ratio: exact_r,
            near_sequential_ratio: near_r,
            random_seek_ratio: random_r,
            candidates,
            confidence,
            confidence_label,
        });
    }
    results
}

fn known_action(relative: &str) -> &'static str {
    let name = relative
        .rsplit('/')
        .next()
        .unwrap_or(relative)
        .to_ascii_lowercase();
    let suffix = match name.rfind('.') {
        Some(i) if i > 0 => name[i..].to_string(),
        _ => String::new(),
    };
    if KNOWN_LOOSE_BASENAMES.contains(&name.as_str())
        || KNOWN_LOOSE_SUFFIXES.contains(&suffix.as_str())
    {
        return "loose";
    }
    if KNOWN_STREAM_STORE_SUFFIXES.contains(&suffix.as_str()) {
        return "store";
    }
    "compress"
}

fn initial_recommendations(files: &[FileMetrics]) -> Vec<Recommendation> {
    if files.is_empty() {
        return Vec::new();
    }
    let read_counts: Vec<u64> = files.iter().map(|f| f.read_count as u64).collect();
    let requested_sizes: Vec<u64> = files.iter().map(|f| f.requested_bytes).collect();
    let read_hot_threshold = (percentile(&read_counts, 90.0) as u64).max(32);
    let byte_hot_threshold = (percentile(&requested_sizes, 90.0) as u64).max(16 * 1024 * 1024);
    let mut out = Vec::new();
    for (i, m) in files.iter().enumerate() {
        if m.read_count < 1 || m.requested_bytes < 1 {
            continue;
        }
        let repeat_64k = m.candidates[&KIB64].repeat_ratio;
        let mut layout = classify_layout(
            m.p50,
            m.p90,
            m.tiny_4k_ratio,
            m.small_16k_ratio,
            m.small_64k_ratio,
            m.large_256k_ratio,
            m.exact_sequential_ratio,
            m.near_sequential_ratio,
            m.random_seek_ratio,
            repeat_64k,
            m.read_count,
        );
        let action = known_action(&m.relative);
        if action == "store" {
            layout = "streaming";
        }
        let mut hot = layout != "streaming"
            && ((m.read_count >= 16 && repeat_64k >= 0.22)
                || m.read_count as u64 >= read_hot_threshold
                || (m.read_count >= 8 && m.requested_bytes >= byte_hot_threshold));
        let allowed: Vec<u64> = BLOCK_SIZES
            .iter()
            .copied()
            .filter(|&s| match layout {
                "random" => s <= 128 * 1024,
                "mixed" => (32 * 1024..=256 * 1024).contains(&s),
                _ => s >= 64 * 1024,
            })
            .collect();
        let mut selected = allowed[0];
        let key = |s: u64| {
            (
                m.candidates[&s].local_score,
                (s as f64 / KIB64 as f64).log2().abs(),
                s,
            )
        };
        for &s in &allowed[1..] {
            let (a, b) = (key(s), key(selected));
            let less = a.0 < b.0 || (a.0 == b.0 && (a.1 < b.1 || (a.1 == b.1 && a.2 < b.2)));
            if less {
                selected = s;
            }
        }
        if m.p50 <= 256.0 && m.read_count >= 64 {
            selected = 16 * 1024;
            layout = "random";
            hot = true;
        }
        let group = if action == "loose" {
            "loose"
        } else if action == "store" || layout == "streaming" {
            "stream"
        } else if layout == "random" {
            "metadata"
        } else {
            "bulk"
        };
        out.push(Recommendation {
            metrics_index: i,
            action,
            layout,
            block_size: selected,
            hot,
            group,
        });
    }
    out
}

/// An `f64` the heap can order; the reference tool orders its heap by a float too.
#[derive(PartialEq)]
struct Ratio(f64);
impl Eq for Ratio {}
impl PartialOrd for Ratio {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Ratio {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0
            .partial_cmp(&other.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

fn enforce_index_budget(recs: &mut [Recommendation], files: &[FileMetrics], budget: u64) {
    let packed: Vec<usize> = (0..recs.len())
        .filter(|&i| recs[i].action != "loose")
        .collect();
    if packed.is_empty() {
        return;
    }
    let strategy_weight = 1.0; // balanced
    let mut heap: BinaryHeap<Reverse<(Ratio, u64, usize, u64)>> = BinaryHeap::new();
    let mut generations = vec![0u64; packed.len()];
    let push = |heap: &mut BinaryHeap<Reverse<(Ratio, u64, usize, u64)>>,
                recs: &[Recommendation],
                generations: &[u64],
                index: usize| {
        let r = &recs[packed[index]];
        let Some(pos) = BLOCK_SIZES.iter().position(|&s| s == r.block_size) else {
            return;
        };
        if pos + 1 >= BLOCK_SIZES.len() {
            return;
        }
        let next_size = BLOCK_SIZES[pos + 1];
        let m = &files[r.metrics_index];
        let current = &m.candidates[&r.block_size];
        let next = &m.candidates[&next_size];
        if next.metadata_bytes >= current.metadata_bytes {
            return;
        }
        let saved = (current.metadata_bytes - next.metadata_bytes) as f64;
        let score_cost = (next.local_score - current.local_score).max(0.0);
        let mut importance = 1.0;
        if r.hot {
            importance *= 2.5;
        }
        if r.layout == "random" {
            importance *= 1.8;
        } else if r.layout == "streaming" {
            importance *= 0.55;
        }
        importance *= 0.75 + 0.5 * m.confidence;
        let ratio = score_cost * importance * strategy_weight / saved;
        heap.push(Reverse((
            Ratio(ratio),
            generations[index],
            index,
            next_size,
        )));
    };
    for index in 0..packed.len() {
        push(&mut heap, recs, &generations, index);
    }
    let total = |recs: &[Recommendation]| -> u64 {
        packed.iter().map(|&i| recs[i].metadata_bytes(files)).sum()
    };
    while total(recs) > budget {
        let Some(Reverse((_ratio, generation, index, next_size))) = heap.pop() else {
            break;
        };
        if generation != generations[index] {
            continue;
        }
        if next_size <= recs[packed[index]].block_size {
            continue;
        }
        recs[packed[index]].block_size = next_size;
        generations[index] += 1;
        push(&mut heap, recs, &generations, index);
    }
}

fn estimate_index_bytes(
    recs: &[Recommendation],
    files: &[FileMetrics],
    pack_count: u64,
    index_paths: &BTreeMap<String, u64>,
) -> u64 {
    let chunk_bytes: u64 = recs
        .iter()
        .filter(|r| r.action != "loose")
        .map(|r| r.metadata_bytes(files))
        .sum();
    let file_bytes = index_paths.len() as u64 * FILE_RECORD_BYTES;
    let string_bytes: u64 = index_paths.keys().map(|p| p.len() as u64 + 1).sum();
    chunk_bytes + file_bytes + pack_count * PACK_RECORD_BYTES + string_bytes + 4096
}

fn choose_lanes(recs: &[Recommendation], files: &[FileMetrics]) -> u64 {
    let packed: Vec<&Recommendation> = recs.iter().filter(|r| r.action != "loose").collect();
    let total_size: u64 = packed
        .iter()
        .map(|r| files[r.metrics_index].file_size)
        .sum();
    let n = packed.len();
    if n <= 1 {
        return 1;
    }
    if total_size < 2 * (1 << 30) && n < 64 {
        return 2;
    }
    if total_size < 16 * (1 << 30) && n < 1024 {
        return 3;
    }
    4
}

fn build_groups(
    recs: &[Recommendation],
    files: &[FileMetrics],
    lanes: u64,
) -> Vec<(&'static str, Group)> {
    let mut out = Vec::new();
    for name in ["metadata", "bulk", "stream"] {
        let items: Vec<&Recommendation> = recs
            .iter()
            .filter(|r| r.action != "loose" && r.group == name)
            .collect();
        if items.is_empty() {
            continue;
        }
        let total_size: u64 = items.iter().map(|r| files[r.metrics_index].file_size).sum();
        let n = items.len() as u64;
        let (pack_count, max_pack_size) = match name {
            "bulk" => (lanes.min(n.max(1)), 16u64 << 30),
            "metadata" => {
                let cap = if total_size < 8 * (1 << 30) { 2 } else { 4 };
                (lanes.min(cap).max(1).min(n), 8u64 << 30)
            }
            _ => (lanes.clamp(1, 2).min(n), 16u64 << 30),
        };
        out.push((
            name,
            Group {
                pack_count,
                max_pack_size,
            },
        ));
    }
    out
}

struct AccessStream {
    keys: Vec<u64>,
    raw_sizes: Vec<u64>,
    overlaps: Vec<u64>,
    hot: Vec<bool>,
    reset_before: Vec<bool>,
}

fn block_overlaps(e: &ReadEvent, block_size: u64) -> Vec<(u64, u64, u64)> {
    let first = e.offset / block_size;
    let last = (e.end() - 1) / block_size;
    let mut out = Vec::with_capacity((last - first + 1) as usize);
    for block in first..=last {
        let start = block * block_size;
        let end = (start + block_size).min(e.file_size);
        let overlap = e.end().min(end).saturating_sub(e.offset.max(start));
        if overlap > 0 {
            out.push((block, end - start, overlap));
        }
    }
    out
}

fn touch_count(e: &ReadEvent, block_size: u64) -> u64 {
    if e.length == 0 || block_size == 0 {
        return 0;
    }
    (e.end() - 1) / block_size - e.offset / block_size + 1
}

fn build_access_stream(
    events: &[ReadEvent],
    by_path: &HashMap<&str, &Recommendation>,
    max_touches: u64,
) -> AccessStream {
    let mut eligible: Vec<(&ReadEvent, &Recommendation, u64)> = Vec::new();
    let mut cumulative_ends: Vec<u64> = Vec::new();
    let mut total = 0u64;
    let mut ordinals: HashMap<&str, u64> = HashMap::new();
    for e in events {
        let Some(r) = by_path.get(e.relative.as_str()) else {
            continue;
        };
        if r.action == "loose" || r.layout == "streaming" {
            continue;
        }
        let touches = touch_count(e, r.block_size);
        if touches == 0 {
            continue;
        }
        let n = ordinals.len() as u64 + 1;
        ordinals.entry(e.relative.as_str()).or_insert(n);
        total += touches;
        eligible.push((e, r, touches));
        cumulative_ends.push(total);
    }
    let mut s = AccessStream {
        keys: Vec::new(),
        raw_sizes: Vec::new(),
        overlaps: Vec::new(),
        hot: Vec::new(),
        reset_before: Vec::new(),
    };
    if eligible.is_empty() {
        return s;
    }
    let append_slice = |s: &mut AccessStream,
                        e: &ReadEvent,
                        r: &Recommendation,
                        first_touch: u64,
                        count: u64,
                        reset: bool|
     -> u64 {
        let mut appended = 0u64;
        let ordinal = ordinals[e.relative.as_str()];
        let stop = first_touch + count;
        for (local, (block, raw, overlap)) in
            block_overlaps(e, r.block_size).into_iter().enumerate()
        {
            let local = local as u64;
            if local < first_touch {
                continue;
            }
            if local >= stop {
                break;
            }
            s.keys.push((ordinal << 48) | block);
            s.raw_sizes.push(raw);
            s.overlaps.push(overlap);
            s.hot.push(r.hot);
            s.reset_before.push(reset && appended == 0);
            appended += 1;
        }
        appended
    };
    if max_touches == 0 || total <= max_touches {
        let mut first = true;
        for (e, r, touches) in &eligible {
            append_slice(&mut s, e, r, 0, *touches, first);
            first = false;
        }
    } else {
        let window_count = (max_touches / 4096).clamp(1, 64);
        let window_budget = (max_touches / window_count).max(1);
        for w in 0..window_count {
            let seg_start = total * w / window_count;
            let seg_end = total * (w + 1) / window_count;
            let seg_size = seg_end.saturating_sub(seg_start);
            if seg_size == 0 {
                continue;
            }
            let budget = window_budget.min(seg_size);
            let start_touch = seg_start + (seg_size - budget) / 2;
            let end_touch = start_touch + budget;
            let mut i = cumulative_ends.partition_point(|&end| end <= start_touch);
            let mut window_first = true;
            while i < eligible.len() {
                let event_end = cumulative_ends[i];
                let event_start = if i == 0 { 0 } else { cumulative_ends[i - 1] };
                if event_start >= end_touch {
                    break;
                }
                let (e, r, touches) = eligible[i];
                let local_start = start_touch.saturating_sub(event_start);
                let local_end = touches.min(end_touch - event_start);
                if local_end > local_start {
                    let appended = append_slice(
                        &mut s,
                        e,
                        r,
                        local_start,
                        local_end - local_start,
                        window_first,
                    );
                    if appended > 0 {
                        window_first = false;
                    }
                }
                let _ = event_end;
                i += 1;
            }
        }
    }
    s
}

/// An LRU keyed by block with insertion order kept, as the reference's OrderedDict.
struct Lru {
    order: std::collections::VecDeque<u64>,
    sizes: HashMap<u64, u64>,
    tomb: HashMap<u64, u64>,
}

impl Lru {
    fn new() -> Self {
        Self {
            order: Default::default(),
            sizes: HashMap::new(),
            tomb: HashMap::new(),
        }
    }
    fn clear(&mut self) {
        self.order.clear();
        self.sizes.clear();
        self.tomb.clear();
    }
    fn touch(&mut self, key: u64) -> Option<u64> {
        let size = *self.sizes.get(&key)?;
        *self.tomb.entry(key).or_insert(0) += 1;
        self.order.push_back(key);
        Some(size)
    }
    fn insert(&mut self, key: u64, size: u64) {
        self.sizes.insert(key, size);
        self.order.push_back(key);
    }
    fn pop_oldest(&mut self) -> Option<(u64, u64)> {
        while let Some(key) = self.order.pop_front() {
            if let Some(n) = self.tomb.get_mut(&key) {
                if *n > 0 {
                    *n -= 1;
                    continue;
                }
            }
            if let Some(size) = self.sizes.remove(&key) {
                return Some((key, size));
            }
        }
        None
    }
}

fn simulate_cache(stream: &AccessStream, capacities: &[u64]) -> Vec<CacheSimulation> {
    let mut out = Vec::new();
    for &capacity in capacities {
        let mut cache = Lru::new();
        let mut seen_once: HashSet<u64> = HashSet::new();
        let mut resident = 0u64;
        let mut requested = 0u64;
        let mut hit_bytes = 0u64;
        for i in 0..stream.keys.len() {
            if stream.reset_before[i] {
                cache.clear();
                seen_once.clear();
                resident = 0;
            }
            let key = stream.keys[i];
            let raw = stream.raw_sizes[i];
            let overlap = stream.overlaps[i];
            requested += overlap;
            if cache.touch(key).is_some() {
                hit_bytes += overlap;
                continue;
            }
            let admit = stream.hot[i] || seen_once.contains(&key);
            if !admit {
                seen_once.insert(key);
                continue;
            }
            seen_once.remove(&key);
            if raw > capacity {
                continue;
            }
            while resident + raw > capacity {
                match cache.pop_oldest() {
                    Some((_, size)) => resident -= size,
                    None => break,
                }
            }
            if resident + raw <= capacity {
                cache.insert(key, raw);
                resident += raw;
            }
        }
        out.push(CacheSimulation {
            capacity,
            requested_bytes: requested,
            hit_bytes,
        });
    }
    out
}

fn choose_cache_size(sims: &[CacheSimulation]) -> u64 {
    if sims.is_empty() {
        return 64 * 1024 * 1024;
    }
    let best = sims.iter().map(|s| s.hit_ratio()).fold(0.0f64, f64::max);
    if best < 0.08 {
        return sims
            .iter()
            .map(|s| s.capacity)
            .filter(|&c| c >= 64 * 1024 * 1024)
            .min()
            .unwrap_or(sims[0].capacity);
    }
    let nearest = |target: u64| -> u64 {
        sims.iter()
            .min_by_key(|s| (s.capacity.abs_diff(target), s.capacity))
            .map(|s| s.capacity)
            .unwrap()
    };
    if best < 0.15 {
        return nearest(128 * 1024 * 1024);
    }
    if best < 0.25 {
        return nearest(256 * 1024 * 1024);
    }
    let mut previous: Option<&CacheSimulation> = None;
    for s in sims {
        if s.capacity < 64 * 1024 * 1024 {
            previous = Some(s);
            continue;
        }
        let within_best = best - s.hit_ratio() <= 0.03;
        let marginal = match previous {
            Some(p) => s.hit_ratio() - p.hit_ratio(),
            None => s.hit_ratio(),
        };
        if within_best || (s.hit_ratio() >= 0.30 && marginal < 0.05) {
            return s.capacity;
        }
        previous = Some(s);
    }
    sims[sims.len() - 1].capacity
}

fn choose_physical_cache(recs: &[Recommendation], files: &[FileMetrics]) -> u64 {
    let eligible: Vec<&FileMetrics> = recs
        .iter()
        .filter(|r| r.action != "loose" && r.layout != "streaming")
        .map(|r| &files[r.metrics_index])
        .collect();
    if eligible.is_empty() {
        return 8 * 1024 * 1024;
    }
    let requested: u64 = eligible.iter().map(|m| m.requested_bytes).sum();
    if requested == 0 {
        return 8 * 1024 * 1024;
    }
    let rq = requested as f64;
    let weighted_repeat = eligible
        .iter()
        .map(|m| m.requested_bytes as f64 * m.candidates[&KIB64].repeat_ratio)
        .sum::<f64>()
        / rq;
    let weighted_small = eligible
        .iter()
        .map(|m| m.requested_bytes as f64 * m.small_64k_ratio)
        .sum::<f64>()
        / rq;
    let weighted_random = eligible
        .iter()
        .map(|m| m.requested_bytes as f64 * m.random_seek_ratio)
        .sum::<f64>()
        / rq;
    let pressure = weighted_repeat + 0.5 * weighted_small * weighted_random;
    if requested >= 16 * (1 << 30) && pressure >= 0.20 {
        return 64 * 1024 * 1024;
    }
    if requested >= 2 * (1 << 30) || pressure >= 0.10 {
        return 32 * 1024 * 1024;
    }
    16 * 1024 * 1024
}

fn choose_runtime_workers(events: &[ReadEvent], selected: &HashSet<&str>) -> u32 {
    let sel: Vec<&ReadEvent> = events
        .iter()
        .filter(|e| selected.contains(e.relative.as_str()))
        .collect();
    if sel.is_empty() {
        return 2;
    }
    let mut bytes_by_path: HashMap<&str, u64> = HashMap::new();
    let mut switches = 0u64;
    let mut previous: Option<&str> = None;
    for e in &sel {
        *bytes_by_path.entry(e.relative.as_str()).or_insert(0) += e.length;
        if let Some(p) = previous {
            if p != e.relative {
                switches += 1;
            }
        }
        previous = Some(e.relative.as_str());
    }
    let total: u64 = bytes_by_path.values().sum();
    let dominant = if total != 0 {
        *bytes_by_path.values().max().unwrap() as f64 / total as f64
    } else {
        1.0
    };
    let switch_ratio = switches as f64 / ((sel.len() - 1).max(1)) as f64;
    let file_count = bytes_by_path.len();
    if file_count >= 1000 && switch_ratio >= 0.70 {
        return 8;
    }
    if file_count >= 128 && switch_ratio >= 0.50 {
        return 6;
    }
    if dominant >= 0.85 && file_count <= 8 {
        return 2;
    }
    4
}

fn choose_latency_reserve(events: &[ReadEvent], selected: &HashSet<&str>, workers: u32) -> u32 {
    if workers <= 1 {
        return 0;
    }
    let sel: Vec<&ReadEvent> = events
        .iter()
        .filter(|e| selected.contains(e.relative.as_str()))
        .collect();
    if sel.is_empty() {
        return 0;
    }
    let total = sel.len() as f64;
    let small = sel.iter().filter(|e| e.length <= 64 * 1024).count() as f64 / total;
    let tiny = sel.iter().filter(|e| e.length <= 16 * 1024).count() as f64 / total;
    let bulk = sel.iter().filter(|e| e.length >= 256 * 1024).count() as f64 / total;
    if small >= 0.45 || tiny >= 0.20 || (small >= 0.25 && bulk >= 0.20) {
        1
    } else {
        0
    }
}

fn round_pool_size(required: u64) -> u64 {
    let quantum = 128 * 1024 * 1024;
    (required.div_ceil(quantum) * quantum).max(256 * 1024 * 1024)
}

/// Generate the profile for `traces` (one or more sessions of the same game).
pub fn generate(traces: &[TraceSpec]) -> Result<Generated, String> {
    if traces.is_empty() {
        return Err("no traces".into());
    }
    let mut events: Vec<ReadEvent> = Vec::new();
    let mut index_paths: BTreeMap<String, u64> = BTreeMap::new();
    let mut warnings = Vec::new();
    let (mut records, mut reads) = (0u64, 0u64);
    let mut ampr_index_bytes = 0u64;
    for (ordinal, spec) in traces.iter().enumerate() {
        let index = load_index(&spec.index)?;
        ampr_index_bytes =
            ampr_index_bytes.max(std::fs::metadata(&spec.index).map(|m| m.len()).unwrap_or(0));
        let load = stream_one_trace(spec, ordinal, &index)?;
        events.extend(load.events);
        records += load.records;
        reads += load.reads;
        for (relative, size) in load.path_sizes {
            match index_paths.get(&relative) {
                Some(&existing) if existing != size => {
                    warnings.push(format!(
                        "{}: indexed size changed for {relative}: {existing} -> {size}; using the larger entry",
                        spec.name
                    ));
                    if size > existing {
                        index_paths.insert(relative, size);
                    }
                }
                Some(_) => {}
                None => {
                    index_paths.insert(relative, size);
                }
            }
        }
        warnings.extend(
            load.warnings
                .into_iter()
                .map(|w| format!("{}: {w}", spec.name)),
        );
    }
    events.sort_by_key(|e| e.order_key());
    let files = build_file_metrics(&events);
    let mut recs = initial_recommendations(&files);
    let provisional_lanes = choose_lanes(&recs, &files);
    let provisional_pack_count: u64 = build_groups(&recs, &files, provisional_lanes)
        .iter()
        .map(|(_, g)| g.pack_count)
        .sum();
    let mandatory = estimate_index_bytes(&[], &files, provisional_pack_count, &index_paths);
    let chunk_budget = DEFAULT_INDEX_BUDGET.saturating_sub(mandatory);
    enforce_index_budget(&mut recs, &files, chunk_budget);
    let lanes = choose_lanes(&recs, &files);
    let groups = build_groups(&recs, &files, lanes);
    let pack_count: u64 = groups.iter().map(|(_, g)| g.pack_count).sum();
    let projected = estimate_index_bytes(&recs, &files, pack_count, &index_paths);
    if mandatory > DEFAULT_INDEX_BUDGET {
        warnings.push(format!(
            "mandatory AMPRPAK4 file/string tables {} exceed index budget {} before any packed chunks",
            human_size(mandatory as f64),
            human_size(DEFAULT_INDEX_BUDGET as f64)
        ));
    }
    if projected > DEFAULT_INDEX_BUDGET {
        warnings.push(format!(
            "projected pack index {} still exceeds budget {}",
            human_size(projected as f64),
            human_size(DEFAULT_INDEX_BUDGET as f64)
        ));
    }
    let by_path: HashMap<&str, &Recommendation> = recs
        .iter()
        .map(|r| (files[r.metrics_index].relative.as_str(), r))
        .collect();
    let stream = build_access_stream(&events, &by_path, DEFAULT_CACHE_MAX_TOUCHES);
    let sims = simulate_cache(&stream, &DEFAULT_CACHE_CANDIDATES);
    let cache_size = choose_cache_size(&sims);
    let physical = choose_physical_cache(&recs, &files);
    let selected: HashSet<&str> = recs
        .iter()
        .filter(|r| r.action != "loose")
        .map(|r| files[r.metrics_index].relative.as_str())
        .collect();
    let workers = choose_runtime_workers(&events, &selected);
    let latency = choose_latency_reserve(&events, &selected, workers);
    let pool = round_pool_size(
        projected
            + ampr_index_bytes
            + cache_size
            + physical
            + RUNTIME_PIPELINE_SLOTS * RUNTIME_PIPELINE_IO_BYTES
            + workers as u64 * RUNTIME_WORKER_SCRATCH_BYTES
            + RUNTIME_POST_CACHE_RESERVE_BYTES,
    );
    let toml = render_toml(
        &recs, &files, &groups, projected, cache_size, physical, workers, latency, pool,
    );
    Ok(Generated {
        toml,
        summary: Summary {
            traces: traces.len(),
            records,
            reads,
            files_observed: files.len(),
            files_packed: recs.iter().filter(|r| r.action != "loose").count(),
            decoded_cache_bytes: cache_size,
            physical_cache_bytes: physical,
            runtime_workers: workers,
            latency_reserve_workers: latency,
            projected_index_bytes: projected,
            warnings,
        },
    })
}

fn profile_comment(recs: &[&Recommendation], files: &[FileMetrics]) -> Vec<String> {
    let reads: usize = recs.iter().map(|r| files[r.metrics_index].read_count).sum();
    let requested: u64 = recs
        .iter()
        .map(|r| files[r.metrics_index].requested_bytes)
        .sum();
    let mut confidence: BTreeMap<&str, usize> = BTreeMap::new();
    for r in recs {
        *confidence
            .entry(files[r.metrics_index].confidence_label)
            .or_insert(0) += 1;
    }
    vec![
        format!(
            "observed files={}, reads={reads}, submitted={}",
            recs.len(),
            human_size(requested as f64)
        ),
        format!(
            "confidence={}",
            confidence
                .iter()
                .map(|(k, v)| format!("{k}:{v}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn render_toml(
    recs: &[Recommendation],
    files: &[FileMetrics],
    groups: &[(&'static str, Group)],
    projected: u64,
    cache: u64,
    physical: u64,
    workers: u32,
    latency: u32,
    pool: u64,
) -> String {
    // "exact" pattern mode: one rule per distinct (action, layout, block, hot, group), its
    // paths sorted, at most MAX_RULE_FILES per rule.
    let mut by_profile: BTreeMap<(String, String, u64, bool, String), Vec<&Recommendation>> =
        BTreeMap::new();
    for r in recs.iter().filter(|r| r.action != "loose") {
        by_profile.entry(r.profile_key()).or_default().push(r);
    }
    let mut lines: Vec<String> = vec![
        format!("# Generated by ampr_pack_profile.py {TOOL_VERSION} from APR traces."),
        "# IMPORTANT: traces contain only APR reads observed in recorded scenarios.".into(),
        "# Adapt this config to the real game by adding remaining files/directories".into(),
        "# or deliberately keep them loose. A trace cannot prove complete coverage.".into(),
        format!(
            "# Projected resident pack index: {}.",
            human_size(projected as f64)
        ),
        format!("# Recommended decoded cache: {}.", human_size(cache as f64)),
        format!(
            "# Recommended physical-page cache: {}.",
            human_size(physical as f64)
        ),
        format!("# Recommended runtime workers: {workers}."),
        format!("# Recommended latency-reserved workers: {latency}."),
        format!(
            "# Recommended AMPR internal pool: {}.",
            human_size(pool as f64)
        ),
        String::new(),
        "[pack]".into(),
        "index_name = \"ampr_assets.index\"".into(),
        "pack_pattern = \"ampr_assets-{group}-lane{lane:02d}-vol{volume:02d}-{id:03d}.pak\"".into(),
        "default_action = \"loose\"".into(),
        "default_block_size = \"64KiB\"".into(),
        format!("io_page_size = \"{}\"", size_literal(DEFAULT_IO_PAGE)),
        format!("payload_alignment = \"{}\"", size_literal(DEFAULT_IO_PAGE)),
        "chunk_alignment = \"64B\"".into(),
        "workers = 8".into(),
        "compression_mode = \"hc\"".into(),
        "compression_level = 12".into(),
        "acceleration = 1".into(),
        "deduplicate = true".into(),
        "deduplicate_scope = \"lane\"".into(),
        "deduplicate_streaming = false".into(),
        "min_savings_bytes = 64".into(),
        "min_savings_ratio = 0.01".into(),
        "io_neutral_min_savings_bytes = \"8KiB\"".into(),
        "io_neutral_min_savings_ratio = 0.125".into(),
        "auto_loose_large_files = true".into(),
        "auto_loose_hot_files = false".into(),
        "auto_loose_min_file_size = \"64MiB\"".into(),
        "auto_loose_sample_blocks = 32".into(),
        "auto_loose_sample_bytes = \"16MiB\"".into(),
        "auto_loose_min_savings_ratio = 0.05".into(),
        "auto_loose_max_raw_ratio = 0.90".into(),
        "preserve_mtime = true".into(),
        "validate_index_metadata = true".into(),
        String::new(),
        "[runtime]".into(),
        format!("decoded_cache_bytes = {cache}"),
        format!("physical_cache_bytes = {physical}"),
        format!("workers = {workers}"),
        format!("latency_reserve_workers = {latency}"),
        String::new(),
    ];
    for (name, g) in groups {
        lines.push(format!("[groups.{name}]"));
        lines.push(format!("pack_count = {}", g.pack_count));
        lines.push("assignment = \"balanced\"".into());
        lines.push(format!(
            "max_pack_size = {}",
            toml_string(&size_literal(g.max_pack_size))
        ));
        lines.push("stripe_large_files = false".into());
        lines.push(format!(
            "io_page_size = {}",
            toml_string(&size_literal(DEFAULT_IO_PAGE))
        ));
        lines.push(String::new());
    }
    for (key, items) in &by_profile {
        let (action, layout, block_size, hot, group) = key;
        let mut paths: Vec<(&str, &Recommendation)> = items
            .iter()
            .map(|r| (files[r.metrics_index].relative.as_str(), *r))
            .collect();
        paths.sort_by(|a, b| a.0.cmp(b.0));
        for chunk in paths.chunks(MAX_RULE_FILES) {
            let subset: Vec<&Recommendation> = chunk.iter().map(|(_, r)| *r).collect();
            for c in profile_comment(&subset, files) {
                lines.push(format!("# {c}"));
            }
            lines.push("[[rule]]".into());
            lines.push(format!("action = {}", toml_string(action)));
            if chunk.len() == 1 {
                lines.push(format!("include = [{}]", toml_string(chunk[0].0)));
            } else {
                lines.push("include = [".into());
                for (p, _) in chunk {
                    lines.push(format!("  {},", toml_string(p)));
                }
                lines.push("]".into());
            }
            lines.push(format!(
                "block_size = {}",
                toml_string(&size_literal(*block_size))
            ));
            lines.push(format!("group = {}", toml_string(group)));
            lines.push(format!("layout = {}", toml_string(layout)));
            if *hot {
                lines.push("hot = true".into());
                if *block_size <= 32 * 1024 {
                    lines.push("min_savings_bytes = 16".into());
                    lines.push("min_savings_ratio = 0.0025".into());
                    lines.push("io_neutral_min_savings_bytes = \"2KiB\"".into());
                    lines.push("io_neutral_min_savings_ratio = 0.125".into());
                }
            }
            if layout == "streaming" {
                lines.push("streaming = true".into());
            }
            lines.push(String::new());
        }
    }
    lines.push("# Safety exclusions. The last matching rule wins.".into());
    lines.push("[[rule]]".into());
    lines.push("action = \"loose\"".into());
    lines.push("include = [".into());
    for p in STANDARD_LOOSE_PATTERNS {
        lines.push(format!("  {},", toml_string(p)));
    }
    lines.push("]".into());
    lines.push(String::new());
    lines.join("\n")
}
