# ampr_emu 0.4.2.1 (runtime binaries and source)

Unmodified upstream binaries of `libSceAmpr.sprx`, the AMPR emulation runtime that reads LZ4 asset
packs (and, in the trace build, records which files a game reads). `ps5-dump-forge-lz4` embeds both
`.sprx` files with `include_bytes!` and recognises a runtime found in a game folder by SHA-256.

| | |
|---|---|
| Upstream | https://github.com/drakmor/ampr_emu |
| Release / tag | `0.4.2.1` (https://github.com/drakmor/ampr_emu/releases/tag/0.4.2.1) |
| Downloaded | 2026-10-09 |
| Local changes | none: the binaries are the release assets byte for byte |

| File here | Upstream asset | Bytes | SHA-256 |
|---|---|---:|---|
| `libSceAmpr.sprx` | `libSceAmpr.sprx` (identical to `libSceAmpr.sprx-0.4.2.1-test-pack`) | 423350 | `69e6c4d5e4f5fb83c9e01815db5861c4c75734acbf4595cafa50d4c218116d1a` |
| `libSceAmpr-trace.sprx` | `libSceAmpr.sprx-0.4.2.1-test-debug-pack` | 633094 | `b44a986f2fa9903e74a34cbbd4681e899cd99196613652cd073ed11f2ca947c2` |
| `ampr_emu-0.4.2.1-src.tar.gz` | `archive/refs/tags/0.4.2.1.tar.gz` | 616475 | `81ea07fee4ad62e775a79fabe3516a0dda62cc940623a5ebe4befea4e345b62a` |

The release runtime is the `test-pack` build, not a separate stable variant. The `test-nopack` build
(`libSceAmpr.sprx-0.4.2.1-test-nopack`, 303878 bytes) is deliberately not embedded.

Download URLs:

```text
https://github.com/drakmor/ampr_emu/releases/download/0.4.2.1/libSceAmpr.sprx
https://github.com/drakmor/ampr_emu/releases/download/0.4.2.1/libSceAmpr.sprx-0.4.2.1-test-pack
https://github.com/drakmor/ampr_emu/releases/download/0.4.2.1/libSceAmpr.sprx-0.4.2.1-test-debug-pack
https://github.com/drakmor/ampr_emu/archive/refs/tags/0.4.2.1.tar.gz
```

## Corresponding source

`ampr_emu-0.4.2.1-src.tar.gz` is the tagged source archive and is kept here as the corresponding
source of the binaries, so nothing depends on upstream staying online. The exact compile-time overrides
and build recipe used for the historical release binaries are not known from the source defaults alone;
this is not a claim that the binaries can be reproduced from the archive.

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
trace and pack job.

## Pinning and upgrading

`crates/ps5-dump-forge-lz4/src/runtime.rs` pins the sizes and SHA-256 of both `.sprx` files and checks
that both contain `AMPRPAK4` and `AMPRDAT3`. To upgrade, replace both binaries, the source archive,
this README (version, URLs, date, sizes, hashes) and the pin tests together.
