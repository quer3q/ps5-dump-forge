<p align="center"><img src="app/src-tauri/icons/128x128@2x.png" width="128" alt="PS5 Dump Forge"></p>

<h1 align="center">PS5 Dump Forge</h1>

<p align="center">Convert PS5 game dumps between a folder and the images <a href="https://github.com/drakmor/ShadowMountPlus">ShadowMountPlus</a> 1.7 mounts, and build or extract debug FPKG packages.<br>
macOS, Windows and Linux apps + command-line tool, and a PS5 payload with the same UI in a browser. Offline, no settings.</p>

- **Any source to any target**: folder, `.exfat`, `.ffpkg` (UFS2), `.ffpfs` (PFS), `.ffpfsc` (compressed PFS
  container around an exFAT, UFS2 or PFS image), debug FPKG `.pkg`. Streams straight from source to output,
  no temporary copy, not even the image inside a `.ffpfsc`.
- **Verified output**: every output is written to a `.part` file and read back through its own reader before
  it gets its final name: every structural check, plus BLAKE3 of a random sample of the content (or of every
  byte with **Full verification**; see [Verification](#verification)).
- **Runs on the PS5 too**: an ELF payload that converts on the console itself, driven from the PS5 browser
  or any browser on the network (see [PS5](#ps5-jailbroken-with-an-elf-loader)). Experimental.
- **Safe with your dump**: never modifies the source; macOS junk (`.DS_Store`, `._*`, ...) is skipped,
  not deleted; bad file names are reported up front, never silently renamed.
- **Inspect** a dump or image: cover, title, required firmware, `param.json`, leftover `.part` files.
- **Backport detection**: a top-level `fakelib/` (or SMP 1.7's `fakelib2/`) marks a backport; its firmware is
  read from the game's executables, not from `param.json`. Backport libraries go into a `.pkg` byte for byte,
  never "repaired".
- **DLC detection**: DLC embedded in a dump is listed by content id, from the DLC emulator's `dlc_emu.ini`,
  a DLC's own `param.json`/`param.sfo`, and folders named after its content id.
- **Writes onto USB sticks and SD cards** formatted exFAT or FAT32.
- **Fast**: an 89 GB game becomes `.exfat`/`.ffpkg` in about 2–2.5 min (write + verify) on an Apple Silicon
  Mac; `.pkg` takes ≈ 20 min (Kraken compression).

> **v0.0.1-pre4 is a test release.** Not every format has been hardware-tested on a console yet; keep the
> original dump.

## Formats

| Format | Create | Read / extract |
|---|---|---|
| Folder | yes (extract) | yes |
| `.exfat` (512 B sectors, 64 KiB clusters) | yes | yes |
| `.ffpkg` (UFS2) | yes | yes |
| `.fpkg` (debug FPKG, saved as `.pkg`) | yes | yes (packages from this builder or compatible debug builders) |
| `.ffpfs` (PFS, uncompressed) | yes | yes |
| `.ffpfsc` (PFSC container) | yes (exFAT, UFS2 or PFS inside; streamed, no temporary image) | yes (exFAT, UFS2 or PFS inside) |

The app starts on `.ffpkg`, which ShadowMountPlus recommends; `.exfat` is for compatibility with games that
only run like external-drive content. `.ffpfs` and `.ffpfsc` are experimental in ShadowMountPlus 1.7; a `.ffpfsc`
is the smallest but always mounts read-only and reads slower on the console. A `.ffpfs` holds ASCII file names
only, and ShadowMountPlus fails to mount a `.ffpfs`/`.ffpfsc` whose file name is over 63 bytes, so generated
names are cut to fit. Not sure which to pick? See the app's **About formats** tab.
Installing a `.pkg` needs a console with kstuff + fpkg-enable + drakmor's ppr-patch.

The app has three tabs: **Convert** (pick a source and target, Build, a jobs list with progress and Show in
Finder / Explorer / folder), **Inspect** (game summary; Files / param.json / Details; leftover `.part`
files) and **About formats**.

## Verification

Every output is read back through its own reader before it is published. Two modes, on every platform:

- **Fast** (the default): every structural check (the image opens, geometry, every path, empty folder and
  size, the `.ffpfsc` container and its whole offset table), then BLAKE3 of a sample of the content. Files
  up to 16 MiB are hashed whole; a larger file gets its first, last and one random interior 8 MiB slice (on
  `.ffpkg`, also the slice at 512.75 MiB, where UFS2 switches to double-indirect blocks), plus random slices
  worth `min(1 GiB, 1%)` of the bytes not yet sampled. The seed is fresh each job and logged with the
  coverage (`verify: fast, 2.1 GiB of 80.9 GiB in 41 samples (seed …), policy 1`).
- **Full**: the **Full verification** switch in the app (off by default), `--full-verify` in the CLI.
  Re-reads every byte of the output and compares it with the source; takes longer.

The source is hashed once, while writing, in fixed 8 MiB slices, so both modes compare slices. A `.pkg` is
always fully verified (the package builder checks every block); the switch shows on and disabled for `.fpkg`.
A finished job says "Fast verification passed" or "Full verification passed".

What fast can miss: a damaged block in a region it didn't sample. It catches structural and layout errors
and corruption in any sampled region; use full verification when the output's every byte matters more
than the time.

## Download and run

Every release archive on the Releases page holds the app, the `ps5-dump-forge` command-line tool,
README.md, LICENSE and THIRD-PARTY-NOTICES.md. PS5 Dump Forge has no settings and writes nothing next to
itself beyond its own WebView data (see each platform below), so the app can live in any folder you can
write to.

### macOS (11 or later)

Get `ps5-dump-forge-<version>-macos-universal.zip` (Apple Silicon and Intel, in one zip) from Releases and
unzip it anywhere.

#### First launch

The app is ad-hoc signed only: no Apple Developer ID, not notarized. macOS therefore blocks it on first
launch ("PS5 Dump Forge is damaged" or "cannot be opened because Apple cannot check it for malicious
software"). Use **one** of these:

1. **Terminal** (works on every macOS version). In the folder that holds `PS5 Dump Forge.app`, run

   ```sh
   xattr -dr com.apple.quarantine "PS5 Dump Forge.app"
   ```

   then double-click `PS5 Dump Forge.app`.
2. **System Settings.** Double-click `PS5 Dump Forge.app` once and dismiss the warning, then open
   System Settings → Privacy & Security, scroll down to the message about PS5 Dump Forge and click
   **Open Anyway**. Confirm in the next dialog.

#### Command-line tool

The zip also has `ps5-dump-forge`:

```sh
./ps5-dump-forge inspect PPSA01234.exfat
./ps5-dump-forge convert ~/Games/PPSA01234 --to ffpkg      # folder | exfat | ffpkg | ffpfs | ffpfsc | pkg (debug FPKG)
./ps5-dump-forge convert ~/Games/PPSA01234 --to ffpfsc --inner exfat   # inner: exfat (default) | ffpkg | ffpfs
```

### Windows (10 or 11 on x86-64, 11 on arm64)

On x86-64 (Windows 10 or 11), get `ps5-dump-forge-<version>-windows-x86-64.zip` from Releases; it uses the Microsoft Edge WebView2
Runtime, which Windows 11 and most Windows 10 PCs already have. If yours doesn't (or you can't install
it), get `ps5-dump-forge-<version>-windows-x86-64-webview2.zip` instead: larger, but it carries its own copy
of the runtime in a `WebView2` folder next to the exe. On arm64 (Windows 11 only), get
`ps5-dump-forge-<version>-windows-arm64.zip`; it uses the installed WebView2 Runtime, which Windows 11 ships. Extract the zip to a folder you can write to (not
Program Files, not a network share) — `PS5 Dump Forge.exe` keeps its WebView data in a `data/webview`
folder beside itself, and that folder needs to be writable.

The app is unsigned, so Windows SmartScreen may block the first launch: click **More info**, then **Run
anyway**. If WebView2 is missing or broken, the app shows a message box explaining what to do instead of
a blank window.

#### Command-line tool

The zip also has `ps5-dump-forge.exe`, the same CLI as macOS and Linux:

```sh
ps5-dump-forge.exe inspect PPSA01234.exfat
ps5-dump-forge.exe convert C:\Games\PPSA01234 --to ffpkg
```

### Linux (x86-64 or arm64, glibc 2.35 or later, e.g. Ubuntu 22.04 or newer)

Get `ps5-dump-forge-<version>-linux-x86-64.tar.gz` or
`ps5-dump-forge-<version>-linux-arm64.tar.gz` from Releases and extract it (`tar -xzf`) to a folder you
can write to, then run `./forge.sh`. It needs no FUSE and no install; keep the `PS5 Dump Forge.AppDir`
folder next to `forge.sh` (the app keeps its WebView data in a `data/webview` folder beside it, same as
Windows).

#### Command-line tool

The tarball also has `ps5-dump-forge`:

```sh
./ps5-dump-forge inspect PPSA01234.exfat
./ps5-dump-forge convert ~/Games/PPSA01234 --to ffpkg
```

### PS5 (jailbroken, with an ELF loader)

Get `ps5-dump-forge-<version>-ps5.elf` from Releases. It is PS5 Dump Forge running on the console as an ELF
payload: the same conversions, read from and written to the console's own storage, with the app's UI as a
web page. Tested on firmware 13.60 with elfldr and itsPLK's webkit autoloader.

- **Load it** with any ELF loader: send it to elfldr's port 9021 (`nc -w 3 <ps5-ip> 9021 <
  ps5-dump-forge-<version>-ps5.elf`, or any ELF sender), or list it in an autoloader's `autoload.txt`.
  A notification shows the address.
- **Open the UI** in the PS5 browser or any browser on the same network: `http://<ps5-ip>:8095`. The file
  browser starts at `/data` (the internal SSD) and the drives that are mounted (`/mnt/usb0..7`,
  `/mnt/ext0..1`). Closing the browser doesn't stop a job; reopening the page picks it up again.
- **Home-screen tile**: on start, Forge adds a tile, "PS5 Dump Forge" (`PDFG00001`), that opens the UI in
  the PS5 browser. It also saves a copy of itself to `/data/ps5-dump-forge/ps5-dump-forge.elf`, so after a
  console restart the tile can start Forge again: the tile's page (kept by the browser) asks elfldr on port
  9021 to run that copy (or a `ps5-dump-forge.elf` at a USB drive's root). That needs the jailbreak or
  autoloader to have run first. Open the tile once while Forge runs, so the browser keeps the page. (Tested
  on the console: after a reboot and the autoloader, the tile starts Forge, also after the tile was removed and
  Forge loaded fresh again.)
- **No access control.** No password, pairing or other check: anyone who can reach the console on the network,
  and any web page open in a browser there, can browse its files and start jobs. Use it on a trusted network
  only.
- **USB output is experimental**: not tested on hardware. Each job first probes the output folder (a test write,
  `fsync` and read-back, the file-size limit, case folding, hard links) and refuses what it isn't sure of; the
  job log says what the probe saw. Hardware runs so far used `/data` only.
- **Logs**: `/data/ps5-dump-forge/serve-<pid>.txt`.
- **Stop it** with "Stop PS5 Dump Forge" at the top right of the page (it asks first when jobs are running,
  cancels them and cleans up). Loading it a second time only shows "already running".

#### Speed on the console

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

That is ~95% of the drive's ~288 MB/s write ceiling. Every cancel left no `.part` behind.

## Build

Rust (latest stable) and Node.js:

```sh
cargo test --workspace --release
cd app && npm ci && npm run tauri dev
```

On Linux the app also needs Tauri's WebKitGTK build dependencies (Debian/Ubuntu package names, as CI
installs them):

```sh
sudo apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev patchelf file
```

The PS5 payload builds in Docker (any macOS or Linux host with Docker and Node; the web UI is built on the
host and embedded):

```sh
ps5/build.sh                     # target/ps5/ps5-dump-forge.elf (+ stage1.elf, and .elf.txt from llvm-readelf)
ps5/test-entry.sh                # host check of entry.c (launch modes, args file, log redirection)
scripts/release-ps5.sh           # dist/ps5-dump-forge-<version>-ps5.elf, checked; SKIP_BUILD=1 checks what's built
scripts/test-freebsd.sh          # core + server tests on FreeBSD UFS, FAT32 and nullfs volumes (qemu VM in Docker)
cargo run -p ps5-dump-forge-cli -- serve --root <dir>       # the web UI on this computer, http://localhost:8095
(cd app && npm run build:http && node scripts/smoke.mjs)    # the web bundle + client against the real serve
FORGE_DEBUG_API=1 ps5/build.sh   # bring-up only: adds POST /api/debug; never shipped (release-ps5.sh refuses it)
```

See `AGENTS.md` for the layout and the format rules, and `DESIGN.md` for the UI design system.

## PS5 payload internals

For developers; the user side is under [PS5](#ps5-jailbroken-with-an-elf-loader).

### Harness (`ps5/`)

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
  `build.rs` embeds (see Self copy). `FORGE_DEBUG_API` stops after stage 1.
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
  never replays it; its output goes to `log-<pid>.txt`. It exports `ps5_notify` (at most 3,074 bytes).
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

### Web server (`crates/ps5-dump-forge-server`, CLI `serve`)

- Std only: hand-rolled HTTP/1.1, one request per connection, no server push; the page polls `GET
  /api/jobs` (every 1 s while a job is unfinished, else 5 s). Binds `0.0.0.0:8095` (`--port`). Roots for the
  file browser: on the PS5 `/data`, `/mnt/usb0..7`, `/mnt/ext0..1` (a drive counts only while `statfs` names
  it as its own mount point with blocks: those folders exist with nothing plugged in; re-checked on each
  use), canonicalized and logged; on a computer the `--root` folders.
- **No protections, by decision**: no pairing, token, `Host`/`Origin`/CSRF checks, security headers or
  path confinement; paths go to core as given, as in the Tauri app. No delete route, and none may be added.
- Limits: head 16 KiB, body 1 MiB (`Content-Length` only), 10 s per request, 30 s per write, 16 handler
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
| `POST /api/start_job` | `{request: ConvertRequest}` | job id |
| `POST /api/cancel_job` | `{id}` | `null` (idempotent) |
| `POST /api/stale_parts` | `{dirs}` | `string[]`, without running jobs' parts |
| `POST /api/list_dir` | `{path: string\|null}` | `{path, parent, entries: [{name, path, dir, size}], truncated}` |
| `GET /api/jobs` | | `{instance, jobs: [{id, request, progress, done, log, log_total}]}` |
| `POST /api/quit` | `{}` | `{}`, then cancels every job, waits for cleanup and exits |

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

### Tile, self copy and launch on open

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

### Writing safely on the PS5

Console-side lessons (from ps5upload and ShadowMountPlus): a cross-device `rename()` panics the kernel;
sustained multi-GB writes panicked it from dirty-page buildup; Sony's exFAT (`exfatfs`) shows freed space as
used until the folder is fsynced; some filesystems refuse `fsync` with `EINVAL`/`ENOTSUP`; `fsync` fails
transiently; sparse files on PS5 UFS collapse to 2–3 MiB/s. `/data` is `bfs` or `ufs`, sometimes through
`nullfs` (on the test console: `nullfs`, no hard links). The code labels each piece U1–U8:

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

## TODO

- Hardware-test every format on a console before the first release
- PS5 payload: boot a Forge-made image with SMP, the other output formats and USB output on the console
- Run the Windows and Linux builds on real machines (not just the CI runners that produce them)
- FPKG patches and DLC; extracting packages from other builders
- Batch queue, history and library screens; upload to console
- Replace the vendored ps5upload crates with this project's own implementation

Full list with details: [TODO.md](TODO.md).

## Thanks

Big thanks to **[PhantomPtr](https://github.com/phantomptr)** for
**[ps5upload](https://github.com/phantomptr/ps5upload)**. Its Rust engine is the reference implementation this
project learned the PS5 formats from: the FPKG package builder and reader (PFS, PFSC, Kraken, CNT/FIH,
PlayGo, debug license), and the exFAT and UFS2 readers. Its crates are vendored here for now
(`vendor/`, v6.1.2); replacing them with this project's own implementation is on the TODO list.

Big thanks to **[drakmor](https://github.com/drakmor)**, author of
**[ShadowMountPlus](https://github.com/drakmor/ShadowMountPlus)**, for the mount side this tool writes for and
for help along the way: the image-building scripts (`mkexfat_macos.sh` and friends) whose sizing and layout
rules the `.exfat`/`.ffpkg` writers follow, the format guidance in SMP's README and release notes, and ppr-patch,
which makes debug `.pkg` installs possible.

Credits for other references are in `THIRD-PARTY-NOTICES.md`.

## License

GPL-3.0-or-later. See `LICENSE`.
