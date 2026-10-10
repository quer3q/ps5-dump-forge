# Third-party notices

PS5 Dump Forge ("Forge", the `ps5-dump-forge` CLI and the source in this repository) is
distributed under the GNU General Public License, version 3 (see `LICENSE`). Parts of it come
from, are ported from, or follow the projects below. Their notices are kept here, as their
licenses require.

The complete corresponding source of every release is attached to that release as
`ps5-dump-forge-<version>-source.tar.gz` (including the vendored crates and the build scripts);
`Cargo.lock` and `app/package-lock.json` pin every dependency.

## ps5upload (GPL-3.0-or-later)

`vendor/ps5upload-fpkg` (FPKG builder; exFAT, UFS2, PFSC, FIH/CNT and Kraken readers) and
`vendor/ps5upload-pkg` (UFS2 reader, package inspection) are taken from ps5upload with a small
local patch series (`vendor/patches/`, described in `vendor/README.md`).

- Upstream: https://github.com/phantomptr/ps5upload, tag `v6.1.2` (`4eaa6af`)
- License: GNU General Public License, version 3 or (at your option) any later version.
  The GPL-3.0 text is in `LICENSE`.

### Embedded debug FPKG key material

`vendor/ps5upload-fpkg/src/keys.rs` builds in the key material that debug (fake) packages
need, taken from LibProsperoPKG (`Keys/Data/{passcode,mount_image}.bin` and related values),
as ps5upload does. It is not all public: besides public RSA moduli and format constants, it
contains the debug RIF symmetric key (`RIF_DEBUG_KEY`) and the debug RIF RSA private exponent
(`DEBUG_RIF_PRIVATE_EXPONENT`), which `license.rs` uses to sign `license.dat`. These are the
well-known *debug* keys every fake-package tool carries, not retail console keys; they let the
app build packages that a console running the fpkg-enable / ppr-patch payloads installs.
Embedding them is this project's own distribution decision; all key use stays
behind `keys.rs`.

## MkPFS (GPL-3.0)

The exFAT image writer (`crates/ps5-dump-forge-exfat`, including its up-case table) is ported
from MkPFS's `exfat_writer.py` and `_exfat_upcase.py`.

The PFS crate (`crates/ps5-dump-forge-pfs`) writes and reads the PFS image layout (header,
inode table, flat path table and its collision resolver, directory records) and the PFSC
container as MkPFS 1.1.0 (`mkpfs/pfs.py`) does: MkPFS was read for the format and its layout
reimplemented in Rust, so that `.ffpfs` and `.ffpfsc` images match what `mkpfs pack` makes.

Code ported from MkPFS is under the GNU General Public License, version 3 (text in `LICENSE`).

- Upstream: https://github.com/PSBrew/MkPFS (version 1.1.0 for the PFS and PFSC layout)

## PS5 UltraPack (MIT)

No PS5 UltraPack code is included. Two of its documented console findings are followed: PFS
images use 64 KiB blocks (its README reports a 4 KiB build of the same game crashing on launch
on firmware 11.60, where 64 KiB boots), and image names stay short enough to mount (its
`check_ffpfsc_name_lengths.sh` notes ShadowMountPlus failing longer names with ENAMETOOLONG; the exact
per-format limits come from ShadowMountPlus's mount point, see README.md, "Formats").

- Upstream: https://github.com/knutwurst/ps5-ultrapack
- License: MIT, Copyright (c) 2024–2026 Knutwurst

## UFS2Tool (BSD-2-Clause)

The UFS2 image writer (`crates/ps5-dump-forge-ufs2`) follows UFS2Tool's image sizing (blocks
needed plus 10%) and was checked against its failure modes. No UFS2Tool code is included.

- Upstream: https://github.com/SvenGDK/UFS2Tool

```
BSD 2-Clause License

Copyright (c) 2026, SvenGDK
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

## FreeBSD (BSD-3-Clause)

`crates/ps5-dump-forge-ufs2` reproduces the cylinder-group geometry arithmetic of FreeBSD
`newfs` (`sbin/newfs/mkfs.c`, in `src/geom.rs`) and writes the on-disk structures laid out
by `sys/ufs/ffs/fs.h` and `sys/ufs/ufs/dinode.h` (`src/ondisk.rs`). FreeBSD `makefs`
(`usr.sbin/makefs`) was the model for the one-pass layout.

- Upstream: https://github.com/freebsd/freebsd-src

`sbin/newfs/mkfs.c`:

```
Copyright (c) 2002 Networks Associates Technology, Inc.
All rights reserved.

