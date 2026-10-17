//! `inspect` and `default_output`: what a source holds, and what to call its conversion.

use std::collections::BTreeMap;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use anyhow::{Context, bail};
use ps5_dump_forge_fpkg::FpkgSource;
use ps5_dump_forge_pfs::{PfsHeader, PfsSource};
use ps5upload_fpkg::exfat::ExFat;
use ps5upload_fpkg::{PkgFile, fih};
use unicode_normalization::{UnicodeNormalization, is_nfc};

use crate::convert::{Kind, Nested, open_source, percent};
use crate::preflight::{self, GameInfo, listing};
use crate::scan::JunkFiltered;
use crate::{Format, InspectFile, Inspection, Lz4Facts, Lz4Packed};
use ps5_dump_forge_lz4::reader;
use ps5upload_fpkg::source::{SourceFile, SourceTree};
use serde_json::Value;

const ALREADY_SMP: &str = "already 512-byte sectors + 64 KiB clusters; conversion only rewrites it";

/// (BytesPerSectorShift, SectorsPerClusterShift) as the boot sector stores them.
fn shifts(g: &ps5upload_fpkg::exfat::Geometry) -> (u32, u32) {
    let sector = g.sector_size.trailing_zeros();
    (
        sector,
        g.cluster_size.trailing_zeros().saturating_sub(sector),
    )
}

/// The note for an `.exfat` source that already has the SMP geometry; the job still runs.
pub(crate) fn already_smp(path: &Path) -> Option<String> {
    let g = ExFat::open(path).ok()?.geometry();
    (shifts(&g) == (9, 7) && g.volume_offset == 0)
        .then(|| format!("{}: {ALREADY_SMP}", path.display()))
}

/// The version in the maker's mark of an `.exfat` (`exfat`) or `.ffpkg` image; `None` when
/// it has none or can't be read (inspect goes on without it).
fn maker_version<R: Read + Seek>(r: &mut R, exfat: bool) -> Option<String> {
    let mark = if exfat {
        ps5_dump_forge_exfat::read_maker(r)
    } else {
        ps5_dump_forge_ufs2::read_volname(r)
    };
    let version = mark
        .ok()??
        .strip_prefix(crate::convert::MAKER_PREFIX)?
        .to_string();
    let printable = version.bytes().all(|b| b.is_ascii_graphic());
    (!version.is_empty() && printable).then_some(version)
}

pub(crate) fn inspect(path: &Path, password: Option<&str>) -> anyhow::Result<Inspection> {
    let kind = Kind::of(path)?;
    let mut archive = None;
    let mut details = Vec::new();
    let mut pfs_findings = Vec::new();
    let mut cnt_id = None;
    let mut forge_version = None;
    let mut tree = if kind == Kind::Pkg {
        let pkg =
            FpkgSource::open(path, None).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
        (details, cnt_id) = pkg_header(path)?;
        details.extend(pkg.details());
        Box::new(JunkFiltered::new(Box::new(pkg)))
    } else if kind == Kind::Ffpfs {
        let pfs = PfsSource::open(path).map_err(|e| anyhow::anyhow!("{e}"))?;
        pfs_facts("PFS", pfs.header(), &mut details, &mut pfs_findings);
        Box::new(JunkFiltered::new(Box::new(pfs)))
    } else if kind == Kind::Ffpfsc {
        let (tree, info) = crate::convert::open_ffpfsc(path)?;
        pfs_facts("outer PFS", &info.outer, &mut details, &mut pfs_findings);
        // The zlib header's level class, with the levels this app's writer gives it.
        let class = match info.zlib_level_class {
            Some(0) => ", zlib class fastest (level 1 here)",
            Some(1) => ", zlib class fast (levels 2–3 here)",
            Some(2) => ", zlib class default (levels 4–8 here)",
            Some(_) => ", zlib class maximum (level 9 here)",
            None => "",
        };
        details.push(if info.compressed {
            format!(
                "container: {}, {} bytes stored in {} ({}%), {} of {} 64 KiB blocks compressed{class}",
                info.inner_name,
                info.raw_size,
                info.stored_size,
                percent(info.stored_size, info.raw_size),
                info.compressed_blocks,
                info.blocks
            )
        } else {
            format!(
                "container: {}, {} bytes, not compressed",
                info.inner_name, info.raw_size
            )
        });
        // The maker's mark is in the inner image.
        let inner = info.inner_name.to_ascii_lowercase();
        if [".exfat", ".ffpkg", ".ufs2"]
            .iter()
            .any(|e| inner.ends_with(e))
        {
            forge_version = (|| {
                let file = std::fs::File::open(path).ok()?;
                let outer = PfsSource::from_reader(Box::new(file), String::new()).ok()?;
                let mut nested = Nested::new(outer, &info.inner_name).ok()?;
                maker_version(&mut nested, inner.ends_with(".exfat"))
            })();
        }
        // A nested `.ffpfs` has a block size of its own.
        if info.inner_name.to_ascii_lowercase().ends_with(".ffpfs") {
            let file = std::fs::File::open(path)?;
            let label = path.display().to_string();
            let outer = PfsSource::from_reader(Box::new(file), label.clone())
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let nested = Nested::new(outer, &info.inner_name)?;
            let inner = PfsSource::from_reader(Box::new(nested), label)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            pfs_facts("inner PFS", inner.header(), &mut details, &mut pfs_findings);
        }
        Box::new(JunkFiltered::new(tree))
    } else if let Kind::Archive(format) = kind {
        let (tree, facts) = archive_tree(path, password, format)?;
        details.push(format!(
            "archive: {} bytes in {} part(s){}{}; game folder {}",
            facts.archive_bytes,
            facts.volumes.len(),
            if facts.solid { ", solid" } else { "" },
            if facts.encrypted { ", encrypted" } else { "" },
            if facts.root.is_empty() {
                "at the root"
            } else {
                &facts.root
            },
        ));
        archive = Some(facts);
        Box::new(JunkFiltered::new(Box::new(tree)))
    } else {
        open_source(path, kind, &AtomicBool::new(false))?
    };
    let (mut info, mut findings) = preflight::input(tree.as_mut());
    findings.append(&mut pfs_findings);
    if info.content_id.is_none() {
        info.content_id = cnt_id;
    }
    if info.title_id.is_none() {
        info.title_id = info
            .content_id
            .as_deref()
            .and_then(preflight::title_from_content_id);
    }
    if kind == Kind::Exfat {
        let g = ExFat::open(path)
            .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?
            .geometry();
        let (sector_shift, cluster_shift) = shifts(&g);
        let fast = sector_shift == 9 && cluster_shift == 7;
        details.push(format!(
            "exFAT: {} B sectors, {} KiB clusters (shifts {sector_shift}/{cluster_shift}), \
             {} clusters, volume at byte {}; {} the SMP fast path (9/7: 512 B / 64 KiB)",
            g.sector_size,
            g.cluster_size / 1024,
            g.cluster_count,
            g.volume_offset,
            if fast { "matches" } else { "does not match" },
        ));
        if g.volume_offset != 0 {
            findings.push(
                "the volume does not start at byte 0 (partitioned); SMP needs a raw image"
                    .to_string(),
            );
        }
        if fast && g.volume_offset == 0 {
            findings.push(ALREADY_SMP.to_string());
        } else if !fast {
            findings.push(
                "geometry differs from the SMP fast path (512 B sectors, 64 KiB clusters); \
                 converting to .exfat rebuilds it"
                    .to_string(),
            );
        }
    }
    if matches!(kind, Kind::Exfat | Kind::Ffpkg | Kind::Ffpfs | Kind::Ffpfsc) {
        let len = std::fs::metadata(path)?.len();
        details.push(format!("image file: {len} bytes"));
        if len % (64 * 1024) != 0 {
            findings.push("the image size is not a multiple of 64 KiB".to_string());
        }
    }
    if matches!(kind, Kind::Exfat | Kind::Ffpkg) {
        forge_version = std::fs::File::open(path)
            .ok()
            .and_then(|mut f| maker_version(&mut f, kind == Kind::Exfat));
    }
    let paths: Vec<String> = tree.files().iter().map(|f| f.path.clone()).collect();
    if kind != Kind::Folder {
        // Image names are untrusted: say up front what extraction would refuse.
        for line in crate::extract::extraction_findings(
            &paths,
            tree.empty_dirs(),
            crate::extract::CASE_INSENSITIVE,
        ) {
            findings.push(format!("extraction: {}", line.trim_start()));
        }
    }
    let not_nfc: Vec<String> = paths.iter().filter(|p| !is_nfc(p)).cloned().collect();
    let not_ascii: Vec<String> = paths.iter().filter(|p| !p.is_ascii()).cloned().collect();
    for (kind, names) in [
        ("name not in NFC (refused by every image format)", &not_nfc),
        ("name not ASCII (refused by .ffpfs)", &not_ascii),
    ] {
        findings.extend(
            listing(kind, names)
                .into_iter()
                .map(|l| l.trim_start().to_string()),
        );
    }
    let param = info.param_json.as_ref();
    let text = |key: &str| param.and_then(|p| p.get(key)).and_then(Value::as_str);
    let title_name = param.and_then(title_name);
    let title_names = param.map(title_names).unwrap_or_default();
    let version = text("contentVersion").map(str::to_string);
    let firmware = text("requiredSystemSoftwareVersion").map(bcd_version);
    let sdk = text("sdkVersion").map(bcd_version);
    // `fakelib/` (or SMP 1.7's exclusive `fakelib2/`) holds emulators and the backport.
    let fakelib = crate::backport::classify(tree.as_mut());
    let backport_blocked = if fakelib.libs.is_empty() {
        None
    } else {
        crate::backport::blocked(tree.as_mut(), info.param_json.as_ref())
    };
    let backport_firmware = if paths.iter().any(|p| is_fakelib(p)) {
        crate::sdk::lowest_firmware(tree.as_mut())
    } else {
        None
    };
    let dlcs = crate::dlc::find(tree.as_mut(), info.content_id.as_deref());
    let cover = cover(tree.as_mut());
    let lz4 = lz4_facts(tree.as_mut(), &mut findings);
    let files: Vec<InspectFile> = tree
        .files()
        .iter()
        .map(|f| InspectFile {
            path: f.path.clone(),
            size: f.size,
        })
        .collect();
    Ok(Inspection {
        kind: kind.name().to_string(),
        describe: tree.describe(),
        title_id: info.title_id,
        content_id: info.content_id,
        additional_content: info.additional_content,
        title_name,
        title_names,
        version,
        firmware,
        sdk,
        backport: fakelib.libs,
        emulators: fakelib.emulators,
        backport_blocked,
        backport_firmware,
        dlcs,
        forge_version,
        cover,
        param_json: info.param_json,
        total_bytes: files.iter().fold(0u64, |s, f| s.saturating_add(f.size)),
        files,
        empty_dirs: tree.empty_dirs().to_vec(),
        details,
        findings,
        lz4,
        archive,
        interrupted_edit: kind != Kind::Folder
            && ps5_dump_forge_patch::journal_path(path)
                .symlink_metadata()
                .is_ok(),
    })
}

