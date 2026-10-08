# AGENTS.md

ps5-dump-forge: converts a PS5 game between a folder and every image format ShadowMountPlus 1.7
mounts (`.exfat`, `.ffpkg` UFS2, `.ffpfs` PFS, `.ffpfsc` compressed PFS container; read and write), plus
debug FPKG `.pkg` (create and extract). Rust core, Tauri 2 GUI (`PS5 Dump Forge.app`/`.exe`/`.AppDir`), a
CLI (`ps5-dump-forge`), and a PS5 ELF payload (the CLI's `serve`: the same UI as a web page on port 8095).
v1 ships macOS (one universal zip), Windows x86-64 and arm64, Linux x86-64 and arm64, and the PS5 `.elf`;
hardware tests and real arm64 machine tests are still open. `TODO.md` lists what's left. README.md's "PS5
payload internals" holds the payload's details (harness, HTTP contract, U1–U8 write safety).

## Layout
- `crates/ps5-dump-forge-core`: everything a job does. Own folder scanner (no symlink follow,
  exact names), preflight (names, special files, space, FAT32, depth), jobs (worker thread per job, one at a time, cancel flag,
  `catch_unwind`), `.part` + atomic no-replace rename (macOS exFAT has none: check, then rename), BLAKE3 verification
  (`verify.rs`: 8 MiB slice hashes, fast sample or full), `inspect` (cover, firmware, backport firmware from
  executables, embedded DLC), `prefetch.rs` (read-ahead thread). FreeBSD/PS5 only: `dest.rs` (destination probe,
  U1), `durable.rs` (`sync_retry`, `SyncEvery`, drive-removal check; U4/U5/U7). Its `lib.rs` is the public API
  the CLI, the app and the server build against.
- `crates/ps5-dump-forge-exfat`: forward-only exFAT writer (512 B sectors, 64 KiB clusters), ported
  from MkPFS. `crates/ps5-dump-forge-ufs2`: write-once UFS2 writer for the one SMP geometry
  (`newfs -O 2 -b 65536 -f 65536 -m 0 -S 4096`). Both: `plan` (validates names, sizes the image)
  then `write` (strict offset order into a caller-owned file).
- `crates/ps5-dump-forge-pfs`: PFS writer for `.ffpfs` (MkPFS's unsigned PS5 layout, same `plan`/`write`
  shape), the streaming `.ffpfsc` container writer (`wrap`: the inner image's writer → `Stream` → zlib
  workers → `.part`), and the PFS/PFSC readers (`PfsSource`, `open_ffpfsc`).
- `crates/ps5-dump-forge-fpkg`: streaming `.pkg` reader (`FpkgSource`), CNT entries merged with the
  inner filesystem.
- `crates/ps5-dump-forge-cli`: binary `ps5-dump-forge` (`inspect`, `convert`, `serve`), JSON-lines events.
- `crates/ps5-dump-forge-server`: `serve`, std-only HTTP/1.1 (`http.rs`), routes mirroring the Tauri commands
  (`api.rs`), job table for polling (`table.rs`), roots (`paths.rs`), PS5 tile (`tile.rs`), self copy
  (`self_copy.rs`), `debug.rs` (only with `FORGE_DEBUG_API`). `build.rs` embeds `app/dist-http` and the self copy.
- `ps5/`: the payload's Docker harness: `Dockerfile`, `build.sh`, `linker.sh`, `entry.c` (launch modes,
  logs, `ps5_notify`), `compat.c` (statfs symvers), `launcher.c` (tile registration), `icon0.png`,
  `test-entry.sh`; `shim/` + `x86_64-ps5-freebsd.json` + `prepare-rust-std.py` are ps5-ai-cli's, byte-identical.
- `app/`: React/TS/Vite UI; `app/src-tauri` is the `ps5-dump-forge-gui` crate (binary `Forge`,
  bundled and renamed to "PS5 Dump Forge" via `mainBinaryName`; on Linux `tauri.linux.conf.json`
  overrides it to `ps5-dump-forge-gui`, else the AppImage step would overwrite the CLI's own
  `ps5-dump-forge` binary of the same kebab-case name). `npm run build:http` (Vite mode `http`) builds the
  web UI into `app/dist-http`: `transport-http.tsx` replaces `transport-tauri.tsx`, `Picker.tsx` the native
  dialogs, `main-http.tsx` runs `launcher.ts` (start the payload via elfldr) and `appcache.ts` first.
  `app/scripts/*.mjs`: plain-node checks (`check-paths`, `check-poller`, `check-launcher`, `smoke`).
