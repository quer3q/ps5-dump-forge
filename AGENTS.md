# AGENTS.md

ps5-dump-forge: converts a PS5 game between a folder and every image format ShadowMountPlus 1.7
mounts (`.exfat`, `.ffpkg` UFS2, `.ffpfs` PFS, `.ffpfsc` compressed PFS container; read and write), plus
debug FPKG `.pkg` (create and extract). Rust core, Tauri 2 GUI (`PS5 Dump Forge.app`/`.exe`/`.AppDir`), a
CLI (`ps5-dump-forge`), and a PS5 ELF payload (the CLI's `serve`: the same UI as a web page on port 8095).
v1 ships macOS (one universal zip), Windows x86-64 and arm64, Linux x86-64 and arm64, and the PS5 `.elf`;
hardware tests and real arm64 machine tests are still open. `TODO.md` lists what's left. `ps5/README.md`
holds the payload's details (harness, HTTP contract and routes, tile/self copy/launch on open, U1–U10
write safety, console speeds).

## Layout
- `crates/ps5-dump-forge-core`: everything a job does. Own folder scanner (no symlink follow,
  exact names), preflight (names, special files, space, FAT32, depth, name limits), jobs (a worker thread
  per job, one heavy job at a time in FIFO order, cancel flag, `catch_unwind`), `.part` + atomic no-replace
  rename (`finalize.rs`; macOS exFAT has none: check, then rename; FreeBSD: hard link where the folder has
  them, U3), BLAKE3 verification (`verify.rs`: 8 MiB slice hashes on a pool of up to 8 threads, fast sample
  or full), `inspect.rs` (cover, firmware, backport firmware from executables, embedded DLC; default and
  generated output names), `prefetch.rs` (read-ahead thread). FreeBSD/PS5 only: `dest.rs` (destination
  probe, U1), `durable.rs` (`sync_retry`, `SyncEvery`, drive-removal check; U4/U5/U7). Its `lib.rs` is the
  public API the CLI, the app and the server build against.
- `crates/ps5-dump-forge-exfat`: forward-only exFAT writer (512 B sectors, 64 KiB clusters), ported
  from MkPFS. `crates/ps5-dump-forge-ufs2`: write-once UFS2 writer for the one SMP geometry
  (`newfs -O 2 -b 65536 -f 65536 -m 0 -S 4096`). Both: `plan` (validates names, sizes the image)
  then `write` (strict offset order into a caller-owned file).
- `crates/ps5-dump-forge-pfs`: PFS writer for `.ffpfs` (MkPFS's unsigned PS5 layout, same `plan`/`write`
  shape), the streaming `.ffpfsc` container writer (`wrap`: the inner image's writer → `Stream` → zlib
  workers → `.part`), and the PFS/PFSC readers (`PfsSource`, `open_ffpfsc`).
- `crates/ps5-dump-forge-fpkg`: streaming `.pkg` reader (`FpkgSource`), CNT entries merged with the
  inner filesystem.
- `crates/ps5-dump-forge-lz4`: ampr_emu asset packs: AMPRPAK4 manifest + AMPRDAT3 volume writer and
  reader (unpack view as a `SourceTree`), AMPRIDX3 and AMPRCMD1 (journal) readers, pack rules (`rules.rs`),
  the embedded runtimes (`runtime.rs`). Glue in `core/src/lz4.rs` (open/unpack, trace, index, runtime swap);
  `core/src/lz4_patch.rs` is the in-place folder patch and unpatch; `core/src/lz4_profile.rs` is Save as profile.
  `tree.rs` is `PackedTree`, the packs served as a `SourceTree` from a `writer::measure` pass.
  `vendor/ampr_emu/`: the two upstream 0.4.2.1 `.sprx` runtimes + the source archive (see its README).
