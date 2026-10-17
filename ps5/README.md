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
  leaves its `.part`, which the next start lists. UnRAR's C++ (`unrar_sys`: `.rar` sources work on the
  console too) is built with `prospero-clang++` (`CXX_`/`CXXFLAGS_x86_64_ps5_freebsd`), and `linker.sh`
  links the SDK's libc++, libc++abi and libunwind.
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
  `/data/dumpforge/serve-<pid>.txt`. A developer hook: an args file `/data/dumpforge/args` (one
  argument per line, the command first; CR dropped, blank lines skipped; over 16 KiB, a NUL or more than 62
  arguments is refused, never cut short) is renamed to `args.done` before it runs, so an autoloaded boot
  never replays it; its output goes to `log-<pid>.txt`. Before either log is opened, earlier runs'
  `serve-<pid>.txt`/`log-<pid>.txt` whose process is gone (`kill(pid, 0)` → `ESRCH`) are deleted except
  the newest by mtime, so the run before a crash stays readable. It exports `ps5_notify` (at most 3,074 bytes).
  Folder migration: the PS5 version was "PS5 Dump Forge" with `/data/ps5-dump-forge`; it is "Dump Forge"
  with `/data/dumpforge` now (the desktop app keeps "PS5 Dump Forge"). Before the folder is created, a
  payload launch (not the plain CLI) renames `/data/ps5-dump-forge` to `/data/dumpforge` with one
  `rename(2)` when the old one is a folder and the new one doesn't exist; a failure notifies "cannot move"
  and the launch goes on with a new folder. After the folder is made, a relative link `/data/ps5-dump-forge`
  → `dumpforge` is created where nothing is at the old path (one `symlink(2)`, silent on failure, a real
  old folder never replaced), so a launcher page an earlier version left in AppCache (it knows only the
  old path) still starts the saved copy. Nothing walks through it: the Library doesn't follow links, the
  self copy, tile and logs use `/data/dumpforge`. `ps5/test-entry.sh` covers rename, link, link added
  later, both folders, and a refused rename.
