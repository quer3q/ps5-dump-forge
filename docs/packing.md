# Custom pack rules (TOML)

Without a profile Forge uses its built-in packing strategy: root files and the keep-loose list
(container indexes, configs, media, protected and video folders, Insomniac's containers) stay
loose, everything else is packed ([LZ4.md](LZ4.md#packing-strategy) has the whole list). A TOML
profile replaces that strategy with your own rules, for zstd and LZ4 Pack alike. The format is
upstream ampr_emu's `ampr_pack.py` profile, as the Lazy_AMPR community profiles write it:
<https://github.com/Nazky/Lazy_AMPR/tree/main/toml_profiles>. A commented sample for a generic
AMPR game: [packing-sample.toml](packing-sample.toml).

**Wrong rules can leave a game that won't start.** Anything the game reads other than through
AMPR (plain reads, mmap), or before AMPR starts, must stay loose. Keep a copy of the game.

## How to use it

- Pack tab: open the **Custom rules (TOML)** fold under the settings and **Pick** a `.toml` file
  on this device, a paired device or a saved NAS. The fold stays closed until a file is picked;
  then its title and a path line name the file. **Clear** goes back to the built-in strategy. The
  choice is kept in memory until the app closes and is sent only with a Pack, never with an
  Unpack. The job's tag reads `... · custom rules`.
- CLI: `--pack-profile <toml>` with `--zstd-pack`, `--lz4-pack` or `--to lz4`, for example
  `ps5-dump-forge convert <game_dir> --to folder --zstd-pack --pack-profile docs/packing-sample.toml`.
  With any other job (an unpack, a plain conversion, an edit, ...) it is refused.

The file is read once, in preflight: a regular file (not a link) of at most 1 MiB, UTF-8. Every
problem in it is a preflight finding (`pack rules <file>: ...`) and nothing is written.

## Supported keys

`[pack]`:

| Key | Default | Allowed |
| --- | --- | --- |
| `default_action` | `loose` | `compress`, `store`, `loose`: for files no rule matches |
| `default_block_size` | 64 KiB | a size, power of two, 16 KiB to 1 MiB |
| `auto_loose_large_files` | `true` | boolean: sample large files (LZ4 only) |
| `auto_loose_hot_files` | `false` | boolean: sample hot files too |
| `auto_loose_min_file_size` | 64 MiB | a size: smallest file sampled |
| `auto_loose_sample_blocks` | 32 | integer, 1 to 4096 |
| `auto_loose_sample_bytes` | 16 MiB | a size, at least 1 byte |
| `auto_loose_min_savings_ratio` | 0.05 | 0 up to (not including) 1 |
| `auto_loose_max_raw_ratio` | 0.90 | 0 to 1 |
| `index_name` | | only `"ampr_assets.index"` |
| `io_page_size` | | only 64 KiB |

`[[rule]]` (one table per rule):

| Key | Default | Allowed |
| --- | --- | --- |
| `action` | `compress` | `compress`, `store`, `loose` |
| `include` | `"**"` | a glob or a list of globs |
| `exclude` | none | a glob or a list of globs |
| `block_size` | `default_block_size` | a size, power of two, 16 KiB to 1 MiB |
| `layout` | `auto` | `auto`, `random`, `mixed`, `streaming` (used as `mixed`) |
| `hot` | `false` | boolean |

A size is an integer (bytes) or a string with a unit: `k`/`kb`, `kib`, `m`/`mb`, `mib`,
`g`/`gb`, `gib`, `t`/`tb`, `tib` (any case, `_` allowed), so `"64KiB"` and `65536` are the same.
Actions and layouts take any case.

Ignored, not refused: every other table (`[groups]`, `[runtime]`, ...) and every other key in
`[pack]` or `[[rule]]` (upstream tuning such as `workers`, `rule.group`, typos). The job logs
them once, `pack rules: ignored keys: groups.*, pack.workers, rule.group`, so community profiles
load as they are.

Refused (a finding): bad TOML, a bad value for a key above, `rule` that isn't `[[rule]]`, a rule
with an empty `include` list, and `include_from`/`exclude_from`. The profile may sit on another
device, so files beside it are not read: paste the list into the rule as `include = [...]` (or
`exclude = [...]`).

## Matching

- Paths are relative to the game root with `/` separators (a `\` or a leading `/` in a pattern
  is normalized the same way).
- Globs are Python's `fnmatchcase`: case-sensitive, `*` matches any run of characters and crosses
  `/` (so `**` is the same as `*`), `?` one character, `[abc]`/`[!abc]` a set.
- A rule matches a file when an `include` glob matches and no `exclude` glob does. The last
  matching rule in the file wins; `default_action` covers files no rule matches.
- A file is random access when `layout = "random"`, or `layout = "auto"` with `hot = true`.

## Priority

A profile overrides every built-in rule: the keep-loose list, the protected and video folders
and the root-file rule are all its call. Only these always stay loose, whatever it says:

- empty files;
- `eboot.bin` (any folder, any case) and any `.elf`, `.self`, `.prx` or `.sprx`;
- anything inside a root `sce_sys/` or `sce_module/` folder or a root folder whose name starts
  with `fakelib` (any case);
- the AMPR files: `ampr_assets.index`, `ampr_assets.index.crc`, `ampr_assets.index.runtime`,
  `ampr_emu.index`, `fakelib/libSceAmpr.sprx`, `ampr_commands.bin`, `ampr_emu.log`, `apr_emu.log`;
- for zstd also the pack files themselves (`ampr_zstd.index`, `ampr_zstd-NNN.pak`).

An LZ4-packed source is unpacked first and then packed by the profile (or the built-in
strategy); the files its old packs held are not reused.

## zstd and LZ4

- LZ4: `block_size` sets each file's chunk size; `store` keeps its chunks raw; `hot` and
  `layout` set the file's hot and random flags in the manifest; auto-loose samples large
  compressed files with the profile's `auto_loose_*` values.
- zstd: chunks are always 256 KiB, so `block_size`, `hot` and `layout` have no effect; a `store`
  file is packed as any other (a chunk zstd can't shrink is kept raw anyway); there is no
  auto-loose.

## The job log

A zstd Pack with a profile logs, for example:

    pack rules: <file>: 7 rules; 4810 files packed, 312 loose; zstd level 5, 256 KiB chunks
    packing strategy: kept loose 312 files, 3.1 GB: the profile 280 files, 3.0 GB, system and runtime files (always_loose) 32 files, 100.0 MB

LZ4 names its level instead (`LZ4 level 0`) and adds `, auto-loose <size>` to the second line.
Without a profile the first line reads `packing strategy: built-in; ...` and the second counts
the built-in reasons (root files, keep-loose suffixes with the top 5, protected folders and
names): a large one there is where a profile could pack more.
