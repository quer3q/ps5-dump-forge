//! Metadata recovery: a game's plaintext `sce_sys` metadata from Sony's own copy of the same version.
//!
//! `versionFileUri` → version XML → the manifest for `contentVersion` → its one `_sc.pkg` piece (a `\x7FCNT` container), size and SHA-256 checked; or a given URL of any of the three.
//!
//! Taken: the [`RESTORED`] records that differ from the game's; protected records and the rest left alone. `param.json` must name the game's ids and version and keeps [`RUNTIME_KEYS`] unless `original_runtime`.
//!
//! Two uses: a build's [`overlay`] (served from memory, any failure a warning) and Edit's [`run`] (an edit job whose adds come from a [`Temp`] folder; any failure fails it).

use std::path::PathBuf;

use anyhow::{Context, bail};
use sha2::{Digest, Sha256};

use crate::extract_paths::preflight_failed;
use crate::jobs::Ctx;
use ps5upload_fpkg::source::SourceTree;

use crate::{ConvertRequest, JobReport, MetadataRecovery, VerifyMode, VerifySummary};

const PARAM_JSON: &str = "sce_sys/param.json";

/// The recovery's own stage; the edit's follow it.
pub(crate) const STAGES: [&str; 1] = ["download"];
/// A version XML or a manifest is a few KiB.
const MAX_DOC: u64 = 4 << 20;
/// The metadata piece (70 MB measured), held whole while records are copied out: twice this fits a job's 512 MiB.
const MAX_PIECE: u64 = 160 << 20;
/// `sce_sys` files taken from the piece.
const RESTORED: &[&str] = &[
    "param.json",
    "icon0.png",
    "icon0.dds",
    "pic0.png",
    "pic0.dds",
    "pic1.png",
    "pic1.dds",
    "pic2.png",
    "pic2.dds",
    "snd0.at9",
    "save_data.png",
    "shareparam.json",
    "playgo-chunk.dat",
    "playgo-hash-table.dat",
    "playgo-ficm.dat",
    "playgo-scenario.json",
];
/// `param.json` values the dump keeps unless asked otherwise.
const RUNTIME_KEYS: [&str; 3] = [
    "sdkVersion",
    "requiredSystemSoftwareVersion",
    "applicationDrmType",
];

/// The dump's own `param.json`, as read before anything is downloaded.
struct Local {
    json: serde_json::Map<String, serde_json::Value>,
}

impl Local {
    fn text(&self, key: &str) -> &str {
        self.json
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
    }
}

/// What the request itself can't be.
fn request_findings(req: &ConvertRequest) -> Vec<String> {
    let mut findings = Vec::new();
    if !req.extract.is_empty()
        || !req.transfer.is_empty()
        || req.install.is_some()
        || req.restore_metadata
        || req.lz4.is_some()
        || req.zstd_pack
        || req.zstd_unpack
        || req.remove_backport
        || req.replace
        || req.delete_source
        || req.pack_profile.is_some()
    {
        findings.push(
            "metadata recovery goes with an edit only: no extract, transfer, install, LZ4, zstd, \
             backport, replace or delete option"
                .to_string(),
        );
    }
    let source = &req.source;
    if crate::remote::is_remote(source) {
        findings.push(format!(
            "{}: metadata is recovered only into a game on this device",
            source.display()
        ));
        return findings;
    }
    match crate::convert::Kind::of(source) {
        Ok(crate::convert::Kind::Archive(_)) => findings.push(format!(
            "{}: an archive is not edited: convert it first",
            source.display()
        )),
        Ok(crate::convert::Kind::Pkg) if ps5_dump_forge_fpkg::is_patch(source) => findings.push(
            format!("{}: a patch package is not edited", source.display()),
        ),
        Ok(_) => {}
        Err(e) => findings.push(format!("{}: {e:#}", source.display())),
    }
    findings.extend(crate::edit_job::smp_inside(source));
    findings.extend(crate::edit_job::installed_here(source));
    findings
}

