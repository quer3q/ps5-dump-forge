//! The embedded AMPR runtime (ampr-emu-ng v0.5.1) and recognition of a game's runtime by SHA-256,
//! or for ampr-emu-ng by its magics.

use std::sync::OnceLock;

use sha2::{Digest, Sha256};

// `ZSTD_PACKED`/`ZSTD_LEN`: the runtime as one zstd frame (build.rs), decoded on first use.
include!(concat!(env!("OUT_DIR"), "/runtimes.rs"));

/// A runtime's bytes, decoded once and kept for the process's life.
fn unpacked(cell: &'static OnceLock<Vec<u8>>, packed: &[u8], len: usize) -> &'static [u8] {
    cell.get_or_init(|| {
        let raw = ::zstd::bulk::decompress(packed, len).expect("embedded runtime decodes");
        assert_eq!(raw.len(), len, "embedded runtime length");
        raw
    })
}

/// ampr-emu-ng v0.5.1's `libSceAmpr.sprx`, byte for byte: reads zstd and LZ4 packs.
pub fn zstd() -> &'static [u8] {
    static CELL: OnceLock<Vec<u8>> = OnceLock::new();
    unpacked(&CELL, ZSTD_PACKED, ZSTD_LEN)
}

/// The last drakmor ampr_emu release Forge bundled (no longer shipped; still reported).
pub const VERSION: &str = "0.4.2.1";
/// The version the bundled ampr-emu-ng ([`zstd()`]) names itself with (pinned by a test).
pub const NG_VERSION: &str = "0.5.1";
/// drakmor's ampr_emu 0.4.2.1 release build Forge once installed (`RuntimeKind::ForgeRelease`).
const RELEASE_SHA256: &str = "69e6c4d5e4f5fb83c9e01815db5861c4c75734acbf4595cafa50d4c218116d1a";
/// The tracing runtime (`test-debug-pack`) Forge once installed: recognized so a conversion
/// replaces it.
const TRACE_SHA256: &str = "b44a986f2fa9903e74a34cbbd4681e899cd99196613652cd073ed11f2ca947c2";
/// [`zstd()`]: the ampr-emu-ng v0.5.1 release's `libSceAmpr.sprx`
/// (`vendor/ampr-emu-ng/SHA256SUMS`).
const ZSTD_SHA256: &str = "7ca61afc444f0b44d2f005506dde16cb43e74dae4b1e045323b291b8db1c3d5d";
// Older ampr-emu-ng builds, recognized by hash: releases by version, PREVIOUS = v0.1.0, NG_6972875
// = a local build before v0.3.0, OLD = the local build before v0.1.0.
const V041_ZSTD_SHA256: &str = "9c7c41b3216bc4f877d67e3c2e5c9c8f9ffabdd5065f4f3fe0dd98fa7228386f";
const V040_ZSTD_SHA256: &str = "8320eb65c16565f42031b9d88b15a447039f0fd7e5694faf5f1cb84cafdba218";
const V035_ZSTD_SHA256: &str = "998f2029b250d954725f864b57f2eee72aefbfb88054a3f1c3df550d4d6c8261";
const V034_ZSTD_SHA256: &str = "7d1bb260f954b755ed687733e52ccfa38c31440a9b88a6fbba5b3b75e08bfa26";
const V033_ZSTD_SHA256: &str = "1c1051f91bc7cba19b15362b2b832114b53a4c3d6193f50c90fdf1ae5d78d268";
const V032_ZSTD_SHA256: &str = "9a8f553a68d4e6e6be11bd9f8d5519b5faa68560761881966abff1dd65bf19f7";
const V031_ZSTD_SHA256: &str = "bdd93ce2fbcc2d0986b16d77604cc0770d994ccc124b9051ba9f53aeac39f56f";
const V030_ZSTD_SHA256: &str = "5f39ca8ebe169aa15d8a8ba8ec2f9cd4195f509d55daaf4b4edbfa80b145035c";
const NG_6972875_SHA256: &str = "dc7d59d551071dc7ad2002865efe9e54ea0393523d1258ec1ce9fc7389a45bc4";
const V020_ZSTD_SHA256: &str = "7acb2173d9691b5dfb94ea4007660920b7250111c5fd7a86c6eb91b763ad3d76";
const PREVIOUS_ZSTD_SHA256: &str =
    "69ee3b420c6798fa1b5fb5a9fed75479403bc8a64fe382db301477b1bf705956";
