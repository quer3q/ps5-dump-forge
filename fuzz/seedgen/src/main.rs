//! Seed corpora for the fuzz targets: one small valid input per target, built with this
//! project's own writers, so the fuzzer starts past every magic number and checksum.
//!
//!   cargo run --release --manifest-path fuzz/Cargo.toml -p ps5-dump-forge-fuzz-seedgen -- fuzz/corpus
//!
//! (`fuzz/seed.sh` does that.) Images are stored in the sparse encoding of `src/sparse.rs`.

#[path = "../../src/sparse.rs"]
#[allow(dead_code)]
mod sparse;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use ps5upload_fpkg::build::{self, BuildRequest};
use ps5upload_fpkg::kraken::{self, Half};
use ps5upload_fpkg::kraken_image::{self, KrakenImage, Owner, StoredBlock};
use ps5upload_fpkg::{ffpfsc, source};

type Res<T> = Result<T, Box<dyn std::error::Error>>;
type KrakenSample = (Vec<u8>, Vec<u8>, Vec<Vec<u8>>);

fn main() -> Res<()> {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("usage: seedgen <corpus-dir>")?,
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mini = root.join("vendor/ps5upload-fpkg/tests/fixtures/mini.exfat");
    let work = std::env::temp_dir().join(format!("forge-seedgen-{}", std::process::id()));
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work)?;
    let result = run(&out, &mini, &work);
    let _ = fs::remove_dir_all(&work);
    result
}

fn run(out: &Path, mini: &Path, work: &Path) -> Res<()> {
    let game = fixture(&work.join("game"))?;
    let cancel = AtomicBool::new(false);

    // exFAT: the upstream fixture (4 KiB clusters, AppleDouble junk) and ours (64 KiB).
    seed(out, "exfat", "mini", &sparse::encode(&fs::read(mini)?))?;
    let exfat = work.join("game.exfat");
    {
        let mut tree = source::open(&game)?;
        let layout = ps5_dump_forge_exfat::plan(&*tree, &Default::default(), &cancel)?;
        let mut file = fs::File::create(&exfat)?;
        ps5_dump_forge_exfat::write(&mut *tree, &layout, &mut file, &cancel, &mut |_, _| {})?;
    }
    seed(out, "exfat", "forge", &sparse::encode(&fs::read(&exfat)?))?;

    // UFS2 (.ffpkg), from the same tree.
    let ffpkg = work.join("game.ffpkg");
    {
        let mut tree = source::open(&game)?;
        let layout = ps5_dump_forge_ufs2::plan(&*tree, &Default::default(), &cancel)?;
        let mut file = fs::File::create(&ffpkg)?;
        ps5_dump_forge_ufs2::write(&mut *tree, &layout, &mut file, &cancel, &mut |_, _| {})?;
    }
    seed(out, "ufs2", "forge", &sparse::encode(&fs::read(&ffpkg)?))?;

    // .ffpfs from the same tree, and .ffpfsc around our exFAT and .ffpfs of it.
    let time = Some(1_700_000_000);
    let ffpfs = work.join("game.ffpfs");
    {
        let mut tree = source::open(&game)?;
        let opts = ps5_dump_forge_pfs::Options { time };
        let layout = ps5_dump_forge_pfs::plan(&*tree, &opts, &cancel)?;
        let mut file = fs::File::create(&ffpfs)?;
        ps5_dump_forge_pfs::write(&mut *tree, &layout, &mut file, &cancel, &mut |_, _| {})?;
    }
    seed(out, "pfs", "forge", &sparse::encode(&fs::read(&ffpfs)?))?;
    for (inner, image) in [("exfat", &exfat), ("ffpfs", &ffpfs)] {
        let len = fs::metadata(image)?.len();
        let wrapped = work.join(format!("game-{inner}.ffpfsc"));
        let mut file = fs::File::create(&wrapped)?;
        let opts = ps5_dump_forge_pfs::WrapOptions {
            threads: 1,
            time,
            ..Default::default()
        };
        let name = format!("PPSA01234.{inner}");
        ps5_dump_forge_pfs::wrap(&name, len, &mut file, &opts, &cancel, |s| {
            std::io::copy(&mut fs::File::open(image)?, s)?;
            Ok(())
        })?;
        let seed_name = format!("ffpfsc-{inner}");
        seed(
            out,
            "pfs",
            &seed_name,
            &sparse::encode(&fs::read(&wrapped)?),
        )?;
    }

    // .ffpfsc around the upstream exFAT fixture (zlib blocks and stored ones).
    let ffpfsc = work.join("mini.ffpfsc");
    let options = ffpfsc::WrapOptions {
        time: Some(1_700_000_000),
        threads: 1,
        ..Default::default()
    };
    ffpfsc::wrap(mini, &ffpfsc, &options, &mut ffpfsc::Control::default())?;
    seed(out, "pfsc", "mini", &sparse::encode(&fs::read(&ffpfsc)?))?;

    // A debug FPKG built from the exFAT fixture, as the app builds them (Kraken, plaintext).
    let pkg_dir = work.join("pkg");
    fs::create_dir_all(&pkg_dir)?;
    let request = BuildRequest {
        time: Some((1_700_000_000, 0)),
        seed: Some([0x5A; 16]),
        threads: Some(1),
        ..BuildRequest::production(mini, &pkg_dir)
    };
    let report = build::build(&request, &mut |_| {})?;
    let pkg = sparse::encode(&fs::read(&report.path)?);
    seed(out, "fih_cnt", "mini", &pkg)?;
    seed(out, "fpkg_source", "mini", &pkg)?;

    // Kraken: a descriptor with real LZ, LZ-delta and raw halves, and the blocks themselves.
    let (blob, image, blocks) = kraken_sample()?;
    let mut input = (blob.len() as u32).to_le_bytes().to_vec();
    input.extend_from_slice(&blob);
    input.extend_from_slice(&image);
    seed(out, "kraken_descriptor", "sample", &input)?;
    for (i, data) in blocks.iter().enumerate() {
        let halves = kraken::encode_block(data);
        let kind = |h: &Half| match h {
            Half::Raw(_) => 0u8,
            Half::Lz(_) => 1,
            Half::LzDelta(_) => 2,
        };
        let mut input = vec![kind(&halves[0]) | halves.get(1).map_or(0, |h| kind(h) << 2)];
        input.extend_from_slice(&((data.len() - 1) as u32).to_le_bytes());
        input.extend_from_slice(&(halves[0].bytes().len() as u32).to_le_bytes());
        for h in &halves {
            input.extend_from_slice(h.bytes());
        }
        seed(out, "kraken_block", &format!("block{i}"), &input)?;
    }
    Ok(())
}

