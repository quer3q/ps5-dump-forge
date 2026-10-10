//! Input shape of the `lz4_unpack` target, shared with the seed generator (`#[path]`):
//! `[manifest_len u16 LE][crc_len u16 LE][manifest][crc][volume 0]`. The tree it makes holds
//! `ampr_assets.index`, `ampr_assets-000.pak` (when non-empty) and `.crc` (when `crc_len` > 0), plus the
//! loose `eboot.bin` ([`LOOSE_EBOOT`]: 100 bytes of 7) that the seeds' manifest lists as a loose record.

use std::collections::BTreeMap;

use ps5_dump_forge_lz4::{CRC_SIDECAR, MANIFEST, volume_name};
use ps5upload_fpkg::source::{SourceFile, SourceTree};
use ps5upload_fpkg::{Error, Result};

pub struct Mem {
    files: Vec<SourceFile>,
    data: BTreeMap<String, Vec<u8>>,
}

impl Mem {
    pub fn new(entries: impl IntoIterator<Item = (String, Vec<u8>)>) -> Self {
        let data: BTreeMap<String, Vec<u8>> = entries.into_iter().collect();
        let files = data
            .iter()
            .map(|(p, d)| SourceFile {
                path: p.clone(),
                size: d.len() as u64,
            })
            .collect();
        Self { files, data }
    }
}

impl SourceTree for Mem {
    fn files(&self) -> &[SourceFile] {
        &self.files
    }
    fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        self.data
            .get(path)
            .cloned()
            .ok_or_else(|| Error::Format(format!("no {path}")))
    }
    fn describe(&self) -> String {
        "memory".into()
    }
}

/// The loose file the seed manifests list (`unpack` checks loose records against the tree).
pub const LOOSE_EBOOT: (&str, u8, usize) = ("eboot.bin", 7, 100);

/// The tree for one fuzz input.
pub fn tree(data: &[u8]) -> Mem {
    let word = |at: usize| {
        data.get(at..at + 2)
            .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
    };
    let rest = data.get(4..).unwrap_or(&[]);
    let m = (word(0) as usize).min(rest.len());
    let c = (word(2) as usize).min(rest.len() - m);
    let (name, byte, len) = LOOSE_EBOOT;
    let mut entries = vec![
        (MANIFEST.to_string(), rest[..m].to_vec()),
        (name.to_string(), vec![byte; len]),
    ];
    if c > 0 {
        entries.push((CRC_SIDECAR.to_string(), rest[m..m + c].to_vec()));
    }
    if rest.len() > m + c {
        entries.push((volume_name(0), rest[m + c..].to_vec()));
    }
    Mem::new(entries)
}

/// The inverse, for seeds.
pub fn join(manifest: &[u8], crc: &[u8], volume: &[u8]) -> Vec<u8> {
    let mut v = (manifest.len() as u16).to_le_bytes().to_vec();
    v.extend((crc.len() as u16).to_le_bytes());
    v.extend_from_slice(manifest);
    v.extend_from_slice(crc);
    v.extend_from_slice(volume);
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use ps5_dump_forge_lz4::reader::unpack;

    /// The generated `lz4_unpack` seeds (not the fuzzer's own finds in the corpus) must open and read back whole.
    #[test]
    fn unpack_seeds_decode() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/lz4_unpack");
        for name in ["pack", "pack-nocrc"] {
            let data = std::fs::read(dir.join(name)).expect("run fuzz/seed.sh first");
            let mut u = unpack(Box::new(tree(&data))).expect("seed must unpack");
            let files: Vec<_> = u.files().to_vec();
            assert!(!files.is_empty());
            for f in files {
                let d = u.read(&f.path).expect("seed file must read");
                assert_eq!(d.len() as u64, f.size, "{}", f.path);
            }
        }
    }
}
