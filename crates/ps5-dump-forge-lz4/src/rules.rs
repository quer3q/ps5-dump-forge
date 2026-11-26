//! The packing strategy (zstd and LZ4 Pack): built-in, or a TOML profile (upstream `ampr_pack.py`'s
//! format) with LZ4 auto-loose. Globs are Python's `fnmatchcase` (`*` crosses `/`).

use std::collections::HashMap;

use toml::{Table, Value};

use crate::PackSpec;

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
    out.shrink_to_fit(); // held for the whole job, one per pattern
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

/// A bracket body as CPython's `fnmatch.translate` reads it (reversed ranges dropped, empty class
/// never matches).
fn bracket(body: &[char]) -> Bracket {
    // (char, is a literal): only hyphens joining chunks stay range operators, as in CPython.
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

/// Python `fnmatch.fnmatchcase` over normalized paths; O(len(path) * len(pattern)).
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

/// A size string as upstream's `parse_size`: `_` removed, fractional prefix truncated,
/// case-insensitive unit.
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

/// Paths never packed (console- or runtime-loaded, and `*.ini`); compared ASCII case-insensitively
/// since images fold case.
pub fn always_loose(path: &str) -> bool {
    let file = path.rsplit('/').next().unwrap_or(path);
    let first = path.split('/').next().unwrap_or("").to_ascii_lowercase();
    let in_dir = path.contains('/');
    file.eq_ignore_ascii_case("eboot.bin")
        || has_suffix(path, &[".elf", ".self", ".prx", ".sprx", ".ini"])
        || (in_dir && (first == "sce_sys" || first == "sce_module" || first.starts_with("fakelib")))
        || artifact(path)
}

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
    rules: Vec<Rule>,
    pub auto_loose: AutoLoose,
    /// Keys with no effect here, as `scope.key`, for the job log.
    pub ignored: Vec<String>,
    compiled: Compiled,
}

impl Profile {
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }
}

/// `Profile::rules` compiled once: exact-path patterns are map lookups; picks the same rule as a
/// reverse scan.
#[derive(Clone, Default)]
struct Compiled {
    globs: Vec<Vec<Vec<Tok>>>,
    excludes: Vec<Vec<Vec<Tok>>>,
    exact_excludes: HashMap<String, Vec<usize>>,
    globbed: Vec<usize>,
    exact: HashMap<String, Vec<usize>>,
}

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

/// The one path a pattern matches when every token is one fixed char, else `None`.
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
            let mut excludes = Vec::new();
            for p in &r.exclude {
                let toks = compile(&norm(p));
                match literal(&toks) {
                    Some(path) => c.exact_excludes.entry(path).or_default().push(i),
                    None => excludes.push(toks),
                }
            }
            c.excludes.push(excludes);
        }
        c
    }

    fn last_match(&self, path: &str) -> Option<usize> {
        let s = norm(path);
        let hit = |ps: &[Vec<Tok>]| ps.iter().any(|t| glob_match_compiled(t, &s));
        let key: String = s.iter().collect();
        let out = self.exact_excludes.get(&key);
        let excluded = |r: usize| hit(&self.excludes[r]) || out.is_some_and(|rs| rs.contains(&r));
        let exact = self
            .exact
            .get(&key)
            .and_then(|rs| rs.iter().rev().copied().find(|&r| !excluded(r)));
        // A later rule can only beat it through a glob include.
        self.globbed
            .iter()
            .rev()
            .copied()
            .take_while(|&r| exact.is_none_or(|e| r > e))
            .find(|&r| hit(&self.globs[r]) && !excluded(r))
            .or(exact)
    }
}
/// The packing strategy: `None` keeps the file loose. Empty and `always_loose` files first; else 64
/// KiB blocks, or the profile's rules.
pub fn select_everything(path: &str, size: u64, profile: Option<&Profile>) -> Option<PackSpec> {
    if size == 0 || always_loose(path) {
        return None;
    }
    match profile {
        None => Some(plain_spec()),
        // A profile is the user's call: only `always_loose` (system and runtime files) wins.
        Some(p) => profile_spec(path, p),
    }
}

