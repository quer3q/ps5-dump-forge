# TODO

What is left after v1 (folder ⇄ `.exfat` / `.ffpkg` / `.ffpfs` / `.ffpfsc` / `.pkg` on macOS universal, Windows
x86-64 and arm64, Linux x86-64 and arm64; v0.0.1-pre3 built and passed CI on every target).
Grouped by area; the first section blocks a release.

## Before the first release
- [ ] **Hardware smoke test** per format, on a console: SMP 1.7 mounts `.exfat` and `.ffpkg` and the game
  boots (read-only and `image_rw=`); a `.pkg` installs and boots with kstuff + fpkg-enable + ppr-patch. Only the
  console proves a format is accepted.
- [ ] **`.ffpfs` and `.ffpfsc` on a console**: SMP 1.7 mounts and boots a `.ffpfs` (under `/data`, and on USB) and a
  `.ffpfsc` with each inner format (`.exfat`, `.ffpkg`, `.ffpfs`). Both follow MkPFS's layouts, but neither
  has been booted from this builder.
- [ ] **zlib on the console**: `.ffpfsc` blocks come from `flate2`'s `miniz_oxide`, not the zlib backends MkPFS uses.
  MkPFS reports that ISA-L output can crash the console's hardware decompressor; a standard zlib stream should be
  fine, but only a console run of a whole game (every block read) proves miniz's streams are accepted.
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
- [ ] No frontend test runner in the repo. `app/src/paths.ts` (path helpers, including the trailing-`\`
  macOS case) is import-free and ready for one. The UI was checked this session with a throwaway
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
