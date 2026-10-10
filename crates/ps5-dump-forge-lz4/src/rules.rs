//! Which files go into the packs, and how: the TOML profile subset, trace-observed files and the
//! built-in fallback, behind one `select`. Globs follow Python's `fnmatchcase` (as upstream
//! `ampr_pack.py`'s profiles do): `*` crosses `/`, case-sensitive, whole-path match.

use std::collections::BTreeSet;
use std::path::Path;

use crate::{PackSpec, RuntimeProfile};

// ---- glob ---------------------------------------------------------------------------------------

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
    [MANIFEST, CRC_SIDECAR, PROFILE, INDEX, RUNTIME, JOURNAL].contains(&path)
        || LOGS.contains(&path)
}

/// Paths that stay loose whatever a profile or a trace says: they are loaded by the console or the
/// runtime before (or outside) the pack layer. `path` is relative with `/` separators.
pub fn always_loose(path: &str) -> bool {
    let file = path.rsplit('/').next().unwrap_or(path);
    let first = path.split('/').next().unwrap_or("");
    let in_dir = path.contains('/');
    file == "eboot.bin"
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

/// Directory families that are loose at any depth, and the two that are only loose at the root.
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

/// The keep-loose list, for every rule source (a profile included): what an engine may open,
/// stat or list before the game initializes AMPR, when packed files do not exist yet (container
/// indexes, configs, media, the protected directory families).
pub fn keep_loose(path: &str) -> bool {
    if has_suffix(path, FALLBACK_SUFFIXES) {
        return true;
    }
    let dirs: Vec<&str> = path.split('/').collect();
    let dirs = &dirs[..dirs.len() - 1];
    dirs.iter().any(|d| PROTECTED_DIRS.contains(d))
        || dirs.first().is_some_and(|d| ROOT_ONLY_DIRS.contains(d))
}

/// The built-in guess's loose set: the keep-loose list and every root file.
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

impl Rule {
    fn matches(&self, path: &str) -> bool {
        self.include.iter().any(|p| glob_match(p, path))
            && !self.exclude.iter().any(|p| glob_match(p, path))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub default_action: Action,
    pub default_block_shift: u8,
    pub rules: Vec<Rule>,
    pub runtime: Option<RuntimeProfile>,
    pub auto_loose: AutoLoose,
    /// Known keys that are accepted and have no effect here, as `scope.key`, for the job log.
    pub ignored: Vec<String>,
}

/// Where the choices come from, in the order the caller prefers: an explicit profile, this dump's
/// traces (relative paths of the files the game read), the built-in table.
pub enum Selection<'a> {
    Profile(&'a Profile),
    Observed(&'a BTreeSet<String>),
    Fallback,
}

/// `None` keeps the file loose. Empty files and `always_loose` paths are loose before anything else.
pub fn select(path: &str, size: u64, sel: &Selection) -> Option<PackSpec> {
    if size == 0 || always_loose(path) {
        return None;
    }
    match sel {
        // Traces only narrow the built-in guess: the engine opens containers' indexes, configs and
        // media with plain file I/O at boot, so an observed file the fallback keeps loose stays loose.
        Selection::Observed(set) => (set.contains(path) && !fallback_loose(path)).then(plain_spec),
        Selection::Fallback => (!fallback_loose(path)).then(plain_spec),
        // A profile cannot pack what the keep-loose list keeps (Lazy_AMPR forces its exclusions on
        // custom profiles too); root files are the profile's call.
        Selection::Profile(p) => profile_spec(path, p).filter(|_| !keep_loose(path)),
    }
}

/// What `p`'s own rules choose for a non-empty, not `always_loose` file: its last matching rule,
/// else the profile default. [`select`] then applies the keep-loose list.
pub fn profile_spec(path: &str, p: &Profile) -> Option<PackSpec> {
    let rule = p.rules.iter().rev().find(|r| r.matches(path));
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

use toml::{Table, Value};

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
const PACK_IGNORED: &[&str] = &[
    "pack_pattern",
    "payload_alignment",
    "chunk_alignment",
    "workers",
    "compression_mode",
    "compression_level",
    "acceleration",
    "min_savings_bytes",
    "min_savings_ratio",
    "io_neutral_min_savings_bytes",
    "io_neutral_min_savings_ratio",
    "deduplicate",
    "deduplicate_scope",
    "deduplicate_streaming",
    "preserve_mtime",
    "validate_index_metadata",
    "self_contained",
    "required_packed",
];
const GROUP_IGNORED: &[&str] = &[
    "pack_count",
    "assignment",
    "max_pack_size",
    "stripe_large_files",
    "stripe_threshold",
    "stripe_group_blocks",
    "io_page_size",
];
const RULE_KEYS: &[&str] = &[
    "action",
    "include",
    "exclude",
    "include_from",
    "exclude_from",
    "block_size",
    "layout",
    "hot",
];
const RULE_IGNORED: &[&str] = &[
    "group",
    "compression_mode",
    "compression_level",
    "acceleration",
    "min_savings_bytes",
    "min_savings_ratio",
    "io_neutral_min_savings_bytes",
    "io_neutral_min_savings_ratio",
    "streaming",
    "force_pack",
];
const RUNTIME_KEYS: &[&str] = &[
    "decoded_cache_bytes",
    "physical_cache_bytes",
    "workers",
    "latency_reserve_workers",
];

const DEFAULT_INDEX_NAME: &str = "ampr_assets.index";
const IO_PAGE: u64 = 65536;

struct Ctx {
    dir: std::path::PathBuf,
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

    /// Unknown keys are findings, known-ignored ones go to the log list.
    fn keys(&mut self, scope: &str, t: &Table, honored: &[&str], ignored: &[&str]) {
        for k in t.keys() {
            if honored.contains(&k.as_str()) {
            } else if ignored.contains(&k.as_str()) {
                self.note(format!("{scope}.{k}"));
            } else {
                self.err(format!("unknown key `{scope}.{k}`"));
            }
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

    /// Pattern lines of a file that must resolve inside the TOML's own directory.
    fn pattern_file(&mut self, key: &str, name: &str) -> Vec<String> {
        let rel = Path::new(name);
        if rel.is_absolute() || name.starts_with(['/', '\\']) {
            self.err(format!("`{key}`: `{name}` must be relative to the profile"));
            return Vec::new();
        }
        let root = match self.dir.canonicalize() {
            Ok(r) => r,
            Err(e) => {
                self.err(format!("`{key}`: profile folder: {e}"));
                return Vec::new();
            }
        };
        let full = match root.join(rel).canonicalize() {
            Ok(f) => f,
            Err(e) => {
                self.err(format!("`{key}`: `{name}`: {e}"));
                return Vec::new();
            }
        };
        if !full.starts_with(&root) {
            self.err(format!("`{key}`: `{name}` leaves the profile's folder"));
            return Vec::new();
        }
        match std::fs::read_to_string(&full) {
            Ok(s) => s
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(str::to_owned)
                .collect(),
            Err(e) => {
                self.err(format!("`{key}`: `{name}`: {e}"));
                Vec::new()
            }
        }
    }
}

fn parse_rule(c: &mut Ctx, n: usize, v: &Value, default_shift: u8) -> Option<Rule> {
    let t = c.table(&format!("rule[{n}]"), v)?;
    c.keys("rule", t, RULE_KEYS, RULE_IGNORED);
    let before = c.errs.len();
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
    let lists = |c: &mut Ctx, key: &str, from: &str| {
        let mut out = Vec::new();
        if let Some(v) = t.get(key) {
            out.extend(c.strings(&format!("rule.{key}"), v).unwrap_or_default());
        }
        let mut loaded = Vec::new();
        if let Some(v) = t.get(from) {
            for f in c.strings(&format!("rule.{from}"), v).unwrap_or_default() {
                loaded.extend(c.pattern_file(&format!("rule.{from}"), &f));
            }
        }
        (out, loaded, t.contains_key(key))
    };
    let (inc, inc_loaded, inc_given) = lists(c, "include", "include_from");
    let (exc, exc_loaded, _) = lists(c, "exclude", "exclude_from");
    r.include = inc;
    if !inc_given && inc_loaded.is_empty() {
        r.include.push("**".into());
    }
    r.include.extend(inc_loaded);
    r.exclude = exc;
    r.exclude.extend(exc_loaded);
    if r.include.is_empty() && c.errs.len() == before {
        c.err(format!("rule[{n}] has no include patterns"));
    }
    (c.errs.len() == before).then_some(r)
}

fn parse_runtime(c: &mut Ctx, v: &Value) -> Option<RuntimeProfile> {
    let t = c.table("runtime", v)?;
    c.keys("runtime", t, RUNTIME_KEYS, &[]);
    let before = c.errs.len();
    let get = |c: &mut Ctx, k: &str| -> Option<&Value> {
        let v = t.get(k);
        if v.is_none() {
            c.err(format!("`runtime.{k}` is required"));
        }
        v
    };
    let cache = |c: &mut Ctx, k: &str| {
        let v = get(c, k)?;
        let n = c.size(&format!("runtime.{k}"), v)?;
        if n % 16384 != 0 {
            c.err(format!(
                "`runtime.{k}` must be a multiple of 16384, got {n}"
            ));
            return None;
        }
        Some(n)
    };
    let decoded = cache(c, "decoded_cache_bytes");
    let physical = cache(c, "physical_cache_bytes");
    let int = |c: &mut Ctx, k: &str| {
        let v = get(c, k)?;
        let n = v.as_integer().and_then(|i| u32::try_from(i).ok());
        if n.is_none() {
            c.err(format!("`runtime.{k}` must be a non-negative integer"));
        }
        n
    };
    let workers = int(c, "workers");
    let reserve = int(c, "latency_reserve_workers");
    if let Some(w) = workers
        && !(1..=16).contains(&w)
    {
        c.err(format!("`runtime.workers` must be 1..16, got {w}"));
    }
    if let (Some(w), Some(r)) = (workers, reserve)
        && (1..=16).contains(&w)
        && r >= w
    {
        c.err(format!(
            "`runtime.latency_reserve_workers` must be 0..{}, got {r}",
            w - 1
        ));
    }
    if c.errs.len() != before {
        return None;
    }
    Some(RuntimeProfile {
        decoded_cache_bytes: decoded?,
        physical_cache_bytes: physical?,
        workers: workers?,
        latency_reserve_workers: reserve?,
    })
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

/// Parses and validates a profile; every problem is a finding. Pattern files resolve against the
/// TOML's own folder and may not leave it.
pub fn load_profile(path: &Path) -> Result<Profile, Vec<String>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| vec![format!("profile {}: {e}", path.display())])?;
    let root: Table = text
        .parse()
        .map_err(|e| vec![format!("profile {}: {e}", path.display())])?;
    // Through a symlink, pattern files sit next to the real TOML.
    let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let dir = real
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut c = Ctx {
        dir: dir.to_path_buf(),
        errs: Vec::new(),
        ignored: Vec::new(),
    };
    let mut p = Profile {
        default_action: Action::Loose,
        default_block_shift: SHIFT_64K,
        rules: Vec::new(),
        runtime: None,
        auto_loose: AutoLoose::default(),
        ignored: Vec::new(),
    };
    for k in root.keys() {
        if !["pack", "rule", "runtime", "groups"].contains(&k.as_str()) {
            c.err(format!("unknown key `{k}`"));
        }
    }
    if let Some(v) = root.get("pack")
        && let Some(t) = c.table("pack", v)
    {
        c.keys("pack", t, PACK_KEYS, PACK_IGNORED);
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
    if let Some(v) = root.get("groups")
        && let Some(g) = c.table("groups", v)
    {
        for (name, v) in g {
            if let Some(t) = c.table(&format!("groups.{name}"), v) {
                c.keys(&format!("groups.{name}"), t, &[], GROUP_IGNORED);
            }
        }
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
    if let Some(v) = root.get("runtime") {
        p.runtime = parse_runtime(&mut c, v);
    }
    if !c.errs.is_empty() {
        return Err(c.errs);
    }
    p.ignored = c.ignored;
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_table() {
        let t = |p: &str, s: &str, want: bool| assert_eq!(glob_match(p, s), want, "{p} vs {s}");
        t("*", "a/b/c", true);
        t("**", "", true);
        t("a?c", "abc", true);
        t("a?c", "a/c", true);
        t("a?c", "ac", false);
        t("[abc]x", "bx", true);
        t("[a-c]x", "dx", false);
        t("[!a-c]x", "dx", true);
        t("[!a-c]x", "ax", false);
        t("[]]x", "]x", true);
        t("[!]]x", "ax", true);
        t("[z-a]x", "zx", false);
        // reversed ranges go first, then a lone `!` is any char (CPython fnmatch.translate)
        t("data/[z-a!].bin", "data/a.bin", true);
        t("data/[z-a!].bin", "data/!.bin", true);
        t("[!z-a]", "x", true);
        t("[!z-a]", "!", true);
        t("[z-a]", "z", false);
        t("[z-a]", "a", false);
        t("[b-a]x", "x", false);
        t("[a-c-e]", "d", false);
        t("[a-c-e]", "-", true);
        t("data/[aa-!-z].bin", "data/b.bin", false);
        t("data/[aa-!-z].bin", "data/-.bin", true);
        t("data/[aa-!-z].bin", "data/a.bin", true);
        t("data/[aa-!-z].bin", "data/z.bin", true);
        t("[a-cz-xq]", "q", true);
        t("[a-cz-xq]", "b", true);
        t("[a-cz-xq]", "z", false);
        t("[!a-cz-x]", "z", true);
        // unclosed bracket is a literal
        t("a[b", "a[b", true);
        t("a[b", "ab", false);
        t("[", "[", true);
        t("[]", "[]", true);
        t("[!]", "[!]", true);
        t("a[]b", "a[]b", true);
        // literal brackets via a set
        t("a[[]b", "a[b", true);
        // backtracking
        t("*a*b*c", "xaxbxbxc", true);
        t("*a*b*c", "xaxbxbx", false);
        t(
            "a*a*a*a*b",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaac",
            false,
        );
        t("*ab", "aab", true);
        t("*aab", "aaab", true);
        // root vs nested
        t("**/*.x", "a.x", false);
        t("**/*.x", "d/a.x", true);
        t("*.x", "d/a.x", true);
        t("**/*.x", "d/e/a.x", true);
        // case sensitive, whole string
        t("*.X", "a.x", false);
        t("a", "ab", false);
        // separators
        t("d\\*.x", "d/a.x", true);
        t("d/*.x", "d\\a.x", true);
        t("/d/*", "d/a", true);
        t("d/*", "/d/a", true);
        // non-ASCII is compared per char
        t("é?", "éé", true);
    }

    #[test]
    fn size_table() {
        let ok = |s: &str, n: u64| assert_eq!(parse_size(s), Ok(n), "{s}");
        ok("0", 0);
        ok("123", 123);
        ok("64KiB", 65536);
        ok(" 64 kib".trim(), 65536);
        ok("1_024", 1024);
        ok("1K", 1000);
        ok("1kb", 1000);
        ok("2MiB", 2 << 20);
        ok("3M", 3_000_000);
        ok("1GiB", 1 << 30);
        ok("1GB", 1_000_000_000);
        ok("1TiB", 1 << 40);
        ok("1T", 1_000_000_000_000);
        ok("5B", 5);
        ok("1.5KiB", 1536);
        ok("0.9", 0);
        ok(".5K", 500);
        ok("1.0001KB", 1000);
        ok("18446744073709551615", u64::MAX);
        for bad in [
            "",
            "-1",
            "-1K",
            "KiB",
            "1XB",
            "1 K B",
            "1e3",
            "nan",
            "inf",
            "1.2.3",
            ".",
            "18446744073709551616",
            "17179869184GiB",
            "99999999999999999999999999999999999999999",
        ] {
            assert!(parse_size(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn always_loose_table() {
        for p in [
            "eboot.bin",
            "a/b/eboot.bin",
            "x.ELF",
            "d/y.Self",
            "m.prx",
            "d/m.SPRX",
            "sce_sys/param.json",
            "sce_module/a.bin",
            "fakelib/libSceAmpr.sprx",
            "fakelib2/a.dat",
            "ampr_assets.index",
            "ampr_assets.index.crc",
            "ampr_assets.index.runtime",
            "ampr_emu.index",
            "ampr_commands.bin",
            "ampr_emu.log",
            "apr_emu.log",
        ] {
            assert!(always_loose(p), "{p}");
        }
        for p in [
            "fakelib",
            "fakelibs.txt",
            "d/sce_sys/a",
            "data/eboot.bin.bak",
            "ampr_assets-000.pak",
            "d/ampr_emu.log",
        ] {
            assert!(!always_loose(p), "{p}");
        }
    }

    fn spec(shift: u8, store: bool, hot: bool, random: bool) -> Option<PackSpec> {
        Some(PackSpec {
            block_shift: shift,
            store,
            hot,
            random,
        })
    }

    #[test]
    fn fallback_and_observed() {
        let f = |p: &str| select(p, 10, &Selection::Fallback);
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
        assert!(
            !keep_loose("root.bin"),
            "the root rule is the fallback's only"
        );
        assert_eq!(select("data/a.bin", 0, &Selection::Fallback), None, "empty");
        let set: BTreeSet<String> = ["data/a.bin", "data/x.png", "sce_sys/b.bin", "root.bin"]
            .map(String::from)
            .into();
        let o = |p: &str| select(p, 10, &Selection::Observed(&set));
        assert_eq!(o("data/a.bin"), spec(16, false, false, false));
        assert_eq!(o("data/x.png"), None, "traces only narrow the fallback");
        assert_eq!(o("root.bin"), None, "root files stay loose");
        assert_eq!(o("sce_sys/b.bin"), None, "safety beats traces");
        assert_eq!(o("data/other.bin"), None);
    }

    #[test]
    fn observed_stellar_blade_packs_only_ucas() {
        let paths = [
            "contentids.json",
            "engine/saved/config/ps5/manifest.ini",
            "sb/content/paks/global.ucas",
            "sb/content/paks/global.utoc",
            "sb/content/paks/pakchunk0-ps5.pak",
            "sb/content/paks/pakchunk0-ps5.ucas",
            "sb/content/paks/pakchunk0-ps5.utoc",
            "sb/content/paks/pakchunk1-ps5.ucas",
            "sb/saved/config/ps5/engine.ini",
        ];
        let set: BTreeSet<String> = paths.map(String::from).into();
        let packed: Vec<&str> = paths
            .into_iter()
            .filter(|p| select(p, 1000, &Selection::Observed(&set)).is_some())
            .collect();
        assert_eq!(
            packed,
            [
                "sb/content/paks/global.ucas",
                "sb/content/paks/pakchunk0-ps5.ucas",
                "sb/content/paks/pakchunk1-ps5.ucas"
            ]
        );
    }

    // ---- profile fixtures

    struct Dir(std::path::PathBuf);
    impl Dir {
        fn new(tag: &str) -> Dir {
            let d = std::env::temp_dir().join(format!("forge-rules-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            Dir(d)
        }
        fn write(&self, name: &str, body: &str) -> std::path::PathBuf {
            let p = self.0.join(name);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, body).unwrap();
            p
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn load(tag: &str, toml: &str) -> Result<Profile, Vec<String>> {
        let d = Dir::new(tag);
        load_profile(&d.write("p.toml", toml))
    }

    fn errs(tag: &str, toml: &str) -> String {
        load(tag, toml).unwrap_err().join("\n")
    }

    #[test]
    fn last_match_wins_and_defaults() {
        let p = load(
            "last",
            r#"
[pack]
default_action = "Compress"
default_block_size = "128KiB"
[[rule]]
include = "data/**"
action = "store"
[[rule]]
include = ["data/**"]
exclude = "**/*.raw"
block_size = 32768
hot = true
[[rule]]
include = "data/keep/**"
action = "loose"
"#,
        )
        .unwrap();
        let s = |path: &str| select(path, 5, &Selection::Profile(&p));
        assert_eq!(
            s("other/a.bin"),
            spec(17, false, false, false),
            "pack default"
        );
        assert_eq!(
            s("data/a.bin"),
            spec(15, false, true, true),
            "last matching rule, auto+hot is random"
        );
        assert_eq!(
            s("data/a.raw"),
            spec(17, true, false, false).map(|mut x| {
                x.block_shift = 17;
                x
            }),
            "store rule, default block"
        );
        assert_eq!(s("data/keep/a.bin"), None);
        assert_eq!(s("sce_sys/a.bin"), None);
        assert_eq!(select("data/a.bin", 0, &Selection::Profile(&p)), None);
    }

    #[test]
    fn profiles_respect_the_keep_loose_list() {
        let p = load("keep", "[pack]\ndefault_action = \"compress\"\n").unwrap();
        let s = |path: &str| select(path, 5, &Selection::Profile(&p));
        for path in [
            "sb/content/paks/a.pak",
            "sb/content/paks/a.utoc",
            "sb/config/x.ini",
            "d/a.json",
            "sb/SB.uproject",
            "d/system/a.bin",
            "d/save/a.bin",
            "decrypted/a.bin",
            "_DUPLEX_/a.bin",
        ] {
            assert_eq!(s(path), None, "{path}");
            assert!(
                profile_spec(path, &p).is_some(),
                "{path}: the profile alone packs it"
            );
        }
        assert_eq!(s("sb/content/paks/a.ucas"), spec(16, false, false, false));
        assert_eq!(
            s("data.0"),
            spec(16, false, false, false),
            "root files: the profile's call"
        );
        assert_eq!(s("d/decrypted/a.bin"), spec(16, false, false, false));
        assert_eq!(s("eboot.bin"), None);
    }

    #[test]
    fn auto_loose_keys() {
        assert_eq!(load("al0", "").unwrap().auto_loose, AutoLoose::default());
        let p = load(
            "al1",
            r#"
[pack]
auto_loose_large_files = false
auto_loose_hot_files = true
auto_loose_min_file_size = "1MiB"
auto_loose_sample_blocks = 4
auto_loose_sample_bytes = 131072
auto_loose_min_savings_ratio = 0.1
auto_loose_max_raw_ratio = 1
"#,
        )
        .unwrap();
        let want = AutoLoose {
            enabled: false,
            hot: true,
            min_file_size: 1 << 20,
            sample_blocks: 4,
            sample_bytes: 131072,
            min_savings_ratio: 0.1,
            max_raw_ratio: 1.0,
        };
        assert_eq!(p.auto_loose, want);
        assert!(
            p.ignored.is_empty(),
            "honored, not ignored: {:?}",
            p.ignored
        );
        for (tag, bad, key) in [
            (
                "al2",
                "auto_loose_large_files = 1",
                "auto_loose_large_files",
            ),
            (
                "al3",
                "auto_loose_hot_files = \"no\"",
                "auto_loose_hot_files",
            ),
            (
                "al4",
                "auto_loose_min_file_size = -1",
                "auto_loose_min_file_size",
            ),
            (
                "al5",
                "auto_loose_min_file_size = \"1QB\"",
                "auto_loose_min_file_size",
            ),
            (
                "al6",
                "auto_loose_sample_blocks = 0",
                "auto_loose_sample_blocks",
            ),
            (
                "al7",
                "auto_loose_sample_blocks = 4097",
                "auto_loose_sample_blocks",
            ),
            (
                "al8",
                "auto_loose_sample_blocks = 1.5",
                "auto_loose_sample_blocks",
            ),
            (
                "al9",
                "auto_loose_sample_bytes = 0",
                "auto_loose_sample_bytes",
            ),
            (
                "al10",
                "auto_loose_min_savings_ratio = 1.0",
                "auto_loose_min_savings_ratio",
            ),
            (
                "al11",
                "auto_loose_min_savings_ratio = -0.1",
                "auto_loose_min_savings_ratio",
            ),
            (
                "al12",
                "auto_loose_max_raw_ratio = 1.5",
                "auto_loose_max_raw_ratio",
            ),
            (
                "al13",
                "auto_loose_max_raw_ratio = \"x\"",
                "auto_loose_max_raw_ratio",
            ),
        ] {
            let e = errs(tag, &format!("[pack]\n{bad}\n"));
            assert!(e.contains(&format!("pack.{key}")), "{bad}: {e}");
        }
        let ok = "[pack]\nauto_loose_min_savings_ratio = 0\nauto_loose_max_raw_ratio = 0\n";
        assert!(load("al14", ok).is_ok());
    }

    #[test]
    fn auto_loose_sampling_and_decision() {
        let a = AutoLoose::default();
        let plain = PackSpec {
            block_shift: 16,
            store: false,
            hot: false,
            random: false,
        };
        let big = 64 << 20;
        assert!(a.applies(big, &plain));
        assert!(!a.applies(big - 1, &plain), "under the minimum");
        assert!(
            !a.applies(
                big,
                &PackSpec {
                    store: true,
                    ..plain
                }
            ),
            "store is not sampled"
        );
        assert!(
            !a.applies(big, &PackSpec { hot: true, ..plain }),
            "hot exempt by default"
        );
        let hot = AutoLoose { hot: true, ..a };
        assert!(hot.applies(big, &PackSpec { hot: true, ..plain }));
        assert!(
            !AutoLoose {
                enabled: false,
                ..a
            }
            .applies(big, &plain)
        );
        // 1024 blocks of 64 KiB: 32 picks from the first to the last, fixed.
        let ix = a.sample_indices(big, 16);
        assert_eq!(ix.len(), 32);
        assert_eq!((ix[0], ix[1], ix[31]), (0, 33, 1023));
        assert_eq!(ix, a.sample_indices(big, 16), "deterministic");
        // 16 MiB / 1 MiB blocks caps it at 16; fewer blocks than picks: all of them.
        assert_eq!(a.sample_indices(big, 20).len(), 16);
        assert_eq!(a.sample_indices(3 * 65536 + 1, 16), [0, 1, 2, 3]);
        let one = AutoLoose {
            sample_blocks: 1,
            ..a
        };
        assert_eq!(one.sample_indices(big, 16), [512]);
        let s = |stored: u64, raw_blocks: u64| Sample {
            raw_bytes: 1000,
            stored_bytes: stored,
            blocks: 10,
            raw_blocks,
        };
        assert!(a.keeps_loose(&s(960, 0)), "saves 4%");
        assert!(!a.keeps_loose(&s(950, 0)), "saves 5%");
        assert!(a.keeps_loose(&s(500, 9)), "90% RAW");
        assert!(!a.keeps_loose(&s(500, 8)));
    }

    #[test]
    fn default_action_is_loose() {
        let p = load("defl", "[[rule]]\ninclude = \"a/*\"\n").unwrap();
        assert_eq!(select("b/x", 1, &Selection::Profile(&p)), None);
        assert_eq!(
            select("a/x", 1, &Selection::Profile(&p)),
            spec(16, false, false, false)
        );
    }

    #[test]
    fn layouts() {
        let p = load(
            "lay",
            r#"
[[rule]]
include = "r/**"
layout = "random"
[[rule]]
include = "m/**"
layout = "Mixed"
hot = true
[[rule]]
include = "s/**"
layout = "streaming"
hot = true
streaming = true
[[rule]]
include = "h/**"
hot = true
"#,
        )
        .unwrap();
        let s = |path: &str| select(path, 5, &Selection::Profile(&p));
        assert_eq!(s("r/a"), spec(16, false, false, true));
        assert_eq!(s("m/a"), spec(16, false, true, false));
        assert_eq!(s("s/a"), spec(16, false, true, false));
        assert_eq!(s("h/a"), spec(16, false, true, true));
        assert!(p.ignored.iter().any(|i| i.contains("streaming")));
        assert!(p.ignored.contains(&"rule.streaming".to_string()));
    }

    #[test]
    fn pattern_files() {
        let d = Dir::new("pf");
        d.write("inc.txt", "# comment\n\n  data/*.bin  \n#data/no\nsub/**\n");
        d.write("sub/exc.txt", "**/skip*\n");
        let p = d.write(
            "p.toml",
            r#"
[pack]
default_action = "compress"
[[rule]]
exclude = "q/*"
[[rule]]
include_from = "inc.txt"
exclude_from = "sub/exc.txt"
action = "store"
[[rule]]
include = "z/*"
include_from = "inc.txt"
action = "loose"
"#,
        );
        let p = load_profile(&p).unwrap();
        assert_eq!(
            p.rules[1].include,
            ["data/*.bin", "sub/**"],
            "no default ** with include_from"
        );
        assert_eq!(p.rules[2].include, ["z/*", "data/*.bin", "sub/**"]);
        assert_eq!(p.rules[0].include, ["**"]);
        assert_eq!(p.rules[1].exclude, ["**/skip*"]);
        let s = |path: &str| select(path, 5, &Selection::Profile(&p));
        assert_eq!(s("data/a.bin"), None, "the loose rule matches last");
        assert_eq!(s("sub/skipme"), None);
        assert_eq!(s("q/a"), spec(16, false, false, false));
        assert_eq!(s("w/a"), spec(16, false, false, false));
    }

    #[test]
    #[cfg(unix)]
    fn pattern_file_escape() {
        let d = Dir::new("esc");
        let outside = Dir::new("esc-out");
        outside.write("o.txt", "x\n");
        d.write("in.txt", "x\n");
        let abs = outside.0.join("o.txt");
        std::os::unix::fs::symlink(&abs, d.0.join("link.txt")).unwrap();
        std::os::unix::fs::symlink(&outside.0, d.0.join("linkdir")).unwrap();
        for (i, name) in [
            "../forge-rules-esc-out-x/o.txt",
            "link.txt",
            "linkdir/o.txt",
            abs.to_str().unwrap(),
            "missing.txt",
            "/etc/hosts",
            "..\\x",
        ]
        .iter()
        .enumerate()
        {
            let body = format!(
                "[[rule]]\ninclude_from = \"{}\"\n",
                name.replace('\\', "\\\\")
            );
            let p = d.write(&format!("p{i}.toml"), &body);
            assert!(load_profile(&p).is_err(), "{name}");
        }
        let up = format!(
            "../{}/o.txt",
            outside.0.file_name().unwrap().to_str().unwrap()
        );
        let p = d.write("up.toml", &format!("[[rule]]\nexclude_from = \"{up}\"\n"));
        let e = load_profile(&p).unwrap_err().join("\n");
        assert!(e.contains("leaves"), "{e}");
        let p = d.write("ok.toml", "[[rule]]\ninclude_from = \"./in.txt\"\n");
        assert!(load_profile(&p).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn pattern_files_resolve_next_to_the_real_toml() {
        let d = Dir::new("symlink");
        d.write("game/inc.txt", "*.bin\n");
        d.write("game/p.toml", "[[rule]]\ninclude_from = \"inc.txt\"\n");
        std::os::unix::fs::symlink(d.0.join("game/p.toml"), d.0.join("link.toml")).unwrap();
        // `inc.txt` is not beside the link, only beside the real file.
        assert!(load_profile(&d.0.join("link.toml")).is_ok());
    }

    #[test]
    fn unknown_vs_ignored_keys() {
        let p = load(
            "ign",
            r#"
[pack]
deduplicate = true
workers = 3
[groups.main]
pack_count = 2
io_page_size = 4096
[[rule]]
group = "main"
force_pack = true
compression_level = 3
"#,
        )
        .unwrap();
        for k in [
            "pack.deduplicate",
            "pack.workers",
            "groups.main.pack_count",
            "groups.main.io_page_size",
            "rule.group",
            "rule.force_pack",
        ] {
            assert!(p.ignored.contains(&k.to_string()), "{k}: {:?}", p.ignored);
        }
        assert!(errs("u1", "bogus = 1").contains("unknown key `bogus`"));
        assert!(errs("u2", "[pack]\ncompresion_level = 1").contains("`pack.compresion_level`"));
        assert!(errs("u3", "[[rule]]\nincluds = \"a\"").contains("`rule.includs`"));
        assert!(errs("u4", "[runtime]\nworkers = 1\nwrkers = 2").contains("`runtime.wrkers`"));
        assert!(
            errs("u5", "[groups.g]\npack_count = 1\nnonsense = 2").contains("`groups.g.nonsense`")
        );
        assert!(errs("u6", "[groups]\ng = 1").contains("`groups.g`"));
        assert!(errs("u7", "rule = 1").contains("`rule`"));
    }

    #[test]
    fn bad_values() {
        assert!(errs("b1", "[pack]\ndefault_action = \"zip\"").contains("default_action"));
        assert!(errs("b2", "[pack]\ndefault_action = 1").contains("default_action"));
        assert!(errs("b3", "[pack]\ndefault_block_size = 12288").contains("power of two"));
        assert!(errs("b4", "[pack]\ndefault_block_size = 8192").contains("16 KiB"));
        assert!(errs("b5", "[pack]\ndefault_block_size = \"2MiB\"").contains("1 MiB"));
        assert!(errs("b6", "[pack]\ndefault_block_size = \"1XB\"").contains("unit"));
        assert!(errs("b7", "[pack]\nindex_name = \"x.index\"").contains("index_name"));
        assert!(errs("b8", "[pack]\nio_page_size = \"4KiB\"").contains("io_page_size"));
        assert!(errs("b9", "[[rule]]\nlayout = \"zigzag\"").contains("layout"));
        assert!(errs("b10", "[[rule]]\naction = \"x\"").contains("action"));
        assert!(errs("b11", "[[rule]]\nhot = \"yes\"").contains("hot"));
        assert!(errs("b12", "[[rule]]\ninclude = 5").contains("include"));
        assert!(errs("b13", "[[rule]]\ninclude = []").contains("no include"));
        assert!(errs("b14", "[[rule]]\nblock_size = 4096").contains("block_size"));
        assert!(errs("b15", "not toml [").contains("profile"));
        assert!(load_profile(Path::new("/nonexistent/p.toml")).is_err());
        for ok in ["\"16KiB\"", "65536", "\"1MiB\"", "\"256 KiB\""] {
            let t = format!("[pack]\ndefault_block_size = {ok}");
            assert!(load("b16", &t).is_ok(), "{ok}");
        }
    }

    #[test]
    fn runtime_table() {
        let ok = "[runtime]\ndecoded_cache_bytes = \"128MiB\"\nphysical_cache_bytes = 32768\nworkers = 4\nlatency_reserve_workers = 1\n";
        let rt = load("r0", ok).unwrap().runtime.unwrap();
        assert_eq!(
            rt,
            RuntimeProfile {
                decoded_cache_bytes: 128 << 20,
                physical_cache_bytes: 32768,
                workers: 4,
                latency_reserve_workers: 1
            }
        );
        assert!(load("r1", "").unwrap().runtime.is_none());
        let with = |from: &str, to: &str| errs("r2", &ok.replace(from, to));
        assert!(with("workers = 4", "workers = 0").contains("1..16"));
        assert!(with("workers = 4", "workers = 17").contains("1..16"));
        assert!(with("workers = 4", "workers = true").contains("integer"));
        assert!(with("workers = 4", "workers = 4.0").contains("integer"));
        assert!(with("reserve_workers = 1", "reserve_workers = 4").contains("0..3"));
        assert!(with("reserve_workers = 1", "reserve_workers = -1").contains("integer"));
        assert!(with("32768", "20000").contains("16384"));
        assert!(with("\"128MiB\"", "false").contains("decoded_cache_bytes"));
        assert!(with("\"128MiB\"", "\"1XB\"").contains("unit"));
        assert!(
            errs("r3", "[runtime]\nworkers = 2\n")
                .contains("`runtime.decoded_cache_bytes` is required")
        );
        assert!(load("r4", &ok.replace("32768", "0")).is_ok());
    }
}
