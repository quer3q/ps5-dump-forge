//! Which files go into the packs, and how: the built-in guess (the keep-loose list and every
//! root file stay loose), then upstream `ampr_pack.py`'s auto-loose over the large files left.

use crate::PackSpec;

// ---- safety and fallback ------------------------------------------------------------------------

fn has_suffix(path: &str, suffixes: &[&str]) -> bool {
    let p = path.as_bytes();
    suffixes
        .iter()
        .any(|s| p.len() >= s.len() && p[p.len() - s.len()..].eq_ignore_ascii_case(s.as_bytes()))
}

fn artifact(path: &str) -> bool {
    use crate::{CRC_SIDECAR, INDEX, JOURNAL, LOGS, MANIFEST, PROFILE, RUNTIME};
    [MANIFEST, CRC_SIDECAR, PROFILE, INDEX, RUNTIME, JOURNAL].contains(&path)
        || LOGS.contains(&path)
}

/// Paths that are never packed: they are loaded by the console or the
/// runtime before (or outside) the pack layer. `path` is relative with `/` separators.
pub fn always_loose(path: &str) -> bool {
    let file = path.rsplit('/').next().unwrap_or(path);
    let first = path.split('/').next().unwrap_or("");
    let in_dir = path.contains('/');
    file == "eboot.bin"
        || has_suffix(path, &[".elf", ".self", ".prx", ".sprx"])
        || (in_dir && (first == "sce_sys" || first == "sce_module" || first.starts_with("fakelib")))
        || artifact(path)
}

const FALLBACK_SUFFIXES: &[&str] = &[
    ".bak",
    ".dat",
    ".utoc", // general
    ".json",
    ".ini",
    ".cfg",
    ".xml",
    ".txt", // config/text
    ".bk2",
    ".mp4",
    ".ivf",
    ".usm",
    ".bnk",
    ".wem",
    ".at9",
    ".pfs",
    ".img", // media/native
    ".png",
    ".jpg",
    ".jpeg",
    ".webp",
    ".gif",
    ".bmp",
    ".ico",
    ".tga",
    ".tif",
    ".tiff",
    ".exr",
    ".hdr",
    ".psd", // images
    ".dds",
    ".ktx",
    ".ktx2",
    ".astc",
    ".basis",
    ".gnf",
    ".gnfp",
    ".jxm",
    ".vtf", // GPU textures
    ".zip",
    ".7z",
    ".rar",
    ".gz",
    ".xz",
    ".bz2",
    ".zst",
    ".lz4",
    ".pak", // containers
    ".mp3",
    ".ogg",
    ".flac",
    ".aac",
    ".opus",
    ".m4a", // audio
    ".avi",
    ".mkv",
    ".mov",
    ".wmv",
    ".flv",
    ".webm",
    ".m4v", // video
    ".bik",
    ".mpg",
    ".mpeg",
    ".m2v", // more video
    ".uproject",
    ".uplugin",
    ".upluginmanifest", // Unreal reads these at boot
];

/// Directory families that are loose at any depth, and the two that are only loose at the root.
const PROTECTED_DIRS: &[&str] = &[
    "sce_module",
    "sce_sys",
    "system",
    "mods",
    "save",
    "fakelib",
    "trophy2",
    "uds",
];
const ROOT_ONLY_DIRS: &[&str] = &["decrypted", "_DUPLEX_"];

/// The keep-loose list: what an engine may open,
/// stat or list before the game initializes AMPR, when packed files do not exist yet (container
/// indexes, configs, media, the protected directory families).
pub fn keep_loose(path: &str) -> bool {
    if has_suffix(path, FALLBACK_SUFFIXES) {
        return true;
    }
    let dirs: Vec<&str> = path.split('/').collect();
    let dirs = &dirs[..dirs.len() - 1];
    dirs.iter().any(|d| PROTECTED_DIRS.contains(d))
        || dirs.first().is_some_and(|d| ROOT_ONLY_DIRS.contains(d))
}

/// The built-in guess's loose set: the keep-loose list and every root file.
fn fallback_loose(path: &str) -> bool {
    !path.contains('/') || keep_loose(path)
}

// ---- selection ----------------------------------------------------------------------------------

const SHIFT_64K: u8 = 16;

