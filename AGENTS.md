# AGENTS.md

ps5-dump-forge: converts a PS5 game between a folder and every image format ShadowMountPlus 1.7
mounts (`.exfat`, `.ffpkg` UFS2, `.ffpfs` PFS, `.ffpfsc` compressed PFS container; read and write), plus
debug FPKG `.pkg` (create and extract; on the PS5 also install), and edits a game's files in a folder or image (the Edit screen:
in place or rebuilt) or copies some out (Extract). Rust core, Tauri 2 GUI (`PS5 Dump Forge.app`/`.exe`/`.AppDir`), a
CLI (`ps5-dump-forge`), and a PS5 ELF payload (the CLI's `serve`: the same UI as a web page on port 8095).
Releases: single files, no archives (by user decision): per OS and arch (arm64, x86-64) the app (macOS `.dmg`, Windows `.exe`, Linux `.AppImage`) and the bare CLI `ps5-dump-forge-cli-<v>-<os>-<arch>[.exe]` (the web UI embedded, `FORGE_REQUIRE_WEB=1`; Linux's static musl, `release.yml`'s `linux-musl` job, built natively in the pinned Rust Alpine image), the PS5 `.elf` (and `payloads.json`, its ps5-payload-manager catalog), SHA256SUMS (the binaries); the licences ship in the source archives only.
`ps5/README.md` holds the payload's details (harness, HTTP routes, tile/self copy/launch on open, U1–U10
write safety); `docs/LZ4.md` the Pack user guide (zstd and LZ4 packs); `docs/zstd.md` the zstd pack format; `docs/packing.md` the TOML profiles of the packing strategy (`docs/packing-sample.toml` a sample).