This software was developed for the FreeBSD Project by Marshall
Kirk McKusick and Network Associates Laboratories, the Security
Research Division of Network Associates, Inc. under DARPA/SPAWAR
contract N66001-01-C-8035 ("CBOSS"), as part of the DARPA CHATS
research program.

Copyright (c) 1980, 1989, 1993
	The Regents of the University of California.  All rights reserved.
```

`sys/ufs/ffs/fs.h`, `sys/ufs/ufs/dinode.h`:

```
Copyright (c) 1982, 1986, 1993
	The Regents of the University of California.  All rights reserved.

Copyright (c) 2002 Networks Associates Technology, Inc.
All rights reserved.
```

These files are distributed under the following terms (`dinode.h` is BSD-2-Clause AND
BSD-3-Clause; clause 3 applies to the University's parts):

```
Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions
are met:
1. Redistributions of source code must retain the above copyright
   notice, this list of conditions and the following disclaimer.
2. Redistributions in binary form must reproduce the above copyright
   notice, this list of conditions and the following disclaimer in the
   documentation and/or other materials provided with the distribution.
3. Neither the name of the University nor the names of its contributors
   may be used to endorse or promote products derived from this software
   without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
SUCH DAMAGE.
```

## ampr_emu (GPL-3.0-or-later)

`vendor/ampr_emu/` holds the unmodified upstream 0.4.2.1 binaries `libSceAmpr.sprx` (release) and
`libSceAmpr-trace.sprx` (trace), which Forge embeds and installs into LZ4-packed or traced games, and the
tagged source archive `ampr_emu-0.4.2.1-src.tar.gz`, kept there as their corresponding source (the exact
compile-time options of the release binaries are not recorded; this is no claim of reproducibility).
Forge's pack format code is written from the documented format and checked against upstream's Python
tools (`scripts/check-lz4.sh`); no ampr_emu code is compiled into Forge.

- Upstream: https://github.com/drakmor/ampr_emu, tag `0.4.2.1`, by drakmor
- License: GNU General Public License, version 3 or (at your option) any later version.

The runtimes contain these third-party parts (their sources are in the archive):

- LZ4 1.10.0 (`third_party/lz4`), BSD 2-Clause. The license text, from the header of its `lz4.c`/`lz4.h`:

```
LZ4 - Fast LZ compression algorithm
Copyright (C) 2011-2023, Yann Collet.

BSD 2-Clause License (http://www.opensource.org/licenses/bsd-license.php)

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

    * Redistributions of source code must retain the above copyright
notice, this list of conditions and the following disclaimer.
    * Redistributions in binary form must reproduce the above
copyright notice, this list of conditions and the following disclaimer
in the documentation and/or other materials provided with the
distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

You can contact the author at :
 - LZ4 homepage : http://www.lz4.org
 - LZ4 source repository : https://github.com/lz4/lz4
```

- HDE64 (Hacker Disassembler Engine 64 C), Copyright (c) 2008-2009 Vyacheslav Patkov, all rights reserved.
  The archive's sources carry only the copyright line; the BSD terms are those published with MinHook
  (https://github.com/TsudaKageyu/minhook/blob/master/LICENSE.txt), reproduced here:

```
Hacker Disassembler Engine 64 C
Copyright (c) 2008-2009, Vyacheslav Patkov.
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions
are met:

 1. Redistributions of source code must retain the above copyright
    notice, this list of conditions and the following disclaimer.
 2. Redistributions in binary form must reproduce the above copyright
    notice, this list of conditions and the following disclaimer in the
    documentation and/or other materials provided with the distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE REGENTS OR
CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL,
EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR
PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

[Lazy_AMPR](https://github.com/Nazky/Lazy_AMPR) is the reference workflow (trace, then pack). No code or
profile of it is included.

## lz4_flex (MIT)

`crates/ps5-dump-forge-lz4` compresses and decompresses LZ4 blocks with
[lz4_flex](https://github.com/PSeitz/lz4_flex) 0.14.

```
The MIT License (MIT)

Copyright (c) 2020 Pascal Seitz

Permission is hereby granted, free of charge, to any person obtaining a copy of
this software and associated documentation files (the "Software"), to deal in
the Software without restriction, including without limitation the rights to
use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software is furnished to do so,
subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
```

## Tauri and other dependencies

The app is built with [Tauri](https://tauri.app) 2 (`tauri`, `tauri-build`,
`tauri-plugin-dialog`, `@tauri-apps/api`, `@tauri-apps/plugin-dialog`: MIT or Apache-2.0)
and [React](https://react.dev) (MIT). The remaining Rust crates (`Cargo.lock`) and npm
packages (`app/package-lock.json`) are under their own permissive licenses (MIT, Apache-2.0,
BSD, Zlib, Unicode and similar), as stated in each package's metadata.
