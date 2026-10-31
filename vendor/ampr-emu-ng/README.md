# ampr-emu-ng 0.1.0 (release binaries and source)

The release assets of ampr-emu-ng v0.1.0, the AMPR emulation runtime (a Rust hard fork of ampr_emu)
that reads zstd packs (`ampr_zstd.index` + `ampr_zstd-NNN.pak`, format in `docs/PAK.md`) and LZ4 packs
(`AMPRPAK4`), plus the tagged source archive.

| | |
|---|---|
| Upstream | https://github.com/quer3q/ampr-emu-ng |
| Release / tag | `v0.1.0` (https://github.com/quer3q/ampr-emu-ng/releases/tag/v0.1.0), commit `61cb12274a182a5fc696d8319a70b9f5c4d21b6b` |
| Published | 2026-10-29 |
| Downloaded | 2026-10-29 |
| Local changes | none: every file is the release asset byte for byte |

| File here | Upstream asset | Bytes | SHA-256 |
|---|---|---:|---|
| `libSceAmpr.sprx` | `libSceAmpr.sprx` (release build) | 667408 | `69ee3b420c6798fa1b5fb5a9fed75479403bc8a64fe382db301477b1bf705956` |
| `libSceAmpr-debug.sprx` | `libSceAmpr-debug.sprx` (debug build) | 709104 | `297f8d2cd279fdaca5b944c251cd7ebba202a483dbfe14a3e6c37cc4b444ae44` |
| `SHA256SUMS` | `SHA256SUMS` (the two `.sprx` above) | 170 | `5babf27f27d27ba9baefe33543eab9f7d177b13ac736de41ef8368482532ce2b` |
| `ampr-emu-ng-0.1.0-src.tar.gz` | `archive/refs/tags/v0.1.0.tar.gz` | 378968 | `f98dff789a873c8aca29cf0f6a441a7904c68d2c91b78bf1285909159d90990d` |

Check: `shasum -a 256 -c SHA256SUMS` in this folder. The release binary carries the strings
`ampr-emu-ng 0.1.0`, `AMPRZPK1`, `AMPRPAK4` and `AMPRDAT3`.

Download URLs:

```text
https://github.com/quer3q/ampr-emu-ng/releases/download/v0.1.0/libSceAmpr.sprx
https://github.com/quer3q/ampr-emu-ng/releases/download/v0.1.0/libSceAmpr-debug.sprx
https://github.com/quer3q/ampr-emu-ng/releases/download/v0.1.0/SHA256SUMS
https://github.com/quer3q/ampr-emu-ng/archive/refs/tags/v0.1.0.tar.gz
```

## Used by the build

`ps5-dump-forge-lz4` embeds `libSceAmpr.sprx` from here: `crates/ps5-dump-forge-lz4/build.rs` stores it
zstd-compressed (`ZSTD_PACKED`, `ZSTD_LEN`) and `runtime::zstd()` decodes it on first use; a test pins its
SHA-256 (`ZSTD_SHA256`, the value in `SHA256SUMS`). An upgrade changes the path in `build.rs` and that hash.
Forge installs it at `fakelib/libSceAmpr.sprx` on every zstd Pack and on an LZ4 Pack with runtime `ng` (the
default), and keeps it there and in a zstd-packed game converted as it is. Every other conversion (a plain
convert, an LZ4 Unpack, an LZ4 Pack with `ampr_emu`, a zstd Unpack) replaces it with the upstream release
runtime (`vendor/ampr_emu/`).

The local build Forge embedded before this release (`vendor/ampr_emu/libSceAmpr-zstd.sprx`, 679712 bytes,
SHA-256 `0bf2c3e39580d937c20bc320323f6394ff624fb5839fc1160ef27ceea764a4f7`) is no longer in the repo; its
hash stays pinned (`OLD_ZSTD_SHA256`) so a game carrying it is recognized as ampr-emu-ng (kept or replaced by
the rule above). Any other build holding the `AMPRZPK1`, `AMPRPAK4` and `AMPRDAT3` magics counts as ampr-emu-ng
too. The debug build is kept for bring-up only and is never embedded.

## Corresponding source

`ampr-emu-ng-0.1.0-src.tar.gz` is the tagged source archive, kept as the corresponding source of the
binaries so nothing depends on upstream staying online (by user decision: vendored, never downloaded at
build or check time). `ps5/build-docker.sh` in it builds `libSceAmpr.sprx` and `libSceAmpr-debug.sprx`. The
exact environment of the release build is not recorded; this is not a claim that the archive reproduces
the binaries byte for byte.

## Licenses

- ampr-emu-ng: GPL-3.0 (`LICENSE` in the archive). Forge is GPL-3.0-or-later too.
- Compiled in, per the archive's `Cargo.toml` files: `zstd-sys` 2.1.2 (libzstd, BSD-3-Clause or GPL-2.0),
  `lz4_flex` 0.14.0 (MIT), `crc32fast` 1.5.3 (MIT or Apache-2.0).

## Upgrading

Replace the files with a newer release's assets, check them with `shasum -a 256 -c SHA256SUMS`, and update
this README (version, tag commit, dates, sizes, hashes, URLs) and the pin in `runtime.rs` (`ZSTD_SHA256` and
the size in `pinned_sizes_and_hashes`) together. Keep the replaced release's hash pinned beside
`OLD_ZSTD_SHA256` so games carrying it are still kept.
