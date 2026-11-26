// Ported from drakmor's ampr_emu tools (https://github.com/drakmor/ampr_emu, GPL-3.0). This
// Rust port is part of ps5upload, GPL-3.0-or-later.
//! AMPR seekable LZ4 asset packs: a game's files stored as independent LZ4 blocks in a few
//! large volumes, which `ampr_emu` 0.4 and later (a backport's `fakelib/libSceAmpr.sprx`) serves
//! back to the title as the original files. The game image gets smaller; the packed originals
//! are not shipped.
//!
//! The format and the packing rules are drakmor's (drakmor/ampr_emu, GPL-3:
//! `tools/ampr_pack.py`, `tools/ampr_pack_format.py`, `docs/ASSET_PACKS.md`); this is a port.
//! Files in a pack set, next to `ampr_emu.index` at `/app0`:
//!
//! ```text
//! ampr_assets.index          AMPRPAK4 manifest: 128-byte header; a 48-byte file record per
//!                            AMPRIDX3 entry (file id = record + 1), packed or loose; 12-byte
//!                            chunk records (48-bit offset, 16-bit volume, 20-bit size − 1,
//!                            2-bit codec, 8 flag bits); 32-byte volume records; strings.
//!                            CRC-32 over the header and over the payload.
//! ampr_assets-NNN.pak        AMPRDAT3 volumes: 64-byte header, payload from the first I/O
//!                            page (64 KiB), size a whole number of pages.
//! ampr_assets.index.crc      AMPRCRC1: one CRC-32 per decoded chunk; offline checks only.
//! ampr_assets.index.runtime  AMPRCFG1: cache sizes and workers, when the profile sets them.
//! ```
//!
//! A chunk is one block of a file (16 KiB to 1 MiB, 64 KiB by default) stored as a raw LZ4
//! block, or raw when LZ4 does not save enough. Outside streaming files a chunk never straddles
//! an I/O page unless it is page-sized and page-aligned. Identical blocks are stored once per
//! lane. The build id, which every header carries, is a hash of the configuration and records,
//! so the same files and profile give the same set from this packer and from drakmor's.
//!
//! The runtime intercepts AMPR asset reads (and, in its PackedStdio build, ordinary read-only
//! file reads) but never `mmap`: executables and modules have to stay loose.

pub mod config;
pub mod format;
pub mod glob;
pub mod image;
pub mod lz4;
pub mod ng;
pub mod pack;
pub mod profile_gen;
pub mod read;
pub mod regen;

pub use config::{
    default_profile, default_profile_for, default_profile_name, large_files_profile, unity_profile,
    unreal_profile, Config, GameEngine, LARGE_FILE_MIN,
};
pub use pack::{build, measure, Control, Output};
pub use read::Reader;

/// Does this `libSceAmpr.sprx` serve asset packs, and which version is it? Read from the
/// module's own strings: the manifest magic it checks and the version it logs at load
/// (`[AMPR_EMU] name=libSceAmpr version=0.4.2.1 (c) Drakmor`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSupport {
    pub packs: bool,
    pub version: Option<String>,
}

pub fn runtime_support(module: &[u8]) -> RuntimeSupport {
    let has = |needle: &[u8]| module.windows(needle.len()).any(|w| w == needle);
    let packs = has(format::INDEX_MAGIC) && has(b"ampr_assets");
    // The banner is one C string ending in the marker: "0.4.2.1 (c) Drakmor", and in the
    // 0.3 series "0.3.6 (public beta) (c) Drakmor" (#449). The version is its first token
    // made of digits and dots.
    let marker = b" (c) Drakmor";
    let version = module
        .windows(marker.len())
        .position(|w| w == marker)
        .and_then(|end| {
            let start = module[..end]
                .iter()
                .rposition(|&b| b == 0)
                .map_or(0, |p| p + 1);
            let banner = std::str::from_utf8(&module[start..end]).ok()?;
            banner
                .split(|c: char| !(c.is_ascii_digit() || c == '.'))
                .find(|t| t.contains('.') && t.starts_with(|c: char| c.is_ascii_digit()))
                .map(|t| t.trim_end_matches('.').to_string())
        });
    RuntimeSupport { packs, version }
}