fn plain_spec() -> PackSpec {
    PackSpec {
        block_shift: SHIFT_64K,
        store: false,
        hot: false,
        random: false,
    }
}

/// `None` keeps the file loose: empty files, `always_loose` paths, the keep-loose list and root
/// files; everything else is packed with 64 KiB blocks.
pub fn select(path: &str, size: u64) -> Option<PackSpec> {
    if size == 0 || always_loose(path) || fallback_loose(path) {
        return None;
    }
    Some(plain_spec())
}

// ---- auto-loose ---------------------------------------------------------------------------------

/// Upstream `ampr_pack.py`'s auto-loose at its defaults: a compressed file of at least this size
/// is sampled, and stays loose when its sampled blocks barely shrink (packing it would only add
/// the pack layer to every read).
pub const AUTO_LOOSE_MIN_SIZE: u64 = 64 << 20;
/// At most this many blocks, and this many bytes of them (one block at least), are sampled.
pub const AUTO_LOOSE_SAMPLE_BLOCKS: u64 = 32;
pub const AUTO_LOOSE_SAMPLE_BYTES: u64 = 16 << 20;
/// Kept loose when the sample saves less than this, or ...
pub const AUTO_LOOSE_MIN_SAVINGS: f64 = 0.05;
/// ... at least this share of its blocks stays RAW.
pub const AUTO_LOOSE_MAX_RAW: f64 = 0.90;

/// What sampling a file's blocks found, as the writer would store them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sample {
    pub raw_bytes: u64,
    pub stored_bytes: u64,
    pub blocks: u64,
    pub raw_blocks: u64,
}

impl Sample {
    pub fn savings(&self) -> f64 {
        if self.raw_bytes == 0 {
            return 0.0;
        }
        1.0 - self.stored_bytes as f64 / self.raw_bytes as f64
    }

    pub fn raw_ratio(&self) -> f64 {
        if self.blocks == 0 {
            return 0.0;
        }
        self.raw_blocks as f64 / self.blocks as f64
    }

    /// Whether the file stays loose: it saves less than [`AUTO_LOOSE_MIN_SAVINGS`], or at least
    /// [`AUTO_LOOSE_MAX_RAW`] of its blocks stay RAW.
    pub fn keeps_loose(&self) -> bool {
        self.savings() < AUTO_LOOSE_MIN_SAVINGS || self.raw_ratio() >= AUTO_LOOSE_MAX_RAW
    }
}

/// Whether a `size`-byte file packed per `spec` is sampled: compressed (not `store`), not hot,
/// at least [`AUTO_LOOSE_MIN_SIZE`].
pub fn auto_loose_applies(size: u64, spec: &PackSpec) -> bool {
    !spec.store && !spec.hot && size >= AUTO_LOOSE_MIN_SIZE
}

