//! Read a range through the normal package file-tree API and compare it with an
//! independently decoded artifact.
//!
//! ```text
//! cargo run --release -p ps5upload-fpkg --example roundtrip_stage -- \
//!   <pkg> <expected-file> <path> <offset> <length> [<path> <offset> <length> ...]
//! ```

use ps5upload_fpkg::source::SourceTree;
use sha2::{Digest, Sha256};

fn hex(s: &str) -> u64 {
    u64::from_str_radix(s.trim_start_matches("0x"), 16).expect("offsets are hexadecimal")
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn main() -> ps5upload_fpkg::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert!(
        args.len() >= 5 && (args.len() - 2).is_multiple_of(3),
        "usage: roundtrip_stage <pkg> <expected-file> \
         <path> <offset> <length> [<path> <offset> <length> ...]"
    );

    let expected = std::fs::read(&args[1])?;
    let mut tree = ps5upload_fpkg::pkg_source::open(
        std::path::Path::new(&args[0]),
        ps5upload_fpkg::crypto::DEFAULT_PASSCODE,
    )?;
    for file in tree.files().iter().filter(|f| f.path.contains("pfs")) {
        println!("package entry: {} ({:#x} bytes)", file.path, file.size);
    }
    let mut actual = Vec::with_capacity(expected.len());
    for range in args[2..].as_chunks::<3>().0 {
        let bytes = tree.read_range(&range[0], hex(&range[1]), hex(&range[2]) as usize)?;
        println!(
            "read {} bytes from {} at {}",
            bytes.len(),
            range[0],
            range[1]
        );
        actual.extend_from_slice(&bytes);
    }

    println!("assembled {} bytes", actual.len());
    println!("actual   sha256={}", sha256_hex(&actual));
    println!("expected sha256={}", sha256_hex(&expected));
    assert_eq!(
        actual, expected,
        "PkgSource round-trip differs from the independent artifact"
    );
    println!("MATCH");
    Ok(())
}
