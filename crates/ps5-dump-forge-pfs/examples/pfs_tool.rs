//! Builds and reads PFS images from the command line, for checks against other tools.
//!
//! ```sh
//! cargo run --release -p ps5-dump-forge-pfs --example pfs_tool -- ffpfs <dir> <out.ffpfs> <time>
//! cargo run --release -p ps5-dump-forge-pfs --example pfs_tool -- ffpfsc <dir> <out.ffpfsc> <time> <exfat|ffpkg|ffpfs>
//! cargo run --release -p ps5-dump-forge-pfs --example pfs_tool -- extract <image> <dir>
//! ```

use std::fs::File;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use ps5_dump_forge_pfs::{Options, PfsSource, WrapOptions, open_ffpfsc, plan, wrap, write};
use ps5upload_fpkg::Result;
use ps5upload_fpkg::source::{FolderSource, SourceTree};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cancel = AtomicBool::new(false);
    let none = &mut |_: u64, _: u64| {};
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["ffpfs", src, out, time] => {
            let mut tree = FolderSource::open(Path::new(src))?;
            let opts = Options {
                time: Some(time.parse().expect("time")),
            };
            let layout = plan(&tree, &opts, &cancel)?;
            let r = write(
                &mut tree,
                &layout,
                &mut File::create_new(out)?,
                &cancel,
                none,
            )?;
            println!("{r:?}");
        }
        ["ffpfsc", src, out, time, inner] => {
            let mut tree = FolderSource::open(Path::new(src))?;
            let time = time.parse().expect("time");
            let opts = WrapOptions {
                time: Some(time),
                ..WrapOptions::default()
            };
            let mut file = File::create_new(out)?;
            let name = format!("IMAGE.{inner}");
            let (_, r) = match inner {
                "exfat" => {
                    let l = ps5_dump_forge_exfat::plan(&tree, &Default::default(), &cancel)?;
                    wrap(&name, l.image_size, &mut file, &opts, &cancel, |s| {
                        ps5_dump_forge_exfat::write(&mut tree, &l, s, &cancel, none).map(drop)
                    })?
                }
                "ffpkg" => {
                    let l = ps5_dump_forge_ufs2::plan(&tree, &Default::default(), &cancel)?;
                    wrap(&name, l.image_size, &mut file, &opts, &cancel, |s| {
                        ps5_dump_forge_ufs2::write(&mut tree, &l, s, &cancel, none).map(drop)
                    })?
                }
                _ => {
                    let l = plan(&tree, &Options { time: Some(time) }, &cancel)?;
                    wrap(&name, l.image_size, &mut file, &opts, &cancel, |s| {
                        write(&mut tree, &l, s, &cancel, none).map(drop)
                    })?
                }
            };
            println!("{r:?}");
        }
        ["extract", image, dir] => {
            let file = Box::new(File::open(image)?);
            let mut tree: Box<dyn SourceTree> = if image.ends_with(".ffpfsc") {
                let (tree, info) = open_ffpfsc(file, image)?;
                println!("{info:?}");
                tree
            } else {
                let src = PfsSource::from_reader(file, image.to_string())?;
                println!("{:?}", src.header());
                Box::new(src)
            };
            let dir = Path::new(dir);
            for d in tree.empty_dirs().to_vec() {
                std::fs::create_dir_all(dir.join(d))?;
            }
            for f in tree.files().to_vec() {
                let path = dir.join(&f.path);
                std::fs::create_dir_all(path.parent().expect("a parent"))?;
                let mut out = File::create_new(&path)?;
                let mut at = 0;
                while at < f.size {
                    let n = (f.size - at).min(8 << 20) as usize;
                    let buf = tree.read_range(&f.path, at, n)?;
                    std::io::Write::write_all(&mut out, &buf)?;
                    at += buf.len() as u64;
                }
            }
            println!("{} files", tree.files().len());
        }
        _ => {
            eprintln!("usage: pfs_tool ffpfs|ffpfsc|extract ... (see the source)");
            std::process::exit(2);
        }
    }
    Ok(())
}
