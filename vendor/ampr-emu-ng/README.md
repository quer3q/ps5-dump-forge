# ampr-emu-ng v0.5.1 (release binary and source)

The unmodified v0.5.1 release of ampr-emu-ng, the AMPR emulation runtime (a Rust hard fork of ampr_emu)
that reads zstd packs (`ampr_zstd.index` + `ampr_zstd-NNN.pak`, format in `docs/zstd.md`) and LZ4 packs
(`AMPRPAK4`), plus the tag's source archive. It serves fully packed games of either format (everything but
executables, `sce_sys`, `sce_module`, `fakelib` and `.ini` files: the "pack everything except system files" strategy).
Forge bundles this one build only; older builds are recognized by their pinned SHA-256 (below), not shipped.

| | |
|---|---|
| Upstream | https://github.com/quer3q/ampr-emu-ng |
| Release | https://github.com/quer3q/ampr-emu-ng/releases/tag/v0.5.1 (tagged 2026-11-23) |
| Tag | `v0.5.1` (commit `e1789c11bf36c2f4e9b67c5452935c1816674a2c`) |
| Local changes | none: the binary is the release asset byte for byte |

| File here | Upstream asset | Bytes | SHA-256 |
|---|---|---:|---|
| `libSceAmpr.sprx` | `libSceAmpr.sprx` (release build) | 706976 | `7ca61afc444f0b44d2f005506dde16cb43e74dae4b1e045323b291b8db1c3d5d` |
| `SHA256SUMS` | `SHA256SUMS` | 170 | `8d987705fa876a3f2ec9d5ebdc5d2d822a42997dad5c5a4e6508778d9ea6f797` |
| `VERSION` | `VERSION` (`v0.5.1`, no newline) | 6 | `422b56331f7827e0f68e37702308220f3b6879a2acfff605a6e00bbdd475d271` |
| `ampr-emu-ng-0.5.1-src.tar.gz` | `git archive --prefix=ampr-emu-ng-0.5.1/ v0.5.1`, gzip | 685475 | `cba0c355f2d66bb902e5f88ae8dccdd2955778f89482037cf347739e69e25f2b` |

Release 0.5.1 has no debug build, so none is vendored: the v0.4.1 `libSceAmpr-debug.sprx` was removed
with the rest of 0.4.1. `SHA256SUMS` still names a `libSceAmpr-debug.sprx` line; check with
`shasum -a 256 -c SHA256SUMS --ignore-missing` in this folder. The release binary carries the strings
`ampr-emu-ng 0.5.1`, `AMPRZPK1`, `AMPRZDT1`, `AMPRPAK4` and `AMPRDAT3`. The source archive's contents
match `git archive v0.5.1` of the upstream repository file for file (the gzip bytes may differ).

Changes since v0.4.1, the build Forge embedded before (`git log v0.4.1..v0.5.1`, 30 commits): the zstd and
LZ4 pack formats are unchanged (`crates/ampr-format` rewrote its in-memory view; the magics, versions and
record layouts are the same; the index ledger stays 80 MiB). Behaviour: mmap of a packed file is no longer
served (the `/data/ampr_cache` disk cache was removed; upstream says no measured game mmaps a packed
file); a smaller PRX (150.89 MiB) with a decoded cache that grows into the index ledger's idle room;
runtime threads on pooled stacks; packed POSIX reads on the caller's thread; save-icon, save-mount and
directory-listing fixes. Dependencies: `zstd-sys` 2.1.3, `lz4_flex` 0.14.2, `crc32fast` 1.5.5,
`iced-x86` 1.22.0.

## Used by the build

`ps5-dump-forge-lz4` embeds `libSceAmpr.sprx` from here: `crates/ps5-dump-forge-lz4/build.rs` stores it
zstd-compressed (`ZSTD_PACKED`, `ZSTD_LEN`) and `runtime::zstd()` decodes it on first use; a test pins its
size, SHA-256 (`ZSTD_SHA256`, the value in `SHA256SUMS`) and version (`NG_VERSION`). An upgrade replaces the
files here (the path in `build.rs` names no version) and changes those pins.
It is the one runtime Forge bundles (drakmor's ampr_emu is no longer shipped, only recognized). Forge
installs it at `fakelib/libSceAmpr.sprx` on every zstd and LZ4 Pack; any conversion of an AMPR title gets it
where it has none and in place of an older ampr-emu-ng, Forge's old trace build and (unless the request
keeps it, `--keep-ampr-emu`) an ampr_emu build; an ampr-emu-ng at this version or newer is kept. A `.pkg`
carries no runtime at all (the console's loader provides it).

Older ampr-emu-ng builds stay pinned in `runtime.rs` so games carrying one are still recognized as
ampr-emu-ng (kept or replaced by the rule above):

| Pin | Build | SHA-256 |
|---|---|---|
| `V041_ZSTD_SHA256` | v0.4.1 release (733104 bytes), embedded before v0.5.1 | `9c7c41b3216bc4f877d67e3c2e5c9c8f9ffabdd5065f4f3fe0dd98fa7228386f` |
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

Any other build holding the `AMPRZPK1`, `AMPRPAK4` and `AMPRDAT3` magics counts as ampr-emu-ng too.

## Corresponding source

`ampr-emu-ng-0.5.1-src.tar.gz` is the `v0.5.1` tag's source archive, kept as the corresponding source of the
binary so nothing depends on upstream staying online (by user decision: vendored, never downloaded at
build or check time). `ps5/build-docker.sh` in it builds `libSceAmpr.sprx` (and a `libSceAmpr-debug.sprx` the release doesn't carry). The exact environment of the
release build is not recorded; this is not a claim that the archive reproduces the binary byte for byte.

## Licenses

- ampr-emu-ng: GPL-3.0-or-later (the archive's crate `Cargo.toml` files and `LICENSE`). Forge is GPL-3.0-or-later too.
- Compiled in, per the archive's `Cargo.toml` and `Cargo.lock`: `zstd-sys` 2.1.3 (libzstd, BSD-3-Clause or GPL-2.0),
  `lz4_flex` 0.14.2 (MIT), `crc32fast` 1.5.5 (MIT or Apache-2.0) with `cfg-if` 1.0.5 (MIT or Apache-2.0), `iced-x86` 1.22.0 (MIT),
  `lazy_static` 1.5.1 (MIT or Apache-2.0), `spin` 0.9.9 (MIT).

## Upgrading

Replace the files with a newer release's assets (one build only, no second version), check them with
`shasum -a 256 -c SHA256SUMS --ignore-missing`, and update this README (version, tag, dates, sizes, hashes,
URLs) and the pins in `runtime.rs` (`ZSTD_SHA256`, `NG_VERSION` and the size in `pinned_sizes_and_hashes`)
together. Keep the replaced build's hash pinned with the others above (a new `V<NNN>_ZSTD_SHA256` in
`classify`) so games carrying it are still recognized.
