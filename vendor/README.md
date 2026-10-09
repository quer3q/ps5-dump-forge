# Vendored crates

`ps5upload-fpkg` (FPKG builder and readers) and `ps5upload-pkg` (UFS2 reader, package
inspection), taken from ps5upload with a small local patch series on top.

| | |
|---|---|
| Upstream | https://github.com/phantomptr/ps5upload |
| Tag / commit | `v6.1.2` / `4eaa6af` ("6.1.2: game-folder uploads no longer freeze on the console (#395)") |
| Path upstream | `engine/crates/ps5upload-fpkg`, `engine/crates/ps5upload-pkg` |
| License | GPL-3.0-or-later (see `LICENSE-ps5upload`, and the note below) |
| Edition | 2021, as upstream, on purpose: it keeps the diff to upstream small |

The trees here are upstream plus every patch in `patches/`, in order. Nothing else differs.

## Patches

Paths in each patch are relative to this folder (`a/ps5upload-fpkg/...`). Each patch carries its
own tests.

| Patch | What it does |
|---|---|
| `0001-cargo-pins.patch` | Pins `edition`, `license` and `version` in both `Cargo.toml`s instead of inheriting them from upstream's workspace. |
| `0002-build-from-source-tree.patch` | Splits the build into `build::prepare(tree, request, control) -> Prepared` and `build::write_package(prepared, tree, out_file, out_path, progress, control)`, which writes into a caller-owned file (no `.partial`, no rename) and verifies it. `Prepared` exposes the effective manifest (`ManifestEntry { path, size, origin: Origin }`), the container-only artwork, empty dirs, content id, warnings, estimated size (now covering the container's payloads, per-block tables, per-file/per-directory records and the `param.json` fields `pfsimage.xml` repeats), `generated_bytes` and `read_range` (the packaged bytes of any manifest file). Files read whole (`param.json`, container payloads) are bounded to 256 MiB together, by declared size, before any is read. `build`, `build_controlled` and `build_in_memory` now run on the two. |
| `0003-production-request.patch` | `BuildRequest::production(source, output_dir)`: PlaintextNoAuth, Kraken, Balanced, Stored metadata, default chunks, and `env_overrides: false`, so no `PS5UPLOAD_FPKG_*` variable is read, neither in the constructor nor during the build (`PARAM_FILE`, `SPOOL_DIR`, `DRM_TYPE`, `KRAKEN_STORE`). `BuildRequest::new` is unchanged, except that a `PARAM_FILE` replacement is held to the same 256 MiB bound. |
| `0004-compression-thread-cap.patch` | `BuildRequest::threads: Option<usize>` (`None`/`Some(0)` = every core, larger caps clamp to the core count) sizes the Kraken compression pool. The output is the same for every cap. |
| `0005-cancellation.patch` | `Error::Cancelled`; `verify::verify_streaming_controlled(path, passcode, progress, cancel)` (`verify_streaming` wraps it) and `verify::verify_file_controlled(PkgFile, ...)` check the flag on every block read, the sweep and the CRC table; `write_package` verifies through the caller's handle, not by reopening the path. `prepare` checks it after each stage callback, per file (SELF headers, module marks) and between container payload reads; compression checks it per block in the producer and in the pool; the streaming writer checks it per data and metadata block, between sections and before the final sync. Not covered: `source::readiness`'s bounded AMPR scan (at most ~80 MiB read). |
| `0006-reader-empty-dirs.patch` | `ExFatSource` and `Ufs2Source` override `SourceTree::empty_dirs()` (directories with nothing kept under them after junk filtering), as `FolderSource` does. Adds `ExFat::walk_tree`. |
| `0007-reader-bounds.patch` | UFS2: `list_dir` reads up to `ufs2::MAX_DIR_BYTES` (64 MiB, was 1 MiB) and `ufs2::MAX_DIR_ENTRIES` (1,000,000) entries; `read_inodes` refuses runs over `ufs2::MAX_INODE_RUN` (8192) and the source walker splits its runs to fit; a directory linked twice (cycle) is an error. exFAT: clusters over the specification's 32 MiB are refused (two in-range shifts could make 128 GiB); a directory cycle (two directories at one cluster) is an error; a file larger than the volume is refused at the walk. Both walkers: at most a million entries listed in all (counted as each directory is listed, so pending ancestors count too), and 256 MiB of retained path bytes. |
| `0008-kraken-describe-bounds.patch` | `kraken_image::describe` checks every table against the blob before slicing and refuses blocks at or past the mount; `decode_described` refuses blocks longer than 256 KiB, stored spans over 512 KiB or outside the image; the Kraken decoder refuses an LZ first half shorter than its 8-byte seed and a Huffman code length past 11 (both used to panic). Truncated, corrupt and randomly damaged descriptors are errors, never panics. |
| `0009-source-tree-send.patch` | `SourceTree: Send`, so `Box<dyn SourceTree>` from `source::open` moves to a worker thread. Chosen over an `open_send` twin because every tree already was `Send`. |
| `0010-junk-trashes.patch` | `.Trashes` (macOS creates it on exFAT volumes) is junk, case-insensitively, like `.Spotlight-V100`. |
| `0011-strict-names.patch` | Names are read as they are on disk. UFS2 `list_dir` and the exFAT walk refuse a name that is not UTF-8 / has an unpaired UTF-16 surrogate, or holds a NUL or a `/`, with `Ufs2Error::BadName` / a Format error naming the directory and the raw bytes or units in hex (they were decoded lossily, UFS2 trimmed trailing NULs, and a `/` became a subfolder). A live UFS2 entry whose name runs past its record is `BadDirEntry` (was skipped). `.` and `..` are still skipped. `Ufs2Source` prefixes `list_dir` errors with the directory's path. |
| `0012-ufs2-no-silent-drops.patch` | `Ufs2Source` no longer drops entries: an inode the image cannot produce (out of range, past the end) fails the walk naming the entry (it dropped the entry's whole run), and entries that are neither files nor directories (symlink, device, FIFO, socket, whiteout, free inode) are refused together after the walk, the first 50 by path and kind, the rest counted. |
| `0013-header-bounds.patch` | `cnt::read` checks its on-disk sums (`cnt_offset + table`, body offset + size; an overflow panicked in debug builds, found by the `fih_cnt` fuzz target) and bounds what it reads: at most `cnt::MAX_ENTRIES` (4096) entries and `cnt::MAX_BYTES` (1 GiB) of container, before allocating. `header_rollup_ok`, `sc_entries2_ok` and `body_digest_ok` slice through checked sums. `fih::parse` refuses a `pfs_offset + pfs_size` that overflows (readers address `pfs_offset + i * BLOCK`). `naps::parse` holds the outer block count to the blob's bytes before sizing a vector from it. |
| `0014-fakelib2.patch` | `is_fakelib` also matches a top-level `fakelib2/` (ShadowMountPlus 1.7's exclusive backport folder), case-insensitively like `fakelib/`, so its libraries are packaged byte for byte and never "repaired"; `data/fakelib2/...` still is not one. |
| `0015-ufs2-range-reads.patch` | UFS2 `read_range` reads blocks that lie back to back on disk as one run (one seek and one read, bounded by the requested length; a hole, a jump or a bad pointer ends the run and is met next, as before), and `block_ptr` reads a whole pointer block once and keeps the last one per indirection depth, keyed by fragment, instead of a seek and an 8-byte read per level per block. Found on hardware: a `.ffpkg` → `.exfat` conversion of an 87 GB image ran at ~105 MB/s on a PS5, ~6 syscalls per 64 KiB block. An 8 MiB read of a contiguous file is now 2 reads at 64 KiB blocks (single indirect; was 256) and 3 at 16 KiB blocks in the double-indirect range (was 1536). Bytes and errors match a block at a time, except that a pointer block cut short by a truncated image now fails even when the entries a range needs are present (`read_file` already did). Every byte offset `read_range` and `block_ptr` compute from a pointer is checked (fragment × size, + offset, + length) and must end inside the declared image size, else `BlockOutOfRange`: with a forged `fs_size`, a huge pointer's saturated offset wrapped to byte 0 in release builds (and panicked with overflow checks on). |
| `0016-kraken-sparse-extents.patch` | `kraken_image::describe` reads the boundary table's kind byte: a 0x40 boundary before the mount's end opens a sparse extent (nothing stored, no record) up to the next boundary, returned as zero blocks of at most 256 KiB (`stored_len` 0), which `decode_described` decodes to zeros; a block stored in no bytes without the whole sparse sentinel (`stored_at`, `even_len` 0, no LZ flags, no mode bits) is refused there. A real record whose stored span is empty is refused, and so is a boundary table whose mount end disagrees with the ublock count (ceil(mount / 256 KiB)) or that has a boundary past the mount's end, so a tiny descriptor cannot ask for millions of zero blocks. The walk is O(n log n) in boundaries: the boundary and sparse tables are sorted and looked up by binary search (with linear scans 100,000 boundaries took 3.9 s, extrapolated ~1.5 h for the ~5.6M a 32 MiB descriptor holds; 4M now take 0.24 s). Found in a third-party package: a 256 KiB hole between two files, which shifted every later block. `layout` is `assemble` plus its unchanged read-back check. |
| `0017-kraken-entropy-halves.patch` | `decode_described` decodes a non-LZ half stored shorter than its logical size as one bare entropy array (`kraken::decode_entropy_half`: raw or Huffman, consumed exactly), as a real Oodle encoder writes them (a single-symbol table for a zero half; full tables otherwise; 1,330 of 717,081 blocks in a third-party package); such halves used to go to the LZ decoder. A block of at most 128 KiB with bytes past its even half is refused. Also: `decode_half` requires the excess streams to meet, and a single-symbol Huffman table must fill its body exactly. |
| `0018-pic-png-entries.patch` | `pic1.png` (0x1006) and `pic2.png` (0x2040) join `PRESENTATION`, `CONTAINER_ONLY` and the system general digest (`ids::SYSTEM_DIGEST_IDS`, id order): a folder that has them builds a package that carries them as container entries, like `pic0.png`, and a reader that merges the container gets them back under `sce_sys/`. A third-party package carries both, outside its image, and its system digest matches only with both in it (before, our verifier reported that digest as wrong). Ids and names from that package's name table. |
| `0019-kraken-decode-speed.patch` | Decoder speed, same bytes: a match copies eight bytes at a time and delta literals add eight at a time (every distance is at least 8, so a word never reads a byte not yet written); the Huffman reader refills a whole word while eight bytes of its stream are left (byte by byte at the end, as before) and fills its output by stream rotation instead of `i % 3`; `decode_described` decodes from borrowed halves (`kraken::decode_parts`) instead of copying each into a `Half`. With the reader's parallel decode this took a 188 GB third-party package from 403 to 1,440 MB/s (4 MiB reads, 14 cores); every one of its 521 files hashes as before. |
| `0020-parallel-write-and-sweep.patch` | The write stage of a compressed build reads its spooled image back 8 MiB at a time and digests (and, for a native image, encrypts) the blocks on every core (`stream::digest_blocks`), each block's SHA3 taken once for both `imagedigs` and the block list (it was taken twice, one block at a time). The verifier's outer-block sweep (`verify::failing_blocks`) and its `playgo-chunk.crc` recomputation (`si::chunk_crc_table_controlled`) read 8 MiB at a time and work on every core. Same bytes, digests and verdicts. Found on an 89 GB build: after a ~4 min compress, the write and verify stages ran on one core. |
| `0021-sampled-self-check.patch` | `BuildControl::sample: Some(seed)` makes the built package's self-check (`verify::verify_file_sampled`) read only a seeded sample of its outer blocks and of the `playgo-chunk.crc` table: the first, the last and 1% of the other 8 MiB batches, at most 128 (1 GiB), SplitMix64 as in Forge's fast verification. Every header, container, digest-table, inode and flat-path check still runs. `None` (the default, and `verify_file_controlled`) sweeps every block as before. For a caller that checks the package's files itself: Forge's fast verification, which made a `.pkg` re-read all of itself twice. |
| `0022-exfat-cluster-runs.patch` | exFAT `read_stream` reads clusters that follow each other on disk as one run (a contiguous stream always does; a chained one while its FAT links are consecutive), bounded by the requested length; every cluster is checked before the read, as before, and a contiguous stream's next cluster number is a checked add. Same bytes and errors. Before, it read one 64 KiB cluster per call: a cached `.exfat` read 10.7 GB/s (UFS2 21), and inside a `.ffpfsc` every call decoded a single PFSC block, so the reader could not decode them in parallel (665 MB/s; 3.2 GB/s with this). |

## License file

Upstream's `LICENSE` at `4eaa6af` is not the license text: it holds the GPLv3 title and
preamble opening, then the placeholder "[Full GPLv3 text would go here - truncated for
brevity]", then the usual "either version 3 of the License, or (at your option) any later
version" notice. `LICENSE-ps5upload` here is the full, unmodified GPL-3.0 text instead (the same
file as this repository's `LICENSE`). The grant stays GPL-3.0-or-later, as upstream's notice
and both `Cargo.toml`s say.

## Checking and regenerating

The patches apply, in order, to a pristine upstream checkout and reproduce this folder exactly.
From this folder:

```sh
git clone https://github.com/phantomptr/ps5upload /tmp/ps5upload
git -C /tmp/ps5upload checkout 4eaa6af
rm -rf /tmp/series && mkdir /tmp/series
cp -R /tmp/ps5upload/engine/crates/ps5upload-fpkg /tmp/ps5upload/engine/crates/ps5upload-pkg /tmp/series/
(cd /tmp/series && git init -q && git add -A && git commit -qm pristine)
for p in patches/*.patch; do
    (cd /tmp/series && git apply "$OLDPWD/$p" && git add -A && \
        git commit -qm "$(basename "$p" .patch | cut -d- -f2-)")
done
diff -r /tmp/series/ps5upload-fpkg ps5upload-fpkg && diff -r /tmp/series/ps5upload-pkg ps5upload-pkg
```

`/tmp/series` now holds the series as one commit per patch. To change patch N, copy the edited
crates over `/tmp/series`, `git commit --fixup <commit of patch N>`, then
`GIT_SEQUENCE_EDITOR=: git rebase -i --autosquash <pristine commit>`; to add a patch, commit
it on top. Then write every patch again, each against its parent:

```sh
cd /tmp/series && n=0
for c in $(git rev-list --reverse HEAD~$(($(git rev-list --count HEAD) - 1))..HEAD); do
    n=$((n + 1))
    git diff --histogram "$c~1" "$c" > "$OLDPWD/patches/$(printf %04d $n)-$(git log -1 --format=%s "$c" | tr ' ' -).patch"
done
```

Every patch state builds and passes its own tests; check with the gates below at each commit
when the series changes. Gates: `cargo fmt -p ps5upload-fpkg -p ps5upload-pkg --check`,
`cargo clippy -p ps5upload-fpkg -p ps5upload-pkg --all-targets` with no warnings, and
`cargo test -p ps5upload-fpkg -p ps5upload-pkg --release`.
