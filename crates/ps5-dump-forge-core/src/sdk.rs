//! The firmware a game's executables ask for: the PS5 SDK version in each one's process or
//! module param (`PT_SCE_PROCPARAM`, `PT_SCE_MODULE_PARAM`), which the console checks
//! against its own. A backport lowers it in every executable it patches, so the highest
//! one left is the lowest firmware the game runs on. Layout as backport tools patch it
//! (`ps5_backport.c`: magic first, or after an 8-byte size; the PS5 SDK at +0xC).

use ps5upload_fpkg::source::SourceTree;

#[cfg(test)]
const PT_LOAD: u32 = 1;
pub(crate) const PT_SCE_PROCPARAM: u32 = 0x6100_0001;
pub(crate) const PT_SCE_MODULE_PARAM: u32 = 0x6100_0002;
pub(crate) const PROCESS_PARAM_MAGIC: u32 = 0x4942_524F;
pub(crate) const MODULE_PARAM_MAGIC: u32 = 0x3C13_F4BF;
/// The PS5 SDK word, from the param's magic.
const PS5_SDK_AT: usize = 0xC;
const ELF: [u8; 4] = [0x7F, b'E', b'L', b'F'];
/// Fake-signed SELFs, PS5 and PS4: the ELF header and program headers follow the segment
/// table, and each segment's data sits where its table entry says.
const SELF_PS5: [u8; 4] = [0x54, 0x14, 0xF5, 0xEE];
const SELF_PS4: [u8; 4] = [0x4F, 0x15, 0x3D, 0x1D];
/// Headers are read from the first 64 KiB; real ones take a few hundred bytes.
const HEAD: usize = 0x1_0000;
/// Executables by extension, as backport tools pick them.
const EXTENSIONS: &[&str] = &["bin", "elf", "self", "prx", "sprx"];
/// ponytail: the first this many files with an ELF or SELF magic are parsed (a game has a
/// few dozen); data `.bin` files are passed over by a 4-byte read and do not count.
const MAX_EXECUTABLES: usize = 4096;
/// SELF entry properties: its data is encrypted or compressed, so not readable in place.
const SELF_ENCRYPTED: u64 = 1 << 1;
const SELF_COMPRESSED: u64 = 1 << 3;

/// The lowest firmware the game's executables allow, as `7.00`: their highest PS5 SDK.
/// The backport's own `fakelib`/`fakelib2` libraries are left out: they come from the
/// newer firmware they stand in for. `None` when no executable carries a readable param.
pub(crate) fn lowest_firmware(tree: &mut dyn SourceTree) -> Option<String> {
    firmware(highest_sdk(tree)?)
}

/// The highest valid PS5 SDK word across the executables, `fakelib`/`fakelib2` left out.
pub(crate) fn highest_sdk(tree: &mut dyn SourceTree) -> Option<u32> {
    let paths: Vec<String> = tree
        .files()
        .iter()
        .filter(|f| f.size >= 4 && !crate::inspect::is_fakelib(&f.path) && is_executable(&f.path))
        .map(|f| f.path.clone())
        .collect();
    let mut sdks = Vec::new();
    let mut parsed = 0;
    for path in &paths {
        let Ok(magic) = tree.read_range(path, 0, 4) else {
            continue;
        };
        if ![ELF, SELF_PS5, SELF_PS4].iter().any(|m| magic == m) {
            continue;
        }
        parsed += 1;
        if parsed > MAX_EXECUTABLES {
            break;
        }
        // Each word on its own: one malformed executable must not hide the others.
        sdks.extend(ps5_sdk(tree, path).filter(|&w| firmware(w).is_some()));
    }
    sdks.into_iter().max()
}

fn is_executable(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, ext)| EXTENSIONS.iter().any(|e| ext.eq_ignore_ascii_case(e)))
}

/// `0x07000038` as `7.00`, `0x10500040` as `10.50`: the top two bytes are BCD.
pub(crate) fn firmware(sdk: u32) -> Option<String> {
    let [major, minor, _, _] = sdk.to_be_bytes();
    let bcd = |b: u8| b >> 4 <= 9 && b & 0xF <= 9;
    (bcd(major) && bcd(minor) && sdk != 0).then(|| format!("{major:x}.{minor:02x}"))
}

/// `N` bytes at `at`, every bound checked (offsets come from the file).
fn le<const N: usize>(bytes: &[u8], at: usize) -> Option<[u8; N]> {
    bytes.get(at..at.checked_add(N)?)?.try_into().ok()
}

