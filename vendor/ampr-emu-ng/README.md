# ampr-emu-ng v0.3.1 (release binaries and source)

The unmodified v0.3.1 release of ampr-emu-ng, the AMPR emulation runtime (a Rust hard fork of ampr_emu)
that reads zstd packs (`ampr_zstd.index` + `ampr_zstd-NNN.pak`, format in `docs/zstd.md`) and LZ4 packs
(`AMPRPAK4`), plus the tag's source archive. It serves fully packed games of either format (everything but
executables, `sce_sys`, `sce_module` and `fakelib`: the "pack everything except system files" strategy).

| | |
|---|---|
| Upstream | https://github.com/quer3q/ampr-emu-ng |
| Release | https://github.com/quer3q/ampr-emu-ng/releases/tag/v0.3.1 |
| Tag | `v0.3.1` (tag object `72e4ce91ff1b2bd4f6262d97bb13ca927b4c08a5`, commit `c9721cc645a52913c55ce19bc48fc03c85b9d52e`) |
| Local changes | none: the binaries are the release assets byte for byte |

| File here | Upstream asset | Bytes | SHA-256 |
|---|---|---:|---|
| `libSceAmpr.sprx` | `libSceAmpr.sprx` (release build) | 694256 | `bdd93ce2fbcc2d0986b16d77604cc0770d994ccc124b9051ba9f53aeac39f56f` |
| `libSceAmpr-debug.sprx` | `libSceAmpr-debug.sprx` (debug build) | 776224 | `b767fdf25b341e0fb52c5c758f58d2d1ac71251312d5eda43d61ec6ae0d57ce9` |
| `SHA256SUMS` | `SHA256SUMS` | 170 | `9702cde0b5465ff56af905ba452a375ef91662521c984d81e1013239ea909ea1` |
| `ampr-emu-ng-0.3.1-src.tar.gz` | `git archive --prefix=ampr-emu-ng-0.3.1/ v0.3.1`, gzip -n -9 | 478232 | `d3983124fccc0424a270266d7bfb8ce440e9ff009eb75606f1afc4d3cbc2bb64` |

The three release files match the digests GitHub lists for the assets. Check: `shasum -a 256 -c SHA256SUMS`
in this folder. The release binary carries the strings `ampr-emu-ng 0.3.1`, `AMPRZPK1`, `AMPRPAK4` and
`AMPRDAT3`.

## Used by the build

`ps5-dump-forge-lz4` embeds `libSceAmpr.sprx` from here: `crates/ps5-dump-forge-lz4/build.rs` stores it
zstd-compressed (`ZSTD_PACKED`, `ZSTD_LEN`) and `runtime::zstd()` decodes it on first use; a test pins its
SHA-256 (`ZSTD_SHA256`, the value in `SHA256SUMS`). An upgrade changes the path in `build.rs` and that hash.
Forge installs it at `fakelib/libSceAmpr.sprx` on every zstd Pack and on an LZ4 Pack with runtime `ng` (the
default), and keeps it there and in a zstd-packed game converted as it is. Every other conversion (a plain
convert, an LZ4 Unpack, an LZ4 Pack with `ampr_emu`, a zstd Unpack) replaces it with the upstream release
runtime (`vendor/ampr_emu/`).

Older ampr-emu-ng builds stay pinned in `runtime.rs` so games carrying one are still recognized as
ampr-emu-ng (kept or replaced by the rule above):

| Pin | Build | SHA-256 |
|---|---|---|
| `V030_ZSTD_SHA256` | v0.3.0 release (694192 bytes) | `5f39ca8ebe169aa15d8a8ba8ec2f9cd4195f509d55daaf4b4edbfa80b145035c` |
| `NG_6972875_SHA256` | local build of commit 6972875 (after v0.2.0, 690736 bytes), embedded before v0.3.0 | `dc7d59d551071dc7ad2002865efe9e54ea0393523d1258ec1ce9fc7389a45bc4` |
| `V020_ZSTD_SHA256` | v0.2.0 release | `7acb2173d9691b5dfb94ea4007660920b7250111c5fd7a86c6eb91b763ad3d76` |
| `PREVIOUS_ZSTD_SHA256` | v0.1.0 release | `69ee3b420c6798fa1b5fb5a9fed75479403bc8a64fe382db301477b1bf705956` |
| `OLD_ZSTD_SHA256` | local build embedded before v0.1.0 (`vendor/ampr_emu/libSceAmpr-zstd.sprx`, 679712 bytes) | `0bf2c3e39580d937c20bc320323f6394ff624fb5839fc1160ef27ceea764a4f7` |

Any other build holding the `AMPRZPK1`, `AMPRPAK4` and `AMPRDAT3` magics counts as ampr-emu-ng too. The
debug build is kept for bring-up only and is never embedded.

## Corresponding source

`ampr-emu-ng-0.3.1-src.tar.gz` is the `v0.3.1` tag's source archive, kept as the corresponding source of the
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
this README (version, tag, dates, sizes, hashes, URLs) and the pin in `runtime.rs` (`ZSTD_SHA256` and the
size and version in `pinned_sizes_and_hashes`) together. Keep the replaced build's hash pinned with the
others above so games carrying it are still kept.
