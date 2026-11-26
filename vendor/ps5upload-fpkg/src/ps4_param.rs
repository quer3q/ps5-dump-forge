//! `sce_sys/param.json` for a PS4 game written as an image or a folder.
//!
//! ShadowMount+ reads `param.json`, and a PS4 game only has `param.sfo`: its image mounts and
//! installs, and then the console says the game couldn't be started (NP-103270-3, measured by
//! Alembic on a PS5 with ShadowMount+ 1.7beta8; not yet by us). The file is made from `param.sfo`
//! as SadykovIV/PS4pkg_to_ffpfsc makes it (GPL-3.0, `build_param_json`), by way of
//! fthdgn-gh/Alembic's `sfo::ps4_param_json` (GPL-3.0-or-later). Never for a package.

use std::collections::BTreeMap;

use ps5upload_pkg::{sfo_params, SfoValue};

use crate::source::{SourceFile, SourceTree};
use crate::Result;

/// `TITLE_00`..`TITLE_29` in the console's language order, as `appmeta` spells them.
const TITLE_LOCALES: [&str; 30] = [
    "ja-JP", "en-US", "fr-FR", "es-ES", "de-DE", "it-IT", "nl-NL", "pt-PT", "ru-RU", "ko-KR",
    "zh-Hant", "zh-Hans", "fi-FI", "sv-SE", "da-DK", "no-NO", "pl-PL", "pt-BR", "en-GB", "tr-TR",
    "es-419", "ar-AE", "fr-CA", "cs-CZ", "hu-HU", "el-GR", "ro-RO", "th-TH", "vi-VN", "id-ID",
];

fn quoted(s: &str) -> String {
    serde_json::Value::from(s).to_string()
}

/// One level of a sorted JSON object, at `indent` spaces.
fn object(entries: &BTreeMap<String, String>, indent: usize) -> String {
    let pad = " ".repeat(indent);
    let body: Vec<String> = entries
        .iter()
        .map(|(k, v)| format!("{pad}{}: {v}", quoted(k)))
        .collect();
    format!("{{\n{}\n{}}}", body.join(",\n"), " ".repeat(indent - 2))
}

/// The `param.json` of a PS4 game, from its `param.sfo` values.
pub fn param_json(params: &[(String, SfoValue)], title_id: &str) -> String {
    let text = |key: &str| -> String {
        params
            .iter()
            .find_map(|(k, v)| match v {
                SfoValue::Text(t) if k == key => Some(t.trim().to_string()),
                _ => None,
            })
            .unwrap_or_default()
    };
    let numbered = |i: usize| text(&format!("TITLE_{i:02}"));

    // TITLE, else TITLE_01, else the first other TITLE_NN, else the IDs.
    let title = [text("TITLE"), text("TITLE_01")]
        .into_iter()
        .chain((0..30).filter(|i| *i != 1).map(numbered))
        .chain([text("CONTENT_ID"), text("TITLE_ID")])
        .find(|t| !t.is_empty())
        .unwrap_or_else(|| "Unknown Game".to_string());

    let mut localized = BTreeMap::new();
    for (i, locale) in TITLE_LOCALES.iter().enumerate() {
        let name = numbered(i);
        if !name.is_empty() {
            localized.insert((*locale).to_string(), name);
        }
    }
    let english = numbered(1);
    localized.insert(
        "en-US".to_string(),
        if english.is_empty() {
            title.clone()
        } else {
            english
        },
    );
    let mut parameters: BTreeMap<String, String> = localized
        .into_iter()
        .map(|(locale, name)| {
            let entry = BTreeMap::from([("titleName".to_string(), quoted(&name))]);
            (locale, object(&entry, 6))
        })
        .collect();
    parameters.insert("defaultLanguage".to_string(), quoted("en-US"));

    let mut top = BTreeMap::from([
        ("localizedParameters".to_string(), object(&parameters, 4)),
        ("titleId".to_string(), quoted(title_id)),
        ("titleName".to_string(), quoted(&title)),
    ]);
    for i in 1..=4 {
        let key = format!("USER_DEFINED_PARAM_{i}");
        let value = params.iter().find_map(|(k, v)| match v {
            SfoValue::Int(n) if *k == key && *n != 0 => Some(*n),
            _ => None,
        });
        if let Some(n) = value {
            top.insert(format!("userDefinedParam{i}"), n.to_string());
        }
    }
    format!("{}\n", object(&top, 2))
}

/// Four capitals and five digits.
fn is_title_id(id: &str) -> bool {
    let b = id.as_bytes();
    b.len() == 9
        && b[..4].iter().all(u8::is_ascii_uppercase)
        && b[4..].iter().all(u8::is_ascii_digit)
}

/// A PS4 game with the `sce_sys/param.json` it lacks.
pub struct Ps4Params<'a> {
    inner: &'a mut dyn SourceTree,
    files: Vec<SourceFile>,
    path: String,
    json: Vec<u8>,
}

impl<'a> Ps4Params<'a> {
    /// `inner` with the file added, or `None` when it needs none: it has a `param.json`, or no
    /// `param.sfo` with a title ID, or that title ID is a PS5 game's. An unreadable `param.sfo`
    /// is also `None`: the build goes on as it did, and the file was never there to lose.
    pub fn wrap(inner: &'a mut dyn SourceTree) -> Result<Option<Self>> {
        let is = |f: &SourceFile, name: &str| f.path.eq_ignore_ascii_case(name);
        if inner.files().iter().any(|f| is(f, "sce_sys/param.json")) {
            return Ok(None);
        }
        let Some(sfo) = inner
            .files()
            .iter()
            .find(|f| is(f, "sce_sys/param.sfo"))
            .map(|f| f.path.clone())
        else {
            return Ok(None);
        };
        let bytes = crate::source::read_small(inner, &sfo, crate::source::MAX_PARAM_BYTES)?;
        let Ok(params) = sfo_params(&bytes) else {
            return Ok(None);
        };
        let title_id = params.iter().find_map(|(k, v)| match v {
            SfoValue::Text(t) if k == "TITLE_ID" => Some(t.trim().to_string()),
            _ => None,
        });
        let Some(title_id) = title_id.filter(|id| is_title_id(id) && !id.starts_with("PPSA"))
        else {
            return Ok(None);
        };
        let json = param_json(&params, &title_id).into_bytes();
        // Beside param.sfo, in its folder's own spelling: UFS tells the cases apart.
        let path = format!("{}param.json", &sfo[..sfo.len() - "param.sfo".len()]);
        let mut files = inner.files().to_vec();
        files.push(SourceFile {
            path: path.clone(),
            size: json.len() as u64,
        });
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(Some(Self {
            inner,
            files,
            path,
            json,
        }))
    }
}

impl SourceTree for Ps4Params<'_> {
    fn release(&mut self) {
        self.inner.release();
    }

    fn files(&self) -> &[SourceFile] {
        &self.files
    }
    fn empty_dirs(&self) -> &[String] {
        self.inner.empty_dirs()
    }
    fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        if path == self.path {
            return Ok(self.json.clone());
        }
        self.inner.read(path)
    }
    fn read_range(&mut self, path: &str, offset: u64, len: usize) -> Result<Vec<u8>> {
        if path == self.path {
            let start = usize::try_from(offset)
                .unwrap_or(usize::MAX)
                .min(self.json.len());
            let end = start.saturating_add(len).min(self.json.len());
            return Ok(self.json[start..end].to_vec());
        }
        self.inner.read_range(path, offset, len)
    }
    fn describe(&self) -> String {
        format!("{} (PS4 param.json added)", self.inner.describe())
    }
}
