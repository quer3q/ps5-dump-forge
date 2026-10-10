# PS5 payload internals

Developer notes for the PS5 payload: the CLI's `serve` built as a PS5 ELF, with the app's UI as a web page.
How to load and use it is in the main [README](../README.md#ps5-jailbroken-with-an-elf-loader).

```sh
ps5/build.sh                     # target/ps5/ps5-dump-forge.elf (+ stage1.elf, and .elf.txt from llvm-readelf)
ps5/test-entry.sh                # host check of entry.c (launch modes, args file, log redirection)
scripts/release-ps5.sh           # dist/ps5-dump-forge-<version>-ps5.elf, checked; SKIP_BUILD=1 checks what's built
scripts/test-freebsd.sh          # core + server tests on FreeBSD UFS, FAT32 and nullfs volumes (qemu VM in Docker)
cargo run -p ps5-dump-forge-cli -- serve --root <dir>       # the web UI on this computer, http://localhost:8095
(cd app && npm run build:http && node scripts/smoke.mjs)    # the web bundle + client against the real serve
FORGE_DEBUG_API=1 ps5/build.sh   # bring-up only: adds POST /api/debug; never shipped (release-ps5.sh refuses it)
```

## Harness (`ps5/`)

- `Dockerfile`: `rust:1.99.0-slim-trixie` (pinned by digest), Debian clang/lld/llvm 19, ps5-payload-dev SDK
  v0.43 (zip pinned by SHA-256), `rust-src` with a one-line FreeBSD 11 std patch (`prepare-rust-std.py`
  refuses a changed context). `RUSTUP_TOOLCHAIN=1.99.0` overrides `rust-toolchain.toml`, since build-std
  needs that toolchain's patched `rust-src`. Rust, the std patch, the SDK, the target JSON and the shims are
  pinned together; bump them together.
- `build.sh`: `npm run build:http` on the host (`FORGE_SKIP_WEB=1` skips it), the C files with
  `prospero-clang`, then `cargo build -p ps5-dump-forge-cli --target ps5/x86_64-ps5-freebsd.json -Z
  build-std=std,panic_abort` with `RUSTC_BOOTSTRAP=1` and `--cfg libc_unstable_freebsd_version="11"`. The
  PS5 fills FreeBSD 11 structs (`stat` 120 bytes, `dirent` 264); libc ≥ 0.2.187 ignores the old
  `RUST_LIBC_UNSTABLE_FREEBSD_VERSION` variable, which left the first hardware build on FreeBSD 12 structs
  (every path looked like neither file nor folder), so the CLI asserts the struct sizes at compile time.
  It deletes the CLI's linked output first, so a C change always relinks (a rebuild takes ~8 s). Output
  and `CARGO_HOME` live in `target/ps5/`. `panic-strategy` is `abort`: a job panic ends the payload and
  leaves its `.part`, which the next start lists.
- Two stages, because a running payload can't read its own file: stage 1 (`target/ps5/stage1.elf`, no
  copy inside), then stage 2, the release ELF, with `FORGE_SELF_ELF` naming stage 1, which the server's
  `build.rs` embeds (see [Self copy](#tile-self-copy-and-launch-on-open)). `FORGE_DEBUG_API` stops after stage 1.
- `linker.sh`: rustc's linker. Maps `-lm -lrt -lutil -lexecinfo -lkvm ...` to `-lc` and `-lgcc_s` to
  `-lunwind`, adds the shims and the `--wrap` list, and names the SDK libs (rustc passes `-nodefaultlibs`).
- cc-rs (blake3's assembly) derives `--target=x86_64-unknown-freebsd-ps5`, which clang refuses;
  `CFLAGS_x86_64_ps5_freebsd='--target=x86_64-sie-ps5 -mno-red-zone -femulated-tls'` puts the SDK target
  back (the later flag wins).
- `entry.c` (`__wrap_main`): elfldr starts a payload with no arguments. No arguments, or `serve` first
  (elfldr's `args=serve`, as the tile's page sends it), runs `serve`, logged to
  `/data/ps5-dump-forge/serve-<pid>.txt`. A developer hook: an args file `/data/ps5-dump-forge/args` (one
  argument per line, the command first; CR dropped, blank lines skipped; over 16 KiB, a NUL or more than 62
  arguments is refused, never cut short) is renamed to `args.done` before it runs, so an autoloaded boot
  never replays it; its output goes to `log-<pid>.txt`. Before either log is opened, earlier runs'
  `serve-<pid>.txt`/`log-<pid>.txt` whose process is gone (`kill(pid, 0)` → `ESRCH`) are deleted except
  the newest by mtime, so the run before a crash stays readable. It exports `ps5_notify` (at most 3,074 bytes).
- `compat.c`: `statfs@FBSD_1.0` / `fstatfs@FBSD_1.0` forwarders (outside `shim/`). `launcher.c`: registers
  the tile (after ps5-ai-cli's launcher).
- `shim/`, `x86_64-ps5-freebsd.json`, `prepare-rust-std.py`: copied unchanged from ps5-ai-cli `eb1d494`
  (GPL-3.0-or-later, `shim/UPSTREAM`): FreeBSD 11 `.symver` stat/readdir/kevent, raw-syscall `--wrap`s for
  ~40 calls the PS5 renumbers or lacks, an `fcntl` `F_DUPFD_CLOEXEC` fallback, `getrandom`/`getentropy` over
  `arc4random_buf`, `pthread_setname_np`. Keep them byte-identical; our changes go in `entry.c`/`linker.sh`.
- The ELF: PIE `DYN`, `NEEDED` exactly `libkernel_web.sprx`, `libSceLibcInternal.sprx`,
  `libSceSystemService.sprx` (rest-mode tick) and `libSceAppInstUtil.sprx` (the tile), no `PT_INTERP`, no TLS.
  `scripts/release-ps5.sh` checks that, the entry point, that every `app/dist-http` file is embedded byte for
  byte, no debug routes, the version marker (`VERSION_MARKER` in the server) against the workspace version,
  and the self copy (stage 1 carries none; the release ELF exactly one, decompressing to stage 1).

## Web server (`crates/ps5-dump-forge-server`, CLI `serve`)

- Std only: hand-rolled HTTP/1.1, one request per connection, no server push; the page polls `GET
  /api/jobs` (every 1 s while a job is unfinished, else 5 s). Binds `0.0.0.0:8095` (`--port`). Roots for the
  file browser: on the PS5 `/data`, `/mnt/shadowmnt` (while it exists: ShadowMountPlus's mount points, so a
  mounted image can be picked as a folder), `/mnt/usb0..7`, `/mnt/ext0..1` (a drive counts only while
  `statfs` names it as its own mount point with blocks: those folders exist with nothing plugged in;
  re-checked on each use), canonicalized and logged; on a computer the `--root` folders.
- **No protections, by decision**: no pairing, token, `Host`/`Origin`/CSRF checks, security headers or
  path confinement; paths go to core as given, as in the Tauri app. No delete route, and none may be added.
- Limits: head 16 KiB, body 1 MiB (`Content-Length` only), 10 s per request, 30 s per write (a download:
  per 1 MiB chunk), 16 handler
  threads (then 503), 8 unfinished jobs (then 429), 2 inspections or naming requests at once (429,
  `Retry-After: 1`), 32 finished jobs and 200 log lines each kept, `list_dir` 10,000 entries (`truncated`),
  `stale_parts` 1,000. `compression_threads: null` is `max(1, cores − 1)`.
- Errors are `{"error": "<text>"}` with 400 / 404 / 409 (stopping) / 413 / 429 / 500; core errors are the
  Tauri commands' `format!("{e:#}")`. API replies are `Cache-Control: no-store`; `index.html` and the
  AppCache manifest `no-cache`; hashed `assets/*` a year, immutable.

| Route | Body | Returns |
|---|---|---|
| `GET /api/session` | | `{app: "ps5-dump-forge", version, platform: "ps5"\|"host", instance, stopping, self_copy, separator: "/"\|"\\"}` |
| `POST /api/inspect` | `{path}` | `Inspection` |
| `POST /api/default_output` | `{source, format, dir}` | path |
| `POST /api/generated_output` | `{source, format, dir, taken}` | path |
| `POST /api/start_job` | `{request: ConvertRequest}` | job id (`lz4_in_place: true` replaces an `.exfat`/`.ffpkg` source; `output` ignored) |
| `POST /api/cancel_job` | `{id}` | `null` (idempotent) |
| `POST /api/stale_parts` | `{dirs}` | `string[]`, without running jobs' parts (an in-place job's beside its source) |
| `POST /api/lz4_patch` | `{source}` | `Lz4Patch`; 409 while a job is queued or running |
| `POST /api/lz4_unpatch` | `{source}` | `Lz4Patch` (release runtime); 409 while a job is queued or running |
| `POST /api/lz4_plan_profile` | `{request}` (a Pack `start_job` body) | `Lz4PlanProfile` `{file_name, toml, packed, loose, log}`: Save as profile; reads only (counts as an inspection, 429 past 2); the page downloads `toml` as `file_name` |
| `GET /api/lz4_traces?source=` | | both trace files as one zip download (below) |
| `POST /api/list_dir` | `{path: string\|null}` | `{path, parent, entries: [{name, path, dir, size}], truncated}` |
| `GET /api/jobs` | | `{instance, jobs: [{id, request, progress, done, log, log_total}]}` |
| `POST /api/quit` | `{}` | `{}`, then cancels every job, waits for cleanup and exits |

`lz4_traces` (LZ4 → Trace → Download traces): `source` (a folder or any image Forge reads; percent-encoded, else
400); streams `ampr_commands.bin` and `ampr_emu.index` from the source's root (core's `lz4_traces`: a
folder's files opened directly, an image through its reader) as one zip at the archive root: STORED
entries, flag bit 3 (CRC-32 computed while streaming, then a data descriptor), central directory, end
record; ZIP64 fields only where a size or offset reaches 0xFFFFFFFF (core's `zip.rs`, whose reader Pack
uses). `200`, `application/zip`, `Content-Disposition: attachment; filename="<ASCII stand-in>"` plus
`filename*=UTF-8''<percent-encoded>` for a non-ASCII name, the name `[GAME_TITLE]-[TITLE_ID]-amprtrace.zip`
(`amprtrace.zip` without either), the exact `Content-Length` (planned up front), `no-store`, read and
written 1 MiB at a time (never whole in memory). Either file missing: 404 naming it; a source core can't
open: 500 with its error. Each chunk is read only while the source is unchanged
since opening (the image file, or a folder's trace file: length, mtime, identity); a change (a game
relaunched on a writable mount truncates its journal) ends the transfer short and is logged. Opening
counts as an inspection (the 2-at-once cap), the transfer doesn't; jobs may run meanwhile. A read failing
mid-transfer closes the connection short of `Content-Length`.

`instance` is random per start, so the page notices a restart and rebuilds its job list. `list_dir` with
`null` lists the roots; directories first, then by name; non-UTF-8 names left out; symlinks resolved.
Events keep core's serde shape (`{"Ok": …}` / `{"Err": "cancelled"}`); ids stay below 2^53. A job's table
entry is made by its first event (core can emit `Done` before `Jobs::start` returns). Quit closes
admission first (`start_job` → 409); on a computer SIGINT/SIGTERM does the same. A port already taken by
Forge notifies "already running at <url>" and exits 0; by anything else, exits 1.

- The URL shown uses the address of a UDP `connect` toward the internet (no packet sent).
- The web bundle: `npm run build:http` (Vite mode `http`) writes `app/dist-http` with `transport-http.tsx`
  in place of Tauri (`Picker.tsx` for the dialogs, `http-client.ts`'s poller for the events); `build.rs`
  embeds it, or a placeholder page with a warning (`FORGE_REQUIRE_WEB=1`, set by `ps5/build.sh`, makes that
  an error). Paths are the server's (`/`), never the viewer's. Plain-node checks: `app/scripts/`
  (`check-paths`, `check-poller`, `check-launcher`, `smoke`).

## Tile, self copy and launch on open

- Tile (`tile.rs`, `ps5/launcher.c`): `/user/app/PDFG00001/sce_sys/{param.json,icon0.png}`, `deeplinkUri`
  `http://127.0.0.1:<port>/`, icon `ps5/icon0.png`. Registered only when a file was written. A `PDFG00001`
  not marked by `/data/ps5-dump-forge/tile-PDFG00001.owner` is another title's and is never touched. Runs in
  the background and never fails the server; the serve log says what it did.
- Self copy (`self_copy.rs`): stage 2 embeds `PDFGSELF`, u64 LE raw length, u64 LE compressed length,
  SHA-256 of stage 1, then stage 1 zlib-compressed (6.3 MB ELF holding 1.7 MB for a 4.5 MB copy). On start a
  thread unpacks it (bounded, hash, ELF magic and version marker checked) into
  `/data/ps5-dump-forge/ps5-dump-forge.elf`: written when missing, older, the same version with other bytes,
  or unmarked; kept when identical or newer. Temp file `O_EXCL|O_NOFOLLOW`, fsync, rename, folder fsync. A
  copy that loses the port race, and quit, finish the save first. `GET /api/session` `self_copy`:
  `saving`, `saved`, `up to date`, `kept newer <v>`, `failed: <why>`, or `none` (stage 1, debug, computers).
- Launch on open (`launcher.ts`, `appcache.ts`): the page is kept by AppCache (`forge.appcache`, a hash over
  every bundle file). Only on the console's own browser (a loopback host) and only at page load: if `GET
  /api/session` doesn't answer, it sends `GET
  http://127.0.0.1:9021/data/ps5-dump-forge/ps5-dump-forge.elf?args=serve&pipe=0` (then the USB copies) and
  waits up to 30 s. A newer payload's manifest swaps the page in and reloads once, never mid-dialog.

## Writing safely on the PS5

Console-side lessons (from ps5upload and ShadowMountPlus): a cross-device `rename()` panics the kernel;
sustained multi-GB writes panicked it from dirty-page buildup; Sony's exFAT (`exfatfs`) shows freed space as
used until the folder is fsynced; some filesystems refuse `fsync` with `EINVAL`/`ENOTSUP`; `fsync` fails
transiently; sparse files on PS5 UFS collapse to 2–3 MiB/s. `/data` is `bfs` or `ufs`, sometimes through
`nullfs` (on the test console: `nullfs`, no hard links). The code labels each piece U1–U10:

- **U1, destination probe** (`dest.rs`): once per job, through one held folder descriptor (right under
  `nullfs`). `fstatfs` for free space, type and read-only; a 1 MiB `.forge-probe-<pid>.part` written,
  fsynced, read back, same `st_dev`; the file-size limit from `_PC_FILESIZEBITS`, else by name (`msdosfs`
  32 bits; `exfatfs`, `ufs`, `bfs` 64), else 4 GiB − 1; case and NFC/NFD folding by lookup, where only
  `ENOENT` means distinct and unknown means folds; hard links by `linkat`, where only `EOPNOTSUPP`/`EPERM`
  mean unsupported and any other error refuses the job. One log line (`destination /mnt/usb0: exfatfs, 118
  GiB free, 64-bit files, folds names, no hard links`). Fail closed.
- **U2** folder output's name findings use the probe's folding. **U3 publish** (`finalize.rs`): same
  device or refuse; a file where hard links work is published by `linkat` (`EEXIST` = taken; cleanup after
  it never fails the job); a folder, or no hard links, takes the check-then-rename (accepted window).
- **U4 fsync** (`durable.rs` `sync_retry`): ps5upload's 20/60/200/600 ms backoff on `EINTR`, `EAGAIN`,
  `EBUSY`, `ETIMEDOUT`, `ENOENT`, `ENXIO`, `ENODEV` and Sony's `0x8002xxxx` forms; `EIO`, `ENOSPC`, `EROFS`
  fail at once; unsupported on a folder is best effort, on a file an error. A fsync that succeeded only
  after a retry fails the job (dropped pages can't be told apart).
- **U5 bounded writes** (`SyncEvery`, around every image writer, the `.ffpfsc` wrap and extraction; a
  background syncer for `.pkg`): large writes split, gaps past the end written as zeros (no sparse files,
  no `set_len` extends), a sync every N dirty bytes, N from 32 MiB adapting to ~1 s of measured rate within
  16..256 MiB, each change logged. Forward seeks from the end don't sync. No preallocation (U9).
- **U6** deletes close the handle, unlink, then fsync the folder; free space is re-checked at the next
  preflight ("space may still be being freed"). **U7** on any failure, the recorded mount points of source
  and output are re-checked; a gone drive is named before the original error. **U8**
  `sceSystemServicePowerTick` every 30 s while a job runs holds off auto rest (rest entered by hand: no
  clean-failure guarantee; `stale_parts` lists what was left).
- **U10 testing without USB**: host unit tests with a fault-injecting output; `scripts/test-freebsd.sh` runs
  core and server tests on md-backed UFS, FAT32 and nullfs-over-UFS volumes (`tests/freebsd_volumes.rs`:
  every target end to end per volume, BLAKE3 read-back, a late failure that publishes nothing). FreeBSD has
  no native exFAT, so `exfatfs` stays covered by the probe only.
- Read-ahead (`prefetch.rs`): the source is read one range (≤ 8 MiB) ahead on its own thread, overlapping
  hashing and writing.

## Speed on the console

Measured on a PS5 (firmware 13.60), sources and outputs on `/data`:

| Pattern on `/data` | Speed |
|---|---|
| 8 MiB reads | 912 MB/s |
| 64 KiB reads | 150 MB/s (~0.35 ms per syscall) |
| 8 MiB writes | ~288 MB/s |
| `fsync` every 64 or 256 MiB | no measurable cost |

| Conversion | Size | Time | Verify |
|---|---|---|---|
| 007 `.ffpkg` → `.exfat` (UFS2 reader before vendor patch 0015) | 87.3 GB | 26 min | full |
| 007 `.ffpkg` → folder (with patch 0015) | 86.9 GB | 11.6 min | full |
| Directive 8020 folder → `.exfat` | 59.2 GB | 4.75 min (~208 MB/s end to end) | fast, 377 samples, 2.1 GiB |

Write speed of folder → `.exfat` through the speed work (15 s samples of the same game):

| Build | Write speed |
|---|---|
| One thread, two hash passes | ~203 MB/s |
| Read-ahead on its own thread + one hash pass (slice hashes only) | ~265 MB/s |
| No hashing of sync intervals (a retried `fsync` fails the job instead) | ~273 MB/s |

That is ~95% of the drive's ~288 MB/s write ceiling. Every cancel left no `.part` behind. Vendor patches
are listed in [vendor/README.md](../vendor/README.md).