fn seed(out: &Path, target: &str, name: &str, bytes: &[u8]) -> Res<()> {
    let dir = out.join(target);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join(name), bytes)?;
    println!("{target}/{name}: {} bytes", bytes.len());
    Ok(())
}

/// A tiny game: nested dirs, an empty dir and file, a directory with many entries, and a
/// 1 MiB file (UFS2 indirect blocks) that is zeros apart from its ends, so it costs nothing in
/// the sparse seed.
fn fixture(dir: &Path) -> Res<PathBuf> {
    fs::create_dir_all(dir.join("sce_sys"))?;
    fs::create_dir_all(dir.join("data/levels/a"))?;
    fs::create_dir_all(dir.join("data/empty"))?;
    fs::create_dir_all(dir.join("data/many"))?;
    fs::write(
        dir.join("eboot.bin"),
        [0x4F, 0x15, 0x3D, 0x1D, 0, 1, 1, 0x12],
    )?;
    fs::write(
        dir.join("sce_sys/param.json"),
        br#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-FORGEFUZZSEED000"}"#,
    )?;
    fs::write(dir.join("data/levels/a/small.txt"), b"hello\n")?;
    fs::write(dir.join("data/zero.bin"), b"")?;
    for i in 0..40 {
        fs::write(dir.join(format!("data/many/f{i:02}.txt")), format!("{i}\n"))?;
    }
    let mut big = vec![0u8; 1 << 20];
    big[..16].copy_from_slice(b"first block head");
    let n = big.len();
    big[n - 16..].copy_from_slice(b"last block tail!");
    fs::write(dir.join("data/levels/big.bin"), big)?;
    Ok(dir.to_path_buf())
}

/// Blocks shaped like `kraken_image`'s own descriptor test (a two-block file, a short one, the
/// gap and the metadata block), but with real encoded halves, laid out and walked back.
/// Returns (descriptor, stored image, logical bytes of each block).
fn kraken_sample() -> Res<KrakenSample> {
    let text: Vec<u8> = (0..0x60000u32)
        .flat_map(|i| format!("line {} of the quick brown fox\n", i % 977).into_bytes())
        .collect();
    let mut noise = 0x9E37_79B9u32;
    let mut noise = move || {
        noise ^= noise << 13;
        noise ^= noise >> 17;
        noise ^= noise << 5;
        noise as u8
    };
    let file0: Vec<u8> = text[..0x41234].to_vec();
    let file1: Vec<u8> = (0..0x1000).map(|_| noise()).collect();
    let gap = vec![0u8; 0x3ddcc];
    let meta: Vec<u8> = text[0x1000..0x41000]
        .iter()
        .map(|b| b.wrapping_add(1))
        .collect();
    let parts: [(u64, &[u8], Owner); 5] = [
        (0, &file0[..0x40000], Owner::File(0)),
        (0x40000, &file0[0x40000..], Owner::File(0)),
        (0x41234, &file1, Owner::File(1)),
        (0x42234, &gap, Owner::Gap),
        (0x80000, &meta, Owner::Meta),
    ];
    let mut image = Vec::new();
    let mut blocks = Vec::new();
    for (logical, data, owner) in parts {
        let halves = kraken::encode_block(data);
        blocks.push(StoredBlock {
            logical,
            len: data.len() as u32,
            stored_at: image.len() as u64,
            halves: halves
                .iter()
                .map(|h| (h.bytes().len() as u32, h.is_lz()))
                .collect(),
            delta: [0, 1].map(|i| halves.get(i).is_some_and(Half::is_delta)),
            owner,
        });
        for h in &halves {
            image.extend_from_slice(h.bytes());
        }
    }
    let end = image.len() as u64;
    let img = KrakenImage {
        blocks,
        image_len: end.next_multiple_of(ps5upload_fpkg::BLOCK),
        file_digests: Vec::new(),
        file_stored_at: Vec::new(),
        mount_size: 0xc0000,
        data_end: 0x42234,
        meta_base: 0x80000,
    };
    let blob = kraken_image::layout(&img, &[0, 0x41234])?;
    // The seed must decode back to what went in.
    let want: Vec<Vec<u8>> = parts.iter().map(|p| p.1.to_vec()).collect();
    for (b, data) in kraken_image::describe(&blob)?.iter().zip(&want) {
        if &kraken_image::decode_described(&image, b)? != data {
            return Err(
                format!("kraken seed block at {:#x} does not round-trip", b.logical).into(),
            );
        }
    }
    Ok((blob, image, want))
}
