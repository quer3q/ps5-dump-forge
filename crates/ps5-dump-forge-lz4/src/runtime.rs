//! The two embedded `ampr_emu` runtimes (unmodified upstream 0.4.2.1, see `vendor/ampr_emu/README.md`)
//! and recognition of a runtime found in a game folder, by SHA-256 of the whole file.

use sha2::{Digest, Sha256};

/// The release runtime (upstream's `test-pack` build): reads packs, writes no trace.
pub static RELEASE: &[u8] = include_bytes!("../../../vendor/ampr_emu/libSceAmpr.sprx");
/// The tracing runtime (upstream's `test-debug-pack` build): also writes the command journal.
pub static TRACE: &[u8] = include_bytes!("../../../vendor/ampr_emu/libSceAmpr-trace.sprx");

pub const VERSION: &str = "0.4.2.1";
/// Logged on every trace and pack job.
pub const WARNING: &str =
    "ampr_emu 0.4.2.1 is an upstream test build; known issue: some games crash when saving";

const RELEASE_SHA256: &str = "69e6c4d5e4f5fb83c9e01815db5861c4c75734acbf4595cafa50d4c218116d1a";
const TRACE_SHA256: &str = "b44a986f2fa9903e74a34cbbd4681e899cd99196613652cd073ed11f2ca947c2";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKind {
    ForgeRelease,
    ForgeTrace,
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

/// Which runtime `bytes` (the whole `libSceAmpr.sprx`) is.
pub fn classify(bytes: &[u8]) -> RuntimeKind {
    if bytes.is_empty() {
        return RuntimeKind::None;
    }
    match sha256_hex(bytes).as_str() {
        RELEASE_SHA256 => RuntimeKind::ForgeRelease,
        TRACE_SHA256 => RuntimeKind::ForgeTrace,
        _ => RuntimeKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contains(hay: &[u8], needle: &[u8]) -> bool {
        hay.windows(needle.len()).any(|w| w == needle)
    }

    #[test]
    fn pinned_sizes_and_hashes() {
        assert_eq!(RELEASE.len(), 423350);
        assert_eq!(TRACE.len(), 633094);
        assert_eq!(sha256_hex(RELEASE), RELEASE_SHA256);
        assert_eq!(sha256_hex(TRACE), TRACE_SHA256);
        assert_eq!(RELEASE_SHA256.len(), 64);
        assert_eq!(TRACE_SHA256.len(), 64);
    }

    #[test]
    fn both_know_the_pack_formats() {
        for rt in [RELEASE, TRACE] {
            assert!(contains(rt, b"AMPRPAK4"));
            assert!(contains(rt, b"AMPRDAT3"));
        }
    }

    #[test]
    fn classifies() {
        assert_eq!(classify(RELEASE), RuntimeKind::ForgeRelease);
        assert_eq!(classify(TRACE), RuntimeKind::ForgeTrace);
        assert_eq!(classify(&RELEASE[1..]), RuntimeKind::Other);
        assert_eq!(classify(b"x"), RuntimeKind::Other);
        assert_eq!(classify(b""), RuntimeKind::None);
    }

    #[test]
    fn warning_text() {
        assert_eq!(
            WARNING,
            "ampr_emu 0.4.2.1 is an upstream test build; known issue: some games crash when saving"
        );
        assert!(WARNING.contains(VERSION));
    }
}