/// The most an archive's inspection unpacks into memory.
const ARCHIVE_READ_BUDGET: u64 = 256 << 20;

/// An archive's game folder as inspecting sees it: its listing, and the bytes
/// [`archive_reads`] chose; reading past those fails, so a check that needs more says so.
struct ArchiveTree {
    files: Vec<SourceFile>,
    dirs: Vec<String>,
    /// Path → (its size, the bytes read from its start).
    bytes: BTreeMap<String, (u64, Vec<u8>)>,
    describe: String,
}

impl SourceTree for ArchiveTree {
    fn files(&self) -> &[SourceFile] {
        &self.files
    }

    fn read(&mut self, path: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
        self.read_range(path, 0, usize::MAX)
    }

    fn read_range(
        &mut self,
        path: &str,
        offset: u64,
        len: usize,
    ) -> ps5upload_fpkg::Result<Vec<u8>> {
        if let Some((size, head)) = self.bytes.get(path) {
            let end = offset.saturating_add(len as u64).min(*size);
            if end <= head.len() as u64 {
                return Ok(head[offset.min(end) as usize..end as usize].to_vec());
            }
        }
        Err(ps5upload_fpkg::Error::Format(format!(
            "{path}: not unpacked for inspecting the archive"
        )))
    }

    /// Every folder; [`JunkFiltered`] keeps the empty ones.
    fn empty_dirs(&self) -> &[String] {
        &self.dirs
    }

    fn describe(&self) -> String {
        self.describe.clone()
    }
}

/// An archive's listing, its game folder and the few files inspecting reads (in memory).
fn archive_tree(
    path: &Path,
    password: Option<&str>,
    format: ps5_dump_forge_archive::ArchiveFormat,
) -> anyhow::Result<(ArchiveTree, crate::ArchiveFacts)> {
    let cancel = AtomicBool::new(false);
    let password = crate::archive::password(path, password, &cancel)?;
    let password = password.as_deref();
    let listing = ps5_dump_forge_archive::list(path, password)?;
    let root = crate::library::archive_game_root(path, password, &listing, &cancel)?;
    let under = |p: &str| p.strip_prefix(root.as_str()).map(str::to_string);
    let files: Vec<SourceFile> = listing
        .entries
        .iter()
        .filter(|e| !e.dir)
        .filter_map(|e| {
            Some(SourceFile {
                path: under(&e.path)?,
                size: e.size,
            })
        })
        .collect();
    let dirs = listing
        .entries
        .iter()
        .filter(|e| e.dir)
        .filter_map(|e| under(&e.path))
        .filter(|d| !d.is_empty())
        .collect();
    let keep = archive_reads(&files);
    let mut bytes = ps5_dump_forge_archive::read_heads(
        path,
        password,
        &|p, _| under(p).and_then(|p| keep.get(&p).copied()),
        keep.values().sum(),
        &cancel,
    )?;
    let bytes = files
        .iter()
        .filter_map(|f| {
            let head = bytes.remove(&format!("{root}{}", f.path))?;
            Some((f.path.clone(), (f.size, head)))
        })
        .collect();
    let name = |p: &PathBuf| p.file_name().map(|n| n.to_string_lossy().into_owned());
    let facts = crate::ArchiveFacts {
        format,
        archive_bytes: listing.archive_bytes,
        volumes: listing.volumes.iter().filter_map(name).collect(),
        solid: listing.solid,
        encrypted: listing.encrypted,
        root,
    };
    let describe = format!(
        "{} archive {}",
        crate::archive::name(format),
        path.display()
    );
    Ok((
        ArchiveTree {
            files,
            dirs,
            bytes,
            describe,
        },
        facts,
    ))
}