const OLD_ZSTD_SHA256: &str = "0bf2c3e39580d937c20bc320323f6394ff624fb5839fc1160ef27ceea764a4f7";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKind {
    ForgeRelease,
    /// Forge's old tracing runtime: always replaced.
    ForgeTrace,
    /// ampr-emu-ng ([`zstd()`] or another build): reads zstd and LZ4 packs.
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

/// Which runtime `bytes` (the whole `libSceAmpr.sprx`) is; any build with the zstd and LZ4 magics
/// is ampr-emu-ng.
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
        ZSTD_SHA256 | V041_ZSTD_SHA256 | V040_ZSTD_SHA256 | V035_ZSTD_SHA256 | V034_ZSTD_SHA256
        | V033_ZSTD_SHA256 | V032_ZSTD_SHA256 | V031_ZSTD_SHA256 | V030_ZSTD_SHA256
        | NG_6972875_SHA256 | V020_ZSTD_SHA256 | PREVIOUS_ZSTD_SHA256 | OLD_ZSTD_SHA256 => {
            RuntimeKind::Zstd
        }
        _ if reads_zstd(bytes) => RuntimeKind::Zstd,
        _ => RuntimeKind::Other,
    }
}

/// The ampr_emu version before the first " (c) Drakmor" in its log line, else `None`.
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

/// The version after the first `ampr-emu-ng ` in an ampr-emu-ng binary, else `None`.
pub fn ng_version_of(bytes: &[u8]) -> Option<String> {
    const MARK: &[u8] = b"ampr-emu-ng ";
    bytes
        .windows(MARK.len())
        .enumerate()
        .filter(|(_, w)| *w == MARK)
        .find_map(|(at, _)| {
            let rest = &bytes[at + MARK.len()..];
            let end = rest
                .iter()
                .position(|b| !(b.is_ascii_digit() || *b == b'.'))
                .unwrap_or(rest.len());
            let v = std::str::from_utf8(&rest[..end]).ok()?;
            let numbers = v.split('.').all(|p| p.parse::<u64>().is_ok());
            (numbers && v.contains('.')).then(|| v.to_owned())
        })
}

/// Whether a runtime holds the LZ4 pack magics (`AMPRPAK4`, `AMPRDAT3`).
// ponytail: a name check, not proof it reads this exact layout; the version decides the rest.
pub fn reads_packs(bytes: &[u8]) -> bool {
    let has = |m: &[u8]| bytes.windows(m.len()).any(|w| w == m);
    has(b"AMPRPAK4") && has(b"AMPRDAT3")
}

/// Whether a runtime holds the zstd index magic (`AMPRZPK1`) and reads LZ4 packs: ampr-emu-ng.
fn reads_zstd(bytes: &[u8]) -> bool {
    const MAGIC: &[u8] = b"AMPRZPK1";
    reads_packs(bytes) && bytes.windows(MAGIC.len()).any(|w| w == MAGIC)
}

/// Whether an ampr-emu-ng `version` is [`NG_VERSION`] or newer (else replaced by the bundled
/// build).
pub fn ng_not_older(version: &str) -> bool {
    at_least(version, NG_VERSION)
}