fn parse_object(bytes: &[u8]) -> anyhow::Result<serde_json::Map<String, serde_json::Value>> {
    let text = std::str::from_utf8(bytes).context("not UTF-8")?;
    match serde_json::from_str(text.trim_start_matches('\u{feff}'))? {
        serde_json::Value::Object(map) => Ok(map),
        _ => bail!("not a JSON object"),
    }
}

/// `get`, mapped to a cancel when the job was cancelled.
fn fetch(
    url: &str,
    max: u64,
    ctx: &Ctx,
    progress: &mut dyn FnMut(u64, u64),
) -> anyhow::Result<Vec<u8>> {
    match ps5_dump_forge_updater::psn::get(url, max, ctx.cancel, progress) {
        Ok(body) => Ok(body),
        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
            ctx.check()?;
            Err(e.into())
        }
        Err(e) => Err(anyhow::anyhow!("couldn't download the metadata: {e}")),
    }
}

/// The path of `url` without its query, lower-cased.
fn url_path(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or("");
    path.to_ascii_lowercase()
}

/// XML's five entities undone (`&amp;` in a URL's query).
fn unescape(v: &str) -> String {
    v.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Each `<package .../>` element's attributes in a version XML.
fn packages(xml: &str) -> Vec<Vec<(String, String)>> {
    let mut out = Vec::new();
    for tag in xml.split('<').skip(1) {
        let Some(rest) = tag.strip_prefix("package") else {
            continue;
        };
        if !rest.starts_with(|c: char| c.is_whitespace() || c == '/' || c == '>') {
            continue;
        }
        let body = rest.split('>').next().unwrap_or("");
        let mut attrs = Vec::new();
        let mut s = body;
        while let Some(eq) = s.find('=') {
            let name = s[..eq].trim().to_string();
            let after = s[eq + 1..].trim_start();
            let Some(quote) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
                break;
            };
            let Some(end) = after[1..].find(quote) else {
                break;
            };
            attrs.push((name, unescape(&after[1..1 + end])));
            s = &after[end + 2..];
        }
        out.push(attrs);
    }
    out
}

/// The manifest URL a version XML offers for `version`, else why there is none.
fn manifest_for(xml: &str, version: &str) -> anyhow::Result<String> {
    let all = packages(xml);
    let attr =
        |p: &Vec<(String, String)>, k: &str| p.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    let offered: Vec<String> = all
        .iter()
        .filter_map(|p| attr(p, "content_ver").or_else(|| attr(p, "version")))
        .collect();
    let found = all.iter().find(|p| {
        attr(p, "content_ver")
            .or_else(|| attr(p, "version"))
            .as_deref()
            == Some(version)
    });
    match found.and_then(|p| attr(p, "manifest_url")) {
        Some(url) => Ok(url),
        None if found.is_some() => bail!("the version XML names no manifest_url for {version}"),
        None => bail!(
            "Sony's version XML offers {} but not this game's version {version}: give the \
             manifest's or the _sc.pkg's URL for {version}",
            if offered.is_empty() {
                "no version".to_string()
            } else {
                offered.join(", ")
            }
        ),
    }
}

/// The manifest's metadata piece: its URL, size and SHA-256.
fn piece_of(manifest: &[u8]) -> anyhow::Result<(String, u64, String)> {
    let json: serde_json::Value =
        serde_json::from_slice(manifest).context("the manifest is not JSON")?;
    let pieces = json
        .get("pieces")
        .and_then(|p| p.as_array())
        .ok_or_else(|| anyhow::anyhow!("the manifest lists no pieces"))?;
    let sc: Vec<&serde_json::Value> = pieces
        .iter()
        .filter(|p| {
            p.get("url")
                .and_then(|u| u.as_str())
                .is_some_and(|u| url_path(u).ends_with("_sc.pkg"))
        })
        .collect();
    let [piece] = sc[..] else {
        bail!("the manifest lists {} _sc.pkg pieces, not one", sc.len());
    };
    let url = piece["url"].as_str().unwrap_or("").to_string();
    let size = piece
        .get("fileSize")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("the manifest gives the _sc.pkg piece no fileSize"))?;
    let hash = piece
        .get("hashValue")
        .and_then(|v| v.as_str())
        .filter(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| anyhow::anyhow!("the manifest gives the _sc.pkg piece no SHA-256"))?
        .to_ascii_lowercase();
    if size > MAX_PIECE {
        bail!("the metadata piece is {size} bytes, over the {MAX_PIECE} taken");
    }
    Ok((url, size, hash))
}

