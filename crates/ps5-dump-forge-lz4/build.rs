//! Stores the embedded runtime as one zstd frame (level 22) in `$OUT_DIR/runtimes.rs`;
//! `runtime::zstd()` decodes it once.

use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let path = manifest.join("../../vendor/ampr-emu-ng/libSceAmpr.sprx");
    println!("cargo:rerun-if-changed={}", path.display());
    let raw = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let packed = zstd::bulk::compress(&raw, 22).unwrap();
    let file = out.join("ZSTD.sprx.zst");
    std::fs::write(&file, packed).unwrap();
    let file = file.to_str().expect("OUT_DIR must be UTF-8");
    let code = format!(
        "const ZSTD_PACKED: &[u8] = include_bytes!({file:?});\nconst ZSTD_LEN: usize = {};\n",
        raw.len()
    );
    std::fs::write(out.join("runtimes.rs"), code).unwrap();
}