/// The PS5 SDK word of one executable, `None` when it is not a readable ELF or fSELF with
/// a process or module param.
pub(crate) fn ps5_sdk(tree: &mut dyn SourceTree, path: &str) -> Option<u32> {
    let head = tree.read_range(path, 0, HEAD).ok()?;
    let u16_at = |at: usize| le::<2>(&head, at).map(u16::from_le_bytes);
    let u32_at = |at: usize| le::<4>(&head, at).map(u32::from_le_bytes);
    let u64_at = |at: usize| le::<8>(&head, at).map(u64::from_le_bytes);
    let magic: [u8; 4] = le(&head, 0)?;
    let (elf, segments) = if magic == SELF_PS5 || magic == SELF_PS4 {
        let n = usize::from(u16_at(0x18)?);
        (0x20 + n * 0x20, n)
    } else if magic == ELF {
        (0, 0)
    } else {
        return None;
    };
    if le::<4>(&head, elf)? != ELF {
        return None;
    }
    let phoff = usize::try_from(u64_at(elf + 0x20)?).ok()?;
    let phentsize = usize::from(u16_at(elf + 0x36)?);
    let phnum = usize::from(u16_at(elf + 0x38)?);
    // (type, file offset, file size) per program header.
    let headers: Vec<(u32, u64, u64)> = (0..phnum)
        .map(|i| {
            let ph = elf
                .checked_add(phoff)?
                .checked_add(i.checked_mul(phentsize)?)?;
            Some((
                u32_at(ph)?,
                u64_at(ph.checked_add(8)?)?,
                u64_at(ph.checked_add(0x20)?)?,
            ))
        })
        .collect::<Option<_>>()?;
    let (kind, param, _) = *headers
        .iter()
        .find(|h| h.0 == PT_SCE_PROCPARAM || h.0 == PT_SCE_MODULE_PARAM)?;
    let at = if segments == 0 {
        param
    } else {
        // In a fSELF each ELF segment's data sits where its table entry `{props, offset,
        // stored size, memory size}` says (segment `props >> 20`), when stored plainly. The
        // param is read through any segment that holds it: its own, a PT_LOAD, a RELRO.
        let entry = |seg: usize, size: u64| {
            (0..segments).find_map(|k| {
                let at = 0x20 + k * 0x20;
                let props = u64_at(at)?;
                let plain = props & (SELF_ENCRYPTED | SELF_COMPRESSED) == 0;
                let ours = (props >> 20) & 0xFFFF == seg as u64 && u64_at(at + 0x10)? == size;
                (plain && ours).then(|| u64_at(at + 8))?
            })
        };
        headers
            .iter()
            .enumerate()
            .find_map(|(seg, &(_, off, size))| {
                let inside = off <= param && param < off.saturating_add(size);
                inside.then(|| entry(seg, size)?.checked_add(param - off))?
            })?
    };
    let bytes = tree.read_range(path, at, 0x20).ok()?;
    let word = |at: usize| le::<4>(&bytes, at).map(u32::from_le_bytes);
    let want = if kind == PT_SCE_PROCPARAM {
        PROCESS_PARAM_MAGIC
    } else {
        MODULE_PARAM_MAGIC
    };
    let base = [0, 8].into_iter().find(|&b| word(b) == Some(want))?;
    word(base + PS5_SDK_AT)
}

