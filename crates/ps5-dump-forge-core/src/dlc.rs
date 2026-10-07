//! DLC embedded in a game dump. There is no one layout for it, so three traces are read:
//! the DLC emulator's list (`dlc_emu.ini` at the root, one `[PSAC]` section per content id,
//! answered by its `fakelib/libSceAppContent.sprx`),
//! a DLC's own metadata (`sce_sys/param.json` or `sce_sys/param.sfo`) anywhere but the
//! game's own `param.json` (a DLC copied into the game root leaves its `param.sfo` there),
//! and folders named by a DLC's content id (`EP1003-PPSA09017_00-FALLOUT4DLC00003`, `-ac`
//! as the console mounts them). Both are merged by content id. Metadata counts as a DLC's
//! when a `param.sfo` says so (CATEGORY `ac`, additional content), else only when its
//! content id differs from a known game id: a game's own generated `param.sfo` (CATEGORY
//! `gd`, `gp`, ...) never does.

use std::collections::BTreeMap;

use ps5upload_fpkg::source::SourceTree;
use serde::Serialize;

use crate::preflight::MAX_PARAM_JSON;

/// One DLC found in a dump.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Dlc {
    /// `EP1003-PPSA09017_00-FALLOUT4DLC00003`.
    pub content_id: String,
    /// Its last 16 characters, the label that tells a game's DLCs apart.
    pub label: String,
    /// From its metadata, when it has some.
    pub name: Option<String>,
    /// The folder it sits in, when it has its own (else its files are merged in the game's).
    pub folder: Option<String>,
    /// Bytes under `folder`.
    pub bytes: u64,
    /// Listed in the DLC emulator's `dlc_emu.ini`: its `download_status` there
    /// (`NO_EXTRA_DATA`: an entitlement only, no data of its own).
    pub emulated: Option<String>,
}

/// The DLC emulator's list, at the game root.
const DLC_EMU: &str = "dlc_emu.ini";
/// A list of a few hundred DLC is a few KiB; anything bigger is not one.
const MAX_DLC_EMU: u64 = 1024 * 1024;

/// Every DLC in `tree`, by content id. `game` is the game's own content id.
pub(crate) fn find(tree: &mut dyn SourceTree, game: Option<&str>) -> Vec<Dlc> {
    let mut found: BTreeMap<String, Dlc> = BTreeMap::new();
    let mut add =
        |id: String, name: Option<String>, folder: Option<String>, emulated: Option<String>| {
            if game == Some(id.as_str()) {
                return;
            }
            let d = found.entry(id.clone()).or_insert_with(|| Dlc {
                label: id[id.len() - 16..].to_string(),
                content_id: id,
                name: None,
                folder: None,
                bytes: 0,
                emulated: None,
            });
            d.name = d.name.take().or(name);
            d.folder = d.folder.take().or(folder);
            d.emulated = d.emulated.take().or(emulated);
        };

    let files = tree.files().to_vec();
    // What the DLC emulator unlocks.
    if let Some(f) = files.iter().find(|f| f.path.eq_ignore_ascii_case(DLC_EMU))
        && f.size <= MAX_DLC_EMU
        && let Ok(bytes) = tree.read(&f.path)
    {
        for (id, status) in emulated(&String::from_utf8_lossy(&bytes)) {
            add(id, None, None, Some(status));
        }
    }
    // Folders named by a content id, at any depth.
    for f in &files {
        let parts: Vec<&str> = f.path.split('/').collect();
        for i in 0..parts.len() - 1 {
            if let Some(id) = content_id_of(parts[i]) {
                add(id, None, Some(parts[..=i].join("/")), None);
            }
        }
    }
    // Metadata other than the game's own.
    for f in &files {
        let lower = f.path.to_ascii_lowercase();
        let json = lower.ends_with("sce_sys/param.json");
        if !(json || lower.ends_with("sce_sys/param.sfo")) || f.path == "sce_sys/param.json" {
            continue;
        }
        if f.size > MAX_PARAM_JSON {
            continue;
        }
        let Ok(bytes) = tree.read(&f.path) else {
            continue;
        };
        let meta = if json {
            from_json(&bytes).map(|(id, name)| (id, name, None))
        } else {
            from_sfo(&bytes)
        };
        let Some((id, name, category)) = meta else {
            continue;
        };
        let is_dlc = match category.as_deref() {
            Some(c) => c.starts_with("ac"),
            None => game.is_some_and(|g| g != id),
        };
        if is_dlc {
            // `<folder>/sce_sys/param.*`; none when it is the game root's.
            let folder = f.path.rsplitn(3, '/').nth(2).map(str::to_string);
            add(id, name, folder, None);
        }
    }

    let mut dlcs: Vec<Dlc> = found.into_values().collect();
    for d in &mut dlcs {
        if let Some(folder) = &d.folder {
            let under = format!("{folder}/");
            d.bytes = files
                .iter()
                .filter(|f| f.path.starts_with(&under))
                .fold(0u64, |sum, f| sum.saturating_add(f.size));
        }
    }
    dlcs
}

