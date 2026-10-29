//! LZ4 asset packs in the AMPRPAK4 layout read by the `ampr_emu` runtime: byte formats, the
//! embedded runtimes, the pack writer and the unpacking reader.

pub mod block;
pub mod format;
pub mod index;
pub mod reader;
pub mod rules;
pub mod runtime;
pub mod tree;
pub mod writer;

pub use rules::{Sample, always_loose, auto_loose_applies, keep_loose, sample_indices, select};

/// What the rules decide for one packed file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackSpec {
    /// log2 of the block size, 14..=20.
    pub block_shift: u8,
    pub store: bool,
    pub hot: bool,
    pub random: bool,
}

/// The optional AMPRCFG1 runtime profile (`ampr_assets.index.runtime`): `reader` reads it from
/// other tools' packs; `writer` can encode it (Forge's own packs carry none).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeProfile {
    pub decoded_cache_bytes: u64,
    pub physical_cache_bytes: u64,
    pub workers: u32,
    pub latency_reserve_workers: u32,
}

pub const MANIFEST: &str = "ampr_assets.index";
pub const CRC_SIDECAR: &str = "ampr_assets.index.crc";
pub const PROFILE: &str = "ampr_assets.index.runtime";
pub const INDEX: &str = "ampr_emu.index";
pub const RUNTIME: &str = "fakelib/libSceAmpr.sprx";
pub const JOURNAL: &str = "ampr_commands.bin";
pub const LOGS: [&str; 2] = ["ampr_emu.log", "apr_emu.log"];

/// Volume file name for pack `n`: `ampr_assets-000.pak` (three digits at least).
pub fn volume_name(n: u32) -> String {
    format!("ampr_assets-{n:03}.pak")
}
