//! What a package is (a game, its update, or add-on content) and the version helpers a game
//! collection needs. The rules are PS Game Library's (`pspkg.py`), verified there against every
//! package on a real library.

use serde::{Deserialize, Serialize};

/// The three things a package can be to its game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Base,
    Patch,
    Dlc,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Base => "base",
            Kind::Patch => "patch",
            Kind::Dlc => "dlc",
        }
    }
}

/// `content_type` values the container header carries, with their platform. PS4: 0x1A game
/// data (a game or its patch), 0x1B additional content with data, 0x1C additional content
/// without data (a licence that unlocks something already in the game), 0x1E a delta patch.
/// PS5: 0x20 a game, 0x21 additional content, 0x26 an application.
pub fn content_type_platform(content_type: u32) -> Option<&'static str> {
    match content_type {
        0x1A | 0x1B | 0x1C | 0x1E => Some("PS4"),
        0x20 | 0x21 | 0x26 => Some("PS5"),
        _ => None,
    }
}

const DLC_CONTENT_TYPES: [u32; 3] = [0x1B, 0x1C, 0x21];
/// A PS4 delta patch says it is a patch by its type alone.
const PATCH_CONTENT_TYPES: [u32; 1] = [0x1E];
/// 0x40000000 a later patch, 0x00100000 the first patch.
const PATCH_FLAG_MASK: u32 = 0x4010_0000;

/// The PARAM-style category a PS5 package's header says, for when its param.json (which has no
/// category) is all there is: `gp` for a patch, `ac` for additional content, else `gd`. A PS5
/// add-on read as `gd` was refused as a full game whose title runs from an image (#724).
pub fn category_from_header(content_type: u32, content_flags: u32) -> &'static str {
    if content_flags & PATCH_FLAG_MASK != 0 {
        "gp"
    } else if DLC_CONTENT_TYPES.contains(&content_type) {
        "ac"
    } else {
        "gd"
    }
}

/// What a PARAM category says, when it says one of the three.
fn kind_of_category(category: &str) -> Option<Kind> {
    match category.trim().to_ascii_lowercase().as_str() {
        "gd" => Some(Kind::Base),
        "gp" => Some(Kind::Patch),
        "ac" => Some(Kind::Dlc),
        _ => None,
    }
}

/// What the header says the package is, whether that is certain, and why.
///
/// The header (`content_type`, `content_flags`) decides. On PS4 the `param.sfo` CATEGORY is a
/// second witness: when it disagrees the header still wins, but the answer is marked unsure,
/// and the reason names both. Two exceptions, both because a DLC taken for the game makes the
/// game look stored twice (#638): a CATEGORY of `ac` beats a header that only says "game"
/// (no add-on type, no patch bit), and a header type that is not known defers to the CATEGORY.
pub fn classify(content_type: u32, content_flags: u32, category: &str) -> (Kind, bool, String) {
    let known = content_type_platform(content_type).is_some();
    let (kind, why) = if DLC_CONTENT_TYPES.contains(&content_type) {
        (
            Kind::Dlc,
            format!("content_type={content_type:#04x} is additional content"),
        )
    } else if PATCH_CONTENT_TYPES.contains(&content_type) {
        (
            Kind::Patch,
            format!("content_type={content_type:#04x} is a delta patch"),
        )
    } else if content_flags & PATCH_FLAG_MASK != 0 {
        (
            Kind::Patch,
            format!(
                "content_flags={content_flags:#010x} has the patch bit {:#010x}",
                content_flags & PATCH_FLAG_MASK
            ),
        )
    } else {
        (
            Kind::Base,
            format!(
                "content_type={content_type:#04x}, content_flags={content_flags:#010x} has no patch bit"
            ),
        )
    };
    let sfo_kind = kind_of_category(category);
    if !known {
        return match sfo_kind {
            Some(sfo) => (
                sfo,
                false,
                format!("content_type={content_type:#04x} unrecognized; param.sfo CATEGORY={category} says {}", sfo.as_str()),
            ),
            None => (
                kind,
                false,
                format!("content_type={content_type:#04x} unrecognized; {why}"),
            ),
        };
    }
    if let Some(sfo) = sfo_kind {
        if sfo != kind {
            let decided = if kind == Kind::Base && sfo == Kind::Dlc {
                Kind::Dlc
            } else {
                kind
            };
            return (
                decided,
                false,
                format!(
                    "header says {} ({why}) but param.sfo CATEGORY={category} says {}",
                    kind.as_str(),
                    sfo.as_str()
                ),
            );
        }
    }
    (kind, true, why)
}

