//! The stored-Kraken diagnostic layout (every block raw): only `PS5UPLOAD_FPKG_KRAKEN_STORE=1`
//! selects it, so this binary holds one test and sets the variable before anything else runs.

mod common;

use common::{assert_matches, build};
use ps5_dump_forge_fpkg::FpkgSource;

#[test]
fn a_stored_kraken_package_reads_back_as_its_manifest() {
    // SAFETY: the only test in this binary, so no other thread reads the environment yet.
    unsafe { std::env::set_var("PS5UPLOAD_FPKG_KRAKEN_STORE", "1") };
    let built = build("store", 0, |r| r.env_overrides = true);
    let mut source = FpkgSource::open(&built.path, None).unwrap();
    let details = source.details().join("\n");
    assert!(details.contains("layout: Kraken"), "{details}");
    let raw = details
        .lines()
        .find(|l| l.starts_with("layout:"))
        .unwrap()
        .to_string();
    // Every block is stored raw: "N blocks (N stored raw)".
    let numbers: Vec<&str> = raw
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .collect();
    assert_eq!(numbers.len(), 2, "{raw}");
    assert_eq!(numbers[0], numbers[1], "{raw}");
    assert_matches(&mut source, &built);
}
