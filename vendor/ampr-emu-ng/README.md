# ampr-emu-ng 0.2.0 (release binaries and source)

The release assets of ampr-emu-ng v0.2.0, the AMPR emulation runtime (a Rust hard fork of ampr_emu)
that reads zstd packs (`ampr_zstd.index` + `ampr_zstd-NNN.pak`, format in `docs/zstd.md`) and LZ4 packs
(`AMPRPAK4`), plus the tagged source archive.

| | |
|---|---|
| Upstream | https://github.com/quer3q/ampr-emu-ng |
| Release / tag | `v0.2.0` (https://github.com/quer3q/ampr-emu-ng/releases/tag/v0.2.0), commit `fd058e43a49a490a5770374e84aec77648831b6a` |
| Published | 2026-11-02 |
| Downloaded | 2026-11-03 |
| Local changes | none: every file is the release asset byte for byte |

| File here | Upstream asset | Bytes | SHA-256 |
|---|---|---:|---|
| `libSceAmpr.sprx` | `libSceAmpr.sprx` (release build) | 678416 | `7acb2173d9691b5dfb94ea4007660920b7250111c5fd7a86c6eb91b763ad3d76` |
| `libSceAmpr-debug.sprx` | `libSceAmpr-debug.sprx` (debug build) | 747184 | `15fd9e82561f1dcd181126672164aabc8a8954db0d5d79d13641706bfdb31881` |
| `SHA256SUMS` | `SHA256SUMS` (the two `.sprx` above) | 170 | `6624abfa1f5615f42971abe5177de52b703ccb265ddb1bd5756c2b91568ac633` |
| `ampr-emu-ng-0.2.0-src.tar.gz` | `archive/refs/tags/v0.2.0.tar.gz` | 433150 | `4736daf60620d3addbce8b4dd806439ff84a733d5d9b1c260b4e52a30653a305` |

Check: `shasum -a 256 -c SHA256SUMS` in this folder. The release binary carries the strings
`ampr-emu-ng 0.2.0`, `AMPRZPK1`, `AMPRPAK4` and `AMPRDAT3`.

Download URLs:

```text
https://github.com/quer3q/ampr-emu-ng/releases/download/v0.2.0/libSceAmpr.sprx
https://github.com/quer3q/ampr-emu-ng/releases/download/v0.2.0/libSceAmpr-debug.sprx
https://github.com/quer3q/ampr-emu-ng/releases/download/v0.2.0/SHA256SUMS
https://github.com/quer3q/ampr-emu-ng/archive/refs/tags/v0.2.0.tar.gz
```

## Used by the build

`ps5-dump-forge-lz4` embeds `libSceAmpr.sprx` from here: `crates/ps5-dump-forge-lz4/build.rs` stores it
zstd-compressed (`ZSTD_PACKED`, `ZSTD_LEN`) and `runtime::zstd()` decodes it on first use; a test pins its
SHA-256 (`ZSTD_SHA256`, the value in `SHA256SUMS`). An upgrade changes the path in `build.rs` and that hash.
Forge installs it at `fakelib/libSceAmpr.sprx` on every zstd Pack and on an LZ4 Pack with runtime `ng` (the
default), and keeps it there and in a zstd-packed game converted as it is. Every other conversion (a plain
convert, an LZ4 Unpack, an LZ4 Pack with `ampr_emu`, a zstd Unpack) replaces it with the upstream release
runtime (`vendor/ampr_emu/`).

The v0.1.0 release's SHA-256 (`69ee3b420c6798fa1b5fb5a9fed75479403bc8a64fe382db301477b1bf705956`)
stays pinned as `PREVIOUS_ZSTD_SHA256` so games carrying it are still recognized as ampr-emu-ng.
The local build Forge embedded before v0.1.0 (`vendor/ampr_emu/libSceAmpr-zstd.sprx`, 679712 bytes,
SHA-256 `0bf2c3e39580d937c20bc320323f6394ff624fb5839fc1160ef27ceea764a4f7`) is no longer in the repo; its
hash stays pinned (`OLD_ZSTD_SHA256`) so a game carrying it is recognized as ampr-emu-ng (kept or replaced by
the rule above). Any other build holding the `AMPRZPK1`, `AMPRPAK4` and `AMPRDAT3` magics counts as ampr-emu-ng
too. The debug build is kept for bring-up only and is never embedded.

## Corresponding source

`ampr-emu-ng-0.2.0-src.tar.gz` is the tagged source archive, kept as the corresponding source of the
binaries so nothing depends on upstream staying online (by user decision: vendored, never downloaded at
build or check time). `ps5/build-docker.sh` in it builds `libSceAmpr.sprx` and `libSceAmpr-debug.sprx`. The
exact environment of the release build is not recorded; this is not a claim that the archive reproduces
the binaries byte for byte.

## Licenses

- ampr-emu-ng: GPL-3.0-or-later (the archive's crate `Cargo.toml` files and `LICENSE`). Forge is GPL-3.0-or-later too.
- Compiled in, per the archive's `Cargo.toml` and `Cargo.lock`: `zstd-sys` 2.1.2 (libzstd, BSD-3-Clause or GPL-2.0),
  `lz4_flex` 0.14.0 (MIT), `crc32fast` 1.5.3 (MIT or Apache-2.0) with `cfg-if` 1.0.5 (MIT or Apache-2.0), `iced-x86` 1.21.0 (MIT),
  `lazy_static` 1.5.1 (MIT or Apache-2.0), `spin` 0.9.9 (MIT).

## Upgrading

Replace the files with a newer release's assets, check them with `shasum -a 256 -c SHA256SUMS`, and update
this README (version, tag commit, dates, sizes, hashes, URLs) and the pin in `runtime.rs` (`ZSTD_SHA256` and
the size in `pinned_sizes_and_hashes`) together. Keep the replaced release's hash pinned beside
`OLD_ZSTD_SHA256` so games carrying it are still kept.