/// The metadata piece for `url`: a piece itself, or found through a version XML or a manifest.
fn download(url: &str, version: &str, ctx: &Ctx, stage: &str) -> anyhow::Result<Vec<u8>> {
    let mut quiet = |_: u64, _: u64| {};
    let mut url = url.trim().to_string();
    let mut expected: Option<(u64, String)> = None;
    if !url_path(&url).ends_with("_sc.pkg") {
        ctx.log(format!("metadata: reading {url}"));
        let mut doc = fetch(&url, MAX_DOC, ctx, &mut quiet)?;
        let text = String::from_utf8_lossy(&doc);
        if text
            .trim_start_matches('\u{feff}')
            .trim_start()
            .starts_with('<')
        {
            url = manifest_for(&text, version)?;
            ctx.log(format!("metadata: reading {url}"));
            doc = fetch(&url, MAX_DOC, ctx, &mut quiet)?;
        }
        let (piece, size, hash) = piece_of(&doc)?;
        url = piece;
        expected = Some((size, hash));
    }
    ctx.log(format!(
        "metadata: downloading the metadata piece only: {url}"
    ));
    ctx.progress(stage, 0, expected.as_ref().map_or(1, |e| e.0));
    let body = fetch(&url, MAX_PIECE, ctx, &mut |got, total| {
        ctx.progress(stage, got, total.max(got));
    })?;
    if let Some((size, hash)) = expected {
        let got = Sha256::digest(&body);
        let got: String = got.iter().map(|b| format!("{b:02x}")).collect();
        if body.len() as u64 != size || got != hash {
            bail!(
                "the metadata piece is {} bytes with SHA-256 {got}, the manifest says {size} \
                 bytes and {hash}: nothing was changed",
                body.len()
            );
        }
        ctx.log(format!(
            "metadata: {size} bytes, SHA-256 {hash} as the manifest says"
        ));
    }
    Ok(body)
}

/// One record of the piece and what becomes of it.
struct Record {
    name: String,
    bytes: Vec<u8>,
}

/// The piece's named records: those to restore, and the names of those left alone, with why.
fn records(piece: Vec<u8>, content_id: &str) -> anyhow::Result<(Vec<Record>, Vec<String>)> {
    use ps5upload_fpkg::cnt::{Cnt, ids};
    if !piece.starts_with(b"\x7FCNT") || piece.len() < 0x14 {
        bail!("the downloaded _sc.pkg does not start with a CNT header");
    }
    // The parser holds every entry the header declares: bounded as a package's container is.
    let count = u32::from_be_bytes(piece[0x10..0x14].try_into().unwrap_or_default()) as usize;
    if count > ps5upload_fpkg::cnt::MAX_ENTRIES {
        bail!("the metadata piece claims {count} entries");
    }
    let cnt = Cnt::from_bytes(piece).map_err(|e| anyhow::anyhow!("the metadata piece: {e}"))?;
    if cnt.content_id != content_id {
        bail!(
            "the metadata piece is {}'s, this game is {content_id}: nothing was changed",
            cnt.content_id
        );
    }
    let names = cnt
        .entry(ids::ENTRY_NAMES)
        .map(|e| cnt.payload(e))
        .unwrap_or(&[]);
    let mut restore: Vec<Record> = Vec::new();
    let mut left = Vec::new();
    for e in &cnt.entries {
        let at = e.name_off as usize;
        if at == 0 || at >= names.len() {
            continue;
        }
        let Some(len) = names[at..].iter().position(|b| *b == 0) else {
            bail!("the metadata piece's name table is not terminated");
        };
        let name = String::from_utf8_lossy(&names[at..at + len]).to_string();
        if e.flags1 & (1 << 31) != 0 {
            if name == "param.json" {
                bail!(
                    "the metadata piece's param.json is protected; it can't be checked against this game"
                );
            }
            left.push(format!("{name} (protected)"));
            continue;
        }
        if !RESTORED.contains(&name.as_str()) {
            left.push(format!("{name} (not plain metadata)"));
            continue;
        }
        if restore.iter().any(|r| r.name == name) {
            bail!("the metadata piece holds {name} twice");
        }
        restore.push(Record {
            name,
            bytes: cnt.payload(e).to_vec(),
        });
    }
    Ok((restore, left))
}

