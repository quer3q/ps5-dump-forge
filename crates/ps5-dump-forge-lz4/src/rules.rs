//! The packing strategy, shared by zstd and LZ4 Pack (the pack format is a separate choice):
//! which files go into the packs, and how. The built-in packing strategy (the keep-loose list
//! and every root file stay loose), or a TOML profile of it (upstream `ampr_pack.py`'s format, as
//! the Lazy_AMPR community profiles write it), then, for LZ4 only, auto-loose over the large files
//! left. Globs follow Python's `fnmatchcase`: `*` crosses `/`, case-sensitive, whole-path match.

use std::collections::HashMap;

use toml::{Table, Value};

use crate::PackSpec;

// ---- glob ---------------------------------------------------------------------------------------

#[derive(Clone)]
enum Tok {
    Star,
    Any,
    Lit(char),
    Set {
        neg: bool,
        ranges: Vec<(char, char)>,
    },
}

/// `\` becomes `/`, leading `/` goes: both the pattern and the path are compared in this form.
fn norm(s: &str) -> Vec<char> {
    s.replace('\\', "/")
        .trim_start_matches('/')
        .chars()
        .collect()
}

fn compile(pat: &[char]) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < pat.len() {
        let c = pat[i];
        i += 1;
        match c {
            '*' => {
                if !matches!(out.last(), Some(Tok::Star)) {
                    out.push(Tok::Star);
                }
            }
            '?' => out.push(Tok::Any),
            '[' => {
                // fnmatch: an unclosed `[` is a literal; a `]` right after `[` or `[!` is a member.
                let mut j = i;
                if pat.get(j) == Some(&'!') {
                    j += 1;
                }
                if pat.get(j) == Some(&']') {
                    j += 1;
                }
                while j < pat.len() && pat[j] != ']' {
                    j += 1;
                }
                if j >= pat.len() {
                    out.push(Tok::Lit('['));
                    continue;
                }
                match bracket(&pat[i..j]) {
                    Bracket::Never => out.push(Tok::Set {
                        neg: false,
                        ranges: Vec::new(),
                    }),
                    Bracket::Any => out.push(Tok::Any),
                    Bracket::Set { neg, ranges } => out.push(Tok::Set { neg, ranges }),
                }
                i = j + 1;
            }
            c => out.push(Tok::Lit(c)),
        }
    }
    out
}

enum Bracket {
    Never,
    Any,
    Set {
        neg: bool,
        ranges: Vec<(char, char)>,
    },
}

/// A bracket body as CPython's `fnmatch.translate` reads it: split at range hyphens, drop reversed
/// ranges (merging the chunks around them), then an empty class never matches and a lone `!` is `.`.
fn bracket(body: &[char]) -> Bracket {
    // (char, is a literal): only the hyphens that join chunks stay range operators, as in
    // CPython, which escapes every other hyphen when it rebuilds the class.
    let mut stuff: Vec<(char, bool)> = body.iter().map(|&c| (c, true)).collect();
    if body.contains(&'-') {
        let mut chunks: Vec<Vec<char>> = Vec::new();
        let (mut i, mut k) = (0, if body.first() == Some(&'!') { 2 } else { 1 });
        while let Some(h) = body.get(k..).and_then(|t| t.iter().position(|&c| c == '-')) {
            let h = k + h;
            chunks.push(body[i..h].to_vec());
            i = h + 1;
            k = h + 3;
        }
        let last = body.get(i..).unwrap_or(&[]).to_vec();
        if last.is_empty() {
            if let Some(c) = chunks.last_mut() {
                c.push('-');
            }
        } else {
            chunks.push(last);
        }
        for k in (1..chunks.len()).rev() {
            if let (Some(&a), Some(&b)) = (chunks[k - 1].last(), chunks[k].first())
                && a > b
            {
                chunks[k - 1].pop();
                let rest = chunks[k][1..].to_vec();
                chunks[k - 1].extend(rest);
                chunks.remove(k);
            }
        }
        stuff.clear();
        for (n, c) in chunks.iter().enumerate() {
            if n > 0 {
                stuff.push(('-', false));
            }
            stuff.extend(c.iter().map(|&c| (c, true)));
        }
    }
    if stuff.is_empty() {
        return Bracket::Never;
    }
    if stuff == [('!', true)] {
        return Bracket::Any;
    }
    let neg = stuff[0] == ('!', true);
    let body = if neg { &stuff[1..] } else { &stuff[..] };
    let mut ranges = Vec::new();
    let mut k = 0;
    while k < body.len() {
        if k + 2 < body.len() && body[k + 1] == ('-', false) {
            ranges.push((body[k].0, body[k + 2].0));
            k += 3;
        } else {
            ranges.push((body[k].0, body[k].0));
            k += 1;
        }
    }
    Bracket::Set { neg, ranges }
}

fn tok_matches(t: &Tok, c: char) -> bool {
    match t {
        Tok::Any => true,
        Tok::Lit(l) => *l == c,
        Tok::Set { neg, ranges } => ranges.iter().any(|&(a, b)| a <= c && c <= b) != *neg,
        Tok::Star => false,
    }
}