## Layout
- `crates/ps5-dump-forge-core`: everything a job does. Own folder scanner (no symlink follow,
  exact names; no per-file open on unix: type, nlink and read permission from the lstat (mode bits, else
  `access(2)`; ponytail: an ACL deny shows at read), `OpenAhead` opens the next 16 files on 4 threads once reads go
  in `files()` order), preflight (names, special files, space, FAT32, depth, name limits), jobs (a worker thread
  per job, one heavy job and one transfer at a time, each slot FIFO, conflicts refused at start, cancel
  flag (a cancelled job's error is `cancelled`, or `cancelled; ` + what else failed or stays when that is
  more than the cancel), `catch_unwind`), `.part` + atomic no-replace
  rename (`finalize.rs`; macOS exFAT, Linux NFS and FUSE have none: check, then rename; FreeBSD: hard link where the folder has
  them, U3), `awake.rs` (`awake()`: one refcounted U8 guard for jobs, paired peers and browser
  uploads/downloads), BLAKE3 verification (`verify.rs`: 8 MiB slice hashes on a pool of up to 8 threads (hashed inline if none starts),
  at most 64 files and 64 MiB of partial slices held at once, past that a file hashed by a second read; fast
  sample or full), `fself.rs` (`Signed`: unsigned PS5 ELF executables served as fake-signed SELFs, make_fself's bytes;
  `unsigned`), `inspect.rs` (cover, firmware, backport firmware from executables, embedded DLC; default and
  generated output names), `prefetch.rs` (read-ahead thread), `delete.rs` (Edit's and Storage's delete,
  Storage's move; `in_use`, the job-path guard; a `flink://` path handed to its peer, `delete_remote`/`move_remote`;
  a NAS path through its adapter, `nas_delete`/`nas_move`/`nas_root`), `listing.rs` (`list_dir`; `list_explorer`, the PS5 explorer's ShadowMount+ rule), `library_roots.rs` (PS5 filesystem classifier, the refused-folder
  list) and `library.rs` (`scan_packages`, the Package installer's `.pkg` walk; `folder_refusal`; `archive_game_root`).
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
  trait (stat, list, read_at, read_checked, put_new, mkdir, create, write_at, sync, close, reopen (a closed,
  unrenamed file open again; `Unsupported` on an older peer and every NAS),
  rename_no_replace, rename_replace (a file over a file; Replace), remove, rmdir (an empty folder; `Unsupported` on a
  peer), free_space; `delete_path`/`move_path`, refused by default (`delete::not_here`), a paired peer's its own Delete
  and Move to, a NAS adapter's `delete::nas_delete`/`nas_move` over its own calls), `Scheme` `Flink`/`Ftp`/`Smb`/`Nfs` (`flink://<key_id>`,
  `ftp://`, `smb://`, `nfs://<host[:port]>`; `prefix`, `default_port` 21/445/2049, `authority`,
  `host_port`), `get_for` (a paired peer: the `Remote` Connect registers (`set`); a server: its core
  adapter), `FlinkPath` (any scheme; `within`: scheme, device, ASCII case-folded on SMB), `is_remote`,
  `is_nas` (smb + nfs: never edited, deleted and moved from the File explorer only), `RemoteReader`/`RemoteTree` sources, outputs through
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
  files and folders copied into a folder here, on a peer or on a NAS), `metadata.rs` (a game's plaintext
  `sce_sys` metadata from Sony's update server: into a build's output, or a recovery edit job; Key
  decisions), `estimate.rs` (`pack_estimate`: a Pack's, an fpkg's
  or an ffpfsc's compressed size before the job, `EstimateKind` zstd/lz4/kraken/zlib → `PackEstimate`: the source opened
  as a job opens it, the built-in strategy's packed set (every file for Kraken and zlib), evenly spread chunks
  compressed by the real encoder, at most 64 MiB and 10 s, extrapolated by size; reads only), `lib.rs`'s
  `free_space` (a folder here, `flink://`, `smb://` or `nfs://`; `None` for FTP or a failed lookup) and `out`
  (exclusive create, syncs, free space; for the peer side). FreeBSD/PS5 only: `dest.rs` (destination probe, U1), `durable.rs`
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
  `SHUFFLES`, upstream's since v12.6.30); `peek` (header and container only, a few KiB: patch, content ID and type,
  plaintext `param.json`, `icon0.png` size; the package scan's); `protected.rs` decrypts a protected container
  entry (`flags1` bit 31: `nptitle.dat`, the `npbind.dat` files a package keeps only in its container; issue #45),
  the inverse of the builder's `crypto::encrypt_entry`, once at open and served from memory (`Where::Held`), after
  `key_check` passes the passcode against the entry-keys slot (else it is left out, a details line says why; the
  image's own copy still wins); `mount.rs` builds its 32 B `KBlock`s during `kraken_image::describe_each`'s
  walk (vendor patch 0023; a counting walk sizes the list first), so a layout descriptor opens within
  `MAX_OPEN` (384 MiB) up to about 1.4 TB of mount at 256 KiB blocks (issue #72: a 398 GiB package's 32 MB
  descriptor needs about 137 MB); `pfs.rs` reads the outer image's digests in 64 KiB windows and back-to-back
  outer blocks in one call of up to 1 MiB (issue #71: a data read was followed by a seek to the digest table); `lih.rs` reads a patch package (`\x7FLIH`) as the package it patches (Patch packages
  below).
- `crates/ps5-dump-forge-lz4`: ampr_emu asset packs: AMPRPAK4 manifest + AMPRDAT3 volume writer and
  reader (unpack view as a `SourceTree`), the AMPRIDX3 reader, the packing strategy (`rules.rs`, shared by zstd and LZ4 Pack), LZ4 blocks through
  the C liblz4 (`block.rs`, `lz4-sys`: `LZ4_compress_default` at level 0, `LZ4_compress_HC` at 3..=12,
  `LZ4_decompress_safe`), the embedded runtime (`runtime.rs`: ampr-emu-ng 0.5.1 `runtime::zstd()`, the one
  bundled, `NG_VERSION`: `build.rs` stores the `.sprx` as one zstd frame (level 22), decoded once on first use
  into a `OnceLock`; size and SHA-256 pinned (`pinned_sizes_and_hashes`); `classify` also recognizes by pinned
  SHA-256 builds no longer shipped: drakmor's ampr_emu 0.4.2.1 release (`RELEASE_SHA256`, `VERSION`), the old
  trace build (`TRACE_SHA256`) and the old ng builds (`OLD_ZSTD_SHA256`, `NG_6972875_SHA256`, `V020_ZSTD_SHA256`, `V030_ZSTD_SHA256`, `V031_ZSTD_SHA256`, `V032_ZSTD_SHA256`, `V033_ZSTD_SHA256`, `V034_ZSTD_SHA256`, `V035_ZSTD_SHA256`, `V040_ZSTD_SHA256`, `V041_ZSTD_SHA256`, `PREVIOUS_ZSTD_SHA256`); `version_of`
  reads an ampr_emu build's version, `ng_version_of` an ampr-emu-ng's). Glue in `core/src/lz4.rs` (open/unpack of LZ4 and zstd packs, index,
  runtime swap). `tree.rs` is `PackedTree`, the packs served as a `SourceTree` from a `writer::measure` pass.
  `vendor/ampr-emu-ng/`: the ampr-emu-ng v0.5.1 release (tag commit e1789c1; `libSceAmpr.sprx`, embedded as
  `runtime::zstd()`; `VERSION`; `SHA256SUMS`, the release's, which still names a debug build 0.5.1 doesn't
  ship) + the tag's source archive. drakmor's ampr_emu (`vendor/ampr_emu/`, `runtime::release()`) is gone (by
  user decision); its builds are still detected.
- `crates/ps5-dump-forge-zstd`: zstd packs (`ampr_zstd.index` + `ampr_zstd-NNN.pak` volumes of at most 4 GiB -
  64 KiB, 256 KiB chunks, one zstd frame or raw each): the format (encode + validating decode), the
  writer (`writer::pack_with`; `writer::measure` + `tree::PackedTree`, the packs made on demand for an
  image, as LZ4's) and the reader (`reader::detect`, `reader::unpack`: the index and every
  volume header checked, then the logical files served as a `SourceTree`, 8 chunks per volume read decoded
  in parallel; a loose file found by a binary search of the sorted listing, `Unpacked::listed`); spec in `docs/zstd.md`. Without its default `std` feature the crate is `no_std` and keeps
  only the convention (magics, layout, names, `Chunk`, the volume header): ampr-emu-ng builds against
  that. Glue in `core/src/zstd_pack.rs` (the pack job's tree, plan, packing,
  checks).
- `crates/ps5-dump-forge-archive`: `.zip`/`.7z`/`.rar` sources (`detect`, `list`, `read_files`, `Reader`,
  `ArchiveError`, the two password strings). `zip.rs` own reader (ZIP64, STORED + Deflate via `flate2`), `sevenz.rs`
  over `sevenz-rust2` (decode only), `rar.rs` our FFI wrapper over `unrar_sys` (bundles UnRAR's C++; every target,
  the PS5 payload included), `parts.rs` multi-part naming, `stream.rs` the `Reader`. Glue: `core/src/archive.rs` (`ArchiveTree`), `inspect.rs`,
  `library::archive_game_root`. Licence: `vendor/LICENSE-UnRAR.txt`, `vendor/LICENSE-UnRAR-exception.md` (in every release's source archives).
- `crates/ps5-dump-forge-smb`: own blocking SMB2 client (2.0.2/2.1; the `smb` crate was rejected: ~180
  crates and its guest login fails on Samba 4.23): `ntlm.rs` (raw NTLMSSP NTLMv2, in-house MD4; an empty user
  is a guest login), `wire.rs` (framing, credits, 1 MiB reads, HMAC-SHA256 signing when required),
  `rpc.rs` (srvsvc NetShareEnumAll: disk shares), `lib.rs` (`Session::connect`, `shares`, `list`, `stat`,
  `open` → `File::read_at`; writes (by user decision: a share is an output too): `create_file` (exclusive),
  `mkdir`, `rename` (never replaces), `rename_replace` (ReplaceIfExists 1: a file over a file; Replace),
  `remove` (files), `rmdir` (an empty folder), `free_space`, `File::write_at`/`flush`).
  Glue: `core/src/smb.rs`. ponytail: no SMB 3.x, encryption, DFS, reconnect or
  pipelining, response signatures not verified, no NTLM MIC.
- `crates/ps5-dump-forge-ftp`: own blocking plain-FTP client (RFC 959 + FEAT, MLSD/MLST/SIZE/REST, EPSV;
  passive, binary; user + password or anonymous): `Session` (list, stat, reads, mkdir, rename (check, then
  `RNTO`), `rename_replace` (`RNTO` with no check; the server's filesystem refuses a folder), remove, `rmdir` (`RMD`, a bare refusal looked at for its kind), `can_rewrite` (a `REST` + `STOR` probe)), `Writer` (any offset, `REST` to go back);
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
  Replace), `remove` (files only), `rmdir` (an empty folder), `free_space`;
  absolute server paths). Glue: `core/src/nfs.rs`. ponytail: one connection per session, requests
  serialized, no reconnect; cached handles live 3 s; an idle v4.1 session may expire (dropped, reconnected).
- `crates/ps5-dump-forge-cli`: binary `ps5-dump-forge` (`inspect`, `convert`, `edit`, `serve`, `update`), JSON-lines events.
  `update` (the CLI's one network command) prints whether a newer release lists this build's CLI asset
  (`ps5_dump_forge_updater::cli_asset`) and its link; `update --install` downloads it, checks it against the
  release's SHA256SUMS and replaces the running binary (unix: temp beside it, 0755, rename; Windows: the running
  exe renamed to `<exe>.old`, removed on a later run).
  `convert --restore-metadata` sends `ConvertRequest.restore_metadata`; `edit --restore-metadata [--metadata-url U]
  [--original-runtime]` sends `ConvertRequest.recover_metadata` (alone or with other changes).
  `edit` (PR #69, by user decision): `--add from:to`, `--delete`, `--rename from:to`, `--mkdir` in command-line
  order into `ConvertRequest.edits`, `--rebuild` (`edit_rebuild`); `--check` prints `edit_check`'s JSON and exits 1
  when the mode the command would run has findings.
  A second Ctrl-C quits at once (unix `_exit(130)`; Windows the default handler), the first prints
  "cancelling... (press Ctrl-C again to quit now)"; the `.part` stays (`stale_parts`).
- `crates/ps5-dump-forge-ps5`: the payload's global allocator (`Pool`, installed by the CLI only for
  the PS5 and musl, the static Linux CLI; musl's gain unmeasured): freed blocks of 64 B..4 MiB kept in power-of-two classes, since the PS5 libc serializes
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
  (no 1 MiB cap; `BodyReader` in `http.rs`). Identical capped reads (inspect, default_output,
  generated_output, package_scan, host_tree, pack_estimate, edit_check) share one run
  (`shared.rs`, keyed by a BLAKE3 hash of route + body, no body kept): a waiter takes no read slot and answers
  202 after 60 s so the page asks again; a finished reply is kept 120 s for one `?retry=1` request (issue #74:
  a long PS5 read outlived the browser's request); `http-client.ts` retries such a read with `?retry=1` after a
  network error while `/api/session` answers (10 tries, 30 min). `generated_output` reads only
  `sce_sys/param.json`, not a scan of the folder. Connect: the `ps5-dump-forge-flink` crate (below),
  `connect.rs` (`Connect`: identity (`node.key`),
  approvals (`Approvals`, `Outgoing`), `peers.json`, the TCP 9883 listener, pair/forget/devices, liveness, `snapshot`,
  a peer's `install` and `install_here` RPCs (`set_install`, the PS5's server only; `Connect::install` asks a PS5);
  also used by the Tauri app), `connect_fs.rs` (a peer's files over FLINK: the peer side's RPCs and lanes,
  `ConnectRemote`, core's `Remote` on this side; the busy hooks both ways; `set_delete_move`, a peer's Delete and
  Move to here; the speed test, `speed_test`, `serve_speed`: the `speed` RPC, then `speed_up`/`speed_down` lanes,
  nothing on disk), `discovery.rs` (Forge on UDP 8094; a socket error other
  than WouldBlock logged once, `discovery: <e> (retrying every second)`, and retried every second, so the
  responder outlives a network down/up or rest mode; ponytail: the sockets are never bound again), `probe.rs` (Add a device: what answers at an address), `payload.rs` (elfldr's address as Install on PS5 and Send a payload take it, port 9021 by default), `mdns.rs` (NAS: smb/ftp/nfs; `merge`), `wsd.rs` (Windows PCs by WS-Discovery),
  `nas.rs` (saved NAS devices: `nas.json`, one entry per host + port, `protocol` smb/ftp/nfs, credentials;
  `publish` hands SMB logins to `core::smb::set_credentials`, FTP ones to `core::ftp::set_logins`).
- `crates/ps5-dump-forge-updater`: the PS5's ELF loader (elfldr, port 9021) from here (one `lib.rs`): `install`
  (elfldr probed, connect and close; then `fetch`: a release's `ps5-dump-forge-<v>-ps5.elf` from GitHub over
  HTTPS, redirects followed but never to plain HTTP, 50 s; `200`, a Content-Length of at most 64 MiB and the ELF
  magic required, a cut or longer body refused; held in memory, or with a `spill` file (the PS5's self copy) saved
  over it as the self copy saves itself: temp + sync + rename under its flock; then the bytes streamed to elfldr),
  `send` / `send_bytes` (an ELF file or bytes streamed, at most 64 MiB, 120 s). One dependency, `ureq` with
  rustls (ring, webpki roots; + `libc` on unix for `O_NOFOLLOW`), by user decision: elfldr can't follow GitHub's
  302, so it is never handed a URL. ponytail: TLS authenticates GitHub; no signature or version check.
  `psn.rs` (metadata recovery's GETs, by user request): `get(url, max, cancel, progress)`, http/https only, no
  credentials in the URL, at most 5 redirects followed by hand (never HTTPS to HTTP), Content-Length and `max`
  enforced, 15 s connect and 60 s per read; a `*.playstation.net` host is verified against "SCEI DNAS Root 05"
  only (`scei-dnas-root-05.der`, SHA-256 pinned by a test; `sgst.prod.dl.playstation.net` chains to it, its leaf
  RSA/SHA-1, which rustls refuses, so a SHA-1 verifier (ring's legacy one) joins that agent's chain algorithms),
  then on a rustls error against the public roots; any other host the public roots. Direct `ring` and `rustls`
  dependencies (the versions ureq links).
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
  Shell: `App.tsx` (the side rail of screens, its foot; L2/R2), `prefs.tsx` (`forge.ui`: theme, scheme, rail,
  advanced folds, the replace switch).
  Build screen (`app/src/build/`; Convert and Pack merged): `Build.tsx` (folds `STEPS`, the summary, the space
  line, Build / Add to queue), `steps.ts` (the step contract), `packs.tsx` (the Target step: packs and format,
  levels, the result panel, output), `backport.tsx` (the Backport step), `options.tsx` (Restore metadata,
  Delete the source, full verification), `build.css`, `fields.tsx` (Build's fields, explanations in tooltips), `formats.ts` (targets,
  pack levels); `PackHelp.tsx` (How pack works).
  Edit screen: `Edit.tsx` (card, explorer, row actions, drops), `EditChanges.tsx` (pending bar, conflict
  and Discard questions, what an add brings: `fromHost`, `dropped`), `EditApply.tsx` (mode, `edit_check`,
  confirm, uploads), `explorer.ts` (pure: the tree, the pending changes' fold, a port of core's `Edited`
  rules, `planAdds` (clashes), `pendingReducer` (batches), `toEdits`, `checkable`, `stagingNames`).
  Queue screen: `Queue.tsx` (head, filters, Retry failed, Clear finished), `JobList.tsx` (`JobsCard`, each job's
  card: segmented bar, stage crumbs, result, log panel), `jobs.ts` (the reducer, `STAGE_WEIGHT`,
  `stageFraction`), `queue.css`.
  Storage screen: `Storage.tsx` (view switch, PS5 only), `Files.tsx` (the File explorer), `FilesTabs.tsx`
  (its devices, `platformIcon`; `useRowDrag`, shared with Transfer), `Packages.tsx` (the PS5's Package installer); `InstallDialog.tsx` (the one
  "Install this package?" question and `installRequest`, shared with Connect's Install pkg and Transfer).
  Transfer screen: `Transfer.tsx` (the device tabs, the two panes, Copy and Install, drags and system
  drops, `transferRequest`; the web build's This browser upload and download), `TransferPane.tsx` (one
  device's pane: listing, batch selection, `dragPaths`, `transferDropTarget`), `transfer.css`.
  Shared by Transfer and the File explorer: `DeviceTabs.tsx` (the browser-style device tab strip,
  `tabPanelProps`), `bookmarks.tsx` (`Bookmarks` chips, `BookmarkToggle`, `useBookmarks`), `devices.css`.
  Connect screen: `Connect.tsx` (a device hub: left This device with the QR and Add a device; right the
  Devices list (peers and NAS) with its filter and Nearby (Autodiscovery's finds); device and NAS
  details, the pairing wait, Add SMB / FTP / NFS; File Transfer and Send open the Transfer screen), `ConnectHelp.tsx` (How to connect), `useConnectState.ts` (polls
  `connect_state`; selectors `useOnlineCount`, `useRemoteJobs` (kept until its JSON text changes) and
  `useSavedNames` (kept until a name or address changes); `common.tsx`'s `usePeerPath`/`useDeviceName` read
  names only, so a poll that changes nothing re-renders nothing). `Picker.tsx`: the tabbed picker (a tab per online device; this device's bookmarks on its tab).
  Settings screen: `Settings.tsx` (appearance, advanced, language, output names, the PS5 notification switch).
  About screen: `Formats.tsx` (features, Compare formats, credits).
  `app/scripts/*.mjs`: plain-node checks (`check-paths`, `check-poller`, `check-launcher`, `check-update`,
  `check-upload` (the upload URL's encoding against the server's decoder, `upload` over a mocked XHR),
  `check-explorer` (`explorer.ts`: the fold mirrors core's `Edited` rules; the folder-merge case is checked
  only there), `check-shoulders` (`shoulders.ts`: L2/R2 keys, tab order), `check-peers` (`peers.ts`: saved
  devices' names in paths and backend text), `smoke`); run by hand (`node scripts/<name>.mjs`; no npm
  script, not in CI).
- `vendor/ps5upload-{fpkg,pkg}`: ps5upload v12.6.30 crates (readers, FPKG builder, verify) plus a
  local patch series in `vendor/patches/`; see `vendor/README.md`. Edition 2021 on purpose.
  `vendor/lzma-rust2`: crates.io 0.22.0 + patch 0021 (bounded multithreaded LZMA2 decode, which
  `sevenz.rs` sizes per block by dictionary and largest run), via `[patch.crates-io]`, excluded
  from the workspace.
- `scripts/`: checks and releases (see Commands); `check-versions.sh` also matches the tag on a tag build;
  `make-payloads-json.sh` writes `payloads.json`, a ps5-payload-manager custom repository (the release's `.elf`
  URL, `v<version>`, its SHA-256, the stable `filename` `ps5-dump-forge.elf`) and parses it back; CI's `ps5` job
  runs it on the ELF it builds, the publish job after `SHA256SUMS` (which doesn't list it).
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
- Archive sources (`.zip`/`.7z`/`.rar`, one game root ≤ 4 levels deep, found by `library::archive_game_root` from the
  listing) are read in place, never unpacked to disk: `core/src/archive.rs` serves the game folder as an
  `ArchiveTree` over the archive crate's `Reader`, which decodes front to back on its own thread (a session; bounded
  queue) and serves the entry asked for, skipping those before it; an entry the session went past (or a ZIP/7z skip
  that never recorded it) starts a new session from the archive's start: cheap when non-solid (header skips),
  a re-decode of everything before it when solid (by user decision: no temporary files, even for solid
  archives). Verification hashes as written, so nothing re-reads the archive. The source stamp covers every
  part. Every part of a multi-part source is guarded from delete while its job is unfinished; edits refuse an archive (Edit browses and extracts from it only). `inspect` reads only the entries it needs (no
  extraction). Passwords for 7z and rar only (encrypted zip refused), `ArchivePassword` redacted in `Debug`, never
  serialized or logged; with none given, `archive::password` tries `KNOWN_PASSWORDS` (by user decision: six
  release-site passwords, in order; one fits when the game folder's `param.json` reads, and when that
  file is in the clear, the game's smallest encrypted file too; `Entry::encrypted`) for inspection, naming and
  jobs alike, and only when none fits is the error "needs a password" (the UI's prompt); CLI
  `--password-stdin` / `FORGE_ARCHIVE_PASSWORD`. Inspecting reads the password check and its files in one
  pass; the last 4 archives opened keep the password that opened them and their `param.json`
  (`archive::OPENED`, keyed by every part's stamp, one check per archive at a time, a job's wait
  cancellable; a typed password is handed only to a request giving it), so naming and a job's start read
  nothing more (issue #30; an archive with several game folders still reads their `param.json` to find
  the game). Multi-part: `.partN.rar`, `.rar`+`.rNN`,
  `.7z.NNN`/`.zip.NNN`, split `.z01`…`.zip`; first part only, missing parts named. 7z Deflate is refused: its
  `deflate` feature would switch the workspace's `flate2` to zlib-rs, replacing `.ffpfsc`'s miniz_oxide.
  Memory (by user decision): a job stays within 512 MiB except for the archive's own dictionary,
  which the decoder allocates whole: RAR is accepted up to 4 GiB and 7z up to 1 GiB, so an archive
  packed with a dictionary over ~256 MiB goes past the cap. 7z LZMA2 decodes on several threads
  only in a job's session (`Sink::streams`; small reads stay on one) and for a block that is a lone
  LZMA2 coder, sized per block (dictionary + 6 × its largest run within 256 MiB; `sevenz::decode_threads`); RAR5 decodes on UnRAR's own threads (at most 8).
- LZ4 packs are an asset-pack transform of a game folder (AMPR's), not an SMP image format: `--to lz4`
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
  fast/full policy. A title whose `eboot.bin` doesn't import `libSceAmpr` is packed too (by user decision, core, CLI
  and UI alike): ShadowMount+ force-loads `fakelib/libSceAmpr.sprx`, so ampr-emu-ng serves such a game. Compression is the C liblz4
  (`block.rs`, by user decision): `lz4_level` 0 (Fast, `LZ4_compress_default`, the default) or an HC level
  3..=12 (Medium 9, Max 12), any other value a finding; CLI `--lz4-level`. The runtime is always the bundled
  ampr-emu-ng (`runtime::zstd()`) and the strategy pack everything except system files (by user decision, D3:
  `lz4_runtime` keeps only `ng`; `Lz4Runtime::AmprEmu` is a preflight finding and the CLI has no
  `--lz4-runtime`). By user decision: no traces; which files are packed is the packing strategy
  (built-in or a TOML profile; Packing strategy, below).
- zstd packs (`core/src/zstd_pack.rs`, `ps5-dump-forge-zstd`; spec `docs/zstd.md`; named zstd, files on disk AMPR
  style, by user decision: `ampr_zstd.index` (`AMPRZPK1`) + `ampr_zstd-NNN.pak` (`AMPRZDT1`, from 000)): volumes
  (zstd, 256 KiB chunks, a chunk kept raw when zstd doesn't shrink it) that ampr-emu-ng (`runtime::zstd()`,
  `RuntimeKind::Zstd`, id `zstd`, `vendor/ampr-emu-ng/libSceAmpr.sprx`, the v0.5.1 release, the one runtime
  Forge bundles, size and SHA-256 pinned (`ZSTD_SHA256`); the user's runtime, which reads LZ4 packs too and serves
  a fully packed zstd or LZ4 game; by user decision, D2, 0.5.x no longer serves mmap of a packed file (its
  `/data/ampr_cache` disk cache went; upstream measured no game that does); its index cap is 80 MiB;
  the v0.4.1 release (`V041_ZSTD_SHA256`), the v0.4.0 release (`V040_ZSTD_SHA256`), the v0.3.5 release (`V035_ZSTD_SHA256`), the v0.3.4 release (`V034_ZSTD_SHA256`), the v0.3.3 release (`V033_ZSTD_SHA256`), the v0.3.2 release (`V032_ZSTD_SHA256`), the v0.3.1 release (`V031_ZSTD_SHA256`), the v0.3.0 release (`V030_ZSTD_SHA256`), the local 6972875 build embedded before v0.3.0 (`NG_6972875_SHA256`), the v0.2.0 release
  (`V020_ZSTD_SHA256`), the v0.1.0 release (`PREVIOUS_ZSTD_SHA256`) and the old local build (`OLD_ZSTD_SHA256`),
  their files removed, are still recognized by their hashes, and any other build holding the
  `AMPRZPK1`, `AMPRPAK4` and `AMPRDAT3` magics counts as ampr-emu-ng too) reads.
  Pack (`zstd_pack`, `zstd_level` 1..=22, default 5; CLI `--zstd-pack [--zstd-level N]`): into a `folder` or
  straight into an `.exfat`/`.ffpkg`/`.ffpfs`/`.ffpfsc` (`.pkg` refused), no LZ4 option, any title (one whose `eboot.bin` doesn't import libSceAmpr gets ampr-emu-ng injected, logged), no source file named like a pack file;
  else preflight findings. `zstd_pack::prepare`: ampr-emu-ng at `fakelib/libSceAmpr.sprx` (installed or
  replacing any), journal, logs and old index left out, a fresh `ampr_emu.index` (the job's `mtime`) listing
  every file written, loose and packed. The packed set (by user decision, branch feature/pack-zstd) is the built-in
  strategy **pack everything except system files** (`rules::select_everything`: every non-empty file but `always_loose`, root files, audio, video and configs
  included; an LZ4-packed source too, by user decision: its packs' old set is not reused) or a TOML profile's; never
  `zstd_pack::loose` (`always_loose`, the pack files, the index); no auto-loose. No store-only files (the 1% rule
  is dropped: one pass per file, a chunk zstd doesn't shrink kept raw). A folder: stages scan, preflight,
  write, pack, verify, finalize. An image: two passes as LZ4's (`writer::measure`, then the image written from
  `tree::PackedTree`; stages scan, preflight, measure, write, verify, finalize). Verify checks the index and
  every volume header, then the logical files read back through the packs.
  Unpack (`zstd_unpack`, CLI `--zstd-unpack`): any target but `lz4`, no LZ4 option, not with `zstd_pack`;
  the packs decoded (`reader::unpack`), the runtime by the runtime rule (below; a current ampr-emu-ng kept) and a
  fresh index (into a `.pkg` the builder's, the runtime left out); a source without `ampr_zstd.index` is a finding. `lz4::open` decodes LZ4 packs for any LZ4 option or zstd pack, zstd
  packs for zstd pack or unpack and for LZ4 packing.
  A zstd-packed source (root `ampr_zstd.index` starting `AMPRZPK1`; `Inspection.zstd_packed`)
  is otherwise an ordinary source: a plain conversion into a folder or an image
  carries the packs as plain files, keeps its `ampr_emu.index` byte for byte (it lists the packed
  files) and keeps ampr-emu-ng (`Runtime::ng_action`: installed where missing; an ampr-emu-ng older than the bundled one (`runtime::NG_VERSION`, `ng_not_older`) or naming no version, and any other runtime, replaced, by user decision: v0.3.4's 48 MiB index cap, #59). Into a `.pkg` a packed game (zstd or LZ4) goes unpacked, without asking (issue #73, by user
  decision): `lz4::auto_unpack` turns a plain request into that `zstd_unpack` or `lz4: unpack` (log "a .pkg
  carries no packs: unpacking the game's zstd packs"), Build sends the unpack itself (the game's own kind +
  fpkg is `PackMode` unpack). Additional content (no `eboot.bin`, which takes no unpack option) and an Edit
  rebuild of a packed `.pkg` keep their packs; there the runtime is left out (the fpkg exception, below) and
  the builder packages its index as it is (upstream's `source::packed_kind`: LZ4 or zstd packs). Remove backport is
  refused in the UI for a packed source (`zstdBackport`/`lz4Backport`: unpack first). Its label is
  shown wherever the LZ4 one is: the source card's AMPR tile (`AmprTile`, one for LZ4 and zstd, by user
  decision: the runtime with its version as the title, e.g. `ampr-emu-ng 0.5.1`, `ampr_emu 0.4.2.1`, "No runtime" (with no notes, by user decision);
  the value Packed (zstd) / Packed (LZ4) in the green neon look, Damaged packs as before, Unpacked greyed
  `.tile.unpacked`).
  UI: Build's packs row (Build, below); How pack works in `PackHelp.tsx`. Job rows: `jobType` `pack`, a tag
  `LZ4 pack · <level>`, `zstd pack · <level>` (the preset's name, else `level N`; ` · custom rules` with a profile),
  `LZ4 unpack` or `zstd unpack`. The source card's AMPR tile (Edit's Game facts fold only; Build's card has none) names
  what a conversion does to the runtime (`runtimeAction`: install or replace; `Lz4Facts.runtime_action`) and a
  non-ok index; with no runtime (`none`) it carries no notes (by user decision: the title "No runtime" says it).
- A folder output (a conversion's: plain, LZ4 or zstd pack; local `.part` folder or a peer/NAS folder) puts
  `sce_sys/**` and the root `eboot.bin` in place last, after every other file and the packs, right before verify
  (`convert::launch_order`): ShadowMount+ finds a game folder by them, so a half-written one isn't
  launched. A plain local folder keeps the source's read order (a solid archive is decoded once): the folder writer
  writes the launch files under temporary names at the `.part` root and renames them (`sce_sys/` first, then
  `eboot.bin`) once every other file is written and synced and its writers are joined, before the folder's sync.
  Onto a peer or NAS, and in an LZ4/zstd pack's folder, they are written in a last pass of their own, `sce_sys/`
  first (ponytail: read out of order there, so a solid archive is decoded again; a peer renames only files it saw
  created). The folder writer (`extract::write`) reads files of at most 4 MiB whole on the job thread and has 4
  threads create, write, sync and close them (at most 13 files in flight; joined before the folder's sync, so the
  macOS F_FULLFSYNC barrier and FreeBSD's per-file `sync_retry` hold); larger files take the old path.
- Every conversion fake-signs a plain ELF executable (user request, from a Borderlands 4 dump that never started; core `fself.rs`): a file with an
  executable extension (`sdk::is_executable`; a `.bin` only when named `eboot*`, so data `.bin` files aren't opened), the ELF magic and `e_type` `0xFE10`/`0xFE18` (`fakelib*` included)
  doesn't launch, so `Inspection.unsigned` lists it (CLI `unsigned:` line; Edit's source card warn note and a
  Build summary line, `common.unsigned`, not for a patch) and a conversion or Pack job (not an edit, Extract, transfer or install)
  serves it through `fself::Signed` right after the backport step, before the AMPR index: byte for byte
  ampr-emu-ng's `make_fself.py --ptype fake` (pinned by a BLAKE3 test), planned from the headers (one 16 KiB read at 0, so a remote source fetches no read-ahead window), the segments
  copied as the file is written; log `fself: signing N unsigned executables: ...`, one it can't plan kept as it is
  (`fself: can't sign ...`). `ampr_emu.index`'s records of a signed file that list its ELF size get the SELF size
  in place (a plain u64, no checksum; log `fself: ampr_emu.index: N records set to the signed sizes`), so a
  packed game converted as it is signs too (by user decision; executables are `always_loose`, and the LZ4
  manifest's loose records may differ from the disk) and an unpacked game's otherwise exact index stays exact.
  ponytail: the ex-info's SHA-256 of the whole ELF sits in front, so each ELF is read twice (a solid archive
  decodes it again); an executable inside a third-party pack isn't seen.
- Every conversion repairs an AMPR title (`eboot.bin` imports libSceAmpr; by user decision): any target, Build,
  CLI, server and pack jobs alike (`lz4::plain` for a request without LZ4 options, `prepare`'s full path otherwise).
  Edit jobs are the exception (their own index rule, runtime never swapped; see the Edit screen below).
  Runtime (by user decision, the redesign's rule): one bundled runtime, ampr-emu-ng 0.5.1 (`runtime::zstd()`), no
  version selector; drakmor's ampr_emu is no longer shipped, only detected (`classify`, `version_of`,
  `Lz4Facts.runtime`). `lz4::runtime_of(tree)` → `Runtime`, then `Runtime::action(ampr, packs,
  replace_ampr_emu)` → `RuntimeAction`, shared by the jobs and `inspect`: `Install` where an AMPR title has
  none; `Replace` an ampr-emu-ng older than the bundled one (`ng_not_older`) or naming no version, and Forge's old
  trace runtime (recognized by its pinned SHA-256, on any title); `Keep` ampr-emu-ng at the bundled version or
  newer; an ampr_emu build (Forge's old release one, another, or one that doesn't read) replaced with
  ampr-emu-ng while `ConvertRequest.replace_ampr_emu` is true (the default; CLI `--keep-ampr-emu` turns it off;
  the UI sends Settings' switch with every Build request), else kept, except into an output holding LZ4 packs
  (an LZ4-packed source as it is), which also needs `runtime::reads_packs`; `Untouched` for a non-AMPR title.
  Where only ampr-emu-ng serves the output (zstd and LZ4 Pack, whatever `eboot.bin` says, and a zstd-packed
  source as it is) `Runtime::ng_action`: a current ampr-emu-ng kept (`Runtime::bundled` byte for byte, or by
  version), none installed, anything else replaced. Unpack (LZ4 or zstd) follows `action`. A kept ampr_emu
  is logged ("keeping the game's ampr_emu ... (replace_ampr_emu is off)"), as is a game's newer ampr-emu-ng.
  **fpkg exception** (by user decision, D7): a `.pkg` never carries an AMPR runtime: `Runtime::pkg_action` →
  `Omit` for an AMPR title or wherever a runtime is there (`lz4::leave_out_for_pkg`), nothing installed, the
  console's loader provides it (kraken-mount's shared ampr-emu-ng); the builder's fresh `ampr_emu.index` lists no
  runtime. It holds for every `.pkg` Forge writes: a conversion, an unpack into `.pkg`, a packed source as it is,
  an Edit rebuild (log "leaving out fakelib/libSceAmpr.sprx: a .pkg carries no AMPR runtime, ...").
  `Lz4Facts.runtime` names the runtime (`forge_release`/`forge_trace`/`zstd` (ampr-emu-ng)/`other`/`none`),
  `runtime_version` its version, `shipped_runtime_version` the ampr_emu last bundled; `runtime_action` (plain
  convert, `replace_ampr_emu` on), `pack_runtime_action` (Pack/Unpack) and `pkg_runtime_action` (`omit`) name the
  action (`install`/`replace`/`keep`, `None` for a non-AMPR title); the CLI's LZ4 line says it too.
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
  runtime (or a signed executable) then differs from the LZ4 manifest's loose record, which `reader::unpack` lets pass
  for any loose record (the runtime reads loose files from the disk).
  A damaged one is logged with its size and the reader's reason (`LZ4: ampr_emu.index does not read (damaged): 4096 bytes, ...`).
  `Lz4Facts.index` (`ok`/`stale`/`damaged`/`missing`; `None` when packed, when the action is `Untouched`, or for an
  archive's index over 16 MiB) is that comparison. Edit's game facts, Build's summary and the CLI say a
  conversion refreshes it.
- Packing strategy (by user decision one neutral name: which files are packed and which stay loose, shared by
  zstd and LZ4 Pack; zstd and LZ4 are only the pack formats; `docs/packing.md` the TOML keys): one built-in
  strategy (by user decision, D3; its name as logged and shown, `lz4::EVERYTHING`): **pack everything except system
  files** (`rules::select_everything`: every non-empty file but `always_loose`; no auto-loose) for zstd and LZ4
  Pack alike, a game without AMPR too; the compatibility LZ4 strategy (LZ4 Pack with ampr_emu) is gone. Log `packing
  strategy: <name>; N files to pack, M loose; ...`. Or a TOML profile (by user request:
  upstream `ampr_pack.py`'s / Lazy_AMPR's format; `ConvertRequest.pack_profile`, CLI `--pack-profile <toml>`,
  only with an LZ4 or zstd pack, else a finding). The profile may be on this device, a paired peer or a saved NAS
  (`net::read_small`, at most 1 MiB, a regular UTF-8 file), read once in preflight (ponytail: not guarded as a job
  path); every problem a finding `pack rules <file>: ...`. Only `[pack]` and `[[rule]]` are read: `default_action`
  (default `loose`), `default_block_size`, the `auto_loose_*` keys; a rule's `action` compress|store|loose,
  `include`/`exclude` (Python `fnmatchcase`: `*` crosses `/`), `block_size`, `layout`, `hot`; the last matching
  rule wins. Every other key (`[groups]`, lanes, dedupe, a typo) is ignored, one log line `pack rules: ignored
  keys: ...`; `include_from`/`exclude_from` read pattern files beside the profile (inside its folder, 1 MiB
  each, 16 MiB and 200,000 patterns in all, 10,000 with a wildcard). With a profile only
  `rules::always_loose` (system and runtime files, and every `*.ini`, by user request: `dlc_emu.ini` and other settings stay editable) overrides it (the
  user's call; Build warns). zstd: `store` packs
  as compress (chunks that don't shrink are raw anyway), no auto-loose. Log `pack rules: <file>: N rules; M files
  packed, K loose; ...`.
  `--lz4-unpack` (automatic when the target is lz4; any other target, `.pkg` and `.ffpfsc` included) restores the
  logical assets; the runtime follows the runtime rule above (kept unless installed, replaced or, into a `.pkg`, left out) and an `ampr_emu.index` listing exactly the output stays.
  A packed record (LZ4 or zstd) whose path (ASCII case-folded) is also a file on disk is never refused (by user
  decision; a third-party release packed `sce_sys/ext_info.dat` and kept it): identical copies (same name, same
  size, at most 16 MiB, same bytes) keep the plain file (it leaves the packed set), any other case serves the packed
  copy and leaves the physical file out of the listing (both readers' `unpack`); one log line `LZ4: N files both
  packed and on disk: I identical (plain copy kept), D different (packed copy used): ...` (`zstd: ...` for zstd packs).
  One line per Pack job says what stays loose (`lz4::everything_loose_line`: system and runtime files and empty
  files; with a profile `lz4::loose_line`: the profile's and `always_loose`'s counts). With a profile, then
  auto-loose (upstream `ampr_pack.py`'s defaults, the profile's `auto_loose_*` keys, `AutoLoose`; LZ4 only):
  a compress-spec file (not store, not hot) of ≥ 64 MiB is sampled during preflight (32 blocks / ≤ 16 MiB at fixed, evenly
  spread block indices, `writer::sample` = the writer's `encode`) and kept loose when it saves < 5% or
  ≥ 90% of the blocks stay RAW; one log line `auto-loose: N large files kept loose (incompressible samples): …`.
- Never touch the source: junk (`.DS_Store`, `._*`, `.fseventsd`, ...) is filtered, not deleted.
  Three exceptions, by user decision, each an explicit request:
  1. Deleting the Edit screen's source, a File explorer or Package installer row (the trash button of the Edit
     card or of a row, or its Delete key; one **No** / **Yes, delete**
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
     A peer's path goes to that peer's own Delete and Move to, a NAS's to its adapter (A peer's files and
     Managed from the File explorer, under Connect below).
  2. Edits (the Edit screen, below): a request with non-empty `ConvertRequest.edits` is an edit job, which
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
     (`finalize::path_id`: unix dev + ino, Windows volume serial + file index; an id of 0 is refused except a volume root's, which a FAT/exFAT drive reports as 0, issue #34) and every empty folder, as the
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
     peer, before its own delete) goes with the folder. UI (Build): **Delete the source** (by user request)
     (Build's options step, `build/options.tsx`, shown, not folded), off by default, in memory only (kept
     across sources), its tooltip `deleteSourceHint` ("No undo. ..."), a warn line in Build's summary while on;
     for a NAS source off and disabled (tooltip `deleteSourceNas`), and `delete_source` never sent for a NAS
     path; hidden for a patch package. Job rows (`JobList.tsx` `sourceLine`, matched by the backend's English prefix, left out of the
     Verified fold): `jobs.sourceDeleted` as a muted `.note-line`, `jobs.sourceKept` and
     `jobs.sourcePartlyDeleted` as a `.note-line.warn`. A job whose checks carry `source deleted:` makes
     `App.tsx` call `notifyDeleted` from its Done event and, in the web build, when the poller restores it
     already finished (`jobs://restore`: a reload, another viewer's job); not a peer's mirrored job; the
     notified job ids are kept in memory, so each is heard once (ponytail: by id only, so a payload restart
     reusing a notified id isn't heard).
- Metadata recovery (by user request; the one online lookup besides the update check; core `metadata.rs`,
  GETs through `ps5_dump_forge_updater::psn`): `param.json`'s `versionFileUri` (or a given URL: a version XML,
  a manifest or the `_sc.pkg` itself) → the version XML's `<package content_ver>` equal to the game's
  `contentVersion` (else "offers X but not this game's version": give the manifest's or `_sc.pkg`'s URL) → its
  manifest's one `_sc.pkg` piece (a `\x7FCNT` container, 70 MB measured, at most 160 MiB, in memory, its
  entry count bounded as a package's), size and
  SHA-256 checked; its content ID and its `param.json`'s title ID, content ID and content version must be the
  game's. Taken where they differ from the game's (`metadata::changes`, compared through the source tree):
  `param.json`, `icon0`/`pic0`/`pic1`/`pic2` `.png` and `.dds`, `snd0.at9`, `save_data.png`, `shareparam.json`,
  the PlayGo tables (`playgo-chunk.dat`, `-hash-table.dat`, `-ficm.dat`, `playgo-scenario.json`); `param.json`
  keeps the game's `sdkVersion`, `requiredSystemSoftwareVersion` and `applicationDrmType` (a backport lowers
  them) unless `original_runtime` (CLI `--original-runtime`), then rewritten through serde_json. Never taken:
  protected records (`flags1` bit 31: the npbind files), the trophy and UDS `.ucp` (sealed in the piece,
  decrypted in a dump), patch metadata. Two uses (by user decision; the source dump is not touched by a build):
  - Build's **Restore metadata online** switch (off by default, in memory, any source but a patch; tooltip
    `metadata.buildHint`, a muted summary line; CLI `convert --restore-metadata`; `ConvertRequest.restore_metadata`,
    any conversion or pack, Delete the source too): `metadata::overlay` wraps the opened source in
    `lz4::Overlay`, the files served from memory in place of the source's, the source's file order kept (a solid
    archive is read in it; an added file last) (before `lz4::open`, so an index or a
    pack built later lists them; the download's progress under `scan`). Never blocking: any failure but a cancel
    is one `warning: metadata not restored, building the game as it is: ...` line. With an edit, extract,
    transfer, install or recovery a preflight finding.
  - Edit's **Restore metadata online** switch (by user decision: the same switch as Build's, off by default, in
    memory across sources, on the game card for a game here, not an archive or a patch; tooltip
    `metadata.editHint`; CLI `edit --restore-metadata [--metadata-url U] [--original-runtime]`) sends
    `ConvertRequest.recover_metadata {url, original_runtime}` with Apply's edits, or alone (the pending slot then
    shows a "Sony's metadata only" bar with its own Apply, the format's mode: folder directly, exfat/ffpkg in place,
    the rest rebuilt); the confirm dialog says the metadata is downloaded first. Core (`metadata::run`): stages
    `download` and then the edit's (`Ctx::fix_stages`, so the edit pipeline it hands over to announces none); the
    changed files written to a temporary folder (`metadata::Temp`, `forge-metadata-<pid>-<n>` in the system temp
    folder, on the PS5 `/data/dumpforge/tmp`; deleted when the job ends, and `metadata::clean_stale` at every
    `Jobs::new` deletes one whose process is gone: unix `kill(pid, 0)`, elsewhere one older than a day), the game
    re-read just before the edit (changed meanwhile: refused), then Sony's `Edit::Add`s followed by the user's own
    edits (on one path theirs stays) run by `convert::convert` as one edit job: the mode the request asks for, an
    `.ffpfs`/`.ffpfsc`/`.pkg` rebuilt (`metadata::rebuilds`). Any failure fails the job: one before the edit (no
    internet included) with nothing changed, the edit's own by its rules (an image restored; a folder edit has no
    rollback and names what stays). Nothing differing: "nothing to restore", the user's edits still run. Job
    rows: `jobType` `edit`; alone, tag **Metadata** and error box "Can't restore the metadata:", with edits the
    edit's own. Edit locks the changes and reads the source again when it ends. Refused: a NAS or peer source, an
    archive, a patch, (PS5) `/mnt/shadowmnt`, an extract, transfer, install, LZ4, zstd, backport, replace or
    delete option. `edit --check` with `--restore-metadata` is refused (a check doesn't download).
  A build or edit without them makes no network request (checked with networking denied: `sandbox-exec`).
- Replace after confirmation (by user decision): transfer jobs (the Transfer screen), the phone upload
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
- Shell (by user decision, the redesign after kraken-mount): a side rail (`<nav className="rail">`, `App.tsx`) with the screen
  on its right; the rail lists Build, Queue, Transfer, Edit, Storage, Connect, Settings and About (`TAB_ORDER`, by user
  decision; Transfer has its own `transfer` icon; the About screen's id stays `formats`), collapses to icons (its
  toggle, kept in `forge.ui`) and under 700px becomes a bottom bar. No screen has a page header (by user decision:
  the rail names it; Queue's title is gone too, its counts lead its head). Every screen stays mounted (a half-filled
  form survives a switch); `#connect` opens Connect, else Build. Its foot: the Connect status (a button that opens
  Connect), the web build's offline tag, Stop (web build only; always asks first, even with no job running) and GitHub. kraken-mount's themes (`prefs.tsx`, `<html data-theme data-scheme>`): dark and
  light, each in nine schemes: cobalt (the default, also for an unknown value), abyss, ember, moss, dusk, rose,
  sand, volt and mono; Forge's tokens are mapped onto them in `styles.css` (ponytail: light-theme
  contrast not measured).
- Build (by user decision, Convert and Pack merged; `app/src/build/`): Source (no findings on its card: each is a Summary line, a blocking one a "Can't build" note; a source
  that can't be read, or that reads as no game (no title, no title ID: `inspectSource` throws its findings), an
  error card with Pick; a packed game's `zstd`/`LZ4` tag after its kind pill on every game card), then the steps in `STEPS`
  (`Build.tsx`: `packsStep` (Target), `backportStep` (Backport), `optionsStep` (Options); no step numbers), then
  the summary, the space line and one button. `steps.ts` is the
  contract (`BuildStep`, `defineStep`): `available`, `context` (facts for the later steps, `DeriveCtx`),
  `onSource` (a new source, or the page emptied after a start: choices that stick kept), `Body`, `request`,
  `summary` (`SummaryLine`s, `warn` or `muted`) and `findings` (why Build can't start); a new feature is one step
  file plus one entry in `STEPS`. Target (a small Compare formats in its head) has the Packs and Format rows of
  36px chips (`button.b-chip`, the level chips' size): Packs **None** then **More** (opened: **zstd**, **LZ4** and a small How pack
  works; only None for a patch or additional content); a green dot marks None and FPKG as recommended. Backport
  is its own card: the Remove backport switch, off with a muted line why when it doesn't apply (no backport;
  a packed game: unpack first). Options: Restore metadata online (a green Recommended notice), Delete the
  source, the Advanced fold with Full verification. The **packs rule** (by user decision, D8) keeps packs only when the chosen kind is the game's own
  (`PackMode`): a plain game None as is, zstd or LZ4 packed; a packed game None decoded (`lz4: "unpack"` or
  `zstd_unpack`), its own kind a plain request (`lz4: null`, packs byte for byte; into fpkg unpacked), the other kind decoded then
  packed. Default None + FPKG for every source. The format row shows Folder and FPKG (a pack's packed folder);
  **More** shows exfat, ffpkg, ffpfs and ffpfsc in the same row (More toggles on every click; closed, it still
  shows the chosen chip behind it); packs go into a folder, exfat, ffpkg or ffpfs (all shown at once, no More, while a pack is chosen)
  (`ZSTD_PACK_FORMATS`, `LZ4_PACK_FORMATS`), an unpack into any target (`TARGETS`). The compression level is
  shown as chips toned fast green, medium yellow, max red, stored gray: the pack preset (Fast / Medium / Max,
  `LEVEL_OF`: zstd 5/9/19, LZ4 0/9/12), FPKG's Kraken level; ffpfsc's zlib level is a slider. The **Advanced** fold (`prefs.tsx` `Advanced`) holds only the chosen format's
  or mode's own options: ffpfsc's inner image, a pack's custom TOML rules (`RulesPick`, any device, in memory
  only, sent as `pack_profile`), full verification; it is gone when nothing applies, and Settings' "Always show
  advanced options" (off by default) shows every pack and format chip (no More, no How pack works) and keeps
  every fold open. Delete the source is shown, not folded. No
  description text (explanations in tooltips): blocking findings, the summary (what the job does to packs,
  the runtime (an FPKG: left out), the index (not for an FPKG: its builder indexes), the unsigned executables,
  verification), the space line, the result panel, and one warning stays: Delete the source. The space line: the output folder's
  free space (`api.freeSpace`, 300 ms after it or a finished job last changed; `null` unknown) against about what
  core's preflight asks (`neededBytes`: the game's bytes, decoded for an unpack, laid out as the format's writer
  sizes its image (64 KiB per file tail and per folder), + 64 MiB; fpkg, ffpfsc and packs (folder or image) by
  their uncompressed worst case, so the estimate never lowers it, `build.spaceRaw` saying so), warn when short. The result panel beside the choices
  (by user decision a gauge, no percentage tags: "Game now" and the source's size on disk, `Inspection.file_bytes`
  (else its files), then "About, after the build" and the build's size, the track's fill the build's share; a source
  into its own format shows its current size, no estimate; "custom rules: no estimate" with a profile): core's `pack_estimate` (`api.packEstimate`, 500 ms
  after the choice last changed, only the latest answer lands) for a pack (zstd, LZ4), an fpkg (Kraken) or an
  ffpfsc (zlib). The button reads Build, or **Add to queue** while another heavy job is unfinished. Once a job
  starts the page empties for the next game (the steps' `onSource`) and the job shows in the Queue. The
  request matches what Convert and Pack sent for the same choices, plus `replace_ampr_emu` from Settings.
  Routes: server `POST /api/free_space {path}` → bytes or `null` (confined on the PS5 as `list_dir`; FTP `null`),
  Tauri `free_space {path}`; server `POST /api/pack_estimate {source, kind, level, password?}` → `PackEstimate`
  (`kind` zstd/lz4/kraken/zlib; the 2-read cap; 400 with the error's text), Tauri `pack_estimate`; `GET
  /api/session` `bundled_ng_version` and Tauri `bundled_ng_version` (Settings).
- Edit screen (after Build; it replaced Inspect, by user decision; `app/src/Edit.tsx`): left Build's game card
  stacked (`SourceCard stacked`, by user decision: the cover above the text in its 1fr:2fr column from 1100px,
  the same chips and tiles, findings; folds Game facts, param.json, Details; Delete in its head,
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
    undo; close the game first; an interrupted edit is restored first) and starts the job. A rebuild says how
    long it takes before Apply (issue #43, `EditApply.tsx` `RebuildTime`, from the game's size: a `.pkg` at ~26 MB/s
    on a PS5, another image at ~60 MB/s on a PS5, ~200 MB/s on a computer; ponytail: fixed rates, English only).
    While the Apply runs (uploads, the last check) the changes, the source pickers, Delete, Extract and a Storage
    hand-off are locked (a pick or hand-off isn't opened, a note says so). Once its job starts the page resets (no
    source, no changes) and the job runs in the Queue (by user decision; core still refuses a second job on one
    image); the web build's staged uploads are tracked outside the page (`stagedJobs`) and discarded when the job
    ends. Opening a source (another, the same again, a Storage hand-off) drops the pending changes without asking.
    An edit job of the source open now (an earlier Apply, a reload's, another viewer's) locks the changes until
    it ends; then the source is read again, the changes kept. Job rows: `.tag.edit` **Edit** or **Edit ·
        rebuild**, the error box led by "Can't apply the changes:"; a cancelled job's error that starts
    "cancelled; " (what stays changed) is kept and shown as cancelled. The CLI's `edit` (PR #69, by user decision) sends the same request.
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
    repair. A `.pkg` rebuild always leaves the journal, logs and old index out (the builder indexes) and the
    runtime out (the fpkg exception, D7), as a conversion into `.pkg` does.
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
    not a finding (`GameInfo`/`Inspection.additional_content`; without param.json or a
    title ID it is still `preflight::NO_EBOOT`). Every page labels it with a violet **Additional content**
    `.tag` beside the kind/DLC/packed labels (Edit's card, Build's source card); an archive's game root falls back to a param.json-only one
    (`archive_game_root`) when no game root exists.
    Every edit, in every mode, applies to it, Extract too; a conversion goes only into `pkg` or `folder`
    (else `preflight::ADDITIONAL_CONTENT_TARGET`; Build offers only Folder and fpkg, default fpkg (`AC_TARGETS`),
    and only the None packs chip), its param.json category not checked. The `.pkg` builder no longer
    refuses it (vendor patch 0019; `content_class` gives 0x21 when param.json has neither
    `applicationCategoryType` nor `applicationDrmType`, else the game classes 0x20/0x26; package.rs notes "packaged as additional content (DLC) or an update").
  - **Patch packages** (user request, from issue #7): a `.pkg` starting `\x7FLIH` (a debug patch; layout
    measured on one, PPSA23566 01.006, no public spec) is a 64 KiB header with `{offset, size}` pairs for
    the patched package's FIH block (`0x20`), its CNT (`0x30`) and a stored zip of PlayGo/relocation tables
    (`0x40`, unread), and between FIH and CNT the outer blocks the update changed. FIH and CNT describe the
    whole patched package, so `lih::View` places each stored block by its SHA3 in `imagedigs` and serves
    that package; any other block is the base game's (`IN_BASE` error). Its layout descriptor carries
    `00 00 06` where plain builds carry `00 00 04` (any other value skips the records' nibble checks; upstream's `describe` since v12.6.30). The tree walk skips
    directories and inodes the patch doesn't carry, and only files every block of which it carries are
    listed (the container's always); `details` names what is left out. `Inspection.patch`: an orange
    **Patch** tag on the source card; Edit browses and extracts only (`edit.patchNote`; edits refused in
    both modes); Build and every conversion refuse it (`preflight::PATCH_SOURCE`, Build's
    `patchBlocked` with **Open in Edit**; a conversion refuses it before opening it, since opening reads
    and hashes the whole patch); its root isn't judged (no findings, never additional content); `lz4`
    facts `None`. Applying a patch
    to its base needs the base package's own blocks (a dump has none of them): not done.
- Storage (the screen after Edit, `app/src/Storage.tsx`; by user decision the Library view and its backend
  are gone: no game scan, no covers): on a computer the File explorer alone; on the PS5 one card (design "D")
  whose head is the view switch (`.seg.view-switch`, `role="radiogroup"`, smaller than the rail's items so it
  never reads as a second navbar): **[folder] File explorer** (the default) | **[package] Package
  installer** (below), then the view's actions (under 400px the labels are visually hidden, still the radios'
  names); each view owns its card and head, a copy of the switch in each; the view and each device tab's
  folder live in memory only; every view stays mounted. Its install jobs show in the
  Queue. On the PS5 each view's head shows the internal storage first, before Refresh (`FreeSpace` in
  `common.tsx`): "Free 312 GB of 825 GB" (decimal GB, whole numbers, one decimal under 10 GB), from `POST
  /api/storage` → `{free, total}` (`statfs` of `/data`) or `null` (a computer, or `statfs` failing), renewed
  with each listing (by user request). Its reads (a package scan, inspections, naming, `host_tree`,
  `edit_check`, `pack_estimate`) share the server's cap of 2 concurrent source reads: a read waits for a slot
  first, at most 4 waiting, 429 when it stays full for 30 s, 409 once stopping.
  - **Roots** (the Package installer, the PS5's explorer): the server's `Roots::current()`: `/data` and each
    drive while mounted (the browse roots; no `/mnt/shadowmnt`), classified by FreeBSD `statfs`
    (`MNT_LOCAL` and ufs, msdosfs, exfat, exfatfs, bfs, nullfs, cd9660, udf, zfs). Then `/user/app`, the installed
    games (issue #78, by user decision): listed and read (an installed FPKG's `app.pkg` a Build or Edit source; a retail
    one refused by the reader), served only while it resolves to itself, never reached by a paired device
    (`reach`/`above`) nor scanned by the Package installer (`Roots::writable`); the UI lists it only while Settings'
    "Show installed games" is on (`prefs.tsx` `shownRoots`, PS5 listings only); Build's output for such a source
    defaults to `/data` (`target.tsx`). Read only (core `edit_job::installed_inside`: at or inside
    `/user/app` or `/mnt/<drive>/user/app`, as typed or resolved; `installed_here` only where jobs run on the PS5):
    delete, move and replace (`delete.rs`; also a folder holding an existing one, `installed_holder`), edits and metadata recovery,
    every job output (`preflight::output`, extract, transfer), install and `out::ps5_refusal` (peer writes, phone and
    Edit uploads) refuse it; Delete the source keeps it. ponytail: a missing `/mnt/<drive>/user/app` can be created by
    a folder named `app` (nothing installed there to lose).
  - **Refused folders** (core `folder_refusal`, the desktop app's listings, deletes and moves; one list,
    `library_roots::refusal`, checked on the canonical path, ASCII case-insensitive on macOS and Windows):
    the filesystem root (Windows: the system drive root), `~/Library` / `~/AppData` and everything inside,
    and each OS's system folders with everything inside (macOS `/System`, `/usr`, `/private` except
    `/private/tmp`, ...; Linux `/proc`, `/usr`, `/run` except `/run/media/…`, `/var` except `/var/mnt/…` (Fedora Atomic's `/mnt`), `/sysroot`, `/efi`, `/nix`, ...; Windows, Program Files,
    ... on the system drive). A path at or inside the canonical home is allowed first (a `/var/home/me` or
    `/root` home works). Everything else is allowed (home and its children, `/Volumes`, `/media`, `/mnt`,
    volume and drive roots). Always `None` on the PS5.
  - **Package walk limits** (`library.rs`, `scan_packages`): breadth first, links and network mounts never
    entered; depth 6 (root 0), 20,000 directories and 200,000 entries per root, 10,000 entries per
    directory, 256 package opens, 2,000 packages, 15 s budget; skip names, the home's app-data folder, `.part`, non-UTF-8 names
    excluded; hitting a limit sets `truncated` (the UI says the list may be incomplete).
  - **Hand-offs**: the File explorer's Edit and Convert (Build) hand a path to `App` as a `SourceSelection
    {id, path, kind}` (one per screen; the screen switches and loads it; Edit refuses it while changes are
    being applied); its Delete uses the same `confirmDelete` (`app/src/delete.tsx`), `delete_path` and
    refusals as Edit, then `notifyDeleted`, which Edit, Build and the File explorer all hear.
- File explorer (the Storage screen's first view, `app/src/Files.tsx`): Edit's explorer (classes, crumbs, Up,
  filter, `role="grid"`, the same keys, at most 2,000 rows) with no pending list, no Apply and no note: every action
  happens at once, a destructive one after the question. From 880px two panes (by user decision): a folder
  tree ("Folders") beside the list, the chain from the root to the folder shown, each folder on it with its
  subfolders (`list_dir`, lazily, one folder at a time from the root down; kept while the view shows and
  listed again, still shown until the answer, on Refresh, on showing again and after a move or a change
  outside the folder shown; dropped only for a new root, folders off the chain pruned), the current one
  `aria-current`, any opened by a click (focus stays in the tree; one tab stop, arrows/Home/End move); the
  crumbs hide there (Up stays). Below 880px no tree. Root: the tab's device's (Device tabs, below): a peer's
  share root, a NAS's `nasRoot` (an SMB or NFS root lists shares or exports: nothing there changes), on the
  PS5's This device the server's browse roots (`list_dir` `null`; nothing to change there). Row actions: Rename (pencil; inline `NameInput`, F2),
  Move to (the folder picker from the folder shown; `<picked>/<name>`; its own folder refused; for both a
  name of the same kind already there is replaced, a folder merged, once the user confirms (Replace,
  above); `NameInput` still refuses one of the other kind, a link or a mounted image), Delete
  (`confirmDelete`, its file, image and archive variants by extension; Delete key, "fn+Delete" in a Mac's
  tip), a ⋯ menu (`ChooseMenu` with `buttonLabel`) of Edit and Convert for a
  folder or an image/archive by extension (the `SourceSelection` hand-off, Storage above), open a folder. A rename or move also
  `notifyDeleted(from)`. Links: a **Link** tag, opened when they point at a folder, never changed (core gives
  a link's resolved target as its `path`; the row keeps its own path, the target as `target`, which only
  opening uses). One action at a time: its row keeps focus (`aria-disabled`, "Deleting <name>" for a
  delete), can't be opened and its ⋯ is off; every other row's actions are locked. Listed again after each
  action (a failed one too), when the view shows and on others' deletes (one heard during this view's own
  action waits for its end; a folder on the chain that went is left). The grid stays mounted while a folder
  lists (a spinner only past 150 ms); Refresh is `aria-disabled` + `aria-busy` until its answer.
  On the PS5 a `.pkg` row (not a link, not at the roots) has **Install** (`installHere`: the install question,
  then an install job, below). **Device tabs** (by user decision; `FilesTabs.tsx`, drawn by `DeviceTabs.tsx`):
  one tab per paired Forge (a PS5 or a computer, `flink://<key_id>`) and per saved NAS (its `nasRoot`, its
  SMB/FTP/NFS tag); on the PS5's page **This device** first; a computer has no tab of its own (by user decision:
  its files are the Transfer screen's left pane), and with no device online an empty panel with **Open
  Connect**; the strip's **+** opens Connect. An offline device's tab is greyed and can't be chosen; the first
  online tab is chosen, and stays so (a device back online doesn't take the screen back). Each tab is its own
  explorer kept mounted with its own folder while its device is saved. A peer's tab renames, moves and
  deletes there by that device's own rules (`delete_path`/`move_path` RPCs, below), a NAS's through core's
  NAS calls (Connect's Saved NAS devices, below); Move to's picker lists peers only on a peer's tab, peers and
  NAS on a NAS's (a move stays on one device, a NAS's on one share). Bookmarks (`bookmarks.tsx`, as on the
  Transfer screen's panes, below): a bookmark toggle at the end of the crumbs bar and the chips under it. Drags (mouse pointer events, ponytail: no touch; Move to does it there):
  a row onto a folder row of its own tab moves it there. Nothing copies from here (by user decision): no drag
  onto another device's tab, no system drops (the Transfer screen, below, copies and takes them).
  Core `list_dir(path) -> Listing {path, parent, entries: [{name, path, dir, size, link, mounted}],
  truncated}` (`listing.rs`, at most 10,000 entries; links resolved, a dangling one as itself);
  server `POST /api/list_dir` `{path}` (`null` lists the roots; `parent` `null` at a root; on the PS5, by user
  request, a path not at or inside `/data` or a mounted drive, resolved, is 400, as for `host_tree`:
  `paths::Roots::confine`; a link whose target is outside them is left out of a listing);
  Tauri
  `list_dir {path}` (`parent` is the filesystem's: the UI stops Up at its root), `move_path` and
  `delete_path` refuse (`from` and `to`) a refused folder (core `folder_refusal`, Storage above,
  its message).
  Delete and move refuse an undo journal (`*.forge-undo`) while its image is there, and move refuses
  a new name that would read as one ("open that image in Edit"). Paths are `dunce`-canonical (Windows `C:\x`, not `\\?\C:\x`; server roots too).
  The explorer sends `explorer: true` (the Picker doesn't): the server's roots are the same and core's
  `list_explorer` lists; on the PS5 it hides ShadowMount+'s mount
  folder (a path in it is 400, entries resolving into it left out) and marks `mounted` images (a
  **Mounted** tag, and once above the list a note saying why; Edit/Convert (Build) only). Elsewhere it is `list_dir` (Tauri ignores the flag).
- Package installer (the Storage screen's second view, the PS5 only, `app/src/Packages.tsx`): every `.pkg` under the
  server's current roots (any case), scanned on the first visit and on Refresh, nothing kept; core
  `scan_packages(roots)` (`library.rs`: the package walk's limits above, game folders entered too; each
  package read only by `fpkg::peek`, its header and container, a few KiB, a patch's blocks not placed) →
  `PackageScan {entries: [PackageEntry {path, size, content_id, title_id, title_name, title_names, version,
  content, has_cover}], truncated}`, `content` `game`/`patch`/`additional_content`/`unknown` (unreadable, or a
  content type Forge doesn't know: listed untitled, the console judges it), sorted by title, untitled last;
  server `POST /api/package_scan {}` (the 2-read cap, 409 once stopping, 400 on a computer; the app has none).
  Rows (a package icon, no cover read): title (else title ID, else file name), title ID, version, size, a
  content tag (Patch orange, Additional content violet), the path, **Install** and right of it a trash button
  (by user request: `confirmDelete`'s file variant and `delete_path`; the row leaves the list, no rescan);
  the head shows the free space, Refresh and a Cards / List toggle (List the default, memory only).
  The File explorer's Install on a `.pkg` row (PS5) shows the package icon, as Connect's Install pkg and the Transfer screen's Install on <PS5>.
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
  installer, the File explorer's `.pkg` row, the Transfer screen's **Install on <PS5>** (below) and Connect's
  **Install pkg**: an online paired PS5's row in the Devices list picks a `.pkg` on this device (inside its
  share root) and asks that PS5 (Tauri `connect_install {keyId, path}`, server `POST /api/connect/install
  {key_id, path}` → `{job}`; FLINK RPC `install {path}`, relative to the caller's share root, plain names
  only, answered only where the PS5's server set `Connect::set_install`, else `this device can't install
  packages`), which reads it over FLINK; the job is mirrored here. A `path` of `flink://<that key_id>/<rel>`
  (a package on that PS5 itself, the Transfer screen's right pane) sends FLINK RPC `install_here {path:
  rel}` instead: plain names, located as the file RPCs' paths (`connect_fs::local_file`: inside the share
  root, on the PS5 `Roots::reach`, not `/mnt/shadowmnt` (`out::refusal`), not the root, a link or a folder),
  then installed there by its own install job (not mirrored: no `flink://` path in it); an older PS5
  answers `unknown method`: `INSTALL_OLDER` ("that device's Dump Forge is older: update it to install a
  package that is on it"). Job rows: `jobType` `install`, tag **Install**, the package as "from", no format pill, error
  box "Can't install:"; listed in the Queue, a peer's (mirrored) too.
- Connect (the screen after Storage, `app/src/Connect.tsx`; server `connect.rs`, also run by the Tauri app at
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
  protocol's). Read in Build, Edit (browse and Extract only, muted `edit.nasNote`, no Delete), inspect,
  covers, transfers and `list_dir` (`smb://<host>` lists its disk shares, `nfs://<host>` what is above its
  exports); folders and `.exfat`/`.ffpkg`/`.ffpfs`/`.ffpfsc` read in place (each read re-checks the file's
  size + mtime, the `SourceStamp`; no identity: ponytail). Written by Build (an image as a `.part`
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
  is added from another device; the UI's Add lists peers only; `net::refuse`: "a game on a NAS is read only: ... needs
  a local copy"). **Managed from the File explorer** (by user decision; its NAS tabs, above): core's
  `Jobs::delete_path`/`move_path` take a NAS path (refused while one of this device's unfinished jobs uses it at,
  in or above, an SMB path case-folded) to the adapter's `Remote::delete_path`/`move_path`: never a share, an
  export or the server itself (`delete::nas_root`: SMB below a share, NFS strictly inside an export, FTP not
  `/`); a delete (`delete::nas_delete`) removes a file or link, or lists a folder all the way down first (a
  failed listing, an unaddressable name or more than 1,000,000 entries refuses it before anything goes; links
  and special entries removed, never entered; the top path's kind read from its parent's listing, since an
  SMB or FTP stat follows a link; an SMB reparse point (`FILE_ATTRIBUTE_REPARSE_POINT`) is a link, and SMB's
  delete opens take `FILE_OPEN_REPARSE_POINT`, so the link goes, never its target; `Remote::fs_id`, an NFS
  `fsid` (v3 fattr3, v4 `FATTR4_FSID`), refuses a path whose filesystem differs from its parent's (an NFSv4
  export root: v4 names no exports) and a walk that would enter another one, a move of one too), then takes it
  apart bottom-up with the client's `remove` and `rmdir`, a failure part way saying "N of M entries were
  deleted before it"; a move (`delete::nas_move`)
  stays on one share or export (SMB share ASCII case-folded; else `nas_other_share`, "a move stays on one
  share"; FTP one server), `to` in an existing folder, `rename_no_replace` unless `replace`, then only a file
  over a file (`rename_replace`): no folder is merged on a NAS (by user decision), a link or a kind mismatch
  refused; a case-only rename goes through a temporary name. ponytail: a server presenting a link as a folder
  (Samba following a link on its side, an FTP server without MLSD types) has it entered; a change between the listing and the removal isn't seen. Delete
  the source never deletes a NAS source ("delete it in the File explorer"). The Transfer screen lists every
  saved device. Test servers: Docker only (above,
  `tests/`), core `tests/smb.rs`/`tests/nfs.rs` gated by `FORGE_SMB_TEST`/`FORGE_NFS_TEST`, `tests/ftp.rs`
  on the crate's fake server.
  **Pickers** (by user decision; `Picker.tsx`, every source and output picker but the File
  explorer's and the Transfer screen's panes): while a paired peer or saved NAS is online, one modal with a tab per
  device, this device first, then each online peer (`flink://<key_id>`) and each online saved NAS
  (`smb://`, `ftp://`, `nfs://<host>[:port]`, `nasRoot`), for sources and outputs alike (on another device's tab archives and `.pkg` files are listed greyed out,
  not pickable, issue #20; one short note above the list, `picker.netOnlyHere`); Edit's Add lists peers only (`peersOnly`). No system dialog on any platform (by user decision; `tauri-plugin-dialog`
  removed): the desktop's local tab lists Tauri `local_roots` (home and the volume folders, Windows A: to Z:, Linux
  also every disk filesystem in `/proc/self/mountinfo` (issue #52; ponytail: named by its folder, not its label), each
  folder once at its real path, that open and that `list_dir` doesn't refuse; existence only, one thread each, 1.5 s for all; ponytail: a
  stalled root's thread is left running), a root listed only when opened. A source is one **[disc] Pick** button (by user request; Build, Edit) in the
  `folderToo` mode: a file click picks an image or archive, "Choose this folder" a game folder, told apart
  by extension. Rows: at most 2,000 shown, one Tab stop, arrows move; multi-select's button reads
  "Choose N files"; a picker starts where the last browse ended (kept until the page closes).
  Pickers show this device's bookmark chips and toggle on its tab (issue #76: with no device paired no other place pins one). **Bookmarks** (issue #18; the
  pickers' old shortcuts, renamed by user decision; `bookmarks.tsx`, `devices.css`): toggled on the Transfer screen's panes, the File explorer and this device's picker tab. A
  bookmark toggle (`BookmarkToggle`, `aria-pressed`, "Bookmark this folder" / "Remove bookmark") at the end of
  the crumbs bar pins the folder shown (any device's, a root too; at 9 a new one takes the place of the oldest
  that pane can't reach, an offline or other device's; off, `aria-disabled`, when all 9 reach it, "9 bookmarks
  at most"), kept in `localStorage` under the old key `forge.shortcuts` (so saved ones carry over; at most 9,
  newest last, an older list over 9 cut on read; unreadable or invalid storage is none; one store, other
  windows heard); under the crumbs a row of accent chips (`.bm-chip`, the folder's last name, the device's
  name for its root, the folder shown bold), each with an x, shows each one only where that pane reaches it
  (`reachable`: this device's local paths, else a path within the pane's device root) and opens it.
  Nothing online: the same picker without tabs (the web build with each saved NAS as a root, outputs too
  (ponytail: an offline one included); the desktop app has no NAS source button (removed by user request): a
  NAS game is picked on its tab while the NAS is online).
  **A peer's files** (by user decision: streamed, never copied first): `flink://<key_id>/<path>`, relative to the
  peer's share root (`/`; a Windows PC its home folder's drive, by user request; on the PS5, by user request,
  only `/data` and mounted drives within it: `/` lists `data`, and `mnt` while a drive is mounted), read in place like a NAS (core `remote.rs`, `net.rs`;
  `connect_fs::ConnectRemote` registered with `core::remote::set` by `Connect::start`, server and Tauri alike):
  Build, inspect, covers, `list_dir` (`flink://<key_id>` is the share root), Edit's browse, Extract and
  Add, transfers. Outputs may be on a peer too: an image as a `.part` there (`RemotePart`), verified by reading
  it back over FLINK, renamed no-replace; a folder (a conversion, LZ4 pack, Extract, a transfer) written in place
  on the peer, each file through an exclusive temporary name (ponytail: no rollback, the error says what
  stays); an fpkg as a `.part` there too (by user decision, streamed: vendor patch 0020's `PkgOut`, core
  `RemotePkgOut`; the same onto a NAS; its clones read ahead for the self-check's and verify's small reads:
  up to 4 windows per clone, 256 KiB doubling to 4 MiB as a read continues one, least recently used
  dropped, every window dropped by any write, a read over 4 MiB served and not kept; over FTP each read is
  a data connection, and a 10k-file package's verify took ~500 s without them; ponytail: windows per clone,
  one lane at a time). Nothing of it is stored on this device's disk (by user decision):
  the builder's Kraken spool is the output itself (`KrakenSpool::InPlace`; a production request ignores the
  spool-folder variable), and vendor patch 0022 digests each compressed block (SHA3 + CRC32C) in memory as
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
  `sync`, `close`, `reopen {path}` → `{handle, size}` (a closed, unrenamed file of the session open for writing
  again, counted as an open file; one that is open stays as it is, which is how the writer learns the peer can;
  an older peer answers `unknown method`: the writer keeps the file open), `rename` (`replace: true`: over a regular file only, never a folder, link or special
  file, nor another peer's unrenamed file, and refused as this device's own replace is
  (`Connect::set_replace_check(jobs.replace_check)`: jobs, protected paths, ShadowMount+, an undo journal);
  an older peer ignores it and refuses the taken name), `remove`, `free` (on the PS5 where its folder
  leads must be reachable too), `delete_path`/`move_path` (above), `install` (Install, above), every path confined to the share root (`join_checked`, the
  parent canonicalized inside it, then on the PS5 `Roots::reach`: `/data` or a mounted drive, `/` and `/mnt`
  for stat and list only; all in `locate`), links never followed, special files and non-UTF-8 names marked so the
  reader refuses them); writes only to files this session created (exclusive, then a no-replace rename, or with
  `replace` one over a file), at most 32 of them open per session and 64 on the device (a file closed and not
  yet renamed doesn't count, up to 8,192 unrenamed per paired device: issue #42, a pack's volumes are closed once
  written, `RemotePacks::finish`, reopened for their headers and renamed by `publish`; against an older peer
  the volumes stay open, so a pack of more than about 30 still fails there),
  unrenamed ones kept 60 s (`KEPT_FOR`) once the session ends, then removed (at once when the service stops
  or the pairing is forgotten here); `reattach {path}` → `{handle, size, open}` moves such a file (kept, or
  still held by another session) to the calling session, for the pairing that created it only, once its
  writes in flight end (a per-file gate; the old session's later writes are refused); a path its own unfinished jobs use is refused ("in use by a
  job on <name>"; `Connect::set_busy(jobs.uses_hook())`), so is (PS5) one at or inside `/mnt/shadowmnt`
  (`core::out::refusal`; ponytail: no destination probe per peer write), and its jobs refuse a path a peer is writing
  (`Jobs::set_external_busy(connect.writing_hook())`); Delete and Move refuse that path too (a move's
  source and destination). This side keeps one control session per device and
  up to 8 data lanes each for reads and writes (8 MiB chunks, each BLAKE3-checked; a write sends 64 MiB, a
  sequential read fetches a window ahead that doubles up to 64 MiB (`RemoteReader` from 1 MiB, `RemoteTree` from the read's size: issue #36, a packed file's 2 MiB batches no longer pull 64 MiB each), each dealt to the lanes; one session
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
  bytes at once"). A speed lane's write timeout is `SPEED_WRITE_WAIT` (4 s; `Conn::set_write_timeout`), not
  `DEAD_AFTER`: a client cancelled with a chunk on its way can end in FIN_WAIT2 with a zero window (no read, no
  reset), and the send, tried twice by `write_all`, then frees the lane's slot and bytes after up to 8 s, not 24.
  `SPEED_MAX` (the `speed` RPC's `max`, the client's clamp) is `SPEED_BYTES` per direction.
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
  10 s without a push or on Forget) and shown there read only, in its Queue (`snapshot`'s `remote_jobs`): its user may only cancel
  it (`connect/cancel_job`, Tauri `connect_cancel_job`), answered in the next push's reply.
  **Transfer screen** (by user decision it replaced Connect's File Transfer modal; the rail's item after
  Queue, `app/src/Transfer.tsx`, `TransferPane.tsx`, `transfer.css`; kept mounted): browser-style device tabs
  (`DeviceTabs.tsx`, by user decision; WAI-ARIA tabs, one tab stop, arrows/Home/End; too many shrink, then
  scroll sideways), one per saved Forge and NAS (its platform icon or SMB/FTP/NFS tag; an offline one greyed,
  not choosable) and, in the web build but not the console's own browser, **This browser**; the chosen tab
  joins the card under it (`.dev-window`); the first online device is chosen until the user chooses (one gone
  offline stays chosen, greyed; one no longer saved gives way to the first online); the strip's **+** opens
  Connect; none online: an empty panel with Open Connect. Connect's **File Transfer** and a device's **Send**
  open the screen (`App.tsx` `openTransferTo`, Send with that device's tab chosen). The card holds two panes side by side (stacked when narrow), **This device** left (the app's home
  folder, the web build's server roots) and the device right (`flink://<key_id>`, a NAS's `nasRoot`), each
  listed as the File explorer lists (crumbs with the bookmark toggle at their end, bookmark chips, Up, Refresh,
  filter, at most 2,000 rows) with a checkbox per
  file and folder row: a batch kept while the pane moves between folders (Clear; links, mounted images and an
  SMB/NFS root's shares or exports not selectable; a peer's or FTP server's root is a folder like any other; another device chosen, or it gone offline, drops its side's
  batch). A side's action bar: **Copy N items to <other side>** into the other pane's folder shown (off, its
  title saying why, while that device is offline or the pane shows no folder) and, for a batch of only `.pkg`
  files, **Install on <PS5>** (the install question per package, a Cancel stops the rest; on the PS5 an
  install job here, which reads `flink://` and NAS paths itself; on a computer with a paired PS5 chosen,
  `connect.install` on that PS5, a package of its own by `install_here`: Install, above). A row (or the batch
  holding it) dragged onto the other pane copies it into the folder row under the pointer, else the folder
  shown (`useRowDrag` from `FilesTabs.tsx`, `transferDropTarget`; the ghost reads "N items"); in the app,
  files dropped from the system onto a pane copy there the same way. This browser's pane (web build): files
  dropped or picked are uploaded into the left pane's folder shown (only files; dropped folders left out and
  said), and **Download N files** saves the left pane's selected files (below). One action at a time; a
  status line says what started, with Open Queue, or why not. A copy queues a **transfer job**
  (`transferRequest`; `ConvertRequest.transfer`, core `transfer.rs`; peer to peer relayed through this
  device; stages scan, preflight, write, verify as Extract; the copy asks `existing_targets` first and a name
  there is replaced only once the user confirms (Replace, above); a nameless root refused; no rollback: "N of M files were copied before
  it and stay", or, with none finished, "<folder> was created and stays"; a peer's link is never read). Transfers (and installs) run in a second slot beside the one heavy job (convert, edit, pack, extract): at
  most one of each, each FIFO; a job whose paths are at, in or above an unfinished job's of the other kind
  is refused at start ("<path> is in use by another job", or "is being written by another device"). The
  old send/fetch/transfers/cancel/remote_list (routes, Tauri commands and their capability entries, the list) are gone (by user
  decision). Chunks of 8 MiB dealt round robin to 8 lanes on every platform (by user decision, as
  ps5upload; one big file uses them all), the PS5's sockets (FLINK and HTTP) at the largest buffers its
  kernel takes (`flink::tune_buffers`); a lost session resumes the file (above), nothing more: no resume after
  those 3 tries or 60 s, a restart or a Forget (ponytail; Retry starts the job again). Phones: the Connect screen's QR opens
  `<url>/#connect`; the web build's This browser pane moves the browser's files (not offered in the PS5's
  own browser, which can't pick files): files one at a time into the folder shown on this device (progress,
  Cancel; a reload or Back asks first; a failure
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
  plain `.small` **How to connect** in its head, as Build's How pack works and Compare formats), then (by user
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
  "Or add by protocol:" SMB / FTP / NFS). Right: Devices (in its head File Transfer (opens the Transfer screen), primary while a
  device is saved, and then Autodiscovery too; an All / Forge / NAS filter while both kinds are saved,
  "N of M online"; rows with Send for an online device (the Transfer screen with it chosen) and ⋯ for its details; empty: a panel with a
  primary Autodiscovery) and Nearby under it (Autodiscovery inline, no dialog: the finds not saved yet
  with Pair or Save, "Searching this network" and Stop while it runs; "No new devices found"). Details:
  a Forge's address, key ID, last seen and status with File Transfer (online only; the Transfer screen with it chosen), Forget and Close; a
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
  available: <error>". The rail's foot, on every platform, shows the state as a button that opens Connect: grey
  **Disconnected** or green **N devices** (online peers + online saved NAS); on the phone bar a status dot on the
  Connect item instead.
  Other devices by name (by user request): a source card and the Output label carry a green tag with the
  device (paired Forge, or a NAS's saved name) when it isn't this one (Output says **This device** while
  any device is saved), and every `flink://<key_id>` (and `smb://`, `ftp://`, `nfs://` address of a saved
  NAS) the UI shows, paths and backend text alike (errors, job errors, checks, logs: `peers.ts` `readable`;
  `nasName` for the tags), reads "<device name>: /<path>".
- Jobs in the UI (by user decision): one Queue screen (`Queue.tsx`, `JobsCard`, kept mounted) lists every job,
  this device's and the peers' mirrored ones (above, read only but Cancel), newest first; no other screen lists
  jobs, and only Queue's rail item counts the unfinished (queued + running) ones (`.count`, hidden from screen
  readers, which read "N active"). Head: running, queued and failed counts; **Retry failed** (each failed row
  started again one after another, the old rows kept as the per-job Retry keeps its row, D4; a row it retried
  isn't retried twice); **Clear finished** (this device's only); filters All / Running / Failed / Done. No reorder
  (D6). By type (`jobType`: install, transfer, edit for edits and Extract, pack for any LZ4 or zstd pack or unpack
  (`packJob`), else convert) a row is a tile in the app's black (`--inset`, by user decision; no edge ring) with its
  type's tag: Edit blue (`.tag.edit`), Pack (`.tag.lz4`), Transfer and Install green (`.tag.transfer`,
  `.tag.install`; "Can't transfer:", "Can't install:"), a peer's job a green tag with its device.
  Progress (after issue #70's mockup by contributor dark2y; credit them in the release notes, D5): a segmented
  bar (`SegmentBar`), one segment per `job.stages`, weighted (`STAGE_WEIGHT`: compress, write, install 4; measure,
  pack 3; verify 2; others 1), a breadcrumb of stage names (`Crumbs`: ticks for the done ones, the current one bold;
  a failed job keeps it with the failed step crossed out) and a status line "<stage> 46% · compress 62.3 MiB/s
  · 2 h 9 min left · 14:48 in this step" from core's `stage_done`/`stage_total`, `bytes_per_s` and
  `stage_started_ms` (`stageFraction`; an older server without them: guessed, byte stages by weight, others
  easing below 95%). Finished: "Done 58.1 MiB, 89 files → <output>" (`ResultLine`), one muted line with the type,
  the size ratio (`JobReport.source_bytes`) and the time (`started_at_ms`/`ended_at_ms`) (`ResultMore`), then
  the Verified fold, the source-deleted line and the log fold; **Copy details** in the job's head
  (`CopyDetails`). The log panel (`Log`): line count (of the 500-line tail the page keeps), filter, auto-scroll,
  Copy, Save (the app copies instead) and Clear (hides lines up to the last one shown, by its text; ponytail: a
  repeated line hides too much). A failed job's row has **Retry** (by user request; `JobList.tsx`
  `canRetry`): it starts the same request again as a new job; not for a cancelled one, a peer's mirrored job
  or an edit that adds a browser's uploads (their staging folder goes when the job ends; Edit resets once its job
  starts, so any other failed edit is retried here, preflight checking the changes again); an Extract or a transfer asks about names
  already there again first (`askReplace`, Replace above), since a failed copy leaves files behind.
  `App.tsx` renders the screens memoized (`BuildScreen`, `EditScreen`, `StorageScreen`, `ConnectScreen`) with
  stable handlers; Build and Edit get `useSettled(jobs)`, the list kept by identity until a job's id, request or
  result changes, so progress and log events re-render only the Queue (`JobCard` is `memo`).
- Names are never renamed: bad names (non-NFC, exFAT/PFS case collisions, forbidden chars, non-ASCII
  in PFS) fail preflight with every offender listed.
  One exception (by user decision, upstream's finding: four Unity titles crashed a second after start from
  a folder on /data or a `.ffpkg`): into a folder, an LZ4 folder or a `.ffpkg` (case-sensitive where the game
  runs from them) a module whose name differs from what `eboot.bin` loads only in letter case takes eboot's
  spelling (upstream's `module_case::CaseFixed`, applied in `convert` after the metadata overlay; a kept
  `ampr_emu.index` record respelled with it; log `case: N renamed to the spelling eboot.bin loads: ...`). The
  source is never touched; edits and edit rebuilds never rename.
- `.pkg` is built with `BuildRequest::production` only (plaintext `PPRPLAIN-NOAUTH!`, Kraken);
  installing needs kstuff + fpkg-enable + ppr-patch. Debug keys are embedded.
  The builder writes a fresh `ampr_emu.index` from the packaged files (a libSceAmpr title or a source
  with one; a stale index crashes the game; a case clash refuses), except for a packed source, whose own
  index is kept byte for byte: LZ4's runtime checks each record against the pack manifest by file id, and a
  zstd-packed source's index lists files that are inside its packs (upstream's `source::packed_kind`).
  So a plain `.pkg` of an unpacked AMPR title builds no index of its own first (`lz4::plain`): the old index,
  journal, logs and the runtime (the fpkg exception) are left out and the builder indexes what survives
  `sdk_rules::excluded`, so its index lists no runtime.
  A source without `eboot.bin` is built, not refused (vendor patch 0019; its content class: above).
  Vendor patch 0025 (by user request): dumper and tool leftovers left out (`entitlements.txt`,
  `entitlement_key.dat`, `decrypted/`, `.gp5-assets/`, `playgo-languages/`, `*/saved/config/*/*.ini`); the
  generated `playgo-scenario.json` lists the game's own languages (upstream: its `playgo-scenario.json`, log
  `languages as the game declares them: ...`), else those its `playgo-chunk.dat`'s chunk masks narrow to; past
  level 2 the size class is level 3 (`attributePub` 4 + `kernel.appSizeInGib` 256..320; ponytail: not yet seen
  installing); `warning:` log lines for a content ID off the PS5 pattern, a presentation PNG in the wrong color
  mode and a UDS `.ucp` that is sealed or has a stale SHA-1 digest.
- The UI names formats without the dot (`ffpkg`, `exfat`, `ffpfs`, `ffpfsc`, `FPKG`; `KIND_LABEL` in
  `common.tsx`), and the pack formats ZSTD and LZ4 in capitals (by user decision, every language; ids,
  request fields, file names like `ampr_zstd.index` and CLI flags stay lowercase; older lines here still write `zstd`), and in running text in bold (`Prose`); paths and file names keep the extension. The debug
  FPKG is labelled `FPKG`; its files, the `Format`/`Kind` id and the CLI flag stay `pkg`. Format copy and
  the About screen's formats table (`app/src/Formats.tsx`) follow ShadowMountPlus 1.7's README and release notes.
  About (by user decision, the redesign: the feature tiles are gone): Compare formats (a row per format in its hue: FPKG, zstd packs, LZ4 packed folder,
  ffpkg, exfat, ffpfs, ffpfsc, folder; each rating a `.chip` with an icon, never colour alone; Build's Compare
  formats opens it), then Credits on a card of its own. The page's QR code lives on the Connect screen (web build).
- Settings (the screen before About, `app/src/Settings.tsx`; by user decision): **Appearance** (theme dark /
  light, colour scheme (nine swatches, cobalt first), the UI language select: System + each language), **Advanced**
  ("Always show advanced options": every Build fold open; the bundled ampr-emu-ng's version, from the session's
  `bundled_ng_version`, Tauri `bundled_ng_version`; "Replace ampr_emu with ampr-emu-ng", on by default, sent as
  `replace_ampr_emu`; on the PS5 "Show installed games", off by default: Roots above), **Output names** (issue #33; the naming template, below) and, on the PS5 web build, the
  saved job-end notification switch.
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
  (exact tag, then its primary subtag) until Settings' selector (System + each language, every
  platform) does. Game titles come from param.json's `localizedParameters` (`title_names` in
  `Inspection`, `PackageEntry`; `gameTitle`): the chosen language, else the system languages, else
  English, else `title_name` (the default language, still used for sorting). Generated names' `{title}` is the English title (`localizedParameters` `en-US`, `en-GB`, then any `en*`, by user decision), else `title_name`.
  Every UI copy change updates every language; the UI says "game", never "dump" (except the product
  names). Russian writes trace terms as трейс/трейсы/трейсинг. Arabic is right-to-left (`rtl` in
  `LANGUAGES`: `<html dir="rtl">`; styles.css uses logical properties plus a few `[dir="rtl"]` overrides,
  paths stay LTR) with Latin digits (`numbers`), and its Latin file names and paths are wrapped in
  LRI…PDI (U+2066/U+2069) so the bidi algorithm can't reorder them. Durations are whole messages per
  language (`duration`). The server's PS5 notifications (started, port in use, stopped, can't
  close (jobs running), not taken over, a Convert, Pack or Transfer job done or failed (switchable in
  Settings); `crates/ps5-dump-forge-server/src/notice.rs`; by user decision a job's end is a system toast
  (`sceNotificationSend`, ShadowMountPlus's JSON shape, Forge's icon at
  `/user/data/dumpforge/notify-icon.png`), the plain notice its fallback, every other notice stays plain)
  follow the console's language
  (`sceSystemServiceParamGetInt` LANG, mapped to the UI's languages, else English); English elsewhere.
- No settings and no config file: the UI keeps choices in memory until it closes (Connect's pairings,
  above, are the service's own state), except the UI language (`forge.lang` in `localStorage`; unreadable
  storage falls back to the system languages), the bookmarks (`forge.shortcuts` in
  `localStorage`, the old shortcuts' key, issue #18; unreadable storage is none), the output naming (`forge.naming`, issue #33; unreadable or invalid is the default), the UI
  preferences (`forge.ui`, `prefs.tsx`, by user decision, D1: theme, scheme, collapsed rail, "Always show advanced
  options", "Replace ampr_emu with ampr-emu-ng", sent per request as `replace_ampr_emu`, and "Show installed games"; unreadable storage is the
  defaults) and the PS5 Settings notification switch (kept by the server on the console as `notifications.off`; by
  user decision). Defaults: Build's None + fpkg (`.pkg`) for every source (D8: a packed game is decoded unless its
  own kind is picked; picker order Folder, fpkg, then under More exfat, ffpkg, ffpfsc, ffpfs), exfat inside a `.ffpfsc` in the UI (by user decision; core and CLI keep ffpkg; Build's Advanced fold), at zlib
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
  test helper `run` holds every job to it (reported stages only move forward through the list). Each
  `Event::Progress` also carries the current stage's own span (`stage_done`/`stage_total`; verify one span across
  all its passes), `bytes_per_s` (measured in core over the last few seconds, `null` until known and for a stage moving no bytes) and `stage_started_ms` (epoch ms, so a
  reloaded page shows the real time in the step); `JobReport` carries `source_bytes`, `started_at_ms` and
  `ended_at_ms`. The Queue draws them (Jobs in the UI, above).
- The Tauri app has no HTTP sidecar and no `tauri-plugin-fs`; the capability grants only the app's
  commands (`build.rs` `COMMANDS` and `capabilities/default.json` list the same ones, removed commands such as
  `library_scan`/`library_cover` and the old `connect_send`/`connect_fetch`/`connect_cancel`/`connect_remote_list`
  gone from both) and event listening (no dialog plugin: every picker is `Picker.tsx`). The web UI is a separate build served by `serve`.
- Verification: fast by default everywhere (every structural check + seeded BLAKE3 sample of 8 MiB
  slices), full is opt-in (`full_verify`, `--full-verify`, the "Full verification" switch). A fast `.pkg`
  also samples the builder's own block sweep and `playgo-chunk.crc` check (the builder's `BuildControl::sample`, upstream's, with the job's seed).
- Updates (by user decision): every start checks GitHub, always on, no setting, nothing stored (`localStorage`
  keeps only `forge.lang`, `forge.shortcuts`, `forge.naming` and `forge.ui`); **Later** hides the toast for the session. `app/src/update.ts` (no runtime imports;
  `scripts/check-update.mjs`): `GET api.github.com/repos/quer3q/ps5-dump-forge/releases/latest` (10 s, no
  cookies, any error = no toast), `tag_name` `vX.Y.Z` newer than the running version (a prerelease is older than
  its release) and the release lists this build's exact asset (`ps5-dump-forge-<v>-<asset>`, `state`
  `uploaded`, `size` > 0). The toast shows only after a successful check that found an installable update; any
  failure (offline, DNS, HTTP, JSON, timeout) is silent: no toast, no error, no offline state. `UpdateToast.tsx`
  (bottom-right, bottom-left in RTL, full width under 600px; a hidden always-mounted `role="status"` holds
  its title; once per start; Later / Update; a failed update says why with Close / Retry; the app's, once
  the browser opens the download, says so with Close). Transport `updateTarget()` (`{version, asset}`, null = no
  updates) and `startUpdate(v)`. Desktop: Tauri `update_target` (`asset` the full release file name by OS/arch, `{v}`
  for the version: `ps5-dump-forge-{v}-macos-arm64.dmg`, `-windows-x86-64.exe`, `-linux-x86-64.AppImage`, ...;
  `assetName` fills it in) and `open_update {version}` (plain `X.Y.Z`; opens that exact file's URL in the
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
  reading or writing files here, always refuses: "Can't close Dump Forge - some jobs are still running."; details in `ps5/README.md`). `serve` on a computer (`host`): `GET /api/session` `cli_asset` (`ps5_dump_forge_updater::cli_asset()`), the same toast, Update opens that CLI file's link (`downloadingCli`); the CLI's own `update [--install]`.
- PS5 payload: one plain `.elf`, no PKG, no signing (updates: above, or replace the file). FreeBSD 11 ABI via
  `--cfg libc_unstable_freebsd_version="11"` (the env var is ignored by libc ≥ 0.2.187) and struct-size
  asserts in the CLI. `panic=abort`.
- The server has **no protections** by user decision: no pairing, token, Host/Origin/CSRF checks or path
  confinement (except, by user request, on the PS5: listings, `host_tree` and the peer
  side reach only `/data` and mounted drives). Don't re-add them (Connect's pairing guards FLINK on 9883 only, not the HTTP routes,
  `/api/connect/*` included). Its delete and move routes (`delete_path`, Edit's and Storage's;
  `move_path`, the File explorer's; by user decision) refuse only what exception 1 lists (`edit_discard` removes a staging folder by the same rules); the
  page's question is UI, not authentication. `edit_upload` writes only under a source's staging folder.
- The PS5 version is named "Dump Forge" (tile, notices, server texts, the web build's UI: `APP_NAME` in
  `api.ts`, the http page title); the desktop app is "PS5 Dump Forge". The CLI, crates,
  `ps5-dump-forge.elf` and release names keep `ps5-dump-forge`. Home folder `/data/dumpforge`
  (`FORGE_DIR`); `entry.c` moves an old `/data/ps5-dump-forge` there once and leaves a relative link at
  the old path (the package walk doesn't follow it; details in `ps5/README.md`).
- Tile `PDFG00001` "Dump Forge" (deeplink to `http://127.0.0.1:<port>/`), never written without our owner
  file `/data/dumpforge/tile-PDFG00001.owner` (the old-name owner record still counts; see `ps5/README.md`).
  Self copy: `ps5/build.sh` builds twice; the release ELF embeds stage 1 and saves it to
  `/data/dumpforge/ps5-dump-forge.elf`. Launch on open: the AppCache'd page asks elfldr
  (`GET :9021/<elf>?args=serve`; that copy, then the old path, then USB) only from the console's own
  browser at load.
  On the PS5 the start notice only says "Dump Forge is launched" (no URL; the serve log has it); the Connect
  screen shows the server's host from `GET /api/session` `url`, never the viewer's `location`, and its QR
  code (`GET /api/qr?connect=1`, `<url>/#connect`: an SVG drawn by the server with the `qrcode` crate; 404
  when the server can't tell its IP). The desktop app shows no QR.
  L2 / R2 (PS5 only, session `platform` `ps5`; `?ps5` in the URL on a computer too): no badges (by user decision),
  the pad's triggers step through the screens, wrapping, from any focus, not under a dialog (`shoulders.ts`); the same
  flag sets `html.ps5`, the text one size up (`zoom: 1.07`, by user decision: sizes are px). The PS5
  browser keeps L1/R1 (back/forward), has no Gamepad API for pages and sends L2/R2 as `F7`/`F8`
  keydowns (`STEPS`), found with a debug build's `GET /probe` (every input event, logged to
  `GET /api/debug_input`; the rail's **Input probe** button shows when the session says `debug`).
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
  Names: ASCII only, no case collisions, every offender listed. The writer leaves the inodes' unused
  indirect slots 0; the reader and editor also take -1 there (GameCompressor's `.ffpfsc`, PR #79), any other
  value is refused as a layout they can't serve.
- `.ffpfsc`: a single-file PFS (MkPFS `pack file` layout) whose one file is a zlib PFSC container of 64 KiB
  blocks around a nested `.exfat`/`.ffpkg`/`.ffpfs` named `<TITLE_ID>.<ext>` (default `.ffpkg`, by user decision; the UI defaults to exfat;
  exFAT is MkPFS's most stable). Always PFSC, zlib level 0–9 as zlib numbers them (the slider, `--level`, `ffpfsc_level`;
  default 6; 0 stores every block raw); a block is kept compressed only if it saves ≥ 5% and its zlib stream
  passes upstream's `ffpfsc_compat::console_safe` (FW 13.60 misreads a code-length repeat run from the literal into
  the distance lengths, which miniz_oxide writes in ~0.5% of compressed blocks: stored raw instead); a block with
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
  are `[GAME_TITLE]-[TITLE_ID]` by default (`-2`, `-3`, ... when taken); the game title is cut first. Output names (issue #33, by user decision): Settings' **Output names** card (`Settings.tsx`, store `app/src/naming.ts`, `localStorage` `forge.naming`, saved 500 ms after the last keystroke; Build names its output again only while shown, a hidden screen once when shown again, since each name reads the source: `useOutput`'s `active`, an idea from PR #39) is one free-text template, default `[{title}]-[{title_id}]` (core `DEFAULT_NAMING`), tokens `{title}`, `{title_id}`, `{version}` (param.json `contentVersion`: ASCII letters, digits and dots, no dot at an end, at most 16 bytes) `{pack}` (`zstd`/`LZ4`, a Pack job only: `PackKind`, `Format::Lz4` counts as LZ4), `{region}` (`USA`/`EUR`/`JPN`/`ASIA`/`KOR` from the content id's `UP`/`EP`/`JP`/`HP`/`KP`, else empty) and `{fw}` (param.json `requiredSystemSoftwareVersion` as `7.00`, by user request); chips insert a token at the cursor; each part at most once (core and the TS port refuse a repeat; a chip whose part is in the template is off, with a check); one example line (the sample as a zstd Pack names it: one rule for every job, a Convert leaves `{pack}` empty) with a live Length row: the example's bytes of 63 and the bytes left for the title (`naming.ts` `STEM_MAX`, `bytes`); a template cut to nothing falls back to the title id or the source's name; core `naming_problem` (and its TS port) refuses a template over 128 bytes, without `{title}` or `{title_id}`, with another `{`/`}`, a control character or `/ \ : * ? " < > |` (an invalid one is not saved; the server answers 400). A missing value is empty, then the template's own text is cleaned: empty `[]`/`()` removed, a run of one separator (`-`, `_`, space) collapsed, separators trimmed at the ends (ponytail: a spaced ` - ` can double). `generated_output`'s `naming: Option<&str>`, server/Tauri `naming` (absent = the default); within the limit the version is dropped when the rest doesn't fit even without the title, then the title cut, then the rest.
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
- LZ4 packs (ampr-emu-ng reads them, as did ampr_emu 0.4.2.1): index at `ampr_assets.index` (AMPRPAK4, little-endian, CRC32, FNV-1a path hashes)
  with `.crc` sidecar and `ampr_assets-NNN.pak` volumes (AMPRDAT3, volume cap 4 GiB - 64 KiB for FAT32); chunks LZ4 or RAW
  (a RAW chunk is exactly one I/O page); files the runtime cannot serve stay loose. CRCs detect
  corruption, they do not authenticate. The manifest's runtime-contract checks run on every open.
  Not an SMP image: ShadowMountPlus mounts none (no `.lz4` extension, no mount name limit beyond a folder's).
- zstd packs (`docs/zstd.md`): `ampr_zstd.index` (`AMPRZPK1`, version 1, 128-byte header, file, chunk and volume
  records, FNV-1a 64 path hashes, CRC-32 over header and tables) and `ampr_zstd-NNN.pak` volumes (`AMPRZDT1`, from 000; 64-byte header with
  the index's archive id, at most 4 GiB - 64 KiB, at most 4,096); 256 KiB chunks, each one zstd frame or raw
  when zstd doesn't shrink it. Only ampr-emu-ng (`vendor/ampr-emu-ng/libSceAmpr.sprx`, v0.5.1, `runtime::zstd()`) reads them. Not an SMP image, as LZ4 packs.

## Commands
```sh
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace --release
(cd app && npm ci && npm run tauri dev)                       # GUI in dev mode
cargo run -p ps5-dump-forge-cli -- convert <game_dir> --to exfat   # or ffpkg | ffpfs | ffpfsc | pkg | folder
cargo run -p ps5-dump-forge-cli -- convert <game_dir> --to lz4 [--lz4-level 0|3..12]   # asset packs (folder out)
cargo run -p ps5-dump-forge-cli -- convert <packed_dir> --to folder --lz4-unpack   # back to the loose assets (any target but lz4)
cargo run -p ps5-dump-forge-cli -- convert <game_dir> --to ffpkg --lz4-pack [--lz4-level N]   # pack straight into an image (also exfat|ffpfs|ffpfsc)
cargo run -p ps5-dump-forge-cli -- convert <game_dir> --to folder --zstd-pack [--zstd-level 1..22]   # zstd packs (default level 5; also exfat|ffpkg|ffpfs|ffpfsc)
cargo run -p ps5-dump-forge-cli -- convert <zstd_packed_dir> --to folder --zstd-unpack   # back to the loose files (any target but lz4)
cargo run -p ps5-dump-forge-cli -- convert <game_dir> --to pkg --keep-ampr-emu   # keep a game's ampr_emu (by default ampr-emu-ng replaces it)
cargo run -p ps5-dump-forge-cli -- edit <game_or_image> --restore-metadata [--metadata-url <version.xml|manifest.json|_sc.pkg>] [--original-runtime]   # needs the internet
cargo run -p ps5-dump-forge-cli -- edit <game_or_image> --add <file>:<game path> [--delete|--mkdir <path>] [--rename a:b] [--rebuild] [--check]
scripts/check-lz4.sh <lz4_out_dir> <orig_dir> | --reverse   # upstream Python tools (Docker) inspect/verify our packs, unpack theirs
cargo run -p ps5-dump-forge-cli -- convert <game_dir> --to ffpfsc --inner exfat   # inner: exfat | ffpkg | ffpfs; --level 0..9
cargo run --release -p ps5-dump-forge-pfs --example pfs_tool -- ffpfs <dir> <out> <time>   # for MkPFS byte comparisons
scripts/check-exfat.sh <img.exfat> [src_dir]   # macOS fsck_exfat + mount compare + exfatprogs (Docker)
scripts/fsck-ufs.sh <img.ffpkg>...             # FreeBSD fsck_ufs -n in qemu (Docker, ~1 min)
scripts/check-versions.sh                      # Cargo.toml/tauri.conf.json/package.json versions agree
scripts/release-macos.sh                       # arm64 + x86-64: .dmg (PS5 Dump Forge.app only) + bare CLI each, ad-hoc signed
scripts/release-windows.sh                     # Git Bash on Windows: the app .exe + the CLI .exe, for the host's arch
scripts/release-linux.sh                       # Linux: the .AppImage, for the host's arch
scripts/release-linux-musl.sh                  # the static Linux CLI (Docker, Alpine), for the host's arch
cargo run -p ps5-dump-forge-cli -- serve --root <dir>   # the web UI on this computer (cd app && npm run build:http first)
ps5/build.sh                                   # PS5 payload in Docker: target/ps5/ps5-dump-forge.elf
ps5/test-entry.sh                              # host check of entry.c
scripts/release-ps5.sh                         # dist/ps5-dump-forge-<ver>-ps5.elf, checked (SKIP_BUILD=1: check only; needs the zstd CLI)
scripts/make-payloads-json.sh <elf> [out.json] # ps5-payload-manager catalog for that .elf (GITHUB_REPOSITORY, PAYLOADS_DATE)
scripts/test-freebsd.sh                        # core + server tests on FreeBSD UFS/FAT32/nullfs (VM; long)
FORGE_DEBUG_API=1 ps5/build.sh                 # bring-up build with POST /api/debug, GET /probe, /api/debug_input; never released
```
`tauri build` needs an absolute `CARGO_TARGET_DIR` if you set one.

## Preferences
- Do not commit, push or stage unless asked.
- Rust 2024 edition, one pinned release everywhere (by user decision, after 1.100 broke CI): `rust-toolchain.toml`'s
  channel (1.100.0), `rust-version`, `ps5/Dockerfile`'s image and `RUSTUP_TOOLCHAIN`, `scripts/test-freebsd.sh`; bumped
  together. Every CI and release job that builds Rust on a runner installs exactly it (no `rustup update`), fuzz too
  (`RUSTC_BOOTSTRAP=1`, no nightly), and the PS5 jobs build in that image; action tags and `*-latest` runners stay
  unpinned.
- Keep code small and boring (stdlib first, no speculative abstractions; `// ponytail:` notes name
  the ceiling of a deliberate shortcut).
- Changes to `vendor/` go into a new numbered patch in `vendor/patches/` and `vendor/README.md`.
  Don't replace `vendor/` with our own code unless asked.
- Tests (by user decision): only critical ones (round trips, byte pins, journal recover, corruption and safety
  refusals, FLINK auth, cancel that frees resources); add a test only for one of those, and delete one that
  checks wording, a getter, std/serde or what another test already covers.
- Comments (by user decision): short, one line where possible, only the why the code can't show (safety and
  data-loss invariants, format facts, issue refs, `ponytail:`/`SAFETY:` notes). No restating the code, no
  history ("by user decision", "used to"), no copies of this file's prose; those live here.