/// What `p`'s rules choose for a non-empty, not `always_loose` file: last match, else the default.
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

/// Upstream's auto-loose (`[pack] auto_loose_*`): a large compressed file whose samples barely
/// shrink stays loose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoLoose {
    pub enabled: bool,
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
    /// Whether a `size`-byte file per `spec` is sampled: compressed, not hot unless `hot`, big
    /// enough.
    pub fn applies(&self, size: u64, spec: &PackSpec) -> bool {
        self.enabled
            && !spec.store
            && (!spec.hot || self.hot)
            && size > 0
            && size >= self.min_file_size
    }

    /// The block indices to sample, sorted, spread evenly first to last (deterministic).
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

    /// Whether `s` keeps the file loose: savings under `min_savings_ratio` or RAW share at least
    /// `max_raw_ratio`.
    pub fn keeps_loose(&self, s: &Sample) -> bool {
        s.savings() < self.min_savings_ratio || s.raw_ratio() >= self.max_raw_ratio
    }
}

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
    "include_from",
    "exclude_from",
    "block_size",
    "layout",
    "hot",
];
const DEFAULT_INDEX_NAME: &str = "ampr_assets.index";
const IO_PAGE: u64 = 65536;

/// Patterns all of a profile's pattern files may hold (exact paths are map lookups).
const MAX_FILE_PATTERNS: usize = 200_000;
/// Of those, the ones with a wildcard: each is tried against every file, bounding memory and time.
// ponytail: 10,000 globs; a pattern index (by literal prefix) would allow more.
const MAX_FILE_GLOBS: usize = 10_000;

struct Ctx {
    errs: Vec<String>,
    ignored: Vec<String>,
    from_files: usize,
    globs_from_files: usize,
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

