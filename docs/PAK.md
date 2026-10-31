# zstd packs (ampr_zstd.index + ampr_zstd-NNN.pak)

The format of `crates/ps5-dump-forge-zstd`: a general zstd container for a game's
files, not AMPRPAK4 (the LZ4 packs, `docs/LZ4.md`). The crate encodes and
validates both headers, writes packs (`writer`), measures them without writing
(`writer::measure`, served by `tree::PackedTree`) and reads them back
(`reader`, the packs served as a `SourceTree`). Built without its default `std`
feature it is `no_std` and keeps only the convention (magics, layout, names,
`Chunk`, the volume header): the console's runtime, ampr-emu-ng
(`vendor/ampr-emu-ng/libSceAmpr.sprx`), reads the packs with that part and
decodes the chunks with its own code.

All integers little-endian. CRC = CRC-32 (IEEE, as `crc32fast`).
Path hash = FNV-1a 64 (basis 14695981039346656037, prime 1099511628211) over
the UTF-8 path relative to the game root, `/` separators, no leading slash,
case kept (e.g. `data/level1.bin`). A path is non-empty, has no `\` or NUL,
and no empty, `.` or `..` part; at most 65,535 bytes. No file path is a
folder of another (`a` beside `a/b`).

## Files on disk

- `ampr_zstd.index`: at the game root. The whole archive's index.
- `ampr_zstd-000.pak`, `ampr_zstd-001.pak`, ...: volumes, three digits at least, from 000
  (four past 999), as the LZ4 packs' `ampr_assets-NNN.pak`. Volume id 0 is
  `ampr_zstd-000.pak`; at most 4,096 volumes (12-bit id).
  Each volume file is at most 4 GiB less 64 KiB (4,294,901,760 bytes,
  `VOLUME_LIMIT`, as the LZ4 packs), header included: every chunk offset fits
  the u32 chunk offset, and a volume stays under FAT32's 4 GiB file limit.

## ampr_zstd.index

Header, 128 bytes:

| off | type | field |
|---|---|---|
| 0 | [8] | magic `AMPRZPK1` |
| 8 | u32 | version = 1 |
| 12 | u32 | header size = 128 |
| 16 | u8 | chunk_shift = 18 (256 KiB); any other value is refused |
| 17 | u8 | zstd level used, 1..=22 (info only) |
| 18 | u16 | reserved 0 |
| 20 | u32 | volume_count (at most 4,096) |
| 24 | [16] | archive_id (random; repeated in every volume header) |
| 40 | u64 | file_count |
| 48 | u64 | chunk_count (at most 2^32 - 1) |
| 56 | u64 | files_off (= 128) |
| 64 | u64 | chunks_off |
| 72 | u64 | volumes_off |
| 80 | u64 | strings_off |
| 88 | u64 | strings_len (the index file ends at strings_off + strings_len) |
| 96 | u64 | total logical bytes of the packed files |
| 104 | u64 | reserved 0 |
| 112 | u32 | CRC of bytes [128, end) |
| 116 | u32 | CRC of the header with bytes 112..120 zeroed |
| 120 | u64 | reserved 0 |

The tables follow back to back in this order: file records, chunk records,
volume records, strings. Each offset is checked against the counts.

File record, 32 bytes, sorted by (path_hash, path bytes) for binary search,
no duplicates:

| off | type | field |
|---|---|---|
| 0 | u64 | path_hash |
| 8 | u64 | logical_size |
| 16 | u32 | first_chunk (the previous file's first_chunk + its chunk count; chunk count = ceil(size / 256 KiB), 0 for an empty file) |
| 20 | u32 | path_off (into strings) |
| 24 | u16 | path_len (bytes, no NUL) |
| 26 | u16 | flags: bit0 = store-only (every chunk raw); other bits 0 |
| 28 | u32 | reserved 0 |

Chunk record, 8 bytes (u64), one per 256 KiB of a file, flat array:

| bits | field |
|---|---|
| 0..31 | byte offset inside the volume (absolute, from file start; at least 64) |
| 32..43 | volume id (12 bits) |
| 44..61 | stored_size - 1 (18 bits, so stored_size <= 256 KiB) |
| 62..63 | codec: 0 = raw, 1 = zstd; 2 and 3 are invalid |

Volume record, 8 bytes: u64 file size of that volume, header included.

Strings: path bytes, back to back in file-record order, nothing unused.

## Volume file

Header, 64 bytes:

| off | type | field |
|---|---|---|
| 0 | [8] | magic `AMPRZDT1` |
| 8 | u32 | version = 1 |
| 12 | u32 | header size = 64 |
| 16 | u32 | volume id (< volume_count) |
| 20 | u32 | volume_count (1..=4,096) |
| 24 | [16] | archive_id |
| 40 | u64 | payload bytes (volume size - 64) |
| 48 | [12] | reserved 0 |
| 60 | u32 | CRC of the header with bytes 60..64 zeroed |

Chunks follow from offset 64, back to back, no padding, in index order
(files in file-record order, chunks in file order). A file may continue in
the next volume. Each volume record equals the offset where its last chunk
ends; every volume holds at least one chunk.

## Chunks

Chunk `i` of a file decodes to `min(262144, size - i * 262144)` bytes. A raw
chunk's stored_size equals that; a zstd chunk's is strictly smaller. A
store-only file has raw chunks only. A zstd chunk is exactly one zstd frame:
content size in the frame header, frame checksum on (the integrity check),
no dictionary, window at most 256 KiB (`window_log` 18). Every chunk decodes
on its own (own frame, absolute offset), so contiguous chunks within one
volume are one byte range and decode in parallel (a file's chunks may continue
over any number of volumes).

## The writer

`writer::pack_with(source, files, level, threads, output, progress, cancel)`
packs the `files` the caller chose from a `SourceTree` (the caller writes the
loose files and owns creating, syncing and cleaning up outputs through
`PackOutput`). Per file, in (path_hash, path) order:

1. Compress every 256 KiB chunk with zstd at `level` (1..=22) and count the
   stored bytes (a chunk whose frame is not smaller than raw counts raw).
2. If the file saves less than 1% (`stored * 100 > size * 99`), it is
   store-only.
3. Append each chunk to the open volume (raw for a store-only file or a chunk
   zstd doesn't shrink); a chunk that would pass `VOLUME_LIMIT` starts the
   next volume.

Steps 1 and 3 are one pass for most files: pass 1 keeps the encoded chunks (up
to 32 MiB), and once the bytes stored so far plus the raw bytes left can't
exceed 99% of the size, the file can't be store-only, so the kept chunks are
written and the pass goes on writing. A store-only file, or one whose kept
chunks outgrew the 32 MiB before that point, is read again (and compressed
again unless store-only). The result is the same either way.

Chunks go in batches of one per compression thread (`threads`, 0 = every
core, at most 32); this thread reads the next batch while the pool compresses
one, and each thread keeps its zstd context from chunk to chunk. The output
does not depend on the thread count. Progress counts each file twice (a pass
it doesn't need counts as done when skipped), so it ends at twice the packed
bytes. Volume headers are written last (`reopen`, the first 64 bytes only),
once the count is known, then `ampr_zstd.index`. A file listed at another size
than the source's, or that reads short while packing, fails the pack;
`cancel` stops it between batches.

## Measure and PackedTree

`writer::measure(source, files, level, threads, progress, cancel)` runs the
same compression as `pack_with` without writing: every chunk's codec and
stored size go into the returned `Index` (a fresh archive id), no byte is
kept, and a store-only file's second pass reads nothing. Memory is the index
(16 B per chunk) plus the compression's own.

`tree::PackedTree` serves the output `pack_with` would write as a
`SourceTree`: the source's loose files, `ampr_zstd.index` and the volumes at
their measured sizes. A volume's bytes are made on demand: its header from
the index, each chunk re-read from its source file, compressed again with the
one encoder and checked against the index (codec and stored length; a zstd
frame is decoded and must give back the source bytes). An image writer takes
the packs straight from the source this way, with no staging folder; the
packed files are read twice.
