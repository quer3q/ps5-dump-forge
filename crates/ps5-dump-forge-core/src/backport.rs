//! What a game's `fakelib/` holds, and leaving its backport out of a conversion.
//!
//! ShadowMountPlus mounts `fakelib/` (or `fakelib2/`) over the game's system libraries and
//! says "Backport applied" for any of it. Most dumps carry homebrew emulators there, not a
//! backport: each one names its project, in the file it reads from the game root
//! (`ampr_emu.index`, `dlc_emu.ini`, `playgo_stub.dat`) or its build path (`dlc_emu`'s
//! `libSceGameUpdate.sprx` has only that). The rest are system libraries from newer firmware:
//! the backport proper. Removing it is only safe when `eboot.bin`'s SDK was left alone; a
//! backport that lowered it can't be undone, its original values are gone.

use std::collections::HashSet;

use ps5upload_fpkg::source::{SourceFile, SourceTree};
use serde_json::Value;

use crate::Emulator;
use crate::inspect::is_fakelib;

/// An emulator's marker (its project's name, in its files and build path), and its name.
const EMULATORS: &[(&[u8], &str)] = &[
    (b"ampr_emu", "AMPR"),
    (b"dlc_emu", "DLC"),
    (b"playgo_stub", "PlayGo"),
    (b"PlayGo PRX stub", "PlayGo"),
];

/// Libraries are a few hundred KiB; the marker is searched in this much of one.
const MAX_SCAN: u64 = 64 * 1024 * 1024;

/// A `fakelib`/`fakelib2`'s files: backport libraries, and the emulators kept beside them.
#[derive(Debug, Default)]
pub(crate) struct Fakelib {
    pub libs: Vec<String>,
    pub emulators: Vec<Emulator>,
}

/// The emulator `path` is, by its marker; `None` for a backport library. Fails closed: a
/// file that can't be read whole counts as an emulator (`Other`), so it is kept.
/// ponytail: `/app0/` is the homebrew signal (emulators read their own files from the game
/// root); Sony system libraries are expected not to carry it, and one that does is kept, the
/// safe side.
fn emulator(tree: &mut dyn SourceTree, path: &str, size: u64) -> Option<&'static str> {
    const OTHER: Option<&str> = Some("Other");
    let Some(len) = usize::try_from(size).ok().filter(|_| size <= MAX_SCAN) else {
        return OTHER;
    };
    let Ok(bytes) = tree.read_range(path, 0, len) else {
        return OTHER;
    };
    let has = |marker: &[u8]| bytes.windows(marker.len()).any(|w| w == marker);
    EMULATORS
        .iter()
        .find(|(marker, _)| has(marker))
        .map(|(_, name)| *name)
        .or_else(|| has(b"/app0/").then_some("Other"))
}

pub(crate) fn classify(tree: &mut dyn SourceTree) -> Fakelib {
    let files: Vec<SourceFile> = tree
        .files()
        .iter()
        .filter(|f| is_fakelib(&f.path))
        .cloned()
        .collect();
    let mut found = Fakelib::default();
    for f in files {
        match emulator(tree, &f.path, f.size) {
            Some(name) => found.emulators.push(Emulator {
                path: f.path,
                name: name.to_string(),
            }),
            None => found.libs.push(f.path),
        }
    }
    found
}

/// Why the backport can't be removed: `eboot.bin`'s SDK is lower than `param.json`'s
/// `sdkVersion` (the whole SDK word), or either is unreadable. `None`: removable.
/// `eboot.bin` alone: a backport patches it first, untouched modules can't hide that, and
/// other files (`sce_sys/about/right.sprx` is SDK 1.00) legitimately sit lower.
pub(crate) fn blocked(tree: &mut dyn SourceTree, param: Option<&Value>) -> Option<String> {
    let declared = param
        .and_then(|p| p.get("sdkVersion"))
        .and_then(Value::as_str);
    let Some((want, shown)) = declared.and_then(|w| Some((sdk_word(w)?, w))) else {
        return Some(
            "remove backport: param.json has no readable sdkVersion, so whether the backport \
             lowered the executables' SDK can't be told"
                .to_string(),
        );
    };
    let eboot = tree
        .files()
        .iter()
        .find(|f| f.path.eq_ignore_ascii_case("eboot.bin"))
        .map(|f| f.path.clone());
    let sdk = eboot.and_then(|p| crate::sdk::ps5_sdk(tree, &p));
    let Some(have) = sdk.filter(|&w| crate::sdk::firmware(w).is_some()) else {
        return Some(
            "remove backport: eboot.bin has no readable SDK version, so whether the backport \
             lowered it can't be told"
                .to_string(),
        );
    };
    // BCD compares as numbers; the whole word, so a lower build of the same SDK counts.
    (have < want).then(|| {
        format!(
            "remove backport: eboot.bin's SDK {} ({have:#010x}) is lower than param.json's \
             sdkVersion {} ({want:#010x}); the backport lowered it and the original can't be restored",
            crate::sdk::firmware(have).unwrap_or_default(),
            crate::inspect::bcd(shown).unwrap_or_default(),
        )
    })
}

