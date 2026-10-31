<p align="center"><img src="app/src-tauri/icons/128x128@2x.png" width="128" alt="PS5 Dump Forge"></p>

Runs on PS5, Windows, macOS and Linux, and can convert and edit any format: game `folders`, `exfat`, `ffpkg`, `ffpfs`,
`ffpfsc` and `FPKG (pkg)` packages, straight from `zip`, `7z` and `rar` too, with `zstd` and experimental `LZ4` asset packs for AMPR games.

Also supports **transfers and on-the-fly conversion** on all platforms, with the interface translated into **11 languages**.

| Platform | Runs on | You get |
|---|---|---|
| **PS5** | With an ELF loader | The payload (`.elf`); the UI opens in any browser |
| **macOS** | 11 or later, Apple Silicon and Intel (one universal zip) | App + CLI |
| **Windows** | 10 or 11 on x86-64, 11 on arm64 | App + CLI |
| **Linux** | x86-64 or arm64, glibc 2.35+ (e.g. Ubuntu 22.04+) | App (no install, no FUSE) + CLI |

## Features

- **Any source to any target**, streamed straight through: no temporary copy, not even the image inside a `.ffpfsc` or a game in an archive.
- **Every format ShadowMountPlus mounts**, created and read back, plus the debug FPKG.
- **Archives as sources**: a `.zip`, `.7z` or `.rar` holding one game is browsed, converted or packed like a folder.
- **Pack**: compress an AMPR game's assets into zstd packs, or experimental LZ4 packs, and unpack them again; a packed game converts to any format like any other.
- **Storage**: a **Library** of your games to convert and edit, and a **File explorer** to rename, move and delete files and folders, on a computer a paired PS5's too, with drag and drop between them.
- **Package installer on the PS5**: lists every `.pkg` on the console's storage and drives and installs one; a `.pkg` on a computer installs on a paired PS5 from Connect, read over the network.
- **Edit** a dump or image: browse its files, rename, delete, add or replace files and folders, then apply the changes in place or by rebuilding the image; extract files and folders from any source. The game card shows the cover, title, required firmware, backport firmware (`fakelib/`), embedded DLC and `param.json`; leftover `.part` files from interrupted jobs are listed under it when there are any.
- **Connect, transfers and on-the-fly conversion**: another Forge or a NAS (SMB, FTP, NFS), found by **Autodiscovery**, is a source and an output: convert straight between devices or copy files, nothing staged here.
- **One-click install on the PS5** from the desktop app: the console downloads the matching Dump Forge release from GitHub and starts it. Any other ELF payload can be sent the same way.
- **Additional content**: a DLC or an update (no `eboot.bin`, a valid `param.json`) is labelled everywhere, editable, and packs into a debug FPKG.
- **Fast**: (and furious).

## Transfers and on-the-fly conversion

A paired Forge (a computer or a PS5) and a saved NAS (SMB, FTP or NFS) are sources and outputs like a local
folder.

- **On-the-fly conversion**: Convert reads a game folder or image on one device and writes the new image or
  folder onto another, streamed over the network: nothing is copied to this device first, a debug FPKG
  included. An image is written as a `.part`, read back over the network to verify it, then renamed into
  place; a folder is written file by file. A `.pkg` or an archive source has to be on this device.
- **File Transfer** (Connect tab) copies files and folders from any device to a folder on any other, a peer
  to peer copy relayed through this one. It runs beside a conversion, and every file is checked by its size
  when it lands. A name already there is replaced only once you confirm.
- **Phones and browsers**: in the web UI, File Transfer uploads files from the browser's device or downloads one
  to it.

## Pack: zstd and LZ4 asset packs

Only for games whose `eboot.bin` imports `libSceAmpr` (AMPR titles). The app's **Pack** tab compresses a game's
assets into packs that a runtime installed at `fakelib/libSceAmpr.sprx` reads back on the console while the game
runs. Choose the kind of packs:

