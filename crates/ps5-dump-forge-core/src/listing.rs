//! One folder's entries for a file browser ([`list_dir`]); [`list_explorer`] for the PS5's.

use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// At most this many entries per listing.
pub const LIST_MAX: usize = 10_000;

/// A folder's entries, folders first, then by name.
#[derive(Debug, Clone, Serialize)]
pub struct Listing {
    pub path: PathBuf,
    /// The folder above it; `None` at a filesystem root.
    pub parent: Option<PathBuf>,
    pub entries: Vec<ListEntry>,
    /// More than [`LIST_MAX`] entries: the rest are left out.
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListEntry {
    pub name: String,
    /// Where it is; a link's target, resolved, unless it dangles.
    pub path: PathBuf,
    /// A folder (a link: what it points at is one).
    pub dir: bool,
    /// A file's size (a link: what it points at); `None` for a folder.
    pub size: Option<u64>,
    /// The entry itself is a link.
    pub link: bool,
    /// An image ShadowMount+ has mounted ([`list_explorer`] on the PS5 only).
    pub mounted: bool,
}

/// Lists `path` (non-UTF-8 and unreadable entries left out), links resolved unless dangling.
// ponytail: a truncated listing is the first 10,000 the folder yields, sorted, not the first
// 10,000 by name.
pub fn list_dir(path: &Path) -> io::Result<Listing> {
    // A NAS share or a peer.
    if crate::net::is_network(path) {
        return crate::net::list_dir(path).map_err(|e| {
            let kind = e
                .downcast_ref::<io::Error>()
                .map_or(io::ErrorKind::Other, io::Error::kind);
            io::Error::new(kind, format!("{e:#}"))
        });
    }
    // dunce: Windows paths come back as `C:\x`, not `\\?\C:\x` (a network share keeps its
    // verbatim `\\?\UNC\` spelling).
    let dir = dunce::canonicalize(path)?;
    let mut entries = Vec::new();
    let mut truncated = false;
    for entry in std::fs::read_dir(&dir)?.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if entries.len() == LIST_MAX {
            truncated = true;
            break;
        }
        let mut full = dir.join(&name);
        let link = entry.file_type().is_ok_and(|t| t.is_symlink());
        let meta = match std::fs::metadata(&full) {
            Ok(meta) => {
                if link {
                    full = dunce::canonicalize(&full).unwrap_or(full);
                }
                Ok(meta)
            }
            Err(_) => entry.metadata(),
        };
        let Ok(meta) = meta else {
            continue;
        };
        let dir = meta.is_dir();
        entries.push(ListEntry {
            name,
            path: full,
            dir,
            size: (!dir).then_some(meta.len()),
            link,
            mounted: false,
        });
    }
    entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.cmp(&b.name)));
    Ok(Listing {
        parent: dir.parent().map(PathBuf::from),
        path: dir,
        entries,
        truncated,
    })
}

/// [`list_dir`] for the Storage explorer; on the PS5 ShadowMount+'s mount folder hidden (an
/// `InvalidInput` error inside it) and mounted images marked.
pub fn list_explorer(path: &Path) -> io::Result<Listing> {
    #[cfg(target_os = "freebsd")]
    {
        if smp_hidden(path) {
            return Err(smp_hidden_error());
        }
        let listing = list_dir(path)?;
        let stems = crate::edit_job::smp_mounts(
            &mut crate::edit_job::smp_list,
            &mut crate::edit_job::smp_is_mount,
            &|_| true,
        );
        Ok(smp_marked(listing, stems.map_err(|_| ())))
    }
    #[cfg(not(target_os = "freebsd"))]
    list_dir(path)
}

/// Whether `path` is ShadowMount+'s mount folder or inside it, as typed or resolved.
pub(crate) fn smp_hidden(path: &Path) -> bool {
    use crate::library_roots::shadowmnt;
    shadowmnt(&path.components().collect::<PathBuf>())
        || path.canonicalize().is_ok_and(|real| shadowmnt(&real))
}

#[cfg(target_os = "freebsd")]
fn smp_hidden_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "ShadowMount+'s mount folder is not browsed here",
    )
}

/// The explorer's rule: entries in the mount folder left out, files of a mounted stem marked
/// (every image when the mounts couldn't be read: fails closed); links never marked.
#[cfg(any(target_os = "freebsd", test))]
fn smp_marked(mut listing: Listing, mounts: Result<Vec<(String, PathBuf)>, ()>) -> Listing {
    use crate::edit_job::smp_stem;
    listing
        .entries
        .retain(|e| !crate::library_roots::shadowmnt(&e.path));
    for e in listing.entries.iter_mut().filter(|e| !e.dir && !e.link) {
        e.mounted = smp_stem(&e.name).is_some_and(|stem| match &mounts {
            Ok(found) => found.iter().any(|(m, _)| m == stem),
            Err(()) => true,
        });
    }
    listing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorer_hides_shadowmnt_and_marks_mounted_images() {
        assert!(smp_hidden(Path::new("/mnt/shadowmnt")));
        assert!(smp_hidden(Path::new("/mnt/shadowmnt/pfsc/x_0a1b2c3d/")));
        assert!(smp_hidden(Path::new("/mnt/shadowmnt/./x")));
        assert!(!smp_hidden(Path::new("/mnt/shadowmnt2")));
        assert!(!smp_hidden(Path::new("/mnt/usb0")));
        let entry = |name: &str, path: &str, dir: bool, link: bool| ListEntry {
            name: name.into(),
            path: path.into(),
            dir,
            size: (!dir).then_some(1),
            link,
            mounted: false,
        };
        let listing = || Listing {
            path: "/mnt".into(),
            parent: Some("/".into()),
            entries: vec![
                entry("shadowmnt", "/mnt/shadowmnt", true, false),
                entry("into", "/mnt/shadowmnt/pfsc", true, true),
                entry("usb0", "/mnt/usb0", true, false),
                entry("My Game.FFPKG", "/mnt/My Game.FFPKG", false, false),
                entry("My Game.pkg", "/mnt/My Game.pkg", false, false),
                entry("Other.exfat", "/mnt/Other.exfat", false, false),
                entry("My Game.exfat", "/data/My Game.exfat", false, true),
            ],
            truncated: false,
        };
        let shown = |l: Listing| -> Vec<(String, bool)> {
            l.entries.into_iter().map(|e| (e.name, e.mounted)).collect()
        };
        let mounts = Ok(vec![(
            "My Game".into(),
            "/mnt/shadowmnt/My Game_0a1b2c3d".into(),
        )]);
        let s = |n: &str, m: bool| (n.to_string(), m);
        assert_eq!(
            shown(smp_marked(listing(), mounts)),
            [
                s("usb0", false),
                s("My Game.FFPKG", true),
                s("My Game.pkg", false),
                s("Other.exfat", false),
                s("My Game.exfat", false)
            ]
        );
        // Mounts that can't be read lock every image.
        let got = shown(smp_marked(listing(), Err(())));
        assert_eq!(
            got[1..4],
            [
                s("My Game.FFPKG", true),
                s("My Game.pkg", false),
                s("Other.exfat", true)
            ]
        );
    }
}