/// (content id, download status) of every `[PSAC]` section of a `dlc_emu.ini`:
///
/// ```text
/// [PSAC]
/// content_id=EP9000-PPSA13197_00-STELLARBLADEDLC1
/// download_status=NO_EXTRA_DATA
/// ```
fn emulated(text: &str) -> Vec<(String, String)> {
    text.split('[')
        .skip(1)
        .filter_map(|section| {
            let (head, body) = section.split_once(']')?;
            if !head.trim().eq_ignore_ascii_case("PSAC") {
                return None;
            }
            let (mut id, mut status) = (None, String::new());
            for (key, value) in body.lines().filter_map(|l| l.split_once('=')) {
                match key.trim().to_ascii_lowercase().as_str() {
                    "content_id" => id = content_id_of(value.trim()),
                    "download_status" => status = value.trim().to_string(),
                    _ => {}
                }
            }
            Some((id?, status))
        })
        .collect()
}

/// `name` as a content id (`XX0000-XXXX00000_00-` and a 16-character label), `-ac` as the
/// console mounts DLC allowed.
fn content_id_of(name: &str) -> Option<String> {
    let id = name.strip_suffix("-ac").unwrap_or(name);
    let b = id.as_bytes();
    let upper_digit = |c: &u8| c.is_ascii_uppercase() || c.is_ascii_digit();
    let ok = b.len() == 36
        && b[..2].iter().all(u8::is_ascii_uppercase)
        && b[2..6].iter().all(u8::is_ascii_digit)
        && b[6] == b'-'
        && b[7..11].iter().all(u8::is_ascii_uppercase)
        && b[11..16].iter().all(u8::is_ascii_digit)
        && b[16] == b'_'
        && b[17..19].iter().all(u8::is_ascii_digit)
        && b[19] == b'-'
        && b[20..].iter().all(upper_digit);
    ok.then(|| id.to_string())
}

/// (content id, title) from a `param.json`.
fn from_json(bytes: &[u8]) -> Option<(String, Option<String>)> {
    let info = crate::preflight::parse_param(bytes);
    let id = info.content_id.as_deref().and_then(content_id_of)?;
    Some((
        id,
        info.param_json
            .as_ref()
            .and_then(crate::inspect::title_name),
    ))
}

