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

## LZ4 asset packs (ampr_emu)

A separate, optional transform for games that load through `libSceAmpr` (the title's `eboot.bin` imports
it): the game's assets are LZ4-compressed into a few `ampr_assets-NNN.pak` volumes, and the
[ampr_emu](https://github.com/drakmor/ampr_emu) runtime (`libSceAmpr.sprx`, installed by Forge under
`fakelib/`) reads them back on the console while the game runs. The result is still a plain game
**folder**. It is not an image format and ShadowMountPlus does not mount "LZ4 images"; if SMP or the
console ever supports LZ4 images, that is a different feature. Pack can also write the packed game
straight into an `.ffpkg`, `.exfat`, `.ffpfs` or `.ffpfsc` (the image holds the packs and loose files
as plain files), with no temporary folder: Forge measures the packs first (compressing without writing,
keeping about 16 bytes per chunk; the job peaks near 36 bytes per chunk, about 50 MB for a 91 GB game), lays the image out from their exact sizes, then makes each chunk again
while the image is written and checks it against the measurement. The packed files are read twice; on a
2 GB test game (warm cache, 14 cores) the measure pass took about 0.3 s of a 2.1 s job. Converting a packed
folder to an image is still an ordinary second convert (the packs are carried along).

> **ampr_emu 0.4.2.1 is an upstream test build; known issue: some games crash when saving.** Forge logs
> this on every trace and pack job.

Everything LZ4 is in the app's **LZ4** tab, in two scenarios: **Trace** (Patch or Unpatch a game folder,
`.exfat` or `.ffpkg`; Download traces in the console web UI once it holds a journal) and **Pack** (pack by
traces or a profile; Unpack when the source is already packed). The Convert tab copies a packed folder as
it is.

Which files to pack comes from a recording, so it takes two converts:

1. **Trace.** Patch the dump: **LZ4** → **Trace** → **Patch** (UI) on a folder, `.exfat` or `.ffpkg`, or
   convert it with `--lz4-trace` (CLI) to a folder or image. Forge installs ampr_emu's trace runtime,
   which records every file the game reads into `ampr_commands.bin` on the title's own storage. Run it on the console and play one long
   session through everything that matters (levels, languages, DLC). Only the **last** session is used.