/// Python `fnmatch.fnmatchcase(path, pattern)` after separator normalization. `*` crosses `/`.
/// One remembered star position gives the usual O(len(path) * len(pattern)) worst case.
pub fn glob_match(pattern: &str, path: &str) -> bool {
    glob_match_compiled(&compile(&norm(pattern)), &norm(path))
}

fn glob_match_compiled(toks: &[Tok], s: &[char]) -> bool {
    let (mut p, mut i) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while i < s.len() {
        match toks.get(p) {
            Some(Tok::Star) => {
                star = Some((p, i));
                p += 1;
            }
            Some(t) if tok_matches(t, s[i]) => {
                p += 1;
                i += 1;
            }
            _ => match star {
                Some((sp, si)) => {
                    star = Some((sp, si + 1));
                    p = sp + 1;
                    i = si + 1;
                }
                None => return false,
            },
        }
    }
    toks[p..].iter().all(|t| matches!(t, Tok::Star))
}

// ---- sizes --------------------------------------------------------------------------------------

/// A size string as upstream's `parse_size`: `_` removed, a numeric prefix (fractions allowed, the
/// product truncates toward zero) and a case-insensitive unit. No sign, exponent or overflow.
pub fn parse_size(s: &str) -> Result<u64, String> {
    let t: String = s.trim().chars().filter(|&c| c != '_').collect();
    let split = t
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(t.len());
    let (num, unit) = t.split_at(split);
    let unit = unit.trim_start();
    if num.is_empty() {
        return Err(format!("bad size `{s}`"));
    }
    let mult: u128 = match unit.to_ascii_lowercase().as_str() {
        "" | "b" => 1,
        "k" | "kb" => 1_000,
        "kib" => 1 << 10,
        "m" | "mb" => 1_000_000,
        "mib" => 1 << 20,
        "g" | "gb" => 1_000_000_000,
        "gib" => 1 << 30,
        "t" | "tb" => 1_000_000_000_000,
        "tib" => 1 << 40,
        _ => return Err(format!("bad size unit in `{s}`")),
    };
    let bytes = if num.bytes().all(|b| b.is_ascii_digit()) {
        // ponytail: digits beyond u128 are rejected as overflow, which is fine.
        let n: u128 = num.parse().map_err(|_| format!("size `{s}` overflows"))?;
        n.checked_mul(mult)
            .ok_or_else(|| format!("size `{s}` overflows"))?
    } else {
        let f: f64 = num.parse().map_err(|_| format!("bad size `{s}`"))?;
        let v = (f * mult as f64).trunc();
        if !v.is_finite() || v >= 18446744073709551616.0 {
            return Err(format!("size `{s}` overflows"));
        }
        v as u128
    };
    u64::try_from(bytes).map_err(|_| format!("size `{s}` overflows"))
}

// ---- safety and fallback ------------------------------------------------------------------------

fn has_suffix(path: &str, suffixes: &[&str]) -> bool {
    let p = path.as_bytes();
    suffixes
        .iter()
        .any(|s| p.len() >= s.len() && p[p.len() - s.len()..].eq_ignore_ascii_case(s.as_bytes()))
}

fn artifact(path: &str) -> bool {
    use crate::{CRC_SIDECAR, INDEX, JOURNAL, LOGS, MANIFEST, PROFILE, RUNTIME};
    // ASCII case-insensitive, as `always_loose`: an image folds case.
    ([MANIFEST, CRC_SIDECAR, PROFILE, INDEX, RUNTIME, JOURNAL].iter())
        .chain(LOGS.iter())
        .any(|name| name.eq_ignore_ascii_case(path))
}

/// Paths that are never packed: they are loaded by the console or the
/// runtime before (or outside) the pack layer. `path` is relative with `/` separators.
/// Names compare ASCII case-insensitively: exFAT and PFS images fold case, so `SCE_SYS/x` is
/// the folder the console reads.
pub fn always_loose(path: &str) -> bool {
    let file = path.rsplit('/').next().unwrap_or(path);
    let first = path.split('/').next().unwrap_or("").to_ascii_lowercase();
    let in_dir = path.contains('/');
    file.eq_ignore_ascii_case("eboot.bin")
        || has_suffix(path, &[".elf", ".self", ".prx", ".sprx"])
        || (in_dir && (first == "sce_sys" || first == "sce_module" || first.starts_with("fakelib")))
        || artifact(path)
}