#[cfg(test)]
/// A minimal ELF: one PT_LOAD, and a param of `kind` inside it at 0x200 (with the
/// 8-byte size first when `sized`).
pub(crate) fn test_elf(kind: u32, magic: u32, sdk: u32, sized: bool) -> Vec<u8> {
    let mut f = vec![0u8; 0x300];
    f[..4].copy_from_slice(&ELF);
    f[0x20..0x28].copy_from_slice(&0x40u64.to_le_bytes()); // e_phoff
    f[0x36..0x38].copy_from_slice(&0x38u16.to_le_bytes()); // e_phentsize
    f[0x38..0x3A].copy_from_slice(&2u16.to_le_bytes()); // e_phnum
    let ph = |f: &mut Vec<u8>, i: usize, t: u32, off: u64, size: u64| {
        let at = 0x40 + i * 0x38;
        f[at..at + 4].copy_from_slice(&t.to_le_bytes());
        f[at + 8..at + 16].copy_from_slice(&off.to_le_bytes());
        f[at + 0x20..at + 0x28].copy_from_slice(&size.to_le_bytes());
    };
    ph(&mut f, 0, PT_LOAD, 0x100, 0x200);
    ph(&mut f, 1, kind, 0x200, 0x40);
    let p = if sized { 0x208 } else { 0x200 };
    f[p..p + 4].copy_from_slice(&magic.to_le_bytes());
    f[p + 0xC..p + 0x10].copy_from_slice(&sdk.to_le_bytes());
    f
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// `elf` wrapped as a fSELF: a one-entry segment table placing the PT_LOAD at 0x400.
    fn fself(elf_bytes: &[u8]) -> Vec<u8> {
        let table = 0x20 + 0x20;
        let mut f = vec![0u8; 0x400 + 0x200];
        f[..4].copy_from_slice(&SELF_PS5);
        f[0x18..0x1A].copy_from_slice(&1u16.to_le_bytes());
        f[0x20..0x28].copy_from_slice(&(0u64 << 20).to_le_bytes()); // ELF segment 0
        f[0x28..0x30].copy_from_slice(&0x400u64.to_le_bytes());
        f[0x30..0x38].copy_from_slice(&0x200u64.to_le_bytes());
        f[table..table + 0x100].copy_from_slice(&elf_bytes[..0x100]);
        f[0x400..0x600].copy_from_slice(&elf_bytes[0x100..0x300]);
        f
    }

    pub(crate) struct Files(
        Vec<(String, Vec<u8>)>,
        Vec<ps5upload_fpkg::source::SourceFile>,
    );

    impl Files {
        pub(crate) fn new(list: Vec<(&str, Vec<u8>)>) -> Self {
            let meta = list
                .iter()
                .map(|(p, b)| ps5upload_fpkg::source::SourceFile {
                    path: p.to_string(),
                    size: b.len() as u64,
                })
                .collect();
            Self(
                list.into_iter().map(|(p, b)| (p.to_string(), b)).collect(),
                meta,
            )
        }
    }

    impl SourceTree for Files {
        fn files(&self) -> &[ps5upload_fpkg::source::SourceFile] {
            &self.1
        }
        fn read(&mut self, path: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
            Ok(self.0.iter().find(|f| f.0 == path).unwrap().1.clone())
        }
        fn describe(&self) -> String {
            "test".into()
        }
    }

    #[test]
    fn firmware_words() {
        assert_eq!(firmware(0x0700_0038).as_deref(), Some("7.00"));
        assert_eq!(firmware(0x1050_0040).as_deref(), Some("10.50"));
        assert_eq!(firmware(0x0450_0031).as_deref(), Some("4.50"));
        assert_eq!(firmware(0x0A00_0000), None);
        assert_eq!(firmware(0), None);
    }

    #[test]
    fn highest_sdk_left_is_the_lowest_firmware() {
        let mut tree = Files::new(vec![
            (
                "eboot.bin",
                test_elf(PT_SCE_PROCPARAM, PROCESS_PARAM_MAGIC, 0x0450_0031, false),
            ),
            // A module the backport left higher than the eboot wins: it sets the floor.
            (
                "sce_module/libgame.prx",
                fself(&test_elf(
                    PT_SCE_MODULE_PARAM,
                    MODULE_PARAM_MAGIC,
                    0x0500_0033,
                    true,
                )),
            ),
            // The backport's own libraries come from newer firmware and don't count.
            (
                "fakelib/libSceAgc.sprx",
                test_elf(PT_SCE_MODULE_PARAM, MODULE_PARAM_MAGIC, 0x1000_0040, false),
            ),
            // Wrong magic, not an executable, data: ignored.
            (
                "bad.prx",
                test_elf(PT_SCE_MODULE_PARAM, 0x1234, 0x2000_0000, false),
            ),
            ("sce_sys/param.json", b"{}".to_vec()),
            // A malformed SDK word is dropped, not the valid ones.
            (
                "sce_module/odd.prx",
                test_elf(PT_SCE_MODULE_PARAM, MODULE_PARAM_MAGIC, 0xFFFF_FFFF, false),
            ),
            // A program header table at the end of the address space: refused, no panic.
            ("hostile.elf", {
                let mut f = test_elf(PT_SCE_PROCPARAM, PROCESS_PARAM_MAGIC, 0x0900_0000, false);
                f[0x20..0x28].copy_from_slice(&u64::MAX.to_le_bytes());
                f
            }),
            ("data/x.bin", vec![1, 2, 3]),
        ]);
        assert_eq!(lowest_firmware(&mut tree).as_deref(), Some("5.00"));
        let mut none = Files::new(vec![("eboot.bin", vec![0; 16])]);
        assert_eq!(lowest_firmware(&mut none), None);
    }
}
