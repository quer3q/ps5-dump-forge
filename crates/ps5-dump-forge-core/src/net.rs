//! Paths on another device: a paired Forge (`flink://`) or a server (`ftp://`, `smb://`, `nfs://`), all
//! served through [`crate::remote`]. Everything that treats them apart from a local path asks
//! here, so every scheme is read and refused alike.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

use anyhow::bail;
use ps5upload_fpkg::source::SourceTree;

use crate::remote::is_remote;

/// Whether `path` is on another device (`flink://`, `ftp://`, `smb://` or `nfs://`).
pub fn is_network(path: &Path) -> bool {
    is_remote(path)
}

/// An error unless `path` is local: what changes a source can't change one on another device.
pub fn refuse(path: &Path, what: &str) -> anyhow::Result<()> {
    if crate::remote::is_nas(path) {
        bail!(
            "{}: a game on a NAS is read only: {what} needs a local copy (convert or extract it first)",
            path.display()
        );
    }
    if is_remote(path) {
        bail!(
            "{}: a game on another device is read only here: {what} needs a local copy (convert \
             or extract it first)",
            path.display()
        );
    }
    Ok(())
}

/// Why an output can't go to `path`, when it is on another device.
// A conversion's output goes to another device through `remote::output_path`, an Extract
// through `remote::RemoteDir`; what reaches here is a default output beside a game on another
// device (or an in-place job's), which isn't offered.
pub(crate) fn output_refusal(path: &Path) -> Option<String> {
    is_remote(path).then(|| {
        format!(
            "{}: nothing is written beside a game on another device: choose an output folder (here or on a paired device)",
            path.display()
        )
    })
}

/// Whether something is at `path` (local, or on another device); never followed.
pub(crate) fn exists(path: &Path) -> bool {
    if is_remote(path) {
        crate::remote::exists(path)
    } else {
        path.symlink_metadata().is_ok()
    }
}

/// Whether a network path is a folder ([`is_network`] paths only).
pub(crate) fn is_dir(path: &Path) -> anyhow::Result<bool> {
    crate::remote::is_dir(path)
}

/// (length, modification time) of a network path, for [`crate::convert::SourceStamp`].
pub(crate) fn seen(path: &Path) -> anyhow::Result<(u64, Option<SystemTime>)> {
    crate::remote::seen(path)
}

/// The source at a network path (junk filtering is the caller's).
pub(crate) fn open(
    path: &Path,
    kind: crate::convert::Kind,
    cancel: &AtomicBool,
) -> anyhow::Result<Box<dyn SourceTree>> {
    crate::remote::open(path, kind, cancel)
}

/// The file `rel` under the network folder `path` when it is a file of at most `max` bytes.
pub(crate) fn read_small(path: &Path, rel: &str, max: u64) -> anyhow::Result<Option<Vec<u8>>> {
    crate::remote::read_small(path, rel, max)
}

