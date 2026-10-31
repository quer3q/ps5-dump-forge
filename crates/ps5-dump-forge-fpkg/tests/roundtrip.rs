//! Packages built by the vendored builder read back, byte for byte, as their effective manifest.

mod common;

use common::{assert_matches, build, error_of};
use ps5_dump_forge_fpkg::{FpkgSource, WRONG_PASSCODE};
use ps5upload_fpkg::ImageMode;
use ps5upload_fpkg::source::SourceTree;

#[test]
fn a_kraken_package_reads_back_as_its_manifest() {
    let built = build("kraken", 0, |_| {});
    let mut source = FpkgSource::open(&built.path, None).unwrap();
    assert_matches(&mut source, &built);
    // The artwork the builder moved into the container is served from there.
    assert_eq!(
        built.container_only,
        [
            "sce_sys/icon0.png",
            "sce_sys/pic0.png",
            "sce_sys/pic1.png",
            "sce_sys/pic2.png",
            "sce_sys/snd0.at9"
        ]
    );
    assert!(built.empty_dirs.contains(&"data/empty".to_string()));
    let details = source.details().join("\n");
    assert!(details.contains(common::CONTENT_ID), "{details}");
    assert!(details.contains("layout: Kraken"), "{details}");
    assert!(details.contains("plaintext"), "{details}");
    assert!(source.describe().contains(common::CONTENT_ID));
}

#[test]
fn a_native_package_needs_its_passcode() {
    let built = build("native", 0, |r| {
        r.image_mode = ImageMode::Native;
        r.seed = Some([0x5A; 16]);
    });
    let mut source = FpkgSource::open(&built.path, None).unwrap();
    assert!(source.details().join("\n").contains("native AES-XTS"));
    assert_matches(&mut source, &built);
    let e = error_of(FpkgSource::open(
        &built.path,
        Some("11111111111111111111111111111111"),
    ));
    assert!(e.starts_with(WRONG_PASSCODE), "{e}");
}
