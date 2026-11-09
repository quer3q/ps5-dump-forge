# ampr-emu-ng v0.3.4 (release binaries and source)

The unmodified v0.3.4 release of ampr-emu-ng, the AMPR emulation runtime (a Rust hard fork of ampr_emu)
that reads zstd packs (`ampr_zstd.index` + `ampr_zstd-NNN.pak`, format in `docs/zstd.md`) and LZ4 packs
(`AMPRPAK4`), plus the tag's source archive. It serves fully packed games of either format (everything but
executables, `sce_sys`, `sce_module`, `fakelib` and `.ini` files: the "pack everything except system files" strategy).

| | |
|---|---|
| Upstream | https://github.com/quer3q/ampr-emu-ng |
| Release | https://github.com/quer3q/ampr-emu-ng/releases/tag/v0.3.4 |
| Tag | `v0.3.4` (commit `cbdafd8d87f6e904524142dca210f1d1334bf505`) |
| Local changes | none: the binaries are the release assets byte for byte |

| File here | Upstream asset | Bytes | SHA-256 |
|---|---|---:|---|
| `libSceAmpr.sprx` | `libSceAmpr.sprx` (release build) | 712144 | `7d1bb260f954b755ed687733e52ccfa38c31440a9b88a6fbba5b3b75e08bfa26` |
| `libSceAmpr-debug.sprx` | `libSceAmpr-debug.sprx` (debug build) | 819776 | `717299ddd8122716ba11c89e03cc56d05207d0cae2ef898da8a9a65536640997` |
| `SHA256SUMS` | `SHA256SUMS` | 170 | `a8dd28c5e3ae40573353e260477ff5c4d8cb529d7f9a286500f698e256d709af` |
| `ampr-emu-ng-0.3.4-src.tar.gz` | `git archive --prefix=ampr-emu-ng-0.3.4/ v0.3.4`, gzip -n -9 | 510250 | `b9ecf547b3daaff5de905d0d98514730b8d78483875faec1de26d70b9c7466fa` |

The three release files match the digests GitHub lists for the assets. Check: `shasum -a 256 -c SHA256SUMS`
in this folder. The release binary carries the strings `ampr-emu-ng 0.3.4`, `AMPRZPK1`, `AMPRPAK4` and
`AMPRDAT3`.

Changes since v0.3.2, the build Forge v0.5.4 shipped (release notes and the tag diffs). v0.3.3: a packed
file's fd is a synthetic number in `512..1024`, not a reserved kernel descriptor, so a title keeps its
whole fd table; the game's own kernel AIO (`sceKernelAioSubmitReadCommands*` and its poll, wait, delete
and cancel) on a packed file is served; fully installed games no longer ask to continue the download;
Marvel's Wolverine boots again, Ghost of Yotei's background reads through the system reach packed files,
and Hollow Knight Silksong no longer stops at launch on a folder install on the internal SSD. v0.3.4: the
runtime sizes its worker threads to the game
(it starts small, grows while the game reads and gives threads back when the game needs them), so a game
no longer goes black at launch or freezes when it can't start a thread of its own (Crimson Desert); the
pack caches are twice as large; Crimson Desert's saves load and save from a plain folder and from a zstd
pack. The zstd and LZ4 pack formats are unchanged (`crates/ampr-format` and `crates/ampr-zstd` only bump
their versions), and so are the third-party dependencies (`Cargo.lock` only bumps the workspace crates).

## Used by the build

`ps5-dump-forge-lz4` embeds `libSceAmpr.sprx` from here: `crates/ps5-dump-forge-lz4/build.rs` stores it
zstd-compressed (`ZSTD_PACKED`, `ZSTD_LEN`) and `runtime::zstd()` decodes it on first use; a test pins its
SHA-256 (`ZSTD_SHA256`, the value in `SHA256SUMS`). An upgrade replaces the files here (the path in `build.rs` names no version) and changes that hash.
Forge installs it at `fakelib/libSceAmpr.sprx` on every zstd Pack and on an LZ4 Pack with runtime `ng` (the
default), and keeps it there and in a zstd-packed game converted as it is. Every other conversion (a plain
convert, an LZ4 Unpack, an LZ4 Pack with `ampr_emu`, a zstd Unpack) replaces it with the upstream release
runtime (`vendor/ampr_emu/`).

Older ampr-emu-ng builds stay pinned in `runtime.rs` so games carrying one are still recognized as
ampr-emu-ng (kept or replaced by the rule above):

| Pin | Build | SHA-256 |
|---|---|---|
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

`ampr-emu-ng-0.3.4-src.tar.gz` is the `v0.3.4` tag's source archive, kept as the corresponding source of the
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