- `crates/ps5-dump-forge-cli`: binary `ps5-dump-forge` (`inspect`, `convert`, `serve`), JSON-lines events.
- `crates/ps5-dump-forge-server`: `serve`, std-only HTTP/1.1 (`http.rs`), routes mirroring the Tauri commands
  (`api.rs`), job table for polling (`table.rs`), roots (`paths.rs`), PS5 tile (`tile.rs`), self copy
  (`self_copy.rs`), `debug.rs` (PS5 only, with `FORGE_DEBUG_API`). `build.rs` embeds `app/dist-http` and the
  self copy.
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
- LZ4 packs are an asset-pack transform of a game folder (ampr_emu), not an SMP image format: `--to lz4`
  writes a folder (no extension) with `ampr_assets.index`, `.crc`, `ampr_assets-NNN.pak` volumes, loose
  leftovers and the runtime; converting that folder to an image is an ordinary second convert (packs are
  carried). Pack can also target an image directly (`Lz4Mode::Pack`, `lz4: "pack"`, CLI `--lz4-pack`, `format`
  folder/exfat/ffpkg/ffpfs/ffpfsc; `.pkg` refused "not supported yet"; `Format::Lz4` stays the alias of pack
  into a folder): **two passes, no staging folder**. `writer::measure` runs `pack`'s compression without
  writing and keeps only the metadata (`Measured`: manifest with its 12-byte chunk records, CRC sidecar,
  profile, volume sizes, build ID; 16 B per chunk). Per-chunk memory over the job, at 1.4 M chunks: measure
  peaks at 32 B (~45 MB: manifest built plus its self-check decode), write holds 16 B (~22 MB), verify drops
  `Measured` first, then the reader holds 20 B (~28 MB) and ~36 B (~50 MB) while parsing the manifest; ~50 MB is
  the job's peak, plus per-file records and the blocks in flight.
  `tree::PackedTree` lists the loose files plus the pack files at those sizes; reading a volume re-reads
  each chunk's source block, recompresses it with the one `writer::encode`, and fails unless stored length,
  codec, CRC and the LZ4 chunk's decoded CRC match the measurement (a changed source or a nondeterministic
  compressor); one cached chunk, `threads * 4` blocks in flight per volume. `pack` and `measure` share one
  `Sink` (placement, thresholds, rollover; `Volumes` = files or nowhere) and one `metadata`; folder output
  is pinned byte for byte by a hash test. The image job: measure (before the plan, stage `measure`), plan →
  write from `HashingTree(PackedTree(HashingTree(logical)))`, verify as written, then `reader::unpack` of
  the image (every runtime check) and the logical files against the source, same fast/full policy.
  Pack and trace need a title whose `eboot.bin` imports `libSceAmpr`. Compression is `lz4_flex`.
  In the UI every LZ4 action lives in the LZ4 tab (`app/src/Lz4.tsx`), two scenarios: **Trace** (a folder,
  `.exfat` or `.ffpkg`: Patch / Unpatch; a folder through `lz4_patch`/`lz4_unpatch`, an image through a
  conversion with `lz4: "trace"|"unpatch"`, `format` = the source's, `lz4_in_place: true`; Download traces
  in the web build once a journal is there) and **Pack** (traces as a zip or folder, locked to the source's
  own when it is traced; a profile instead; Unpack when the source is packed). Convert has no LZ4 controls
  and sends `lz4: null`.
