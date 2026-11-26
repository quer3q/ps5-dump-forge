//! The app dir: WebView data stays next to the app.

use std::path::{Path, PathBuf};

/// The folder holding the exe, or the one holding `X.app` / the running `.AppImage`
/// (`APPIMAGE` the file, `APPDIR` its mount). Never the working directory.
pub(crate) fn app_dir(exe: &Path) -> PathBuf {
    let var = |k: &str| std::env::var_os(k).map(PathBuf::from);
    // The runtime builds APPDIR from TMPDIR unresolved, while `exe` is canonical: compare resolved.
    let mount = var("APPDIR").map(|m| std::fs::canonicalize(&m).unwrap_or(m));
    app_dir_in(exe, var("APPIMAGE").zip(mount))
}

fn app_dir_in(exe: &Path, appimage: Option<(PathBuf, PathBuf)>) -> PathBuf {
    let dir = exe.parent().unwrap_or(Path::new(""));
    let image = appimage.filter(|(_, mount)| dir.starts_with(mount));
    if let Some(parent) = image.as_ref().and_then(|(file, _)| file.parent()) {
        return parent.to_path_buf();
    }
    let bundle = dir
        .parent()
        .filter(|_| dir.file_name().is_some_and(|n| n == "MacOS"))
        .filter(|contents| contents.file_name().is_some_and(|n| n == "Contents"))
        .and_then(Path::parent)
        .filter(|app| {
            app.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("app"))
        });
    match bundle.and_then(Path::parent) {
        Some(parent) => parent.to_path_buf(),
        None => dir.to_path_buf(),
    }
}
