# TODO

What is left after v1 (folder ⇄ `.exfat` / `.ffpkg` / `.ffpfs` / `.ffpfsc` / `.pkg` on macOS universal, Windows
x86-64 and arm64, Linux x86-64 and arm64; v0.0.1-pre3 built and passed CI on every target; v0.0.1-pre4 adds
the PS5 payload, whose CI and release jobs have not run yet).
Grouped by area; the first section blocks a release.

## Before the first release
- [ ] **Hardware smoke test** per format, on a console: SMP 1.7 mounts `.exfat` and `.ffpkg` and the game
  boots (read-only and `image_rw=`); a `.pkg` installs and boots with kstuff + fpkg-enable + ppr-patch. Only the
  console proves a format is accepted. Include a source with `sce_sys/pic1.png`/`pic2.png`: since vendor patch
  0018 they go into the container only (0x1006/0x2040, in the system digest), as in a third-party package,
  and no such `.pkg` has been installed yet.
- [ ] **`.ffpfs` and `.ffpfsc` on a console**: SMP 1.7 mounts and boots a `.ffpfs` (under `/data`, and on USB) and a
  `.ffpfsc` with each inner format (`.exfat`, `.ffpkg`, `.ffpfs`). Both follow MkPFS's layouts, but neither
  has been booted from this builder.
- [ ] **PS5 payload**: SMP 1.7 mounts and boots an image Forge made on the console (only conversions and
  their checks have run there, no mount). See "PS5 payload" below for the rest.
- [ ] **zlib on the console**: `.ffpfsc` blocks come from `flate2`'s `miniz_oxide`, not the zlib backends MkPFS uses.
  MkPFS reports that ISA-L output can crash the console's hardware decompressor; a standard zlib stream should be
  fine, but only a console run of a whole game (every block read) proves miniz's streams are accepted. Run the
  default level 6 and the ends of the slider: 0 (every block raw, no zlib stream) and 9.
- [x] Run CI on GitHub once and fix what breaks: every test leg (macOS, Linux and Windows on x86-64 and
  arm64), the FreeBSD `fsck_ufs` job, the macOS `fat_volumes` step, the fuzz job, and the release workflow
  (the v0.0.1-pre3 tag: all five targets built, verified and drafted).
- [ ] External review (Codex) of the whole tree; per-crate reviews were done during development, the final
  integration pass was not.
- [ ] Test the published zip on a clean Mac: quarantine instructions, running from a read-only
  (App Translocation) location.
- [ ] Write real 4 GiB+ files in tests (just under and just over 4 GiB); today they are only planned, not
  written. Add a real triple-indirect UFS2 file test (or keep the unit test of the pointer math and say so).
- [x] Release from a clean tagged commit: the release workflow builds from the tag's checkout (a local
  `scripts/release-macos.sh` run still warns when the tree has uncommitted changes).