/// Sony's `param.json` checked against the dump's and given the dump's runtime values.
fn param_json(
    remote: &[u8],
    local: &Local,
    original_runtime: bool,
) -> anyhow::Result<(Vec<u8>, Vec<String>)> {
    let json = parse_object(remote).context("the downloaded param.json does not read")?;
    let text = |k: &str| {
        json.get(k)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string()
    };
    for key in ["titleId", "contentId", "contentVersion"] {
        if local.text(key).is_empty() {
            bail!("this game's param.json has no {key}, so Sony's can't be matched to it");
        }
        if text(key) != local.text(key) {
            bail!(
                "the downloaded param.json's {key} is {:?}, this game's {:?}: nothing was changed",
                text(key),
                local.text(key)
            );
        }
    }
    let mut kept = Vec::new();
    if original_runtime {
        return Ok((remote.to_vec(), kept));
    }
    let mut out = json.clone();
    for key in RUNTIME_KEYS {
        match local.json.get(key) {
            Some(v) if json.get(key) != Some(v) => {
                out.insert(key.to_string(), v.clone());
                kept.push(format!("{key} {v}"));
            }
            _ => {}
        }
    }
    if kept.is_empty() {
        return Ok((remote.to_vec(), kept));
    }
    // ponytail: rewritten through serde_json, so key order and spacing are its printer's.
    let mut bytes = serde_json::to_vec_pretty(&serde_json::Value::Object(out))?;
    bytes.push(b'\n');
    Ok((bytes, kept))
}

/// A record of Sony's that differs from the game's own: its path, bytes, and whether the game has none.
struct Change {
    path: String,
    bytes: Vec<u8>,
    new: bool,
}

/// Sony's records that differ from the game's copy in `tree`; the download's progress under `stage`.
fn changes(
    tree: &mut dyn SourceTree,
    options: &MetadataRecovery,
    ctx: &Ctx,
    stage: &str,
) -> anyhow::Result<Vec<Change>> {
    let size_of = |tree: &dyn SourceTree, path: &str| {
        (tree.files().iter())
            .find(|f| f.path == path)
            .map(|f| f.size)
    };
    match size_of(tree, PARAM_JSON) {
        Some(size) if size <= MAX_DOC => {}
        Some(size) => bail!("{PARAM_JSON} is {size} bytes, over the {MAX_DOC} read"),
        None => bail!("the game has no {PARAM_JSON}"),
    }
    let bytes = tree.read(PARAM_JSON)?;
    let json = parse_object(&bytes).with_context(|| format!("{PARAM_JSON} does not read"))?;
    let local = Local { json };
    let url = match options
        .url
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty())
    {
        Some(url) => url.to_string(),
        None => match local.text("versionFileUri") {
            "" => bail!(
                "param.json names no versionFileUri: give the URL of the game's version XML, \
                 manifest or _sc.pkg"
            ),
            uri => uri.to_string(),
        },
    };
    let version = local.text("contentVersion").to_string();
    ctx.log(format!(
        "metadata: {} {} version {version}",
        local.text("titleId"),
        local.text("contentId")
    ));
    let piece = download(&url, &version, ctx, stage)?;
    let (mut restore, left) = records(piece, local.text("contentId"))?;
    let Some(param) = restore.iter_mut().find(|r| r.name == "param.json") else {
        bail!("the metadata piece has no param.json, so it can't be checked against this game");
    };
    let (merged, kept) = param_json(&param.bytes, &local, options.original_runtime)?;
    param.bytes = merged;
    if !kept.is_empty() {
        ctx.log(format!(
            "metadata: param.json keeps this game's {}",
            kept.join(", ")
        ));
    }
    if !left.is_empty() {
        ctx.log(format!("metadata: left as they are: {}", left.join(", ")));
    }
    let mut out = Vec::new();
    let mut same = Vec::new();
    for r in restore {
        let path = format!("sce_sys/{}", r.name);
        let new = match size_of(tree, &path) {
            None => true,
            Some(size) if size == r.bytes.len() as u64 && tree.read(&path)? == r.bytes => {
                same.push(r.name);
                continue;
            }
            Some(_) => false,
        };
        out.push(Change {
            path,
            bytes: r.bytes,
            new,
        });
    }
    if !same.is_empty() {
        ctx.log(format!("metadata: already identical: {}", same.join(", ")));
    }
    Ok(out)
}

