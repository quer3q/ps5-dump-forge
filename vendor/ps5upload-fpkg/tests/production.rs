//! `BuildRequest::production`: the console's settings whatever the environment says.
//!
//! The only test in its binary, because it sets process-wide environment variables.

use std::path::PathBuf;

use ps5upload_fpkg::build::{self, BuildRequest};
use ps5upload_fpkg::inner::MetaCodec;
use ps5upload_fpkg::kraken::Level;
use ps5upload_fpkg::{cnt, fih, ImageMode, PkgFile};

const CONTENT_ID: &str = "UP0000-PPSA01234_00-TESTGAME00000000";

#[test]
fn a_production_request_ignores_every_environment_override() {
    let root = std::env::temp_dir().join(format!("fpkg-production-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (src, out) = (root.join("src"), root.join("out"));
    std::fs::create_dir_all(src.join("sce_sys")).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(src.join("eboot.bin"), vec![0x5A; 4 << 20]).unwrap();
    std::fs::write(
        src.join("sce_sys/param.json"),
        format!("{{\"contentId\":\"{CONTENT_ID}\",\"titleId\":\"PPSA01234\"}}"),
    )
    .unwrap();

    // Every variable the crate knows, each set to what would change or break a build.
    let missing: PathBuf = root.join("no/such/place");
    for (name, value) in [
        ("PS5UPLOAD_FPKG_IMAGE_MODE", "native".into()),
        ("PS5UPLOAD_FPKG_META_CODEC", "zlib".into()),
        ("PS5UPLOAD_FPKG_KRAKEN", "0".into()),
        ("PS5UPLOAD_FPKG_LEVEL", "fast".into()),
        ("PS5UPLOAD_FPKG_CHUNKS", "7".into()),
        ("PS5UPLOAD_FPKG_FW", "9.99".into()),
        ("PS5UPLOAD_FPKG_KRAKEN_STORE", "1".into()),
        ("PS5UPLOAD_FPKG_DRM_TYPE", "0x0".into()),
        (
            "PS5UPLOAD_FPKG_PARAM_FILE",
            missing.join("param.json").into_os_string(),
        ),
        ("PS5UPLOAD_FPKG_SPOOL_DIR", missing.clone().into_os_string()),
    ] {
        std::env::set_var(name, value);
    }

    // `new` still honours them, as upstream does.
    let new = BuildRequest::new(&src, &out);
    assert_eq!(new.image_mode, ImageMode::Native);
    assert_eq!(new.metadata_codec, MetaCodec::Zlib);
    assert!(!new.kraken);
    assert!(new.env_overrides);
    // ... during the build too: the param file it names does not exist.
    let error = build::build(&new, &mut |_| {}).err().unwrap();
    assert!(error.to_string().contains("No such file") || error.to_string().contains("os error"));

    let request = BuildRequest::production(&src, &out);
    assert_eq!(request.image_mode, ImageMode::PlaintextNoAuth);
    assert_eq!(request.metadata_codec, MetaCodec::Stored);
    assert!(request.kraken);
    assert_eq!(request.level, Level::Balanced);
    assert_eq!(
        request.playgo_chunks,
        ps5upload_fpkg::playgo::DEFAULT_CHUNKS
    );
    assert_eq!(request.firmware, None);
    assert_eq!(request.threads, None);
    assert!(!request.env_overrides);

    // The build reads none of them either: the param file and spool folder do not exist, so
    // reading them would fail the build; the DRM type and the stored blocks would show.
    let report = build::build(&request, &mut |_| {}).unwrap();
    assert!(report.verify.ok(), "{}", report.verify);
    assert!(report
        .verify
        .checks
        .iter()
        .any(|c| c.name == "outer blocks match their imagedigs entry" && c.ok));
    let mut pkg = PkgFile::open(&report.path).unwrap();
    let head = pkg.read_at(0, fih::HEADER_LEN).unwrap();
    let parsed = fih::parse(&head).unwrap();
    let container = cnt::read(&mut pkg, parsed.cnt_offset).unwrap();
    let drm = u32::from_be_bytes(container.bytes[0x70..0x74].try_into().unwrap());
    assert_eq!(drm, ps5upload_fpkg::cnt_write::LICENSED_DRM_TYPE);
    // 4 MiB of one byte: compressed, not stored, the package is far smaller than the source.
    assert!(report.size < 2 << 20, "{} bytes", report.size);

    std::fs::remove_dir_all(&root).ok();
}