- `compat.c`: `statfs@FBSD_1.0` / `fstatfs@FBSD_1.0` forwarders (outside `shim/`), and
  `--wrap=cpuset_getaffinity`: the PS5 refuses the process-level query std's `available_parallelism`
  makes (ERANGE; std then said 16 CPUs from `sysconf`), so it is answered from the thread's mask (a
  payload gets 5, `0xea00`) and every pool, the vendored crates' included, is sized by it. `launcher.c`: registers
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
  path confinement; paths go to core as given, as in the Tauri app. One delete route, by user decision
  (`delete_path`, Edit's and the Library's; the page's question authenticates nobody).
- Limits: head 16 KiB, body 1 MiB (`Content-Length` only; `edit_upload` streams any size, 30 s idle per read), 10 s per request, 30 s per write (a download:
  per 1 MiB chunk), 16 handler
  threads (then 503), 8 unfinished jobs (then 429), 2 inspections, naming, `host_tree` or `edit_check` requests at once (429,
  `Retry-After: 1`), 32 finished jobs and 200 log lines each kept, `list_dir` 10,000 entries (`truncated`),
  `stale_parts` 1,000. `compression_threads: null` is `max(1, cores − 1)`; on the PS5
  `cores` is the payload's 5 CPUs (`compat.c`) and two are left free (3 workers), for the console's
  browser.
- Errors are `{"error": "<text>"}` with 400 / 404 / 409 (stopping, a path a job uses) / 413 / 429 / 500 (`self_update`: 502 / 503); core errors are the
  Tauri commands' `format!("{e:#}")`. API replies are `Cache-Control: no-store`; `index.html` and the
  AppCache manifest `no-cache`; hashed `assets/*` a year, immutable.

| Route | Body | Returns |
|---|---|---|
| `GET /api/session` | | `{app: "ps5-dump-forge", version, platform: "ps5"\|"host", instance, stopping, self_copy, separator: "/"\|"\\", url, handover: true}` (`url`: the page header's host:port; `handover`: takes `quit {"if_idle": true}`) |
| `GET /api/qr` | | `url` as a QR code, `image/svg+xml` (the header's and About's); 404 when the server can't tell its IP |
| `POST /api/inspect` | `{path}` | `Inspection` |
| `POST /api/default_output` | `{source, format, dir}` | path |
| `POST /api/generated_output` | `{source, format, dir, taken}` | path |
| `POST /api/start_job` | `{request: ConvertRequest}` | job id (`lz4_in_place: true` replaces an `.exfat`/`.ffpkg` source; `output` ignored; non-empty `edits` is an edit job of the source, in place or with `edit_rebuild`, `output` and `format` ignored; non-empty `extract` copies those game paths into the folder `output`, `format` `folder`) |
| `POST /api/cancel_job` | `{id}` | `null` (idempotent) |
| `POST /api/stale_parts` | `{dirs}` | `string[]`, without running jobs' parts (an in-place or edit job's beside its source) or a staging folder an upload or unfinished job uses |
| `POST /api/edit_check` | `{request: ConvertRequest}` | `EditCheck {in_place, rebuild, needed_inside, free_inside, rebuild_needs, free_beside}`: why each mode can't apply (empty = it can) and the sizes (bytes, `null` where they don't apply); reads only, except that a leftover undo journal no job uses is restored first (after the ShadowMount+ check) and the U1 probe writes and removes its file. The source and every `Add.from` must be a folder or regular file (else 400); counts as an inspection (429), 409 once stopping |
| `POST /api/host_tree` | `{path}` | `{kind: "file"\|"dir", size, files: [{path, size}], empty_dirs, truncated}`: a file or folder as an edit's add will read it (junk left out; a link or special file refused); `truncated` past 200,000 entries or 15 s. 404 missing, counts as an inspection (429), 409 once stopping, 500 with core's message |
| `POST /api/edit_upload?source=&path=` | raw bytes (`Content-Length` required) | `{path}`: the staged file, `<source>.forge-upload-<pid>.part/<path>`, where the UI sends `path` as `<tag>-<k>/<game path>` (`stagingNames`: a random tag per Apply) (canonical source; folders made on demand, 0777 on the PS5), written to a temporary name and renamed over. 411 no length, 400 bad query, game path or a body that ended early or stalled, 404 source missing, 409 a non-file in the way, the same target uploading, an unfinished job using it, or stopping |
| `POST /api/edit_discard` | `{source}` | `null`: the source's staging folder removed by `delete_path`'s rules (missing: `null`); 409 while an upload into it or a job using it is unfinished, or stopping |
| `POST /api/lz4_patch` | `{source}` | `Lz4Patch`; 409 while a job is queued or running |
| `POST /api/lz4_unpatch` | `{source}` | `Lz4Patch` (release runtime); 409 while a job is queued or running |
| `GET /api/lz4_traces?source=` | | both trace files as one zip download (below) |
| `POST /api/delete_path` | `{path}` | `null`: the file or folder is gone for good (below); 400 refused, 404 missing, 409 a job uses it or stopping, 500 failed (a folder may be partly gone) |
| `POST /api/library_scan` | `{}` (an object; `root` string scans just that folder on any filesystem (a network share included; a chosen root that is a link is resolved first, links inside the walk are never followed, `/mnt/shadowmnt` refused; system folders such as `/` or `/usr` are refused with core's message), non-string 400; `root` is required on a computer (400 "choose a folder to scan" without it) and optional on the PS5) | `{roots, entries: [{path, kind, title_id, title_name, version, has_cover, lz4_packed, additional_content}], truncated, warnings, storage?}`: the Library's scan of that folder, or on the PS5 of the server's current roots (browse roots without `/mnt/shadowmnt`, re-resolved each call so a newly attached drive appears on Refresh). `storage` `{free, total}` (bytes, `statfs` of `/data`) only on the PS5, read on each scan. Reads only; counts as an inspection (429 past 2), 409 once stopping, 500 with core's message. Partial results set `truncated` and add `warnings` (core's scan limits and 15 s cooperative budget). The request deadline governs receiving the request, not core's work, which can't be interrupted |
| `POST /api/library_cover` | `{path}` | a PNG data-URL string, or `null` (no icon, too large, not a PNG); fetched lazily per card. Same cap, 409 and 500 rules (an unreadable source is a 500) |
| `POST /api/list_dir` | `{path: string\|null}` | `{path, parent, entries: [{name, path, dir, size}], truncated}` |
| `GET /api/jobs` | | `{instance, jobs: [{id, request, progress, done, log, log_total}]}` |
| `POST /api/quit` | `{}` | `{}`, then cancels every job, waits for cleanup and exits |
| `POST /api/quit` | `{"if_idle": true}` | `{}`, then exits as above, only when idle: 409 "Dump Forge is busy: …" while a job is queued or running, a `lz4_traces` download streams or a patch or delete holds admission past 1 s, 409 "Dump Forge is stopping" (the handover's request) |
| `GET /api/elfldr` | — | `{up}`: elfldr accepts a connection (1 s; connect and close, nothing sent); 404 on a computer |
| `POST /api/self_update` | `{version}` (`X.Y.Z`, newer than this one) | `{}` once elfldr has answered (downloaded and started it); this server keeps serving until the new copy takes over when idle (below); 404 on a computer, 400 bad or not newer, 409 stopping, a job queued or running, a patch or delete holding admission past 1 s, or another update in flight, 502 elfldr refused or failed, 503 elfldr not listening |

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

`delete_path` (core's `Jobs::delete_path`): a file, or a folder with what is inside (links removed, not
followed), then its folder fsynced. Refused (400): an empty path, a link itself, a special file, a volume
root or mount point, a folder with another volume mounted inside (by `st_dev`), every browse root
(configured and mounted now) and the folders holding one. 409: anything at, in or above what an unfinished
job reads or writes (source, output, `.part`s, profile, traces), by identity after resolving links. The
check and the removal hold the job table and admission, so no job or quit starts in between. No Trash, no
undo.

`instance` is random per start, so the page notices a restart and rebuilds its job list. `list_dir` with
`null` lists the roots; directories first, then by name; non-UTF-8 names left out; symlinks resolved.
Events keep core's serde shape (`{"Ok": …}` / `{"Err": "cancelled"}`); ids stay below 2^53. A job's table
entry is made by its first event (core can emit `Done` before `Jobs::start` returns). Quit closes
admission first (`start_job` → 409); on a computer SIGINT/SIGTERM does the same. A port already taken by
Forge is taken over when that copy is idle, else "Can't close Dump Forge - some jobs are still running."
and exit 0 (see [Updates](#updates)); by anything else, exits 1.

- The URL (session `url`, the page header, the host's start notice, the PS5 serve log) uses the address of
  a UDP `connect` toward the internet (no packet sent). On the PS5 the start notification only says
  "Dump Forge is launched", in the console's language (`notice.rs`).
- The web bundle: `npm run build:http` (Vite mode `http`) writes `app/dist-http` with `transport-http.tsx`
  in place of Tauri (`Picker.tsx` for the dialogs, `http-client.ts`'s poller for the events); `build.rs`
  embeds it, or a placeholder page with a warning (`FORGE_REQUIRE_WEB=1`, set by `ps5/build.sh`, makes that
  an error). Paths are the server's (`/`), never the viewer's. Plain-node checks: `app/scripts/`
  (`check-paths`, `check-poller`, `check-launcher`, `smoke`).

## Tile, self copy and launch on open

- Tile (`tile.rs`, `ps5/launcher.c`): `/user/app/PDFG00001/sce_sys/{param.json,icon0.png}`, `deeplinkUri`
  `http://127.0.0.1:<port>/`, icon `ps5/icon0.png`. Registered only when a file was written. A `PDFG00001`
  not marked by `/data/dumpforge/tile-PDFG00001.owner` is another title's and is never touched. Its
  header is "Dump Forge home-screen tile"; the old "PS5 Dump Forge home-screen tile" (a migrated folder)
  still marks it ours, so the first start rewrites `titleName` "PS5 Dump Forge" → "Dump Forge", registers
  again and records the new header. While `/data/dumpforge/tile-PDFG00001.owner` is missing (the rename
  failed, or both folders existed), `/data/ps5-dump-forge/tile-PDFG00001.owner` is taken over when its
  folder is a real one (not the link), it is a regular file and it names this title folder (header and
  `(st_dev, st_ino)`): rewritten in the new place, then the tile as above. No valid record in either
  place: another title's. Runs in
  the background and never fails the server; the serve log says what it did.
- Self copy (`self_copy.rs`): stage 2 embeds `PDFGSELF`, u64 LE raw length, u64 LE compressed length,
  SHA-256 of stage 1, then stage 1 zlib-compressed (6.3 MB ELF holding 1.7 MB for a 4.5 MB copy). On start a
  thread unpacks it (bounded, hash, ELF magic and version marker checked) into
  `/data/dumpforge/ps5-dump-forge.elf`: written when missing, older, the same version with other bytes,
  or unmarked; kept when identical or newer. One save at a time across processes (`flock` on
  `.ps5-dump-forge.elf.lock` beside it), so two copies starting together keep the newest. Temp file
  `O_EXCL|O_NOFOLLOW`, fsync, rename, folder fsync. A
  copy that loses the port race, and quit, finish the save first. `GET /api/session` `self_copy`:
  `saving`, `saved`, `up to date`, `kept newer <v>`, `failed: <why>`, or `none` (stage 1, debug, computers).
- Launch on open (`launcher.ts`, `appcache.ts`): the page is kept by AppCache (`forge.appcache`, a hash over
  every bundle file). Only on the console's own browser (a loopback host) and only at page load: if `GET
  /api/session` doesn't answer, it sends `GET
  http://127.0.0.1:9021/data/dumpforge/ps5-dump-forge.elf?args=serve&pipe=0` (then the old
  `/data/ps5-dump-forge/ps5-dump-forge.elf`, the link or a folder a failed migration left, then the USB
  copies) and
  waits up to 30 s. A newer payload's manifest swaps the page in and reloads once, never mid-dialog.

## Updates

- The page checks GitHub at every start (always on, no setting, nothing stored; `app/src/update.ts`): the
  latest release's `tag_name` `vX.Y.Z` newer than session `version`, an asset named
  `ps5-dump-forge-<v>-ps5.elf` (uploaded, not empty) and elfldr answering (`GET /api/elfldr`), else no
  toast; any failure on the way (offline, DNS, timeout, an older server without the route) is silent. `serve` on a computer (`platform: "host"`) never offers one.
  **Update** posts `self_update`, then polls `GET /api/session` every 1 s until `instance`
  changes (network errors meanwhile are expected: the old server exits on the handover), then lets AppCache
  swap in the new page. One minute in all from the request, download included; then the toast says to stop
  Dump Forge and open it again (a downloaded version that couldn't take over, such as v0.1.1 without
  handover, has saved itself: the tile starts it).
- `self_update` (`elfldr_load` in `api.rs`): admission checked like `lz4_patch` and released before
  elfldr is asked (the new copy's `if_idle` needs it), one at a time (an `AtomicBool`). It never stops this
  server itself. Connects to `Options.elfldr` (`127.0.0.1:9021`) within 3 s and sends exactly
  `GET /?uri=<percent-encoded U> HTTP/1.1`, `Host`, `Connection: close`, with U =
  `https://github.com/quer3q/ps5-dump-forge/releases/download/v<v>/ps5-dump-forge-<v>-ps5.elf?args=serve&pipe=0`
  (elfldr reads `args`/`pipe` from U's own query). Reads the reply (64 KiB, 55 s: the download happens
  inside it): no `200` → 502 "elfldr refused the request"; a body line with `Error` (elfldr's `Error
  reading HTTP payload` / `Error spawning payload`) → 502 "elfldr couldn't download or start v<v>: <line>".
- Handover: a newly loaded copy of any version (elfldr's `?uri=`, `nc`, an autoloader; an older or the same
  version too) that finds the port held by Forge closes it when idle and takes its place. It reads the
  running copy's session; one with `handover` gets `quit {"if_idle": true}`; one without (v0.1.1 and
  older: they ignore the body and cancel their jobs on quit) first gets `GET /api/jobs` and a plain `quit {}`
  only when every listed job has `done` (`ponytail:` in `lib.rs`: a job admitted between the two requests
  isn't listed yet and is cancelled; a trace download it streams is cut). 3 s client, shared with the
  session probe. A start that couldn't serve (no usable `--root` on a computer) fails before asking. On
  200 it retries `bind` every 200 ms up to 30 s, then (PS5) waits for its self copy and
  saves it again, and serves (the self copy still keeps the newest version: an older ELF runs, but the
  tile starts the newer saved one next time). Busy (a 409, or an unfinished job listed) notifies "Can't
  close Dump Forge - some jobs are still running." and exits 0; no answer or a port still taken after 30 s
  notifies "Dump Forge v<new> could not take over from v<old>" and fails. Both notices are in the
  console's language (`notice.rs`).
- Open risks (unverified on hardware; `ponytail:` in `api.rs`): elfldr's sceHttp has to follow GitHub's
  302 to its asset host and GitHub to ignore the extra query; elfldr ignores certificate errors and nothing
  checks a signature (whoever can intercept the console's traffic can serve an ELF); it reads the body once
  without checking its length, so a short read could start a cut ELF.

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
- **Edits** (the Edit tab): an image ShadowMountPlus has mounted is refused in place and for a rebuild (an
  entry `<stem>_<8 hex>` of `/mnt/shadowmnt` or `/mnt/shadowmnt/pfsc` whose `st_dev` differs from its
  folder's; a listing error other than not found refuses too), at scan and again right before writing or
  replacing: the kernel's cached view of a mounted (`image_rw=`) image would undo or damage the edit. The
  undo journal (`<image>.forge-undo`) and the image are synced through U4 (a retried sync fails the job),
  room for the journal is checked through U1 on the image's folder, and so is the room for a rebuild;
  `edit_check` runs that probe too, so each check writes and removes a probe file there. A folder edit and
  an Extract run the U1 probe on the game folder or the destination (room for the added bytes + 1 MiB) and
  U2's name rules from it; their folders and files are opened up to 0777 like every payload write. Uploads
  are staged 0777 beside the source. Not yet run on the console.
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
| 007 `.ffpkg` → `.exfat` (UFS2 reader before vendor patch ufs2-range-reads, now 0006) | 87.3 GB | 26 min | full |
| 007 `.ffpkg` → folder (with patch 0006) | 86.9 GB | 11.6 min | full |
| Directive 8020 folder → `.exfat` | 59.2 GB | 4.75 min (~208 MB/s end to end) | fast, 377 samples, 2.1 GiB |

Write speed of folder → `.exfat` through the speed work (15 s samples of the same game):

| Build | Write speed |
|---|---|
| One thread, two hash passes | ~203 MB/s |
| Read-ahead on its own thread + one hash pass (slice hashes only) | ~265 MB/s |
| No hashing of sync intervals (a retried `fsync` fails the job instead) | ~273 MB/s |

That is ~95% of the drive's ~288 MB/s write ceiling. Every cancel left no `.part` behind. Vendor patches
are listed in [vendor/README.md](../vendor/README.md).
