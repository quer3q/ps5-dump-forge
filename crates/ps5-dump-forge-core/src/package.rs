//! FPKG output: preflight the source for a package, let the vendored
//! builder prepare and write it into this job's `.part`, read it back through `FpkgSource`
//! against the effective manifest, then publish it.

use std::cell::Cell;
use std::collections::HashMap;
use std::path::Path;

use anyhow::{anyhow, bail};
use ps5_dump_forge_fpkg::FpkgSource;
use ps5upload_fpkg::build::{self, BuildControl, BuildRequest, Origin, Prepared, Stage};
use ps5upload_fpkg::source::{self, SourceFile, SourceTree};
use ps5upload_fpkg::{Error, sdk_rules};
use unicode_normalization::is_nfc;

use crate::convert::SourceStamp;
use crate::finalize::{Part, part_path};
use crate::jobs::Ctx;
use crate::preflight::{self, GameInfo, listing};
use crate::verify::{self, HashingTree};
use crate::{ConvertRequest, JobReport};

/// Shown on every package job: a plaintext debug package installs nowhere else.
pub(crate) const CONSOLE: &str =
    "console: installing needs kstuff + fpkg-enable + ppr-patch (drakmor)";

/// `param.json` `applicationCategoryType` values v1 packages: a game, and an app.
const GAME: u64 = 0;
const APP: u64 = 65536;
/// The AMPR file index a libSceAmpr title reads from the image root.
const AMPR_INDEX: &str = "ampr_emu.index";
/// The readiness check `source::readiness` adds for a title that imports libSceAmpr.
const AMPR_CHECK: &str = "ampr_emu.index for a libSceAmpr title";
/// What `prepare` logs when it cannot generate the index.
const AMPR_SKIPPED: &str = "not generating ampr_emu.index";
const MIB: u64 = 1024 * 1024;

/// What stops a package build before anything is prepared (findings), and what is worth
/// showing either way (notes): readiness, category, firmware, ids, package path names, and
/// the AMPR case-collision rule.
pub(crate) fn source_findings(
    tree: &mut dyn SourceTree,
    info: &GameInfo,
) -> (Vec<String>, Vec<String>) {
    let mut findings = Vec::new();
    let mut notes = Vec::new();
    let files: Vec<String> = tree
        .files()
        .iter()
        .map(|f| f.path.clone())
        .filter(|p| sdk_rules::excluded(p).is_none())
        .collect();

    // ponytail: the category is read from applicationCategoryType alone; a patch that keeps
    // its game's category passes. Tell patches apart once a dumped patch's param.json is in hand.
    if let Some(json) = &info.param_json {
        match json.get("applicationCategoryType").map(|v| v.as_u64()) {
            None | Some(Some(GAME)) => notes.push("category: base game".to_string()),
            Some(Some(APP)) => notes.push("category: app".to_string()),
            Some(other) => findings.push(format!(
                "sce_sys/param.json applicationCategoryType is {}: only a base game ({GAME}) or \
                 an app ({APP}) can be packaged; patches and DLC are not supported yet",
                other.map_or_else(|| "not a number".to_string(), |n| n.to_string())
            )),
        }
        match build::firmware_version(json) {
            Some(v) => notes.push(format!(
                "firmware: the package requires system software {v}"
            )),
            None => {
                notes.push("firmware: param.json names no requiredSystemSoftwareVersion".into())
            }
        }
    }
    if !files.iter().any(|p| p == "eboot.bin") {
        findings.push(
            "no eboot.bin: additional content (DLC) and patches cannot be packaged yet, \
             only base games and apps"
                .to_string(),
        );
    }
    if let (Some(title), Some(cid)) = (&info.title_id, &info.content_id)
        && let Some(from_cid) = preflight::title_from_content_id(cid)
        && *title != from_cid
    {
        notes.push(format!(
            "title id {title} differs from the content id's {from_cid}: the packaged \
             param.json names {from_cid}"
        ));
    }

    // ponytail: readiness scans eboot.bin for libSceAmpr (up to ~80 MiB, not cancellable),
    // and prepare runs it once more.
    let readiness = source::readiness(tree);
    for c in &readiness.checks {
        let mark = if c.ok { "ok" } else { "warning" };
        notes.push(format!("readiness [{mark}] {}: {}", c.name, c.detail));
    }
    if let Some(c) = readiness
        .checks
        .iter()
        .find(|c| c.name == "content id" && !c.ok)
    {
        findings.push(format!(
            "content id: {}: a package needs a 36-character contentId in sce_sys/param.json",
            c.detail
        ));
    }

    let mut bad = Vec::new();
    for path in files.iter().chain(tree.empty_dirs()) {
        if let Some(why) = bad_package_path(path) {
            bad.push(format!("{path} ({why})"));
        }
    }
    findings.extend(listing("package path", &bad));

    let ampr = readiness.checks.iter().any(|c| c.name == AMPR_CHECK);
    let own_index = files.iter().any(|p| p.eq_ignore_ascii_case(AMPR_INDEX));
    if ampr && !own_index {
        let clashes = case_clashes(&files);
        if !clashes.is_empty() {
            findings.push(format!(
                "eboot.bin imports libSceAmpr and the source has no {AMPR_INDEX}; the one the \
                 package needs cannot be generated, because these paths differ only in case:"
            ));
            findings.extend(listing("same path but for case", &clashes));
        }
    }
    (findings, notes)
}

