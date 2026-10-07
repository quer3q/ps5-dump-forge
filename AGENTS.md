# AGENTS.md

ps5-dump-forge: converts a PS5 game between a folder and every image format ShadowMountPlus 1.7
mounts (`.exfat`, `.ffpkg` UFS2, `.ffpfs` PFS, `.ffpfsc` compressed PFS container; read and write), plus
debug FPKG `.pkg` (create and extract). Rust core, Tauri 2 GUI (`PS5 Dump Forge.app`) and a CLI
(`ps5-dump-forge`). v1 ships for Apple Silicon macOS only; the code stays portable. `TODO.md` lists what's
left (other platforms, hardware tests, replacing `vendor/`).

## Layout
- `crates/ps5-dump-forge-core`: everything a job does. Own folder scanner (no symlink follow,
  exact names), preflight (names, special files, space, FAT32, depth), jobs (worker thread per job, one at a time, cancel flag,
  `catch_unwind`), `.part` + atomic no-replace rename (macOS exFAT has none: check, then rename), BLAKE3 manifest verification,
  `inspect` (cover, firmware, backport firmware from executables, embedded DLC). Its `lib.rs` is the public API the CLI and the app build against.
- `crates/ps5-dump-forge-exfat`: forward-only exFAT writer (512 B sectors, 64 KiB clusters), ported
  from MkPFS. `crates/ps5-dump-forge-ufs2`: write-once UFS2 writer for the one SMP geometry
  (`newfs -O 2 -b 65536 -f 65536 -m 0 -S 4096`). Both: `plan` (validates names, sizes the image)
  then `write` (strict offset order into a caller-owned file).
- `crates/ps5-dump-forge-pfs`: PFS writer for `.ffpfs` (MkPFS's unsigned PS5 layout, same `plan`/`write`
  shape), the streaming `.ffpfsc` container writer (`wrap`: the inner image's writer → `Stream` → zlib
  workers → `.part`), and the PFS/PFSC readers (`PfsSource`, `open_ffpfsc`).
- `crates/ps5-dump-forge-fpkg`: streaming `.pkg` reader (`FpkgSource`), CNT entries merged with the
  inner filesystem.
- `crates/ps5-dump-forge-cli`: binary `ps5-dump-forge` (`inspect`, `convert`), JSON-lines events.
- `app/`: React/TS/Vite UI; `app/src-tauri` is the `ps5-dump-forge-gui` crate (binary `Forge`,
  bundled and renamed to "PS5 Dump Forge" via `mainBinaryName`).
- `vendor/ps5upload-{fpkg,pkg}`: ps5upload v6.1.2 crates (readers, FPKG builder, verify) plus a
  local patch series in `vendor/patches/`; see `vendor/README.md`. Edition 2021 on purpose.
- `scripts/`: `check-exfat.sh` (fsck_exfat + exfatprogs), `fsck-ufs.sh` (real FreeBSD fsck_ufs in a
  qemu VM under Docker), `release-macos.sh`. `fuzz/`: cargo-fuzz targets, its own workspace.
- `DESIGN.md`: the UI design system (tokens, components, colour/contrast rules, the cover glow);
  `app/src/styles.css` holds the values. Read it before changing the UI.

## Key decisions
- Every reader/writer meets at `ps5upload_fpkg::source::SourceTree` (sizes up front, `read_range`,
  `empty_dirs`, `Send`). Any source converts to any target with no staging.
- Outputs are raw filesystems at byte 0; the extension picks the SMP driver, so it must match.
- Never touch the source: junk (`.DS_Store`, `._*`, `.fseventsd`, ...) is filtered, not deleted.
- Names are never renamed: bad names (non-NFC, exFAT/PFS case collisions, forbidden chars, non-ASCII
  in PFS) fail preflight with every offender listed.
- `.pkg` is built with `BuildRequest::production` only (plaintext `PPRPLAIN-NOAUTH!`, Kraken);
  installing needs kstuff + fpkg-enable + ppr-patch. Debug keys are embedded.
- The UI calls the debug FPKG `.fpkg` (a label only): its files, the `Format`/`Kind` id and the CLI
  flag stay `pkg`. The default target is `.ffpkg` (SMP 1.7's recommendation; `.exfat` is for
  compatibility). Format copy and the About formats table (`app/src/Formats.tsx`) follow
  ShadowMountPlus 1.7's README and release notes.
- No settings and no config file: the UI keeps choices in memory until it closes. Defaults: target
  `.ffpkg`, exFAT inside a `.ffpfsc`, all cores for compression, no extra free space beyond each
  format's built-in spare.
- Progress is one bar per job: `Event::Progress` `done`/`total` span every pass (write, verify,
  ...); `Ctx::expect_rest` renews the estimate as each pass learns its size.
- No HTTP sidecar, no `tauri-plugin-fs`; the capability grants only the app's commands, dialogs
  and event listening.