/// One line on what changes: "N files (A added, R replaced): names".
fn summary(list: &[Change]) -> String {
    let added = list.iter().filter(|c| c.new).count();
    let names: Vec<&str> = list.iter().map(|c| c.path.as_str()).collect();
    format!(
        "{} files ({added} added, {} replaced): {}",
        list.len(),
        list.len() - added,
        names.join(", ")
    )
}

/// A build's `restore_metadata`: `source` with Sony's metadata served from memory (progress under `scan`); anything but a cancel is a warning and the build goes on.
pub(crate) fn overlay(
    mut source: Box<dyn SourceTree>,
    pkg: bool,
    ctx: &Ctx,
) -> anyhow::Result<Box<dyn SourceTree>> {
    let list = match changes(source.as_mut(), &MetadataRecovery::default(), ctx, "scan") {
        Ok(list) => list,
        Err(_) if ctx.cancel.load(std::sync::atomic::Ordering::Relaxed) => {
            ctx.check()?;
            return Ok(source);
        }
        Err(e) => {
            ctx.log(format!(
                "warning: metadata not restored, building the game as it is: {e:#}"
            ));
            return Ok(source);
        }
    };
    if list.is_empty() {
        ctx.log("metadata: nothing to restore: the game's files match Sony's");
        return Ok(source);
    }
    ctx.log(format!(
        "metadata: the output gets Sony's {}",
        summary(&list)
    ));
    if pkg && list.iter().any(|c| c.path.contains("/playgo-")) {
        ctx.log(
            "metadata: a .pkg writes PlayGo tables of its own; it takes the languages of Sony's \
             playgo-chunk.dat",
        );
    }
    // The source's order kept (an archive is read in it): an added file goes last.
    let mut files = source.files().to_vec();
    let empty_dirs = source.empty_dirs().to_vec();
    let mut generated = std::collections::HashMap::new();
    for c in list {
        let size = c.bytes.len() as u64;
        match files.iter_mut().find(|f| f.path == c.path) {
            Some(f) => f.size = size,
            None => files.push(ps5upload_fpkg::source::SourceFile {
                path: c.path.clone(),
                size,
            }),
        }
        generated.insert(c.path, std::borrow::Cow::Owned(c.bytes));
    }
    Ok(Box::new(crate::lz4::Overlay {
        inner: source,
        files,
        empty_dirs,
        generated,
    }))
}

/// Where a recovery's temporary folders go: the PS5's Dump Forge folder, else the system's.
fn temp_root() -> PathBuf {
    #[cfg(target_env = "ps5")]
    return PathBuf::from("/data/dumpforge/tmp");
    #[cfg(not(target_env = "ps5"))]
    std::env::temp_dir()
}

const TEMP_PREFIX: &str = "forge-metadata-";

/// A recovery's temporary folder `forge-metadata-<pid>-<n>`, deleted when dropped.
pub(crate) struct Temp(PathBuf);