/// Why a package (PFS) path cannot be written as is: `plan::build` checks nothing.
fn bad_package_path(path: &str) -> Option<&'static str> {
    if !is_nfc(path) {
        return Some("not in NFC");
    }
    path.split('/').find_map(|c| match c {
        "" => Some("empty component (leading, trailing or doubled '/')"),
        "." | ".." => Some("'.' or '..' component"),
        _ if c.contains('\0') => Some("NUL in name"),
        _ if c.len() > 255 => Some("component longer than 255 bytes"),
        _ => None,
    })
}

/// Paths the AMPR index cannot tell apart, as `a = b` pairs. Its key folds ASCII case and
/// reads `\` as `/` (`ampr_index::key`).
fn case_clashes(files: &[String]) -> Vec<String> {
    let key = |p: &str| p.replace('\\', "/").to_ascii_lowercase();
    let mut seen: HashMap<String, &str> = HashMap::new();
    let mut clashes = Vec::new();
    for path in files {
        match seen.get(&key(path)) {
            Some(other) => clashes.push(format!("{other} = {path}")),
            None => {
                seen.insert(key(path), path);
            }
        }
    }
    clashes
}

/// Refusals only `prepare` can tell: its own AMPR index skip, as a hard error.
pub(crate) fn prepared_findings(prepared: &Prepared) -> Vec<String> {
    prepared
        .log()
        .iter()
        .filter(|l| l.starts_with(AMPR_SKIPPED))
        .map(|l| format!("{l}: a libSceAmpr title needs it, so the package is refused"))
        .collect()
}

/// The vendored error as the job reports it; a stop the job asked for is `cancelled`.
fn fpkg(what: &str) -> impl Fn(Error) -> anyhow::Error + '_ {
    move |e| match e {
        Error::Cancelled => anyhow!("cancelled"),
        e => anyhow!("{what}: {e}"),
    }
}

