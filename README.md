<p align="center"><img src="app/src-tauri/icons/128x128@2x.png" width="128" alt="PS5 Dump Forge"></p>

Runs on everything and converts everything (including your PS5): game folders to disk images and debug
FPKG packages, and back again. Fully offline, no settings.

| Platform | Runs on | You get |
|---|---|---|
| **PS5** | Jailbroken, with an ELF loader | The payload (`.elf`); the UI opens in any browser |
| **macOS** | 11 or later, Apple Silicon and Intel (one universal zip) | App + CLI |
| **Windows** | 10 or 11 on x86-64, 11 on arm64 | App + CLI |
| **Linux** | x86-64 or arm64, glibc 2.35+ (e.g. Ubuntu 22.04+) | App (no install, no FUSE) + CLI |

## Features

- **Any source to any target**, streamed straight through: no temporary copy, not even the image inside a `.ffpfsc`.
- **Verified output**: read back before it gets its final name; a fast sampled check by default, every byte optionally.
- **Never touches your dump**: junk (`.DS_Store`, `._*`, ...) is skipped, not deleted; bad file names are reported up front, never renamed.
- **Inspect** a dump or image: cover, title, required firmware, backport firmware (`fakelib/`), embedded DLC, `param.json`, leftover `.part` files.
- **Runs on the PS5 itself**: converts on the console's own storage, driven from its browser or any browser on the network.
- **Writes onto USB sticks and SD cards** formatted exFAT or FAT32.
- **Fast**: an 89 GB game becomes `.exfat`/`.ffpkg` in about 2–2.5 min (write + verify) on an Apple Silicon Mac; `.fpkg` in ≈ 5 min.

## Formats