/// The largest `ampr_emu.index` an archive's inspection reads (for [`Lz4Facts::index`]).
const MAX_ARCHIVE_INDEX: u64 = 16 << 20;

/// What inspecting an archive unpacks, path → bytes from its start: whole metadata, cover,
/// `eboot.bin`, `fakelib*/` and the LZ4 manifest, then `ampr_emu.index` (optional: it must not
/// displace `eboot.bin`, which decides AMPR), then other executables smallest first, each only
/// while the sum stays within [`ARCHIVE_READ_BUDGET`].
fn archive_reads(files: &[SourceFile]) -> BTreeMap<String, u64> {
    use ps5_dump_forge_lz4::{INDEX, MANIFEST, format::PAK_HEADER};
    let lower = |f: &SourceFile| f.path.to_ascii_lowercase();
    let mut wanted: Vec<(&SourceFile, u64)> = Vec::new();
    for f in files {
        let l = lower(f);
        let metadata = (l.ends_with("sce_sys/param.json") || l.ends_with("sce_sys/param.sfo"))
            && f.size <= preflight::MAX_PARAM_JSON;
        let small = (f.path == "sce_sys/icon0.png" && f.size <= MAX_COVER)
            || (l == crate::dlc::DLC_EMU && f.size <= crate::dlc::MAX_DLC_EMU);
        if metadata || small {
            wanted.push((f, f.size));
        } else if f.path == MANIFEST {
            let all = f.size <= FULL_PARSE_BYTES;
            wanted.push((f, if all { f.size } else { PAK_HEADER as u64 }));
        }
    }
    let code = |f: &&SourceFile| f.path.eq_ignore_ascii_case("eboot.bin") || is_fakelib(&f.path);
    wanted.extend(files.iter().filter(code).map(|f| (f, f.size)));
    let index = |f: &&SourceFile| f.path == INDEX && f.size <= MAX_ARCHIVE_INDEX;
    wanted.extend(files.iter().filter(index).map(|f| (f, f.size)));
    let mut rest: Vec<&SourceFile> = files
        .iter()
        .filter(|f| !code(f) && crate::sdk::is_executable(&f.path))
        .collect();
    rest.sort_by_key(|f| f.size);
    wanted.extend(rest.into_iter().map(|f| (f, f.size)));
    let mut keep = BTreeMap::new();
    let mut sum = 0u64;
    for (f, n) in wanted {
        if !keep.contains_key(&f.path) && sum.saturating_add(n) <= ARCHIVE_READ_BUDGET {
            sum += n;
            keep.insert(f.path.clone(), n);
        }
    }
    keep
}

/// The LZ4 facts of a source; a damaged manifest becomes a finding. `None` without libSceAmpr
/// or any LZ4 artifact.
fn lz4_facts(tree: &mut dyn SourceTree, findings: &mut Vec<String>) -> Option<Lz4Facts> {
    use crate::lz4::{IndexState, RuntimeAction};
    use ps5_dump_forge_lz4::runtime::RuntimeKind;
    use ps5_dump_forge_lz4::{INDEX, JOURNAL, MANIFEST, reader};
    let size = |tree: &dyn SourceTree, path: &str| {
        tree.files().iter().find(|f| f.path == path).map(|f| f.size)
    };
    let imports_ampr = crate::lz4::imports_ampr(tree);
    let journal_bytes = size(tree, JOURNAL);
    let rt = crate::lz4::runtime_of(tree);
    let kind = rt.kind;
    let mut packed = None;
    let mut manifest_error = None;
    let holds_packs = reader::detect(tree).unwrap_or(false);
    if holds_packs {
        match read_manifest(tree, FULL_PARSE_BYTES) {
            Ok((p, large)) => {
                if large {
                    findings.push(format!(
                        "LZ4 packs: {MANIFEST} manifest too large to inspect fully; \
                         only its header counts are shown"
                    ));
                }
                packed = Some(p);
            }
            Err(e) => {
                findings.push(format!("LZ4 packs: {MANIFEST} is damaged: {e}"));
                manifest_error = Some(e);
            }
        }
    }
    let artifact = holds_packs
        || journal_bytes.is_some()
        || size(tree, INDEX).is_some()
        || matches!(kind, RuntimeKind::ForgeRelease | RuntimeKind::ForgeTrace);
    if !imports_ampr && !artifact {
        return None;
    }
    // As a plain conversion sees it (`lz4::plain`): a runtime it installs or replaces is
    // compared at the release runtime's size.
    let action = rt.action(imports_ampr, holds_packs);
    let index = if holds_packs || action == RuntimeAction::Untouched {
        None
    } else {
        let files = crate::lz4::plain_listing(tree.files(), action);
        match crate::lz4::index_state(tree, &files) {
            Ok(IndexState::Ok(_)) => Some("ok"),
            Ok(IndexState::Stale) => Some("stale"),
            Ok(IndexState::Damaged) => Some("damaged"),
            Ok(IndexState::Missing) => Some("missing"),
            Err(_) => None,
        }
    };
    // Only an AMPR title's runtime is the user's concern (`lz4::Runtime::action`).
    let name = |a: RuntimeAction| match a {
        _ if !imports_ampr => None,
        RuntimeAction::Install => Some("install"),
        RuntimeAction::Replace => Some("replace"),
        RuntimeAction::Keep => Some("keep"),
        RuntimeAction::Untouched => None,
    };
    let runtime = match kind {
        RuntimeKind::ForgeRelease => "forge_release",
        RuntimeKind::ForgeTrace => "forge_trace",
        RuntimeKind::Other => "other",
        RuntimeKind::None => "none",
    };
    Some(Lz4Facts {
        imports_ampr,
        packed,
        manifest_error,
        runtime: runtime.to_string(),
        shipped_runtime_version: ps5_dump_forge_lz4::runtime::VERSION,
        runtime_action: name(action),
        // Pack writes packs; Unpack (a packed source) writes none (`lz4::prepare`).
        pack_runtime_action: name(rt.action(imports_ampr, !holds_packs)),
        runtime_version: rt.version,
        index,
        journal_bytes,
        traces_zip: (journal_bytes.is_some() && size(tree, INDEX).is_some()).then(|| {
            let param = size(tree, "sce_sys/param.json")
                .filter(|&s| s <= preflight::MAX_PARAM_JSON)
                .and_then(|_| tree.read("sce_sys/param.json").ok());
            traces_zip_name(param.as_deref())
        }),
    })
}