/// (content id, title, category) from a `param.sfo` (PSF: header, key table, data table).
fn from_sfo(bytes: &[u8]) -> Option<(String, Option<String>, Option<String>)> {
    let u16_at = |at: usize| Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?));
    let u32_at = |at: usize| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    if bytes.get(..4)? != b"\0PSF" {
        return None;
    }
    let keys = u32_at(0x8)? as usize;
    let data = u32_at(0xC)? as usize;
    let count = u32_at(0x10)? as usize;
    let mut id = None;
    let mut title = None;
    let mut category = None;
    for i in 0..count.min(1024) {
        let e = 0x14 + i * 0x10;
        let key_at = keys.checked_add(usize::from(u16_at(e)?))?;
        let key_end = key_at + bytes.get(key_at..)?.iter().position(|&b| b == 0)?;
        let key = bytes.get(key_at..key_end)?;
        let len = u32_at(e + 4)? as usize;
        let at = data.checked_add(u32_at(e + 0xC)? as usize)?;
        let value = bytes.get(at..at.checked_add(len)?)?;
        let text = || {
            let end = value.iter().position(|&b| b == 0).unwrap_or(value.len());
            String::from_utf8(value[..end].to_vec()).ok()
        };
        match key {
            b"CONTENT_ID" => id = text(),
            b"TITLE" => title = text(),
            b"CATEGORY" => category = text(),
            _ => {}
        }
    }
    Some((
        content_id_of(&id?)?,
        title.filter(|t| !t.is_empty()),
        category.filter(|c| !c.is_empty()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ps5upload_fpkg::source::SourceFile;

    struct Files(Vec<(String, Vec<u8>)>, Vec<SourceFile>);

    impl Files {
        fn new(list: Vec<(&str, Vec<u8>)>) -> Self {
            let meta = list
                .iter()
                .map(|(p, b)| SourceFile {
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
        fn files(&self) -> &[SourceFile] {
            &self.1
        }
        fn read(&mut self, path: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
            Ok(self.0.iter().find(|f| f.0 == path).unwrap().1.clone())
        }
        fn describe(&self) -> String {
            "test".into()
        }
    }

    /// A `param.sfo` with these UTF-8 entries.
    fn sfo(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut keys = Vec::new();
        let mut data = Vec::new();
        let mut table = Vec::new();
        for (k, v) in entries {
            let (ko, dof) = (keys.len() as u16, data.len() as u32);
            keys.extend_from_slice(k.as_bytes());
            keys.push(0);
            data.extend_from_slice(v.as_bytes());
            data.push(0);
            let len = v.len() as u32 + 1;
            table.extend_from_slice(&ko.to_le_bytes());
            table.extend_from_slice(&0x0204u16.to_le_bytes());
            table.extend_from_slice(&len.to_le_bytes());
            table.extend_from_slice(&len.to_le_bytes());
            table.extend_from_slice(&dof.to_le_bytes());
        }
        let key_at = 0x14 + table.len() as u32;
        let data_at = key_at + keys.len() as u32;
        let mut f = b"\0PSF".to_vec();
        f.extend_from_slice(&0x0101u32.to_le_bytes());
        f.extend_from_slice(&key_at.to_le_bytes());
        f.extend_from_slice(&data_at.to_le_bytes());
        f.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        f.extend(table);
        f.extend(keys);
        f.extend(data);
        f
    }

    const GAME: &str = "EP1003-PPSA09017_00-FALLOUT4GAME0000";

    #[test]
    fn dlc_emulator_list() {
        let ini = "; unlocked\r\n[PSAC]\r\ncontent_id=EP9000-PPSA13197_00-STELLARBLADEDLC1\r\n\
                   download_status=NO_EXTRA_DATA\r\n\r\n[PSAC]\ncontent_id = EP9000-PPSA13197_00-NIKKEREWARD00000\n\
                   [OTHER]\ncontent_id=EP9000-PPSA13197_00-NOTADLC000000000\n[PSAC]\ncontent_id=garbage\n";
        let mut tree = Files::new(vec![
            ("eboot.bin", b"x".to_vec()),
            ("dlc_emu.ini", ini.as_bytes().to_vec()),
        ]);
        let found: Vec<(String, Option<String>)> = find(&mut tree, Some(GAME))
            .into_iter()
            .map(|d| (d.label, d.emulated))
            .collect();
        assert_eq!(
            found,
            [
                ("NIKKEREWARD00000".to_string(), Some(String::new())),
                (
                    "STELLARBLADEDLC1".to_string(),
                    Some("NO_EXTRA_DATA".to_string())
                ),
            ]
        );
    }

    #[test]
    fn content_ids() {
        assert_eq!(
            content_id_of("EP1003-PPSA09017_00-FALLOUT4DLC00003-ac").as_deref(),
            Some("EP1003-PPSA09017_00-FALLOUT4DLC00003")
        );
        for bad in [
            "PPSA09017",
            "ep1003-PPSA09017_00-FALLOUT4DLC00003",
            "EP1003-PPSA09017_00-short",
        ] {
            assert_eq!(content_id_of(bad), None, "{bad}");
        }
    }

    #[test]
    fn both_layouts_are_found_and_merged() {
        let dlc_json = br#"{"contentId":"EP1003-PPSA09017_00-FALLOUT4DLC00003",
            "localizedParameters":{"defaultLanguage":"en-US","en-US":{"titleName":"Far Harbor"}}}"#;
        let mut tree = Files::new(vec![
            ("eboot.bin", b"x".to_vec()),
            (
                "sce_sys/param.json",
                format!(r#"{{"contentId":"{GAME}"}}"#).into_bytes(),
            ),
            // The game's own generated param.sfo, even with a stale content id: not a DLC.
            (
                "sce_sys/param.sfo",
                sfo(&[
                    ("CONTENT_ID", "EP1003-PPSA09017_00-OLDGAMEID0000000"),
                    ("CATEGORY", "gd"),
                    ("TITLE", "Fallout 4"),
                ]),
            ),
            // A DLC in its own content-id folder, with its metadata.
            (
                "EP1003-PPSA09017_00-FALLOUT4DLC00003-ac/sce_sys/param.json",
                dlc_json.to_vec(),
            ),
            (
                "EP1003-PPSA09017_00-FALLOUT4DLC00003-ac/data/a.bin",
                vec![0; 100],
            ),
            // A content-id folder with no metadata.
            ("EP1003-PPSA09017_00-FALLOUT4DLC00004/b.bin", vec![0; 7]),
            // A DLC merged into a sub-folder, only its param.sfo left to tell.
            (
                "dlc/sce_sys/param.sfo",
                sfo(&[
                    ("CONTENT_ID", "EP1003-PPSA09017_00-NUKAWORLD0000005"),
                    ("CATEGORY", "ac"),
                    ("TITLE", "Nuka-World"),
                ]),
            ),
        ]);
        let nuka_len = tree.1[6].size;
        let found = find(&mut tree, Some(GAME));
        let ids: Vec<(&str, Option<&str>, Option<&str>, u64)> = found
            .iter()
            .map(|d| {
                (
                    d.label.as_str(),
                    d.name.as_deref(),
                    d.folder.as_deref(),
                    d.bytes,
                )
            })
            .collect();
        assert_eq!(
            ids,
            [
                (
                    "FALLOUT4DLC00003",
                    Some("Far Harbor"),
                    Some("EP1003-PPSA09017_00-FALLOUT4DLC00003-ac"),
                    100 + dlc_json.len() as u64
                ),
                (
                    "FALLOUT4DLC00004",
                    None,
                    Some("EP1003-PPSA09017_00-FALLOUT4DLC00004"),
                    7
                ),
                (
                    "NUKAWORLD0000005",
                    Some("Nuka-World"),
                    Some("dlc"),
                    nuka_len
                ),
            ]
        );
        let mut plain = Files::new(vec![("eboot.bin", b"x".to_vec())]);
        assert!(find(&mut plain, Some(GAME)).is_empty());
        // No game id to compare with: an uncategorised param.sfo can't be told from the game's.
        let mut unknown = Files::new(vec![(
            "sce_sys/param.sfo",
            sfo(&[("CONTENT_ID", "EP1003-PPSA09017_00-FALLOUT4GAME0000")]),
        )]);
        assert!(find(&mut unknown, None).is_empty());
    }
}
