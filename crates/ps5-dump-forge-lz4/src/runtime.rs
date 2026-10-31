//! The embedded `ampr_emu` runtimes (the unmodified upstream 0.4.2.1 release build, see
//! `vendor/ampr_emu/README.md`, and the ampr-emu-ng 0.1.0 release, `vendor/ampr-emu-ng/README.md`)
//! and recognition of a runtime found in a game folder, by SHA-256 of the whole file (the old
//! trace build and the old local ampr-emu-ng build too, no longer shipped), or for ampr-emu-ng
//! by its zstd index magic.

use std::sync::OnceLock;

use sha2::{Digest, Sha256};

// `RELEASE_PACKED`/`RELEASE_LEN`, `ZSTD_PACKED`/`ZSTD_LEN`: each runtime as one zstd frame
// (build.rs), so a binary carries ~0.5 MB of them instead of ~1.1 MB.
include!(concat!(env!("OUT_DIR"), "/runtimes.rs"));

/// A runtime's bytes, decoded once and kept for the process's life.
fn unpacked(cell: &'static OnceLock<Vec<u8>>, packed: &[u8], len: usize) -> &'static [u8] {
    cell.get_or_init(|| {
        let raw = ::zstd::bulk::decompress(packed, len).expect("embedded runtime decodes");
        assert_eq!(raw.len(), len, "embedded runtime length");
        raw
    })
}

/// The release runtime (upstream's `test-pack` build): reads packs, writes no trace.
pub fn release() -> &'static [u8] {
    static CELL: OnceLock<Vec<u8>> = OnceLock::new();
    unpacked(&CELL, RELEASE_PACKED, RELEASE_LEN)
}

/// ampr-emu-ng, the user's runtime: reads zstd packs (`ampr_zstd.index` + `ampr_zstd-NNN.pak`,
/// `docs/PAK.md`) and LZ4 packs. The v0.1.0 release build, byte for byte.
pub fn zstd() -> &'static [u8] {
    static CELL: OnceLock<Vec<u8>> = OnceLock::new();
    unpacked(&CELL, ZSTD_PACKED, ZSTD_LEN)
}

pub const VERSION: &str = "0.4.2.1";
/// Logged on every pack job.
pub const WARNING: &str =
    "ampr_emu 0.4.2.1 is an upstream test build; known issue: some games crash when saving";