/// One folder of a network path, as [`crate::list_dir`] lists a local one.
pub(crate) fn list_dir(path: &Path) -> anyhow::Result<crate::Listing> {
    crate::remote::list_dir(path)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;

    use crate::convert::Kind;
    use crate::jobs::Ctx;
    use crate::remote::tests::peer;
    use crate::{ConvertRequest, Edit, Event, Format, JobReport};

    fn put(root: &std::path::Path, rel: &str, bytes: &[u8]) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    /// A minimal game (as tests/core.rs's): a file longer than a read window, an empty file,
    /// an empty folder and junk.
    fn game(root: &std::path::Path) -> Vec<u8> {
        put(root, "eboot.bin", b"\x7fELF fake eboot");
        put(
            root,
            "sce_sys/param.json",
            br#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-TESTTESTTESTTEST"}"#,
        );
        let big: Vec<u8> = (0..9 * 1024 * 1024 + 7).map(|i| (i % 251) as u8).collect();
        put(root, "data/big.bin", &big);
        put(root, "data/zero.bin", b"");
        std::fs::create_dir_all(root.join("data/empty")).unwrap();
        put(root, ".DS_Store", b"junk");
        big
    }

    fn request(source: &std::path::Path, format: &str, output: &std::path::Path) -> ConvertRequest {
        let mut req: ConvertRequest = serde_json::from_value(serde_json::json!({
            "source": source, "format": format, "output": output
        }))
        .unwrap();
        req.full_verify = true;
        req
    }

    fn convert(req: &ConvertRequest) -> anyhow::Result<JobReport> {
        let cancel = AtomicBool::new(false);
        let emit: Box<dyn Fn(Event) + Send + Sync> = Box::new(|_| {});
        crate::convert::run(req, &Ctx::new(1, &emit, &cancel))
    }

    #[test]
    fn a_game_on_a_peer_inspects_converts_and_lists() {
        let dir = crate::test_dir("net-peer-game");
        let big = game(&dir.join("src"));
        let (root, _) = peer(&dir.join("src"));
        let url = root.url();

        // The picker: the peer's share root.
        let l = crate::list_dir(&url).unwrap();
        assert_eq!(l.path, url);
        assert_eq!(l.parent, None);
        let names: Vec<&str> = l.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["data", "sce_sys", ".DS_Store", "eboot.bin"]);
        assert_eq!(l.entries[1].path, root.join("sce_sys").url());
        assert_eq!(Kind::of(&url).unwrap(), Kind::Folder);

        let i = crate::inspect(&url).unwrap();
        assert_eq!(i.title_id.as_deref(), Some("PPSA01234"));
        assert_eq!(i.empty_dirs, ["data/empty"]);
        assert!(i.files.iter().all(|f| f.path != ".DS_Store"));

        // Into a local folder, and a local image.
        let out = dir.join("out");
        convert(&request(&url, "folder", &out)).unwrap();
        assert_eq!(std::fs::read(out.join("data/big.bin")).unwrap(), big);
        assert!(out.join("data/empty").is_dir());
        assert!(!out.join(".DS_Store").exists());
        let image = dir.join("game.exfat");
        convert(&request(&url, "exfat", &image)).unwrap();

        // An image on a peer is read through it.
        std::fs::create_dir_all(dir.join("images")).unwrap();
        std::fs::rename(&image, dir.join("images/game.exfat")).unwrap();
        let (images, _) = peer(&dir.join("images"));
        let at = images.join("game.exfat").url();
        assert_eq!(Kind::of(&at).unwrap(), Kind::Exfat);
        let i = crate::inspect(&at).unwrap();
        assert_eq!(i.title_id.as_deref(), Some("PPSA01234"));
        let back = dir.join("back");
        convert(&request(&at, "folder", &back)).unwrap();
        assert_eq!(std::fs::read(back.join("data/big.bin")).unwrap(), big);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_image_is_written_on_a_peer() {
        let dir = crate::test_dir("net-peer-output");
        let src = dir.join("src");
        let big = game(&src);
        std::fs::create_dir_all(dir.join("dest/out")).unwrap();
        put(&dir, "dest/taken.exfat", b"keep");
        let (root, fake) = peer(&dir.join("dest"));
        let has = |e: anyhow::Error, what: &str| {
            assert!(format!("{e:#}").contains(what), "{e:#}");
        };

        // Each image format, fully verified, then read back from the peer.
        for format in ["exfat", "ffpkg", "ffpfs", "ffpfsc"] {
            let out = root.join(&format!("out/game.{format}")).url();
            let report =
                convert(&request(&src, format, &out)).unwrap_or_else(|e| panic!("{format}: {e:#}"));
            assert_eq!(report.output, out);
            assert!(report.checks.iter().any(|c| c.starts_with("published")));
            let back = dir.join(format!("back-{format}"));
            convert(&request(&out, "folder", &back)).unwrap();
            assert_eq!(std::fs::read(back.join("data/big.bin")).unwrap(), big);
        }
        // A .pkg too, the builder writing through the peer (its in-place spool and self-check
        // read it back there); its bytes, converted back here, hold the game.
        let out = root.join("out/game.pkg").url();
        let report = convert(&request(&src, "pkg", &out)).unwrap();
        assert!(report.checks.iter().any(|c| c.starts_with("published")));
        let local = dir.join("game.pkg");
        std::fs::write(&local, fake.file("out/game.pkg").unwrap()).unwrap();
        let back = dir.join("back-pkg");
        convert(&request(&local, "folder", &back)).unwrap();
        assert_eq!(std::fs::read(back.join("data/big.bin")).unwrap(), big);

        // A generated name skips one taken on the peer.
        let generated =
            || crate::generated_output(&src, Format::Exfat, &root.join("out").url(), &[]).unwrap();
        let first = generated();
        convert(&request(&src, "exfat", &first)).unwrap();
        let second = generated();
        assert!(second.to_string_lossy().ends_with("-2.exfat"), "{second:?}");

        // A name taken there is never replaced.
        let taken = root.join("taken.exfat").url();
        has(
            convert(&request(&src, "exfat", &taken)).unwrap_err(),
            "already exists",
        );
        let (part, file) =
            crate::remote::RemotePart::create_file(&root.join("taken.exfat.1-1.part").url())
                .unwrap();
        file.finish().unwrap();
        has(part.publish(&taken).unwrap_err(), "renaming");
        assert_eq!(fake.file("taken.exfat").unwrap(), b"keep");

        // Cancelled while writing: no part stays on the peer.
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let emit: Box<dyn Fn(Event) + Send + Sync> = Box::new(move |e| {
            if let Event::Progress { stage, done, .. } = e
                && stage == "write"
                && done > 0
            {
                flag.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        });
        let req = request(&src, "ffpkg", &root.join("out/cancelled.ffpkg").url());
        has(
            crate::convert::run(&req, &Ctx::new(1, &emit, &cancel)).unwrap_err(),
            "cancel",
        );
        let created = fake.created.lock().unwrap().clone();
        assert!(
            created
                .iter()
                .any(|p| p.starts_with("out/cancelled.ffpkg."))
        );
        assert!(
            fake.paths().iter().all(|p| !p.ends_with(".part")),
            "{:?}",
            fake.paths()
        );

        // No room: refused, nothing created.
        *fake.free.lock().unwrap() = Some(1 << 20);
        has(
            convert(&request(&src, "exfat", &root.join("out/full.exfat").url())).unwrap_err(),
            "not enough space",
        );
        *fake.free.lock().unwrap() = None;
        let made = fake.created.lock().unwrap().len();
        assert_eq!(made, created.len());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_peer_is_read_only_here() {
        let dir = crate::test_dir("net-peer-refusals");
        game(&dir.join("src"));
        put(&dir, "src/x.pkg", b"not a package");
        put(&dir, "src/x.zip", b"not an archive");
        let (root, _) = peer(&dir.join("src"));
        let url = root.url();
        let has = |e: anyhow::Error, what: &str| {
            assert!(format!("{e:#}").contains(what), "{e:#}");
        };

        // No default output beside a peer's game.
        has(
            crate::inspect::default_output(&url, Format::Exfat, std::path::Path::new(""), None)
                .unwrap_err(),
            "nothing is written beside a game on another device",
        );
        // A .pkg or an archive there isn't read.
        let pkg = root.join("x.pkg").url();
        has(
            crate::convert::open_source(&pkg, Kind::of(&pkg).unwrap(), &AtomicBool::new(false))
                .err()
                .unwrap(),
            "can't be read yet",
        );
        has(
            Kind::of(&root.join("x.zip").url()).unwrap_err(),
            "an archive on another device can't be read yet",
        );
        // Browsed, never changed: edits, LZ4 in place, delete, move.
        let mut req = request(&url, "folder", &dir);
        req.edits = vec![Edit::Delete {
            path: "data/zero.bin".into(),
        }];
        let findings = crate::edit_job::request_findings(&req);
        assert!(
            findings[0].contains("browsed but not edited"),
            "{findings:?}"
        );
        // An offline peer's image gets the same refusal, not a connection error.
        let gone = PathBuf::from("flink://ffffffffffffffffffffffffffffffff/g.exfat");
        let mut req = request(&gone, "exfat", &dir);
        req.edits = vec![Edit::Delete {
            path: "data/zero.bin".into(),
        }];
        let check = crate::Jobs::new(|_| {}).edit_check(&req);
        for findings in [&check.in_place, &check.rebuild] {
            assert!(
                findings.len() == 1 && findings[0].contains("browsed but not edited"),
                "{check:?}"
            );
        }
        let mut req = request(&dir.join("src"), "folder", &dir);
        req.edits = vec![Edit::Add {
            from: root.join("eboot.bin").url(),
            to: "eboot2.bin".into(),
        }];
        // Adding from a peer is fine (edit.rs and edit_job.rs test it).
        assert_eq!(
            crate::edit_job::request_findings(&req),
            Vec::<String>::new()
        );
        let jobs = crate::Jobs::new(|_| {});
        let refused = |r: Result<(), crate::DeleteError>| {
            assert!(
                matches!(&r, Err(crate::DeleteError::Refused(why)) if why.contains("another device")),
                "{r:?}"
            );
        };
        refused(jobs.delete_path(&root.join("eboot.bin").url(), &[]));
        refused(jobs.move_path(&root.join("eboot.bin").url(), &dir.join("e"), false, &[]));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
