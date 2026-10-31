# Pack: zstd and LZ4 asset packs

An optional transform for games that load through `libSceAmpr` (the title's `eboot.bin` imports it): the
game's assets are compressed into a few pack volumes, and a runtime that Forge installs at
`fakelib/libSceAmpr.sprx` reads them back on the console while the game runs. The result is a plain game
**folder**, or an image holding one. Packs are not an image format: ShadowMountPlus mounts no "pack images".

Everything is in the app's **Pack** tab (**How pack works** there shows the steps in short, offline). Choose
the kind of packs first:

- **zstd** (the default): zstd packs, `ampr_zstd.index` + `ampr_zstd-NNN.pak` (`docs/PAK.md` has the format).
- **LZ4** (experimental): ampr_emu's asset packs, `ampr_assets.index` + `ampr_assets-NNN.pak`.

Both are read by **ampr-emu-ng**, the bundled runtime Pack installs by default. An LZ4 Pack can install the
bundled upstream [ampr_emu](https://github.com/drakmor/ampr_emu) 0.4.2.1 instead.

A plain game gets **Pack**; a game already packed by the kind chosen gets **Unpack**. A game packed by the
other kind must be unpacked under that kind first. A game that doesn't use AMPR, a patch package and
additional content get a note and no action.

> **ShadowMount+ can replace the runtime.** Its fakelib update replaces ampr-emu-ng, and the game then
> can't read its packs: turn that update off for packed games. The Pack tab
> says so whenever Pack installs ampr-emu-ng.

> **ampr_emu 0.4.2.1 is an upstream test build; known issue: some games crash when saving.** Forge logs
> this on every LZ4 Pack with runtime ampr_emu. Keep a copy of the original game.

## zstd packs

**Pack** writes a new game folder or goes straight into an `.ffpkg`, `.exfat` or `.ffpfs` (the CLI also
`.ffpfsc`; a `.pkg` is refused). Pick a preset: **Fast** (zstd level 5, the default), **Medium** (9) or
**Max** (19: the smallest packs, much slower to build and more memory). The CLI takes any level from 1 to 22:

```sh
ps5-dump-forge convert <game> --to folder --zstd-pack [--zstd-level 1..22]   # also exfat, ffpkg, ffpfs, ffpfsc
```

Forge installs ampr-emu-ng at `fakelib/libSceAmpr.sprx` (replacing whatever is there), leaves out the old
journal (`ampr_commands.bin`) and logs, and writes a fresh `ampr_emu.index` that lists every file, loose and
packed. Files go into 256 KiB chunks, each one zstd frame, or stored raw when zstd doesn't make it smaller.
Which files are packed follows LZ4's built-in rules (below; no auto-loose): executables, `sce_sys`,
`sce_module`, `fakelib`, files at the game's root and the keep-loose list stay loose. An LZ4-packed source
repacked as zstd keeps the set its LZ4 packs held. Into an image there is no temporary folder: as for LZ4
(below), Forge measures the packs first (about 16 bytes per chunk), then makes each chunk again while the
image is written and checks it against the measurement. The job is verified through the packs (the index,
every volume header, then the game's files read back against the source).

**Unpack** writes the packed files back as plain files, into a folder (the default), `.ffpkg`, `.exfat`,
`.ffpfs` or fpkg (CLI: any target but `lz4`). The bundled ampr_emu release runtime replaces ampr-emu-ng and a
fresh `ampr_emu.index` is written:

```sh
ps5-dump-forge convert <zstd_packed_game> --to folder --zstd-unpack
```

## LZ4 packs

**Pack** compresses the game's assets with the LZ4 library into an **LZ4 packed folder** (the default) or
straight into a `.ffpkg`, `.exfat` or `.ffpfs` (the CLI also `.ffpfsc`; a `.pkg` is not supported). Pick a
preset: **Fast** (LZ4's default compressor, the default), **Medium** (LZ4 HC 9) or **Max** (LZ4 HC 12: the
smallest packs, the slowest). The CLI takes 0 (Fast) or an HC level from 3 to 12. Into an image there is no
temporary folder: Forge measures the packs first (compressing without writing, keeping about 16 bytes per
chunk; the job peaks near 36 bytes per chunk, about 50 MB for a 91 GB game), lays the image out from their
exact sizes, then makes each chunk again while the image is written and checks it against the measurement.
The packed files are read twice. The image is verified as written and through its packs (the logical files
against the source).

```sh
ps5-dump-forge convert <game> --to lz4 [--lz4-level 0|3..12] [--lz4-runtime ng|ampr_emu]   # an LZ4 packed folder
ps5-dump-forge convert <game> --to ffpkg --lz4-pack [--lz4-level N]   # also exfat, ffpfs, ffpfsc
```

Which files to pack comes from the **built-in rules**: everything but the game's root files and the
keep-loose list. Packed files exist for the game only once it has started AMPR, so anything it reads before
that must stay loose. The **keep-loose list**: container indexes (`.pak`, `.utoc`), configs and text (`.ini`,
`.json`, ...), Unreal project files (`.uproject`, `.uplugin`, `.upluginmanifest`), images, video (`.bik`,
`.mpg`, `.mpeg`, `.m2v`, ...), audio and the protected folders (`sce_module`, `system`, `save`, ...). Then
**auto-loose** (as upstream's packer): a file of 64 MiB or more left to compress is sampled (32 blocks spread
across it, at most 16 MiB) and stays loose when the samples save under 5% or 90% of them stay uncompressed.

The runtime (**Runtime** in the tab, `--lz4-runtime`): **ampr-emu-ng** (`ng`, the default) is installed
unless the game already has it. **ampr_emu** installs the bundled release runtime where there is none and
replaces an older one, one whose version can't be read, one that can't read packs and ampr-emu-ng; ampr_emu
at Forge's version or newer that reads packs stays. The old journal and logs are left out and
`ampr_emu.index` is always written afresh.

**Unpack** (`--lz4-unpack`; automatic with `--to lz4`) restores the loose assets byte for byte, into a folder
(the default), `.ffpkg`, `.exfat` or `.ffpfs` (the CLI any target). The runtime stays unless it is outdated
(below), and so does an `ampr_emu.index` that lists exactly the restored files, so the folder keeps working.

```sh
ps5-dump-forge convert <lz4_packed_game> --to folder --lz4-unpack
```

## A packed game as a source

A zstd or LZ4 packed game (labelled **zstd** or **LZ4** on the source card and in the Library) is an ordinary source. Convert copies it to any format, a debug FPKG (`.pkg`) included, with its
packs as plain files: its `ampr_emu.index` (which lists the files inside the packs), pack index or manifest,
journal and logs are kept byte for byte, and the package builder packages that index as it is. zstd packs
get ampr-emu-ng (kept, else installed or put in place of any other runtime); an LZ4 packed game follows
the runtime rule below. To change the packed files, unpack first; **Remove backport** is refused for
a packed game for the same reason. Edit changes a packed game like any other but leaves its packs, its pack
index and `ampr_emu.index` as they are: a change to a packed file is refused.

## Every conversion of an AMPR game

Any conversion of an AMPR title (Convert, the CLI, the console web UI and the Pack tab's jobs; every target)
fixes two things in the output by itself, with nothing to choose; Edit, Convert and the **Pack** tab say
which. The source is left as it is.

- **The runtime** (`fakelib/libSceAmpr.sprx`). Installed: a game without its own ampr_emu gets the bundled
  release runtime (0.4.2.1). Replaced: Forge's old trace runtime, an ampr_emu older than Forge's, one whose
  version can't be read, and ampr-emu-ng: it goes into a game only by packing, so any other conversion
  (an LZ4 Unpack and a zstd Unpack included) puts the release runtime in its place. Kept: Forge's release
  runtime, or another ampr_emu at Forge's version or newer; into LZ4 packs (LZ4 Pack, or an LZ4 packed game
  copied as it is) only if it can also read them, else it is replaced. Packing sets its own runtime: a zstd
  Pack and an LZ4 Pack with ampr-emu-ng (the default) keep ampr-emu-ng or put it in, and a zstd packed game
  copied as it is keeps ampr-emu-ng and gets it in place of any other runtime.
- **The index.** `ampr_emu.index` stays only when it lists exactly the files written, with their sizes;
  otherwise (stale, damaged or missing) a fresh one is written, and the journal and logs are left out with
  the old one, since the journal names files by the old index's order. The files written include the
  runtime the output gets: an index that lists the old runtime's size (or no runtime) is rebuilt, one that
  already lists the release runtime at its size stays. **Remove backport** leaves files out, so it makes an
  index stale too. An index far bigger than the game's file list needs is judged stale without being read.
  An fpkg (`.pkg`) always gets a fresh index from the package builder, built from the files it actually
  packages.
- **A packed game** gets the same runtime rule, but keeps its index, packs, journal and logs byte for byte
  (above). A replaced runtime then no longer matches the size an LZ4 manifest lists; Forge's reader allows
  that for the runtime alone. Not yet confirmed on the console.

## Things to know

- **Only AMPR titles.** A game whose `eboot.bin` does not import `libSceAmpr` is refused (the **Pack** tab
  says "This game doesn't use AMPR, so zstd packs can't be used.", or LZ4 packs; Convert shows no AMPR
  notes for it). A game without its own ampr_emu is fine: Pack installs a runtime.
- **Pack never changes your source**: it writes a new folder or image. Delete (in Edit or Storage) is what
  removes a game.
- **Checks are CRCs.** The packs' checksums detect corruption; they do not authenticate anything.

Credits for ampr_emu and Lazy_AMPR are in the [README](../README.md#thanks); the LZ4 library and HDE64 notices in
[THIRD-PARTY-NOTICES.md](../THIRD-PARTY-NOTICES.md).
