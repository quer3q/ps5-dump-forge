# AGENTS.md

ps5-dump-forge: converts a PS5 game between a folder and every image format ShadowMountPlus 1.7
mounts (`.exfat`, `.ffpkg` UFS2, `.ffpfs` PFS, `.ffpfsc` compressed PFS container; read and write), plus
debug FPKG `.pkg` (create and extract; on the PS5 also install), and edits a game's files in a folder or image (the Edit tab:
in place or rebuilt) or copies some out (Extract). Rust core, Tauri 2 GUI (`PS5 Dump Forge.app`/`.exe`/`.AppDir`), a
CLI (`ps5-dump-forge`), and a PS5 ELF payload (the CLI's `serve`: the same UI as a web page on port 8095).
Releases: macOS (one universal zip), Windows x86-64 and arm64, Linux x86-64 and arm64, the PS5 `.elf`.
`ps5/README.md` holds the payload's details (harness, HTTP routes, tile/self copy/launch on open, U1–U10
write safety); `docs/LZ4.md` the Pack user guide (zstd and LZ4 packs); `docs/PAK.md` the zstd pack format.

## Layout
- `crates/ps5-dump-forge-core`: everything a job does. Own folder scanner (no symlink follow,
  exact names), preflight (names, special files, space, FAT32, depth, name limits), jobs (a worker thread
  per job, one heavy job and one transfer at a time, each slot FIFO, conflicts refused at start, cancel
  flag (a cancelled job's error is `cancelled`, or `cancelled; ` + what else failed or stays when that is
  more than the cancel), `catch_unwind`), `.part` + atomic no-replace
  rename (`finalize.rs`; macOS exFAT, Linux NFS and FUSE have none: check, then rename; FreeBSD: hard link where the folder has
  them, U3), `awake.rs` (`awake()`: one refcounted U8 guard for jobs, paired peers and browser
  uploads/downloads), BLAKE3 verification (`verify.rs`: 8 MiB slice hashes on a pool of up to 8 threads (hashed inline if none starts),
  at most 64 files and 64 MiB of partial slices held at once, past that a file hashed by a second read; fast
  sample or full), `inspect.rs` (cover, firmware, backport firmware from executables, embedded DLC; default and
  generated output names), `prefetch.rs` (read-ahead thread), `delete.rs` (Edit's and Storage's delete,
  Storage's move; `in_use`, the job-path guard; a `flink://` path handed to its peer, `delete_remote`/`move_remote`), `listing.rs` (`list_dir`; `list_explorer`, the PS5 explorer's ShadowMount+ rule), `library_roots.rs` (PS5 filesystem classifier, the chosen-folder refusal
  list) and `library.rs` (`scan_library`, `library_cover`; `scan_packages`, the Package installer's `.pkg` walk).
  `install.rs` (an install job: a `.pkg` served on a 127.0.0.1 port of its own and handed to the PS5's
  installer, `ps5/installer.c`). Edits: `edit.rs` (`Edited`: the `Edit`s folded
  over a source tree, every refusal listed, host files stamped and re-checked on each read; records the
  format editor's `patch::Op`s), `edit_job.rs` (an image edit job in place: scan, preflight, apply through
  the undo journal, verify, commit or roll back; the rebuild dispatch and `edit_check`'s two halves),
  `edit_folder.rs` (a folder edit job, applied directly), `temp_file.rs` (a file written through an
  exclusive `.<name>.forge-<pid>-<n>.tmp`, synced, renamed over or onto its target; `write_temp_new` syncs as
  `extract::sync_file` (on macOS a plain fsync, the caller's folder F_FULLFSYNC the barrier), `write_temp`
  (replace) fully), `host_tree.rs` (`host_tree`: a host file or
  folder as `Edited` will scan it, for the UI's preview), `extract_paths.rs` (an Extract job: chosen
  files and folders of any source copied out; `existing_targets`, `replace_findings`), `remote.rs` (every path on another device: the `Remote`
  trait (stat, list, read_at, read_checked, put_new, mkdir, create, write_at, sync, close,
  rename_no_replace, rename_replace (a file over a file; Replace), remove, free_space; `delete_path`/`move_path`, refused
  by default (`delete::not_here`), a paired peer's its own Delete and Move to), `Scheme` `Flink`/`Ftp`/`Smb`/`Nfs` (`flink://<key_id>`,
  `ftp://`, `smb://`, `nfs://<host[:port]>`; `prefix`, `default_port` 21/445/2049, `authority`,
  `host_port`), `get_for` (a paired peer: the `Remote` Connect registers (`set`); a server: its core
  adapter), `FlinkPath` (any scheme; `within`: scheme, device, ASCII case-folded on SMB), `is_remote`,
  `is_nas` (smb + nfs), `RemoteReader`/`RemoteTree` sources, outputs through
  `RemoteFile`/`RemotePart`/`RemotePkgOut`/`RemoteDir`/`RemotePacks`), the server adapters, each a thin
  `Remote` over its crate (session cache per server, uploads by handle, one reconnect on a connection
  error): `ftp.rs` (`ftp://`; `set_logins`, `can_rewrite` the `REST` probe an fpkg or LZ4 packs need;
  `Ftp::with`: a cached session that died (reset, EOF, vsftpd's `421` to an idle one) is dropped and the
  call runs once more on a fresh login, cached only if it didn't die too; a timeout drops the session
  without a retry; ponytail: a mutation whose reply was lost is sent again, and calls finding no session
  at once each log in),
  `smb.rs` (`smb://<host>/<share>/<path>`; `set_credentials`, guest without one; `smb://host` lists the
  disk shares), `nfs.rs` (`nfs://<host>/<server path>`, the server's absolute path; no login;
  `nfs://host` lists above the exports), `net.rs` (the one place for another device: dispatch to
  `remote`, refusals (`refuse`: NAS wording for `is_nas`), `exists`), `transfer.rs` (a transfer job:
  files and folders copied into a folder here, on a peer or on a NAS), `lib.rs`'s `out` (exclusive create, syncs, free space; for the peer side). FreeBSD/PS5 only: `dest.rs` (destination probe, U1), `durable.rs`
  (`sync_retry`, `SyncEvery`, drive-removal check; U4/U5/U7). `lib.rs` is the public API the CLI, the app
  and the server build against.
- `crates/ps5-dump-forge-exfat`: forward-only exFAT writer (512 B sectors, 64 KiB clusters), ported
  from MkPFS. `crates/ps5-dump-forge-ufs2`: write-once UFS2 writer for the one SMP geometry
  (`newfs -O 2 -b 65536 -f 65536 -m 0 -S 4096`). Both: `plan` (validates names, sizes the image)
  then `write` (strict offset order into a caller-owned file). Each also has `edit.rs`, its in-place
  editor: `Volume::open` (checks the volume, metadata within 128 MiB `MEMORY_LIMIT`), `edit::plan(image,
  ops) -> Result<Vec<Write>, Vec<String>>` (reads only) and `plan_sized` (+ `Space {needed, free}`, the
  numbers behind its "the changes need N MB" finding).
- `crates/ps5-dump-forge-pfs`: PFS writer for `.ffpfs` (MkPFS's unsigned PS5 layout, same `plan`/`write`
  shape), the streaming `.ffpfsc` container writer (`wrap`: the inner image's writer → `Stream` → zlib
  workers → `.part`), and the PFS/PFSC readers (`PfsSource`, `open_ffpfsc`). `edit.rs`: the `.ffpfs`
  in-place editor (same `plan` shape; what it can do: Format rules).
- `crates/ps5-dump-forge-patch`: std-only (+ `crc32fast`) in-place write engine the format editors plan
  for: `Op` (`Delete`/`Rename`/`Mkdir`/`Put`), `Data` (`Bytes`/`Zero`/`Source`), `Write {offset, data,
  live}`; `apply` (undo journal, then free-space writes, then live ones), `Applied::commit`/`rollback`,
  `recover`, `journal_path`; `Syncs` hooks (core passes FreeBSD's `sync_retry`). The format crates depend
  on it for types, so they never depend on core.
- `crates/ps5-dump-forge-fpkg`: streaming `.pkg` reader (`FpkgSource`), CNT entries merged with the
  inner filesystem; Kraken blocks decoded by the vendored `kraken_image` (PFSv3 byte shuffles undone,
  `SHUFFLES`; vendor patch 0029); `peek` (header and container only, a few KiB: patch, content ID and type,
  plaintext `param.json`, `icon0.png` size; the package scan's); `lih.rs` reads a patch package (`\x7FLIH`) as the package it patches (Patch packages
  below).
- `crates/ps5-dump-forge-lz4`: ampr_emu asset packs: AMPRPAK4 manifest + AMPRDAT3 volume writer and
  reader (unpack view as a `SourceTree`), the AMPRIDX3 reader, pack rules (`rules.rs`), LZ4 blocks through
  the C liblz4 (`block.rs`, `lz4-sys`: `LZ4_compress_default` at level 0, `LZ4_compress_HC` at 3..=12,
  `LZ4_decompress_safe`), the embedded runtimes (`runtime.rs`: the release runtime `runtime::release()` and ampr-emu-ng
  v0.1.0 `runtime::zstd()`, functions: `build.rs` stores each `.sprx` as one zstd frame (level 22), decoded
  once on first use into a `OnceLock`; sizes and SHA-256 pinned (`pinned_sizes_and_hashes`); `classify` also recognizes by pinned
  SHA-256 the old trace build and the old local ng build (`OLD_ZSTD_SHA256`), neither shipped). Glue in `core/src/lz4.rs` (open/unpack of LZ4 and zstd packs, index,
  runtime swap). `tree.rs` is `PackedTree`, the packs served as a `SourceTree` from a `writer::measure` pass.
  `vendor/ampr_emu/`: the upstream 0.4.2.1 release `.sprx` + the source archive. `vendor/ampr-emu-ng/`: the
  ampr-emu-ng v0.1.0 release (`libSceAmpr.sprx`, embedded as `runtime::zstd()`; `libSceAmpr-debug.sprx`, never
  embedded; `SHA256SUMS`) + its source archive.
- `crates/ps5-dump-forge-zstd`: zstd packs (`ampr_zstd.index` + `ampr_zstd-NNN.pak` volumes of at most 4 GiB -
  64 KiB, 256 KiB chunks, one zstd frame or raw each): the format (encode + validating decode), the
  writer (`writer::pack_with`; `writer::measure` + `tree::PackedTree`, the packs made on demand for an
  image, as LZ4's) and the reader (`reader::detect`, `reader::unpack`: the index and every
  volume header checked, then the logical files served as a `SourceTree`, 8 chunks per volume read decoded
  in parallel; a loose file found by a binary search of the sorted listing, `Unpacked::listed`); spec in `docs/PAK.md`. Without its default `std` feature the crate is `no_std` and keeps
  only the convention (magics, layout, names, `Chunk`, the volume header): ampr-emu-ng builds against
  that. Glue in `core/src/zstd_pack.rs` (the pack job's tree, plan, packing,
  checks).
- `crates/ps5-dump-forge-archive`: `.zip`/`.7z`/`.rar` sources (`detect`, `list`, `read_files`, `Reader`,
  `ArchiveError`, the two password strings). `zip.rs` own reader (ZIP64, STORED + Deflate via `flate2`), `sevenz.rs`
  over `sevenz-rust2` (decode only), `rar.rs` our FFI wrapper over `unrar_sys` (bundles UnRAR's C++; every target,
  the PS5 payload included), `parts.rs` multi-part naming, `stream.rs` the `Reader`. Glue: `core/src/archive.rs` (`ArchiveTree`), `inspect.rs`,
  `library::archive_game_root`. Licence: `vendor/LICENSE-UnRAR.txt`, `vendor/LICENSE-UnRAR-exception.md` (copied into every release).
- `crates/ps5-dump-forge-smb`: own blocking SMB2 client (2.0.2/2.1; the `smb` crate was rejected: ~180
  crates and its guest login fails on Samba 4.23): `ntlm.rs` (raw NTLMSSP NTLMv2, in-house MD4; an empty user
  is a guest login), `wire.rs` (framing, credits, 1 MiB reads, HMAC-SHA256 signing when required),
  `rpc.rs` (srvsvc NetShareEnumAll: disk shares), `lib.rs` (`Session::connect`, `shares`, `list`, `stat`,
  `open` → `File::read_at`; writes (by user decision: a share is an output too): `create_file` (exclusive),
  `mkdir`, `rename` (never replaces), `rename_replace` (ReplaceIfExists 1: a file over a file; Replace),
  `remove` (files), `free_space`, `File::write_at`/`flush`).
  Glue: `core/src/smb.rs`. ponytail: no SMB 3.x, encryption, DFS, reconnect or
  pipelining, response signatures not verified, no NTLM MIC.
- `crates/ps5-dump-forge-ftp`: own blocking plain-FTP client (RFC 959 + FEAT, MLSD/SIZE/MDTM/REST, EPSV;
  passive, binary; user + password or anonymous): `Session` (list, stat, reads, mkdir, rename (check, then
  `RNTO`), `rename_replace` (`RNTO` with no check; the server's filesystem refuses a folder), remove, `can_rewrite` (a `REST` + `STOR` probe)), `Writer` (any offset, `REST` to go back);
  `fake.rs` (feature `fake`) a test server (`kick` closes every control connection, as an idle timeout
  does). A `421` reply is `ConnectionAborted` (the server is closing); a `550` to `MLST` is `NotFound` unless it
  says it was refused. Glue: `core/src/ftp.rs`. ponytail:
  no FTPS, no active mode, no reconnect here (core retries); without MLST a file's change shows by its
  size only.
- `crates/ps5-dump-forge-nfs`: own blocking NFS client, std only (+ `libc` on unix for the privileged
  source port), NFSv4.1 and NFSv3 (by user decision both; `Session::connect(host, port)` tries v4.1, then
  v3; `version()`): `rpc.rs` (ONC RPC over TCP, record marking, XDR by hand, AUTH_SYS uid 0, a source
  port from 1023 down, else any), `v3.rs` (portmapper unless a port is given, MOUNT, READDIR(PLUS),
  WRITE UNSTABLE + COMMIT with the verifier checked, CREATE GUARDED), `v4.rs` (EXCHANGE_ID,
  CREATE_SESSION with one slot, SEQUENCE in every COMPOUND, OPEN/CLOSE, the server's granted sizes and
  ops honoured), `lib.rs` (`Session`: `exports`, `stat`, `list`, `read_at`, `mkdir`, `create` →
  `File::write_at`/`sync` (never replaces), `rename_no_replace`, `rename_replace` (RENAME over a file, never a folder;
  Replace), `remove` (files only), `free_space`;
  absolute server paths). Glue: `core/src/nfs.rs`. ponytail: one connection per session, requests
  serialized, no reconnect; cached handles live 3 s; an idle v4.1 session may expire (dropped, reconnected).
- `crates/ps5-dump-forge-cli`: binary `ps5-dump-forge` (`inspect`, `convert`, `serve`), JSON-lines events.
  A second Ctrl-C quits at once (unix `_exit(130)`; Windows the default handler), the first prints
  "cancelling... (press Ctrl-C again to quit now)"; the `.part` stays (`stale_parts`).
- `crates/ps5-dump-forge-ps5`: the payload's global allocator (`Pool`, installed by the CLI only for
  the PS5): freed blocks of 64 B..4 MiB kept in power-of-two classes, since the PS5 libc serializes
  every malloc across threads (a `.pkg`'s Kraken pool: 12.7 → 49.3 MB/s at 4 threads).
- `crates/ps5-dump-forge-server`: `serve`, std-only HTTP/1.1 (`http.rs`), routes mirroring the Tauri commands
  (`api.rs`), job table for polling (`table.rs`), roots (`paths.rs`), PS5 tile (`tile.rs`), self copy
  (`self_copy.rs`), `debug.rs` (PS5 only, with `FORGE_DEBUG_API`). `build.rs` embeds `app/dist-http` (an
  `assets/` file that gzip shrinks by 5% or more stored gzipped, `ASSETS: &[(&str, bool, &[u8])]`; `index.html`
  and `forge.appcache` plain; sent with `Content-Encoding: gzip` when `Accept-Encoding` takes it, else decoded
  per request, `Response::gzip_asset`; both with `Vary: Accept-Encoding`) and the self copy (`PDFGSELF`,
  lengths, SHA-256, one zstd frame at level 22; `zstd` a dependency on the PS5 only, `flate2` a build one).
  Past `MAX_HANDLERS` (16) the accept thread answers 503, shuts its write side and drains the request
  for up to 20 ms, so the close sends no reset. `POST /api/edit_upload` and `POST /api/connect/upload` are the routes whose body is streamed
  (no 1 MiB cap; `BodyReader` in `http.rs`). Connect: the `ps5-dump-forge-flink` crate (below),
  `connect.rs` (`Connect`: identity (`node.key`),
  approvals (`Approvals`, `Outgoing`), `peers.json`, the TCP 9883 listener, pair/forget/devices, liveness, `snapshot`,
  a peer's `install` RPC (`set_install`, the PS5's server only; `Connect::install` asks a PS5);
  also used by the Tauri app), `connect_fs.rs` (a peer's files over FLINK: the peer side's RPCs and lanes,
  `ConnectRemote`, core's `Remote` on this side; the busy hooks both ways; `set_delete_move`, a peer's Delete and
  Move to here; the speed test, `speed_test`, `serve_speed`: the `speed` RPC, then `speed_up`/`speed_down` lanes,
  nothing on disk), `discovery.rs` (Forge on UDP 8094; a socket error other
  than WouldBlock logged once, `discovery: <e> (retrying every second)`, and retried every second, so the
  responder outlives a network down/up or rest mode; ponytail: the sockets are never bound again), `probe.rs` (Add a device: what answers at an address), `payload.rs` (elfldr's address as Install on PS5 and Send a payload take it, port 9021 by default), `mdns.rs` (NAS: smb/ftp/nfs; `merge`), `wsd.rs` (Windows PCs by WS-Discovery),
  `nas.rs` (saved NAS devices: `nas.json`, one entry per host + port, `protocol` smb/ftp/nfs, credentials;
  `publish` hands SMB logins to `core::smb::set_credentials`, FTP ones to `core::ftp::set_logins`).
- `crates/ps5-dump-forge-updater`: the PS5's ELF loader (elfldr, port 9021) from here (one `lib.rs`): `download`
  (a release's `ps5-dump-forge-<v>-ps5.elf` from GitHub over HTTPS, redirects followed, 50 s; `200`, a
  Content-Length of at most 64 MiB and the ELF magic required, a cut or longer body refused; held in memory, or
  with a `spill` file (the PS5's self copy) saved over it as the self copy saves itself: temp + sync + rename
  under its flock), `install` (elfldr probed, connect and close; then download, then stream the bytes to elfldr),
  `send` / `send_bytes` (an ELF file or bytes streamed, at most 64 MiB, 120 s). One dependency, `ureq` with
  rustls (ring, webpki roots; + `libc` on unix for `O_NOFOLLOW`), by user decision: elfldr can't follow GitHub's
  302, so it is never handed a URL. ponytail: TLS authenticates GitHub; no signature or version check.
- `crates/ps5-dump-forge-flink`: FLINK v2, Connect's protocol (one `lib.rs`: 16 B frames with CRC-32,
  BLAKE3 MACs, the Hello/Pair handshake, RPC, data lanes, `tune_buffers`; the listening side's handshake
  and a lane's Join bounded in total by `HANDSHAKE_WAIT` (15 s, `Conn::set_deadline`), `PAIR_WAIT` more once
  a `Pair` is in, so a peer dripping bytes can't hold one of the 64 conn slots; `Conn::send_data` MACs a
  `Data` frame's head and bytes as parts, never building one body, and `read_frame` fills a body as it reads,
  never zeroed first); std + blake3 + crc32fast + serde_json + x25519-dalek + getrandom.
- `ps5/`: the payload's Docker harness: `Dockerfile`, `build.sh`, `linker.sh`, `entry.c` (launch modes,
  logs, `ps5_notify`), `compat.c` (statfs symvers; `cpuset_getaffinity` answered per thread, so std sees the 5 CPUs
  a payload may use, not 16), `launcher.c` (tile registration), `installer.c` (`forge_pkg_install`:
  `sceAppInstUtilInstallByPackage` under the SYSTEM install authid; `forge_appinst_begin`/`end`, the one lock
  every sceAppInstUtil user holds, `launcher.c`'s tile registration included; the library an install
  initialized is never terminated), `icon0.png`,
  `test-entry.sh`; `shim/` + `x86_64-ps5-freebsd.json` + `prepare-rust-std.py` are ps5-ai-cli's, byte-identical.
- `app/`: React/TS/Vite UI; `app/src-tauri` is the `ps5-dump-forge-gui` crate (binary `Forge`,
  bundled as "PS5 Dump Forge" via `mainBinaryName`; Linux's `tauri.linux.conf.json` names it
  `ps5-dump-forge-gui` so the AppImage step can't overwrite the CLI's `ps5-dump-forge`).
  `npm run build:http` (Vite mode `http`) builds the web UI into `app/dist-http`: `transport-http.tsx`
  replaces `transport-tauri.tsx`, `main-http.tsx` runs `launcher.ts`
  (start the payload via elfldr) and `appcache.ts` first.
  Edit tab: `Edit.tsx` (card, explorer, row actions, drops), `EditChanges.tsx` (pending bar, conflict
  and Discard questions, what an add brings: `fromHost`, `dropped`), `EditApply.tsx` (mode, `edit_check`,
  confirm, uploads), `explorer.ts` (pure: the tree, the pending changes' fold, a port of core's `Edited`
  rules, `planAdds` (clashes), `pendingReducer` (batches), `toEdits`, `checkable`, `stagingNames`).
  Pack tab: `Pack.tsx` (the zstd / LZ4 selector, Pack or Unpack), `PackHelp.tsx` (How pack works).
  Storage tab: `Storage.tsx` (view switch), `Library.tsx`, `Files.tsx` (the File explorer), `FilesTabs.tsx`
  (its device tabs and row drags), `Packages.tsx` (the PS5's Package installer); `InstallDialog.tsx` (the one
  "Install this package?" question and `installRequest`, shared with Connect's Install pkg).
  Connect tab: `Connect.tsx` (a device hub: left This device with the QR and Add a device; right the
  Devices list (peers and NAS) with its filter and Nearby (Autodiscovery's finds); device and NAS
  details, the pairing wait, Add SMB / FTP / NFS), `ConnectHelp.tsx` (How to connect), `Transfer.tsx` (File
  Transfer: Send as a transfer job, the browser's upload and download), `useConnectState.ts` (polls
  `connect_state`; selectors `useOnlineCount`, `useRemoteJobs` (kept until its JSON text changes) and
  `useSavedNames` (kept until a name or address changes); `common.tsx`'s `usePeerPath`/`useDeviceName` read
  names only, so a poll that changes nothing re-renders nothing). `Picker.tsx`: the tabbed picker (a tab per online device);
  `JobList.tsx` the job list (`JobsCard`; `App.tsx` renders one per tab, that tab's jobs).
  `app/scripts/*.mjs`: plain-node checks (`check-paths`, `check-poller`, `check-launcher`, `check-update`,
  `check-upload` (the upload URL's encoding against the server's decoder, `upload` over a mocked XHR),
  `check-explorer` (`explorer.ts`: the fold mirrors core's `Edited` rules; the folder-merge case is checked
  only there), `check-shoulders` (`shoulders.ts`: L2/R2 keys, tab order), `check-peers` (`peers.ts`: saved
  devices' names in paths and backend text), `smoke`); run by hand (`node scripts/<name>.mjs`; no npm
  script, not in CI).
- `vendor/ps5upload-{fpkg,pkg}`: ps5upload v7.0.22 crates (readers, FPKG builder, verify) plus a
  local patch series in `vendor/patches/`; see `vendor/README.md`. Edition 2021 on purpose.
  `vendor/lzma-rust2`: crates.io 0.22.0 + patch 0021 (bounded multithreaded LZMA2 decode, which
  `sevenz.rs` sizes per block by dictionary and largest run), via `[patch.crates-io]`, excluded
  from the workspace.
- `scripts/`: checks and releases (see Commands); `check-versions.sh` also matches the tag on a tag build.
  `fuzz/`: cargo-fuzz targets, its own workspace.
- `tests/`: the Connect smoke suite against a real console: `README.md` there, every script in
  `tests/scripts/` (`smoke.mjs`, 26 cases over both Forges' HTTP APIs; by user decision at most 30, only
  critical ones, 10–15 minutes on a console; `gen-game.mjs` a fake game ≤ 1 GiB; `dry-run.sh` two local
  Forges instead). Its servers run in Docker only (by user decision: setup refuses an address that isn't
  this computer + the container's published port, so a real NAS is never written): `nas.sh` a Samba guest
  share (1445, writable `out`), `ftp.sh` vsftpd (2121, `forge`/`forge`, idle sessions closed after 20 s for
  the stale-session case), `nfs.sh` unfs3 NFSv3 (12049) and NFS-Ganesha NFSv4.1 (12050, its
  `/share` a Docker volume). The SMB and NFS crates' and core's live tests use the same containers
  (`FORGE_SMB_TEST`, `FORGE_NFS_TEST`; FTP's run against the crate's `fake` server); see `tests/README.md`.
- `DESIGN.md`: the UI design system (tokens, components, colour/contrast rules, the cover glow);
  `app/src/styles.css` holds the values. Read it before changing the UI.
  `app/src/locales/`: the UI text, one file per language (see Key decisions).

## Key decisions
- Every reader/writer meets at `ps5upload_fpkg::source::SourceTree` (sizes up front, `read_range`,
  `empty_dirs`, `Send`). Any source converts to any target with no staging, archives included.
- Archive sources (`.zip`/`.7z`/`.rar`, one game root ≤ 4 levels deep, found by the Library's recognition from the
  listing) are read in place, never unpacked to disk: `core/src/archive.rs` serves the game folder as an
  `ArchiveTree` over the archive crate's `Reader`, which decodes front to back on its own thread (a session; bounded
  queue) and serves the entry asked for, skipping those before it; an entry the session went past (or a ZIP/7z skip
  that never recorded it) starts a new session from the archive's start: cheap when non-solid (header skips),
  a re-decode of everything before it when solid (by user decision: no temporary files, even for solid
  archives). Verification hashes as written, so nothing re-reads the archive. The source stamp covers every
  part. Every part of a multi-part source is guarded from delete while its job is unfinished; edits refuse an archive (Edit browses and extracts from it only); the Library lists none. `inspect` reads only the entries it needs (no
  extraction). Passwords for 7z and rar only (encrypted zip refused), `ArchivePassword` redacted in `Debug`, never
  serialized or logged; with none given, `archive::password` tries `KNOWN_PASSWORDS` (by user decision: six
  release-site passwords, in order; one fits when the game folder's `param.json` reads, and when that
  file is in the clear, the game's smallest encrypted file too; `Entry::encrypted`) for inspection, naming and
  jobs alike, and only when none fits is the error "needs a password" (the UI's prompt); CLI `--password-stdin` / `FORGE_ARCHIVE_PASSWORD`. Multi-part: `.partN.rar`, `.rar`+`.rNN`,
  `.7z.NNN`/`.zip.NNN`, split `.z01`…`.zip`; first part only, missing parts named. 7z Deflate is refused: its
  `deflate` feature would switch the workspace's `flate2` to zlib-rs, replacing `.ffpfsc`'s miniz_oxide.
  Memory (by user decision): a job stays within 512 MiB except for the archive's own dictionary,
  which the decoder allocates whole: RAR is accepted up to 4 GiB and 7z up to 1 GiB, so an archive
  packed with a dictionary over ~256 MiB goes past the cap. 7z LZMA2 decodes on several threads
  only in a job's session (`Sink::streams`; small reads stay on one) and for a block that is a lone
  LZMA2 coder, sized per block (dictionary + 6 × its largest run within 256 MiB; `sevenz::decode_threads`); RAR5 decodes on UnRAR's own threads (at most 8).
- LZ4 packs are an asset-pack transform of a game folder (ampr_emu), not an SMP image format: `--to lz4`
  writes a folder (no extension) with `ampr_assets.index`, `.crc`, `ampr_assets-NNN.pak` volumes, loose
  leftovers and the runtime; converting that folder to an image is an ordinary second convert (packs are
  carried). Pack can also target an image directly (`Lz4Mode::Pack`, `lz4: "pack"`, CLI `--lz4-pack`;
  `format` folder/exfat/ffpkg/ffpfs/ffpfsc, `.pkg` refused "not supported yet"; `Format::Lz4` is the alias of
  pack into a folder): **two passes, no staging folder**. `writer::measure` runs `pack`'s compression
  without writing and keeps only the metadata (`Measured`: manifest, CRC sidecar, volume sizes, build ID,
  level; 16 B per chunk; the job peaks near 36 B per chunk while verify parses the manifest).
  `tree::PackedTree` lists the loose files plus the pack files at those sizes; reading a volume re-reads
  each chunk's source block, recompresses it with the one `writer::encode`, and fails unless stored length,
  codec, CRC and the LZ4 chunk's decoded CRC match the measurement; one cached chunk, `threads * 4` blocks
  in flight per volume. `pack` and `measure` share one `Sink` (placement, thresholds, rollover) and one
  `metadata`; folder output is pinned byte for byte by a hash test. The image job: measure (stage
  `measure`), plan → write from `HashingTree(PackedTree(HashingTree(logical)))`, verify as written, then
  `reader::unpack` of the image (every runtime check) and the logical files against the source, same
  fast/full policy. Pack needs a title whose `eboot.bin` imports `libSceAmpr`. Compression is the C liblz4
  (`block.rs`, by user decision): `lz4_level` 0 (Fast, `LZ4_compress_default`, the default) or an HC level
  3..=12 (Medium 9, Max 12), any other value a finding; CLI `--lz4-level`. The runtime is `lz4_runtime`
  (`Lz4Runtime`, `ng` default | `ampr_emu`; CLI `--lz4-runtime`): ampr-emu-ng (`runtime::zstd()`) or the
  release build; `runtime::WARNING` is logged only for `ampr_emu`. By user decision: no traces and no
  profiles; the rules are the built-in guess (below).
- zstd packs (`core/src/zstd_pack.rs`, `ps5-dump-forge-zstd`; spec `docs/PAK.md`; named zstd, files on disk AMPR
  style, by user decision: `ampr_zstd.index` (`AMPRZPK1`) + `ampr_zstd-NNN.pak` (`AMPRZDT1`, from 000)): volumes
  (zstd, 256 KiB chunks, a chunk kept raw when zstd doesn't shrink it) that ampr-emu-ng (`runtime::zstd()`,
  `RuntimeKind::Zstd`, id `zstd`, `vendor/ampr-emu-ng/libSceAmpr.sprx`, the v0.1.0 release, size and SHA-256
  pinned (`ZSTD_SHA256`); the user's runtime, which reads LZ4 packs too; the old local build (`OLD_ZSTD_SHA256`,
  file removed) is still recognized by its hash, so a game carrying it keeps it, and any other build holding the
  `AMPRZPK1`, `AMPRPAK4` and `AMPRDAT3` magics counts as ampr-emu-ng too) reads.
  Pack (`zstd_pack`, `zstd_level` 1..=22, default 5; CLI `--zstd-pack [--zstd-level N]`): into a `folder` or
  straight into an `.exfat`/`.ffpkg`/`.ffpfs`/`.ffpfsc` (`.pkg` refused), no LZ4 option, an AMPR title (`eboot.bin` imports libSceAmpr), no source file named like a pack file;
  else preflight findings. `zstd_pack::prepare`: ampr-emu-ng at `fakelib/libSceAmpr.sprx` (installed or
  replacing any), journal, logs and old index left out, a fresh `ampr_emu.index` (the job's `mtime`) listing
  every file written, loose and packed. The packed set is LZ4's: the built-in guess (`rules::select`:
  root files and the keep-loose list stay loose) or, for an LZ4-packed source, the files its packs held; never
  `zstd_pack::loose` (`always_loose`, the pack files, the index); no auto-loose. A folder: stages scan, preflight,
  write, pack, verify, finalize. An image: two passes as LZ4's (`writer::measure`, then the image written from
  `tree::PackedTree`; stages scan, preflight, measure, write, verify, finalize). Verify checks the index and
  every volume header, then the logical files read back through the packs.
  Unpack (`zstd_unpack`, CLI `--zstd-unpack`): any target but `lz4`, no LZ4 option, not with `zstd_pack`;
  the packs decoded (`reader::unpack`), the release runtime replacing ampr-emu-ng and a fresh index; a source
  without `ampr_zstd.index` is a finding. `lz4::open` decodes LZ4 packs for any LZ4 option or zstd pack, zstd
  packs for zstd pack or unpack and for LZ4 packing.
  A zstd-packed source (root `ampr_zstd.index` starting `AMPRZPK1`; `Inspection.zstd_packed`,
  `LibraryEntry.zstd_packed`) is otherwise an ordinary source: a plain conversion (any format, `.pkg`
  included) carries the packs as plain files, keeps its `ampr_emu.index` byte for byte (it lists the packed
  files) and keeps ampr-emu-ng (`zstd_action`: installed where missing, any other runtime replaced); into a `.pkg` the builder
  packages that index as it is (vendor patch 0030: `source::lz4_packed` answers true for it). Remove backport is
  refused in the UI for a packed source (`convert.zstdBackport`/`lz4Backport`: unpack first). Its label is
  shown wherever the LZ4 one is: the source card's **zstd** tile (`ZstdTile`, in the LZ4 tile's place:
  Packed, "a conversion keeps them", the runtime), the Library's **zstd** chip and card/row badge.
  UI (by user decision): the **Pack** tab (`app/src/Pack.tsx`, after Edit; **How pack works** in `PackHelp.tsx`)
  with a kind selector **zstd** (default) / **LZ4**; the action is **Unpack** for a game packed
  by the kind chosen, else **Pack**. Its empty source card (`PackEmpty`, by user request) is the shared one's text ("Choose a game folder,
  image or archive", format chips, **Pick**) in Pack's yellow (`.empty.lz4`, compress icon, `button.lz4-btn`); before a source the settings side shows
  nothing. LZ4's line under the selector: an unpacked game a warn "Some games may not start. ...", a packed one
  plain "Writes the LZ4 packs back as plain files."; How pack works opens its LZ4 list with "**Some games may
  not start.** Keep a copy.". Blocked (a note in place of the settings): a patch package, additional
  content, a game packed by the other kind ("unpack it under <kind> first"), a game that doesn't use AMPR
  (`pack.noAmprGame`). Pack's targets (a UI limit; core and CLI keep `.ffpfsc`): zstd folder (default)/
  `.exfat`/`.ffpkg`/`.ffpfs`, LZ4 packed folder (default)/`.exfat`/`.ffpkg`/`.ffpfs`, an image with the muted
  `pack.direct` (reads the game twice). Unpack: zstd folder (default)/`.exfat`/`.ffpkg`/`.ffpfs`/`.pkg`, LZ4
  folder (default)/`.exfat`/`.ffpkg`/`.ffpfs`. Pack's **Compression** presets **Fast** / **Medium** / **Max**
  (`LevelField`, `LEVEL_OF`: zstd 5/9/19, LZ4 0/9/12; Fast default), each with a `levelNotes` line (Fast
  tagged Recommended). LZ4 Pack's **Runtime** (`RuntimeField`): **ampr-emu-ng** (`ng`, default, Recommended:
  reads zstd and LZ4 packs) or **ampr_emu <version>** (upstream). A Pack that installs ampr-emu-ng (every zstd
  Pack, LZ4 with `ng`) shows the warning `pack.ngNote`: turn off ShadowMount+'s fakelib update, or it replaces
  the runtime. LZ4 Unpack, or Pack with ampr_emu: the runtime action from `Lz4Facts.pack_runtime_action`
  (install, replace or keep, naming an `other` build's version; a game's ampr-emu-ng replaced:
  `convert.amprNgReplaced`); LZ4 Pack: a non-ok index written afresh
  (`pack.freshIndex`). Job rows: `jobType` `pack`, a tag `LZ4 pack · <level>`, `zstd pack · <level>` (the
  preset's name, else `level N`), `LZ4 unpack` or `zstd unpack`.
  Convert always sends `lz4: null` (by user decision: no Update AMPR switch, nothing to choose); under Build, for an
  AMPR game only, muted `info` notes say what the conversion does by itself: the runtime action
  (`Lz4Facts.runtime_action`: install, replace, keep with the version; `convert.amprRuntime`, a game's
  ampr-emu-ng `convert.amprNgReplaced`) and the index (a
  non-ok one written afresh, `amprIndex`, an ok one counted stale while Remove backport is on; for an fpkg target
  always `amprPkgIndex`: the builder writes it; none for a packed game). The Edit tab's
  LZ4 tile (its Game facts fold) names install/replace (keep only for an `other` build with a version) and a non-ok index, and shows for
  an AMPR game a conversion installs the runtime into.
- Every conversion repairs an AMPR title (`eboot.bin` imports libSceAmpr; by user decision): any target, Convert,
  CLI, server and Pack jobs alike (`lz4::plain` for a request without LZ4 options, `prepare`'s full path otherwise).
  Edit jobs are the exception (their own index rule, runtime never swapped; see the Edit tab below).
  Runtime: one rule, `lz4::runtime_of(tree).action(ampr, packs)` → `RuntimeAction`, shared by the jobs and `inspect`:
  `Install` where there is none; `Replace` an ampr_emu older than the bundled one (`runtime::version_of` +
  `not_older`), one naming no version or unreadable, and Forge's old trace runtime (recognized by its pinned
  SHA-256, on any title); `Keep` Forge's release runtime and another ampr_emu at the bundled version or newer (by
  user decision), except into an output holding LZ4 packs (LZ4 Pack, an LZ4-packed source as it is), which also
  needs `runtime::reads_packs` (upstream's no-pack build is replaced); ampr-emu-ng goes into a game only by
  packing (by user decision: zstd Pack always, LZ4 Pack with `ng`), so every other job replaces it with the
  release build (`runtime::release()`, no option: a plain conversion, an LZ4-packed source as it is, LZ4 and
  zstd Unpack; log "replacing ampr-emu-ng at ... with the bundled ampr_emu <v>"), except a zstd-packed source
  converted as it is (`zstd_action`, below: only it reads zstd packs); `Untouched` for a non-AMPR
  title. LZ4 Pack follows `pack_action(lz4_runtime)`: `ng` keeps ampr-emu-ng, else installs it; `ampr_emu`
  replaces ampr-emu-ng, else `action(true, true)`. zstd Pack always writes ampr-emu-ng. `Lz4Facts.runtime` names it
  (`forge_release`/`forge_trace`/`zstd` (ampr-emu-ng)/`other`/`none`). `Lz4Facts.runtime_action` (plain convert) and `pack_runtime_action` (Pack/Unpack) name it
  (`install`/`replace`/`keep`, `None` for a non-AMPR title); the CLI's LZ4 line says it too.
  Index (any action but `Untouched`): an unpacked source keeps `ampr_emu.index` only when it lists exactly the
  output's (path, size) set (`lz4::index_state`, the one comparison, `inspect`'s too; a replaced or installed
  runtime counted at the release size, `plain_listing`), else a fresh one is written (stale, damaged or missing alike); with a rebuild the journal
  and logs are left out (the journal's file ids follow the old index's record order), with a kept index they stay
  (Pack jobs always leave them out). `index_state` reads no index bigger than `index_bound` (4 × a fresh index for
  the listing + 1 MiB: `Stale` unread) nor over 64 MiB (`MAX_INDEX_CHECK`: `Damaged` unread), so the check
  stays well within a job's 512 MiB.
  Into a `.pkg` no index is built here: the old one (an AMPR title's), the journal and logs are left out and the
  builder writes it after its SDK filtering (below), so leftovers it drops can't fail preflight. A packed source
  (LZ4 or zstd) gets the runtime action and keeps its index, manifest, journal and logs byte for byte; a replaced
  runtime then differs from the LZ4 manifest's loose record, which `reader::unpack` lets pass for `RUNTIME` only.
  `Lz4Facts.index` (`ok`/`stale`/`damaged`/`missing`; `None` when packed, when the action is `Untouched`, or for an
  archive's index over 16 MiB) is that comparison. Edit's game facts, Convert, the Pack tab and the CLI say a
  conversion refreshes it.
- LZ4 rules: the built-in guess only (profiles and `--lz4-profile` removed, by user decision).
  `--lz4-unpack` (automatic when the target is lz4) restores the logical assets; the runtime follows the runtime
  rule above (kept unless installed or replaced) and an `ampr_emu.index` listing exactly the output stays.
  Packed files exist for the game only after it starts AMPR, so anything the engine opens, stats or lists
  before that (Unreal `.pak`/`.utoc`, `.ini`, `.uproject`) must stay loose. The keep-loose list
  (`rules::keep_loose`: `FALLBACK_SUFFIXES` + the protected directory families) and every root file stay
  loose (`rules::select`). Then auto-loose (upstream `ampr_pack.py`'s defaults, `rules::auto_loose_applies`):
  a compress-spec file (not store, not hot) of ≥ 64 MiB is sampled during preflight (32 blocks / ≤ 16 MiB at fixed, evenly
  spread block indices, `writer::sample` = the writer's `encode`) and kept loose when it saves < 5% or
  ≥ 90% of the blocks stay RAW; one log line `auto-loose: N large files kept loose (incompressible samples): …`.
- Never touch the source: junk (`.DS_Store`, `._*`, `.fseventsd`, ...) is filtered, not deleted.
  Three exceptions, by user decision, each an explicit request:
  1. Deleting the Edit tab's source, a Library entry or a File explorer row (the trash button of the Edit
     card, of a Library card/row or of an explorer row, or its Delete key; one **No** / **Yes, delete**
     dialog and one route; core `Jobs::delete_path(path, protected)`
     → `DeleteError` `Refused`/`Missing`/`Busy`/`Failed`, `core/src/delete.rs`; server
     `POST /api/delete_path` `{path}` → `null`, 400/404/409/500, 409 also while stopping; Tauri
     `delete_path` `{path}`; the server's admission is held only to admit it, the removal runs counted
     (`deleting`): meanwhile `quit {"if_idle"}` and `self_update` answer 409 "a game is being deleted", a stop
     doesn't wait, `finish` waits for it before exiting; core's job table stays locked through the removal). A file (`remove_file`) or folder (`remove_dir_all`: links inside removed, not
     followed), then its folder synced; no Trash, no undo, and a folder that fails part way may be partly
     gone (the error says so). Refused: an empty path, a link itself, a special file, a volume root or
     mount point, a folder with another volume mounted inside (unix, by `st_dev`; a same-filesystem Linux
     bind mount is not caught), each `protected` path and folders holding it (server: browse roots,
     configured and current, and on the PS5 Forge's own files in `/data/dumpforge`: the self copy, its
     lock, the tile's owner record; app: the home folder), and anything at, in or above an unfinished job's
     source, output or `.part`s (an edit job's undo journal and every `Add.from`), compared by identity after canonicalize, links on the typed path included
     (`delete::in_use`, also `Jobs::uses`). Check and removal run with the job table locked, so no job is
     admitted between them. Leftover `.part`s are not cleaned up here. On the PS5 (delete and move,
     `delete::smp_refusal`, by user request) also anything at or in `/mnt/shadowmnt` (source or target), an
     image ShadowMount+ has mounted (`edit_job::smp_mounts`) and, while it has any, a folder holding an
     image of a mounted stem (walked, links not followed, 200,000 entries); fails closed.
     Moving or renaming (the File explorer's Rename and Move to; core `Jobs::move_path(from, to,
     replace, protected)` → `DeleteError`, `delete::move_`; server `POST /api/move_path` `{from, to,
     replace?}` → `null`, same codes and `keep` list; Tauri `move_path` `{from, to, replace?}`): `from`
     refused as delete refuses it; `to` a plain name in an existing folder, never replaced unless `replace`
     (Replace, below; else `rename_no_replace`, "already exists; nothing is replaced"; a case-only rename
     uses a plain rename), not inside `from`, on the same volume (by `st_dev`, or EXDEV: "can't move between drives"),
     not at, in or above an unfinished job's paths (Busy); an image with an undo journal beside it (or one
     at the new name) refused. Both folders synced. No copy, no new folder, no upload (by user decision).
  2. Edits (the Edit tab, below): a request with non-empty `ConvertRequest.edits` is an edit job, which
     changes its source (`output` and `format` ignored; `JobReport.output` is the source). A folder is
     changed directly (no rollback); an image is edited in place by its format's editor behind an undo
     journal, or rebuilt beside and renamed over it (`edit_rebuild`): the job writes
     `<source>.<job>-<pid>.part` beside the source, verifies it as usual, re-checks the source's
     `SourceStamp`, closes the source and puts the part in its place (`Part::replace`: the original kept as
     `*.orig.part` (a hard link, or a rename where the volume has none), folder synced, the part renamed over
     the source, folder synced, the backup removed). Any failure leaves the source byte-identical and the
     `.part` deleted (where putting the original back fails, the error names its `*.orig.part` backup).
     The server's stale-part filter takes an edit job's `.part` from its source.
  3. Delete the source after a successful job (opt-in; `ConvertRequest.delete_source`, CLI
     `--delete-source`): a conversion or an LZ4/zstd pack or unpack only (with an edit, extract, transfer or
     install a preflight finding). The job records what it read (`delete::ReadSource` in `Ctx.read`, handed
     back to `Inner::run`): a folder's `FolderListing` (every file's path, size, mtime and identity
     (`finalize::path_id`: unix dev + ino, Windows volume serial + file index) and every empty folder, as the
     job's scanner kept them), or an image's or archive's `SourceStamp` (an archive's every part as stamped
     when opened; parts are never looked for again after the job). Once the result is Ok (verified, output in
     place), `jobs.rs` `Inner::delete_source` checks it first, outside the job-table lock
     (`delete::source_unchanged`; a source on a NAS or server, `smb://`/`nfs://`/`ftp://`, kept at once with no
     rescan), then drops the job's own paths and removes it with the table locked (`delete::remove_source`:
     the protected, guard and in-use checks, then the removal; no job admitted between; a peer source is deleted
     with the table unlocked, the job's own paths kept in it as the reservation, since the peer's delete takes its
     own table and two devices deleting on each other at once would deadlock): a folder scanned again by the
     same rules must list exactly the same files, sizes, mtimes, identities, empty folders and (local) the
     folder's own identity (skipped junk,
     `.DS_Store`, `._*`, goes with it, by design), an image or every archive part must keep its stamp, else
     `source kept: <path> changed since it was read`; each local part of a multi-part archive `check`ed before
     the first goes, then `delete::delete` per part with the `Jobs::set_protected` list and every other
     unfinished job's paths guarded; a source holding the output is refused; a peer source goes to the peer's
     own Delete (`source_on_peer`: refused or busy means nothing went; any other failure, a not-found answer
     too (the rescan just found it: a retried request whose first reply was lost), is "<path> may be partly
     deleted"), a NAS is refused. By user decision the job stays Ok either way: one line in
     `JobReport.checks` and the log, `source deleted: <path>`, `source kept: <reason>`, or `source partly
     deleted: <what is gone>; <error>` when something went before a failure (a later archive part, a folder
     whose removal stopped part way, a sync after the removal). ponytail: a peer folder (and an OS without
     file identities) is compared by size + mtime only; a file added between the rescan and the removal (on a
     peer, before its own delete) goes with the folder. UI (Convert and Pack): **Delete the source** (by user request)
     (`target.tsx` `DeleteSourceSwitch`, `.field.verify.delete-source` under Full verification), off by
     default, in memory only (kept across sources), a muted hint ("No undo. The source is deleted only
     after a successful conversion."); for a NAS source off and disabled with a `.note-line.warn` (`target.deleteSourceNas`), and
     `delete_source` never sent for a NAS path; hidden for a patch package, in Pack shown only with its
     settings. Job rows (`JobList.tsx` `sourceLine`, matched by the backend's English prefix, left out of the
     Verified fold): `jobs.sourceDeleted` as a muted `.note-line`, `jobs.sourceKept` and
     `jobs.sourcePartlyDeleted` as a `.note-line.warn`. A job whose checks carry `source deleted:` makes
     `App.tsx` call `notifyDeleted` from its Done event and, in the web build, when the poller restores it
     already finished (`jobs://restore`: a reload, another viewer's job); not a peer's mirrored job; the
     notified job ids are kept in memory, so each is heard once (ponytail: by id only, so a payload restart
     reusing a notified id isn't heard).
- Replace after confirmation (by user decision): File Transfer jobs, the phone upload
  (`/api/connect/upload`), Edit's Extract and the File explorer's Rename and Move to refuse a name
  already there, as before; once the user confirms, they put the new one in its place. Opt-in per request
  (`ConvertRequest.replace`, `move_path`'s `replace`, the upload's `replace=1`); without it behaviour and
  messages are unchanged. The UI asks first by a read-only check, never by an error's text: core
  `Jobs::existing_targets(&ConvertRequest)` (an extract's or transfer's top-level targets that exist, on
  a peer or NAS too; Tauri `existing_targets {request}`, server `POST /api/existing_targets {request}`,
  the 2-read cap), `list_dir` for Rename/Move and the upload; then one dialog (`app/src/replace.tsx`
  `confirmReplace`: "N items already exist there", the folder, at most 10 names + "and N more", Cancel
  focused, Replace). Only a file onto a file, and a folder onto a folder, which merges (same-named files
  replaced, everything else there kept, by user decision); a kind mismatch at any level, a link or a
  special file is a finding ("<path> is a folder: a file is not put in its place", "... is a file ...",
  "... is a link ..."); the upload answers 400 for one (409 a job using it) and the web UI leaves such
  a file out and says so (`uploadSkipped`). A replaced
  file is refused as Delete refuses it (`delete::replace_refusal`: `delete::check`, an undo journal
  beside its image; a merged folder only at, in or above another unfinished job's paths;
  `Jobs::set_protected` gives a job delete's `protected` list: server `keep_list`, app the home folder);
  on a peer or NAS only this device's jobs are checked here, the device judges the rest. Each file
  still goes through its exclusive temporary name, synced, then renamed over the old one
  (`std::fs::rename`, `Remote::rename_replace`; never remove-then-rename); a merged folder is kept, not
  re-created. Free space counts every byte written (the replaced files aren't credited). One log line
  `replace: N files replaced: …` (first 10 + a count). A move with replace: a file over a file in one
  rename, a folder merged entry by entry (every check first, then the moves, the emptied source folders
  removed; a failure part way says "N of M entries were moved before it and stay"). ponytail: checked at
  preflight; a target changed between the check and its rename isn't seen.
- Edit tab (after Convert; it replaced Inspect, by user decision; `app/src/Edit.tsx`): left a compact game card
  (`SourceCard compact`, smaller than the other tabs' by user decision: columns 1fr:2fr from 1100px, 56px
  cover, title ID and version tiles, findings; folds Game facts, param.json, Details; Delete in its head,
  none for a NAS or peer source; under it a Leftovers card only when `.part`s were found),
  right an explorer of the source (`role="grid"`, crumbs, Up, a filter over
  everything below the folder shown, folders first in natural order, at most 2,000 rows shown; arrows,
  Home/End, PageUp/PageDown (10 rows), Enter or a double-click, Backspace or Alt+Up, F2 rename, Delete),
  "How this works" before a source, a Reading card while one is read. Archives are browsed and extracted
  from, never edited (by user decision). Changes: rename, delete,
  Add files / Add folder (the tabbed picker: this device or a paired peer, never a NAS; the web picker browses
  the server and multi-selects within one folder; from a peer `Add.from` is its `flink://` path, read over
  FLINK, each file stamped by size + mtime and re-checked on every read; a peer's root is refused,
  `edit.peerRoot`) and drag and drop onto a folder row, elsewhere on the card the folder shown (the count
  line says "Drop to add to <folder>"; Tauri `onDragDropEvent` gives paths, a browser `File`s); no New
  folder button (by user decision: a `Mkdir` comes only from a dropped or added empty folder). They stack in the UI and nothing is written until **Apply** (by user decision);
  **Undo** takes back the last batch (one drop is one batch), **Discard all** clears them (asks above one
  change); no per-change removal (ruling). The pending list folds as core will (`explorer.ts`, a port of
  `Edited`'s rules; a host folder listed through `host_tree`, so it folds file by file) with badges New
  (green), Replaced (orange), Renamed (blue, "Was <path>"); a clash asks once per action (Cancel, Skip,
  Replace; Replace of the other kind deletes it first; a folder onto a folder merges, as core does).
  - **Modes** (by user decision, shown in the pending bar): **In place** (written into the source; the
    default when both work) or **Rebuild** (a new image of the source's format beside it, verified as a
    conversion, then renamed over it; every image format, `.ffpfsc` and `.pkg` included); a folder is
    always **Changed directly**. The UI asks `edit_check` 500 ms after the last change (core
    `Jobs::edit_check(&ConvertRequest) -> EditCheck`, reads only but for settling a leftover undo journal
    (below) and, on the PS5, the destination probe's file: `in_place`/`rebuild` findings, empty =
    that mode works; `needed_inside`/`free_inside` from the `.exfat`/`.ffpkg` editor's `plan_sized`;
    `rebuild_needs` = the image + 64 MiB (`.ffpfsc` its every-block-raw worst case, `.pkg` the builder's
    estimate + 1% + 64 MiB) and `free_beside`; fields that don't apply `null`; a job using the image gives
    "a job is using <image>" in both lists). In place refused shows **Rebuild needed** with the reason
    (`.ffpfsc`/`.pkg`: "always rebuilt"),
    both refused **Can't apply** with core's findings, both possible a radio choice (by user decision In
    place green, Rebuild red, chosen or not; the single-mode tags the same, Can't apply red, Changed
    directly blue); a failed check says so with **Check again**; changes that leave the source as it is
    show "Nothing to change" (no mode; Apply refuses an empty list). Apply is off while a change won't
    apply ("N changes won't apply"). Apply confirms (no
    undo; close the game first; an interrupted edit is restored first) and starts the job; while the Apply
    or its job runs, the changes, the source pickers, Delete, Extract and a Library hand-off are locked (a
    pick or hand-off isn't opened, a note says so); otherwise opening a source (another, the same
    again, a Library hand-off) drops the pending changes without asking. Done Ok: the source is read again and the
    changes cleared; a failure or cancel keeps them with the error (a folder is read again: part may be
    in). An edit job of the source not started here (a reload, another viewer):
    the source is read again when it ends, the changes kept. Job rows: `.tag.edit` **Edit** or **Edit ·
    rebuild**, the error box led by "Can't apply the changes:"; a cancelled job's error that starts
    "cancelled; " (what stays changed) is kept and shown as cancelled. No CLI subcommand for edits (ruling).
  - **Room** (by user decision, as ps5upload): no growing an image and no FAT reserve; what an in-place
    edit adds must fit the free space inside, else one finding "the changes need N MB, the image has M MB
    free" (the editor's own count: data, new and grown folders, UFS2 indirect blocks) and Rebuild is
    offered. A rebuild follows exception 2 above: `<source>.<job>-<pid>.part` beside it (room for the image +
    64 MiB), verified, the `SourceStamp` and ShadowMount+ re-checked, `Part::replace`; any failure leaves
    the source byte-identical and the `.part` deleted (where putting the original back fails, the error
    names its `*.orig.part` backup). A rebuilt `.ffpfsc` keeps its inner format at the
    request's `ffpfsc_level`, a `.pkg` takes `kraken_level` (the original's level can't be recovered, so
    the size may differ).
  - **Undo journal** (in place; the formats have no journal of their own): `edit_job.rs` hashes the
    expected tree's verify sample first, re-checks the `SourceStamp` and ShadowMount+, then `patch::apply`
    writes `<image>.forge-undo` beside the image (`FORGEUNDO1`, image and journal lengths, entries of
    offset + length + old bytes, `FORGEEND` + CRC-32) with the old bytes of every region the edit
    overwrites, syncs it and its folder, writes new data into free space, syncs, then the live writes
    (metadata; a `.ffpfs` replacement's data too, journaled), syncs. Verify reopens the image with its reader against the edited tree (fast or full); a
    mismatch, cancel, error or panic before the commit rolls back ("the edit did not verify; the image was
    restored"); the commit deletes the journal (a delete that fails rolls back) and syncs the folder; a sync failure
    after the delete fails the job saying the image holds the edit (ruling: it can't roll back then). Restored means every byte the
    filesystem uses is the original's; free space may keep the new, unreferenced data. Every open for
    editing (job, rebuild scan, `edit_check` under the job-table lock unless a job uses the image) runs
    `patch::recover` after the ShadowMount+ check: a complete journal is restored then deleted, one cut
    short is deleted (nothing live was written; the image isn't opened, so a missing or read-only one doesn't
    block it), a damaged one is kept with an error ("keep both files").
    `Inspection.interrupted_edit` (a journal beside the image) is noted on the card. Room for the journal
    beside the image is a preflight finding (`undo journal:`).
  - **Refused** (preflight findings, nothing written; every offender listed): any LZ4 or zstd option,
    `remove_backport`, an archive, in a packed game a change to its packs (below), a bad game path (relative,
    `/`-separated, no empty/`.`/`..` part, no `\`), a source on a NAS or a peer (browsed and extracted from
    only), an op that doesn't apply, an `Add.from` on a NAS or that is missing,
    a link, not a file or folder, the image itself or inside the source folder (by identity), names the
    format can't hold (the conversion's checks run on the edited tree: `preflight::input`, depth, the
    writer's layout), the editor's own findings, and on the PS5 (by user decision, as ps5upload) an image
    ShadowMount+ has mounted (an entry `<stem>_<8 hex>` of `/mnt/shadowmnt[/pfsc]` that is a mount point
    by `st_dev`; fails closed; checked in both modes at scan and again before writing or replacing), and a
    source folder or image at or inside `/mnt/shadowmnt` (a mounted game; `edit_job::smp_inside`). Host
    files are stamped when listed (identity, size, mtime) and checked on every read: a change fails the job.
  - **Packed game** (by user decision, `edit_job::Packs`): edited like any other, only its packs locked:
    an op naming, or an edit removing or resizing, the LZ4 manifest, `.crc`, `.runtime`, a volume it names or
    `ampr_emu.index` (its records match the manifest's by file id), and anything at or under a packed path
    (or a file at a folder of one) is a finding. zstd packs the same (`Packs::of_zstd`): `ampr_zstd.index`, every
    volume it names, `ampr_emu.index` (it lists the packed files) and the packed paths. The runtime, journal, logs, `eboot.bin` and every loose file
    may change. No fresh index and, into a `.pkg`, nothing left out. ponytail: the index stays as it is, and
    ampr_emu answers `/app0` lookups from it, so a file added, resized or removed through AMPR keeps its old
    entry (a note line says so); `reader::unpack` lets a loose record's file be missing or resized.
  - **AMPR** (a title whose `eboot.bin` imports libSceAmpr or that holds `ampr_emu.index`; `edit_job::fold`,
    shared by folder edits): an edit that touches the index itself, or changes the (path, size) set while the
    index doesn't already list exactly the edited files (`index_state`), writes a fresh `ampr_emu.index` (a
    user's own index is overwritten: a stale index crashes the game) and then removes `ampr_commands.bin`
    and the logs (their file ids follow the old index; one note line); otherwise all three stay. The
    runtime is never swapped, in either mode (ruling: the user's edit
    is what they asked for, and both modes give the same result), so a rebuild edit does no plain-convert
    repair. A `.pkg` rebuild always leaves the journal, logs and old index out (the builder indexes), as a
    conversion into `.pkg` does.
  - **Folder** (`edit_folder.rs`): every op resolved and checked first (free space for the added bytes),
    then applied in order: Delete (links not followed), Rename (no replace), Mkdir, Add (each file to
    `.<name>.forge-<pid>-<n>.tmp`, synced, renamed over); the AMPR rule above; touched folders synced;
    verify re-reads the added files and the folder against the planned listing. No rollback (ruling: a
    copy and swap doubles the space and isn't atomic either): a failure part way leaves the earlier ops
    applied and says which (and "the failed change is partly applied" when it made parents or began a
    folder delete). Each path is pushed one plain name at a time (a `C:` part can't replace the root), the
    game folder's identity is kept from planning and re-checked by every op ("was moved or replaced"), an
    `Add.from` ending in `/`, `.` or `..` is refused, and a temporary name keeps at most 160 bytes of the
    file's name. `edit_check`: findings in `in_place` only, `rebuild` empty, sizes `null`.
    ponytail: no openat walk (a folder inside swapped between check and use is not caught), and a volume
    mounted inside the game folder is not refused.
  - **Uploads** (web build; a browser's files are on the viewer's machine, ruling): nothing is sent before
    Apply, then each file goes one at a time (progress, Cancel) to `POST /api/edit_upload?source=&path=`
    (`encodeURIComponent`, raw body of any size, `Content-Length` required, 30 s idle per read) and is
    staged at `<source>.forge-upload-<pid>.part/<tag>-<k>/<game path>` (`stagingNames`: a fresh random tag
    per Apply, k counts this Apply's uploads, so no two staged paths meet and two Applies on one source
    never replace each other's files) via `.forge-upload-<pid>-<n>.tmp` + rename (0777 on the PS5); 409 for a target an upload in
    flight or an unfinished job uses. Apply then checks again with the uploads in and starts the job, or
    stops. `POST /api/edit_discard {source}` removes the staging folder through `delete_path`'s rules (409
    while an upload or a job uses it) once the job ends or after a failure before the job (Discard all only clears the UI: nothing is staged
    before Apply).
    It costs the files' size of free space beside the source; `stale_parts` lists it once nothing uses it.
    The check before Apply leaves uploads out (`checkable`), or waits for Apply when a later change builds
    on one. The desktop app adds host paths directly (`stage` refuses).
  - **Routes**: Tauri `edit_check {request}`, `host_tree {path}`; server `POST /api/edit_check {request}`
    and `POST /api/host_tree {path}` (both count against the 2-read cap; `edit_check` runs `readable()` on
    the source and each `Add.from` first, so a FIFO is 400), `edit_upload`, `edit_discard`; an edit job
    starts through `start_job`. `host_tree` → `HostTree {kind, size, files, empty_dirs, truncated}`: a
    link refused, a folder through `ScannedFolder::scan` (junk, links, special files, hard links, names as
    `Edited` sees them; a `flink://` one through the peer's listings, `peer_tree`), `truncated` past 200,000 entries or 15 s (ponytail: a truncated listing skips the
    scanner's hard-link and unreadable-file checks; the job's scan runs them).
  - **Extract** (user request, MVP; `extract_paths.rs`): a row's Extract (file or folder on disk, not a
    pending one) and the toolbar's **Extract folder** (the folder shown; at the root every top-level
    entry, the whole game; off, its title saying why, for a folder only the pending changes hold) ask for
    a destination folder on this device, a paired peer or a saved NAS and start a job with
    `ConvertRequest.extract` (game paths), `format` folder, `output` that folder, no edits; any source
    `open_source` opens (folder, every image, `.pkg`, archives), no game-root checks. Each selection lands
    under its last component (`data/movies` → `<output>/movies/…`). Refused before a byte is written: a
    bad or unknown path, two selections with one name, one inside another, a target that exists (unless
    the user confirmed Replace, above: `existing_targets` asked first, then `replace`), an output that is missing, not a folder, inside the game folder or (PS5) at or inside
    `/mnt/shadowmnt`, names the destination
    can't hold, no room (+ 1 MiB), an edit, `edit_rebuild`, `lz4`, `zstd_pack` or `zstd_unpack`. Stages `scan`, `preflight`, `write`, `verify`;
    each file streamed in 8 MiB reads through an exclusive temporary name beside it, synced, renamed
    without replacing (over the file it replaces); the source read one range ahead on its own thread
    (`extract_paths::read_ahead`, core's `Prefetch`; the next file too when the selection is in `files()`
    order); on macOS each new file a plain fsync and each selection's root folder one F_FULLFSYNC as the
    barrier (a replaced file keeps its full sync); BLAKE3 while writing; verify checks sizes (onto a peer or
    NAS in fast mode from one listing per written folder; full: re-reads and re-hashes). No
    rollback (ponytail): a failure or cancel deletes this job's temporary files and says "N of M files were
    extracted before it and stay" (none finished: "<folder> was created and stays"). It reads the source as it is on disk; with changes pending a muted line
    says so. Job row: tag **Extract**, error box "Can't extract:". Server and Tauri: `start_job`, and `existing_targets`
    before it.
  - **Additional content** (by user decision): a source with no `eboot.bin` at its root and a
    `sce_sys/param.json` that parses to a title ID (a DLC, an update or another package) is a source class,
    not a finding (`GameInfo`/`Inspection`/`LibraryEntry.additional_content`; without param.json or a
    title ID it is still `preflight::NO_EBOOT`). Every page labels it with a violet **Additional content**
    `.tag` beside the kind/DLC/packed labels (Edit's card, Convert's and Pack's source card, Library cards and
    rows); the Library lists it (an archive falls back to a param.json-only root when no game root exists).
    Every edit, in every mode, applies to it, Extract too; a conversion goes only into `pkg` or `folder`
    (else `preflight::ADDITIONAL_CONTENT_TARGET`; Convert offers only Folder and fpkg, default fpkg), its
    param.json category not checked; the Pack tab shows a note and no actions. The `.pkg` builder no longer
    refuses it (vendor patch 0023; `content_class` gives 0x21 when param.json has neither
    `applicationCategoryType` nor `applicationDrmType`, else the game classes 0x20/0x26; package.rs notes "packaged as additional content (DLC) or an update").
  - **Patch packages** (user request, from issue #7): a `.pkg` starting `\x7FLIH` (a debug patch; layout
    measured on one, PPSA23566 01.006, no public spec) is a 64 KiB header with `{offset, size}` pairs for
    the patched package's FIH block (`0x20`), its CNT (`0x30`) and a stored zip of PlayGo/relocation tables
    (`0x40`, unread), and between FIH and CNT the outer blocks the update changed. FIH and CNT describe the
    whole patched package, so `lih::View` places each stored block by its SHA3 in `imagedigs` and serves
    that package; any other block is the base game's (`IN_BASE` error). Its layout descriptor carries
    `00 00 06` where plain builds carry `00 00 04` (any other value skips the records' nibble checks; vendor patches 0027, 0028). The tree walk skips
    directories and inodes the patch doesn't carry, and only files every block of which it carries are
    listed (the container's always); `details` names what is left out. `Inspection.patch`: an orange
    **Patch** tag on the source card; Edit browses and extracts only (`edit.patchNote`; edits refused in
    both modes); Convert, Pack and every conversion refuse it (`preflight::PATCH_SOURCE`, Convert's
    `patchBlocked` with **Open in Edit**; a conversion refuses it before opening it, since opening reads
    and hashes the whole patch); its root isn't judged (no findings, never additional content); `lz4`
    facts `None`; the Library leaves it out. Applying a patch
    to its base needs the base package's own blocks (a dump has none of them): not done.
- Storage (the tab after Pack, `app/src/Storage.tsx`; it replaced the Library tab, by user request): one card
  (by user decision, design "D"), its head the view switch (`.seg.view-switch`, `role="radiogroup"`, smaller
  than the screen tabs so it never reads as a second navbar): **[disc] Library** | **[folder] File
  explorer** (the default, by user decision; under 400px the labels are visually hidden, still the radios'
  names), and on the PS5 a third, **[package] Package installer** (below), then the view's actions; each view
  owns its card and head, a copy of the switch in each; the view and, on a computer, one chosen folder shared by
  Library and File explorer live in memory only; every view stays mounted. Storage lists its install jobs
  (`JobsCard`) and counts them in its tab label.
- Library (the Storage tab's first view, `app/src/Library.tsx`): the game dumps on this computer or console, scanned
  on demand; no database, no index, no persisted state. Core `scan_library(roots)` (PS5 server roots only;
  elsewhere it errors) / `scan_library_folder(root)` (a chosen folder) / `library_cover(path)`; Tauri
  `library_scan {root}` (`root` required) / `library_cover`; server `POST /api/library_scan` `{}` (`"root"`
  string: scan that folder on any platform, a non-string is 400; without it the PS5 scans its roots and a
  computer answers 400 "choose a folder to scan"; on the PS5 the reply adds `storage` `{free, total}`
  (bytes, `statfs` of `/data`; absent when `statfs` fails), shown first in the head's actions, before Refresh,
  as "Free 312 GB of 825 GB" (decimal GB, whole numbers, one decimal under 10 GB; the File explorer shows the same,
  from `POST /api/storage` → `{free, total}` or `null` (a computer, or `statfs` failing), renewed with each listing;
  the PS5 only, by user request; no game count line there: the kind chips count)) and `POST /api/library_cover` `{path}` (409 while
  stopping, 429 when the shared cap of 2 concurrent source reads (inspections, naming, covers, scans, `host_tree`, `edit_check`) stays full for
  30 s: a read waits for a slot first, at most 4 waiting; 409 once stopping).
  A chosen folder may be on any filesystem, a network share included; mounts nested inside it are walked,
  a chosen root that is a link is resolved first, links inside the walk are never followed,
  `/mnt/shadowmnt` is refused.
  - **Roots**: on a computer the user chooses one folder (no automatic discovery). On the PS5 the server's
    `Roots::current()`: `/data` and each drive while mounted (the browse roots; no `/mnt/shadowmnt`),
    classified by FreeBSD `statfs` (`MNT_LOCAL` and ufs, msdosfs, exfat, exfatfs, bfs, nullfs, cd9660,
    udf, zfs).
  - **Refused folders** (`scan_library_folder` errors, so the app and the server agree; one list,
    `library_roots::refusal`, checked on the canonical path, ASCII case-insensitive on macOS and Windows):
    the filesystem root (Windows: the system drive root), `~/Library` / `~/AppData` and everything inside,
    and each OS's system folders with everything inside (macOS `/System`, `/usr`, `/private` except
    `/private/tmp`, ...; Linux `/proc`, `/usr`, `/run` except `/run/media/…`, ...; Windows, Program Files,
    ... on the system drive). A path at or inside the canonical home is allowed first (a `/var/home/me` or
    `/root` home works). Everything else is allowed (home and its children, `/Volumes`, `/media`, `/mnt`,
    volume and drive roots). `library_cover` refuses links, devices, `/mnt/shadowmnt`, a file
    without one of the five image extensions and an image on another filesystem than its folder.
    Walk exclusions: 12 skip names, the home's app-data folder, `.part`, non-UTF-8 names.
  - **Limits** (`library.rs`): depth 6 (root 0), 20,000 directories and 200,000 entries per root, 10,000
    entries per directory, 256 image opens, 2,000 games, 15 s budget, 20 warnings; hitting one sets
    `truncated` (the UI says the list may be incomplete).
  - **Recognition**: a folder with a plain `sce_sys` holding a `param.json` (1 B..4 MiB) that parses to a
    title ID is a game and is not entered (a rejected folder is searched on); without a regular
    `eboot.bin` it is listed as additional content (by user decision, above); images by extension
    (`.exfat .ffpkg .ffpfs .ffpfsc .pkg`, any case) are opened with `open_source` and need the same files.
    Entry = path, kind, title ID, localized title (fallback: title ID, then file
    name), version, `has_cover` (a regular `icon0.png` of 1 B..512 KiB by metadata; discovery reads no
    icon), `lz4_packed` (`reader::detect`'s rule: a root `ampr_assets.index` starting with `AMPRPAK4`; the
    UI's LZ4 badge), `zstd_packed` (a root `ampr_zstd.index` starting with `AMPRZPK1`; the zstd badge),
    `additional_content` (its label). Sorted by title, ID, version, path. `library_cover` reads `sce_sys/icon0.png`
    (≤ 512 KiB) lazily, one at a time in list order (the chosen kind chip's entries first), only for an
    entry the page lists; failed ones are tried again after 3 s, at most 5 rounds per scan.
  - **Semantics**: `runtimePlatform()` (`GET /api/session` `platform`) decides. On a computer (`host`)
    nothing scans until the user chooses a folder (**Choose folder**; **Refresh** rescans it). A refused
    or failed newly chosen folder is an error screen with core's message and drops the previous folder and
    list; a failed Refresh keeps the list. On the PS5 the first open scans the server's roots, **Refresh**
    rescans; no picker. Everything lives in memory until the app closes. Cards and rows show the title ID.
    While a scan runs Refresh and Choose folder are `aria-disabled` (they keep focus); no games found is
    an empty panel (on a computer with **Choose another folder**); "Loading" until the platform is known.
    A folder chosen in the File explorer is scanned once the Library shows, as if chosen here.
  - **Kind chips** (under the head and the count): All, then only the kinds present (Folder, ffpkg, exfat,
    ffpfs, ffpfsc, fpkg), **LZ4**, **zstd** and Additional content, each with its count; one chosen
    (`aria-pressed`), in its format hue; a chosen kind that is gone shows All. Cards or List (compact rows).
  - **Actions**: Edit and Convert hand the entry to `App` as a `SourceSelection {id, path, kind}` (one per
    tab; the tab switches and loads it; Edit refuses it while changes are being applied); Delete uses the
    same `confirmDelete` (`app/src/delete.tsx`), `delete_path` and refusals as Edit, then `notifyDeleted`,
    which Edit, Convert, Pack, the Library and the File explorer all hear. While a delete runs every other
    trash button is off and the entry's Edit and Convert too; its own trash reads "Deleting <name>".
- File explorer (the Storage tab's second view, `app/src/Files.tsx`): Edit's explorer (classes, crumbs, Up,
  filter, `role="grid"`, the same keys, at most 2,000 rows) with no pending list, no Apply and no note: every action
  happens at once, a destructive one after the question. From 880px two panes (by user decision): a folder
  tree ("Folders") beside the list, the chain from the root to the folder shown, each folder on it with its
  subfolders (`list_dir`, lazily, one folder at a time from the root down; kept while the view shows and
  listed again, still shown until the answer, on Refresh, on showing again and after a move or a change
  outside the folder shown; dropped only for a new root, folders off the chain pruned), the current one
  `aria-current`, any opened by a click (focus stays in the tree; one tab stop, arrows/Home/End move); the
  crumbs hide there (Up stays). Below 880px no tree. Root: on a computer the chosen folder (Up stops
  there), before one is chosen the home folder (the app: Tauri `home_folder` → `Option<PathBuf>`, not
  shared with the Library, so it never scans home by itself) or the server's roots (the http build); with
  neither (no home, or the start fails to list) the choose-a-folder panel; on the PS5 the server's browse
  roots (`list_dir` `null`; nothing to change there). Row actions: Rename (pencil; inline `NameInput`, F2),
  Move to (the folder picker from the folder shown; `<picked>/<name>`; its own folder refused; for both a
  name of the same kind already there is replaced, a folder merged, once the user confirms (Replace,
  above); `NameInput` still refuses one of the other kind, a link or a mounted image), Delete
  (`confirmDelete`, its file, image and archive variants by extension; Delete key, "fn+Delete" in a Mac's
  tip), a ⋯ menu (`ChooseMenu` with `buttonLabel`) of Edit and Convert for a
  folder or an image/archive by extension (the Library's `SourceSelection` hand-off), open a folder. A rename or move also
  `notifyDeleted(from)`. Links: a **Link** tag, opened when they point at a folder, never changed (core gives
  a link's resolved target as its `path`; the row keeps its own path, the target as `target`, which only
  opening uses). One action at a time: its row keeps focus (`aria-disabled`, "Deleting <name>" for a
  delete), can't be opened and its ⋯ is off; every other row's actions are locked. Listed again after each
  action (a failed one too), when the view shows and on others' deletes (one heard during this view's own
  action waits for its end; a folder on the chain that went is left). The grid stays mounted while a folder
  lists (a spinner only past 150 ms); Refresh is `aria-disabled` + `aria-busy` until its answer.
  On the PS5 a `.pkg` row (not a link, not at the roots) has **Install** (`installHere`: the install question,
  then an install job, below). **Device tabs** (a computer, `platform` `host`; `FilesTabs.tsx`): while a paired
  PS5 is online the view shows a tab bar, **This device** then each online PS5 (its `flink://<key_id>` share
  root, no Choose folder), each tab its own explorer kept mounted with its own folder (a PS5 gone offline:
  its tab goes, its explorer and folder stay, This device shows). A PS5's tab renames, moves and deletes there
  as here, by that PS5's own rules (`delete_path`/`move_path` RPCs, below); its Move to picker lists peers
  only (a move stays on one device). Drags (mouse pointer events, ponytail: no touch; Move to does it there):
  a row onto a folder row of its own tab moves it there, onto another device's tab copies it into that tab's
  folder shown; the app's files dropped from the system copy into the folder row under them, else the tab
  under them, else the folder shown (the web build ignores browser drops: Upload under Connect sends them). A
  copy is a transfer job (`transferRequest`, Replace asked first), listed under Connect; a status line says
  it started, with Open Connect.
  Core `list_dir(path) -> Listing {path, parent, entries: [{name, path, dir, size, link, mounted}],
  truncated}` (`listing.rs`, at most 10,000 entries; links resolved, a dangling one as itself);
  server `POST /api/list_dir` `{path}` (`null` lists the roots; `parent` `null` at a root; on the PS5, by user
  request, a path not at or inside `/data` or a mounted drive, resolved, is 400, as for `host_tree` and a
  Library folder: `paths::Roots::confine`; a link whose target is outside them is left out of a listing);
  Tauri
  `list_dir {path}` (`parent` is the filesystem's: the UI stops Up at its root), `move_path` and
  `delete_path` refuse (`from` and `to`) what the Library refuses as a chosen folder (core `folder_refusal`,
  its message).
  Delete and move refuse an undo journal (`*.forge-undo`) while its image is there, and move refuses
  a new name that would read as one ("open that image in Edit"). Paths are `dunce`-canonical (Windows `C:\x`, not `\\?\C:\x`; server roots too).
  The explorer sends `explorer: true` (the Picker doesn't): the server's roots are the same and core's
  `list_explorer` lists; on the PS5 it hides ShadowMount+'s mount
  folder (a path in it is 400, entries resolving into it left out) and marks `mounted` images (a
  **Mounted** tag, and once above the list a note saying why; Edit/Convert only). Elsewhere it is `list_dir` (Tauri ignores the flag).
- Package installer (the Storage tab's third view, the PS5 only, `app/src/Packages.tsx`): every `.pkg` under the
  server's current roots (any case), scanned on the first visit and on Refresh, nothing kept; core
  `scan_packages(roots)` (`library.rs`: the Library's walk, refusals and limits, game folders entered too; each
  package read only by `fpkg::peek`, its header and container, a few KiB, a patch's blocks not placed) →
  `PackageScan {entries: [PackageEntry {path, size, content_id, title_id, title_name, title_names, version,
  content, has_cover}], truncated}`, `content` `game`/`patch`/`additional_content`/`unknown` (unreadable, or a
  content type Forge doesn't know: listed untitled, the console judges it), sorted by title, untitled last;
  server `POST /api/package_scan {}` (the 2-read cap, 409 once stopping, 400 on a computer; the app has none).
  Rows (a package icon, no cover read): title (else title ID, else file name), title ID, version, size, a
  content tag (Patch orange, Additional content violet), the path, **Install** and right of it a trash button
  (by user request: the Library's `confirmDelete` file variant and `delete_path`; the row leaves the list, no rescan);
  the head shows the free space, Refresh and the Library's Cards / List toggle (List the default, memory only).
  The File explorer's Install on a `.pkg` row (PS5) shows the package icon, as Connect's Install pkg.
- Install (user request; core `install.rs`, `ps5/installer.c`): `ConvertRequest.install` (a `.pkg` here, on a
  peer or a NAS; `source`, `output` and `format` ignored) is an install job, stages `preflight`, `install`; off
  the PS5, with any edit, extract, transfer, LZ4, zstd, backport or replace option, or for a file that isn't a
  `\x7FCNT`/`\x7FLIH` package (a link, empty, not a file) a preflight finding. Sony's installer takes a URL
  (at most 127 bytes) and pulls the package over HTTP Range requests (several connections, HEAD, keep-alive,
  sometimes a `<package>.crc` sidecar), so the job serves it on a `127.0.0.1` port of its own
  (`http://127.0.0.1:<port>/<16 hex>/<name>`: the file's name when plain ASCII, else `<content id>.pkg`, else
  `package.pkg`; at most 16 connections, a fixed past `Last-Modified`) and hands that URL to
  `forge_pkg_install` (`sceAppInstUtilInstallByPackage`, the name hint the file's name without `.pkg`). Sony
  refuses at once with a code (`0x80A3...`, a hint where known: base game missing, wrong version, close the
  game, no space, ...) or fetches; the bar is the bytes served at least once, and the job is done once every
  byte was served and 2 s pass with no request; 5 min with no request fails it, a cancel says the PS5 may still
  list the download. Nothing is written here. ponytail: done means every byte was fetched once; whether the PS5
  finishes the install is not seen. It runs in the transfer slot (an install waits behind a transfer and the
  reverse), its package guarded as a job's source. UI: one question (`InstallDialog.tsx` `confirmInstall`: the
  facts known, Cancel focused, a reinstall warning unless a patch or additional content), from the Package
  installer, the File explorer's `.pkg` row and Connect's **Install pkg**: an online paired PS5's row in the
  Devices list picks a `.pkg` on this device (inside its share root) and asks that PS5 (Tauri `connect_install
  {keyId, path}`, server `POST /api/connect/install {key_id, path}` → `{job}`; FLINK RPC `install {path}`,
  relative to the caller's share root, plain names only, answered only where the PS5's server set
  `Connect::set_install`, else `this device can't install packages`), which reads it over FLINK; the job is
  mirrored here. Job rows: `jobType` `install`, tag **Install**, the package as "from", no format pill, error
  box "Can't install:"; listed under Storage, a peer's (mirrored) under Connect (`tabOf`).
- Connect (the tab after Storage, `app/src/Connect.tsx`; server `connect.rs`, also run by the Tauri app at
  setup: FLINK and UDP only, no HTTP listener): Forge ↔ Forge pairing, a peer's files read and written
  over FLINK, transfer jobs, plus a LAN Discover. **FLINK** v2 (crate `ps5-dump-forge-flink`, our own protocol modelled on ps5upload's AVA1, ideas only; std +
  blake3 + serde_json + x25519-dalek, by user decision) on TCP 9883: authenticated by BLAKE3 MACs (per-direction keys,
  counters never reused) but **not encrypted** (by user decision; plaintext on the LAN, How to connect says
  so). **Pairing is approved on the other device** (by user decision; it replaced the magic code): the
  requester commits to an X25519 key, the target sends its key, the requester reveals (checked against the
  commitment), and both show one six-digit check number (a man in the middle gets one 1-in-a-million guess);
  the target's user sees "<name> (<platform>, <ip>) wants to pair" with that number and Allow / Deny
  (`PairPrompt`, over any screen and dialog, focus on Deny, a fresh dialog per request; closing it
  denies), the requester "Waiting for <address>" with the number and
  Cancel, 60 s at most (`flink::PAIR_WAIT`, pings keep it alive). Refused unasked: a key id already paired
  here (`already_paired`: a claimed id is never proved, so forget it first), more than 4 waiting or a second
  from one address or key id (`busy`, not counted), 5 denied or unanswered per address per 10 min
  (`too_many_attempts`). `/api/connect/approve` is as open as the rest of the HTTP API (by user decision); the
  app answers only in its own window. **Pairing persists** (by
  user decision, the one saved state beside `forge.lang`): `node.key` + `peers.json` in the data dir (PS5
  `/data/dumpforge/connect/`); Forget removes it on this side, closes its inbound control sessions
  and lanes, and refuses further use of outgoing cached sessions. A saved device is keyed by its key id,
  never its address: every handshake (in or out) and discovery reply updates its address in place, so a
  new IP or port never makes a duplicate, and pairing again updates the one entry. Saved devices: at most
  9, online = a handshake within 12 s. At start (by user request), with any device saved, the first probe
  round runs at once and one Discover search runs, so a device on a new address is found; the search runs
  at every start, saved devices or not, in the desktop app and on the PS5 alike (by user request; its finds wait
  under Nearby). Discover, labelled **Autodiscovery** in the UI (by user decision, every language) (10 s): Forge (UDP 8094; name, platform, port, key id,
  never the code) and NAS via mDNS (smb/ftp/nfs, trusted as announced; AFP dropped: no client; each result tagged SMB / FTP / NFS as the Devices list tags them, its **Save** opens that protocol's Add
  dialog prefilled with its name and `host[:port]`, by user decision; a Forge keeps Pair) and Windows PCs via WS-Discovery (`wsd.rs`:
  Probe to 239.255.255.250:3702 for `wsdp:Device`, matches typed `pub:Computer`, Resolve when no XAddrs, the name
  from one WS-Transfer Get to port 5357, else the IP). Every NAS or PC host found is probed meanwhile
  (`Lan::confirm_hosts`, 4 hosts at a time, `probe::serves`: SMB 445, NFS 2049, FTP 21, each confirmed by its
  protocol as Add a device checks it; a protocol mDNS announced on that host isn't checked again); a new
  batch starts at most 3 s past the 10 s (`PROBE_GRACE`; up to 64 mDNS + 64 WSD hosts can't stretch a search to
  minutes), so a search ends at most ~6 s late, a host still unprobed then shown as mDNS announced it (a WSD PC
  not at all). The mDNS list is republished whenever it changes (a later SRV or A record corrects an address
  or port). `mdns::merge` lists one entry per host +
  protocol + port under the host's mDNS (else WSD) name, a WSD host only once something answered. No subnet sweep
  (by user decision: only hosts that announce themselves). DLNA was removed
  (by user decision: it can't list or transfer games; don't re-add it). Pairing works on local networks
  only (by user decision; `connect::is_local`: private/link-local IPv4, 100.64/10 (Tailscale), loopback,
  IPv6 ULA + link-local): `dial` refuses another address, `accept_loop` drops it; a NAS is not limited.
  **Add a device probes first** (by user request; `probe.rs`, `Connect::probe`; server `POST /api/connect/probe`
  `{address}` → `{kind: forge|smb|nfs|ftp, host, port, name}`, Tauri `connect_probe {address}`): `host`, `host:port`
  or `[v6]:port` (`nas::parse_host`), the name looked up once (IPv4 preferred, within 1.5 s), then every check at
  once, each confirmed by its protocol, never by an open port: a Forge by a FLINK Hello (`flink::introduce`:
  Hellos swapped, nothing paired, asked or counted; local addresses only) on the port its reply to Discover's
  query sent unicast to UDP 8094 names (1 s; `discovery::ask`), else on 9883 or the typed port, SMB by an SMB2 NEGOTIATE reply (any status), NFS by an RPC NULL to program
  100003 v3 (PROG_MISMATCH counts), FTP by a `220` greeting. No port: Forge > SMB 445 > NFS 2049 > FTP 21; a
  port: FTP > NFS > SMB > Forge there. Each check gets 1.5 s to connect and 1.5 s for its exchange, then its
  socket is shut down (a server dripping bytes can't hold it): about 3 s, 4 s for a local Forge; replies are
  buffered as bytes arrive. A Forge off the local network (its discovery reply alone) yields to a NAS answering
  there, else pairing's not-local error; nothing: "nothing to connect to at <address>". A Forge's `host` is the
  address checked (a typed IP as typed, an IPv6 scope kept), a NAS's the typed name.
  The UI then pairs with the Forge at `host:port` or opens that protocol's Add dialog prefilled (Nearby's Pair
  sends the request directly, no probe). ponytail: one pick when several answer; a name is probed at one address; a NAS is not
  logged into before Add opens; a stalled name lookup's thread is left running.
  **Saved NAS devices** (by user decision): SMB, FTP or NFS (`protocol`, fixed by
  Add a device's SMB / FTP / NFS button or the probe, no radio; an edit keeps it; by user decision NFS too, and no WebDAV), saved from Autodiscovery (any result's
  **Save** opens Add NAS prefilled with its name, `host[:port]` and protocol) or added by address
  (`host[:port]`, the protocol's port by default: 445, 21, 2049) with an optional user + password (SMB: none
  is a guest login; FTP: anonymous; NFS: no login at all, the fields hidden and a save drops them),
  `nas.json` beside `peers.json` (0600; ponytail: the password plaintext at rest), never logged, never in
  a reply (`has_password`); one entry per host + port, whatever the protocol (a save there updates name,
  protocol and the credentials it names; an edit by `id` moves the one entry), Forget; an offline entry
  saved by IP moves to the one host a search finds with its protocol and a similar name (same letters
  and digits, any case; none saved there yet; `NasList::migrate`, run as every search ends, the one at
  start included), keeping its login; online = a TCP
  connect to its port each probe round (`nas::reachable`: the name looked up within 3 s and every address tried
  at once, so one NAS holds a round up for at most 6 s; ponytail: a lookup past that reads offline and none
  starts again for that host + port while it runs; an NFSv3 server reached only through its portmapper reads
  offline unless its nfsd port is saved). The Devices list shows online devices first, peers before NAS devices, at most 9 in
  all, each with a kind tag (Forge, SMB, FTP, NFS), its facts (a peer's platform and address, a NAS's
  `host[:port]` and user or Guest, none for NFS) and **Online**, else "Offline · seen: 5 hours ago"
  (relative, the largest whole unit; a peer's `last_seen`, a NAS's `last_online`, memory
  only, cleared when its address changes);
  routes `nas_save {id?, name, host, port?, user?, password?, protocol?}` → `{id}`, `nas_forget {id}`, the
  snapshot's `nas`. **Games on a NAS, and outputs onto it** (SMB by user choice over WebDAV; writes by user
  decision): a saved NAS is a game source and an output, served like a peer's files (core `remote.rs`
  through `smb.rs`, `ftp.rs`, `nfs.rs`; streamed, never copied first): `smb://<host>[:<port>]/<share>/<path>`,
  `ftp://<host>[:<port>]/<path>`, `nfs://<host>[:<port>]/<server path>` (the port only when not the
  protocol's). Read in Convert, Edit (browse and Extract only, muted `edit.nasNote`, no Delete), inspect,
  covers, transfers and `list_dir` (`smb://<host>` lists its disk shares, `nfs://<host>` what is above its
  exports); folders and `.exfat`/`.ffpkg`/`.ffpfs`/`.ffpfsc` read in place (each read re-checks the file's
  size + mtime, the `SourceStamp`; no identity: ponytail). Written by Convert (an image as a `.part`
  there, read back over the network to verify, renamed no-replace; a folder or LZ4/zstd pack folder file by file
  through exclusive temporary names; an fpkg streamed as a `.part`), Extract and transfers (no rollback, as
  onto a peer); names going into any remote folder are checked as case-folding (a case-sensitive server
  refuses `A/x` beside `a/y` too: the safe direction). An FTP output that an fpkg or LZ4/zstd packs in a folder need must pass `ftp::can_rewrite` (a
  `REST` + `STOR` probe in the output folder; else "this FTP server can't rewrite part of a file");
  ponytail: FTP has no sync and tells no free space (a full disk fails the upload), its rename is check,
  then `RNTO`. `nas.rs` `NasList::publish` hands SMB logins to `core::smb::set_credentials` and FTP ones to
  `core::ftp::set_logins` (at start, on save and forget), never logged; NFS calls as AUTH_SYS uid 0
  (ponytail: a `root_squash` export maps it to the anonymous user). Refused there: a `.pkg` or an archive
  as a source ("copy it to this device first"), an output beside a game there (choose an output folder),
  edits, `host_tree`, Edit's Add and `host_tree` from a NAS (SMB, NFS and FTP: only a `flink://` path
  is added from another device; the UI's Add lists peers only), delete and move ("a NAS share is read only here"; FTP: "on another
  device"). The File explorer and the Library list no NAS; the File explorer lists a paired PS5 on a
  computer (its device tabs, above), the Library no peer. Test servers: Docker only (above,
  `tests/`), core `tests/smb.rs`/`tests/nfs.rs` gated by `FORGE_SMB_TEST`/`FORGE_NFS_TEST`, `tests/ftp.rs`
  on the crate's fake server.
  **Pickers** (by user decision; `Picker.tsx`, every source and output picker but the Library's, the File
  explorer's): while a paired peer or saved NAS is online, one modal with a tab per
  device, this device first, then each online peer (`flink://<key_id>`) and each online saved NAS
  (`smb://`, `ftp://`, `nfs://<host>[:port]`, `nasRoot`), for sources and outputs alike; Edit's Add lists
  peers only (`peersOnly`). No system dialog on any platform (by user decision; `tauri-plugin-dialog`
  removed): the desktop's local tab lists Tauri `local_roots` (home and the volume folders, Windows C: to Z:,
  that open and that `list_dir` doesn't refuse; existence only, one thread each, 1.5 s for all; ponytail: a
  stalled root's thread is left running), a root listed only when opened. A source is one **[disc] Pick** button (by user request; Convert, Edit, Pack) in the
  `folderToo` mode: a file click picks an image or archive, "Choose this folder" a game folder, told apart
  by extension. Rows: at most 2,000 shown, one Tab stop, arrows move; multi-select's button reads
  "Choose N files"; a picker starts where the last browse ended (kept until the page closes).
  Nothing online: the same picker without tabs (the web build with each saved NAS as a root, outputs too
  (ponytail: an offline one included); the desktop app has no NAS source button (removed by user request): a
  NAS game is picked on its tab while the NAS is online).
  **A peer's files** (by user decision: streamed, never copied first): `flink://<key_id>/<path>`, relative to the
  peer's share root (`/`; a Windows PC its home folder's drive, by user request; on the PS5, by user request,
  only `/data` and mounted drives within it: `/` lists `data`, and `mnt` while a drive is mounted), read in place like a NAS (core `remote.rs`, `net.rs`;
  `connect_fs::ConnectRemote` registered with `core::remote::set` by `Connect::start`, server and Tauri alike):
  Convert, inspect, covers, `list_dir` (`flink://<key_id>` is the share root), Edit's browse, Extract and
  Add, transfers. Outputs may be on a peer too: an image as a `.part` there (`RemotePart`), verified by reading
  it back over FLINK, renamed no-replace; a folder (Convert, LZ4 pack, Extract, a transfer) written in place
  on the peer, each file through an exclusive temporary name (ponytail: no rollback, the error says what
  stays); an fpkg as a `.part` there too (by user decision, streamed: vendor patch 0025's `PkgOut`, core
  `RemotePkgOut`; the same onto a NAS; its clones read ahead for the self-check's and verify's small reads:
  up to 4 windows per clone, 256 KiB doubling to 4 MiB as a read continues one, least recently used
  dropped, every window dropped by any write, a read over 4 MiB served and not kept; over FTP each read is
  a data connection, and a 10k-file package's verify took ~500 s without them; ponytail: windows per clone,
  one lane at a time). Nothing of it is stored on this device's disk (by user decision):
  the builder's Kraken spool is the output itself (`KrakenSpool::InPlace`; a production request ignores the
  spool-folder variable), and vendor patch 0026 digests each compressed block (SHA3 + CRC32C) in memory as
  it is written, so no byte is read back before the self-check; then the builder's self-check (fast:
  samples; full: the block sweep and the `playgo-chunk.crc` pass, each a whole read; ponytail: one fused pass
  would be a vendor patch) and the job's verify read it over the network (pinned by core's
  `an_fpkg_on_a_peer_keeps_nothing_here_and_is_read_back_only_to_check_it`).
  Refused for a peer path: `.pkg`/archive
  sources ("copy it to this device first") and editing a peer's game. Delete and move of a peer path (by
  user request, the File explorer's device tabs): core's `Jobs::delete_path`/`move_path` refuse it while one
  of this device's unfinished jobs uses it, a move across devices ("a move stays on one device") or between
  here and a peer (a transfer), then hand it to the peer (`Remote::delete_path`/`move_path`, the
  `delete_path`/`move_path` RPCs, each on a control session of its own (`ConnectRemote::alone`), never the
  device's shared one, so a long delete doesn't hold up a transfer's per-file calls; listings (`fs_list`) stay
  on the shared cached session; ponytail: under the job-table lock, so a slow peer holds off this device's jobs;
  no retry: a link that breaks mid-delete fails the request, the peer may have finished), which
  confines both paths as every RPC's, refuses its share root itself, and runs its own `Jobs::delete_path`/
  `move_path` with its protected paths (`Connect::set_delete_move`: the server's keep list, the app's home
  folder; refused until set, and while it stops or quits; on the server admitted as the local routes are,
  `admit_delete()`, counted in `deleting`, no admission held through it); the error's kind comes back by its lead
  (`refused: `, `busy: `, not found); an older peer answers `unknown method` ("that device's Dump Forge is
  older: update it ..."). Tauri's `delete_path`/`move_path` skip `folder_refusal` for a network path (the peer
  judges it; a NAS is refused by core). Peer side (`connect_fs.rs`): RPCs `stat`, `fs_list`, `read_open`/`read_close`, `mkdir`, `create`,
  `sync`, `close`, `rename` (`replace: true`: over a regular file only, never a folder, link or special
  file, nor another peer's unrenamed file, and refused as this device's own replace is
  (`Connect::set_replace_check(jobs.replace_check)`: jobs, protected paths, ShadowMount+, an undo journal);
  an older peer ignores it and refuses the taken name), `remove`, `free` (on the PS5 where its folder
  leads must be reachable too), `delete_path`/`move_path` (above), `install` (Install, above), every path confined to the share root (`join_checked`, the
  parent canonicalized inside it, then on the PS5 `Roots::reach`: `/data` or a mounted drive, `/` and `/mnt`
  for stat and list only; all in `locate`), links never followed, special files and non-UTF-8 names marked so the
  reader refuses them); writes only to files this session created (exclusive, then a no-replace rename, or with
  `replace` one over a file),
  unrenamed ones kept 60 s (`KEPT_FOR`) once the session ends, then removed (at once when the service stops
  or the pairing is forgotten here); `reattach {path}` → `{handle, size, open}` moves such a file (kept, or
  still held by another session) to the calling session, for the pairing that created it only, once its
  writes in flight end (a per-file gate; the old session's later writes are refused); a path its own unfinished jobs use is refused ("in use by a
  job on <name>"; `Connect::set_busy(jobs.uses_hook())`), so is (PS5) one at or inside `/mnt/shadowmnt`
  (`core::out::refusal`; ponytail: no destination probe per peer write), and its jobs refuse a path a peer is writing
  (`Jobs::set_external_busy(connect.writing_hook())`); Delete and Move refuse that path too (a move's
  source and destination). This side keeps one control session per device and
  up to 8 data lanes each for reads and writes (8 MiB chunks, each BLAKE3-checked; a write sends 64 MiB, a
  sequential read fetches 64 MiB ahead (`RemoteReader`, `RemoteTree`), each dealt to the lanes; one session
  and lane retry). Each read re-checks size + mtime (no identity: ponytail). Small files (≤ 8 MiB, `PUT_MAX`)
  take one lane round trip each way: `Remote::read_checked` (a lane `read {path}`, the stat after the read in
  `DataHead.stat`) and `Remote::put_new` (a lane `put {tmp, path}`: created, written, synced, renamed; the
  writer holds a file in memory until it passes 8 MiB); a 10k-file game moved ~2.3x faster on the console.
  The peer's RPCs are answered as soon as they finish (no polling).
  Transfers survive a stall (by user request, after a ~30 GB transfer between a PC and the PS5 stopped mid-way): while a
  lane's read, write or `put` works the disk, the peer sends a `Ping` on the lane every `PING_EVERY`
  (`pinging`; the lane reader skips them), so a disk stall past FLINK's 12 s silence limit doesn't end the
  lane. A created file whose session is lost resumes (`ConnectRemote::resuming`): a fresh session
  `reattach`es it and the write, sync, close or rename runs again there (a write from its offset;
  positional, so a piece sent twice is the same bytes, and BLAKE3 is computed as written; a reattached file
  shorter than the writes acknowledged is removed and the loss stands), at most 3 times
  (`RESUMES`) within 60 s, each logged to the serve log ("connect: reconnected to <device>, resuming /<path>
  at <offset>"). A `put` is asked once more, whole, under a fresh temporary name, on a checked (else fresh) session; a `to` already
  holding exactly those bytes is the first one landed. An older peer sends no lane pings and answers
  `reattach` `unknown method`: the loss fails the job as before. peers.json is written outside the `peers`
  lock, which every lane chunk and RPC takes (`save_peers`: one save at a time, never an older snapshot over
  a newer; a pairing or Forget is saved under it before it counts; a failed address or last-seen save is
  written by the next handshake). While a paired device reads or writes files here (`Connect::peers_active`:
  a read open, or a file it created and hasn't renamed, kept ones included) the PS5 holds off auto rest
  mode as during its own jobs (core `awake()`, a process-wide count held by jobs, Connect, the browser's
  uploads and downloads: `sceSystemServicePowerTick` at once, then every 30 s),
  `quit {"if_idle": true}` answers 409 "Dump Forge is busy: a paired device is reading or writing files
  here", and the desktop app's close asks first as for its own jobs (`QuitHeld`, `peerBusy`).
  **Speed test** (user request; by user decision nothing written to disk, both directions, on the data lanes
  as transfers): `Connect::speed_test(key_id)` -> `SpeedTest {up_bytes_per_s, down_bytes_per_s, bytes}`: the
  `speed` RPC first (an older peer answers `unknown method`: `SPEED_OLDER`, "that device's Dump Forge is older:
  update it to run a speed test"), then `SPEED_BYTES` (128 MiB) sent and then fetched in 8 MiB chunks dealt
  round robin to 8 lanes (`speed_up {offset, len}` + `Data`, BLAKE3-checked and dropped, answered `DataOk`;
  `speed_down {offset, len}` answered by a `Data` of zeros), each direction timed from its first request to
  its last answer (lanes joined before the clock starts). The serving side (`serve_speed`): a speed lane takes
  only `speed_up`/`speed_down` (else refused), `len` 1..=`CHUNK` (0 is "bad speed test"); a paired device's
  lanes together at most `SPEED_PEER_LANES` (8) and `SPEED_BUDGET` (2 x `SPEED_BYTES`, 256 MiB, both
  directions) at once, reserved per request in `SpeedLane` (`SPEED_OPEN`, by key id) and released as each
  lane ends; past either a refusal ("at most 8 speed lanes at once", "a device's speed tests carry at most N
  bytes at once"). `SPEED_MAX` (the `speed` RPC's `max`, the client's clamp) is `SPEED_BYTES` per direction.
  Both sides share one static `CHUNK` of zeros and its hash (`speed_zeros`).
  Holds `awake()` on the testing side (ponytail: the peer's lanes don't count as `peers_active`). Routes:
  Tauri `connect_speed_test {keyId, run}`, server `POST /api/connect/speed_test {key_id, run}` (409 while
  stopping, 400 with the error text); transport `connectSpeedTest`, `connect.speedTest(keyId, run)`.
  Cancel (by user request), by run id: the UI makes one id per test (`<count>-<random>`; no
  `crypto.randomUUID` on a plain-HTTP page) and sends it with the test and its cancel (server `POST
  /api/connect/speed_test_cancel {key_id, run}` -> `null`, Tauri `connect_speed_test_cancel {keyId, run}`,
  `connect.speedTestCancel`). `Connect.speed` (`SpeedRuns`): running tests by (key id, run) ->
  `connect_fs::SpeedCancel` (the flag + a clone of each lane's socket); a cancel shuts those sockets, so a
  lane blocked on its answer ends at once, and every lane checks the flag before each chunk; the test ends
  `SPEED_CANCELLED` ("cancelled"). A cancel for a run not started yet is remembered (the last 16, for 60 s,
  used up by that run, which then ends `cancelled` at once); a cancel never touches another run. Stop ends the
  setup too: the test dials a control connection of its own (`session_with(key, Some(&cancel))`, never the
  shared cached session), its socket in `SpeedCancel` from the TCP connect on, so a cancel shuts the handshake
  and the `speed` RPC, and no further address is tried (ponytail: the 3 s TCP connect itself isn't cut).
  **Jobs mirrored** (`connect_jobs.rs`): a job using a paired peer's files (any `flink://<key_id>` among its
  source, output, transfer or added files) is pushed to that peer every 2 s (`jobs` RPC, each
  peer on its own thread, with the run's `instance`: job ids restart at 1, so another run's cancel marks go; this
  device's paths spelled `flink://`, the peer's own as its local paths; kept 30 s after it ends, dropped there after
  10 s without a push or on Forget) and shown there read only (`snapshot`'s `remote_jobs`): its user may only cancel
  it (`connect/cancel_job`, Tauri `connect_cancel_job`), answered in the next push's reply.
  **File Transfer** (by user decision, a modal; a device's **Send** opens it with the destination picker
  at that device's root): **What to send** picks sources (Choose files / Choose a folder, any device: this
  one, a peer or a NAS; a whole device, share or drive refused) and **Send to** a destination folder (this
  device, a peer or a NAS; its button is off, with `connect.sendNeedsDevice`, while no peer or NAS is
  online), each side with a device switch while one is online (switching drops that side's choice, so
  does its device going offline), and Start queues a **transfer job** (the dialog closes)
  (`ConvertRequest.transfer`, core `transfer.rs`; peer to peer relayed through this device; stages scan,
  preflight, write, verify as Extract; Start asks `existing_targets` first and a name there is replaced
  only once the user confirms (Replace, above); a nameless root refused; no rollback: "N of M files were copied before
  it and stay", or, with none finished, "<folder> was created and stays"; a peer's link is never read). Transfers (and installs) run in a second slot beside the one heavy job (convert, edit, pack, extract): at
  most one of each, each FIFO; a job whose paths are at, in or above an unfinished job's of the other kind
  is refused at start ("<path> is in use by another job", or "is being written by another device"). The
  old send/fetch/transfers/cancel/remote_list (routes, Tauri commands, the list) are gone (by user
  decision). Chunks of 8 MiB dealt round robin to 8 lanes on every platform (by user decision, as
  ps5upload; one big file uses them all), the PS5's sockets (FLINK and HTTP) at the largest buffers its
  kernel takes (`flink::tune_buffers`); a lost session resumes the file (above), nothing more: no resume after
  those 3 tries or 60 s, a restart or a Forget (ponytail; Retry starts the job again). Phones: the Connect tab's QR opens
  `<url>/#connect`; the web build's **Upload to this device** / **Download from this device** move the
  browser's files (disabled in the PS5's own browser, which can't pick files): files one at a time into a
  folder picked on this device (progress, Cancel; Close, Escape and a click outside don't cut it; a failure
  or Cancel says "N of M files were uploaded and stay on this device"; a file already there replaced
  only once the user confirms, `replace=1` on every chunk, put in place through `Jobs::move_path` with
  replace: Replace, above), a file of this device saved by the
  browser. Uploads go in 8 MiB chunks, 4 requests at once in a browser (by user decision; FLINK between devices keeps its 8 lanes; a browser runs about 6 per host and the page's polls need room; `connectUpload` in
  `http-client.ts`, `id=&offset=&size=` on `POST /api/connect/upload`, assembled by offset in one `.part`):
  35 MB/s to the PS5 where one request does 28. Both phone upload modes refuse a destination at or
  inside `/mnt/shadowmnt` (aliases resolved) before reading the body or creating a file. Over plain
  HTTP, not FLINK (a browser can't open TCP).
- Connect UI (by user decision, the "Device hub" redesign): two columns
  from 980px. Left: the This device card (name, platform, the page's 220px QR code in the web build, no
  caption (hidden under 600px), the IP tile: the session `url`'s host on the
  web, `ThisDevice.ip` in the app, else "Unknown"; no notes (by user request); a
  plain `.small` **How to connect** in its head, as How pack works and Convert's Compare formats), then (by user
  request, the desktop app only; not the PS5's page) **Dump Forge on PS5** (a plain card, no pitch): a PS5
  address (a saved PS5's IP prefilled, port 9021 by default) and **Install on PS5**, one click: `update.ts`
  `ps5Release` asks GitHub for this app's version's release carrying `ps5-dump-forge-<v>-ps5.elf`, else the latest
  release's, and Tauri `connect_payload_install {address, version}` downloads that ELF into memory on the PC and
  streams it to elfldr, which starts it (`ps5_dump_forge_updater::install`, no spill; a Forge ELF started with no
  arguments runs `serve`, unless a developer `args` file is pending; elfldr probed before the download; the PC needs GitHub, the console no internet); **Send a payload** picks any ELF on this
  computer (ELF magic, at most 64 MiB) and streams it to the same address (`connect_payload_send {address, path}`,
  `ps5_dump_forge_updater::send`; elfldr answers nothing, so a payload
  that fails to start isn't seen). Needs an elfldr that listens on the network (the webkit autoloader's is
  loopback-only). How to connect opens with a highlighted box: its title and a short paragraph on the GitHub download. Then Add a device (one address box + Connect: it probes the address
  first, `connect.probe`, "Checking <address>" meanwhile, a Forge pairs and the waiting dialog shows the
  check number, a NAS opens its Add dialog prefilled; a request that fails keeps the typed address; then
  "Or add by protocol:" SMB / FTP / NFS). Right: Devices (in its head File Transfer, primary while a
  device is saved, and then Autodiscovery too; an All / Forge / NAS filter while both kinds are saved,
  "N of M online"; rows with Send for an online device and ⋯ for its details; empty: a panel with a
  primary Autodiscovery) and Nearby under it (Autodiscovery inline, no dialog: the finds not saved yet
  with Pair or Save, "Searching this network" and Stop while it runs; "No new devices found"). Details:
  a Forge's address, key ID, last seen and status with File Transfer (online only), Forget and Close; a
  NAS's address, user and password saved or not (not NFS) and status with Edit, Forget and Close; either
  closes once the device is forgotten elsewhere. **Speed test** (by user request; bolt icon): an online
  paired Forge's row (by user request an item of its ⋯ menu, `ChooseMenu`, after Details, which is always first; a NAS row's
  menu holds Details only) and its details (a button before File Transfer); no NAS. One test at a time (a module
  store in `Connect.tsx`, `runSpeedTest`/`useSpeed`): the running button or menu item reads "Testing"
  with `Dots`, `aria-disabled` + `aria-busy` (focus kept), every other Speed test button and item off. The result stays in memory
  until the page closes, under the row (`.dev-sub`) and in the details (`.note-line.good`): "Upload N MB/s ·
  Download M MB/s" (decimal MB, `num`, one decimal under 10); a failure an error line led by "Speed test
  failed:" (`connectError`, `peers.ts` `readable`). A test runs in a modal (by user request; `SpeedDialog`,
  opened by `runSpeedTest(keyId, name)`): "Speed test with <device>", "Testing" with `Dots`, the muted
  `connect.speedNote`, Stop focused; the cancel is the overlay's `onClosed`, so every close while it runs (Stop,
  Escape, a click outside, the quit prompt's `closeOverlays`) stops it and keeps no result or error
  (`speedStopped`); once it ends the dialog shows the result line or the error with Close. How to connect has
  one paragraph on it (`connect.helpSpeed`). "Loading" until the service first answers; a failed poll
  keeps the last good state (the pairing prompt too, at most its 60 s); with none, "Connect isn't
  available: <error>". The header's indicator, on every
  platform (it replaced the address button and the How to connect modal): grey **Disconnected** or green
  **Connected (N devices)** (online peers + online saved NAS); a click opens the Connect tab.
  Other devices by name (by user request): a source card and the Output label carry a green tag with the
  device (paired Forge, or a NAS's saved name) when it isn't this one (Output says **This device** while
  any device is saved), and every `flink://<key_id>` (and `smb://`, `ftp://`, `nfs://` address of a saved
  NAS) the UI shows, paths and backend text alike (errors, job errors, checks, logs: `peers.ts` `readable`;
  `nasName` for the tags), reads "<device name>: /<path>".
- Jobs in the UI (by user decision): Convert, Edit, Pack, Storage and Connect each list only their own jobs
  (`JobsCard`, inside the tab's panel, kept mounted; not About), by type (`jobType`: install (Storage; a peer's,
  installing this device's package, Connect: `App.tsx` `tabOf`), transfer (Connect), edit for edits and Extract,
  pack for any LZ4 or zstd pack or unpack (`packJob`), else convert). A row is a tile in the app's black for every type (`--inset`, by user decision; no
  edge ring) and its type's tag: Edit blue (`--edit-*`), Pack yellow (`.tag.lz4`), Transfer and Install green (`--transfer-*`,
  "Can't transfer:", "Can't install:"). Each of those tabs counts its type's unfinished (queued + running) jobs in its label,
  after the name (`.count`, white on every tab, by user decision, hidden from screen readers, which read
  "N active"; Connect counts transfers and a peer's installs, Storage installs). A failed job's row has **Retry** (by user request; `JobList.tsx`
  `canRetry`): it starts the same request again as a new job; not for a cancelled one, a peer's mirrored job
  or an edit (Edit's Apply keeps the changes after a failure); an Extract or a transfer asks about names
  already there again first (`askReplace`, Replace above), since a failed copy leaves files behind.
  `App.tsx` renders the tabs memoized (`ConvertScreen`, ..., `ConnectScreen`) with stable handlers;
  Convert/Edit/Pack get `useSettled(jobs)`, the list kept by identity until a job's id, request or result
  changes, so progress and log events re-render only the job lists (`JobCard` is `memo`).
- Names are never renamed: bad names (non-NFC, exFAT/PFS case collisions, forbidden chars, non-ASCII
  in PFS) fail preflight with every offender listed.
- `.pkg` is built with `BuildRequest::production` only (plaintext `PPRPLAIN-NOAUTH!`, Kraken);
  installing needs kstuff + fpkg-enable + ppr-patch. Debug keys are embedded.
  The builder writes a fresh `ampr_emu.index` from the packaged files (a libSceAmpr title or a source
  with one; a stale index crashes the game; a case clash refuses), except for a packed source, whose own
  index is kept byte for byte: LZ4's runtime checks each record against the pack manifest by file id, and a
  zstd-packed source's index lists files that are inside its packs (vendor patch 0030).
  So a plain `.pkg` of an unpacked AMPR title builds no index of its own first (`lz4::plain`): the old index,
  journal and logs are left out and the builder indexes what survives `sdk_rules::excluded`.
  A source without `eboot.bin` is built, not refused (vendor patch 0023; its content class: above).
- The UI names formats without the dot (`ffpkg`, `exfat`, `ffpfs`, `ffpfsc`, `fpkg`; `KIND_LABEL` in
  `common.tsx`), and in running text in bold (`Prose`); paths and file names keep the extension. The debug
  FPKG is labelled `fpkg`; its files, the `Format`/`Kind` id and the CLI flag stay `pkg`. Format copy and
  the About tab's formats table (`app/src/Formats.tsx`) follow ShadowMountPlus 1.7's README and release notes.
  About (by user decision): the About card (the language select in its head, no introduction line; on the
  PS5 web build, a saved job-end notification switch below the features) holds
  the feature list (`formats.features`: bold name + a few words, the user's text) as one grid of nine
  tiles in three sets: Convert, Transfer, Edit / Streamed, Archives, USB and SD cards / Pack (Forge or LZ4,
  "with smart healing"), Storage (Library + File explorer), Safe; `Formats.tsx` maps the icons by position, so every
  language keeps the nine in that order (typed as nine, `Features`: tsc holds every language to it). Then
  Compare formats (a row per format in its hue: fpkg, zstd packs, LZ4 packed folder, ffpkg, exfat, ffpfs, ffpfsc,
  folder; each rating a `.chip` with an icon, never colour alone), then Credits on a card of its own. The page's QR code lives on the Connect tab (web build).
- UI languages: English (the fallback), Russian, Simplified Chinese (`zh`), German, Spanish, French,
  Japanese, Brazilian Portuguese (`pt`), Italian, Korean and Arabic; UI text only (`app/src/i18n.ts`). Backend text
  (errors, findings, job logs, checks, `describe`), the CLI, `entry.c`'s notices, format ids, file names, product
  names and IEC units stay English; code that matches backend English keeps doing so. Messages live in
  `app/src/locales/<lang>.tsx`, one file per language, grouped by the file that shows them
  (`lz4`, `convert`, ...): `en.tsx` is the source and defines `Messages`, every other one is typed
  `Messages` (tsc fails on a missing key). A new language: copy `en.tsx`, translate, add one line to
  `LANGUAGES` in `i18n.ts` (`tag` where `<html lang>` needs more: `zh-Hans`, `pt-BR`). Each language is its own Vite chunk inside the bundle (embedded in the
  `.elf`, listed in the AppCache manifest; nothing is downloaded), loaded by `initLang` before the
  first render (both entries) and by `setLang` before a switch shows it (one that fails to load leaves
  the language, and the selector, as they were). Components use
  `useMessages().<group>`, other code `msg().<group>` at call time (never at module load); plurals via
  `plural`, numbers via `num`; a sentence is one message per language, never spliced from fragments;
  code compares ids, not displayed text. `http-client.ts`/`launcher.ts` stay free of runtime imports
  (plain-node checks) and return codes the UI translates. The system languages pick the language
  (exact tag, then its primary subtag) until the About tab's selector (System + each language, every
  platform) does. Game titles come from param.json's `localizedParameters` (`title_names` in
  `Inspection`/`LibraryEntry`; `gameTitle`): the chosen language, else the system languages, else
  English, else `title_name` (the default language, still used for generated names and sorting).
  Every UI copy change updates every language; the UI says "game", never "dump" (except the product
  names). Russian writes trace terms as трейс/трейсы/трейсинг. Arabic is right-to-left (`rtl` in
  `LANGUAGES`: `<html dir="rtl">`; styles.css uses logical properties plus a few `[dir="rtl"]` overrides,
  paths stay LTR) with Latin digits (`numbers`), and its Latin file names and paths are wrapped in
  LRI…PDI (U+2066/U+2069) so the bidi algorithm can't reorder them. Durations are whole messages per
  language (`duration`). The server's PS5 notifications (started, port in use, stopped, can't
  close (jobs running), not taken over, a Convert, Pack or Transfer job done or failed (switchable on
  About); `crates/ps5-dump-forge-server/src/notice.rs`; by user decision a job's end is a system toast
  (`sceNotificationSend`, ShadowMountPlus's JSON shape, Forge's icon at
  `/user/data/dumpforge/notify-icon.png`), the plain notice its fallback, every other notice stays plain)
  follow the console's language
  (`sceSystemServiceParamGetInt` LANG, mapped to the UI's languages, else English); English elsewhere.
- No settings and no config file: the UI keeps choices in memory until it closes (Connect's pairings,
  above, are the service's own state), except the UI language (`forge.lang` in `localStorage`; unreadable
  storage falls back to the system languages) and the PS5 About notification switch (kept by the server
  on the console as `notifications.off`; by user decision). Defaults: Convert's target
  fpkg (`.pkg`), exfat for a `.pkg` source (by user decision; picker order Folder, fpkg, exfat, ffpkg, ffpfsc,
  ffpfs), exfat inside a `.ffpfsc` in the UI (by user decision; core and CLI keep ffpkg; under the inner picker
  exfat shows `target.innerExfat`, Recommended, and ffpkg `innerFfpkg`), at zlib
  level 6, all cores for compression (`serve`: cores − 1; PS5: its 5 CPUs − 2), `.pkg` Kraken level `fast` (also `balanced`,
  `smallest`, and `stored`: every block raw, the fallback for a console that won't start a compressed
  package; tests use `fast` only), no extra free space beyond each format's built-in spare, output next
  to the source (an empty output `dir` is the source's folder, never the process's working directory).
- Progress is one bar per job: `Event::Progress` `done`/`total` span every pass (write, verify,
  ...); `Ctx::expect_rest` renews the estimate as each pass learns its size. No pass fills the bar while
  another is left: LZ4 measuring and auto-loose sampling count the write and check passes after them,
  and `Ctx::reserve` holds back a packed image's check through its packs while it is checked as written.
  The UI keeps the bar under 100% until `Done` is `Ok`. Each job first emits `Event::Stages`
  (`{"kind":"stages","job":N,"stages":[...]}`, from `convert::stages(req)`: the `Progress` stage ids
  in order; Tauri `job://stages`, server `GET /api/jobs` `jobs[].stages`, `[]` until known); core's
  test helper `run` holds every job to it (reported stages only move forward through the list).
- The Tauri app has no HTTP sidecar and no `tauri-plugin-fs`; the capability grants only the app's
  commands and event listening (no dialog plugin: every picker is `Picker.tsx`). The web UI is a separate build served by `serve`.
- Verification: fast by default everywhere (every structural check + seeded BLAKE3 sample of 8 MiB
  slices), full is opt-in (`full_verify`, `--full-verify`, the "Full verification" switch). A fast `.pkg`
  also samples the builder's own block sweep and `playgo-chunk.crc` check (vendor patch 0012, the job's seed).
- Updates (by user decision): every start checks GitHub, always on, no setting, nothing stored (`localStorage`
  keeps only `forge.lang`); **Later** hides the toast for the session. `app/src/update.ts` (no runtime imports;
  `scripts/check-update.mjs`): `GET api.github.com/repos/quer3q/ps5-dump-forge/releases/latest` (10 s, no
  cookies, any error = no toast), `tag_name` `vX.Y.Z` newer than the running version (a prerelease is older than
  its release) and the release lists this build's exact asset (`ps5-dump-forge-<v>-<asset>`, `state`
  `uploaded`, `size` > 0). The toast shows only after a successful check that found an installable update; any
  failure (offline, DNS, HTTP, JSON, timeout) is silent: no toast, no error, no offline state. `UpdateToast.tsx`
  (bottom-right, bottom-left in RTL, full width under 600px; a hidden always-mounted `role="status"` holds
  its title; once per start; Later / Update; a failed update says why with Close / Retry; the app's, once
  the browser opens the download, says so with Close). Transport `updateTarget()` (`{version, asset}`, null = no
  updates) and `startUpdate(v)`. Desktop: Tauri `update_target` (asset by OS/arch, `-webview2` when a `WebView2`
  folder sits beside the exe) and `open_update {version}` (plain `X.Y.Z`; opens the release archive's URL in the
  browser via `open_url`, no self-replace; CSP `connect-src` has `https://api.github.com`). PS5 (`ps5.elf`, only
  while `GET /api/elfldr` says `{"up": true}`: a 1 s connect to elfldr, closed at once): `POST
  /api/self_update {version}` has the server download that release's ELF over HTTPS
  (`ps5_dump_forge_updater::install`; HTTPS for this download only, by user decision: elfldr can't follow GitHub's
  302) into memory, or, when that much memory can't be reserved, over the self copy
  `/data/dumpforge/ps5-dump-forge.elf` first (temp + sync + rename under its flock), and stream it to elfldr
  (`Options.elfldr`, `127.0.0.1:9021`; elfldr's `?uri=` is not used); the page then polls `GET /api/session` for a new `instance` (1 s; 1 min in all from the request, download included) and
  reloads through AppCache; the old server never stops itself. Any newly loaded copy, of any version (by user
  decision: older and the same too), that finds Forge on the port closes it when idle and takes over
  (handover: `quit {"if_idle": true}` for one with session `handover: true`; for v0.1.1 and older, a `GET
  /api/jobs` with every job `done` first, then a plain `quit`; a running job, or (`if_idle`) a paired device
  reading or writing files here, always refuses: "Can't close Dump Forge - some jobs are still running."; details in `ps5/README.md`). `serve` on a computer (`host`): no update check.
- PS5 payload: one plain `.elf`, no PKG, no signing (updates: above, or replace the file). FreeBSD 11 ABI via
  `--cfg libc_unstable_freebsd_version="11"` (the env var is ignored by libc ≥ 0.2.187) and struct-size
  asserts in the CLI. `panic=abort`.
- The server has **no protections** by user decision: no pairing, token, Host/Origin/CSRF checks or path
  confinement (except, by user request, on the PS5: listings, `host_tree`, a Library folder and the peer
  side reach only `/data` and mounted drives). Don't re-add them (Connect's pairing guards FLINK on 9883 only, not the HTTP routes,
  `/api/connect/*` included). Its delete and move routes (`delete_path`, Edit's and Storage's;
  `move_path`, the File explorer's; by user decision) refuse only what exception 1 lists (`edit_discard` removes a staging folder by the same rules); the
  page's question is UI, not authentication. `edit_upload` writes only under a source's staging folder.
- The PS5 version is named "Dump Forge" (tile, notices, server texts, the web build's UI: `APP_NAME` in
  `api.ts`, the http page title); the desktop app is "PS5 Dump Forge". The CLI, crates,
  `ps5-dump-forge.elf` and release names keep `ps5-dump-forge`. Home folder `/data/dumpforge`
  (`FORGE_DIR`); `entry.c` moves an old `/data/ps5-dump-forge` there once and leaves a relative link at
  the old path (the Library walk doesn't follow it; details in `ps5/README.md`).
- Tile `PDFG00001` "Dump Forge" (deeplink to `http://127.0.0.1:<port>/`), never written without our owner
  file `/data/dumpforge/tile-PDFG00001.owner` (the old-name owner record still counts; see `ps5/README.md`).
  Self copy: `ps5/build.sh` builds twice; the release ELF embeds stage 1 and saves it to
  `/data/dumpforge/ps5-dump-forge.elf`. Launch on open: the AppCache'd page asks elfldr
  (`GET :9021/<elf>?args=serve`; that copy, then the old path, then USB) only from the console's own
  browser at load.
  On the PS5 the start notice only says "Dump Forge is launched" (no URL; the serve log has it); the Connect
  tab shows the server's host from `GET /api/session` `url`, never the viewer's `location`, and its QR
  code (`GET /api/qr?connect=1`, `<url>/#connect`: an SVG drawn by the server with the `qrcode` crate; 404
  when the server can't tell its IP). The desktop app shows no QR.
  L2 / R2 (PS5 only, session `platform` `ps5`): badges beside the tab bar and the pad's triggers
  step through the tabs, wrapping, from any focus, not under a dialog (`shoulders.ts`). The PS5
  browser keeps L1/R1 (back/forward), has no Gamepad API for pages and sends L2/R2 as `F7`/`F8`
  keydowns (`STEPS`), found with a debug build's `GET /probe` (every input event, logged to
  `GET /api/debug_input`; the page's **Input probe** button shows when the session says `debug`).
- PS5 writes: a retried fsync fails the job; the destination probe decides what a drive supports (never
  the path or fs name) and fails closed. Debug routes (`FORGE_DEBUG_API`) never ship; `release-ps5.sh`
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
  spare inodes (no UFS2Tool `-D` +10%); every inode 0777 root:root; the last cylinder group must hold its
  full metadata (never write past `fs_size`).
- Writable (`image_rw=`) mounts need free blocks/inodes (UFS) or free clusters (exFAT); `.ffpfsc` is always
  mounted read-only.
- PFS (`.ffpfs`): version 2, 64 KiB blocks (the console misreads smaller ones), 32-bit unsigned inodes,
  case-insensitive (mode 0x8), unencrypted; byte-identical to `mkpfs pack folder --raw --no-compress
  --version PS5 --inode-bits 32 --block-size 65536` except that empty directories are kept. Inodes: super-root,
  flat path table, collision resolver (only if two paths' FPT hashes collide), uroot, dirs, files; every node's
  data is contiguous. Files are stored uncompressed (per-file PFSC passes verify but the console misreads it).
  Names: ASCII only, no case collisions, every offender listed.
- `.ffpfsc`: a single-file PFS (MkPFS `pack file` layout) whose one file is a zlib PFSC container of 64 KiB
  blocks around a nested `.exfat`/`.ffpkg`/`.ffpfs` named `<TITLE_ID>.<ext>` (default `.ffpkg`, by user decision; the UI defaults to exfat;
  exFAT is MkPFS's most stable). Always PFSC, zlib level 0–9 as zlib numbers them (the slider, `--level`, `ffpfsc_level`;
  default 6; 0 stores every block raw); a block is kept compressed only if it saves ≥ 5%; a block with
  near-8-bit byte entropy that level 1 shrinks < 2% skips the level's search (stored raw). zlib from
  `flate2` (`miniz_oxide`). Streamed: the inner writer feeds the compressor, which writes the `.part`;
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
- In-place edits (each format crate's `edit.rs`; every overwritten byte journaled; within 128 MiB of
  metadata, else "rebuild it instead"; timestamps of renamed entries unchanged; exFAT and UFS2 write new
  data only into free space and reuse no cluster, block or inode freed within the same plan):
  - exFAT: any valid exFAT (one FAT, no TexFAT, boot and up-case checksums), not one marked VolumeDirty
    (refused in place: editing an unclean volume can compound damage); the plan sets VolumeDirty first and
    clears it last (PercentInUse updated). A new file takes one contiguous run when there is one (NoFatChain),
    else FAT-chained runs; a replace rewrites its entry set in place (name case, attributes and times kept);
    folders grow by clusters; an exact fit keeps the end-of-directory entry.
  - UFS2: only Forge's SMP geometry (the superblock and cylinder groups checked against the writer's layout,
    density 64..256 KiB); soft updates, SUJ, gjournal, metadata check hashes, an unclean volume (`fs_clean`,
    `FS_UNCLEAN`, `FS_NEEDSFSCK`) and a delete or replace of a file with nlink > 1 refused; other `fs_flags` bits (TRIM, ...) are
    accepted. `fs_clean` is cleared first and set last; new inodes 0777 root:root with the writer's times;
    a folder grows by 512 B chunks the kernel's way, up to single indirect (12 + 8192 blocks); a rename
    makes no missing parents.
  - PFS (`.ffpfs`): only the unsigned layout the writer and MkPFS make. It has no free space, so in place
    takes a replacement that fits the old file's blocks (written live, the old bytes journaled), deletes
    and renames/moves; a new path or folder is refused ("a .ffpfs keeps every file in one run: convert it
    to .exfat or .ffpkg to add files") and Rebuild is offered. Changed folders are re-encoded in the
    writer's order (folders first, then files, by lower-cased name) and the path tables rehashed; an
    emptied collision resolver is kept at size 0 (uroot keeps its inode; `PfsSource` accepts it); a rename
    that makes the first hash collision creates the resolver in a freed inode or a spare inode-table slot
    (a layout neither the writer nor MkPFS makes).
  - `.ffpfsc` and `.pkg`: rebuild only (read-only on the console; no editor).
- FPKG: outer PFS plaintext with the `PPRPLAIN-NOAUTH!` marker (native AES-XTS fails the console's auth),
  inner image Kraken-compressed (zlib metadata is rejected by the console, never offer it).
- LZ4 packs (ampr_emu 0.4.2.1 and ampr-emu-ng read them): index at `ampr_assets.index` (AMPRPAK4, little-endian, CRC32, FNV-1a path hashes)
  with `.crc` sidecar and `ampr_assets-NNN.pak` volumes (AMPRDAT3, volume cap 4 GiB - 64 KiB for FAT32); chunks LZ4 or RAW
  (a RAW chunk is exactly one I/O page); files the runtime cannot serve stay loose. CRCs detect
  corruption, they do not authenticate. The manifest's runtime-contract checks run on every open.
  Not an SMP image: ShadowMountPlus mounts none (no `.lz4` extension, no mount name limit beyond a folder's).
- zstd packs (`docs/PAK.md`): `ampr_zstd.index` (`AMPRZPK1`, version 1, 128-byte header, file, chunk and volume
  records, FNV-1a 64 path hashes, CRC-32 over header and tables) and `ampr_zstd-NNN.pak` volumes (`AMPRZDT1`, from 000; 64-byte header with
  the index's archive id, at most 4 GiB - 64 KiB, at most 4,096); 256 KiB chunks, each one zstd frame or raw
  when zstd doesn't shrink it. Only ampr-emu-ng (`vendor/ampr-emu-ng/libSceAmpr.sprx`, v0.1.0, `runtime::zstd()`) reads them. Not an SMP image, as LZ4 packs.

## Commands
```sh
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace --release
(cd app && npm ci && npm run tauri dev)                       # GUI in dev mode
cargo run -p ps5-dump-forge-cli -- convert <game_dir> --to exfat   # or ffpkg | ffpfs | ffpfsc | pkg | folder
cargo run -p ps5-dump-forge-cli -- convert <game_dir> --to lz4 [--lz4-level 0|3..12] [--lz4-runtime ng|ampr_emu]   # asset packs (folder out)
cargo run -p ps5-dump-forge-cli -- convert <packed_dir> --to folder --lz4-unpack   # back to the loose assets
cargo run -p ps5-dump-forge-cli -- convert <game_dir> --to ffpkg --lz4-pack [--lz4-level N]   # pack straight into an image (also exfat|ffpfs|ffpfsc)
cargo run -p ps5-dump-forge-cli -- convert <game_dir> --to folder --zstd-pack [--zstd-level 1..22]   # zstd packs (default level 5; also exfat|ffpkg|ffpfs|ffpfsc)
cargo run -p ps5-dump-forge-cli -- convert <zstd_packed_dir> --to folder --zstd-unpack   # back to the loose files (any target but lz4)
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
scripts/release-ps5.sh                         # dist/ps5-dump-forge-<ver>-ps5.elf, checked (SKIP_BUILD=1: check only; needs the zstd CLI)
scripts/test-freebsd.sh                        # core + server tests on FreeBSD UFS/FAT32/nullfs (VM; long)
FORGE_DEBUG_API=1 ps5/build.sh                 # bring-up build with POST /api/debug, GET /probe, /api/debug_input; never released
```
`tauri build` needs an absolute `CARGO_TARGET_DIR` if you set one.

## Preferences
- Do not commit, push or stage unless asked.
- Rust 2024 edition, latest stable (`rust-version` tracks it). CI is unpinned and latest on purpose.
- Keep code small and boring (stdlib first, no speculative abstractions; `// ponytail:` notes name
  the ceiling of a deliberate shortcut).
- Changes to `vendor/` go into a new numbered patch in `vendor/patches/` and `vendor/README.md`.
  Don't replace `vendor/` with our own code unless asked.
- Tests are kept to the critical ones (round trips, byte pins, journal recover, corruption and safety refusals, FLINK auth); add a test only for one of those.