- The recommended trace path is a traced `.ffpkg` mounted `image_rw=`, then **Download traces** (http build
  only; the desktop app points at Pack): `GET /api/lz4_traces?source=` streams one STORED zip
  (`[GAME_TITLE]-[TITLE_ID]-amprtrace.zip`: the generated output stem, one implementation, + `-amprtrace.zip`) of
  `ampr_commands.bin` + `ampr_emu.index` out of a folder or any image via core's `lz4_traces`
  (`Lz4Traces`: a folder's files opened directly, an image through `open_source`; `write_zip`/`zip_len`)
  in 1 MiB chunks (`Response::download` takes a body writer), never whole in memory. `core/src/zip.rs`
  is the one zip layout, writer and reader (flag bit 3 + data descriptors, ZIP64 only past 0xFFFFFFFF), so
  they can't drift. Every read re-checks a `SourceStamp` (the image file, or a folder's trace files:
  length, mtime, identity) and fails on any change, so the body ends short of `Content-Length` (a
  relaunched game truncates its journal; an image reader would serve stale blocks). `/mnt/shadowmnt` is a
  PS5 root so the mounted image's folder can be picked too. Patching a folder stays offered: on hardware a
  folder patched in place didn't launch (CE-107750-0, cause unknown, even after restoring stock files),
  while a plain Forge `.ffpkg` of the same game did.
  Both upstream runtimes (release, trace) are embedded with `include_bytes!`, pinned by SHA-256; the trace
  runtime is swapped for the release one on every pack or plain conversion of a traced dump. Unpatch
  (`Lz4Mode::Unpatch`, `--lz4-unpatch`) does that explicitly: the release runtime over any runtime
  (Forge's trace build or another; no backup exists), journal and logs left out, a fresh index; refused
  for a packed source ("unpack first") and with `--to lz4` (redundant).
- LZ4 rule sources, first match: `--lz4-profile x.toml`, else the journal of the last trace session
  (`ampr_commands.bin`, mapped to paths through the source's `ampr_emu.index`; only the last session counts),
  else the built-in guess. `--lz4-unpack` (also automatic when the target is lz4) restores the logical
  assets but keeps the runtime and `ampr_emu.index`. A trace (`--lz4-trace`) writes the journal on the
  title's `/app0`, so the output must be writable there: a folder, or an `image_rw` exfat/ffpkg with
  `--lz4-trace-space` MiB (default 256, 64..1024 step 64; measured ≈ 25 MB/hour of heavy loading on Stellar Blade, so ≈ 10 hours) of free room; `.ffpfs`/`.ffpfsc`/`.pkg` are refused.
  `--lz4-traces <zip|folder|ampr_commands.bin>` (the downloaded traces zip: STORED entries only, each CRC-checked,
  the journal streamed from its byte range; a folder holding both files; or the journal with its index
  beside it; `--to lz4` or `--lz4-pack` only, exclusive with a profile) uses traces copied from elsewhere ahead of the dump's own: rule sources are profile, copied
  traces, own journal, built-in guess. The membership check: every path the index lists must be in the
  dump (else refused: another dump or version), leaving out on both sides the journal, index, logs,
  `fakelib/libSceAmpr.sprx` and patch temp names; files only in the dump (a scene `.nfo`) are allowed,
  unobserved and loose, and logged in one line (first 10 + a count). Observed ids resolve through the
  traced index's own record order. Traces only narrow the built-in guess: an observed file is packed only if the fallback rules would pack it too (container indexes `.pak/.utoc`, configs, media, root files stay loose).
  Packed files exist for the game only after it starts AMPR (the runtime publishes its index then), so
  anything the engine opens, stats or lists before that (Unreal mounts `.pak`/`.utoc`, reads `.ini`,
  `.uproject`) must stay loose. The keep-loose list (`rules::keep_loose`: `FALLBACK_SUFFIXES` + the protected
  directory families; not the fallback's root-files rule) applies to every rule source, a profile included
  (`profile_spec` is the profile alone; one log line counts what it wanted packed, first 10).
  Then auto-loose (upstream `ampr_pack.py`, `rules::AutoLoose`, `[pack] auto_loose_*` honored with upstream's
  bounds, defaults otherwise): a compress-spec file (not store; hot only with `auto_loose_hot_files`) of
  ≥ 64 MiB is sampled in core's `plan` during preflight (32 blocks / ≤ 16 MiB at fixed, evenly spread block
  indices, `writer::sample` = the writer's `encode`) and kept loose when it saves < 5% or ≥ 90% of the
  blocks stay RAW; one log line `auto-loose: N large files kept loose (incompressible samples): …`.
- Save as profile (`lz4_plan_profile(&ConvertRequest)` → `Lz4PlanProfile {file_name, toml, packed, loose,
  log}`, `core/src/lz4_profile.rs`; `convert::lz4_plan` runs a Pack job's front half: open, unpack view,
  backport, `lz4::prepare` with the rules, keep-loose and auto-loose; writes nothing, LZ4 findings are the
  error). The TOML: `#` header (Forge version, game stem, rule source, UTC date), `[pack]` loose default,
  64 KiB, `auto_loose_large_files = false` (already applied), one `[[rule]]` per distinct `PackSpec`
  (store → `action = "store"`, hot, random → `layout = "random"`, hot sequential → `"mixed"`) with exact
  paths fnmatch-escaped (`[`→`[[]`, `*`→`[*]`, `?`→`[?]`) and TOML-escaped; the job profile's `[runtime]`.
  Loaded back it selects the same specs (round-trip test). Name: the generated stem + `-lz4profile.toml`.
  CLI `lz4-profile <source> [--lz4-traces|--lz4-profile] [-o out.toml]` (default next to the source,
  never overwrites); server `POST /api/lz4_plan_profile` `{request}` (Blob download in the page); Tauri
  `lz4_save_plan_profile {request, dest}` (dest from the save dialog, `dialog:allow-save`). UI: a secondary
  **Save as profile** beside Pack.
- Never touch the source: junk (`.DS_Store`, `._*`, `.fseventsd`, ...) is filtered, not deleted.
  Two exceptions, by user decision, both explicit requests:
  1. The LZ4 folder patch and unpatch (`lz4_patch`/`lz4_unpatch`, CLI `lz4-patch`/`lz4-unpatch`, server
     `POST /api/lz4_patch`/`POST /api/lz4_unpatch` `{source}`, Tauri `lz4_patch`/`lz4_unpatch` `{path}`)
     change a plain game folder of an AMPR title: the trace (unpatch: release) runtime replaces any
     `fakelib/libSceAmpr.sprx` (no backup), the journal and logs are deleted (root synced) and a fresh
     `ampr_emu.index` is written, each file via an exclusive per-attempt temporary name
     (`.<name>.forge-<pid>-<n>.tmp`, never indexed, ignored by the traces' membership check), sync and
     rename, then the folder synced; 0777 on the PS5. Everything is checked first: images, packed
     folders, non-AMPR titles, a non-file at any target path and (PS5) the destination probe refuse with
     nothing changed; so does a job queued or running.
  2. An image patched or unpatched in place (`ConvertRequest.lz4_in_place`, CLI `convert <img>
     --lz4-trace|--lz4-unpatch --lz4-in-place`): rebuild + replace. Only an `.exfat`/`.ffpkg` source,
     `format` its own, `lz4` `trace` or `unpatch`, not packed (else preflight findings; `.ffpfs`/`.ffpfsc`/
     `.pkg` are read-only on the console). `output` is ignored: the job writes `<source>.<job>-<pid>.part`
     beside the source (the destination check counts the whole new image), verifies it as usual, re-checks
     the source's `SourceStamp`, closes the source and renames the part over it (`Part::replace`, one
     atomic rename, folder synced). Any failure leaves the source byte-identical and the `.part` deleted.
     The server's stale-part filter takes an in-place job's `.part` from its source.
  The user takes the journal and index off the console (Download traces) and packs on a computer
  (`--lz4-traces`).
- Names are never renamed: bad names (non-NFC, exFAT/PFS case collisions, forbidden chars, non-ASCII
  in PFS) fail preflight with every offender listed.
- `.pkg` is built with `BuildRequest::production` only (plaintext `PPRPLAIN-NOAUTH!`, Kraken);
  installing needs kstuff + fpkg-enable + ppr-patch. Debug keys are embedded.
- The UI calls the debug FPKG `.fpkg` (a label only): its files, the `Format`/`Kind` id and the CLI
  flag stay `pkg`. The default target is `.ffpkg` (SMP 1.7's recommendation; `.exfat` is for
  compatibility). Format copy and the About formats table (`app/src/Formats.tsx`: one table, Console
  speed and Size on disk for every target plus the plain folder) follow ShadowMountPlus 1.7's README and
  release notes.
- No settings and no config file: the UI keeps choices in memory until it closes. Defaults: target
  `.ffpkg`, exFAT inside a `.ffpfsc` at zlib level 6, all cores for compression (`serve`: cores − 1),
  `.pkg` Kraken level `fast` (`balanced`/`smallest`: ~2.6% smaller, ~6x/~9x the compress time; tests use
  `fast` only), no extra free space beyond each format's built-in spare, output next to the source (an
  empty output `dir` is the source's folder, never the process's working directory).
- Progress is one bar per job: `Event::Progress` `done`/`total` span every pass (write, verify,
  ...); `Ctx::expect_rest` renews the estimate as each pass learns its size.
- The Tauri app has no HTTP sidecar and no `tauri-plugin-fs`; the capability grants only the app's
  commands, dialogs and event listening. The web UI is a separate build served by `serve`.
- Verification: fast by default everywhere (every structural check + seeded BLAKE3 sample of 8 MiB
  slices), full is opt-in (`full_verify`, `--full-verify`, the "Full verification" switch). A fast `.pkg`
  also samples the builder's own block sweep and `playgo-chunk.crc` check (vendor patch 0021, the job's seed).
- PS5 payload: one plain `.elf`, no PKG, no signing, no updates (replace the file). FreeBSD 11 ABI via
  `--cfg libc_unstable_freebsd_version="11"` (the env var is ignored by libc ≥ 0.2.187) and struct-size
  asserts in the CLI. `panic=abort`.
- The server has **no protections** by user decision: no pairing, token, Host/Origin/CSRF checks or path
  confinement, and no delete route. Don't re-add them.
- Tile `PDFG00001` "PS5 Dump Forge" (deeplink to `http://127.0.0.1:<port>/`), never written without our
  owner file. Self copy: `ps5/build.sh` builds twice; the release ELF embeds stage 1 and saves it to
  `/data/ps5-dump-forge/ps5-dump-forge.elf`. Launch on open: the AppCache'd page asks elfldr
  (`GET :9021/<elf>?args=serve`) only from the console's own browser at load.
- PS5 writes: a retried fsync fails the job; USB is never hardware-tested, so the destination probe decides
  (never the path or fs name) and fails closed. Debug routes (`FORGE_DEBUG_API`) never ship; `release-ps5.sh`
  refuses them.

## Format rules (ShadowMountPlus 1.7)
- Raw filesystem at byte 0, no MBR/GPT; SMP attaches the whole file. The extension picks the driver, so it
  must match (`.ffpkg` UFS, `.exfat` exFAT, `.ffpfs` PFS, `.ffpfsc` PFSC); an inner image in `.ffpfsc` keeps
  its extension.
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
  stable). Always PFSC, zlib level 0–9 as zlib numbers them (the slider, `--level`, `ffpfsc_level`;
  default 6, the measured knee: 7 took 6% longer for 0.02% smaller; 0 stores every block raw); a block is
  kept compressed only if it saves ≥ 5%; a block with near-8-bit byte entropy that level 1 shrinks < 2%
  skips the level's search (stored raw). zlib from `flate2` (`miniz_oxide`; console acceptance is a
  hardware gate in `TODO.md`). Streamed: the inner writer feeds the compressor, which writes the `.part`;
  no temporary inner image. Free-space preflight asks for the worst case (every block raw).