Images are mounted by [ShadowMountPlus](https://github.com/drakmor/ShadowMountPlus); the debug FPKG is
installed. Every format can be created and read back (an image or `.fpkg` can be extracted to a folder).

| Format | Console speed | Size on disk | Notes |
|---|---|---|---|
| `.fpkg` (debug FPKG, saved as `.pkg`) | Native, full speed | 30–60% less, depending on the game | Installs on firmware 11.60 or lower with kstuff + fpkg-enable + ppr-patch. Kraken level (`--kraken`): **Fast** (default), **Balanced** / **Smallest** ≈ 2.6% smaller at ~6× / ~9× the compression time. Reads packages from this builder and compatible debug builders. Name ≤ 63 bytes. |
| `.ffpkg` (UFS2) | Full image speed | Equal to the game size | The default, ShadowMountPlus's recommendation. Name ≤ 63 bytes. |
| `.exfat` (512 B sectors, 64 KiB clusters) | Full image speed | Equal to the game size | For games that only run like external-drive content. Name ≤ 63 bytes. |
| `.ffpfs` (PFS, uncompressed) | Full image speed | Equal to the game size | Experimental in ShadowMountPlus. ASCII file names only. Name ≤ 63 bytes. |
| `.ffpfsc` (compressed PFS container) | 150–250 MB/s, may stutter | Depends on the compression level, usually 30–60% less | Experimental, always mounted read-only. exFAT (default), UFS2 or PFS inside. zlib level 0–9 (`--level`), default 6; a 13 GB game (14 cores): 0 → 3.0 s, 100% stored; 1 → 5.7 s, 78.57%; 4 → 13.0 s, 76.42%; 6 → 16.3 s, 76.25%; 7 → 17.3 s, 76.23%; 9 → 20.4 s, 76.21%. Name ≤ 58 bytes. |
| Folder | Full drive speed | Equal to the game size | Run as it is by ShadowMountPlus. Name ≤ 63 bytes. |

Generated names are `[<GAME TITLE>]-[<TITLE_ID>]`, e.g. `[Astro Bot]-[PPSA01234].ffpkg`. ShadowMountPlus
mounts an image at `/mnt/shadowmnt/<name>_<8 hex>` (`.ffpfsc` under `/mnt/shadowmnt/pfsc/`) and the PS5 caps
mount paths at 88 bytes, hence the name limits above (without the extension; folders and `.fpkg` keep to the same 63 bytes); the
game title is cut first, and a typed name over the limit fails before anything is written.

## Download and run

Every release archive on the Releases page holds the app, the `ps5-dump-forge` command-line tool,
README.md, LICENSE and THIRD-PARTY-NOTICES.md. Nothing to install; the app writes nothing beyond its own
WebView data, so keep it in any folder you can write to.

### PS5 (jailbroken, with an ELF loader)

Get `ps5-dump-forge-<version>-ps5.elf` from Releases: the same conversions, read from and written to the
console's own storage, with the app's UI as a web page. Tested on firmware 13.60 with elfldr and itsPLK's
webkit autoloader.

- **Load it** with any ELF loader: send it to elfldr's port 9021 (`nc -w 3 <ps5-ip> 9021 <
  ps5-dump-forge-<version>-ps5.elf`, or any ELF sender), or list it in an autoloader's `autoload.txt`.
  A notification shows the address.
- **Open the UI** in the PS5 browser or any browser on the same network: `http://<ps5-ip>:8095`. The file
  browser starts at `/data` (the internal SSD) and the drives that are mounted (`/mnt/usb0..7`,
  `/mnt/ext0..1`); the output goes next to the source by default (e.g. `/data/homebrew`). Closing the
  browser doesn't stop a job; reopening the page picks it up again.
- **Home-screen tile**: on start, Forge adds a "PS5 Dump Forge" tile (`PDFG00001`) that opens the UI in the
  PS5 browser, and saves a copy of itself to `/data/ps5-dump-forge/ps5-dump-forge.elf`. After a console
  restart (once the jailbreak or autoloader has run), the tile starts Forge again from that copy (or from a
  `ps5-dump-forge.elf` at a USB drive's root). Open the tile once while Forge runs, so the browser keeps the page.
- **No access control.** No password, pairing or other check: anyone who can reach the console on the network,
  and any web page open in a browser there, can browse its files and start jobs. Use it on a trusted network
  only.
- **USB output is experimental**: not tested on hardware. Each job first probes the output folder and refuses
  what it isn't sure of; the job log says what the probe saw. Hardware runs so far used `/data` only.
- **Logs**: `/data/ps5-dump-forge/serve-<pid>.txt`. Each start deletes older runs' logs, keeping the
  newest (and a running Forge's).
- **Stop it** with "Stop PS5 Dump Forge" at the top right of the page (it asks first when jobs are running,
  cancels them and cleans up). Loading it a second time only shows "already running".

How it works, measured speeds on the console and how it writes safely: [ps5/README.md](ps5/README.md).

### macOS (11 or later)

Get `ps5-dump-forge-<version>-macos-universal.zip` (Apple Silicon and Intel) and unzip it anywhere.

The app is ad-hoc signed only (no Apple Developer ID, not notarized), so macOS blocks the first launch ("PS5
Dump Forge is damaged" or "cannot be opened"). Use **one** of these:

1. **Terminal** (every macOS version), in the folder that holds the app, then double-click it:

   ```sh
   xattr -dr com.apple.quarantine "PS5 Dump Forge.app"
   ```

2. **System Settings.** Double-click the app once and dismiss the warning, then in System Settings → Privacy
   & Security, scroll down to the message about PS5 Dump Forge and click **Open Anyway**.

### Windows (10 or 11 on x86-64, 11 on arm64)

- x86-64: `ps5-dump-forge-<version>-windows-x86-64.zip` uses the Microsoft Edge WebView2 Runtime, which
  Windows 11 and most Windows 10 PCs already have. Without it, get `...-windows-x86-64-webview2.zip`: larger,
  with its own copy of the runtime in a `WebView2` folder.
- arm64: `ps5-dump-forge-<version>-windows-arm64.zip` (uses the WebView2 Runtime Windows 11 ships).

Extract it to a folder you can write to (not Program Files, not a network share): `PS5 Dump Forge.exe` keeps
its WebView data in a `data/webview` folder beside itself. The app is unsigned, so SmartScreen may block the
first launch: click **More info**, then **Run anyway**. If WebView2 is missing or broken, the app says what to
do instead of showing a blank window.

### Linux (x86-64 or arm64, glibc 2.35 or later, e.g. Ubuntu 22.04 or newer)

Extract `ps5-dump-forge-<version>-linux-x86-64.tar.gz` or `...-linux-arm64.tar.gz` (`tar -xzf`) to a folder
you can write to and run `./forge.sh`. No FUSE, no install; keep `PS5 Dump Forge.AppDir` next to `forge.sh`
(its WebView data goes in a `data/webview` folder beside it).

### Command-line tool

Every desktop archive also has `ps5-dump-forge` (`ps5-dump-forge.exe` on Windows):

```sh
./ps5-dump-forge inspect PPSA01234.exfat
./ps5-dump-forge convert ~/Games/PPSA01234 --to ffpkg      # folder | exfat | ffpkg | ffpfs | ffpfsc | pkg (debug FPKG)
./ps5-dump-forge convert ~/Games/PPSA01234 --to ffpfsc --inner exfat   # inner: exfat (default) | ffpkg | ffpfs
./ps5-dump-forge convert ~/Games/PPSA01234 --to ffpfsc --level 6        # zlib level 0 (store) .. 9 (smallest), default 6
./ps5-dump-forge convert ~/Games/PPSA01234 --to pkg --kraken fast       # fast (default) | balanced | smallest
./ps5-dump-forge convert ~/Games/PPSA01234 --to exfat --full-verify     # check every byte, not a sample
```

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

The PS5 payload builds in Docker (any macOS or Linux host with Docker and Node) with `ps5/build.sh`; the
rest of its commands are in [ps5/README.md](ps5/README.md).

See [AGENTS.md](AGENTS.md) for the layout and the format rules, and [DESIGN.md](DESIGN.md) for the UI design
system.

## TODO

- Test `.fpkg` on a console
- USB writes from the PS5 payload (test on hardware)
- Library (planned)
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
