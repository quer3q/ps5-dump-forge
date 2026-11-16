# ampr-emu-ng v0.4.1 (release binaries and source)

The unmodified v0.4.1 release of ampr-emu-ng, the AMPR emulation runtime (a Rust hard fork of ampr_emu)
that reads zstd packs (`ampr_zstd.index` + `ampr_zstd-NNN.pak`, format in `docs/zstd.md`) and LZ4 packs
(`AMPRPAK4`), plus the tag's source archive. It serves fully packed games of either format (everything but
executables, `sce_sys`, `sce_module`, `fakelib` and `.ini` files: the "pack everything except system files" strategy).

| | |
|---|---|
| Upstream | https://github.com/quer3q/ampr-emu-ng |
| Release | https://github.com/quer3q/ampr-emu-ng/releases/tag/v0.4.1 (published 2026-11-15) |
| Tag | `v0.4.1` (commit `8b61a06048717f9241a2d09e3db86031417d6d99`) |
| Local changes | none: the binaries are the release assets byte for byte |

| File here | Upstream asset | Bytes | SHA-256 |
|---|---|---:|---|
| `libSceAmpr.sprx` | `libSceAmpr.sprx` (release build) | 733104 | `9c7c41b3216bc4f877d67e3c2e5c9c8f9ffabdd5065f4f3fe0dd98fa7228386f` |
| `libSceAmpr-debug.sprx` | `libSceAmpr-debug.sprx` (debug build) | 855408 | `f82e26aa9306f142a4120eb1ab70c52403a840b8441f136ec88ce19f2c7f97a2` |
| `SHA256SUMS` | `SHA256SUMS` | 170 | `9f07f587de2cd7c22a6547fbefeb5b46942a2985af143a05b37edb05a9739b13` |
| `ampr-emu-ng-0.4.1-src.tar.gz` | `git archive --prefix=ampr-emu-ng-0.4.1/ v0.4.1`, gzip -n -9 | 636909 | `322f4c923439bf963ea02774d70a70e2e5cf276489253f8fbd9842e5f87058f3` |

The three release files match the digests GitHub lists for the assets. Check: `shasum -a 256 -c SHA256SUMS`
in this folder. The release binary carries the strings `ampr-emu-ng 0.4.1`, `AMPRZPK1`, `AMPRPAK4` and
`AMPRDAT3`.

Changes since v0.3.4, the build Forge embedded before (release notes). v0.3.5 (2026-11-11) and v0.4.0
(2026-11-13): FPKG (`.pkg`) games run (experimental); pack reads take one disk read per chunk, with a
64 MiB decoded cache; large LZ4 reads are split into parallel jobs; a pack index too big for memory is read
on demand; the zstd decoders no longer use the game's heap; a writable `/app0` game can create files beside
its packed files (the packed files stay read-only). v0.4.1 (2026-11-15): a compact in-memory file index with
no fixed 48 MiB cap (the "index tables need ... MiB, over the 48 MiB" failure: FF XVI, Demon's Souls,
Returnal, God of War Ragnarok in Forge issue #59); fixes for Spider-Man Miles Morales' save-thread crash,
AI LIMIT's CE-108255-1 and missing saves, Horizon Forbidden West's load stuck at 20% or 60%, packed folder
handles below 1024, and a folder hidden by a same-named file in another case; `/data/ampr_cache` is hidden
from file explorers. The zstd and LZ4 pack formats are unchanged (`crates/ampr-format`'s readers were
rewritten, the magics and versions are the same), and so are the third-party dependencies (`Cargo.lock`
bumps the workspace crates and adds the workspace crate `ampr-pkg`).

## Used by the build

`ps5-dump-forge-lz4` embeds `libSceAmpr.sprx` from here: `crates/ps5-dump-forge-lz4/build.rs` stores it
zstd-compressed (`ZSTD_PACKED`, `ZSTD_LEN`) and `runtime::zstd()` decodes it on first use; a test pins its
SHA-256 (`ZSTD_SHA256`, the value in `SHA256SUMS`). An upgrade replaces the files here (the path in `build.rs` names no version) and changes that hash.
Forge installs it at `fakelib/libSceAmpr.sprx` on every zstd Pack and on an LZ4 Pack with runtime `ng` (the
default), and keeps it there; a zstd-packed game converted as it is keeps an ampr-emu-ng at this version or newer and gets this one in place of an older one. Every other conversion (a plain
convert, an LZ4 Unpack, an LZ4 Pack with `ampr_emu`, a zstd Unpack) replaces it with the upstream release
runtime (`vendor/ampr_emu/`).

Older ampr-emu-ng builds stay pinned in `runtime.rs` so games carrying one are still recognized as
ampr-emu-ng (kept or replaced by the rule above):

| Pin | Build | SHA-256 |
|---|---|---|
| `V040_ZSTD_SHA256` | v0.4.0 release (775696 bytes) | `8320eb65c16565f42031b9d88b15a447039f0fd7e5694faf5f1cb84cafdba218` |
| `V035_ZSTD_SHA256` | v0.3.5 release (751504 bytes) | `998f2029b250d954725f864b57f2eee72aefbfb88054a3f1c3df550d4d6c8261` |
| `V034_ZSTD_SHA256` | v0.3.4 release (712144 bytes), embedded before v0.4.1 | `7d1bb260f954b755ed687733e52ccfa38c31440a9b88a6fbba5b3b75e08bfa26` |
| `V033_ZSTD_SHA256` | v0.3.3 release (701376 bytes) | `1c1051f91bc7cba19b15362b2b832114b53a4c3d6193f50c90fdf1ae5d78d268` |
| `V032_ZSTD_SHA256` | v0.3.2 release (697072 bytes) | `9a8f553a68d4e6e6be11bd9f8d5519b5faa68560761881966abff1dd65bf19f7` |
| `V031_ZSTD_SHA256` | v0.3.1 release (694256 bytes) | `bdd93ce2fbcc2d0986b16d77604cc0770d994ccc124b9051ba9f53aeac39f56f` |
| `V030_ZSTD_SHA256` | v0.3.0 release (694192 bytes) | `5f39ca8ebe169aa15d8a8ba8ec2f9cd4195f509d55daaf4b4edbfa80b145035c` |
| `NG_6972875_SHA256` | local build of commit 6972875 (after v0.2.0, 690736 bytes), embedded before v0.3.0 | `dc7d59d551071dc7ad2002865efe9e54ea0393523d1258ec1ce9fc7389a45bc4` |
| `V020_ZSTD_SHA256` | v0.2.0 release | `7acb2173d9691b5dfb94ea4007660920b7250111c5fd7a86c6eb91b763ad3d76` |
| `PREVIOUS_ZSTD_SHA256` | v0.1.0 release | `69ee3b420c6798fa1b5fb5a9fed75479403bc8a64fe382db301477b1bf705956` |
| `OLD_ZSTD_SHA256` | local build embedded before v0.1.0 (`vendor/ampr_emu/libSceAmpr-zstd.sprx`, 679712 bytes) | `0bf2c3e39580d937c20bc320323f6394ff624fb5839fc1160ef27ceea764a4f7` |

Any other build holding the `AMPRZPK1`, `AMPRPAK4` and `AMPRDAT3` magics counts as ampr-emu-ng too. The
debug build is kept for bring-up only and is never embedded.

## Corresponding source

`ampr-emu-ng-0.4.1-src.tar.gz` is the `v0.4.1` tag's source archive, kept as the corresponding source of the
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