pub(crate) fn run(
    req: &ConvertRequest,
    ctx: &Ctx,
    tree: &mut dyn SourceTree,
    info: &GameInfo,
    mut findings: Vec<String>,
    out: &Path,
    stamp: &SourceStamp,
) -> anyhow::Result<JobReport> {
    ctx.log(CONSOLE);
    let (more, notes) = source_findings(tree, info);
    findings.extend(more);
    for note in notes {
        ctx.log(note);
    }
    ctx.check()?;
    if !findings.is_empty() {
        bail!("preflight failed:\n  {}", findings.join("\n  "));
    }
    ctx.progress("preflight", 1, 1);

    let dir = out.parent().expect("output_path gives a parent");
    // `source`/`output_dir` are not read by `prepare`/`write_package`; the `.part` is ours.
    let mut request = BuildRequest::production(&req.source, dir);
    request.threads = req.compression_threads;
    let stage = Cell::new(Stage::Check.id());
    let sized = Cell::new(false);
    let mut on_stage = |s: Stage| {
        stage.set(s.id());
        sized.set(false);
        ctx.progress(s.id(), 0, 1);
    };
    let file_bytes = tree
        .files()
        .iter()
        .fold(0u64, |sum, f| sum.saturating_add(f.size));
    // `prepare` sizes the package, filled in below; each pass renews the estimate as it
    // learns its own size.
    let estimate = Cell::new(0u64);
    let mut on_bytes = |done: u64, total: u64| {
        if !sized.replace(true) {
            // Compress reads the source, write and the builder's verify the package, our
            // verify the files.
            let rest = match stage.get() {
                "compress" => total.saturating_add(estimate.get().saturating_mul(2)),
                "write" => total.saturating_mul(2),
                _ => total, // the builder's verify
            };
            ctx.expect_rest(rest.saturating_add(file_bytes));
        }
        ctx.progress(stage.get(), done, total)
    };
    let mut control = BuildControl {
        bytes: Some(&mut on_bytes),
        cancel: Some(ctx.cancel),
        stage: Some(&mut on_stage),
    };
    let prepared =
        build::prepare(tree, &request, &mut control).map_err(fpkg("preparing the package"))?;
    for line in prepared.log() {
        ctx.log(format!("fpkg: {line}"));
    }
    let mut findings = prepared_findings(&prepared);
    // The builder spools its compressed image inside the package itself (production requests
    // read no spool dir), so the package is the only thing written.
    let size = prepared.estimated_size();
    estimate.set(size);
    let need = size.saturating_add(size / 100).saturating_add(64 * MIB);
    let (space, notes) = preflight::destination(dir, need, size);
    findings.extend(space);
    for note in notes {
        ctx.log(note);
    }
    if !findings.is_empty() {
        bail!("preflight failed:\n  {}", findings.join("\n  "));
    }
    ctx.check()?;
    ctx.log(format!(
        "package: content id {}, about {} MiB",
        prepared.content_id(),
        size.div_ceil(MIB)
    ));

    let (part, mut file) = Part::create_file(&part_path(out, ctx.job))?;
    let mut hashing = HashingTree::new(tree);
    let report = build::write_package(
        &prepared,
        &mut hashing,
        &mut file,
        part.path(),
        &mut |line| ctx.log(format!("fpkg: {line}")),
        &mut control,
    )
    .map_err(fpkg("writing the package"))?;
    ctx.check()?;
    let mut checks: Vec<String> = report
        .verify
        .checks
        .iter()
        .map(|c| {
            format!("fpkg verify: {} {}", c.name, c.detail)
                .trim_end()
                .to_string()
        })
        .collect();
    checks.push(format!(
        "package: {} bytes, content id {}",
        report.size, report.content_id
    ));

    // Verification: the expected manifest is what the package must carry, each file hashed as the
    // builder serves it; files the build passed through keep the hash of the source read.
    ctx.progress("verify", 0, 1);
    let source_hashes = hashing.into_hashes();
    let mut packaged = Packaged::new(&prepared, tree);
    let known: HashMap<String, blake3::Hash> = packaged
        .unchanged
        .iter()
        .filter_map(|p| source_hashes.get(p).map(|h| (p.clone(), *h)))
        .collect();
    let expected = verify::expected_from(&mut packaged, &known, ctx)?;
    checks.push(source_summary(&prepared));
    ctx.check()?;

    // Through the handle the package was written with, whatever its name now names.
    let len = file.metadata()?.len();
    let label = part.path().display().to_string();
    let mut back = FpkgSource::from_reader(Box::new(file.try_clone()?), len, label, None)
        .map_err(|e| anyhow!("reading the package back: {e}"))?;
    checks.push(format!("reader: {}", back.describe()));
    for line in back.conflicts() {
        ctx.log(format!("package reader: {line}"));
    }
    checks.extend(verify::compare(&expected, &mut back, ctx)?);
    drop(back);
    checks.push(CONSOLE.to_string());
    for line in &checks {
        ctx.log(format!("check: {line}"));
    }
    drop(file);

    ctx.expect_rest(0);
    ctx.progress("finalize", 0, 1);
    ctx.check()?;
    stamp.check()?;
    part.publish(out)?;
    ctx.progress("finalize", 1, 1);
    checks.push(format!("published {}", out.display()));
    Ok(JobReport {
        output: out.to_path_buf(),
        bytes: report.size,
        files: expected.files.len() as u64,
        checks,
    })
}

