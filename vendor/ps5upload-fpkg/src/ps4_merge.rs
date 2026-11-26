//! A PS4 game with an update merged in: one source tree, for an image or folder that starts at
//! the update's version.
//!
//! An installed PS4 update is a second image the console lays over the game's at launch, and
//! holds only the files that changed or were added (see [`crate::ps4_pkg`]). A game that runs
//! from a folder or an image has no such layer: ShadowMount+ mounts one image or folder per
//! title, and its own pages count "updates merged into the base package" as part of the game
//! (drakmor/ShadowMountPlus docs/api.md), while separate update packages are installed through
//! the console's installer like any package. So an update goes into a game's image or folder
//! by merging it over the game, as merged backups are made: the game's files, each one the
//! update carries replaced by its copy, the files it adds added, and the update's `param.sfo`
//! (its version, title and changes) with CATEGORY `gp` made `gd`, a game again.
//!
//! Checked first: the game is a PS4 game (CATEGORY `gd`), the update is an update (`gp`) for
//! the same title ID, and its `APP_VER` is newer than the game's.
//!
//! Left out: an update's `target-deltainfo.dat` and `origin-deltainfo.dat` (they describe the
//! update against the version under it, which the merged game no longer has), and a
//! `sce_sys/param.json` the game had and the update hasn't (an image or folder of ours carries
//! one made from the old `param.sfo`; the build makes it again from the merged one).

use std::collections::HashMap;

use ps5upload_pkg::kind::version_cmp;
use ps5upload_pkg::sfo_params;

use crate::ps4_pkg::{sfo_text, Ps4Kind};
use crate::source::{SourceFile, SourceTree};
use crate::{format_err, Result};

const PARAM_SFO: &str = "sce_sys/param.sfo";
const PARAM_JSON: &str = "sce_sys/param.json";
/// Files an update carries that describe it against the version under it.
const UPDATE_ONLY: [&str; 2] = [
    "sce_sys/target-deltainfo.dat",
    "sce_sys/origin-deltainfo.dat",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Game,
    Update,
}

/// What a merge is: for the screen and the log.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MergeInfo {
    pub title_id: String,
    pub title: Option<String>,
    /// The game's `APP_VER` and the update's.
    pub game_version: String,
    pub update_version: String,
    /// Files of the game the update replaces, and files it adds.
    pub replaced: usize,
    pub added: usize,
}

/// See the module docs.
pub struct Ps4Merged {
    game: Box<dyn SourceTree>,
    update: Box<dyn SourceTree>,
    files: Vec<SourceFile>,
    /// A listed path -> the side it is read from and its path there.
    from: HashMap<String, (Side, String)>,
    /// The merged `param.sfo` and its listed path.
    sfo: (String, Vec<u8>),
    empty_dirs: Vec<String>,
    info: MergeInfo,
}

fn find<'a>(files: &'a [SourceFile], path: &str) -> Option<&'a SourceFile> {
    files.iter().find(|f| f.path.eq_ignore_ascii_case(path))
}

type Params = Vec<(String, ps5upload_pkg::SfoValue)>;

/// A tree's `param.sfo`: its listed path, bytes and values, or why it has none.
fn params_of(tree: &mut dyn SourceTree, what: &str) -> Result<(String, Vec<u8>, Params)> {
    let Some(path) = find(tree.files(), PARAM_SFO).map(|f| f.path.clone()) else {
        return format_err(format!(
            "the {what} has no sce_sys/param.sfo, so it isn't a PS4 {what}"
        ));
    };
    let bytes = crate::source::read_small(tree, &path, crate::source::MAX_PARAM_BYTES)?;
    let params = sfo_params(&bytes).map_err(|e| {
        crate::Error::Format(format!("the {what}'s sce_sys/param.sfo can't be read: {e}"))
    })?;
    Ok((path, bytes, params))
}

