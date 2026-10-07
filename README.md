<p align="center"><img src="app/src-tauri/icons/128x128@2x.png" width="128" alt="PS5 Dump Forge"></p>

<h1 align="center">PS5 Dump Forge</h1>

<p align="center">Convert PS5 game dumps between a folder and the images <a href="https://github.com/drakmor/ShadowMountPlus">ShadowMountPlus</a> 1.7 mounts, and build or extract debug FPKG packages.<br>
macOS app + command-line tool. Apple Silicon, offline, no settings.</p>

- **Any source to any target**: folder, `.exfat`, `.ffpkg` (UFS2), `.ffpfs` (PFS), `.ffpfsc` (compressed PFS
  container around an exFAT, UFS2 or PFS image), debug FPKG `.pkg`. Streams straight from source to output,
  no temporary copy, not even the image inside a `.ffpfsc`.
- **Verified output**: every image is written to a `.part` file, read back and compared file by file
  (BLAKE3) with the source before it gets its final name.
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

> **v0.0.1-pre is a test release.** Not every format has been hardware-tested on a console yet; keep the
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
Finder), **Inspect** (game summary; Files / param.json / Details; leftover `.part` files) and **About formats**.

## Download and run (Apple Silicon macOS 11+)

Get `ps5-dump-forge-<version>-macos-arm64.zip` from Releases and unzip it anywhere. PS5 Dump Forge has no
settings and writes nothing next to itself, so `PS5 Dump Forge.app` can live in any folder.

### First launch

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

### Command-line tool

The zip also has `ps5-dump-forge`:

```sh
./ps5-dump-forge inspect PPSA01234.exfat
./ps5-dump-forge convert ~/Games/PPSA01234 --to ffpkg      # folder | exfat | ffpkg | ffpfs | ffpfsc | pkg (debug FPKG)
./ps5-dump-forge convert ~/Games/PPSA01234 --to ffpfsc --inner exfat   # inner: exfat (default) | ffpkg | ffpfs
```

## Build

Rust (latest stable) and Node.js:

```sh
cargo test --workspace --release
cd app && npm ci && npm run tauri dev
```

See `AGENTS.md` for the layout and the format rules, and `DESIGN.md` for the UI design system.

## TODO

- Hardware-test every format on a console before the first release
- Intel macOS build
- Windows port (x64, then arm64): portable zip, WebView2 check
- Linux port (x64, then arm64): portable AppDir tarball
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
