//! Embeds the web UI bundle (`npm run build:http` in `app/` writes `app/dist-http`) as
//! `static ASSETS: &[(&str, &[u8])]` in `$OUT_DIR/assets.rs`: each file by its path relative
//! to the bundle, `/`-separated. `FORGE_WEB_DIST` points elsewhere. Without a bundle, a
//! placeholder page is embedded so `cargo test` never needs node, unless `FORGE_REQUIRE_WEB=1`
//! (`ps5/build.sh`), which makes it a build error.
//!
//! For the PS5 payload, `FORGE_SELF_ELF` names stage 1's ELF (`ps5/build.sh`), embedded as the
//! self-copy blob in `$OUT_DIR/self_elf.rs` (`static SELF_ELF: Option<&[u8]>`, layout in
//! `src/self_copy.rs`): `None` without it, and on every other target.

use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

fn main() {
    println!("cargo:rerun-if-env-changed=FORGE_WEB_DIST");
    println!("cargo:rerun-if-env-changed=FORGE_REQUIRE_WEB");
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let dist = std::env::var_os("FORGE_WEB_DIST")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("../../app/dist-http"));
    // A path that doesn't exist yet reruns this script on every build, so a bundle built
    // later is picked up.
    println!("cargo:rerun-if-changed={}", dist.display());
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());

    let mut files = Vec::new();
    walk(&dist, "", &mut files);
    files.sort();
    if !files.iter().any(|(name, _)| name == "index.html") {
        if std::env::var_os("FORGE_REQUIRE_WEB").is_some_and(|v| v == "1") {
            panic!(
                "FORGE_REQUIRE_WEB=1 but {} has no index.html: run `npm run build:http` in app/",
                dist.display()
            );
        }
        println!(
            "cargo:warning=no web UI bundle in {}; embedding a placeholder page (run `npm run build:http` in app/)",
            dist.display()
        );
        let page = out.join("placeholder.html");
        std::fs::write(
            &page,
            "<!doctype html><meta charset=\"utf-8\"><title>PS5 Dump Forge</title>\
             <p>web UI not built: run <code>npm run build:http</code> in app/</p>\n",
        )
        .unwrap();
        files = vec![("index.html".to_string(), page)];
    }

    let mut code = String::from("static ASSETS: &[(&str, &[u8])] = &[\n");
    for (name, path) in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        let path = path.to_str().expect("bundle paths must be UTF-8");
        code += &format!("    ({name:?}, include_bytes!({path:?})),\n");
    }
    code += "];\n";
    std::fs::write(out.join("assets.rs"), code).unwrap();

    std::fs::write(out.join("self_elf.rs"), self_elf(&out)).unwrap();
}

/// `PDFGSELF`, u64 LE raw length, u64 LE compressed length, SHA-256 of the ELF, the zlib stream.
fn self_elf(out: &Path) -> String {
    println!("cargo:rerun-if-env-changed=FORGE_SELF_ELF");
    let none = "static SELF_ELF: Option<&[u8]> = None;\n".to_string();
    let Some(path) = std::env::var_os("FORGE_SELF_ELF").filter(|p| !p.is_empty()) else {
        return none;
    };
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("ps5") {
        println!("cargo:warning=FORGE_SELF_ELF is only for the PS5 payload; ignored");
        return none;
    }
    let path = PathBuf::from(path);
    println!("cargo:rerun-if-changed={}", path.display());
    let elf =
        std::fs::read(&path).unwrap_or_else(|e| panic!("FORGE_SELF_ELF={}: {e}", path.display()));
    assert!(
        elf.starts_with(b"\x7fELF"),
        "FORGE_SELF_ELF={}: not an ELF",
        path.display()
    );
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    z.write_all(&elf).unwrap();
    let packed = z.finish().unwrap();
    let mut blob = b"PDFGSELF".to_vec();
    blob.extend_from_slice(&(elf.len() as u64).to_le_bytes());
    blob.extend_from_slice(&(packed.len() as u64).to_le_bytes());
    blob.extend_from_slice(Sha256::digest(&elf).as_slice());
    blob.extend_from_slice(&packed);
    let file = out.join("self-elf.bin");
    std::fs::write(&file, blob).unwrap();
    let file = file.to_str().expect("OUT_DIR must be UTF-8");
    format!("static SELF_ELF: Option<&[u8]> = Some(include_bytes!({file:?}));\n")
}

/// Every regular file under `dir` (dotfiles left out), named relative to the bundle.
fn walk(dir: &Path, prefix: &str, files: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let entry = entry.unwrap();
        let name = entry
            .file_name()
            .into_string()
            .expect("bundle names must be UTF-8");
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let rel = format!("{prefix}{name}");
        if path.is_dir() {
            walk(&path, &format!("{rel}/"), files);
        } else {
            files.push((rel, path.canonicalize().unwrap()));
        }
    }
}