- Maker's mark `PS5-FORGE-v<version>` (≤ 31 bytes, a const assert in `convert.rs`): `.exfat` in an OEM
  Parameters record (sector 9, our GUID `{182B1321-1B2D-441D-BECA-28B704837CA0}` + 32 bytes ASCII, in both
  boot regions, under the boot checksum), `.ffpkg` as `fs_volname` in every superblock, `.ffpfsc` in its
  inner `.exfat`/`.ffpkg`. `.ffpfs` and `.pkg` carry none (no field for it). `inspect` reports `forge_version`.
- Image names: SMP mounts at `/mnt/shadowmnt[/pfsc]/<stem>_<8 hex>` and FreeBSD 11's MNAMELEN is 88
  (longer fails with ENAMETOOLONG), so the stem (the name without its extension) is ≤ 63 bytes for
  `.exfat`/`.ffpkg`/`.ffpfs` and ≤ 58 for `.ffpfsc` (`preflight::stem_limit`); preflight refuses a longer
  typed name. Folders and `.pkg` keep to the same 63 bytes, so every output follows one rule. Generated names
  are `[GAME_TITLE]-[TITLE_ID]` (`-2`, `-3`, ... when taken); the game title is cut first.
- FPKG: outer PFS plaintext with the `PPRPLAIN-NOAUTH!` marker (native AES-XTS fails the console's auth),
  inner image Kraken-compressed (zlib metadata is rejected by the console, never offer it).

- LZ4 packs (ampr_emu 0.4.2.1): index at `ampr_assets.index` (AMPRPAK4, little-endian, CRC32, FNV-1a path hashes)
  with `.crc` sidecar and `ampr_assets-NNN.pak` volumes (AMPRDAT3, volume cap 4 GiB - 64 KiB for FAT32); chunks LZ4 or RAW
  (a RAW chunk is exactly one I/O page); files the runtime cannot serve stay loose. CRCs detect
  corruption, they do not authenticate. The manifest's runtime-contract checks run on every open.
  Not an SMP image: ShadowMountPlus mounts none (no `.lz4` extension, no mount name limit beyond a folder's).

## Commands
```sh
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace --release
(cd app && npm ci && npm run tauri dev)                       # GUI in dev mode
cargo run -p ps5-dump-forge-cli -- convert <game_dir> --to exfat   # or ffpkg | ffpfs | ffpfsc | pkg | folder
cargo run -p ps5-dump-forge-cli -- convert <game_dir> --to lz4 [--lz4-profile x.toml]   # asset packs (folder out)
cargo run -p ps5-dump-forge-cli -- convert <game_dir> --to folder --lz4-trace [--lz4-trace-space MiB]   # install the trace runtime (also --to exfat|ffpkg)
cargo run -p ps5-dump-forge-cli -- convert <packed_dir> --to folder --lz4-unpack   # back to the loose assets
cargo run -p ps5-dump-forge-cli -- lz4-patch <game_dir>   # IN PLACE: trace runtime, fresh index, stale journal/logs removed
cargo run -p ps5-dump-forge-cli -- lz4-unpatch <game_dir>   # IN PLACE: release runtime, fresh index, journal/logs removed
cargo run -p ps5-dump-forge-cli -- convert <img.ffpkg> --lz4-trace --lz4-in-place   # REPLACES the image (also .exfat; --lz4-unpatch)
cargo run -p ps5-dump-forge-cli -- convert <game_dir> --to lz4 --lz4-traces <x-amprtrace.zip|folder>   # pack with traces from the console
cargo run -p ps5-dump-forge-cli -- convert <game_dir> --to ffpkg --lz4-pack [--lz4-traces <zip>]   # pack straight into an image (also exfat|ffpfs|ffpfsc)
cargo run -p ps5-dump-forge-cli -- lz4-profile <game_dir> [--lz4-traces <zip>] [-o x.toml]   # Save as profile: the pack plan as TOML
scripts/check-lz4.sh <lz4_out_dir> <orig_dir> | --reverse   # upstream Python tools (Docker) inspect/verify our packs, unpack theirs
cargo run -p ps5-dump-forge-cli -- convert <game_dir> --to ffpfsc --inner exfat   # inner: exfat | ffpkg | ffpfs; --level 0..9
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