2. **Pack.** Convert the traced dump (or the folder you copied back) with `--to lz4` / **LZ4** → **Pack**. Forge reads the
   journal, packs the files the game read, except those the engine opens directly (container indexes, configs, media), leaves the rest loose, swaps the trace runtime for the release one
   and removes the journal and logs. Or pass `--lz4-profile x.toml` to choose the files yourself; without
   a recording or a profile it falls back to a built-in guess, which is less reliable.

   Packed files exist for the game only once it has started AMPR, so anything it reads before that must
   stay loose. Whatever the rule source (a profile too), Forge's **keep-loose list** wins: container
   indexes (`.pak`, `.utoc`), configs and text (`.ini`, `.json`, ...), Unreal project files (`.uproject`,
   `.uplugin`, `.upluginmanifest`), images, video (`.bik`, `.mpg`, `.mpeg`, `.m2v`, ...), audio and the
   protected folders (`sce_module`, `system`, `save`, ...); the log names what a profile wanted packed.
   Then **auto-loose** (as upstream's packer): a file of 64 MiB or more left to compress is sampled
   (32 blocks spread across it, at most 16 MiB) and stays loose when the samples save under 5% or 90% of
   them stay uncompressed; hot files (a profile's `hot = true`) are exempt. A profile tunes it with the
   `[pack] auto_loose_*` keys (`auto_loose_large_files = false` turns it off).

   **Save as profile** (beside Pack; CLI `ps5-dump-forge lz4-profile <dump> [--lz4-traces <zip>]
   [-o x.toml]`) saves the plan Pack would use, after all of the above, as an editable TOML,
   `[Game title]-[TITLE_ID]-lz4profile.toml`: one rule per kind of packing listing the exact paths.
   Edit it, then pack with **Use a rules profile** / `--lz4-profile`.
3. Or pack straight into an image: **LZ4** → **Pack** → Target **.ffpkg** (or `.exfat`, `.ffpfs`,
   `.ffpfsc`), CLI `convert <dump> --to ffpkg --lz4-pack [--lz4-traces <zip|folder|journal>]
   [--lz4-profile x.toml]`. `.fpkg` is not offered (packing into a package is not supported yet). The
   image is verified as written and through its packs (the logical files against the source).

**Recommended: a traced `.ffpkg` on the PS5, pack on the computer.** A plain `.ffpkg` of a game launched
where the same game's folder patched in place did not (see below), so record in an image:

1. **Patch the image.** Make a plain `.ffpkg` (Convert), then in **LZ4** → **Trace** pick it and choose
   **Patch**, with enough **Room for the trace** (256 MiB by default, 64 MiB to 1 GiB in 64 MiB steps). Forge writes a patched copy
   beside the image, verifies it, and only then replaces the image with it, so it needs free space about
   the image's size next to it for a while; if anything fails, the image stays as it was. CLI:
   `ps5-dump-forge convert <image.ffpkg> --lz4-trace --lz4-in-place [--lz4-trace-space MiB]`. (Or convert a
   traced copy to a new image: `--to ffpkg --lz4-trace`.)
2. **Mount it writable** with ShadowMountPlus (`image_rw=`): a read-only mount records nothing.
3. **Play** one long session. Each launch overwrites the previous session's trace.
4. **Close the game and wait about a minute**: while the image is mounted, its file on disk can lag
   behind what the game wrote.
5. **Download traces.** Open the console's Forge in a **computer's** browser (`http://<console IP>:8095`), pick
   the traced image (or its mounted folder under `/mnt/shadowmnt`) in **LZ4** → **Trace**, choose **Download traces** and
   download one zip, `[Game title]-[TITLE_ID]-amprtrace.zip`, holding `ampr_commands.bin` and
   `ampr_emu.index` (stored, not compressed: journals are already dense). A journal cut off at the end
   still packs: the reader stops at the last whole record. A download fails (rather than mix two
   sessions) if the game or a sync changes the source meanwhile: close the game and download again.
6. **Pack** the original dump on the computer: `--to lz4 --lz4-traces <the zip>` (CLI) / **LZ4** →
   **Pack** → **Traces**, pick the zip (UI). No need to unzip: Forge reads the zip directly and checks each
   file's CRC-32. A folder holding both files (the same control, or the CLI path of that folder) and the CLI's
   `ampr_commands.bin` path with the index beside it work too. The traces must belong to this dump: every file the
   copied index lists (the runtime, index, journal and logs aside) must be in it, so pack the same dump
   the traced copy was made from. Extra files in the dump (a scene `.nfo`, say) are fine: the game never
   read them, so they stay loose, and the log names them. A profile and traces exclude each other.

Or copy the whole traced image back to the computer and pack it directly (**LZ4** → **Pack** on the
image: it packs by its own traces, and says so). That moves the whole game instead of two files.

**Patching a folder** (in place, no second copy; CLI: `ps5-dump-forge lz4-patch <folder>`) is offered but
not recommended: on the console a patched folder failed to launch (CE-107750-0, cause unknown, even after
restoring the stock files). It changes the folder itself: the trace runtime replaces whatever
`fakelib/libSceAmpr.sprx` was there, the last session's `ampr_commands.bin` and logs are deleted, and a
fresh `ampr_emu.index` is written. Only a plain, unpacked folder of an AMPR title. Its traces are taken
off the console the same way (Download traces on the folder, or copy both files with any file manager or FTP).

**Unpatch** (**LZ4** → **Trace** → **Unpatch**; CLI: `ps5-dump-forge lz4-unpatch <folder>`, or
`convert <image> --lz4-unpatch --lz4-in-place` for an `.exfat`/`.ffpkg`) undoes a patch: Forge's release
runtime (0.4.2.1) replaces whatever runtime is there (no backup of an earlier one exists), the journal and
logs are deleted and a fresh `ampr_emu.index` is written. A folder changes in place; an image is rebuilt
and replaced like Patch. `--lz4-unpatch` also works on an ordinary convert into a new output.

`--lz4-unpack` (converting to anything with the option on; UI: **LZ4** → **Unpack**) restores the loose assets byte for byte; the
runtime and its `ampr_emu.index` stay, so the folder keeps working.

Things to know:

- **The trace needs a writable game location.** The journal is written on the title's `/app0`: a plain
  folder, or an `.exfat` / `.ffpkg` the console mounts read-write (`image_rw=`). Forge leaves
  `--lz4-trace-space` MiB (default 256, 64 to 1024 in steps of 64) of free room in such an image. Measured on Stellar Blade: about 2 MB of journal per 5 minutes of heavy loading, ≈ 25 MB/hour, so 256 MiB ≈ 10 hours.
  `.ffpfs`, `.ffpfsc` and `.pkg` cannot be traced.
- **Only AMPR titles.** A game whose `eboot.bin` does not import `libSceAmpr` is refused.
- **Patch and Unpatch are the only operations that change your source.** Keep the original dump to pack
  from. Only a folder, `.exfat` or `.ffpkg` can be patched (`.ffpfs`, `.ffpfsc` and `.pkg` are read-only on
  the console), and a packed source must be unpacked first.
- **Checks are CRCs.** The pack's checksums detect corruption; they do not authenticate anything.
- **Barely tested on a console.** A folder patched in place did not launch (CE-107750-0); the traced
  `.ffpkg` path is untested. Sizes and speeds the ampr_emu project mentions are upstream's claims; Forge
  has not measured them. Whether a given title runs, loads faster or saves reliably is open (see TODO.md).

Credits: **ampr_emu 0.4.2.1 by [drakmor](https://github.com/drakmor/ampr_emu) (GPL-3.0)**, whose unmodified
runtimes are embedded (source archive in `vendor/ampr_emu/`); [Lazy_AMPR](https://github.com/Nazky/Lazy_AMPR)
as the reference workflow (no code taken). Bundled LZ4 (BSD-2-Clause) and HDE64 notices are in
`THIRD-PARTY-NOTICES.md`.

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