const RELEASE_SHA256: &str = "69e6c4d5e4f5fb83c9e01815db5861c4c75734acbf4595cafa50d4c218116d1a";
/// The tracing runtime (upstream's `test-debug-pack` build) Forge once installed: no longer
/// shipped, still recognized so that a conversion replaces it.
const TRACE_SHA256: &str = "b44a986f2fa9903e74a34cbbd4681e899cd99196613652cd073ed11f2ca947c2";
/// [`zstd()`]: ampr-emu-ng v0.1.0's `libSceAmpr.sprx` (`vendor/ampr-emu-ng/SHA256SUMS`).
const ZSTD_SHA256: &str = "69ee3b420c6798fa1b5fb5a9fed75479403bc8a64fe382db301477b1bf705956";
/// The local ampr-emu-ng build Forge embedded before the v0.1.0 release (679712 bytes): no
/// longer shipped, still recognized so that a game carrying it keeps it.
const OLD_ZSTD_SHA256: &str = "0bf2c3e39580d937c20bc320323f6394ff624fb5839fc1160ef27ceea764a4f7";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKind {
    ForgeRelease,
    /// Forge's old tracing runtime: always replaced.
    ForgeTrace,
    /// ampr-emu-ng ([`zstd()`], the old local build or another build of it): reads zstd and LZ4
    /// packs.
    Zstd,
    /// Some other runtime (a different version or build).
    Other,
    /// No runtime (the file is absent or empty).
    None,
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Which runtime `bytes` (the whole `libSceAmpr.sprx`) is. Any build holding the zstd index
/// magic (`AMPRZPK1`) and the LZ4 pack magics is ampr-emu-ng, so an older ng build is kept,
/// not swapped for upstream ampr_emu.
// ponytail: a name check, as `reads_packs`.
pub fn classify(bytes: &[u8]) -> RuntimeKind {
    if bytes.is_empty() {
        return RuntimeKind::None;
    }
    by_hash(&sha256_hex(bytes), bytes)
}

/// [`classify`] for `bytes` whose SHA-256 is `hash` (hex, lowercase).
fn by_hash(hash: &str, bytes: &[u8]) -> RuntimeKind {
    match hash {
        RELEASE_SHA256 => RuntimeKind::ForgeRelease,
        TRACE_SHA256 => RuntimeKind::ForgeTrace,
        ZSTD_SHA256 | OLD_ZSTD_SHA256 => RuntimeKind::Zstd,
        _ if reads_zstd(bytes) => RuntimeKind::Zstd,
        _ => RuntimeKind::Other,
    }
}

/// The ampr_emu version a runtime names itself with (`AMPR_EMU_VERSION`, "0.4.2.1 (c) Drakmor",
/// in its log line): the dotted number before the first " (c) Drakmor". `None` for a runtime
/// that names no version that way (another author's, or one built differently).
pub fn version_of(bytes: &[u8]) -> Option<String> {
    const MARK: &[u8] = b" (c) Drakmor";
    // Every marker must name the same, well-formed version; anything else is `None`.
    let mut found: Option<String> = None;
    let marks = bytes
        .windows(MARK.len())
        .enumerate()
        .filter(|(_, w)| *w == MARK);
    for (at, _) in marks {
        let start = bytes[..at]
            .iter()
            .rposition(|b| !(b.is_ascii_digit() || *b == b'.'))
            .map_or(0, |i| i + 1);
        let v = std::str::from_utf8(&bytes[start..at]).ok()?;
        let numbers = v.split('.').all(|p| p.parse::<u64>().is_ok());
        if !numbers || !v.contains('.') || found.as_deref().is_some_and(|f| f != v) {
            return None;
        }
        found = Some(v.to_owned());
    }
    found
}

/// Whether a runtime holds the pack format's magics (`AMPRPAK4` manifest, `AMPRDAT3` volumes),
/// as both builds Forge ships do; upstream's no-pack build reads no packs.
// ponytail: a name check, not proof the runtime reads this exact layout; the version decides
// the rest.
pub fn reads_packs(bytes: &[u8]) -> bool {
    let has = |m: &[u8]| bytes.windows(m.len()).any(|w| w == m);
    has(b"AMPRPAK4") && has(b"AMPRDAT3")
}

/// Whether a runtime holds the zstd index magic (`AMPRZPK1`) and reads LZ4 packs: ampr-emu-ng.
fn reads_zstd(bytes: &[u8]) -> bool {
    const MAGIC: &[u8] = b"AMPRZPK1";
    reads_packs(bytes) && bytes.windows(MAGIC.len()).any(|w| w == MAGIC)
}

/// Whether `version` (dotted numbers) is [`VERSION`] or newer: such a runtime is kept as it is.
pub fn not_older(version: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> { v.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    let (mut a, mut b) = (parts(version), parts(VERSION));
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a >= b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_sizes_and_hashes() {
        assert_eq!(release().len(), 423350);
        assert_eq!(sha256_hex(release()), RELEASE_SHA256);
        assert_eq!(RELEASE_SHA256.len(), 64);
        assert_eq!(TRACE_SHA256.len(), 64);
        assert_eq!(zstd().len(), 667408);
        assert_eq!(sha256_hex(zstd()), ZSTD_SHA256);
        assert_eq!(OLD_ZSTD_SHA256.len(), 64);
        assert!(
            OLD_ZSTD_SHA256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        );
        assert!(reads_zstd(zstd()));
        assert_eq!(version_of(zstd()), None);
    }

    #[test]
    fn classifies() {
        assert_eq!(classify(release()), RuntimeKind::ForgeRelease);
        assert_eq!(classify(zstd()), RuntimeKind::Zstd);
        // The old local ampr-emu-ng build (no longer in the repo) is known by its hash alone.
        assert_eq!(by_hash(OLD_ZSTD_SHA256, b"x"), RuntimeKind::Zstd);
        assert_eq!(by_hash(ZSTD_SHA256, b"x"), RuntimeKind::Zstd);
        assert_eq!(by_hash(TRACE_SHA256, b"x"), RuntimeKind::ForgeTrace);
        // Another ampr-emu-ng build: the zstd magic and the pack magics, any hash.
        assert_eq!(classify(&zstd()[..zstd().len() - 1]), RuntimeKind::Zstd);
        assert_eq!(classify(b"AMPRPAK4 AMPRDAT3 AMPRZPK1"), RuntimeKind::Zstd);
        assert_eq!(classify(b"AMPRPAK4 AMPRZPK1"), RuntimeKind::Other);
        assert_eq!(classify(b"AMPRPAK4 AMPRDAT3"), RuntimeKind::Other);
        assert_eq!(classify(&release()[1..]), RuntimeKind::Other);
        assert_eq!(classify(b"x"), RuntimeKind::Other);
        assert_eq!(classify(b""), RuntimeKind::None);
    }
}