## Format rules (ShadowMountPlus 1.7)
- Raw filesystem at byte 0, no MBR/GPT; SMP attaches the whole file. The extension picks the driver
  (`.ffpkg` UFS, `.exfat` exFAT, `.ffpfs` PFS, `.ffpfsc` PFSC); an inner image in `.ffpfsc` keeps its extension.
- `sce_sys/param.json` + `eboot.bin` at the image root. Image sizes are multiples of 64 KiB.
- exFAT: 512-byte sectors, 64 KiB clusters (boot-sector shifts 9/7); size as upstream `mkexfat_macos.sh`
  (0.5% spare, 64..512 MiB). The fast path also needs the file directly under `/data` or `/user`.
- UFS2: `newfs -O 2 -b 65536 -f 65536 -m 0 -S 4096`, `-i` = (data + spare)/(entries+2048) rounded down to
  4 KiB, clamped 64K..256K; size = the smallest layout holding the data, 1024 free blocks (64 MiB) and 2048
  spare inodes (no UFS2Tool `-D` +10%); every inode 0777 root:root; the last cylinder
  group must hold its full metadata (UFS2Tool v4.1 wrote past `fs_size` there and its fsck said clean).
- Writable (`image_rw=`) mounts need free blocks/inodes (UFS) or free clusters (exFAT); `.ffpfsc` is always
  mounted read-only.
- PFS (`.ffpfs`): version 2, 64 KiB blocks (the console misreads smaller ones), 32-bit unsigned inodes,
  case-insensitive (mode 0x8), unencrypted; byte-identical to `mkpfs pack folder --raw --no-compress
  --version PS5 --inode-bits 32 --block-size 65536` except that empty directories are kept. Inodes: super-root,
  flat path table, collision resolver (only if two paths' FPT hashes collide), uroot, dirs, files; every node's
  data is contiguous. Files are stored uncompressed (per-file PFSC passes verify but the console misreads it).
  Names: ASCII only, no case collisions, every offender listed.
- `.ffpfsc`: a single-file PFS (MkPFS `pack file` layout) whose one file is a zlib PFSC container of 64 KiB
  blocks around a nested `.exfat`/`.ffpkg`/`.ffpfs` named `<TITLE_ID>.<ext>` (default exFAT, MkPFS's most
  stable). Always PFSC, level 6, a block is kept compressed only if it saves ≥ 5%; zlib from `flate2`
  (`miniz_oxide`; console acceptance is a hardware gate in `TODO.md`). Streamed: the inner
  writer feeds the compressor, which writes the `.part`; no temporary inner image. Free-space preflight asks
  for the worst case (every block raw).
- Generated `.ffpfs`/`.ffpfsc` names are ≤ 63 bytes (SMP fails longer ones with ENAMETOOLONG); the game-name
  part is cut first.
- FPKG: outer PFS plaintext with the `PPRPLAIN-NOAUTH!` marker (native AES-XTS fails the console's auth),
  inner image Kraken-compressed (zlib metadata is rejected by the console, never offer it).

## Commands
```sh
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace --release
(cd app && npm ci && npm run tauri dev)                       # GUI in dev mode
cargo run -p ps5-dump-forge-cli -- convert <game_dir> --to exfat   # or ffpkg | ffpfs | ffpfsc | pkg | folder
cargo run -p ps5-dump-forge-cli -- convert <game_dir> --to ffpfsc --inner exfat   # inner: exfat | ffpkg | ffpfs
cargo run --release -p ps5-dump-forge-pfs --example pfs_tool -- ffpfs <dir> <out> <time>   # for MkPFS byte comparisons
scripts/check-exfat.sh <img.exfat> [src_dir]   # macOS fsck_exfat + mount compare + exfatprogs (Docker)
scripts/fsck-ufs.sh <img.ffpkg>...             # FreeBSD fsck_ufs -n in qemu (Docker, ~1 min)
scripts/release-macos.sh                       # arm64 zip: PS5 Dump Forge.app + CLI, ad-hoc signed
```
`tauri build` needs an absolute `CARGO_TARGET_DIR` if you set one.

## Preferences
- Do not commit, push or stage unless asked.
- Rust 2024 edition, latest stable (`rust-version` tracks it). CI is unpinned and latest on purpose.
- Keep code small and boring (stdlib first, no speculative abstractions; `// ponytail:` notes name
  the ceiling of a deliberate shortcut).
- Changes to `vendor/` go into a new numbered patch in `vendor/patches/` and `vendor/README.md`.
  Replacing `vendor/` with our own code is a future TODO (`TODO.md`), not current work.
- Before release: hardware smoke test per format (SMP 1.7 mounts and boots `.exfat`/`.ffpkg`/`.ffpfs` and
  `.ffpfsc` with each inner format; `.pkg` installs and boots).