/// Manifests up to this size are parsed whole; a bigger (valid-looking) one is summed up from
/// its header, so inspecting a hostile file never allocates its declared size.
const FULL_PARSE_BYTES: u64 = 64 << 20;

/// The manifest's counts, reading the 128-byte header first. The bool is true when the
/// manifest was too big to parse and only the header counts are known.
fn read_manifest(tree: &mut dyn SourceTree, limit: u64) -> Result<(Lz4Packed, bool), String> {
    use ps5_dump_forge_lz4::MANIFEST;
    use ps5_dump_forge_lz4::format::{self as f, u32_at, u64_at};
    let actual = tree
        .files()
        .iter()
        .find(|x| x.path == MANIFEST)
        .map_or(0, |x| x.size);
    let h = tree
        .read_range(MANIFEST, 0, f::PAK_HEADER)
        .map_err(|e| e.to_string())?;
    if h.len() < f::PAK_HEADER {
        return Err("manifest is shorter than its header".into());
    }
    let e = |r: ps5upload_fpkg::Result<u32>| r.map_err(|e| e.to_string());
    for (at, want, what) in [
        (8, f::PAK_VERSION, "version"),
        (12, f::PAK_HEADER as u32, "header size"),
        (16, 0, "flags"),
        (20, f::ENDIAN_MARKER, "endian marker"),
        (60, f::FILE_RECORD as u32, "file record size"),
        (64, f::CHUNK_RECORD as u32, "chunk record size"),
        (68, f::PACK_RECORD as u32, "pack record size"),
    ] {
        let got = e(u32_at(&h, at))?;
        if got != want {
            return Err(format!("manifest {what} is {got:#x}, not {want:#x}"));
        }
    }
    if f::pak_header_crc(&h).map_err(|e| e.to_string())? != e(u32_at(&h, f::PAK_HEADER_CRC_AT))? {
        return Err("manifest header CRC mismatch".into());
    }
    let at = u64_at(&h, 96).map_err(|e| e.to_string())?;
    let len = u64_at(&h, 104).map_err(|e| e.to_string())?;
    if at.checked_add(len) != Some(actual) {
        return Err(format!(
            "manifest is {actual} bytes, its header declares {at} + {len}"
        ));
    }
    if actual > limit {
        let files = u64_at(&h, 40).map_err(|e| e.to_string())?;
        let volumes = u64::from(e(u32_at(&h, 56))?);
        let counts = Lz4Packed {
            files,
            packed_files: None,
            volumes,
            stored_percent: None,
        };
        return Ok((counts, true));
    }
    let b = tree
        .read_range(MANIFEST, 0, actual as usize)
        .map_err(|e| e.to_string())?;
    let m = reader::open_manifest(&b).map_err(|e| e.to_string())?;
    Ok((packed_counts(&m), false))
}

/// Counts from a sound manifest; the percentage is stored bytes of the packed files over
/// their logical bytes (loose files and metadata are not in either).
fn packed_counts(m: &reader::Manifest) -> Lz4Packed {
    let (mut packed_files, mut stored, mut logical) = (0u64, 0u64, 0u64);
    for f in m.files.iter().filter(|f| f.packed()) {
        packed_files += 1;
        logical = logical.saturating_add(f.size);
        let first = f.first_chunk as usize;
        let end = first
            .saturating_add(f.chunk_count as usize)
            .min(m.chunks.len());
        for c in m.chunks.get(first..end).unwrap_or_default() {
            stored = stored.saturating_add(u64::from(c.stored));
        }
    }
    Lz4Packed {
        files: m.files.len() as u64,
        packed_files: Some(packed_files),
        volumes: m.packs.len() as u64,
        stored_percent: Some(percent(stored, logical)),
    }
}

/// A PFS header as one details line; a block size the console misreads is a finding.
fn pfs_facts(what: &str, h: &PfsHeader, details: &mut Vec<String>, findings: &mut Vec<String>) {
    details.push(format!(
        "{what}: version {}, {} KiB blocks, {} inodes, mode {:#x}, {} blocks declared",
        h.version,
        h.block_size / 1024,
        h.inodes,
        h.mode,
        h.ndblock
    ));
    if u64::from(h.block_size) != ps5_dump_forge_pfs::BLOCK {
        findings.push(format!(
            "{what} block size is {} KiB: the console misreads PFS blocks other than 64 KiB",
            h.block_size / 1024
        ));
    }
}

/// A file under a top-level `fakelib/` or `fakelib2/`, any case, as the package builder and
/// ShadowMountPlus see them.
pub(crate) fn is_fakelib(path: &str) -> bool {
    path.split_once('/').is_some_and(|(top, _)| {
        top.eq_ignore_ascii_case("fakelib") || top.eq_ignore_ascii_case("fakelib2")
    })
}

/// The title in `param.json`'s default language (`localizedParameters`), else a bare
/// `titleName` as hand-made files have.
pub(crate) fn title_name(param: &Value) -> Option<String> {
    let name = |v: &Value| v.get("titleName")?.as_str().map(str::to_string);
    let localized = param.get("localizedParameters");
    let default = localized
        .and_then(|l| l.get("defaultLanguage"))
        .and_then(Value::as_str)
        .unwrap_or("en-US");
    localized
        .and_then(|l| l.get(default))
        .and_then(name)
        .or_else(|| name(param))
}

/// Every `localizedParameters` title by language tag (`en-US`, `ru-RU`, ...), for the UI to
/// pick the viewer's; `defaultLanguage` and entries without a string `titleName` are skipped.
pub(crate) fn title_names(param: &Value) -> BTreeMap<String, String> {
    let Some(localized) = param.get("localizedParameters").and_then(Value::as_object) else {
        return BTreeMap::new();
    };
    localized
        .iter()
        .filter(|(k, _)| *k != "defaultLanguage")
        .filter_map(|(k, v)| Some((k.clone(), v.get("titleName")?.as_str()?.to_string())))
        .collect()
}

/// A BCD version word as the console shows it: `0x1160000000000000` is `11.60`,
/// `0x0700…` is `7.00`; a 32-bit word that lost its leading zero (`0x9000038`) is `9.00`.
pub(crate) fn bcd(word: &str) -> Option<String> {
    let hex = word
        .strip_prefix("0x")
        .or_else(|| word.strip_prefix("0X"))?;
    let width = if hex.len() <= 8 { 8 } else { 16 };
    let hex = format!("{hex:0>width$}");
    let digits = hex.bytes().all(|b| b.is_ascii_hexdigit());
    if hex.len() != width || !digits || !hex[..4].bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let major = hex[..2].trim_start_matches('0').max("0");
    Some(format!("{major}.{}", &hex[2..4]))
}