/// The block indices to sample, sorted: at most [`AUTO_LOOSE_SAMPLE_BLOCKS`] and
/// [`AUTO_LOOSE_SAMPLE_BYTES`] worth (one at least), spread evenly from the first block to the
/// last by integer interpolation: deterministic, no randomness.
pub fn sample_indices(size: u64, block_shift: u8) -> Vec<u64> {
    let total = size.div_ceil(1 << block_shift);
    let by_bytes = (AUTO_LOOSE_SAMPLE_BYTES >> block_shift).max(1);
    let n = AUTO_LOOSE_SAMPLE_BLOCKS.min(by_bytes);
    if total == 0 {
        return Vec::new();
    }
    if total <= n {
        return (0..total).collect();
    }
    let mut v: Vec<u64> = (0..n)
        .map(|k| (u128::from(k) * u128::from(total - 1) / u128::from(n - 1)) as u64)
        .collect();
    v.dedup();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_loose_table() {
        for p in [
            "eboot.bin",
            "a/b/eboot.bin",
            "x.ELF",
            "d/y.Self",
            "m.prx",
            "d/m.SPRX",
            "sce_sys/param.json",
            "sce_module/a.bin",
            "fakelib/libSceAmpr.sprx",
            "fakelib2/a.dat",
            "ampr_assets.index",
            "ampr_assets.index.crc",
            "ampr_assets.index.runtime",
            "ampr_emu.index",
            "ampr_commands.bin",
            "ampr_emu.log",
            "apr_emu.log",
        ] {
            assert!(always_loose(p), "{p}");
        }
        for p in [
            "fakelib",
            "fakelibs.txt",
            "d/sce_sys/a",
            "data/eboot.bin.bak",
            "ampr_assets-000.pak",
            "d/ampr_emu.log",
        ] {
            assert!(!always_loose(p), "{p}");
        }
    }

    fn spec(shift: u8, store: bool, hot: bool, random: bool) -> Option<PackSpec> {
        Some(PackSpec {
            block_shift: shift,
            store,
            hot,
            random,
        })
    }

    #[test]
    fn fallback_table() {
        let f = |p: &str| select(p, 10);
        assert_eq!(f("data/a.bin"), spec(16, false, false, false));
        assert_eq!(f("a/b/c.bin"), spec(16, false, false, false));
        assert_eq!(f("root.bin"), None, "root-level files stay loose");
        assert_eq!(f("data/a.PNG"), None);
        assert_eq!(f("data/a.json"), None);
        assert_eq!(f("data/a.utoc"), None);
        assert_eq!(f("data/system/a.bin"), None, "nested protected family");
        assert_eq!(f("system/a.bin"), None);
        assert_eq!(f("decrypted/a.bin"), None, "root-only family");
        assert_eq!(f("data/decrypted/a.bin"), spec(16, false, false, false));
        assert_eq!(f("data/_DUPLEX_/a.bin"), spec(16, false, false, false));
        assert_eq!(f("_DUPLEX_/a.bin"), None);
        assert_eq!(f("data/eboot.bin"), None);
        for p in [
            "d/a.bik",
            "d/a.MPG",
            "d/a.mpeg",
            "d/a.m2v",
            "sb/SB.uproject",
            "sb/plugins/x.uplugin",
            "sb/plugins/x.UPluginManifest",
        ] {
            assert_eq!(f(p), None, "{p}");
            assert!(keep_loose(p), "{p}");
        }
        assert!(
            !keep_loose("root.bin"),
            "the root rule is the fallback's only"
        );
        assert_eq!(select("data/a.bin", 0), None, "empty");
    }

    #[test]
    fn fallback_stellar_blade_packs_only_ucas() {
        let paths = [
            "contentids.json",
            "engine/saved/config/ps5/manifest.ini",
            "sb/content/paks/global.ucas",
            "sb/content/paks/global.utoc",
            "sb/content/paks/pakchunk0-ps5.pak",
            "sb/content/paks/pakchunk0-ps5.ucas",
            "sb/content/paks/pakchunk0-ps5.utoc",
            "sb/content/paks/pakchunk1-ps5.ucas",
            "sb/saved/config/ps5/engine.ini",
        ];
        let packed: Vec<&str> = paths
            .into_iter()
            .filter(|p| select(p, 1000).is_some())
            .collect();
        assert_eq!(
            packed,
            [
                "sb/content/paks/global.ucas",
                "sb/content/paks/pakchunk0-ps5.ucas",
                "sb/content/paks/pakchunk1-ps5.ucas"
            ]
        );
    }

    #[test]
    fn auto_loose_samples_spread_and_judge() {
        let spec = plain_spec();
        assert!(!auto_loose_applies(AUTO_LOOSE_MIN_SIZE - 1, &spec));
        assert!(auto_loose_applies(AUTO_LOOSE_MIN_SIZE, &spec));
        let stored = PackSpec {
            store: true,
            ..spec
        };
        assert!(!auto_loose_applies(AUTO_LOOSE_MIN_SIZE, &stored));
        // 1 GiB of 64 KiB blocks: 32 picks, first and last block included.
        let ks = sample_indices(1 << 30, 16);
        assert_eq!((ks.len(), ks[0], ks[31]), (32, 0, (1 << 14) - 1));
        // 1 MiB blocks: 16 MiB allows 16.
        assert_eq!(sample_indices(1 << 30, 20).len(), 16);
        assert_eq!(sample_indices(3 << 16, 16), [0, 1, 2]);
        let s = |stored_bytes, raw_blocks| Sample {
            raw_bytes: 100,
            stored_bytes,
            blocks: 10,
            raw_blocks,
        };
        assert!(s(96, 0).keeps_loose(), "saves 4%");
        assert!(s(50, 9).keeps_loose(), "90% RAW");
        assert!(!s(50, 8).keeps_loose());
    }
}