const FALLBACK_SUFFIXES: &[&str] = &[
    ".bak",
    ".dat",
    ".utoc", // general
    ".json",
    ".ini",
    ".cfg",
    ".xml",
    ".txt", // config/text
    ".bk2",
    ".mp4",
    ".ivf",
    ".usm",
    ".bnk",
    ".wem",
    ".at9",
    ".pfs",
    ".img", // media/native
    ".png",
    ".jpg",
    ".jpeg",
    ".webp",
    ".gif",
    ".bmp",
    ".ico",
    ".tga",
    ".tif",
    ".tiff",
    ".exr",
    ".hdr",
    ".psd", // images
    ".dds",
    ".ktx",
    ".ktx2",
    ".astc",
    ".basis",
    ".gnf",
    ".gnfp",
    ".jxm",
    ".vtf", // GPU textures
    ".zip",
    ".7z",
    ".rar",
    ".gz",
    ".xz",
    ".bz2",
    ".zst",
    ".lz4",
    ".pak", // containers
    ".mp3",
    ".ogg",
    ".flac",
    ".aac",
    ".opus",
    ".m4a", // audio
    ".avi",
    ".mkv",
    ".mov",
    ".wmv",
    ".flv",
    ".webm",
    ".m4v", // video
    ".bik",
    ".mpg",
    ".mpeg",
    ".m2v", // more video
    ".uproject",
    ".uplugin",
    ".upluginmanifest", // Unreal reads these at boot
];

/// Directory families that are loose at any depth (`fakelib*` by prefix), and the two that are only
/// loose at the root; every folder name compares ASCII case-insensitively.
const PROTECTED_DIRS: &[&str] = &[
    "sce_module",
    "sce_sys",
    "system",
    "mods",
    "save",
    "fakelib",
    "trophy2",
    "uds",
];
const ROOT_ONLY_DIRS: &[&str] = &["decrypted", "_DUPLEX_"];
/// Video folders, loose at any depth, ASCII case-insensitive (not `cinematic*`: Miles Morales
/// packs `d/cinematic2`, an animation file).
const VIDEO_DIRS: &[&str] = &[
    "video",
    "videos",
    "movie",
    "movies",
    "cutscene",
    "cutscenes",
    "fmv",
];
/// Insomniac's extensionless boot and streaming containers, matched (lower-cased) on a file's
/// name at any depth.
const LOOSE_NAMES: &[&str] = &[
    "bootload_*",
    "critbootload",
    "soundbank",
    "soundbank.*",
    "soundbank_*",
    "wem",
    "wem.*",
    "wem_*",
    "screenreaderwem*",
    "movie",
    "movie_*",
    "movies",
    "movies_*",
];

/// Why the built-in packing strategy keeps a non-empty file loose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Loose {
    /// A root file.
    Root,
    /// A keep-loose suffix (the suffix as listed, lower case).
    Suffix(&'static str),
    /// A system or runtime file, a protected or video folder, or a container name.
    Protected,
}

fn protected(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    let (name, dirs) = parts.split_last().unwrap_or((&"", &[]));
    let name = name.to_ascii_lowercase();
    let is = |d: &str, list: &[&str]| list.iter().any(|v| d.eq_ignore_ascii_case(v));
    dirs.iter().any(|d| {
        is(d, PROTECTED_DIRS)
            || is(d, VIDEO_DIRS)
            || d.get(..7)
                .is_some_and(|p| p.eq_ignore_ascii_case("fakelib"))
    }) || dirs.first().is_some_and(|d| is(d, ROOT_ONLY_DIRS))
        || LOOSE_NAMES.iter().any(|p| glob_match(p, &name))
}

fn suffix(path: &str) -> Option<&'static str> {
    FALLBACK_SUFFIXES
        .iter()
        .copied()
        .find(|s| has_suffix(path, &[s]))
}

/// The keep-loose list: anything the game reads other than through AMPR (plain reads, mmap),
/// before or after it initializes AMPR (container indexes, configs, media, the protected
/// directory families, video folders, Insomniac's boot and streaming containers).
pub fn keep_loose(path: &str) -> bool {
    suffix(path).is_some() || protected(path)
}

/// Why the built-in packing strategy keeps `path` loose, `None` when it packs it (an empty file
/// is loose too, whatever this says).
pub fn loose_reason(path: &str) -> Option<Loose> {
    if !path.contains('/') {
        Some(Loose::Root)
    } else if always_loose(path) {
        Some(Loose::Protected)
    } else if let Some(s) = suffix(path) {
        Some(Loose::Suffix(s))
    } else {
        protected(path).then_some(Loose::Protected)
    }
}

/// The built-in packing strategy's loose set: the keep-loose list and every root file.
fn fallback_loose(path: &str) -> bool {
    !path.contains('/') || keep_loose(path)
}

// ---- selection ----------------------------------------------------------------------------------

const SHIFT_64K: u8 = 16;

fn plain_spec() -> PackSpec {
    PackSpec {
        block_shift: SHIFT_64K,
        store: false,
        hot: false,
        random: false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Compress,
    Store,
    Loose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Auto,
    Random,
    Mixed,
}

/// One `[[rule]]`, validated; patterns are stored as written (normalized when matched).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub action: Action,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub block_shift: u8,
    pub layout: Layout,
    pub hot: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub default_action: Action,
    pub default_block_shift: u8,
    /// Private, so `compiled` always matches it: read through [`Profile::rules`].
    rules: Vec<Rule>,
    pub auto_loose: AutoLoose,
    /// Keys that have no effect here, as `scope.key` (`table.*` for a whole table), for the
    /// job log.
    pub ignored: Vec<String>,
    compiled: Compiled,
}