/// What a game folder or image is, from its PARAM category alone (`gd`, `gp`, `ac`, PS5
/// `gde`/`gdc`…, PS3 `GD`/`HG`/`AC`/`AP`).
pub fn classify_category(category: &str) -> (Kind, bool, String) {
    let c = category.trim();
    let kind = match c.to_ascii_lowercase().as_str() {
        "gp" | "ap" => Some(Kind::Patch),
        "ac" => Some(Kind::Dlc),
        "gd" | "hg" | "gde" | "gdc" | "gda" | "gdb" | "gdd" => Some(Kind::Base),
        _ => None,
    };
    match kind {
        Some(k) => (k, true, format!("CATEGORY={c}")),
        None => (Kind::Base, false, format!("no usable CATEGORY (got {c:?})")),
    }
}

/// Region from a content ID's prefix: `EP0002-…` → Europe.
pub fn region_of_content_id(content_id: &str) -> String {
    let pre: String = content_id
        .split('-')
        .next()
        .unwrap_or("")
        .chars()
        .take(2)
        .collect::<String>()
        .to_ascii_uppercase();
    match pre.as_str() {
        "EP" => "Europe".into(),
        "UP" => "Americas".into(),
        "JP" => "Japan".into(),
        "HP" => "Asia".into(),
        _ => pre,
    }
}

/// Region from a PS3 title ID's prefix, for packages whose content ID says nothing.
pub fn region_of_ps3_title_id(title_id: &str) -> String {
    let pre = title_id.get(..4).unwrap_or("").to_ascii_uppercase();
    match pre.as_str() {
        "NPEB" | "BCES" => "Europe",
        "NPHB" | "BCAS" => "Asia",
        "NPJB" | "BCJS" => "Japan",
        "NPUB" | "BCUS" => "Americas",
        _ => "",
    }
    .into()
}

/// `UP9000-CUSA00900_00-BLOODBORNE000000` → `CUSA00900`.
pub fn title_id_of_content_id(content_id: &str) -> String {
    content_id
        .split('-')
        .nth(1)
        .and_then(|s| s.split('_').next())
        .unwrap_or("")
        .to_string()
}

/// `04.040.100` → `4.40.100`; keeps the conventional `1.06`. Never fails.
pub fn normalize_version(ver: &str) -> String {
    ver.trim()
        .split('.')
        .enumerate()
        .filter_map(|(i, s)| {
            let s = s.trim();
            if s.is_empty() {
                None
            } else if s.chars().all(|c| c.is_ascii_digit()) && (i == 0 || s.len() > 2) {
                Some(s.trim_start_matches('0').to_string()).map(|t| {
                    if t.is_empty() {
                        "0".into()
                    } else {
                        t
                    }
                })
            } else {
                Some(s.to_string())
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// Orders versions numerically segment by segment; non-numeric segments sort last.
pub fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let key = |v: &str| -> Vec<(u8, u64)> {
        v.split('.')
            .map(|s| match s.trim().parse::<u64>() {
                Ok(n) => (0, n),
                Err(_) => (1, 0),
            })
            .collect()
    };
    key(a).cmp(&key(b))
}

/// The higher of two versions, ignoring blanks.
pub fn newer_version(a: &str, b: &str) -> String {
    if a.trim().is_empty() {
        return b.trim().to_string();
    }
    if b.trim().is_empty() {
        return a.trim().to_string();
    }
    if version_cmp(a, b) == std::cmp::Ordering::Less {
        b.trim().to_string()
    } else {
        a.trim().to_string()
    }
}