/// [`bcd`], or the word as it was when it is not one.
fn bcd_version(word: &str) -> String {
    bcd(word).unwrap_or_else(|| word.to_string())
}

/// The longest game name kept in a generated file name, in UTF-8 bytes. With the title id,
/// extension, a `-N` and the job's `.part` suffix the name stays well inside 255
/// bytes (APFS) and 255 UTF-16 units after decomposition (HFS+, exFAT, NTFS).
const MAX_NAME_BYTES: usize = 100;

/// `name` as part of a file name any destination accepts: letters, digits, spaces and
/// `-_.,'()&+!` kept, anything else (separators, `:*?"<>|`, ™, ®) dropped, spaces
/// collapsed, NFC, at most [`MAX_NAME_BYTES`], no dot or space at either end.
fn file_safe(name: &str) -> String {
    let kept: String = name
        .nfc()
        .filter(|c| c.is_alphanumeric() || " -_.,'()&+!".contains(*c))
        .collect();
    let words = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut cut = String::new();
    for c in words.chars() {
        if cut.len() + c.len_utf8() > MAX_NAME_BYTES {
            break;
        }
        cut.push(c);
    }
    cut.trim_matches(|c| c == '.' || c == ' ').to_string()
}

/// Windows reads `CON.anything` as the console device, whatever the extension; likewise
/// `COM0`-`COM9`, `LPT0`-`LPT9` and their superscript `¹²³` forms.
fn is_reserved_on_windows(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim_end();
    let upper = stem.to_ascii_uppercase();
    let port = upper
        .strip_prefix("COM")
        .or_else(|| upper.strip_prefix("LPT"));
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || port.is_some_and(|n| {
            let mut c = n.chars();
            c.next().is_some_and(|d| "0123456789¹²³".contains(d)) && c.next().is_none()
        })
}

/// The most `-N` suffixes tried for a free generated name.
const MAX_SUFFIX: u32 = 999;

/// The biggest cover shown; real ones are 512x512 PNGs of a few hundred KiB.
const MAX_COVER: u64 = 4 * 1024 * 1024;

/// `sce_sys/icon0.png` as a data URL, when there is one of a sane size.
fn cover(tree: &mut dyn SourceTree) -> Option<String> {
    const PATH: &str = "sce_sys/icon0.png";
    let size = tree.files().iter().find(|f| f.path == PATH)?.size;
    if size == 0 || size > MAX_COVER {
        return None;
    }
    png_data_url(&tree.read(PATH).ok()?)
}

/// PNG bytes as a data URL; `None` when they do not start with the PNG signature.
pub(crate) fn png_data_url(png: &[u8]) -> Option<String> {
    png.starts_with(b"\x89PNG")
        .then(|| format!("data:image/png;base64,{}", base64(png)))
}

