//! Stores the two embedded runtimes (`src/runtime.rs`) zstd-compressed: `$OUT_DIR/runtimes.rs`
//! holds each one's frame (level 22) and raw length; `runtime::release()`/`zstd()` decode it
//! once, on first use. About 0.6 MB less in every binary that links this crate.

use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let mut code = String::new();
    for (name, rel) in [
        ("RELEASE", "../../vendor/ampr_emu/libSceAmpr.sprx"),
        ("ZSTD", "../../vendor/ampr-emu-ng/libSceAmpr.sprx"),
    ] {
        let path = manifest.join(rel);
        println!("cargo:rerun-if-changed={}", path.display());
        let raw = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let packed = zstd::bulk::compress(&raw, 22).unwrap();
        let file = out.join(format!("{name}.sprx.zst"));
        std::fs::write(&file, packed).unwrap();
        let file = file.to_str().expect("OUT_DIR must be UTF-8");
        code += &format!(
            "const {name}_PACKED: &[u8] = include_bytes!({file:?});\nconst {name}_LEN: usize = {};\n",
            raw.len()
        );
    }
    std::fs::write(out.join("runtimes.rs"), code).unwrap();
}