impl Temp {
    fn new() -> anyhow::Result<Temp> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = temp_root();
        std::fs::create_dir_all(&root).with_context(|| root.display().to_string())?;
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = root.join(format!("{TEMP_PREFIX}{}-{n}", std::process::id()));
        std::fs::create_dir(&dir).with_context(|| dir.display().to_string())?;
        Ok(Temp(dir))
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Whether process `pid` still runs (another Dump Forge using its folder).
fn alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let Ok(pid) = libc::pid_t::try_from(pid) else {
            return false;
        };
        // SAFETY: signal 0 only checks that the process exists.
        let sent = unsafe { libc::kill(pid, 0) } == 0;
        sent || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(not(unix))]
    {
        // ponytail: no process check here; a folder is taken for live (kept) for a day.
        let _ = pid;
        true
    }
}

/// Deletes the temporary folders of recoveries whose process is gone, at every [`crate::Jobs::new`] (elsewhere than unix, ones older than a day).
pub(crate) fn clean_stale() {
    let Ok(entries) = std::fs::read_dir(temp_root()) else {
        return;
    };
    let me = std::process::id();
    let day = std::time::Duration::from_secs(24 * 3600);
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(rest) = name.to_str().and_then(|n| n.strip_prefix(TEMP_PREFIX)) else {
            continue;
        };
        let Some(pid) = rest.split('-').next().and_then(|p| p.parse::<u32>().ok()) else {
            continue;
        };
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|t| t.elapsed().is_ok_and(|age| age > day));
        if pid != me && (!alive(pid) || (cfg!(not(unix)) && old)) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// How the recovery edits `kind`: a folder directly, `.exfat`/`.ffpkg` in place, every other image rebuilt.
fn rebuilds(kind: crate::convert::Kind) -> bool {
    use crate::convert::Kind;
    matches!(kind, Kind::Ffpfs | Kind::Ffpfsc | Kind::Pkg)
}