/// `0x0900000000000000` (or the 32-bit `0x09000000`) as `0x09000000`: the word an
/// executable's SDK compares with (BCD major.minor, then the build).
fn sdk_word(word: &str) -> Option<u32> {
    let hex = word
        .strip_prefix("0x")
        .or_else(|| word.strip_prefix("0X"))?;
    let width = if hex.len() <= 8 { 8 } else { 16 };
    let hex = format!("{hex:0>width$}");
    let digits = hex.bytes().all(|b| b.is_ascii_hexdigit());
    if word.len() == 2
        || hex.len() != width
        || !digits
        || !hex[..4].bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    u32::from_str_radix(&hex[..8], 16).ok()
}

/// A source without some of its files. A top-level `fakelib`/`fakelib2` left with nothing in
/// it goes too, empty folders inside included: an empty overlay would hide the console's
/// own system libraries.
pub(crate) struct Without {
    inner: Box<dyn SourceTree>,
    files: Vec<SourceFile>,
    empty_dirs: Vec<String>,
}

impl Without {
    pub(crate) fn new(inner: Box<dyn SourceTree>, drop: &[String]) -> Self {
        let drop: HashSet<&str> = drop.iter().map(String::as_str).collect();
        let files: Vec<SourceFile> = inner
            .files()
            .iter()
            .filter(|f| !drop.contains(f.path.as_str()))
            .cloned()
            .collect();
        let top = |p: &str| p.split('/').next().unwrap_or(p).to_string();
        let busy: HashSet<String> = files.iter().map(|f| top(&f.path)).collect();
        let empty_dirs = inner
            .empty_dirs()
            .iter()
            .filter(|d| {
                let t = top(d);
                let fakelib =
                    t.eq_ignore_ascii_case("fakelib") || t.eq_ignore_ascii_case("fakelib2");
                !fakelib || busy.contains(&t)
            })
            .cloned()
            .collect();
        Self {
            inner,
            files,
            empty_dirs,
        }
    }
}

impl SourceTree for Without {
    fn files(&self) -> &[SourceFile] {
        &self.files
    }

    fn read(&mut self, path: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
        self.inner.read(path)
    }

    fn read_range(
        &mut self,
        path: &str,
        offset: u64,
        len: usize,
    ) -> ps5upload_fpkg::Result<Vec<u8>> {
        self.inner.read_range(path, offset, len)
    }

    fn empty_dirs(&self) -> &[String] {
        &self.empty_dirs
    }