impl Profile {
    /// The `[[rule]]`s in file order; the last match wins.
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }
}

/// `Profile::rules` compiled once by `load_profile`, so matching a file compiles nothing and
/// patterns that name one exact path (a profile may list one per packed file) are a map
/// lookup, not a scan. Selects the same rule as the plain reverse scan: the last rule with a
/// matching include and no matching exclude.
#[derive(Clone, Default)]
struct Compiled {
    /// Per rule: its include patterns that are not one exact path.
    globs: Vec<Vec<Vec<Tok>>>,
    /// Per rule: its exclude patterns.
    excludes: Vec<Vec<Vec<Tok>>>,
    /// The rules with any `globs`, ascending.
    globbed: Vec<usize>,
    /// Normalized exact path -> the rules (ascending) with an include naming exactly it.
    exact: HashMap<String, Vec<usize>>,
}

// Derived from `rules`, so equal rules mean equal `Compiled`; too big to print.
impl PartialEq for Compiled {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl std::fmt::Debug for Compiled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Compiled")
    }
}

/// The one path a pattern matches when every token matches one fixed char (`[*]`, `[[]` and
/// `[a-a]` included), else `None`.
fn literal(toks: &[Tok]) -> Option<String> {
    toks.iter()
        .map(|t| match t {
            Tok::Lit(c) => Some(*c),
            Tok::Set { neg: false, ranges } if ranges.len() == 1 && ranges[0].0 == ranges[0].1 => {
                Some(ranges[0].0)
            }
            _ => None,
        })
        .collect()
}

impl Compiled {
    fn new(rules: &[Rule]) -> Compiled {
        let mut c = Compiled::default();
        for (i, r) in rules.iter().enumerate() {
            let mut globs = Vec::new();
            for p in &r.include {
                let toks = compile(&norm(p));
                match literal(&toks) {
                    Some(path) => {
                        let rs = c.exact.entry(path).or_default();
                        if rs.last() != Some(&i) {
                            rs.push(i);
                        }
                    }
                    None => globs.push(toks),
                }
            }
            if !globs.is_empty() {
                c.globbed.push(i);
            }
            c.globs.push(globs);
            c.excludes
                .push(r.exclude.iter().map(|p| compile(&norm(p))).collect());
        }
        c
    }

    /// The index of the last rule matching `path`.
    fn last_match(&self, path: &str) -> Option<usize> {
        let s = norm(path);
        let hit = |ps: &[Vec<Tok>]| ps.iter().any(|t| glob_match_compiled(t, &s));
        let key: String = s.iter().collect();
        let exact = self
            .exact
            .get(&key)
            .and_then(|rs| rs.iter().rev().copied().find(|&r| !hit(&self.excludes[r])));
        // A later rule can only beat it through a glob include.
        self.globbed
            .iter()
            .rev()
            .copied()
            .take_while(|&r| exact.is_none_or(|e| r > e))
            .find(|&r| hit(&self.globs[r]) && !hit(&self.excludes[r]))
            .or(exact)
    }
}
/// `None` keeps the file loose. Empty files and `always_loose` paths are loose before anything
/// else. Without a profile, the built-in packing strategy: the keep-loose list and root files
/// stay loose, everything else is packed with 64 KiB blocks. With one, its rules decide
/// ([`profile_spec`]): the keep-loose list doesn't apply and root files are the profile's call.
pub fn select(path: &str, size: u64, profile: Option<&Profile>) -> Option<PackSpec> {
    if size == 0 || always_loose(path) {
        return None;
    }
    match profile {
        None => (!fallback_loose(path)).then(plain_spec),
        // A profile is the user's call: only `always_loose` (system and runtime files) wins.
        Some(p) => profile_spec(path, p),
    }
}

/// What `p`'s own rules choose for a non-empty, not `always_loose` file: its last matching rule,
/// else the profile default.
pub fn profile_spec(path: &str, p: &Profile) -> Option<PackSpec> {
    let rule = p.compiled.last_match(path).map(|i| &p.rules[i]);
    let (action, block_shift, layout, hot) = match rule {
        Some(r) => (r.action, r.block_shift, r.layout, r.hot),
        None => (p.default_action, p.default_block_shift, Layout::Auto, false),
    };
    let random = layout == Layout::Random || (layout == Layout::Auto && hot);
    match action {
        Action::Loose => None,
        a => Some(PackSpec {
            block_shift,
            store: a == Action::Store,
            hot,
            random,
        }),
    }
}

// ---- auto-loose ---------------------------------------------------------------------------------

/// Upstream `ampr_pack.py`'s auto-loose, the `[pack] auto_loose_*` keys: a large file packed with
/// compression whose sampled blocks barely shrink stays loose (packing it would only add the
/// pack layer to every read). Every rule source uses it; without a profile, the defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoLoose {
    pub enabled: bool,
    /// Hot files (a profile's `hot = true`) are sampled too.
    pub hot: bool,
    pub min_file_size: u64,
    pub sample_blocks: u32,
    pub sample_bytes: u64,
    pub min_savings_ratio: f64,
    pub max_raw_ratio: f64,
}