fn at_least(version: &str, base: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> { v.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    let (mut a, mut b) = (parts(version), parts(base));
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
        assert_eq!(RELEASE_SHA256.len(), 64);
        assert_eq!(TRACE_SHA256.len(), 64);
        assert_eq!(zstd().len(), 706976);
        assert_eq!(sha256_hex(zstd()), ZSTD_SHA256);
        assert_eq!(OLD_ZSTD_SHA256.len(), 64);
        assert!(
            OLD_ZSTD_SHA256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        );
        assert!(reads_zstd(zstd()));
        assert_eq!(version_of(zstd()), None);
        assert_eq!(ng_version_of(zstd()).as_deref(), Some(NG_VERSION));
    }

    #[test]
    fn classifies() {
        // ampr_emu's release build (no longer in the repo) is known by its hash alone.
        assert_eq!(by_hash(RELEASE_SHA256, b"x"), RuntimeKind::ForgeRelease);
        assert_eq!(classify(zstd()), RuntimeKind::Zstd);
        // The old local ampr-emu-ng build (no longer in the repo) is known by its hash alone.
        assert_eq!(by_hash(OLD_ZSTD_SHA256, b"x"), RuntimeKind::Zstd);
        assert_eq!(by_hash(PREVIOUS_ZSTD_SHA256, b"x"), RuntimeKind::Zstd);
        assert_eq!(by_hash(V041_ZSTD_SHA256, b"x"), RuntimeKind::Zstd);
        assert_eq!(by_hash(V040_ZSTD_SHA256, b"x"), RuntimeKind::Zstd);
        assert_eq!(by_hash(V035_ZSTD_SHA256, b"x"), RuntimeKind::Zstd);
        assert_eq!(by_hash(V034_ZSTD_SHA256, b"x"), RuntimeKind::Zstd);
        assert_eq!(by_hash(V033_ZSTD_SHA256, b"x"), RuntimeKind::Zstd);
        assert_eq!(by_hash(V032_ZSTD_SHA256, b"x"), RuntimeKind::Zstd);
        assert_eq!(by_hash(V031_ZSTD_SHA256, b"x"), RuntimeKind::Zstd);
        assert_eq!(by_hash(V030_ZSTD_SHA256, b"x"), RuntimeKind::Zstd);
        assert_eq!(by_hash(V020_ZSTD_SHA256, b"x"), RuntimeKind::Zstd);
        assert_eq!(by_hash(NG_6972875_SHA256, b"x"), RuntimeKind::Zstd);
        assert_eq!(by_hash(ZSTD_SHA256, b"x"), RuntimeKind::Zstd);
        assert_eq!(by_hash(TRACE_SHA256, b"x"), RuntimeKind::ForgeTrace);
        // Another ampr-emu-ng build: the zstd magic and the pack magics, any hash.
        assert_eq!(classify(&zstd()[..zstd().len() - 1]), RuntimeKind::Zstd);
        assert_eq!(classify(b"AMPRPAK4 AMPRDAT3 AMPRZPK1"), RuntimeKind::Zstd);
        assert_eq!(classify(b"AMPRPAK4 AMPRZPK1"), RuntimeKind::Other);
        assert_eq!(classify(b"AMPRPAK4 AMPRDAT3"), RuntimeKind::Other);
        assert_eq!(
            classify(b"AMPRPAK4 AMPRDAT3 0.4.2.1 (c) Drakmor"),
            RuntimeKind::Other
        );
        assert_eq!(classify(b"x"), RuntimeKind::Other);
        assert_eq!(classify(b""), RuntimeKind::None);
    }

    #[test]
    fn versions() {
        // ampr_emu names itself in its log line; every marker must agree.
        let build = b"x 0.4.2.1 (c) Drakmor y 0.4.2.1 (c) Drakmor";
        assert_eq!(version_of(build).as_deref(), Some("0.4.2.1"));
        assert_eq!(version_of(b"0.4.2.1 (c) Drakmor 0.5 (c) Drakmor"), None);
        assert_eq!(version_of(b"v (c) Drakmor"), None);
        assert_eq!(version_of(b"ampr-emu-ng 0.5.1"), None);
        assert_eq!(
            ng_version_of(b"x ampr-emu-ng 0.5.1 y").as_deref(),
            Some("0.5.1")
        );
        assert!(ng_not_older(NG_VERSION) && !ng_not_older("0.4.1"));
    }
}