- **zstd** (the default): zstd packs (`ampr_zstd.index` + `ampr_zstd-NNN.pak`) at **Fast** (zstd 5), **Medium** (9)
  or **Max** (19), into a game folder or straight into an `.ffpkg`, `.exfat` or `.ffpfs`. A zstd packed game gets
  **Unpack** instead, into a folder, `.ffpkg`, `.exfat`, `.ffpfs` or fpkg.
- **LZ4** (experimental): ampr_emu's LZ4 packs (`ampr_assets.index` + `ampr_assets-NNN.pak`) at **Fast** (LZ4),
  **Medium** (LZ4 HC 9) or **Max** (LZ4 HC 12), into an LZ4 packed folder or straight into an `.ffpkg`, `.exfat`
  or `.ffpfs`. The runtime is **ampr-emu-ng** (the default) or the bundled
  [ampr_emu](https://github.com/drakmor/ampr_emu) 0.4.2.1, an upstream test build with which some games crash
  when saving. An LZ4 packed game gets **Unpack**.

Both kinds are read by **ampr-emu-ng**, the bundled runtime that Pack installs (an LZ4 Pack with ampr_emu
excepted). ShadowMount+ can update `fakelib/` and replace it: turn that off for packed games.

## Storage

The **Storage** tab holds the **Library** (the games found in a folder you choose, or on the PS5's drives)
and a **File explorer**. Both open a game in Edit or Convert, and the Library deletes one after you confirm. The
File explorer renames, moves and deletes at once (a delete after you confirm): a delete is permanent (no Trash), a
move stays on the same drive and replaces a name already there (a folder merged) only once you confirm, and anything an unfinished job uses is refused. On the
PS5, images ShadowMountPlus has mounted are tagged **Mounted** in the File explorer and can't be edited, renamed,
moved or deleted, and both views show the console's free space.

On the PS5, the **Package installer** view lists every `.pkg` on the console's storage and drives, with its title,
version and kind (game, patch, additional content). **Install** (also on a `.pkg` row of the File explorer)
asks first, then hands the package to the console's installer, which fetches it from Dump Forge: keep Dump Forge
running until the job ends. If the game is already installed, the console deletes that copy first.

## Editing a game

The **Edit** tab lists a game's files. Rename or delete them, and add files or folders with **Add files**,
**Add folder**, or by dragging them onto the list (onto a folder's row to add them inside it).
A file of the same name is replaced once you confirm. Nothing changes until **Apply**: **Undo** takes back the
last change (a drop of several files is one) and **Discard all** drops them all. New, replaced and renamed entries are badged.

Before you press Apply the tab says how the changes will go in:

- **In place**: written straight into the image. `.exfat` and `.ffpkg` take any change that fits the image's
  free space (an image never grows); `.ffpfs` takes deletes, renames and replacements that fit in the old
  file's space. The undo file needs room beside the image. An image
  marked unclean, or a `.ffpkg` not laid out the way Forge writes one, takes Rebuild instead. If the check after writing fails, or the computer or console stops part way, the image is put back
  from an undo file beside it (`<image>.forge-undo`); one left by a crash is restored on the next edit.
- **Rebuild**: a new image of the same format is written beside the original from the original plus your
  changes, verified, then put in its place. It needs free space beside the original for the new image (the tab says how much: for `.ffpfsc` and `.pkg`
  it can be more than the original's size). It is
  the only way for `.ffpfsc` and `.pkg`, and the way when the changes don't fit in place.
- **Changed directly**: a game folder is changed file by file, with no undo file.

## Connect

The **Connect** tab pairs this Forge with another one on the local network (**Autodiscovery**, which also runs
at every start, or an address under **Add a device**), and you allow it on the other device when both screens
show the same check number. A NAS (SMB, FTP, NFS) is saved the same way; one saved by IP that comes back on a new
address under the same name moves there by itself. While online, paired and saved devices are tabs in the file pickers:
their games are converted and their folders take outputs, but in Edit they are only browsed and extracted from.
**File Transfer** copies files and folders between any of them. A paired PS5's **Install pkg** installs a
`.pkg` from this computer on the console, which reads it over the network. In a browser, the tab's QR code opens the page
on a phone, and File Transfer can upload to or download from the device serving the page.

In the desktop app, **Dump Forge on PS5** installs Dump Forge on a console in one click: type its address and press
**Install on PS5**, and the app downloads the release matching it (or the latest) from GitHub and sends it to the
console's ELF loader, which starts it (the console needs no internet). **Send a payload** sends any other ELF file from the computer the same way. It needs an ELF loader
that listens on the network (port 9021).

## Download and run

Every release archive on the Releases page holds the app and the `ps5-dump-forge` command-line tool. Nothing to install; the app writes nothing beyond its own
WebView data, so keep it in any folder you can write to. At every start the app and the console's page check GitHub
for a newer release and offer it (on the PS5, **Update** downloads it and hands it to the ELF loader).

### PS5 (jailbroken, with an ELF loader)

- **Load it** with any ELF loader: send `ps5-dump-forge-<version>-ps5.elf` to elfldr's port 9021
  (`nc -w 3 <ps5-ip> 9021 < ps5-dump-forge-<version>-ps5.elf`), or list it in an autoloader's
  `autoload.txt`, or use **Install on PS5** in the desktop app's Connect tab. A notification says Dump Forge is
  launched; open it from its icon.
- **Open the UI** from the Dump Forge icon, or in any browser on the same network:
  `http://<ps5-ip>:8095`. In the console's browser, **L2** / **R2** switch tabs.

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
./ps5-dump-forge convert ~/Games/PPSA01234 --to ffpfsc --inner ffpkg   # inner: ffpkg (default) | exfat | ffpfs
./ps5-dump-forge convert ~/Games/PPSA01234 --to ffpfsc --level 6        # zlib level 0 (store) .. 9 (smallest), default 6
./ps5-dump-forge convert ~/Games/PPSA01234 --to pkg --kraken fast       # fast (default) | balanced | smallest | stored
./ps5-dump-forge convert ~/Games/PPSA01234 --to exfat --full-verify     # check every byte, not a sample
./ps5-dump-forge convert Game.part1.rar --to ffpkg --password-stdin     # archive source; password from stdin
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

## Thanks

Big thanks to **[PhantomPtr](https://github.com/phantomptr)** for
**[ps5upload](https://github.com/phantomptr/ps5upload)**. Its Rust engine is the reference implementation this
project learned the PS5 formats from: the FPKG package builder and reader (PFS, PFSC, Kraken, CNT/FIH,
PlayGo, debug license), and the exFAT and UFS2 readers. Its crates are vendored here (`vendor/`, v7.0.22).

Big thanks to **[drakmor](https://github.com/drakmor)**, author of
**[ShadowMountPlus](https://github.com/drakmor/ShadowMountPlus)**, for the mount side this tool writes for and
for help along the way: the image-building scripts (`mkexfat_macos.sh` and friends) whose sizing and layout
rules the `.exfat`/`.ffpkg` writers follow, the format guidance in SMP's README and release notes, and ppr-patch,
which makes debug `.pkg` installs possible.

Thanks again to **[drakmor](https://github.com/drakmor)** for **[ampr_emu](https://github.com/drakmor/ampr_emu)**
(GPL-3.0-or-later), the runtime that first made LZ4 asset packs work: its 0.4.2.1 release runtime is embedded
unmodified, with its source archive in `vendor/ampr_emu/`. **[Lazy_AMPR](https://github.com/Nazky/Lazy_AMPR)**
by Nazky was a reference for the pack rules (no code taken). The LZ4 library (BSD-2-Clause) that
Forge and ampr_emu compile in, and the HDE64 notice, are in `THIRD-PARTY-NOTICES.md`.

Thanks to **[Xxcco](https://github.com/Xxcco)** for the logo.

Credits for other references are in `THIRD-PARTY-NOTICES.md`.

## License

GPL-3.0-or-later. See `LICENSE`.