impl Default for AutoLoose {
    fn default() -> Self {
        Self {
            enabled: true,
            hot: false,
            min_file_size: 64 << 20,
            sample_blocks: 32,
            sample_bytes: 16 << 20,
            min_savings_ratio: 0.05,
            max_raw_ratio: 0.90,
        }
    }
}

/// What sampling a file's blocks found, as the writer would store them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sample {
    pub raw_bytes: u64,
    pub stored_bytes: u64,
    pub blocks: u64,
    pub raw_blocks: u64,
}

impl Sample {
    pub fn savings(&self) -> f64 {
        if self.raw_bytes == 0 {
            return 0.0;
        }
        1.0 - self.stored_bytes as f64 / self.raw_bytes as f64
    }

    pub fn raw_ratio(&self) -> f64 {
        if self.blocks == 0 {
            return 0.0;
        }
        self.raw_blocks as f64 / self.blocks as f64
    }
}

impl AutoLoose {
    /// Whether a `size`-byte file packed per `spec` is sampled: compressed (not `store`), not
    /// hot unless `hot`, at least `min_file_size`.
    pub fn applies(&self, size: u64, spec: &PackSpec) -> bool {
        self.enabled
            && !spec.store
            && (!spec.hot || self.hot)
            && size > 0
            && size >= self.min_file_size
    }

    /// The block indices to sample, sorted: at most `sample_blocks` and `sample_bytes` worth
    /// (one at least), spread evenly from the first block to the last by integer
    /// interpolation: deterministic, no randomness.
    pub fn sample_indices(&self, size: u64, block_shift: u8) -> Vec<u64> {
        let total = size.div_ceil(1 << block_shift);
        let by_bytes = (self.sample_bytes >> block_shift).max(1);
        let n = u64::from(self.sample_blocks).min(by_bytes);
        if total == 0 || n == 0 {
            return Vec::new();
        }
        if total <= n {
            return (0..total).collect();
        }
        if n == 1 {
            return vec![total / 2];
        }
        let mut v: Vec<u64> = (0..n)
            .map(|k| (u128::from(k) * u128::from(total - 1) / u128::from(n - 1)) as u64)
            .collect();
        v.dedup();
        v
    }

    /// Whether `s` keeps the file loose: it saves less than `min_savings_ratio`, or at least
    /// `max_raw_ratio` of its blocks stay RAW.
    pub fn keeps_loose(&self, s: &Sample) -> bool {
        s.savings() < self.min_savings_ratio || s.raw_ratio() >= self.max_raw_ratio
    }
}

// ---- TOML profile -------------------------------------------------------------------------------

const PACK_KEYS: &[&str] = &[
    "default_action",
    "default_block_size",
    "index_name",
    "io_page_size",
    "auto_loose_large_files",
    "auto_loose_hot_files",
    "auto_loose_min_file_size",
    "auto_loose_sample_blocks",
    "auto_loose_sample_bytes",
    "auto_loose_min_savings_ratio",
    "auto_loose_max_raw_ratio",
];
const RULE_KEYS: &[&str] = &[
    "action",
    "include",
    "exclude",
    "block_size",
    "layout",
    "hot",
];
const DEFAULT_INDEX_NAME: &str = "ampr_assets.index";
const IO_PAGE: u64 = 65536;

struct Ctx {
    errs: Vec<String>,
    ignored: Vec<String>,
}

impl Ctx {
    fn err(&mut self, m: String) {
        self.errs.push(m);
    }

    fn note(&mut self, m: String) {
        if !self.ignored.contains(&m) {
            self.ignored.push(m);
        }
    }

    /// Keys Forge has no use for (upstream's tuning, typos) go to the log list, not refused,
    /// so community profiles load.
    fn keys(&mut self, scope: &str, t: &Table, honored: &[&str]) {
        for k in t.keys().filter(|k| !honored.contains(&k.as_str())) {
            self.note(format!("{scope}.{k}"));
        }
    }

