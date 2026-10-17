# LZ4 asset packs (ampr_emu)

A separate, optional transform for games that load through `libSceAmpr` (the title's `eboot.bin` imports
it): the game's assets are LZ4-compressed into a few `ampr_assets-NNN.pak` volumes, and the
[ampr_emu](https://github.com/drakmor/ampr_emu) runtime (`libSceAmpr.sprx`, installed by Forge under
`fakelib/`) reads them back on the console while the game runs. The result is still a plain game
**folder**. It is not an image format and ShadowMountPlus does not mount "LZ4 images". Pack can also write
the packed game straight into an `.ffpkg`, `.exfat`, `.ffpfs` or `.ffpfsc` (the image holds the packs and
loose files as plain files), with no temporary folder: Forge measures the packs first (compressing without
writing, keeping about 16 bytes per chunk; the job peaks near 36 bytes per chunk, about 50 MB for a 91 GB
game), lays the image out from their exact sizes, then makes each chunk again while the image is written
and checks it against the measurement. The packed files are read twice. Converting a packed folder to an
image is still an ordinary second convert (the packs are carried along).

> **ampr_emu 0.4.2.1 is an upstream test build; known issue: some games crash when saving.** Forge logs
> this on every trace and pack job.

Everything LZ4 is in the app's **LZ4** tab (experimental; **How LZ4 works** there shows the steps below
in short, offline), in two scenarios: **Trace** (Patch or Unpatch a game folder, `.exfat` or `.ffpkg`;
Download traces in the console web UI once it holds a journal) and **Pack/Unpack** (pack by traces or a
profile into an LZ4 packed folder, `.ffpkg`, `.exfat` or `.ffpfs`; Unpack a packed source to a folder,
`.ffpkg`, `.exfat` or `.ffpfs`). `.ffpfsc` is CLI only there; any packed or unpacked folder converts to
every format in Convert. The Convert tab copies a packed folder as it is, except an outdated or missing runtime
(see [Every conversion of an AMPR game](#every-conversion-of-an-ampr-game)). For a game that doesn't use
AMPR the **LZ4** tab says "This game doesn't use AMPR, so LZ4 can't be used here" and offers nothing.

## How it works

Which files to pack comes from a recording, so it takes two converts:

1. **Trace.** Patch the dump: **LZ4** → **Trace** → **Patch** (UI) on a folder, `.exfat` or `.ffpkg`, or
   convert it with `--lz4-trace` (CLI) to a folder or image. Forge installs ampr_emu's trace runtime,
   which records every file the game reads into `ampr_commands.bin` on the title's own storage. Run it on
   the console and play one long session through everything that matters (levels, languages, DLC). Only
   the **last** session is used.
2. **Pack.** Convert the traced dump (or the folder you copied back) with `--to lz4` / **LZ4** →
   **Pack/Unpack**. Forge reads the journal, packs the files the game read, except those the engine opens
   directly (container indexes, configs, media) and files at the game's root, leaves the rest loose, swaps
   the trace runtime for the release one (and installs it where the game has none, or replaces an older
   ampr_emu, one that names no version, or one that can't read packs; a build at Forge's version or newer
   that reads packs stays) and removes the journal and logs. Or pass `--lz4-profile x.toml` to choose the files
   yourself; without a recording or a profile it falls back to a built-in guess, which is less reliable.

   Packed files exist for the game only once it has started AMPR, so anything it reads before that must
   stay loose. Whatever the rule source (a profile too), Forge's **keep-loose list** wins: container
   indexes (`.pak`, `.utoc`), configs and text (`.ini`, `.json`, ...), Unreal project files (`.uproject`,
   `.uplugin`, `.upluginmanifest`), images, video (`.bik`, `.mpg`, `.mpeg`, `.m2v`, ...), audio and the
   protected folders (`sce_module`, `system`, `save`, ...); the log names what a profile wanted packed.
   Then **auto-loose** (as upstream's packer): a file of 64 MiB or more left to compress is sampled
   (32 blocks spread across it, at most 16 MiB) and stays loose when the samples save under 5% or 90% of
   them stay uncompressed; hot files (a profile's `hot = true`) are exempt unless `auto_loose_hot_files = true`. A profile tunes it with the
   `[pack] auto_loose_*` keys (`auto_loose_large_files = false` turns it off).
3. Or pack straight into an image: **LZ4** → **Pack/Unpack** → Target **.ffpkg** (or `.exfat`, `.ffpfs`;
   the CLI also `.ffpfsc`), CLI `convert <dump> --to ffpkg --lz4-pack [--lz4-traces <zip|folder|journal>]
   [--lz4-profile x.toml]`. `.fpkg` is not offered (packing into a package is not supported yet). The
   image is verified as written and through its packs (the logical files against the source).

## Recommended: a traced `.ffpkg` on the PS5, pack on the computer

1. **Patch the image.** Make a plain `.ffpkg` (Convert), then in **LZ4** → **Trace** pick it and choose
   **Patch**, with enough **Room for the trace** (256 MiB by default, 64 MiB to 1 GiB in 64 MiB steps).
   Forge writes a patched copy beside the image, verifies it, and only then replaces the image with it, so
   it needs free space about the image's size next to it for a while; if anything fails, the image stays as
   it was. CLI: `ps5-dump-forge convert <image.ffpkg> --lz4-trace --lz4-in-place [--lz4-trace-space MiB]`.
   (Or convert a traced copy to a new image: `--to ffpkg --lz4-trace`.)
2. **Mount it writable** with ShadowMountPlus (`image_rw=`): a read-only mount records nothing. Turn off any
   fakelib updater while tracing: it puts its own `fakelib/libSceAmpr.sprx` back, and nothing is recorded.
3. **Play** one long session. Each launch overwrites the previous session's trace.
4. **Close the game and wait about a minute**: while the image is mounted, its file on disk can lag
   behind what the game wrote.
5. **Download traces.** Open the console's Forge in a **computer's** browser (`http://<console IP>:8095`),
   pick the traced image (or its mounted folder under `/mnt/shadowmnt`) in **LZ4** → **Trace**, choose
   **Download traces** and download one zip, `[Game title]-[TITLE_ID]-amprtrace.zip`, holding
   `ampr_commands.bin` and `ampr_emu.index` (stored, not compressed: journals are already dense). A
   journal cut off at the end still packs: the reader stops at the last whole record. A download fails
   (rather than mix two sessions) if the game or a sync changes the source meanwhile: close the game and
   download again.
6. **Pack** the original dump on the computer: `--to lz4 --lz4-traces <the zip>` (CLI) / **LZ4** →
   **Pack/Unpack** → **Traces**, pick the zip (UI). No need to unzip: Forge reads the zip directly and
   checks each file's CRC-32. A folder holding both files (the same control, or the CLI path of that
   folder) and the CLI's `ampr_commands.bin` path with the index beside it work too. The traces must
   belong to this dump: every file the copied index lists (the runtime, index, journal and logs aside) must
   be in it, so pack the same dump the traced copy was made from. Extra files in the dump (a scene `.nfo`,
   say) are fine: the game never read them, so they stay loose, and the log names them. A profile and
   traces exclude each other.

Or copy the whole traced image back to the computer and pack it directly (**LZ4** → **Pack/Unpack** on the
image: it packs by its own traces, and says so). That moves the whole game instead of two files.

## Patch a folder, Unpatch, Unpack

**Patching a folder** (in place, no second copy; CLI: `ps5-dump-forge lz4-patch <folder>`) is offered, but
the traced image above is the recommended path: on the console a folder patched in place by an earlier
version failed to launch (CE-107750-0). It changes the folder itself: the trace runtime replaces whatever
`fakelib/libSceAmpr.sprx` was there, the last session's `ampr_commands.bin` and logs are deleted, and a
fresh `ampr_emu.index` is written. Only a plain, unpacked folder of an AMPR title. Its traces are taken
off the console the same way (Download traces on the folder, or copy both files with any file manager or
FTP).

**Unpatch** (**LZ4** → **Trace** → **Unpatch**; CLI: `ps5-dump-forge lz4-unpatch <folder>`, or
`convert <image> --lz4-unpatch --lz4-in-place` for an `.exfat`/`.ffpkg`) undoes a patch: Forge's release
runtime (0.4.2.1) replaces whatever runtime is there (no backup of an earlier one exists), the journal and
logs are deleted and a fresh `ampr_emu.index` is written. A folder changes in place; an image is rebuilt
and replaced like Patch. `--lz4-unpatch` also works on an ordinary convert into a new output (CLI): the
bundled release runtime replaces any runtime, even one as new as Forge's that a conversion would keep, and
the journal, logs and old index always stay out. Without it, a conversion already installs or updates the
runtime and refreshes a stale index by itself (below).

`--lz4-unpack` (converting to anything with the option on; UI: **LZ4** → **Pack/Unpack** on a packed
source) restores the loose assets byte for byte; the runtime stays unless it is outdated (below) and so does
an `ampr_emu.index` that lists exactly the restored files, so the folder keeps working.

## Every conversion of an AMPR game

Any conversion of an AMPR title (Convert, the CLI, the console web UI and the LZ4 tab's jobs; every target)
fixes two things in the output by itself, with nothing to choose; Edit, Convert and the **LZ4** tab say
which. The source is left as it is.

- **The runtime** (`fakelib/libSceAmpr.sprx`). Installed: a game without its own ampr_emu gets the bundled
  release runtime (0.4.2.1). Replaced: Forge's trace runtime, an ampr_emu older than Forge's, or one whose
  version can't be read. Kept: Forge's release runtime, or another ampr_emu at Forge's version or newer;
  into packs (Pack, or a packed game copied as it is) only if it can also read them, else it is replaced.
  A trace or Patch still puts the trace runtime in, and Unpatch the release one, whatever is there.
- **The index.** `ampr_emu.index` stays only when it lists exactly the files written, with their sizes;
  otherwise (stale, damaged or missing) a fresh one is written, and the journal and logs are left out with
  the old one, since the journal names files by the old index's order. The files written include the
  runtime the output gets: an index that lists the old runtime's size (or no runtime) is rebuilt, one that
  already lists the release runtime at its size stays. **Remove backport** leaves files out, so it makes an
  index stale too. An index far bigger than the game's file list
  needs is judged stale without being read. A `.fpkg` always gets a fresh index from the package builder,
  built from the files it actually packages.
- **A packed game** gets the same runtime rule, but its `ampr_emu.index`, pack manifest, journal and logs
  are copied byte for byte (the manifest's file ids are the index's records). A replaced runtime then no
  longer matches the size the manifest lists; Forge's reader allows that for the runtime alone. Not yet
  confirmed on the console.

## Things to know

- **The trace needs a writable game location.** The journal is written on the title's `/app0`: a plain
  folder, or an `.exfat` / `.ffpkg` the console mounts read-write (`image_rw=`). Forge leaves
  `--lz4-trace-space` MiB (default 256, 64 to 1024 in steps of 64) of free room in such an image. Measured
  on Stellar Blade: about 2 MB of journal per 5 minutes of heavy loading, ≈ 25 MB/hour, so 256 MiB ≈ 10
  hours. `.ffpfs`, `.ffpfsc` and `.pkg` cannot be traced.
- **Only AMPR titles.** A game whose `eboot.bin` does not import `libSceAmpr` is refused (the **LZ4**
  tab says "This game doesn't use AMPR, so LZ4 can't be used here"; Convert shows no AMPR notes for it).
  A game without its own ampr_emu is fine: Pack, Unpack, Patch and Unpatch install the bundled one.
- **Patch and Unpatch are the only LZ4 operations that change your source** (Delete, in Edit or the
  Library, removes it). Keep the original dump to pack from. Only a folder, `.exfat` or `.ffpkg` can be
  patched (`.ffpfs`, `.ffpfsc` and `.pkg` are read-only on the console), and a packed source must be
  unpacked first.
- **Checks are CRCs.** The pack's checksums detect corruption; they do not authenticate anything.

Credits for ampr_emu and Lazy_AMPR are in the [README](../README.md#thanks); the LZ4 and HDE64 notices in
[THIRD-PARTY-NOTICES.md](../THIRD-PARTY-NOTICES.md).
