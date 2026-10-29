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
FORGE_DEBUG_API=1 ps5/build.sh   # bring-up only: adds POST /api/debug, GET /probe, POST/GET /api/debug_input; never shipped (release-ps5.sh refuses it)
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
  `libSceNet.sprx` (the SDK libc's `getaddrinfo`: NAS and Add a device host names),
  `libSceSystemService.sprx` (rest-mode tick), `libSceNotification.sprx` (job-end toasts) and
  `libSceAppInstUtil.sprx` (the tile, package installs), no `PT_INTERP`, no TLS.
  `scripts/release-ps5.sh` checks that, the entry point, that every `app/dist-http` file is embedded byte for
  byte, no debug routes, the version marker (`VERSION_MARKER` in the server) against the workspace version,
  and the self copy (stage 1 carries none; the release ELF exactly one, decompressing to stage 1).

## Package installer

`installer.c` calls `sceAppInstUtilInstallByPackage` with a URL of at most 127 bytes, served by the
install job on `127.0.0.1`. It initializes AppInstUtil once, swaps this process to the SYSTEM install
authid, every capability and attrs 0x80 for the Sony calls (ps5upload's escalation), and restores the
prior ones afterward. A refusal of 0x80B22404 (Sony's pre-flight of the URL, no byte fetched; ps5upload ties it
to FW < 11 without ShellCore's authid) is asked once more as ShellCore (`0x3800000000000010`). Measured on the
user's console (2026-10-29): Sony's pre-flight asks for the URL with a query of its own
(`?product=0287&serverIpAddr=127.0.0.1&r=00000000`); `install.rs` matches the path without it (a 404 there was
the 0x80B22404). With that, a DLC unlocker installed (accepted, fetched whole, finished by the console itself). The 0x70-byte MetaInfo and all of
PlayGoInfo, including its 6480-byte tail, start zeroed; only the URL and name hint are set. A return
of 0 means Sony accepted the request and will fetch the package over HTTP Range requests (core's
`install.rs` serves it: at most 16 connections, HEAD and a `.crc` sidecar answered, done once every byte was
served and 2 s pass quiet, failed after 5 min with no request; the job never sees the install itself finish). Otherwise
`forge_pkg_install` returns -1 and writes the error code through `code` (an overlong URL gets SCE
kernel EINVAL before calling Sony). Calls are serialized and logged in the serve log.

## Web server (`crates/ps5-dump-forge-server`, CLI `serve`)

- Std only: hand-rolled HTTP/1.1, one request per connection, no server push; the page polls `GET
  /api/jobs` (every 1 s while a job is unfinished, else 5 s; unanswered, 2 s doubling to 10 s while the
  header says "Offline, retrying…"). Binds `0.0.0.0:8095` (`--port`). Roots for the
  file browser: on the PS5 `/data`, `/mnt/usb0..7`, `/mnt/ext0..1` (a drive counts only while
  `statfs` names it as its own mount point with blocks: those folders exist with nothing plugged in;
  re-checked on each use), canonicalized and logged; on a computer the `--root` folders. On the PS5 (by
  user request) they are all a listing reaches: `list_dir`, `host_tree` and a Library folder at any
  other path (resolved) are 400 "<path> is not on this console's storage or a plugged-in drive";
  `/mnt/shadowmnt` isn't listed.
- **No protections, by decision**: no pairing, token, `Host`/`Origin`/CSRF checks, security headers or
  path confinement (but the PS5's listings, above); paths go to core as given, as in the Tauri app. One delete route and one move route,
  by user decision (`delete_path`, Edit's and the Storage tab's; `move_path`, its File explorer's; the
  page's question authenticates nobody).
- Limits: head 16 KiB, body 1 MiB (8 MiB for `start_job`, `edit_check` and `existing_targets`; `Content-Length` only; `edit_upload` streams any size, 30 s idle per read), 10 s per request, 30 s per write (a download:
  per 1 MiB chunk), 16 handler
  threads (then 503), 8 unfinished jobs (then 429), 2 source reads at once (inspections, naming, `host_tree`, `edit_check`, `existing_targets`,
  Library scans and covers; a further read waits up to 30 s for a slot, at most 4 waiting, then 429 `Retry-After: 1`), 32 finished jobs and 200 log lines each kept, `list_dir` 10,000 entries (`truncated`),
  `stale_parts` 1,000. `compression_threads: null` is `max(1, cores − 1)`; on the PS5
  `cores` is the payload's 5 CPUs (`compat.c`) and two are left free (3 workers), for the console's
  browser.
- Errors are `{"error": "<text>"}` with 400 / 404 / 409 (stopping, a path a job uses) / 413 / 429 / 500 (`self_update`: 502 / 503); core errors are the
  Tauri commands' `format!("{e:#}")`. API replies are `Cache-Control: no-store`; `index.html` and the
  AppCache manifest `no-cache`; hashed `assets/*` a year, immutable.

| Route | Body | Returns |
|---|---|---|
| `GET /api/session` | | `{app: "ps5-dump-forge", version, platform: "ps5"\|"host", instance, stopping, self_copy, separator: "/"\|"\\", url, connect_url, handover: true, debug, notifications: bool}` (`url`: the server's host:port, the Connect tab's IP; `connect_url`: `<url>/#connect`; `handover`: takes `quit {"if_idle": true}`; `debug`: true only in a `FORGE_DEBUG_API` PS5 build, the page then shows **Input probe**; `notifications`: job-end notices, saved on the PS5) |
| `POST /api/notifications` | `{on: bool}` | `{on: bool}`: save the PS5 About switch for job-end notices; 500 if the save fails |
| `GET /api/qr` | | `url` as a QR code, `image/svg+xml`; `?connect=1`: `connect_url` (the Connect tab's, web build); 404 when the server can't tell its IP |
| `POST /api/inspect` | `{path}` | `Inspection` |
| `POST /api/default_output` | `{source, format, dir}` | path |
| `POST /api/generated_output` | `{source, format, dir, taken}` | path |
| `POST /api/start_job` | `{request: ConvertRequest}` | job id (`zstd_pack` + `zstd_level` packs into a folder, `zstd_unpack` unpacks zstd packs, `lz4` `pack`/`unpack` the LZ4 ones; non-empty `edits` is an edit job of the source, in place or with `edit_rebuild`, `output` and `format` ignored; non-empty `extract` copies those game paths into the folder `output`, `format` `folder`; non-empty `transfer` is a transfer job: those files and folders (local, `flink://` or `smb://`) into the folder `output` (local or `flink://`), `source` and `format` ignored, run in the transfer slot beside the one heavy job; a job touching an unfinished job's paths of the other kind ends at once "in use by another job"; `install` (a `.pkg` here, `flink://` or on a NAS) is an install job, PS5 only, also in the transfer slot: the package served on `127.0.0.1` to the console's installer, below) |
| `POST /api/cancel_job` | `{id}` | `null` (idempotent) |
| `POST /api/stale_parts` | `{dirs}` | `string[]`, without running jobs' parts (an edit job's beside its source) or a staging folder an upload or unfinished job uses |
| `POST /api/edit_check` | `{request: ConvertRequest}` | `EditCheck {in_place, rebuild, needed_inside, free_inside, rebuild_needs, free_beside}`: why each mode can't apply (empty = it can) and the sizes (bytes, `null` where they don't apply); reads only, except that a leftover undo journal no job uses is restored first (after the ShadowMount+ check) and the U1 probe writes and removes its file. The source and every `Add.from` must be a folder or regular file (else 400); counts as an inspection (the 2-read cap), 409 once stopping |
| `POST /api/existing_targets` | `{request: ConvertRequest}` | an extract's or transfer's targets that exist already (each selection's, top level; on a peer or NAS too) as an array of paths, so the page asks before it sends `replace`; 400 for a request that is neither or a bad body, 500 when the output can't be read or isn't a folder; counts as an inspection (the 2-read cap), 409 once stopping |
| `POST /api/host_tree` | `{path}` | `{kind: "file"\|"dir", size, files: [{path, size}], empty_dirs, truncated}`: a file or folder as an edit's add will read it (junk left out; a link or special file refused); `truncated` past 200,000 entries or 15 s. 400 on the PS5 off `/data` and the drives, 404 missing, counts as an inspection (the 2-read cap), 409 once stopping, 500 with core's message |
| `POST /api/edit_upload?source=&path=` | raw bytes (`Content-Length` required) | `{path}`: the staged file, `<source>.forge-upload-<pid>.part/<path>`, where the UI sends `path` as `<tag>-<k>/<game path>` (`stagingNames`: a random tag per Apply) (canonical source; folders made on demand, 0777 on the PS5), written to a temporary name and renamed over. 411 no length, 400 bad query, game path or a body that ended early or stalled, 404 source missing, 409 a non-file in the way, the same target uploading, an unfinished job using it, or stopping |
| `POST /api/edit_discard` | `{source}` | `null`: the source's staging folder removed by `delete_path`'s rules (missing: `null`); 409 while an upload into it or a job using it is unfinished, or stopping |
| `POST /api/delete_path` | `{path}` | `null`: the file or folder is gone for good (below); 400 refused, 404 missing, 409 a job uses it or stopping, 500 failed (a folder may be partly gone). A paired peer's `flink://` path is deleted by that peer under its own rules (409 while a job here uses it); a NAS path 400 |
| `POST /api/move_path` | `{from, to, replace?}` | `null`: moved or renamed on its volume, `to` never replaced unless `replace` (a file over a file, a folder merged into a folder; a kind mismatch 400, the old file refused as `delete_path` refuses it); `from` refused as `delete_path` refuses it (same roots kept), `to` refused inside `from`, on another drive, taken, or with an undo journal pending; 400 refused, 404 missing (source or target folder), 409 a job uses either or stopping, 500 failed. Two `flink://` paths on one peer are moved by that peer, as `delete_path`; across devices, or between here and a peer, 400 |
| `POST /api/library_scan` | `{}` (an object; `root` string scans just that folder on any filesystem (a network share included; a chosen root that is a link is resolved first, links inside the walk are never followed, `/mnt/shadowmnt` refused; system folders such as `/` or `/usr` are refused with core's message), non-string 400; `root` is required on a computer (400 "choose a folder to scan" without it) and optional on the PS5) | `{roots, entries: [{path, kind, title_id, title_name, version, has_cover, lz4_packed, zstd_packed, additional_content}], truncated, warnings, storage?}`: the Library's scan of that folder, or on the PS5 of the server's current roots (the browse roots, re-resolved each call so a newly attached drive appears on Refresh; a `root` off them is 400). `storage` `{free, total}` (bytes, `statfs` of `/data`) only on the PS5, read on each scan. Reads only; counts as an inspection (the 2-read cap), 409 once stopping, 500 with core's message. Partial results set `truncated` and add `warnings` (core's scan limits and 15 s cooperative budget). The request deadline governs receiving the request, not core's work, which can't be interrupted |
| `POST /api/package_scan` | `{}` | `{entries: [{path, size, content_id, title_id, title_name, title_names, version, content, has_cover}], truncated}`: the Package installer's list, every `.pkg` (any case) under the server's current roots, walked as `library_scan` walks (game folders entered too), each read only for its header and container (`content` `game`, `patch`, `additional_content` or `unknown`: unreadable, listed untitled); sorted by title, untitled last. 400 on a computer; same cap, 409 and 500 rules as `library_scan` |
| `POST /api/library_cover` | `{path}` | a PNG data-URL string, or `null` (no icon, too large, not a PNG); fetched lazily per card. Same cap, 409 and 500 rules (an unreadable source is a 500) |
| `POST /api/list_dir` | `{path: string\|null, explorer?: bool}` | `{path, parent, entries: [{name, path, dir, size, link, mounted}], truncated}` (`null`: the browse roots; `parent` `null` at a root; on the PS5 a path off them 400; `link`: the entry is a link, its `path` the resolved target; with `explorer` on the PS5 `mounted` marks an image ShadowMount+ has mounted) |
| `GET /api/jobs` | | `{instance, jobs: [{id, request, stages, progress, done, log, log_total}]}` |
| `POST /api/quit` | `{}` | `{}`, then cancels every job, waits for cleanup and exits |
| `POST /api/quit` | `{"if_idle": true}` | `{}`, then exits as above, only when idle: 409 "Dump Forge is busy: …" while a job is queued or running, a paired device reads or writes files here or a delete or move holds admission past 1 s, 409 "Dump Forge is stopping" (the handover's request) |
| `GET /api/elfldr` | — | `{up}`: elfldr accepts a connection (1 s; connect and close, nothing sent); 404 on a computer |
| `GET /api/connect/state` | | `{device: {key_id, name, platform, port, listen_error, ip}, devices, nas, discovery, pending, outgoing}`: this device (`ip` the default route's address or `null`), the pairing requests waiting for this device's user (`pending: [{id, name, platform, ip, check}]`), this device's own last request (`outgoing: {id, address, check, state, error, device}`, `state` asking, waiting, paired, denied, timed_out, busy, already_paired, too_many_attempts, failed or cancelled), the saved devices (≤ 9, `online`, `address`, `last_seen`), the saved NAS devices (`[{id, name, host, port, protocol, user, has_password, online, last_online}]`, `protocol` `smb`, `ftp` or `nfs`, never the password; `last_online` unix seconds since start, 0 = none) and the last Discover. Transfers are jobs (`start_job` `transfer`, `GET /api/jobs`) |
| `POST /api/connect/pair` | `{address}` | `null`: asks that device (`ip`, `ip:port` or `host[:port]`) to pair; its user allows or denies it there within 60 s, `state.outgoing` follows it (a new request cancels a running one); 400 not a local address or a bad one |
| `POST /api/connect/pair_cancel` | `{}` | `null`: cancels this device's waiting request |
| `POST /api/connect/approve` | `{id, allow}` | `bool`: this device's user answers a waiting request (`pending[].id`); `false` once it no longer waits. As open as every route here (by user decision) |
| `POST /api/connect/forget` | `{key_id}` | `bool`: the pairing removed on this side |
| `POST /api/connect/install` | `{key_id, path}` | `{job}`: asks the paired PS5 `key_id` to install this device's `.pkg` at `path` (inside the share root); the PS5 starts an install job reading it over FLINK (mirrored here) and answers its id; 400 off the share, a device that isn't a PS5 ("this device can't install packages") or its start's error |
| `POST /api/connect/discover_start` / `discover_stop` | `{}` | `null`: a 10 s search (Forge UDP 8094, mDNS, WS-Discovery) starts or stops; results in `state.discovery` |
| `POST /api/connect/nas_save` | `{id?, name, host, port?, user?, password?, protocol?}` | `{id}`: a NAS saved (`protocol` `smb`, `ftp` or `nfs`: `smb` when none is named, an edit naming none keeps its own; `host` `host[:port]`, port default 445, 21 or 2049 by protocol; an NFS entry keeps no user or password; one entry per host + port, updated in place; `id` an edit's entry, moved to the new address; `user`/`password` omitted keep the saved ones, `""` clears them); 400 a bad address or one another entry has |
| `POST /api/connect/nas_forget` | `{id}` | `bool`: the saved NAS (and its password) removed |
| `POST /api/connect/upload?dir=&name=` | raw bytes (`Content-Length` required) | `{path}`: a phone's file as `<dir>/<name>` via a `.part` and a no-replace rename; 411 no length, 400 not a plain name, 404 `dir` not a folder, 409 taken or stopping. `&replace=1` (the user confirmed it; on the chunked form's first chunk): a file there is replaced by renaming the synced `.part` over it through core's `Jobs::move_path` with replace (refused as `delete_path` refuses that file: a job using it 409, the kept roots and Forge's files, on the PS5 `/mnt/shadowmnt` and mounted images 400); a link, folder or special file there 400 before the body is read |
| `POST /api/connect/upload?dir=&name=&id=&offset=&size=` | one chunk's raw bytes | the web UI's parallel upload (File Transfer's **Upload to this device**, off in the console's own browser; 8 MiB chunks, 4 requests at once from a browser, where FLINK between devices uses 8 lanes; 35 MB/s to the PS5 where one request does 28): chunks of one `id` (1–32 letters/digits) land at their offsets in one `.part`; `{done: false}`, or `{path, done: true}` from the chunk that completes `size` bytes; `&cancel=1` (empty body) drops the upload and its part; a failed chunk drops the whole upload |
| `GET /api/connect/download?path=` | | File Transfer's **Download from this device**: the file, `application/octet-stream`, `Content-Disposition: attachment`; 404 missing, 400 not a file |
| `POST /api/self_update` | `{version}` (`X.Y.Z`, newer than this one) | `{}` once the release's ELF is downloaded and streamed to elfldr (which starts it); this server keeps serving until the new copy takes over when idle (below); 404 on a computer, 400 bad or not newer, 409 stopping, a job queued or running, a delete or move holding admission past 1 s, or another update in flight, 502 the download, the save over the self copy or the send failed, 503 elfldr not listening |

`delete_path` (core's `Jobs::delete_path`): a file, or a folder with what is inside (links removed, not
followed), then its folder fsynced. Refused (400): an empty path, a link itself, a special file, a volume
root or mount point, a folder with another volume mounted inside (by `st_dev`), every browse root
(configured and mounted now) and the folders holding one; on the PS5 also Forge's own files (its self copy,
that copy's lock and the tile owner record), anything at or in `/mnt/shadowmnt`, an image ShadowMount+ has
mounted and a folder holding one (fails closed), and an undo journal while its image is there. 409: anything at, in or above what an unfinished
job reads or writes (source, output, `.part`s), by identity after resolving links. The
check and the removal hold the job table and admission, so no job or quit starts in between. No Trash, no
undo.

`move_path` (core's `Jobs::move_path`): one rename, never replacing unless `replace` (then a file over a file in one rename, or a folder merged entry by entry), then both folders fsynced. `from` is
refused as `delete_path` refuses it, and so is a folder with a volume mounted inside; `to` must be a plain name in an existing folder (404 when it is
missing), not inside `from`, on the same drive (400 "can't move between drives"), not taken (400; with `replace` only a kind mismatch, a link or a file `delete_path` refuses), not at,
in or above an unfinished job's paths (409); an image with an undo journal beside it is refused (400: open
it in Edit first). Same locks as `delete_path`.

Connect (`connect.rs`, crate `ps5-dump-forge-flink`, `discovery.rs`, `mdns.rs`, `nas.rs`): started by `Server::new` when
`Options.connect_dir` is set (`serve` sets it: `/data/dumpforge/connect/` on the PS5, else the OS data
dir + `ps5-dump-forge/connect`; it holds `node.key`, `peers.json` and `nas.json`, all 0600); a failed start answers every
`/api/connect/*` with 503 and the reason, a failed call 400 `{error}`. Ports: **TCP 9883** FLINK
(`FORGE_FLINK_PORT` overrides; a control connection per paired device + up to 8 data lanes each for reads and writes) and **UDP 8094**
Forge discovery (passive responder always on; replies name, platform, FLINK port and key id, never the
code); Discover also sends mDNS to 224.0.0.251:5353 and a WS-Discovery Probe to 239.255.255.250:3702 (UDP
multicast, answered to its ephemeral port), then one HTTP POST (WS-Transfer Get, 2 s timeouts, 64 KiB) to each
Windows PC's port 5357 for its name. A saved NAS is online while a TCP connect to its port succeeds (each probe round). Pairing is local
networks only (`connect::is_local`: private and link-local IPv4, 100.64/10, loopback, IPv6 ULA and
link-local; a non-local `dial` is refused, a non-local inbound peer dropped; a NAS is not limited). FLINK frames are
MACed with BLAKE3 but **not encrypted** (plaintext on the LAN). A paired peer reaches this console's `/`
(the share root; a computer's is `/` too, a Windows PC's the drive holding its home folder), on the PS5 (by user
request) only `/data` and the drives mounted within it: `/` lists `data` (and `mnt` while a drive is mounted),
`mnt` the mounted drives, and every RPC refuses another path. On the PS5 the name is the console's nickname
(`sceRegMgrGetStr` 0x02050000). Pairing is approved on the device asked (FLINK v2: X25519 with a commitment, a six-digit check number both screens show). Session adds
`connect_url` (`<url>/#connect`, the Connect tab's QR). A paired peer reads and writes this device's files
as `flink://<key_id>/<path>` (`connect_fs.rs`; FLINK RPCs `stat`, `fs_list`, `read_open`/`read_close`,
`mkdir`, `create`, `sync`, `close`, `rename`, `remove`, `free`, `reattach`, `delete_path`/`move_path` (a peer's Delete and Move to:
both paths confined, the share root itself refused, then this server's own `delete_path`/`move_path` rules with its keep list;
the kind back in the error's lead (`refused: `, `busy: `, not found), so the caller's route answers as for its own path), `install {path}` (this PS5 installs the caller's
package, `flink://<caller>/<path>`, plain names only; refused off the PS5), the bytes on data lanes in 8 MiB BLAKE3-checked
chunks, a `Ping` on the lane every 2 s while its disk work runs, so a stall past 12 s doesn't end it; a small file (≤ 8 MiB) is one lane round trip: `read {path}` answers its bytes with the stat after
the read, `put {tmp, path}` creates, writes, syncs and renames it): confined to the share root, links never followed, writes only to files that session created
(exclusive, then a no-replace rename, or with `rename`'s `replace: true` one over a regular file only, never a folder or link (by user decision: a
transfer or extract the user confirmed replacing; an older peer ignores the field and refuses the taken name); unrenamed ones kept 60 s
when it ends, for `reattach {path}` from the pairing that created them (the writer resumes on a fresh session, at most 3 times), then
removed, at once on shutdown or Forget), PS5 syncs as a local output
(a retried sync fails; ponytail: no destination probe per peer write); a path at or inside `/mnt/shadowmnt`
or one an unfinished job here uses is refused ("in use by a job on <name>"), and a
job here refuses a path a peer is writing. Transfer jobs hold back `quit {"if_idle": true}` and a takeover
like any job, and so does a paired device reading or writing files here (a read open, or a file it created and
hasn't renamed, kept ones included; by user request); a plain `quit` or shutdown cuts them. While it does, auto
rest mode is held off by the same U8 guard as jobs and browser uploads/downloads. peers.json is saved outside the `peers` lock every lane chunk takes,
so a slow sync doesn't stall a transfer.

Saved NAS devices are game sources and outputs over SMB, FTP or NFS (core `remote.rs` through `smb.rs`,
`ftp.rs`, `nfs.rs`; crates `ps5-dump-forge-smb`, `-ftp`, `-nfs`): `nas.json`'s SMB logins go to
`core::smb::set_credentials`, its FTP ones to `core::ftp::set_logins`, at start and on every
`nas_save`/`nas_forget`; NFS has no login (AUTH_SYS uid 0; NFSv4.1, else NFSv3). Paths:
`smb://<host>[:<port>]/<share>/<path>`, `ftp://<host>[:<port>]/<path>`, `nfs://<host>[:<port>]/<server path>`
(the port spelled only when not 445, 21 or 2049). They work in `inspect`, `library_cover`, `list_dir`
(`smb://<host>` lists the server's disk shares, `nfs://<host>` what is above its exports; `parent` is the
server from a share root, `null` at the server) and `start_job`: Convert, Extract and transfers read from them
and write into them (an image as a `.part` read back to verify and renamed no-replace, a folder file by file, an
fpkg streamed with nothing stored on this device's disk; FTP needs the `REST` rewrite probe for an fpkg or LZ4
or zstd packs in a folder). Folders and `.exfat`/`.ffpkg`/`.ffpfs`/`.ffpfsc` there open in place; a `.pkg` or an
archive is refused ("copy it to this device first"). Edits, `host_tree`,
`delete_path` and `move_path` refuse a NAS path. `list_dir {path: null}` roots stay local; the page's picker adds each
saved NAS's root (`smb://`, `ftp://` or `nfs://<host>[:<port>]`), for outputs too, or, while a paired
device or saved NAS is online, gives each online one a tab beside this device's.

`instance` is random per start, so the page notices a restart and rebuilds its job list. `list_dir` (core's `list_dir`) with
`null` lists the roots; directories first, then by name; non-UTF-8 names left out; symlinks resolved
(`link: true`, `path` the target; a dangling one as itself).
Events keep core's serde shape (`{"Ok": …}` / `{"Err": "cancelled"}`); ids stay below 2^53. A job's table
entry is made by its first event (core can emit `Done` before `Jobs::start` returns). Quit closes
admission first (`start_job` → 409); on a computer SIGINT/SIGTERM does the same. A port already taken by
Forge is taken over when that copy is idle, else "Can't close Dump Forge - some jobs are still running."
and exit 0 (see [Updates](#updates)); by anything else, exits 1.

- The URL (session `url`, the Connect tab, the host's start notice, the PS5 serve log) uses the address of
  a UDP `connect` toward the internet (no packet sent). On the PS5 the start notification only says
  "Dump Forge is launched", in the console's language (`notice.rs`).
- When a conversion, Pack job or transfer of this console finishes or fails (not an edit, an extract or a
  cancel), a system toast names it, in the console's language: `sceNotificationSend` with the JSON
  ShadowMountPlus sends (`InteractiveToastTemplateB`, no action), Forge's icon (the tile's, written to
  `/user/data/dumpforge/notify-icon.png`), the output's name (a transfer's: what it sent, "game-1k +2") as
  the upper line and "Conversion finished" as the main one. Refused, or with no
  icon, it falls back to the plain notice ("Dump Forge: conversion finished: <name>"); every other notice
  (started, stopped, ...) stays plain.
  On by default; `POST /api/notifications {"on": bool}` turns it off or on (session `notifications`), kept
  as `/data/dumpforge/notifications.off` (present = off). The job table sends it once, from whichever of
  `Done` and `start_job`'s admission comes last.
- The web bundle: `npm run build:http` (Vite mode `http`) writes `app/dist-http` with `transport-http.tsx`
  in place of Tauri (`Picker.tsx` for the dialogs, `http-client.ts`'s poller for the events); `build.rs`
  embeds it, or a placeholder page with a warning (`FORGE_REQUIRE_WEB=1`, set by `ps5/build.sh`, makes that
  an error). Paths are the server's (`/`), never the viewer's. Plain-node checks: `app/scripts/`
  (`check-paths`, `check-poller`, `check-launcher`, `check-update`, `check-upload`, `check-explorer`,
  `check-shoulders`, `check-peers`, `smoke`).

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
  waits up to 30 s. A newer payload's manifest swaps the page in and reloads once, never mid-launch, while a dialog is open
  or a field has focus.

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
- `self_update` (`ps5_dump_forge_updater::install`, shared with the desktop app's Install on PS5): admission
  checked as `quit {"if_idle": true}` checks it (within 1 s) and released before the download (the new copy's `if_idle` needs it), one at a time
  (an `AtomicBool`). It never stops this server itself. elfldr can't follow GitHub's 302 to its asset host
  (`Error reading HTTP payload`), so it is never handed a URL (no `?uri=`): the server
  downloads `https://github.com/quer3q/ps5-dump-forge/releases/download/v<v>/ps5-dump-forge-<v>-ps5.elf` itself
  (`ureq` + rustls, webpki roots, redirects followed, 50 s; `200` and a Content-Length of at most 64 MiB
  required, the body exactly that long and starting with the ELF magic). It holds it in memory when that much
  can be reserved; else it first replaces the self copy `/data/dumpforge/ps5-dump-forge.elf` (a temporary
  file beside it, synced, renamed over it, the folder synced, under the self copy's flock; a bad or short
  download leaves the copy as it was) and streams that file. Then it connects to `Options.elfldr`
  (`127.0.0.1:9021`) and streams the raw ELF (1 MiB writes, 120 s in all); a Forge ELF started with no
  arguments runs `serve` (unless a developer `args` file is pending: that runs instead). elfldr is probed
  first (connect and close, 3 s), so nothing is downloaded while it is down. Nothing answering → 503; any other
  failure (download, save, send) → 502 with its text ("couldn't download v<v>: ...").
- Handover: a newly loaded copy of any version (a self-update or Install on PS5, `nc`, an autoloader; an older or the same
  version too) that finds the port held by Forge closes it when idle and takes its place. It reads the
  running copy's session; one with `handover` gets `quit {"if_idle": true}`; one without (v0.1.1 and
  older: they ignore the body and cancel their jobs on quit) first gets `GET /api/jobs` and a plain `quit {}`
  only when every listed job has `done` (`ponytail:` in `lib.rs`: a job admitted between the two requests
  isn't listed yet and is cancelled). 3 s client, shared with the
  session probe. A start that couldn't serve (no usable `--root` on a computer) fails before asking. On
  200 it retries `bind` every 200 ms up to 30 s, then (PS5) waits for its self copy and
  saves it again, and serves (the self copy still keeps the newest version: an older ELF runs, but the
  tile starts the newer saved one next time). Busy (a 409, or an unfinished job listed) notifies "Can't
  close Dump Forge - some jobs are still running." and exits 0; no answer or a port still taken after 30 s
  notifies "Dump Forge v<new> could not take over from v<old>" and fails. Both notices are in the
  console's language (`notice.rs`).
- Known limits (`ponytail:` in the updater's `lib.rs`): TLS authenticates GitHub, but nothing checks a
  signature or the ELF's version marker. elfldr gives no answer, so an ELF that fails to start isn't seen
  (the page's 1-minute wait for a new `instance` ends with its stop-and-reopen note).

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
  `sceSystemServicePowerTick` at once, then every 30 s while a job runs, a paired device reads or writes
  files here or a browser uploads or downloads (`connect/upload`, `edit_upload`, `connect/download`)
  (core `awake()`: one process-wide count, one ticker
  thread), holds off auto rest (rest entered by hand: no
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