    fn table<'a>(&mut self, scope: &str, v: &'a Value) -> Option<&'a Table> {
        let t = v.as_table();
        if t.is_none() {
            self.err(format!("`{scope}` must be a table"));
        }
        t
    }

    fn size(&mut self, key: &str, v: &Value) -> Option<u64> {
        let r = match v {
            Value::Integer(i) => u64::try_from(*i).map_err(|_| format!("`{key}` is negative")),
            Value::String(s) => parse_size(s).map_err(|e| format!("`{key}`: {e}")),
            _ => Err(format!("`{key}` must be an integer or a size string")),
        };
        r.map_err(|e| self.err(e)).ok()
    }

    fn block_shift(&mut self, key: &str, v: &Value) -> Option<u8> {
        let n = self.size(key, v)?;
        if !n.is_power_of_two() || !(1 << 14..=1 << 20).contains(&n) {
            self.err(format!(
                "`{key}` must be a power of two from 16 KiB to 1 MiB, got {n}"
            ));
            return None;
        }
        Some(n.trailing_zeros() as u8)
    }

    fn boolean(&mut self, key: &str, v: &Value) -> Option<bool> {
        let b = v.as_bool();
        if b.is_none() {
            self.err(format!("`{key}` must be a boolean"));
        }
        b
    }

    /// A number in `0..1` (`0..=1` with `one`).
    fn ratio(&mut self, key: &str, v: &Value, one: bool) -> Option<f64> {
        let r = match v {
            Value::Float(f) => Some(*f),
            Value::Integer(i) => Some(*i as f64),
            _ => None,
        };
        match r {
            Some(r) if r >= 0.0 && (r < 1.0 || (one && r == 1.0)) => Some(r),
            _ => {
                let range = if one {
                    "0 to 1"
                } else {
                    "0 up to (not including) 1"
                };
                self.err(format!("`{key}` must be a number from {range}"));
                None
            }
        }
    }

    fn action(&mut self, key: &str, v: &Value) -> Option<Action> {
        match v.as_str().map(str::to_ascii_lowercase).as_deref() {
            Some("compress") => Some(Action::Compress),
            Some("store") => Some(Action::Store),
            Some("loose") => Some(Action::Loose),
            _ => {
                self.err(format!(
                    "`{key}` must be \"compress\", \"store\" or \"loose\""
                ));
                None
            }
        }
    }

    fn layout(&mut self, v: &Value) -> Option<Layout> {
        match v.as_str().map(str::to_ascii_lowercase).as_deref() {
            Some("auto") => Some(Layout::Auto),
            Some("random") => Some(Layout::Random),
            Some("mixed") => Some(Layout::Mixed),
            Some("streaming") => {
                self.note("rule.layout=streaming (used as mixed)".into());
                Some(Layout::Mixed)
            }
            _ => {
                self.err(
                    "`rule.layout` must be \"auto\", \"random\", \"mixed\" or \"streaming\"".into(),
                );
                None
            }
        }
    }

    /// One string or a list of strings.
    fn strings(&mut self, key: &str, v: &Value) -> Option<Vec<String>> {
        let one = |x: &Value| x.as_str().map(str::to_owned);
        let r = match v {
            Value::String(s) => Some(vec![s.clone()]),
            Value::Array(a) => a.iter().map(one).collect(),
            _ => None,
        };
        if r.is_none() {
            self.err(format!("`{key}` must be a string or a list of strings"));
        }
        r
    }
}

fn parse_rule(c: &mut Ctx, n: usize, v: &Value, default_shift: u8) -> Option<Rule> {
    let t = c.table(&format!("rule[{n}]"), v)?;
    c.keys("rule", t, RULE_KEYS);
    let before = c.errs.len();
    // The profile may sit on another device: pattern files beside it are not read.
    for k in ["include_from", "exclude_from"] {
        if t.contains_key(k) {
            c.err(format!(
                "`rule.{k}` is not supported: list the patterns in the rule itself"
            ));
        }
    }
    let mut r = Rule {
        action: Action::Compress,
        include: Vec::new(),
        exclude: Vec::new(),
        block_shift: default_shift,
        layout: Layout::Auto,
        hot: false,
    };
    if let Some(v) = t.get("action") {
        r.action = c.action("rule.action", v).unwrap_or(r.action);
    }
    if let Some(v) = t.get("block_size") {
        r.block_shift = c.block_shift("rule.block_size", v).unwrap_or(default_shift);
    }
    if let Some(v) = t.get("layout") {
        r.layout = c.layout(v).unwrap_or(Layout::Auto);
    }
    if let Some(v) = t.get("hot") {
        match v.as_bool() {
            Some(b) => r.hot = b,
            None => c.err("`rule.hot` must be a boolean".into()),
        }
    }
    let list = |c: &mut Ctx, key: &str| match t.get(key) {
        Some(v) => c.strings(&format!("rule.{key}"), v).unwrap_or_default(),
        None => Vec::new(),
    };
    r.include = match t.contains_key("include") {
        true => list(c, "include"),
        false => vec!["**".into()],
    };
    r.exclude = list(c, "exclude");
    if r.include.is_empty() && c.errs.len() == before {
        c.err(format!("rule[{n}] has no include patterns"));
    }
    (c.errs.len() == before).then_some(r)
}