/// The metadata recovery: download, then Sony's changes and the request's `edits` as one edit job (files staged in a [`Temp`]); a failure before the edit changes nothing.
pub(crate) fn run(req: &ConvertRequest, ctx: &Ctx) -> anyhow::Result<JobReport> {
    let options = req.recover_metadata.clone().unwrap_or_default();
    let findings = request_findings(req);
    let kind = crate::convert::Kind::of(&req.source).ok();
    // The edit it becomes, whatever the changes: its stages are known now.
    let edit = ConvertRequest {
        recover_metadata: None,
        edits: match req.edits.is_empty() {
            true => vec![crate::Edit::Mkdir {
                path: "sce_sys".into(),
            }],
            false => req.edits.clone(),
        },
        edit_rebuild: req.edit_rebuild || kind.is_some_and(rebuilds),
        ..req.clone()
    };
    let mut stages = STAGES.to_vec();
    stages.extend(crate::convert::stages(&edit));
    ctx.stages(&stages);
    ctx.fix_stages();
    if !findings.is_empty() {
        return Err(preflight_failed(findings));
    }
    let kind = kind.ok_or_else(|| anyhow::anyhow!("{}: not a game", req.source.display()))?;
    ctx.progress("download", 0, 1);
    let mut tree = crate::convert::open_source(&req.source, kind, ctx.cancel)?;
    let before = tree.read(PARAM_JSON).ok();
    let list = changes(tree.as_mut(), &options, ctx, "download")?;
    drop(tree);
    if list.is_empty() {
        let line = "metadata: nothing to restore: the game's files match Sony's".to_string();
        ctx.log(line.clone());
        // The user's own changes still go in.
        if !req.edits.is_empty() {
            let mut report = crate::convert::convert(&edit, ctx)?;
            report.checks.insert(0, line);
            return Ok(report);
        }
        return Ok(JobReport {
            output: req.source.clone(),
            bytes: 0,
            files: 0,
            checks: vec![line],
            verify: VerifySummary {
                mode: VerifyMode::Fast,
                checked_bytes: 0,
                total_bytes: 0,
                samples: 0,
                seed: 0,
            },
            source_bytes: 0,
            started_at_ms: 0,
            ended_at_ms: 0,
        });
    }
    let line = format!("metadata: restoring {}", summary(&list));
    ctx.log(line.clone());
    let temp = Temp::new()?;
    let mut edits = Vec::new();
    for c in &list {
        let name = c.path.rsplit('/').next().unwrap_or(&c.path);
        let from = temp.0.join(name);
        std::fs::write(&from, &c.bytes).with_context(|| from.display().to_string())?;
        edits.push(crate::Edit::Add {
            from,
            to: c.path.clone(),
        });
    }
    // The user's own changes after Sony's: on the same path, theirs is what stays.
    edits.extend(req.edits.iter().cloned());
    // ponytail: the game is read again just before the edit; a change between this and the edit's own scan isn't seen.
    let mut again = crate::convert::open_source(&req.source, kind, ctx.cancel)?;
    if again.read(PARAM_JSON).ok() != before {
        bail!(
            "{}'s param.json changed while the metadata downloaded: nothing was changed",
            req.source.display()
        );
    }
    drop(again);
    let mut report = crate::convert::convert(&ConvertRequest { edits, ..edit }, ctx)?;
    report.checks.insert(0, line);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The version XML Sony serves (PPSA22946's, trimmed): the manifest for the dump's version; another refused.
    #[test]
    fn version_xml_names_the_manifest_of_the_dump_version() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?><title_patch nptitleid="PPSA22946_00">
<app_tag content_id="EP1018-PPSA22946_00-HARRYPOTTER17000" revision="3">
<package content_ver="01.000.001" delta_url="http://gst/x-DP.pkg" manifest_url="https://sgst/a/b.json?x=1&amp;y=2" system_ver="268500992"/>
</app_tag></title_patch>"#;
        assert_eq!(
            manifest_for(xml, "01.000.001").unwrap(),
            "https://sgst/a/b.json?x=1&y=2"
        );
        let err = manifest_for(xml, "01.000.002").unwrap_err().to_string();
        assert!(
            err.contains("offers 01.000.001 but not this game's version 01.000.002"),
            "{err}"
        );
    }

    /// The manifest's one `_sc.pkg` piece, with its size and hash; two or none refused.
    #[test]
    fn manifest_names_one_metadata_piece() {
        let piece = |n: &str| {
            format!(
                r#"{{"url":"http://h/{n}","fileSize":5,"hashValue":"{}"}}"#,
                "A".repeat(64)
            )
        };
        let json = format!(
            r#"{{"pieces":[{},{}]}}"#,
            piece("x_0.pkg"),
            piece("x_sc.pkg")
        );
        assert_eq!(
            piece_of(json.as_bytes()).unwrap(),
            ("http://h/x_sc.pkg".into(), 5, "a".repeat(64))
        );
        let two = format!(
            r#"{{"pieces":[{},{}]}}"#,
            piece("x_sc.pkg"),
            piece("y_sc.pkg")
        );
        assert!(piece_of(two.as_bytes()).is_err());
        assert!(piece_of(br#"{"pieces":[]}"#).is_err());
    }

    /// A backported dump keeps its lowered SDK and firmware; another version's param.json is refused.
    #[test]
    fn param_json_keeps_the_dump_runtime_and_must_match() {
        let local_bytes = br#"{"titleId":"PPSA1","contentId":"C","contentVersion":"01.000.001","sdkVersion":"0x0100000000000000"}"#;
        let local = Local {
            json: parse_object(local_bytes).unwrap(),
        };
        let sony = br#"{"titleId":"PPSA1","contentId":"C","contentVersion":"01.000.001","sdkVersion":"0x0900000000000000","pubtools":{}}"#;
        let (out, kept) = param_json(sony, &local, false).unwrap();
        let out = parse_object(&out).unwrap();
        assert_eq!(out["sdkVersion"], "0x0100000000000000");
        assert!(out.contains_key("pubtools"));
        assert_eq!(kept.len(), 1);
        assert_eq!(param_json(sony, &local, true).unwrap().0, sony.to_vec());
        let other = br#"{"titleId":"PPSA1","contentId":"C","contentVersion":"01.000.002"}"#;
        assert!(param_json(other, &local, false).is_err());
    }
}