/// How the package's files relate to the source's, for the report.
fn source_summary(prepared: &Prepared) -> String {
    let unchanged = prepared
        .manifest()
        .iter()
        .filter(|e| e.origin == Origin::Source)
        .count()
        + prepared.container_only().len();
    let changed: Vec<String> = prepared
        .manifest()
        .iter()
        .filter(|e| e.origin != Origin::Source)
        .map(|e| {
            let how = match e.origin {
                Origin::Rewritten => "rewritten",
                Origin::Repaired => "repaired",
                _ => "generated",
            };
            format!("{} ({how})", e.path)
        })
        .collect();
    let total = prepared.manifest().len() + prepared.container_only().len();
    let mut line = format!(
        "source hashes: {unchanged} of {total} packaged files are the source's bytes \
         ({} as container entries)",
        prepared.container_only().len()
    );
    if !changed.is_empty() {
        const SHOWN: usize = 10;
        line += &format!(
            "; made by the build: {}",
            changed[..changed.len().min(SHOWN)].join(", ")
        );
        if changed.len() > SHOWN {
            line += &format!(" and {} more", changed.len() - SHOWN);
        }
    }
    line
}

/// What the package must read back as: the effective manifest plus the container-only
/// artwork, each served exactly as the builder packaged it.
struct Packaged<'a> {
    prepared: &'a Prepared,
    tree: &'a mut dyn SourceTree,
    files: Vec<SourceFile>,
    /// Paths whose packaged bytes are the source's, unchanged.
    unchanged: Vec<String>,
}

impl<'a> Packaged<'a> {
    fn new(prepared: &'a Prepared, tree: &'a mut dyn SourceTree) -> Self {
        let entries = prepared.manifest().iter().chain(prepared.container_only());
        let mut files: Vec<SourceFile> = entries
            .clone()
            .map(|e| SourceFile {
                path: e.path.clone(),
                size: e.size,
            })
            .collect();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let unchanged = entries
            .filter(|e| e.origin == Origin::Source)
            .map(|e| e.path.clone())
            .collect();
        Self {
            prepared,
            tree,
            files,
            unchanged,
        }
    }
}