/// The `[pack] auto_loose_*` keys over upstream's defaults, with upstream's bounds.
fn parse_auto_loose(c: &mut Ctx, t: &Table, a: &mut AutoLoose) {
    let key = |k: &str| format!("pack.auto_loose_{k}");
    if let Some(v) = t.get("auto_loose_large_files") {
        a.enabled = c.boolean(&key("large_files"), v).unwrap_or(a.enabled);
    }
    if let Some(v) = t.get("auto_loose_hot_files") {
        a.hot = c.boolean(&key("hot_files"), v).unwrap_or(a.hot);
    }
    if let Some(v) = t.get("auto_loose_min_file_size") {
        a.min_file_size = c.size(&key("min_file_size"), v).unwrap_or(a.min_file_size);
    }
    if let Some(v) = t.get("auto_loose_sample_blocks") {
        match v.as_integer().and_then(|i| u32::try_from(i).ok()) {
            Some(n) if (1..=4096).contains(&n) => a.sample_blocks = n,
            _ => c.err(format!(
                "`{}` must be an integer from 1 to 4096",
                key("sample_blocks")
            )),
        }
    }
    if let Some(v) = t.get("auto_loose_sample_bytes") {
        match c.size(&key("sample_bytes"), v) {
            Some(0) => c.err(format!("`{}` must be at least 1 byte", key("sample_bytes"))),
            Some(n) => a.sample_bytes = n,
            None => {}
        }
    }
    if let Some(v) = t.get("auto_loose_min_savings_ratio") {
        let k = key("min_savings_ratio");
        a.min_savings_ratio = c.ratio(&k, v, false).unwrap_or(a.min_savings_ratio);
    }
    if let Some(v) = t.get("auto_loose_max_raw_ratio") {
        a.max_raw_ratio = c
            .ratio(&key("max_raw_ratio"), v, true)
            .unwrap_or(a.max_raw_ratio);
    }
}