## Formats
- [ ] `.ffpfs` names are ASCII only (MkPFS's limit, and the only layout known to boot); widen to UTF-8 only
  after a console test with such names.
- [ ] PFS reader: only the contiguous unsigned layout MkPFS writes (32-bit inodes, unencrypted, `db[0]` extents);
  signed, 64-bit-inode, encrypted or indirect-block images are refused with a message.
- [ ] FPKG patches and DLC (today only app/base game; patches are detected only by
  `applicationCategoryType`, which a patch may keep).
- [ ] FPKG extraction from other builders (LibProsperoPKG, scene tools): needs a test corpus; Sony-style
  inner encryption is not handled; the flat-layout reader relies on this builder's placement rule.
  After 0.0.1-pre4, the reader also handles what a third-party package (a real Oodle Kraken image) needs: sparse
  extents in the layout descriptor (vendor patch 0016), bare entropy-array halves (0017), and
  `param.json`/NP files kept only in the container. Every file of that package (521 files, 188 GB) reads
  back through the reader; every block decoded in a prototype scan of the same fixes (blocks no file covers
  were not read with the shipped code). Converted to `.exfat` and back to `.pkg`, 520 of its files came out
  byte-identical (the other two: a generated `ampr_emu.index` and `param.json`'s cleared `versionFileUri`),
  not yet installed on a console. Nothing checks the decoded bytes against Oodle's own decoder; other Kraken array types (RLE,
  tANS, multi-array) are still refused.
- [ ] Optional user-supplied FPKG key file (all key use already sits behind vendored `keys.rs`).

## Platforms (after v1)
- [x] **Intel macOS**: done, the macOS release is one universal zip.
- [x] **Windows (x86-64 and arm64)**: zip with `PS5 Dump Forge.exe` (frontend embedded); the WebView2 check
  with a native message box, the x86-64 `-webview2` zip with the Fixed Version runtime, SmartScreen in README,
  WebView data in `<app dir>/data/webview`.
- [ ] Windows core gaps marked `ponytail:`: free-space and FAT32 checks, file identity (stability and
  `.part` identity), hard-link and reparse-point detection in the scanner, handle-relative extraction
  (`FILE_FLAG_OPEN_REPARSE_POINT`), CLI Ctrl-C, image-source stability compares only length + mtime.
  `MoveFileExW` no-replace rename compiles but is untested.
- [x] **Linux (x86-64 and arm64)**: `.tar.gz` with an extracted AppDir and a `forge.sh` launcher (no FUSE),
  `WEBKIT_DISABLE_DMABUF_RENDERER=1`, the arch-aware Wayland `libwayland-client` preload, built on
  `ubuntu-22.04` (glibc 2.35 floor).
- [x] CI runners per target (`ubuntu-22.04`, `ubuntu-22.04-arm`, `windows-latest`, `windows-11-arm`, `macos-15`);
  Tauri doesn't cross-compile between OSes.
- [ ] Test each **published archive** on a clean VM or real machine, arm64 included (CI only builds them, runs
  the CLI and, on Linux, starts the GUI under Xvfb; the Windows GUI has not been started from a release zip).
- [ ] GitHub deprecated the `ubuntu-22.04` and `ubuntu-22.04-arm` runner images (retirement 2027-04-17): before then, keep the glibc 2.35
  floor (e.g. build in an ubuntu:22.04 container on a newer runner) or raise it and say so in README.

## App
- [ ] Batch queue screen, History, Library.
- [ ] Inspect: a **Verify** action for an existing image or package (core has no standalone verify API yet).
- [ ] Leftover `.part` files: offer deletion (today list-only on Inspect), limited to this app's
  `<name>.<job>-<pid>.part` pattern, never while a job runs.
- [ ] Progress: show what each stage counts (bytes vs. blocks) instead of always bytes.
- [ ] macOS: confirm-before-quit for Dock Quit, logout and shutdown (needs an app delegate; today jobs are
  cancelled and cleaned up without asking). On macOS 11 the quit prompt doesn't block keyboard input behind it.
- [ ] A log file (`<app dir>/data/logs`); today logs go to the UI and stderr only.
- [ ] Inspect names every bad file name up front: `inspect()` checks NFC only, so forbidden characters and
  exFAT case collisions surface only when the job's preflight fails (it lists them all, before writing).
  Needs per-target findings (a `:` is fine in UFS2, not in exFAT; PFS takes ASCII only); the UI then stops matching core's
  English finding text (`// ponytail:` at `classify` in `app/src/common.tsx`).
- [ ] UI checks run in Playwright WebKit with a mocked bridge; drive the real app (WKWebView, native
  dialogs, Show in Finder, Cmd+Q during a job) once per release.
- [ ] No frontend test runner in the repo; `app/scripts/` has plain-node checks (`check-paths`,
  `check-poller`, `check-launcher`, `smoke` against the real `serve`), run by hand. The UI was checked this session with a throwaway
  Playwright WebKit screenshot harness (mocked Tauri bridge fed by real CLI data, prod build served with
  the real CSP) that isn't in the repo; consider adding a small dev-only version of it.
- [ ] `.fpkg`'s build-time estimate (~80 MB/s effective) is approximate, from a partial real build.
  Measure a full `.pkg` build once and adjust the constant in `Convert.tsx` (`PKG_BYTES_PER_SEC`) and the
  About formats table (`Formats.tsx`).
- [ ] `.ffpfs` and `.ffpfsc` build times are not measured: the About formats table says "Not measured" and
  Convert shows no estimate. Time a full game for each (and each `.ffpfsc` inner) and fill them in.
- [ ] UX ideas not done (ruled out for now, modest value for the backend/state they'd need): show output
  size and free space before Build; share the chosen source between Convert and Inspect; a param.json copy
  button; "Show in Finder" for leftover `.part` files; a notification when a long job finishes while the
  app is backgrounded; a note that Tab only reaches buttons/radios when macOS's "Keyboard navigation"
  setting is on.
- [ ] `.fpkg` and `.ffpkg` differ by one letter and sit next to each other in the picker and the About
  formats table; watch for user confusion despite the different colors and taglines.

## Core and writers
- [ ] UFS2: directories are built whole in memory and files keep a per-file block list (bounded, but large
  for huge trees); two blocks before each backup superblock stay unused (~128 KiB per group); tiny trees make
  ≥ ~130 MB images because of the 2048 spare inodes at the 64 KiB density floor.
- [ ] UFS2 tight sizing (64 MiB free, no +10%): SMP 1.7 mount + boot of a tight `.ffpkg`, read-only
  and `image_rw=`, before release. FreeBSD `fsck_ufs` passed on boundary images.
- [ ] UFS2: the vendored reader verifies at most 1M files+dirs, so a bigger tree writes its image and then
  fails verification; refuse it in preflight. `plan`'s tree build and per-object passes check cancel
  only between passes.
- [ ] FPKG: `readiness` runs twice per build and its libSceAmpr scan (~80 MiB read) can't be cancelled.
- [ ] `FpkgSource`: per-block SHA3 check costs throughput; the 256 MiB whole-`read()` cap is untested.
- [ ] Readers: a UFS2 entry with a zero name length and an exFAT entry whose name never completes are still
  skipped silently instead of failing.
- [ ] Core tests leave per-run folders in `target/tmp` (`<name>-<pid>`, ~29 GB after ~150 runs); clean them
  up on success (a guard that removes its folder on drop).
- [ ] The one-in-many flake seen once in `crates/ps5-dump-forge-core/tests/core.rs` was traced to the
  Done-ordering race in `jobs.rs` and fixed; keep an eye on it in CI.
- [ ] `inspect.rs`'s non-NFC finding still reads "refused by .exfat/.ffpkg"; `.pkg`/`.fpkg` refuses
  non-NFC names too (`package.rs`), and so do `.ffpfs`/`.ffpfsc`. The UI already warns for those targets
  regardless (`common.tsx`); fix the core wording to match.
- [ ] QA minors, not fixed: preflight's free-space estimate adds a fixed 64 MiB reserve, which over-asks
  on tiny games; `scripts/check-exfat.sh`'s Docker step and `scripts/fsck-ufs.sh` keep deleted images open
  in Docker Desktop's VM until it restarts; `ps5-dump-forge inspect --help` / `convert --help` print the
  generic top-level usage instead of subcommand help.

## PS5 payload
Done on hardware (PS5, firmware 13.60, elfldr on 9021, sources and outputs on `/data`): the payload starts,
`inspect`, `.ffpkg` → `.exfat`/folder (full verification) and folder → `.exfat` (fast), publish by
checked rename (`/data` is nullfs, no hard links), clean cancels, the tile, the self copy and its start
through elfldr, the release ELF without debug routes, the cover glow in the PS5 browser. Results in README.md.
- [ ] **Other outputs on the console**: `.ffpkg`, `.ffpfs`, `.ffpfsc` (each inner) and `.pkg` built on the PS5,
  then mounted/installed and booted. Cancel mid-job leaves no `.part` for each.
- [ ] **USB output on hardware** (Sony `exfatfs`, and FAT32, whose fs name is unconfirmed, probably `msdosfs`).
  Experimental until then; the probe's `destination ...` log line makes the first run a diagnostic. Then tune
  `SyncEvery`'s constants (start 32 MiB, ~1 s, 16..256 MiB; on `/data` the fsync cadence costs nothing).
- [ ] **CI and release jobs never ran on GitHub**: `ci.yml` `ps5` (`release-ps5.sh` + `test-entry.sh`) and
  `release.yml` `ps5` + the `publish` asset list; the cold Docker build time (image, SDK, build-std) is
  unknown. The FreeBSD VM tests stay out of CI (TCG takes 80+ min); run `scripts/test-freebsd.sh` locally
  before a hardware run that touches the FreeBSD paths.
- [ ] **Faster exFAT and PFS readers** (the same fix as vendor patch 0015 for UFS2: contiguous runs in one read,
  cached metadata): full verification and `.exfat`/`.ffpfs`/`.ffpfsc` sources read in small syscalls, and a
  64 KiB read costs ~0.35 ms on the console (150 MB/s vs 912 MB/s at 8 MiB).
- [ ] **Forced drive-removal VM test**: `mdconfig -d -u <unit> -o force` during a small write, last and under a
  timeout (U7's decision is unit-tested and its `statfs` lookup VM-tested; the removal run is not written).
- [ ] **Verification switch on Windows and Linux**: drive the desktop UI once on each (via CI) with the "Full
  verification" switch on and off, `.fpkg` showing it on and disabled.
- [x] The tile across a real restart: after a reboot and the autoloader, the tile starts Forge from its saved
  copy (also after the tile was removed and Forge loaded fresh again).
- [ ] Hardware, not yet recorded: reload restoring a running job in the PS5 browser, the Stop button (the
  `/api/quit` route itself ended the payload cleanly), a second load answering "already running".
- [ ] A 100 GB+ job on `/data` with no kernel panic (87 GB ran clean); rest mode during a job
  (`sceSystemServicePowerTick` links but is untested; rest entered by hand has no clean-failure guarantee).
- [ ] `panic=abort`: a job panic ends the payload (its `.part` stays, listed on the next start). Try
  `panic-strategy: unwind` with `-Z build-std=std,panic_unwind` (`.eh_frame` present, `_Unwind_*` resolved,
  but the shim's `dl_iterate_phdr` returns `ENOSYS`); pass only if a panicking job fails, its `.part` is
  deleted and the next job runs.
- [ ] Measure on hardware: thread stacks (main from elfldr; job and compression threads ask for 2 MiB),
  `available_parallelism`, heap limits. Don't stack compression, hashing and server thread pools.
- [ ] Fast `.pkg` verification: needs a numbered vendor patch with a sampling policy (the builder's verify
  sweeps every block).
- [ ] `ponytail:`s: the `.pkg` background syncer has no backpressure (upgrade: a vendor patch threading
  `Output` through `build::write_package` and `kraken_image::compress`), and the builder's final `sync_all`
  has no retries (a transient error fails the job, nothing is published); `Part` is path-based (upgrade:
  descriptor-relative ops); the shown IP comes from a UDP `connect` (`sceNetCtlGetInfo` if it proves wrong).
- [ ] Re-audit vendor entry points before core calls any new one on the PS5: the vendor `build_mode` space gate
  (`statvfs`) and `estimate`'s `probe_write_rate` are not on any core path today.
- [ ] FAT32 limits in the tests: link/symlink fixtures skip on `EOPNOTSUPP`; a same-size rename-over within
  2 s is not caught on msdosfs (`SourceStamp`).
- [ ] Nits: on a PC the "has stopped" dialog says "Start the payload on the PS5"; no favicon (a 404); the CLI's
  `signal()` doesn't check `SIG_ERR`.
- [ ] Later, not planned: in-app updates (stage beside the target `O_EXCL|O_NOFOLLOW` + fsync, a copy-based
  rotated `.prev` since USB has no hard links, rename + folder fsync; fetching needs TLS in the payload, a
  browser upload doesn't) and signatures (ed25519, only if releases come from more than one place).

## Replace the vendored ps5upload crates
Not scheduled; v1 ships with `vendor/` (ps5upload v6.1.2 + `vendor/patches/`). ps5upload stays the reference
implementation and keeps its credit in README.md. The app stays GPL-3.0.

Rule for every step: the vendored code stays as a **dev-dependency oracle** until its replacement passes
differential tests against it (same input → same tree, same bytes; for writers, byte-identical output given the
same time/seed where the format allows). Then that vendored module is deleted and its fuzz targets move over.

| Step | What | Replaces | Size / risk |
|---|---|---|---|
| R0 | `ps5-dump-forge-tree`: `SourceTree`, `SourceFile`, `Error`/`Result`, one junk predicate | `source.rs` trait + `is_junk` | small; touches every crate's imports |
| R1 | Readers next to their writers: exFAT in `-exfat`, UFS2 in `-ufs2`, PFSC (`.ffpfsc`) in `-pfs` (**PFS/PFSC part done**: `PfsSource`, `open_ffpfsc`; the inner exFAT/UFS2 still go through the vendored readers) | `exfat.rs`, `ufs2_source.rs`, `ps5upload-pkg/ufs2.rs`, `pfsc_reader.rs` | medium; formats already known from the writers |
| R2 | FPKG read side fully ours: header/CNT/outer PFS/inner image parsers (`-fpkg` already re-parses most), RustCrypto `aes`/`xts-mode`/`sha3`/`hmac` instead of hand-rolled crypto, Kraken **decoder** (evaluate existing crates first, else port) | `fih`, `cnt`, `outer`, `naps`, `inner` (read), `kraken_image` (read), `kraken` (decode), `xts`, `crypto` | medium-large; the decoder is the bulk |
| R3a | FPKG writer with **stored** Kraken layout (no encoder): plan, inner PFS image, outer PFS (plaintext `PPRPLAIN-NOAUTH!`), CNT/FIH, `pfsimage.xml`, PlayGo, `license.dat` (debug RIF + RSA sign, keys behind one `keys.rs`), `param.json` rewrites, SELF repair, `ampr_emu.index`, readiness, streaming verify | `build`, `plan`, `inner`/`outer_write`, `cnt_write`, `fih_write`, `si_write`, `playgo`, `license`, `rsa`, `keys`, `self_repair`, `sdk_rules`, `ampr_index`, `verify`, `stream` | large; **hardware gate** before it ships |
| R3b | Kraken **encoder** (Balanced) for compressed packages | `kraken` (encode), `kraken/huff` | large; compare ratio/speed with the oracle; hardware gate |
| R4 | `.ffpfsc` writer on our own PFS code (**done**: `ps5-dump-forge-pfs`; the vendored `ffpfsc.rs`/`pfsc_reader.rs` are no longer used and go with R5) | `ffpfsc::wrap` | medium |
| R5 | Delete `vendor/`, its patches and the oracle tests; update THIRD-PARTY-NOTICES (credit stays) | — | small |

Order: R0 → R1 → R2 → R3a → R3b; R4 any time after R2; R5 last. Each step ends with the usual gate (fmt,
clippy, tests, fuzz, external review); R3a/R3b also need the console smoke test.