    /// Unused keys (upstream's tuning, typos) go to the log list, not refused.
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

type ReadPatterns<'a> = dyn FnMut(&str) -> Result<String, String> + 'a;

/// A rule's `include_from` / `exclude_from` patterns: relative to the profile's folder, one per
/// line, `#` comments skipped.
fn pattern_files(c: &mut Ctx, t: &Table, key: &str, read: &mut ReadPatterns) -> Vec<String> {
    let Some(v) = t.get(key) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for name in c.strings(&format!("rule.{key}"), v).unwrap_or_default() {
        let inside = !name.is_empty()
            && !name.contains(['\\', ':'])
            && !name.starts_with('/')
            && name.split('/').all(|p| !["", ".", ".."].contains(&p));
        if !inside {
            c.err(format!(
                "`rule.{key}` entry {name:?} must be a path inside the profile's folder \
                 (`/`-separated, no `.`, `..`, `\\` or `:`)"
            ));
            continue;
        }
        if c.from_files > MAX_FILE_PATTERNS {
            return out;
        }
        match read(&name) {
            Err(e) => c.err(format!("`rule.{key}` file {name}: {e}")),
            Ok(text) => {
                // Python's `splitlines`, as upstream: a lone `\r` ends a line too.
                let lines = text.split(|ch| {
                    matches!(
                        ch,
                        '\n' | '\r' | '\x0b' | '\x0c' | '\x1c'
                            ..='\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}'
                    )
                });
                for l in lines.map(str::trim) {
                    if l.is_empty() || l.starts_with('#') {
                        continue;
                    }
                    let glob = literal(&compile(&norm(l))).is_none();
                    let over = if c.from_files == MAX_FILE_PATTERNS {
                        Some(format!("more than {MAX_FILE_PATTERNS} patterns"))
                    } else if glob && c.globs_from_files == MAX_FILE_GLOBS {
                        Some(format!(
                            "more than {MAX_FILE_GLOBS} patterns with a wildcard"
                        ))
                    } else {
                        None
                    };
                    if let Some(over) = over {
                        c.err(format!("pattern files: {over} in all"));
                        c.from_files = MAX_FILE_PATTERNS + 1; // past a cap: no file is read again
                        return out;
                    }
                    c.from_files += 1;
                    c.globs_from_files += usize::from(glob);
                    out.push(l.to_owned());
                }
            }
        }
    }
    out
}

fn parse_rule(
    c: &mut Ctx,
    n: usize,
    v: &Value,
    default_shift: u8,
    read: &mut ReadPatterns,
) -> Option<Rule> {
    let t = c.table(&format!("rule[{n}]"), v)?;
    c.keys("rule", t, RULE_KEYS);
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
    let list = |c: &mut Ctx, key: &str| match t.get(key) {
        Some(v) => c.strings(&format!("rule.{key}"), v).unwrap_or_default(),
        None => Vec::new(),
    };
    let include_from = pattern_files(c, t, "include_from", read);
    let exclude_from = pattern_files(c, t, "exclude_from", read);
    r.include = match (t.contains_key("include"), include_from.is_empty()) {
        (true, _) => list(c, "include"),
        // upstream: the default `**` only when the pattern files gave no pattern
        (false, true) => vec!["**".into()],
        (false, false) => Vec::new(),
    };
    r.exclude = list(c, "exclude");
    r.include.extend(include_from);
    r.exclude.extend(exclude_from);
    if r.include.is_empty() && c.errs.len() == before {
        c.err(format!("rule[{n}] has no include patterns"));
    }
    (c.errs.len() == before).then_some(r)
}

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

/// Parses and validates a profile's text; every problem a finding, unused keys in
/// [`Profile::ignored`]. `read` loads a checked relative pattern file.
pub fn parse_profile(
    text: &str,
    read: &mut dyn FnMut(&str) -> Result<String, String>,
) -> Result<Profile, Vec<String>> {
    let root: Table = text.parse().map_err(|e| vec![format!("{e}")])?;
    let mut c = Ctx {
        errs: Vec::new(),
        ignored: Vec::new(),
        from_files: 0,
        globs_from_files: 0,
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
                    .extend(parse_rule(&mut c, n, v, p.default_block_shift, read));
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
    fn everything_but_system_files() {
        let f = |p: &str| select_everything(p, 10, None);
        for p in [
            "data/a.bin",
            "root.bin",
            "d/a.bik",
            "a/Cutscenes/c.bin",
            "data/a.json",
        ] {
            assert_eq!(f(p), spec(16, false, false, false), "{p}");
        }
        for p in [
            "eboot.bin",
            "data/eboot.bin",
            "SCE_SYS/a.bin",
            "sce_module/x.prx",
            "fakelib_x/a.bin",
            "fakelib/libSceAmpr.sprx",
            "ampr_emu.index",
            "d/dlc_emu.INI",
        ] {
            assert_eq!(f(p), None, "{p}");
        }
        assert_eq!(select_everything("data/a.bin", 0, None), None, "empty");
    }

    /// A Lazy_AMPR community profile: `[groups]`, tuning keys and a typo load as ignored keys.
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

    fn no_files(name: &str) -> Result<String, String> {
        Err(format!("no file {name}"))
    }

    #[test]
    fn community_profile_selects_and_broken_ones_are_findings() {
        let p = parse_profile(ASTROBOT, &mut no_files).unwrap();
        assert!(!p.auto_loose.enabled);
        for k in ["groups.*", "rule.ayout"] {
            assert!(p.ignored.iter().any(|i| i == k), "{k}: {:?}", p.ignored);
        }
        let f = |path: &str| select_everything(path, 10, Some(&p));
        assert_eq!(f("data/prein/sound/a.bin"), spec(14, false, true, true));
        assert_eq!(
            f("data/prein/video/a.bin"),
            None,
            "the last matching rule wins"
        );
        assert_eq!(
            f("data/prein/t/a.gnfp"),
            spec(17, false, false, false),
            "a profile decides the packing"
        );
        assert_eq!(f("sce_sys/icon0.png"), None, "always_loose still wins");
        assert_eq!(f("data/prein/DLC_EMU.INI"), None, "an .ini stays editable");
        assert_eq!(f("data/other.bin"), None, "default_action = loose");
        for bad in [
            "[pack\n",
            "[[rule]]\naction = \"pack\"\n",
            "[pack]\nindex_name = \"other.index\"\n",
        ] {
            assert!(parse_profile(bad, &mut no_files).is_err(), "{bad}");
        }
    }

    #[test]
    fn pattern_files_extend_rules_and_bad_names_are_findings() {
        let files = std::collections::HashMap::from([
            (
                "lists/pack.txt",
                "# packed\r\n\r\n  data/a/**  \r\ndata/b.bin\r\n",
            ),
            // a lone `\r` ends a line, as in upstream's `splitlines`
            ("skip.txt", "data/a/skip.bin\rdata/a/skip2.bin"),
            ("none.txt", "# nothing\n\n"),
        ]);
        let mut read = |name: &str| {
            files
                .get(name)
                .map(|t| t.to_string())
                .ok_or_else(|| "missing".to_string())
        };
        let text = "[pack]\ndefault_action = \"loose\"\n[[rule]]\naction = \"compress\"\n\
                    include_from = \"lists/pack.txt\"\nexclude_from = [\"skip.txt\"]\n";
        let p = parse_profile(text, &mut read).unwrap();
        assert!(p.ignored.is_empty(), "{:?}", p.ignored);
        let f = |path: &str| select_everything(path, 10, Some(&p));
        assert_eq!(f("data/a/x.bin"), spec(16, false, false, false));
        assert_eq!(f("data/b.bin"), spec(16, false, false, false));
        assert_eq!(f("data/c.bin"), None, "no default `**` with include_from");
        assert_eq!(f("data/a/skip.bin"), None, "exclude_from");
        assert_eq!(
            f("data/a/skip2.bin"),
            None,
            "a `\\r`-separated exact exclude"
        );
        let p = parse_profile("[[rule]]\ninclude_from = \"none.txt\"\n", &mut read).unwrap();
        assert_eq!(
            select_everything("data/c.bin", 10, Some(&p)),
            spec(16, false, false, false),
            "no pattern in the files: the default `**`, as upstream"
        );
        let capped = |lines: String, why: &str| {
            let errs = parse_profile("[[rule]]\ninclude_from = \"a\"\n", &mut |_| {
                Ok(lines.clone())
            })
            .unwrap_err();
            assert!(errs.iter().any(|e| e.contains(why)), "{why}: {errs:?}");
        };
        capped(
            (0..=MAX_FILE_PATTERNS)
                .map(|i| format!("d/{i}\n"))
                .collect(),
            "more than 200000 patterns in all",
        );
        capped(
            (0..=MAX_FILE_GLOBS).map(|i| format!("d/{i}*\n")).collect(),
            "more than 10000 patterns with a wildcard",
        );
        for (bad, why) in [
            ("/abs", "must be a path inside the profile's folder"),
            ("../x", "must be a path inside the profile's folder"),
            ("a/./b", "must be a path inside the profile's folder"),
            ("a\\\\b", "must be a path inside the profile's folder"),
            ("C:x", "must be a path inside the profile's folder"),
            ("gone.txt", "`rule.include_from` file gone.txt: missing"),
        ] {
            let text = format!("[[rule]]\ninclude_from = \"{bad}\"\n");
            let errs = parse_profile(&text, &mut read).unwrap_err();
            assert!(errs.iter().any(|e| e.contains(why)), "{bad}: {errs:?}");
        }
    }
}
