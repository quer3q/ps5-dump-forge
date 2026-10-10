//! LZ4 asset packs in the AMPRPAK4 layout read by the `ampr_emu` runtime: byte formats, the
//! embedded runtimes, the pack writer and the unpacking reader.

pub mod format;
pub mod index;
pub mod journal;
pub mod reader;
pub mod rules;
pub mod runtime;
pub mod tree;
pub mod writer;

pub use rules::{
    AutoLoose, Profile, Sample, Selection, always_loose, keep_loose, load_profile, profile_spec,
    select,
};

/// What the rules decide for one packed file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackSpec {
    /// log2 of the block size, 14..=20.
    pub block_shift: u8,
    pub store: bool,
    pub hot: bool,
    pub random: bool,
}

/// The optional AMPRCFG1 runtime profile (`ampr_assets.index.runtime`), from a TOML `[runtime]`
/// table. `rules` parses and validates it; `writer` encodes it.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_names() {
        assert_eq!(volume_name(0), "ampr_assets-000.pak");
        assert_eq!(volume_name(12), "ampr_assets-012.pak");
        assert_eq!(volume_name(1000), "ampr_assets-1000.pak");
    }
}
