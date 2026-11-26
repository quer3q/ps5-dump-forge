//! Map package-file offsets to stored inner-image blocks, and extract decoded
//! inner-image logical ranges.
//!
//! ```text
//! cargo run --release -p ps5upload-fpkg --example read_map -- <pkg> [logical-offset ...]
//! cargo run --release -p ps5upload-fpkg --example read_map -- <pkg> \
//!   --logical <logical-offset> <length> <output>
//! cargo run --release -p ps5upload-fpkg --example read_map -- <pkg> --file <file-offset>
//! ```

use ps5upload_fpkg::{cnt, crypto, fih, kraken_image, outer, PkgFile};
use sha2::{Digest, Sha256};

fn hex(s: &str) -> u64 {
    u64::from_str_radix(s.trim_start_matches("0x"), 16).expect("offsets are hexadecimal")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn block_bounds(blob: &[u8]) -> ps5upload_fpkg::Result<Vec<u64>> {
    let word = |at: usize| u64::from_le_bytes(blob[at..at + 8].try_into().unwrap());
    let files = (word(0) & 0xFF_FFFF) as usize + 1;
    let outer = (word(8) & 0xFF_FFFF) as usize;
    let at = 16 + outer * 8;
    let table = blob
        .get(at..at + files * 6)
        .ok_or_else(|| ps5upload_fpkg::Error::Format("layout ends inside its file table".into()))?;
    Ok(table
        .chunks(6)
        .map(|r| u64::from_le_bytes([r[0], r[1], r[2], r[3], r[4], 0, 0, 0]))
        .collect())
}

fn main() -> ps5upload_fpkg::Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    assert!(!args.is_empty(), "usage: read_map <pkg> [options]");
    let path = args.remove(0);
    let (logical_range, file_targets, logical_targets) = match args.first().map(String::as_str) {
        Some("--logical") => {
            assert_eq!(
                args.len(),
                4,
                "usage: read_map <pkg> --logical <offset> <length> <output>"
            );
            let range = (hex(&args[1]), hex(&args[2]), args[3].clone());
            (Some(range), Vec::new(), Vec::new())
        }
        Some("--file") => {
            let mut file_targets = Vec::new();
            let mut i = 0;
            while i < args.len() {
                assert_eq!(
                    args[i], "--file",
                    "usage: read_map <pkg> --file <offset> [--file <offset> ...]"
                );
                let target = args.get(i + 1).expect("--file needs an offset");
                file_targets.push(hex(target));
                i += 2;
            }
            (None, file_targets, Vec::new())
        }
        _ => {
            let targets = if args.is_empty() {
                vec![0x2d2d_0000, 0x2e4e_0000, 0x6e7b_0000, 0x6e7c_0000]
            } else {
                args.iter().map(|s| hex(s)).collect()
            };
            (None, Vec::new(), targets)
        }
    };

    let mut pkg = PkgFile::open(std::path::Path::new(&path))?;
    let head = pkg.read_at(0, fih::HEADER_LEN)?;
    let parsed = fih::parse(&head)?;
    let container = cnt::read(&mut pkg, parsed.cnt_offset)?;
    let image = outer::open(&mut pkg, &parsed, &container, crypto::DEFAULT_PASSCODE)?;
    let nodes = image.dinodes();
    let uroot = nodes.get(2).expect("no uroot inode");
    let mut naps = None;
    for d in image.dirents(uroot) {
        if d.name == "naps_pkg_layout.dat" {
            naps = Some(image.file_data(&nodes[d.ino as usize]));
        }
    }
    let naps = naps.expect("no naps_pkg_layout.dat");
    let blocks = kraken_image::describe(&naps)?;
    let bounds = block_bounds(&naps)?;

    println!("{path}");
    println!(
        "FIH pfs=({:#x}, {:#x}) cnt={:#x} superblock={:#x} game_digest={:02x?}",
        parsed.pfs_offset,
        parsed.pfs_size,
        parsed.cnt_offset,
        u64::from_le_bytes(head[0x20..0x28].try_into().unwrap()),
        &parsed.game_digest[..4]
    );
    println!(
        "descriptor={} bytes descriptions={} bounds={} first={:#x} data_end={:#x} meta_base={:#x} mount={:#x}",
        naps.len(),
        blocks.len(),
        bounds.len(),
        bounds.first().copied().unwrap_or(0),
        bounds[bounds.len() - 3],
        bounds[bounds.len() - 2],
        bounds[bounds.len() - 1]
    );
    for (i, n) in nodes.iter().enumerate() {
        println!(
            "inode {i} size={:#x} stored={:#x} blocks={} direct0={} direct1={}",
            n.size, n.size_compressed, n.blocks, n.direct[0].block, n.direct[1].block
        );
    }

    for target in logical_targets {
        println!("\ntarget {target:#x}");
        let i = blocks.partition_point(|b| b.logical + b.len <= target);
        match blocks.get(i) {
            Some(b) if b.logical <= target && target < b.logical + b.len => println!(
                "  logical block {i}: logical={:#x}..{:#x} stored={:#x}..{:#x} \
                 file={:#x}..{:#x} even={:#x} stored_len={:#x} mode={:#x} \
                 lz=({},{})",
                b.logical,
                b.logical + b.len,
                b.stored_at,
                b.stored_at + b.stored_len,
                parsed.pfs_offset + b.stored_at,
                parsed.pfs_offset + b.stored_at + b.stored_len,
                b.even_len,
                b.stored_len,
                b.mode_bits,
                b.even_lz,
                b.odd_lz
            ),
            _ => println!("  no logical block covers this offset"),
        }
    }

    for target in file_targets {
        println!("\npackage-file target {target:#x}");
        let Some(stored_target) = target.checked_sub(parsed.pfs_offset) else {
            println!(
                "  before the inner image at FIH pfs_offset {:#x}",
                parsed.pfs_offset
            );
            continue;
        };
        let found = blocks.iter().enumerate().find(|(_, b)| {
            b.stored_at <= stored_target && stored_target < b.stored_at + b.stored_len
        });
        match found {
            Some((i, b)) => println!(
                "  stored block {i}: stored={:#x}..{:#x} package_file={:#x}..{:#x} \
                 logical={:#x}..{:#x} stored_len={:#x} even={:#x}",
                b.stored_at,
                b.stored_at + b.stored_len,
                parsed.pfs_offset + b.stored_at,
                parsed.pfs_offset + b.stored_at + b.stored_len,
                b.logical,
                b.logical + b.len,
                b.stored_len,
                b.even_len
            ),
            None => println!(
                "  no stored inner-image block covers file offset {target:#x} \
                 (image-relative {stored_target:#x})"
            ),
        }
    }

    if let Some((start, len, out)) = logical_range {
        let end = start
            .checked_add(len)
            .expect("logical range end overflowed");
        let image = image.file_data(&nodes[3]);
        assert_eq!(image.len() as u64, nodes[3].size, "pfs_image.dat is short");
        assert!(
            end <= image.len() as u64,
            "logical range {start:#x}..{end:#x} is past the image end {:#x}",
            image.len()
        );

        let mut staged = vec![0u8; len as usize];
        let mut pos = start;
        let mut first = blocks.partition_point(|b| b.logical + b.len <= start);
        println!(
            "\nlogical range {start:#x}..{end:#x} ({} bytes)",
            staged.len()
        );
        while pos < end {
            let b = blocks
                .get(first)
                .filter(|b| b.logical <= pos && pos < b.logical + b.len)
                .ok_or_else(|| {
                    ps5upload_fpkg::Error::Format(format!(
                        "no inner-image block covers logical offset {pos:#x}"
                    ))
                })?;
            let stored = image
                .get(b.stored_at as usize..(b.stored_at + b.stored_len) as usize)
                .ok_or_else(|| {
                    ps5upload_fpkg::Error::Format(format!(
                        "stored block {first} runs past pfs_image.dat"
                    ))
                })?;
            let local = kraken_image::DescribedBlock {
                stored_at: 0,
                ..b.clone()
            };
            let plain = kraken_image::decode_described(stored, &local)?;
            assert_eq!(
                plain.len() as u64,
                b.len,
                "decoded block {first} has the wrong length"
            );

            let from = (pos - b.logical) as usize;
            let take = ((end - pos) as usize).min(plain.len() - from);
            staged[(pos - start) as usize..(pos - start) as usize + take]
                .copy_from_slice(&plain[from..from + take]);
            println!(
                "  block {first}: logical={:#x}..{:#x} stored={:#x}..{:#x} \
                 package_file={:#x}..{:#x} copy={:#x}..{:#x} sha256={}",
                b.logical,
                b.logical + b.len,
                b.stored_at,
                b.stored_at + b.stored_len,
                parsed.pfs_offset + b.stored_at,
                parsed.pfs_offset + b.stored_at + b.stored_len,
                pos,
                pos + take as u64,
                sha256_hex(&plain)
            );
            pos += take as u64;
            first += 1;
        }

        std::fs::write(&out, &staged)?;
        println!(
            "  wrote {} bytes to {out}; sha256={}",
            staged.len(),
            sha256_hex(&staged)
        );
    }
    Ok(())
}