impl SourceTree for Packaged<'_> {
    fn files(&self) -> &[SourceFile] {
        &self.files
    }

    fn read(&mut self, path: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
        let size = self
            .files
            .iter()
            .find(|f| f.path == path)
            .map_or(0, |f| f.size);
        self.read_range(path, 0, size as usize)
    }

    fn read_range(
        &mut self,
        path: &str,
        offset: u64,
        len: usize,
    ) -> ps5upload_fpkg::Result<Vec<u8>> {
        // Container-only artwork is the source's file; `Prepared::read_range` serves the image.
        if self
            .prepared
            .container_only()
            .iter()
            .any(|e| e.path == path)
        {
            return self.tree.read_range(path, offset, len);
        }
        self.prepared.read_range(self.tree, path, offset, len)
    }

    fn empty_dirs(&self) -> &[String] {
        self.prepared.empty_dirs()
    }

    fn describe(&self) -> String {
        format!("package manifest of {}", self.tree.describe())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tree in memory: case-colliding names cannot be made on a case-insensitive disk.
    struct Mem(Vec<SourceFile>, HashMap<String, Vec<u8>>);

    impl Mem {
        fn new(files: &[(&str, Vec<u8>)]) -> Self {
            Self(
                files
                    .iter()
                    .map(|(p, b)| SourceFile {
                        path: p.to_string(),
                        size: b.len() as u64,
                    })
                    .collect(),
                files
                    .iter()
                    .map(|(p, b)| (p.to_string(), b.clone()))
                    .collect(),
            )
        }
    }

    impl SourceTree for Mem {
        fn files(&self) -> &[SourceFile] {
            &self.0
        }
        fn read(&mut self, path: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
            self.1
                .get(path)
                .cloned()
                .ok_or_else(|| Error::Format(format!("no {path}")))
        }
        fn describe(&self) -> String {
            "mem".into()
        }
    }

    const PARAM: &str =
        r#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-TESTTESTTESTTEST"}"#;

    fn ampr_eboot() -> Vec<u8> {
        let mut eboot = vec![0u8; 4096];
        eboot[..4].copy_from_slice(b"\x7fELF");
        eboot[2048..2062].copy_from_slice(b"libSceAmpr.prx");
        eboot
    }

    fn findings_of(tree: &mut Mem) -> (Vec<String>, Vec<String>) {
        let (info, _) = preflight::input(tree);
        source_findings(tree, &info)
    }

    #[test]
    fn ampr_case_collision_is_refused_before_and_by_prepare() {
        let files = [
            ("eboot.bin", ampr_eboot()),
            ("sce_sys/param.json", PARAM.as_bytes().to_vec()),
            ("data/Level.bin", vec![1; 10]),
            ("data/level.bin", vec![2; 10]),
        ];
        let mut tree = Mem::new(&files);
        let (findings, notes) = findings_of(&mut tree);
        assert!(
            findings
                .iter()
                .any(|f| f.contains("data/Level.bin = data/level.bin")),
            "{findings:?}"
        );
        assert!(notes.iter().any(|n| n.contains(AMPR_CHECK)), "{notes:?}");

        // The backstop: prepare itself skips the index, and that skip is refused too.
        let request = BuildRequest::production("unused", "unused");
        let prepared = build::prepare(&mut tree, &request, &mut BuildControl::default()).unwrap();
        let found = prepared_findings(&prepared);
        assert_eq!(found.len(), 1, "{:?}", prepared.log());

        // Its own index makes the collision harmless; without libSceAmpr it never mattered.
        let mut own = files.to_vec();
        own.push((AMPR_INDEX, vec![0; 8]));
        assert!(findings_of(&mut Mem::new(&own)).0.is_empty());
        let mut plain = files.to_vec();
        plain[0].1 = vec![0x7f; 4096];
        assert!(findings_of(&mut Mem::new(&plain)).0.is_empty());
    }

    #[test]
    fn package_names_and_category() {
        let param = r#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-TESTTESTTESTTEST","applicationCategoryType":33554432}"#;
        let long = format!("data/{}", "x".repeat(256));
        let mut tree = Mem::new(&[
            ("eboot.bin", vec![0x7f; 64]),
            ("sce_sys/param.json", param.as_bytes().to_vec()),
            ("data/cafe\u{301}.bin", vec![1]),
            ("data//twice", vec![1]),
            (long.as_str(), vec![1]),
            ("sce_sys/playgo-chunk.dat", vec![1]),
        ]);
        let (findings, _) = findings_of(&mut tree);
        let text = findings.join("\n");
        for want in [
            "applicationCategoryType is 33554432",
            "not in NFC",
            "empty component",
            "longer than 255 bytes",
        ] {
            assert!(text.contains(want), "{want:?} not in:\n{text}");
        }
        assert_eq!(findings.len(), 4, "{text}");
    }

    #[test]
    fn short_content_id_is_refused() {
        let mut tree = Mem::new(&[
            ("eboot.bin", vec![0x7f; 64]),
            (
                "sce_sys/param.json",
                br#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-X"}"#.to_vec(),
            ),
        ]);
        let (findings, _) = findings_of(&mut tree);
        assert!(
            findings.iter().any(|f| f.starts_with("content id:")),
            "{findings:?}"
        );
    }
}