    fn describe(&self) -> String {
        self.inner.describe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk::tests::Files;
    use crate::sdk::{
        MODULE_PARAM_MAGIC, PROCESS_PARAM_MAGIC, PT_SCE_MODULE_PARAM, PT_SCE_PROCPARAM, test_elf,
    };

    fn lib_with(marker: &[u8]) -> Vec<u8> {
        let mut f = vec![0u8; 64];
        f.extend_from_slice(b"/app0/");
        f.extend_from_slice(marker);
        f
    }

    #[test]
    fn emulators_are_told_from_backport_libraries() {
        let mut tree = Files::new(vec![
            ("fakelib/libSceAmpr.sprx", lib_with(b"ampr_emu.index")),
            ("fakelib/libSceAppContent.sprx", lib_with(b"dlc_emu.ini")),
            ("FAKELIB2/libScePlayGo.sprx", lib_with(b"playgo_stub.dat")),
            ("fakelib/libSceAgc.sprx", vec![0x7F, b'E', b'L', b'F', 0, 0]),
            ("fakelib/libhomebrew.sprx", lib_with(b"its_own.cfg")),
            // Only its build path names the project.
            (
                "fakelib/libSceGameUpdate.sprx",
                b"C:/dev/ps5/dlc_emu/out/Prospero_Release/libSceGameUpdate.prx".to_vec(),
            ),
            ("eboot.bin", lib_with(b"dlc_emu.ini")),
        ]);
        let found = classify(&mut tree);
        assert_eq!(found.libs, ["fakelib/libSceAgc.sprx"]);
        let names: Vec<(&str, &str)> = found
            .emulators
            .iter()
            .map(|e| (e.path.as_str(), e.name.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                ("fakelib/libSceAmpr.sprx", "AMPR"),
                ("fakelib/libSceAppContent.sprx", "DLC"),
                ("FAKELIB2/libScePlayGo.sprx", "PlayGo"),
                ("fakelib/libhomebrew.sprx", "Other"),
                ("fakelib/libSceGameUpdate.sprx", "DLC"),
            ]
        );
    }

    /// Lists one file it can't read, and one too big to search.
    struct Unread(Vec<SourceFile>);

    impl SourceTree for Unread {
        fn files(&self) -> &[SourceFile] {
            &self.0
        }
        fn read(&mut self, _: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
            Err(ps5upload_fpkg::Error::Format("unreadable".into()))
        }
        fn describe(&self) -> String {
            "unread".into()
        }
    }

    #[test]
    fn what_cannot_be_read_whole_is_kept() {
        let file = |path: &str, size| SourceFile {
            path: path.into(),
            size,
        };
        let mut tree = Unread(vec![
            file("fakelib/broken.sprx", 10),
            file("fakelib/huge.sprx", MAX_SCAN + 1),
        ]);
        let found = classify(&mut tree);
        assert!(found.libs.is_empty());
        assert!(found.emulators.iter().all(|e| e.name == "Other"));
        assert_eq!(found.emulators.len(), 2);
    }

    fn game(sdk: u32) -> Files {
        Files::new(vec![
            (
                "eboot.bin",
                test_elf(PT_SCE_PROCPARAM, PROCESS_PARAM_MAGIC, sdk, false),
            ),
            (
                "sce_module/libgame.prx",
                test_elf(PT_SCE_MODULE_PARAM, MODULE_PARAM_MAGIC, 0x0200_0009, false),
            ),
            // The fakelib's own (lowered) SDK never counts.
            (
                "fakelib/libSceAgc.sprx",
                test_elf(PT_SCE_MODULE_PARAM, MODULE_PARAM_MAGIC, 0x0100_0000, false),
            ),
        ])
    }

    #[test]
    fn a_lowered_sdk_blocks_removal() {
        let param = serde_json::json!({"sdkVersion": "0x1200000000000000"});
        assert_eq!(blocked(&mut game(0x1200_0038), Some(&param)), None);
        assert_eq!(blocked(&mut game(0x1250_0000), Some(&param)), None);
        let lowered = blocked(&mut game(0x0900_0040), Some(&param)).unwrap();
        assert!(
            lowered.contains("SDK 9.00 (0x09000040) is lower than param.json's sdkVersion 12.00")
        );
        // A lower build of the declared SDK counts too.
        let built = serde_json::json!({"sdkVersion": "0x0900004000000000"});
        assert!(blocked(&mut game(0x0900_0038), Some(&built)).is_some());
        assert_eq!(blocked(&mut game(0x0900_0040), Some(&built)), None);
        // A module left at 12.00 does not hide a lowered eboot.bin.
        let mut patched = Files::new(vec![
            (
                "eboot.bin",
                test_elf(PT_SCE_PROCPARAM, PROCESS_PARAM_MAGIC, 0x0500_0033, false),
            ),
            (
                "sce_module/libother.prx",
                test_elf(PT_SCE_MODULE_PARAM, MODULE_PARAM_MAGIC, 0x1200_0038, false),
            ),
        ]);
        assert!(blocked(&mut patched, Some(&param)).is_some());
        // 32-bit words are read too.
        let short = serde_json::json!({"sdkVersion": "0x09000000"});
        assert_eq!(blocked(&mut game(0x0900_0040), Some(&short)), None);
        // Unknown either way: refused.
        assert!(blocked(&mut game(0x1200_0038), None).is_some());
        let bad = serde_json::json!({"sdkVersion": "12.00"});
        assert!(blocked(&mut game(0x1200_0038), Some(&bad)).is_some());
        let empty = serde_json::json!({"sdkVersion": "0x"});
        assert!(blocked(&mut game(0x1200_0038), Some(&empty)).is_some());
        let mut no_exe = Files::new(vec![("eboot.bin", vec![0; 16])]);
        assert!(
            blocked(&mut no_exe, Some(&param))
                .unwrap()
                .contains("eboot.bin has no")
        );
    }

    struct Listed(Files, Vec<String>);

    impl SourceTree for Listed {
        fn files(&self) -> &[SourceFile] {
            self.0.files()
        }
        fn read(&mut self, path: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
            self.0.read(path)
        }
        fn empty_dirs(&self) -> &[String] {
            &self.1
        }
        fn describe(&self) -> String {
            "listed".into()
        }
    }

    #[test]
    fn without_drops_files_and_the_fakelib_they_empty() {
        let files = Files::new(vec![
            ("eboot.bin", vec![1]),
            ("fakelib/libSceAgc.sprx", vec![2]),
            ("fakelib/libSceAmpr.sprx", vec![3]),
            ("fakelib2/libSceGnm.sprx", vec![4]),
        ]);
        let dirs = ["fakelib/x", "fakelib2/y", "data/empty"]
            .map(String::from)
            .to_vec();
        let drop = ["fakelib/libSceAgc.sprx", "fakelib2/libSceGnm.sprx"].map(String::from);
        let mut tree = Without::new(Box::new(Listed(files, dirs)), &drop);
        let paths: Vec<&str> = tree.files().iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["eboot.bin", "fakelib/libSceAmpr.sprx"]);
        // fakelib still holds the emulator, so its folder stays; fakelib2 is gone whole.
        assert_eq!(tree.empty_dirs(), ["fakelib/x", "data/empty"]);
        assert_eq!(tree.read("fakelib/libSceAmpr.sprx").unwrap(), [3]);
    }
}