/// Parses and validates a profile's text; every problem is a finding. Keys and tables Forge has
/// no use for (`[groups]`, `[runtime]`, upstream's tuning, typos) are listed in
/// [`Profile::ignored`], not refused.
pub fn parse_profile(text: &str) -> Result<Profile, Vec<String>> {
    let root: Table = text.parse().map_err(|e| vec![format!("{e}")])?;
    let mut c = Ctx {
        errs: Vec::new(),
        ignored: Vec::new(),
    };
    let mut p = Profile {
        default_action: Action::Loose,
        default_block_shift: SHIFT_64K,
        rules: Vec::new(),
        auto_loose: AutoLoose::default(),
        ignored: Vec::new(),
        compiled: Compiled::default(),
    };
    for k in root
        .keys()
        .filter(|k| !["pack", "rule"].contains(&k.as_str()))
    {
        c.note(format!("{k}.*"));
    }
    if let Some(v) = root.get("pack")
        && let Some(t) = c.table("pack", v)
    {
        c.keys("pack", t, PACK_KEYS);
        if let Some(v) = t.get("default_action") {
            p.default_action = c
                .action("pack.default_action", v)
                .unwrap_or(p.default_action);
        }
        if let Some(v) = t.get("default_block_size") {
            p.default_block_shift = c
                .block_shift("pack.default_block_size", v)
                .unwrap_or(SHIFT_64K);
        }
        if let Some(v) = t.get("index_name")
            && v.as_str() != Some(DEFAULT_INDEX_NAME)
        {
            c.err(format!(
                "`pack.index_name` must be \"{DEFAULT_INDEX_NAME}\""
            ));
        }
        if let Some(v) = t.get("io_page_size")
            && c.size("pack.io_page_size", v).is_some_and(|n| n != IO_PAGE)
        {
            c.err("`pack.io_page_size` must be 64 KiB".into());
        }
        parse_auto_loose(&mut c, t, &mut p.auto_loose);
    }
    match root.get("rule") {
        None => {}
        Some(Value::Array(a)) => {
            for (n, v) in a.iter().enumerate() {
                p.rules
                    .extend(parse_rule(&mut c, n, v, p.default_block_shift));
            }
        }
        Some(_) => c.err("`rule` must be an array of tables ([[rule]])".into()),
    }
    if !c.errs.is_empty() {
        return Err(c.errs);
    }
    p.ignored = c.ignored;
    p.compiled = Compiled::new(&p.rules);
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(shift: u8, store: bool, hot: bool, random: bool) -> Option<PackSpec> {
        Some(PackSpec {
            block_shift: shift,
            store,
            hot,
            random,
        })
    }

    #[test]
    fn fallback_table() {
        let f = |p: &str| select(p, 10, None);
        assert_eq!(f("data/a.bin"), spec(16, false, false, false));
        assert_eq!(f("a/b/c.bin"), spec(16, false, false, false));
        assert_eq!(f("root.bin"), None, "root-level files stay loose");
        assert_eq!(f("data/a.PNG"), None);
        assert_eq!(f("data/a.json"), None);
        assert_eq!(f("data/a.utoc"), None);
        assert_eq!(f("data/system/a.bin"), None, "nested protected family");
        assert_eq!(f("system/a.bin"), None);
        assert_eq!(f("decrypted/a.bin"), None, "root-only family");
        assert_eq!(f("data/decrypted/a.bin"), spec(16, false, false, false));
        assert_eq!(f("data/_DUPLEX_/a.bin"), spec(16, false, false, false));
        assert_eq!(f("_DUPLEX_/a.bin"), None);
        assert_eq!(f("data/eboot.bin"), None);
        // Protected families, any case; `fakelib*` by prefix at any depth.
        for p in ["data/System/a.bin", "data/fakelib_x/a.bin"] {
            assert_eq!(f(p), None, "{p}");
        }
        assert_eq!(
            f("data/systems/a.bin"),
            spec(16, false, false, false),
            "whole names only"
        );
        for p in [
            "d/a.bik",
            "d/a.MPG",
            "d/a.mpeg",
            "d/a.m2v",
            "sb/SB.uproject",
            "sb/plugins/x.uplugin",
            "sb/plugins/x.UPluginManifest",
        ] {
            assert_eq!(f(p), None, "{p}");
            assert!(keep_loose(p), "{p}");
        }
        // Insomniac's containers and video folders, any depth and case.
        for p in ["D/BootLoad_main", "a/Cutscenes/c.bin"] {
            assert_eq!(f(p), None, "{p}");
            assert_eq!(loose_reason(p), Some(Loose::Protected), "{p}");
        }
        for p in ["d/cinematic2", "a/videogame/b.bin"] {
            assert_eq!(f(p), spec(16, false, false, false), "{p}");
            assert_eq!(loose_reason(p), None, "{p}");
        }
        assert!(
            !keep_loose("root.bin"),
            "the root rule is the fallback's only"
        );
        assert_eq!(select("data/a.bin", 0, None), None, "empty");
    }

    /// A Lazy_AMPR community profile (Astro Bot's), as users share them: `[groups]`, upstream
    /// tuning keys and a typo (`ayout`) load as ignored keys.
    const ASTROBOT: &str = r#"# Generated by ampr_pack_profile.py 4.0 from APR traces.

[pack]
index_name = "ampr_assets.index"
pack_pattern = "ampr_assets-{group}-lane{lane:02d}-vol{volume:02d}-{id:03d}.pak"
default_action = "loose"
default_block_size = "64KiB"
io_page_size = "64KiB"
payload_alignment = "64KiB"
chunk_alignment = "64B"
workers = 8
compression_mode = "hc"
compression_level = 12
acceleration = 1
deduplicate = true
deduplicate_scope = "lane"
deduplicate_streaming = false
min_savings_bytes = 64
min_savings_ratio = 0.01
io_neutral_min_savings_bytes = "8KiB"
io_neutral_min_savings_ratio = 0.125
auto_loose_large_files = false
auto_loose_hot_files = false
auto_loose_min_file_size = "64MiB"
auto_loose_sample_blocks = 32
auto_loose_sample_bytes = "16MiB"
auto_loose_min_savings_ratio = 0.05
auto_loose_max_raw_ratio = 0.90
preserve_mtime = true
validate_index_metadata = true

[groups.assets]
pack_count = 4
assignment = "balanced"
max_pack_size = "16GiB"
stripe_large_files = false

[[rule]]
action = "compress"
include = ["data/prein/**"]
block_size = "64KiB"
group = "assets"
layout = "mixed"

[[rule]]
action = "compress"
include = ["data/prein/**/*.gnfp", "data/prein/**/*.jxm"]
block_size = "128KiB"
group = "assets"
layout = "mixed"

# observed files=1, reads=211, submitted=5.25 MiB
# confidence=low:1
[[rule]]
action = "compress"
include = ["data/prein/sound/**"]
block_size = "16KiB"
group = "assets"
ayout = "random"
hot = true
min_savings_bytes = 16
min_savings_ratio = 0.0025
io_neutral_min_savings_bytes = "2KiB"
io_neutral_min_savings_ratio = 0.125

# Safety exclusions. The last matching rule wins.
[[rule]]
action = "loose"
include = [
  "data/prein/video/*",
  "sce_module/**",
  "system/**",
  "mods/**",
  "save/**",
  "**/*.prx",
  "**/*.sprx",
  "eboot.bin",
  "ampr_emu.index",
  "ampr_assets.index",
  "ampr_assets-*.pak",
]
"#;

    #[test]
    fn community_profile_selects_and_broken_ones_are_findings() {
        let p = parse_profile(ASTROBOT).unwrap();
        assert!(!p.auto_loose.enabled);
        for k in ["groups.*", "rule.ayout"] {
            assert!(p.ignored.iter().any(|i| i == k), "{k}: {:?}", p.ignored);
        }
        let f = |path: &str| select(path, 10, Some(&p));
        assert_eq!(f("data/prein/sound/a.bin"), spec(14, false, true, true));
        assert_eq!(
            f("data/prein/video/a.bin"),
            None,
            "the last matching rule wins"
        );
        assert_eq!(
            f("data/prein/t/a.gnfp"),
            spec(17, false, false, false),
            "a profile overrides the keep-loose list"
        );
        assert_eq!(f("sce_sys/icon0.png"), None, "always_loose still wins");
        assert_eq!(f("data/other.bin"), None, "default_action = loose");
        for bad in [
            "[pack\n",
            "[[rule]]\naction = \"pack\"\n",
            "[pack]\nindex_name = \"other.index\"\n",
        ] {
            assert!(parse_profile(bad).is_err(), "{bad}");
        }
    }
}
