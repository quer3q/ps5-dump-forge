//! Write any source `source::open` reads (a game image or a debug package) out as a folder.
//!
//! `cargo run --release -p ps5upload-fpkg --example tree_extract -- <source> <out-dir>`

use std::io::Write;
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(src), Some(out)) = (args.next(), args.next()) else {
        eprintln!("usage: tree_extract <source> <out-dir>");
        std::process::exit(2);
    };
    let out = PathBuf::from(out);
    let mut tree = match ps5upload_fpkg::source::open(std::path::Path::new(&src)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("open: {e}");
            std::process::exit(1);
        }
    };
    let files: Vec<_> = tree
        .files()
        .iter()
        .map(|f| (f.path.clone(), f.size))
        .collect();
    let empty: Vec<_> = tree.empty_dirs().to_vec();
    let started = std::time::Instant::now();
    let mut total = 0u64;
    for (path, size) in &files {
        let dest = out.join(path);
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        let mut f = std::fs::File::create(&dest).unwrap();
        let mut at = 0u64;
        while at < *size {
            let n = (*size - at).min(8 << 20) as usize;
            let buf = tree.read_range(path, at, n).unwrap_or_else(|e| {
                eprintln!("{path}: {e}");
                std::process::exit(1);
            });
            assert_eq!(buf.len(), n, "{path}: short read at {at}");
            f.write_all(&buf).unwrap();
            at += n as u64;
        }
        total += size;
    }
    for d in &empty {
        std::fs::create_dir_all(out.join(d)).unwrap();
    }
    let secs = started.elapsed().as_secs_f64();
    println!(
        "{} files, {} empty dirs, {total} bytes in {secs:.1}s ({:.0} MB/s)",
        files.len(),
        empty.len(),
        total as f64 / secs / 1e6
    );
}
