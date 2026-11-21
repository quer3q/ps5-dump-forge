//! End to end: `edit --check --json` against a Forge-built exFAT image through the real binary.

use std::path::{Path, PathBuf};
use std::process::Command;

fn dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn edit_check_reports_in_place_feasibility() {
    let root = dir("edit-check");
    let src = root.join("src");
    std::fs::create_dir_all(src.join("sce_sys")).unwrap();
    std::fs::write(
        src.join("eboot.bin"),
        b"\x7fELF fake eboot importing libSceAmpr.sprx",
    )
    .unwrap();
    std::fs::write(
        src.join("sce_sys/param.json"),
        br#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-TESTTESTTESTTEST"}"#,
    )
    .unwrap();
    std::fs::write(src.join("data.bin"), vec![7u8; 100_000]).unwrap();

    let forge = || Command::new(env!("CARGO_BIN_EXE_ps5-dump-forge"));
    let image = root.join("game.exfat");
    let run = forge()
        .arg("convert")
        .arg(&src)
        .args(["--to", "exfat", "-o"])
        .arg(&image)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );

    let new = root.join("new.sprx");
    std::fs::write(&new, b"a replacement runtime").unwrap();
    let run = forge()
        .arg("edit")
        .arg(&image)
        .arg("--add")
        .arg(format!("{}:fakelib/libSceAmpr.sprx", new.display()))
        .args(["--check", "--json"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let check: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(check["in_place"], serde_json::json!([]), "{check}");
    let needed = check["needed_inside"].as_u64().unwrap();
    let free = check["free_inside"].as_u64().unwrap();
    assert!(needed > 0, "{check}");
    assert!(free >= needed, "{check}");
}