/// `sfo` with its CATEGORY set to `category`, written in place: same layout, same length.
pub fn with_category(sfo: &[u8], category: &str) -> Result<Vec<u8>> {
    let bad = || crate::Error::Format("param.sfo has no CATEGORY that can be rewritten".into());
    let le16 = |at: usize| {
        sfo.get(at..at + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
    };
    let le32 = |at: usize| {
        sfo.get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    if sfo.get(..4) != Some(b"\0PSF".as_slice()) {
        return Err(bad());
    }
    let (keys, data, count) = (
        le32(8).ok_or_else(bad)?,
        le32(12).ok_or_else(bad)?,
        le32(16),
    );
    for i in 0..count.ok_or_else(bad)?.min(256) {
        let row = 0x14 + i * 16;
        let (Some(key_at), Some(fmt), Some(max), Some(at)) =
            (le16(row), le16(row + 2), le32(row + 8), le32(row + 12))
        else {
            return Err(bad());
        };
        let key_start = keys + key_at as usize;
        let key = sfo
            .get(key_start..)
            .and_then(|k| k.split(|b| *b == 0).next())
            .ok_or_else(bad)?;
        if key != b"CATEGORY" {
            continue;
        }
        let start = data + at;
        if fmt != 0x0204 || category.len() + 1 > max || start + max > sfo.len() {
            return Err(bad());
        }
        let mut out = sfo.to_vec();
        out[row + 4..row + 8].copy_from_slice(&(category.len() as u32 + 1).to_le_bytes());
        out[start..start + max].fill(0);
        out[start..start + category.len()].copy_from_slice(category.as_bytes());
        return Ok(out);
    }
    Err(bad())
}

impl Ps4Merged {
    /// `update` merged over `game`, after the checks in the module docs.
    pub fn new(mut game: Box<dyn SourceTree>, mut update: Box<dyn SourceTree>) -> Result<Self> {
        let (_, _, game_params) = params_of(&mut *game, "game")?;
        let (update_sfo_path, update_sfo, update_params) = params_of(&mut *update, "update")?;
        let category = |p: &[(String, ps5upload_pkg::SfoValue)]| {
            let c = sfo_text(p, "CATEGORY").unwrap_or_default();
            (Ps4Kind::of_category(&c), c)
        };
        match category(&game_params) {
            (Some(Ps4Kind::Game), _) => {}
            (Some(Ps4Kind::Patch), _) => {
                return format_err("the game chosen to merge into is itself an update")
            }
            (_, c) => {
                return format_err(format!(
                    "the game chosen to merge into isn't a PS4 game (its CATEGORY is {c:?})"
                ))
            }
        }
        if category(&update_params).0 != Some(Ps4Kind::Patch) {
            return format_err(format!(
                "the update chosen isn't a PS4 update (its CATEGORY is {:?}, not \"gp\")",
                category(&update_params).1
            ));
        }
        let title_of = |p: &[(String, ps5upload_pkg::SfoValue)]| {
            sfo_text(p, "TITLE_ID")
                .or_else(|| {
                    sfo_text(p, "CONTENT_ID")
                        .map(|c| ps5upload_pkg::kind::title_id_of_content_id(&c))
                })
                .unwrap_or_default()
                .to_ascii_uppercase()
        };
        let (game_id, update_id) = (title_of(&game_params), title_of(&update_params));
        if game_id.is_empty() || game_id != update_id {
            return format_err(format!(
                "the update is for {update_id}, and the game is {game_id}: they don't go together"
            ));
        }
        let game_version = sfo_text(&game_params, "APP_VER").unwrap_or_default();
        let update_version = sfo_text(&update_params, "APP_VER").unwrap_or_default();
        if version_cmp(&update_version, &game_version) != std::cmp::Ordering::Greater {
            return format_err(format!(
                "the update ({update_version}) isn't newer than the game ({game_version}): \
                 the game already has it, or a later one"
            ));
        }

        // The game's files, then the update's over them, by name in any letter case.
        let mut files: Vec<SourceFile> = Vec::new();
        let mut from = HashMap::new();
        let mut index: HashMap<String, usize> = HashMap::new();
        let update_has_json = find(update.files(), PARAM_JSON).is_some();
        for f in game.files() {
            if f.path.eq_ignore_ascii_case(PARAM_JSON) && !update_has_json {
                continue;
            }
            index.insert(f.path.to_ascii_lowercase(), files.len());
            from.insert(f.path.clone(), (Side::Game, f.path.clone()));
            files.push(f.clone());
        }
        let (mut replaced, mut added) = (0, 0);
        for f in update.files() {
            if UPDATE_ONLY.iter().any(|p| f.path.eq_ignore_ascii_case(p)) {
                continue;
            }
            match index.get(&f.path.to_ascii_lowercase()) {
                Some(&i) => {
                    from.remove(&files[i].path);
                    files[i] = f.clone();
                    replaced += 1;
                }
                None => {
                    index.insert(f.path.to_ascii_lowercase(), files.len());
                    files.push(f.clone());
                    added += 1;
                }
            }
            from.insert(f.path.clone(), (Side::Update, f.path.clone()));
        }
        let sfo = with_category(&update_sfo, "gd")?;
        if let Some(f) = files.iter_mut().find(|f| f.path == update_sfo_path) {
            f.size = sfo.len() as u64;
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let mut empty: Vec<String> = game.empty_dirs().to_vec();
        for d in update.empty_dirs() {
            if !empty.iter().any(|e| e.eq_ignore_ascii_case(d)) {
                empty.push(d.clone());
            }
        }
        let empty_dirs = crate::ps4_pkg::still_empty(&empty, &files);
        let info = MergeInfo {
            title_id: game_id,
            title: sfo_text(&update_params, "TITLE").or_else(|| sfo_text(&game_params, "TITLE")),
            game_version,
            update_version,
            replaced,
            added,
        };
        Ok(Self {
            game,
            update,
            files,
            from,
            sfo: (update_sfo_path, sfo),
            empty_dirs,
            info,
        })
    }

    pub fn info(&self) -> &MergeInfo {
        &self.info
    }

    fn side(&mut self, path: &str) -> Result<(&mut dyn SourceTree, String)> {
        match self.from.get(path) {
            Some((Side::Game, p)) => Ok((&mut *self.game, p.clone())),
            Some((Side::Update, p)) => Ok((&mut *self.update, p.clone())),
            None => format_err(format!("{path} is not in the merged game")),
        }
    }
}

impl SourceTree for Ps4Merged {
    fn release(&mut self) {
        self.game.release();
        self.update.release();
    }

    fn files(&self) -> &[SourceFile] {
        &self.files
    }
    fn empty_dirs(&self) -> &[String] {
        &self.empty_dirs
    }
    fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        if path == self.sfo.0 {
            return Ok(self.sfo.1.clone());
        }
        let (tree, at) = self.side(path)?;
        tree.read(&at)
    }
    fn read_range(&mut self, path: &str, offset: u64, len: usize) -> Result<Vec<u8>> {
        if path == self.sfo.0 {
            let b = &self.sfo.1;
            let start = usize::try_from(offset).unwrap_or(usize::MAX).min(b.len());
            let end = start.saturating_add(len).min(b.len());
            return Ok(b[start..end].to_vec());
        }
        let (tree, at) = self.side(path)?;
        tree.read_range(&at, offset, len)
    }
    fn describe(&self) -> String {
        format!(
            "{} with its update {} merged in ({} -> {}): {}",
            self.game.describe(),
            self.update.describe(),
            self.info.game_version,
            self.info.update_version,
            self.info.title_id
        )
    }
}

/// The game at `game` with the update at `update` merged in: each opened as
/// [`crate::source::open`] opens it (a package, image or folder, here or on a server).
pub fn open(game: &std::path::Path, update: &std::path::Path) -> Result<Ps4Merged> {
    Ps4Merged::new(crate::source::open(game)?, crate::source::open(update)?)
}