- `vendor/ps5upload-{fpkg,pkg}`: ps5upload v6.1.2 crates (readers, FPKG builder, verify) plus a
  local patch series in `vendor/patches/`; see `vendor/README.md`. Edition 2021 on purpose.
- `scripts/`: `check-exfat.sh` (fsck_exfat + exfatprogs), `fsck-ufs.sh` (real FreeBSD fsck_ufs in a
  qemu VM under Docker), `check-versions.sh` (Cargo.toml/tauri.conf.json/package.json versions agree,
  and match the tag on a tag build), `release-macos.sh`, `release-windows.sh`, `release-linux.sh`,
  `release-ps5.sh` (builds and checks the `.elf`), `test-freebsd.sh` (core + server tests on FreeBSD UFS,
  FAT32, nullfs in the qemu VM).
  `fuzz/`: cargo-fuzz targets, its own workspace.
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
- The Tauri app has no HTTP sidecar and no `tauri-plugin-fs`; the capability grants only the app's
  commands, dialogs and event listening. The web UI is a separate build served by `serve`.
- Verification: fast by default everywhere (every structural check + seeded BLAKE3 sample of 8 MiB slices),
  full is opt-in (`full_verify`, `--full-verify`, the "Full verification" switch). `.pkg` is always full.
- PS5 payload: one plain `.elf`, no PKG, no signing, no updates (replace the file). FreeBSD 11 ABI via
  `--cfg libc_unstable_freebsd_version="11"` (the env var is ignored by libc ≥ 0.2.187) and struct-size
  asserts in the CLI. `panic=abort`.
- The server has **no protections** by user decision: no pairing, token, Host/Origin/CSRF checks or path
  confinement, and no delete route. Don't re-add them.
- Tile `PDFG00001` "PS5 Dump Forge" (deeplink to `http://127.0.0.1:<port>/`), never written without our owner
  file. Self copy: `ps5/build.sh` builds twice; the release ELF embeds stage 1 and saves it to
  `/data/ps5-dump-forge/ps5-dump-forge.elf`. Launch on open: the AppCache'd page asks elfldr (`GET :9021/<elf>?args=serve`)
  only from the console's own browser at load.
- PS5 writes: a retried fsync fails the job; USB is never hardware-tested, so the destination probe decides
  (never the path or fs name) and fails closed. Debug routes (`FORGE_DEBUG_API`) never ship; `release-ps5.sh`
  refuses them.

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
scripts/check-versions.sh                      # Cargo.toml/tauri.conf.json/package.json versions agree
scripts/release-macos.sh                       # universal zip: PS5 Dump Forge.app + CLI, ad-hoc signed
scripts/release-windows.sh                     # Git Bash on Windows: x86-64 zip + x86-64-webview2 zip, or one arm64 zip
scripts/release-linux.sh                       # Linux: x86-64 or arm64 tarball by host (extracted AppDir + forge.sh + CLI)
cargo run -p ps5-dump-forge-cli -- serve --root <dir>   # the web UI on this computer (cd app && npm run build:http first)
ps5/build.sh                                   # PS5 payload in Docker: target/ps5/ps5-dump-forge.elf
ps5/test-entry.sh                              # host check of entry.c
scripts/release-ps5.sh                         # dist/ps5-dump-forge-<ver>-ps5.elf, checked (SKIP_BUILD=1: check only)
scripts/test-freebsd.sh                        # core + server tests on FreeBSD UFS/FAT32/nullfs (VM; long)
FORGE_DEBUG_API=1 ps5/build.sh                 # bring-up build with POST /api/debug; never released
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
  `.ffpfsc` with each inner format; `.pkg` installs and boots), and the PS5 payload (page loads, a conversion
  on `/data`, Stop).