/// Standard base64 with padding (RFC 4648 §4).
fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().fold(0u32, |n, &b| n << 8 | b as u32) << (8 * (3 - chunk.len()));
        for i in 0..4 {
            out.push(if i <= chunk.len() {
                ABC[(n >> (18 - 6 * i) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}

/// The finalized-image header and the content id from the CNT header. Two bounded reads.
fn pkg_header(path: &Path) -> anyhow::Result<(Vec<String>, Option<String>)> {
    const CNT_MAGIC: &[u8; 4] = b"\x7FCNT";
    let mut file = PkgFile::open(path).with_context(|| format!("{}", path.display()))?;
    let head = file.read_at(0, fih::HEADER_LEN)?;
    let fih = fih::parse(&head)?;
    let mut details = vec![format!(
        "FIH: {} image, format {}, PFS at {:#x} ({} bytes), CNT at {:#x}",
        if fih.is_debug() { "debug" } else { "retail" },
        fih.format_version,
        fih.pfs_offset,
        fih.pfs_size,
        fih.cnt_offset
    )];
    let cnt = file.read_at(fih.cnt_offset, 0x64)?;
    if &cnt[..4] != CNT_MAGIC {
        details.push("no CNT container at the declared offset".to_string());
        return Ok((details, None));
    }
    let id: String = cnt[0x40..0x64]
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as char)
        .collect();
    let id = (!id.is_empty() && id.chars().all(|c| c.is_ascii_graphic())).then_some(id);
    Ok((details, id))
}

/// `[GAME_NAME]-[TITLE_ID]` from the source, brackets included, e.g.
/// `[Astro Bot]-[PPSA01234].ffpkg`; parts it lacks are left out, and with neither it is
/// [`default_output`]'s name. An empty `dir` is the folder holding the source. `password`: an
/// archive's (else its listing or `param.json` may be locked).
/// ponytail: the source is opened again (a folder scanned, a package's image walked) for
/// every generated name, each format or folder change; metadata only, cache it per source
/// if a big game makes that slow.
pub(crate) fn generated_output(
    source: &Path,
    format: Format,
    dir: &Path,
    taken: &[PathBuf],
    password: Option<&str>,
) -> anyhow::Result<PathBuf> {
    let dir = &output_dir(source, dir)?;
    let info = match Kind::of(source)? {
        Kind::Archive(_) => archive_info(source, password)?,
        kind => {
            let mut tree = open_source(source, kind, &AtomicBool::new(false))?;
            preflight::input(tree.as_mut()).0
        }
    };
    let parts = NameParts::of(&info);
    let mut stem = match parts.stem(usize::MAX) {
        Some(stem) => stem,
        None => default_stem(source, info.title_id)?,
    };
    if is_reserved_on_windows(&stem) {
        stem.insert(0, '_');
    }
    // The name can't be edited while it is generated, so it never names something that
    // exists or that a running job (`taken`) will publish: a rebuild becomes `-2`, `-3`, ...
    // (publishing still never replaces, should something take the name meanwhile).
    for n in 1..=MAX_SUFFIX {
        let suffix = if n == 1 {
            String::new()
        } else {
            format!("-{n}")
        };
        // Within the name limit (what ShadowMountPlus can mount): the game name is cut first,
        // the title id kept, then the suffix.
        let max = preflight::stem_limit(format);
        let stem = if stem.len() + suffix.len() > max {
            let room = max.saturating_sub(suffix.len());
            match &parts.name {
                Some(_) => parts.stem(room).unwrap_or_default(),
                None => cut(&stem, room).to_string(),
            }
        } else {
            stem.clone()
        };
        let path = with_extension(dir, &format!("{stem}{suffix}"), format);
        if !taken.contains(&path) && std::fs::symlink_metadata(&path).is_err() {
            return Ok(path);
        }
    }
    bail!(
        "{stem} and its -2 to -{MAX_SUFFIX} variants are all taken in {}",
        dir.display()
    )
}

/// The download name of a source's LZ4 traces, from its `param.json` bytes: the generated
/// output stem ([`NameParts::stem`], `[GAME_TITLE]-[TITLE_ID]`, the title cut first to keep the
/// stem within 63 bytes) and `-amprtrace.zip`; either part alone without the other,
/// `amprtrace.zip` with neither.
pub(crate) fn traces_zip_name(param_json: Option<&[u8]>) -> String {
    let info = param_json.map(preflight::parse_param).unwrap_or_default();
    download_name(&info, "amprtrace.zip")
}

/// `[GAME_TITLE]-[TITLE_ID]-<suffix>` as [`traces_zip_name`] builds it, `<suffix>` alone
/// without either part.
pub(crate) fn download_name(info: &GameInfo, suffix: &str) -> String {
    match name_stem(info) {
        Some(stem) => format!("{stem}-{suffix}"),
        None => suffix.to_string(),
    }
}

/// The generated stem, `[GAME_TITLE]-[TITLE_ID]` within 63 bytes; `None` with neither part.
pub(crate) fn name_stem(info: &GameInfo) -> Option<String> {
    NameParts::of(info).stem(63)
}

/// The parts of a generated name: the game's title made file-safe (`None` when nothing of it
/// is left) and the title id in brackets.
struct NameParts {
    name: Option<String>,
    id: Option<String>,
}

impl NameParts {
    fn of(info: &GameInfo) -> Self {
        Self {
            name: info
                .param_json
                .as_ref()
                .and_then(title_name)
                .map(|n| file_safe(&n))
                .filter(|n| !n.is_empty()),
            id: info.title_id.as_ref().map(|id| format!("[{id}]")),
        }
    }

    /// `[GAME_NAME]-[TITLE_ID]` within `max` bytes, the name cut first ([`stem_with`]); `None`
    /// with neither part.
    fn stem(&self, max: usize) -> Option<String> {
        (self.name.is_some() || self.id.is_some())
            .then(|| stem_with(self.name.as_deref(), self.id.as_deref().unwrap_or(""), max))
    }
}

/// `[name]-` before `id` (bracketed, or empty), the name cut to keep the whole within `max`
/// bytes and left out if nothing of it fits.
fn stem_with(name: Option<&str>, id: &str, max: usize) -> String {
    let sep = usize::from(!id.is_empty());
    let room = max.saturating_sub(id.len() + sep + 2);
    let name = name
        .map(|n| cut(n, room).trim_end_matches(['.', ' ']))
        .filter(|n| !n.is_empty());
    match name {
        Some(n) if id.is_empty() => format!("[{n}]"),
        Some(n) => format!("[{n}]-{id}"),
        None => cut(id, max).to_string(),
    }
}

/// Where a default or generated output goes: `dir`, or, when it is empty, the folder holding
/// `source`, made absolute from the source's path. Never a bare relative name, which would
/// land in the process's working directory (the PS5 payload's is wherever its loader left it).
fn output_dir(source: &Path, dir: &Path) -> anyhow::Result<PathBuf> {
    if !dir.as_os_str().is_empty() {
        return Ok(dir.to_path_buf());
    }
    let mut source =
        std::path::absolute(source).with_context(|| format!("{}", source.display()))?;
    // `absolute` keeps a trailing `..` (`convert ..`), whose lexical parent is the wrong folder.
    if source.file_name().is_none() {
        source = source
            .canonicalize()
            .with_context(|| format!("{}", source.display()))?;
    }
    match source.parent() {
        Some(parent) => Ok(parent.to_path_buf()),
        None => bail!("{} has no folder to put the output in", source.display()),
    }
}

/// `s` cut to at most `max` UTF-8 bytes, at a character boundary.
fn cut(s: &str, max: usize) -> &str {
    let mut end = max.min(s.len());
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn with_extension(dir: &Path, name: &str, format: Format) -> PathBuf {
    match preflight::extension(format) {
        Some(ext) => dir.join(format!("{name}.{ext}")),
        None => dir.join(name),
    }
}

/// `<TITLE_ID>.<ext>` (else the source's own name) in `dir`; an empty `dir` is the folder
/// holding the source. `password`: an archive's.
pub(crate) fn default_output(
    source: &Path,
    format: Format,
    dir: &Path,
    password: Option<&str>,
) -> anyhow::Result<PathBuf> {
    let dir = output_dir(source, dir)?;
    let title = game_info(source, password).ok().and_then(|i| i.title_id);
    Ok(with_extension(&dir, &default_stem(source, title)?, format))
}

/// The title id, else the source's own name.
fn default_stem(source: &Path, title_id: Option<String>) -> anyhow::Result<String> {
    Ok(match title_id {
        Some(id) => id,
        None => {
            let stem = if source.is_dir() {
                source.file_name()
            } else {
                source.file_stem()
            };
            match stem.and_then(|s| s.to_str()) {
                Some(s) if !s.is_empty() => s.to_string(),
                _ => bail!("cannot name an output for {}", source.display()),
            }
        }
    })
}

/// The source's title id (and `param.json` where it is cheap), read as cheaply as each
/// source allows: a package's header gives the title id without opening its image.
fn game_info(source: &Path, password: Option<&str>) -> anyhow::Result<GameInfo> {
    let kind = Kind::of(source)?;
    let info = match kind {
        Kind::Archive(_) => archive_info(source, password)?,
        // Straight from disk: naming an output needs no full scan of the folder.
        Kind::Folder => {
            let param = source.join("sce_sys").join("param.json");
            let file = crate::scan::open_nofollow(&param)?;
            if !file.metadata()?.is_file() {
                return Ok(GameInfo::default());
            }
            let mut bytes = Vec::new();
            file.take(preflight::MAX_PARAM_JSON + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 > preflight::MAX_PARAM_JSON {
                return Ok(GameInfo::default());
            }
            preflight::parse_param(&bytes)
        }
        Kind::Pkg => {
            let (_, content_id) = pkg_header(source)?;
            let title_id = content_id
                .as_deref()
                .and_then(preflight::title_from_content_id);
            GameInfo {
                title_id,
                ..GameInfo::default()
            }
        }
        _ => {
            let mut tree = open_source(source, kind, &AtomicBool::new(false))?;
            preflight::input(tree.as_mut()).0
        }
    };
    Ok(info)
}

/// An archive's `param.json` alone, from its game folder: no other file is unpacked.
fn archive_info(source: &Path, password: Option<&str>) -> anyhow::Result<GameInfo> {
    let cancel = AtomicBool::new(false);
    let password = crate::archive::password(source, password, &cancel)?;
    let password = password.as_deref();
    let listing = ps5_dump_forge_archive::list(source, password)?;
    let root = crate::library::archive_game_root(source, password, &listing, &cancel)?;
    let param = format!("{root}sce_sys/param.json");
    let mut files = ps5_dump_forge_archive::read_files(
        source,
        password,
        &|p| p == param,
        preflight::MAX_PARAM_JSON,
        &cancel,
    )?;
    Ok(files
        .remove(&param)
        .map(|b| preflight::parse_param(&b))
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_archive_index_never_displaces_eboot() {
        let file = |path: &str, size: u64| SourceFile {
            path: path.into(),
            size,
        };
        let index = file(ps5_dump_forge_lz4::INDEX, MAX_ARCHIVE_INDEX);
        let eboot = file("eboot.bin", ARCHIVE_READ_BUDGET - 1024);
        let keep = archive_reads(&[index.clone(), eboot]);
        assert!(keep.contains_key("eboot.bin"), "{keep:?}");
        assert!(!keep.contains_key(&index.path), "{keep:?}");
        let keep = archive_reads(&[index.clone(), file("eboot.bin", 1 << 20)]);
        assert!(keep.contains_key(&index.path), "{keep:?}");
    }

    #[test]
    fn generated_names() {
        assert_eq!(file_safe("ASTRO BOT™"), "ASTRO BOT");
        assert_eq!(file_safe("  a/b:c*d?  e  "), "abcd e");
        assert_eq!(file_safe("Marvel's Spider-Man 2"), "Marvel's Spider-Man 2");
        assert_eq!(file_safe("..hidden.."), "hidden");
        assert_eq!(file_safe("ゼルダ"), "ゼルダ");
        assert_eq!(file_safe(&"x".repeat(200)).len(), MAX_NAME_BYTES);
        assert!(file_safe(&"각".repeat(80)).len() <= MAX_NAME_BYTES);
        for name in [
            "CON",
            "con.Game",
            "LPT1",
            "nul .x",
            "Com9.a",
            "COM¹.Game",
            "lpt³",
            "COM0",
        ] {
            assert!(is_reserved_on_windows(name), "{name}");
        }
        for name in [
            "CONSOLE",
            "CON-PPSA01234-7.00",
            "COM10",
            "LPT",
            "Game",
            "COM¹²",
        ] {
            assert!(!is_reserved_on_windows(name), "{name}");
        }

        let root = crate::test_dir("generated-name");
        let game = root.join("game");
        std::fs::create_dir_all(game.join("sce_sys")).unwrap();
        std::fs::write(game.join("eboot.bin"), b"x").unwrap();
        std::fs::write(
            game.join("sce_sys/param.json"),
            r#"{"titleId":"PPSA01234","requiredSystemSoftwareVersion":"0x0700000000000000",
                "localizedParameters":{"defaultLanguage":"en-US","en-US":{"titleName":"Astro: Bot"}}}"#,
        )
        .unwrap();
        // No firmware in the name, whatever param.json says.
        assert_eq!(
            generated_output(&game, Format::Exfat, &root, &[], None).unwrap(),
            root.join("[Astro Bot]-[PPSA01234].exfat")
        );
        assert_eq!(
            generated_output(&game, Format::Folder, &root, &[], None).unwrap(),
            root.join("[Astro Bot]-[PPSA01234]")
        );
        // Taken names are skipped, never offered.
        std::fs::write(root.join("[Astro Bot]-[PPSA01234].exfat"), b"").unwrap();
        std::fs::write(root.join("[Astro Bot]-[PPSA01234]-2.exfat"), b"").unwrap();
        assert_eq!(
            generated_output(&game, Format::Exfat, &root, &[], None).unwrap(),
            root.join("[Astro Bot]-[PPSA01234]-3.exfat")
        );
        // A running job's output counts as taken too.
        let running = [root.join("[Astro Bot]-[PPSA01234]-3.exfat")];
        assert_eq!(
            generated_output(&game, Format::Exfat, &root, &running, None).unwrap(),
            root.join("[Astro Bot]-[PPSA01234]-4.exfat")
        );
        // No name: only the title id.
        std::fs::write(
            game.join("sce_sys/param.json"),
            r#"{"titleId":"PPSA01234"}"#,
        )
        .unwrap();
        assert_eq!(
            generated_output(&game, Format::Ffpkg, &root, &[], None).unwrap(),
            root.join("[PPSA01234].ffpkg")
        );
        // Nothing usable: the source's own name, still never a taken one.
        std::fs::write(game.join("sce_sys/param.json"), "{}").unwrap();
        std::fs::write(root.join("game.ffpkg"), b"").unwrap();
        assert_eq!(
            generated_output(&game, Format::Ffpkg, &root, &[], None).unwrap(),
            root.join("game-2.ffpkg")
        );
    }

    #[test]
    fn generated_names_fit_smp_mount_points() {
        for format in [
            Format::Exfat,
            Format::Ffpkg,
            Format::Ffpfs,
            Format::Folder,
            Format::Lz4,
            Format::Pkg,
        ] {
            assert_eq!(preflight::stem_limit(format), 63);
        }
        assert_eq!(preflight::stem_limit(Format::Ffpfsc), 58);

        let root = crate::test_dir("generated-smp-name");
        let game = root.join("game");
        std::fs::create_dir_all(game.join("sce_sys")).unwrap();
        std::fs::write(game.join("eboot.bin"), b"x").unwrap();
        let param = |name: &str| {
            let json = format!(r#"{{"titleId":"PPSA01234","titleName":"{name}"}}"#);
            std::fs::write(game.join("sce_sys/param.json"), json).unwrap();
        };
        let stem = |p: &Path| p.file_stem().unwrap().to_str().unwrap().to_string();
        param(&"Long Game Name ".repeat(6));
        // An LZ4 packed folder is named as a folder: no extension.
        assert_eq!(
            generated_output(&game, Format::Lz4, &root, &[], None).unwrap(),
            generated_output(&game, Format::Folder, &root, &[], None).unwrap()
        );
        // Every output cuts it, a folder and a .pkg too.
        for (format, max) in [
            (Format::Exfat, 63),
            (Format::Ffpkg, 63),
            (Format::Ffpfs, 63),
            (Format::Ffpfsc, 58),
            (Format::Pkg, 63),
            (Format::Folder, 63),
        ] {
            let path = generated_output(&game, format, &root, &[], None).unwrap();
            let s = match format {
                Format::Folder => path.file_name().unwrap().to_str().unwrap().to_string(),
                _ => stem(&path),
            };
            assert_eq!(s.len(), max, "{s}");
            assert!(s.starts_with("[Long Game Name Long"), "{s}");
            assert!(s.ends_with("]-[PPSA01234]"), "{s}");
            // With a `-N` suffix the name gives up more room.
            std::fs::write(&path, b"").unwrap();
            let next = stem(&generated_output(&game, format, &root, &[], None).unwrap());
            assert!(
                next.len() <= max && next.ends_with("]-[PPSA01234]-2"),
                "{next}"
            );
        }
        // Cut at a character boundary, never inside one, and no trailing space.
        param(&"ゼルダ ".repeat(12));
        let pfsc = generated_output(&game, Format::Ffpfsc, &root, &[], None).unwrap();
        let s = stem(&pfsc);
        assert!(s.len() <= 58, "{s}");
        assert!(s.starts_with("[ゼルダ ゼルダ") && !s.contains(" ]"), "{s}");
        assert!(s.ends_with("]-[PPSA01234]"), "{s}");
        // A short name is left whole.
        param("Astro Bot");
        assert_eq!(
            generated_output(&game, Format::Ffpfsc, &root, &[], None).unwrap(),
            root.join("[Astro Bot]-[PPSA01234].ffpfsc")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn empty_dir_is_next_to_the_source() {
        let root = crate::test_dir("output-dir");
        let game = root.join("homebrew/game");
        std::fs::create_dir_all(game.join("sce_sys")).unwrap();
        std::fs::write(game.join("eboot.bin"), b"x").unwrap();
        std::fs::write(
            game.join("sce_sys/param.json"),
            r#"{"titleId":"PPSA01234","titleName":"Astro Bot"}"#,
        )
        .unwrap();
        // Never a bare name the process's working directory would resolve.
        let next_to = root.join("homebrew");
        assert_eq!(
            generated_output(&game, Format::Ffpkg, Path::new(""), &[], None).unwrap(),
            next_to.join("[Astro Bot]-[PPSA01234].ffpkg")
        );
        assert_eq!(
            default_output(&game, Format::Exfat, Path::new(""), None).unwrap(),
            next_to.join("PPSA01234.exfat")
        );
        // A trailing separator names the same folder.
        let slash = PathBuf::from(format!("{}/", game.display()));
        assert_eq!(
            default_output(&slash, Format::Exfat, Path::new(""), None).unwrap(),
            next_to.join("PPSA01234.exfat")
        );
        // A trailing `..` is the folder it climbs to (`convert ..`), not its lexical parent.
        // Compared as folders: Windows' `absolute` resolves `..` itself (keeping 8.3 names
        // like RUNNER~1), Unix's goes through `canonicalize` (`\\?\` on Windows).
        let up =
            default_output(&game.join("sce_sys/.."), Format::Exfat, Path::new(""), None).unwrap();
        assert_eq!(up.file_name(), Some("PPSA01234.exfat".as_ref()));
        assert_eq!(
            up.parent().unwrap().canonicalize().unwrap(),
            next_to.canonicalize().unwrap()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn versions_and_titles() {
        assert_eq!(bcd_version("0x1160000000000000"), "11.60");
        assert_eq!(bcd_version("0x0700000000000000"), "7.00");
        assert_eq!(bcd_version("0x0010000000000000"), "0.10");
        assert_eq!(bcd_version("0x9000038"), "9.00");
        assert_eq!(bcd_version("0x07500001"), "7.50");
        assert_eq!(bcd_version("0x1160garbage"), "0x1160garbage");
        assert_eq!(bcd_version("0x11600000000000000"), "0x11600000000000000");
        assert_eq!(bcd_version("garbage"), "garbage");
        let param: Value = serde_json::from_str(
            r#"{"localizedParameters":{"defaultLanguage":"fr-FR","fr-FR":{"titleName":"Jeu"},
                "en-US":{"titleName":"Game"}},"titleName":"bare"}"#,
        )
        .unwrap();
        assert_eq!(title_name(&param).as_deref(), Some("Jeu"));
        let bare: Value = serde_json::from_str(r#"{"titleName":"bare"}"#).unwrap();
        assert_eq!(title_name(&bare).as_deref(), Some("bare"));
        let names = title_names(&param);
        assert_eq!(names.len(), 2);
        assert_eq!(names["fr-FR"], "Jeu");
        assert_eq!(names["en-US"], "Game");
        assert!(title_names(&bare).is_empty());
        let odd: Value = serde_json::from_str(
            r#"{"localizedParameters":{"defaultLanguage":"ru-RU","ru-RU":{"titleName":"Игра"},
                "de-DE":{"titleName":7},"ja-JP":{}}}"#,
        )
        .unwrap();
        let names = title_names(&odd);
        assert_eq!(names.len(), 1);
        assert_eq!(names["ru-RU"], "Игра");
    }

    /// A manifest tree that only ever serves its header.
    struct Header(Vec<u8>, Vec<ps5upload_fpkg::source::SourceFile>);

    impl SourceTree for Header {
        fn files(&self) -> &[ps5upload_fpkg::source::SourceFile] {
            &self.1
        }
        fn read(&mut self, _: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
            panic!("whole manifest read");
        }
        fn read_range(&mut self, _: &str, off: u64, len: usize) -> ps5upload_fpkg::Result<Vec<u8>> {
            assert!(off == 0 && len <= 128, "read {len} bytes at {off}");
            Ok(self.0.clone())
        }
        fn describe(&self) -> String {
            "header".into()
        }
    }

    fn header(files: u64, packs: u32, strings_at: u64, strings_len: u64) -> Vec<u8> {
        use ps5_dump_forge_lz4::format::*;
        let mut h = vec![0u8; 128];
        h[..8].copy_from_slice(PAK_MAGIC);
        for (at, v) in [
            (8, PAK_VERSION),
            (12, 128),
            (20, ENDIAN_MARKER),
            (56, packs),
        ] {
            h[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        for (at, v) in [(60, FILE_RECORD), (64, CHUNK_RECORD), (68, PACK_RECORD)] {
            h[at..at + 4].copy_from_slice(&(v as u32).to_le_bytes());
        }
        h[40..48].copy_from_slice(&files.to_le_bytes());
        h[96..104].copy_from_slice(&strings_at.to_le_bytes());
        h[104..112].copy_from_slice(&strings_len.to_le_bytes());
        let crc = pak_header_crc(&h).unwrap();
        h[116..120].copy_from_slice(&crc.to_le_bytes());
        h
    }

    #[test]
    fn a_huge_declared_manifest_is_not_read_whole() {
        use ps5_dump_forge_lz4::MANIFEST;
        let tree = |h: Vec<u8>, size| {
            Header(
                h,
                vec![ps5upload_fpkg::source::SourceFile {
                    path: MANIFEST.into(),
                    size,
                }],
            )
        };
        let big = 600 << 20;
        // Declares 600 MiB, the file is tiny: damaged, one header read.
        let mut t = tree(header(10, 2, 128, big), 128);
        assert!(read_manifest(&mut t, FULL_PARSE_BYTES).is_err());
        // Declares 600 MiB and is that long: the header counts only.
        let mut t = tree(header(10, 2, 128, big - 128), big);
        let (p, large) = read_manifest(&mut t, FULL_PARSE_BYTES).unwrap();
        assert!(large);
        assert_eq!(
            (p.files, p.volumes, p.packed_files, p.stored_percent),
            (10, 2, None, None)
        );
        // Bad version right after the magic: damaged, not read whole.
        let mut h = header(10, 2, 128, big - 128);
        h[8] = 9;
        assert!(read_manifest(&mut tree(h, big), FULL_PARSE_BYTES).is_err());
    }
}
