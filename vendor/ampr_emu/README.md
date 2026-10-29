# ampr_emu 0.4.2.1 (runtime binaries and source)

One unmodified upstream binary of `libSceAmpr.sprx`, the AMPR emulation runtime that reads LZ4
asset packs, plus `libSceAmpr-zstd.sprx` (ampr-emu-ng, below), a local build that reads zstd and LZ4
packs. `ps5-dump-forge-lz4`
embeds both `.sprx` files with `include_bytes!` and recognises a runtime found in a game folder by
SHA-256.

| | |
|---|---|
| Upstream | https://github.com/drakmor/ampr_emu |
| Release / tag | `0.4.2.1` (https://github.com/drakmor/ampr_emu/releases/tag/0.4.2.1) |
| Downloaded | 2026-10-09 |
| Local changes | none: `libSceAmpr.sprx` is the release asset byte for byte |

| File here | Upstream asset | Bytes | SHA-256 |
|---|---|---:|---|
| `libSceAmpr.sprx` | `libSceAmpr.sprx` (identical to `libSceAmpr.sprx-0.4.2.1-test-pack`) | 423350 | `69e6c4d5e4f5fb83c9e01815db5861c4c75734acbf4595cafa50d4c218116d1a` |
| `ampr_emu-0.4.2.1-src.tar.gz` | `archive/refs/tags/0.4.2.1.tar.gz` | 616475 | `81ea07fee4ad62e775a79fabe3516a0dda62cc940623a5ebe4befea4e345b62a` |

The release runtime is the `test-pack` build, not a separate stable variant. The `test-nopack` build
(`libSceAmpr.sprx-0.4.2.1-test-nopack`, 303878 bytes) is deliberately not embedded, nor is the trace
build (`libSceAmpr.sprx-0.4.2.1-test-debug-pack`, 633094 bytes, SHA-256
`b44a986f2fa9903e74a34cbbd4681e899cd99196613652cd073ed11f2ca947c2`): Forge has no traces. Its hash
stays pinned so a game carrying it gets the release runtime on any conversion.

Download URLs:

```text
https://github.com/drakmor/ampr_emu/releases/download/0.4.2.1/libSceAmpr.sprx
https://github.com/drakmor/ampr_emu/releases/download/0.4.2.1/libSceAmpr.sprx-0.4.2.1-test-pack
https://github.com/drakmor/ampr_emu/archive/refs/tags/0.4.2.1.tar.gz
```

## Corresponding source

`ampr_emu-0.4.2.1-src.tar.gz` is the tagged source archive and is kept here as the corresponding
source of the binaries, so nothing depends on upstream staying online. The exact compile-time overrides
and build recipe used for the historical release binaries are not known from the source defaults alone;
this is not a claim that the binaries can be reproduced from the archive.

## ampr-emu-ng

`libSceAmpr-zstd.sprx` is ampr-emu-ng, built locally from the `ampr-emu zstd` project (a Rust rewrite
of ampr_emu, not upstream; GPL-3.0-or-later). It reads zstd packs (`ampr_zstd.index` + `ampr_zstd-NNN.pak`,
format in `docs/PAK.md`) and LZ4 packs (`AMPRPAK4`), picked at load. It takes the zstd pack convention
from `crates/ps5-dump-forge-zstd` built `no_std` (this repo mounted read-only). In that project
`./ps5/build-docker.sh` builds `libSceAmpr.sprx` (and `libSceAmpr-debug.sprx`) at its root; copy
`libSceAmpr.sprx` over this `libSceAmpr-zstd.sprx` for a fresh build. Current file: 679712 bytes,
SHA-256 `0bf2c3e39580d937c20bc320323f6394ff624fb5839fc1160ef27ceea764a4f7`.

Forge installs it at `fakelib/libSceAmpr.sprx` on every zstd Pack and on an LZ4 Pack with runtime
`ng` (the default), and keeps it wherever it is found, LZ4 packs included; LZ4 Pack with `ampr_emu`
and a zstd Unpack replace it with the release runtime (`runtime.rs`, `core/src/lz4.rs`). Its SHA-256 is
computed at run time, not pinned, because fresh builds replace it. It uses a native libzstd decoder per
worker and splits multi-chunk reads across workers; it does not verify zstd frame checksums.

## Licenses

- ampr_emu: GPL-3.0 (`LICENSE` in the archive; the sources carry `SPDX-License-Identifier:
  GPL-3.0-or-later`). Forge is GPL-3.0-or-later too.
- LZ4 (`third_party/lz4`, v1.10.0, compiled into the runtime): the `lib` files are BSD 2-Clause
  (`third_party/lz4/LICENSE` in the archive).
- HDE64 (`src/hde64.cpp`, `include/hde64.h`): Hacker Disassembler Engine 64 C, Copyright (c) 2008-2009
  Vyacheslav Patkov, all rights reserved, with BSD terms as published through MinHook:
  https://github.com/TsudaKageyu/minhook/blob/master/LICENSE.txt

Credit: ampr_emu 0.4.2.1 by drakmor (GPL-3.0). Workflow reference (no code taken): https://github.com/Nazky/Lazy_AMPR

## Caveat

0.4.2.1 is an upstream test build; known issue: some games crash when saving. Forge logs this on every
LZ4 Pack with runtime `ampr_emu`.

## Pinning and upgrading

`crates/ps5-dump-forge-lz4/src/runtime.rs` pins the size and SHA-256 of the upstream `.sprx` (and
the old trace build's SHA-256) and checks that it contains `AMPRPAK4` and `AMPRDAT3`. To upgrade, replace
the binary, the source archive, this README (version, URLs, date, sizes, hashes) and the pin tests
together.
