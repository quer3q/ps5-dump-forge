#!/usr/bin/env bash
# Dev-only cross-check of the LZ4 asset packs against upstream's own Python tools (ampr_emu
# 0.4.2.1, tools/ampr_pack.py, extracted from the committed vendor/ampr_emu source archive into a
# temp dir). Forge's writer and reader can share a bug; the upstream parser is an unrelated one.
#
#   scripts/check-lz4.sh <forge_lz4_output_dir> <original_source_dir>
#       Forge -> upstream: ampr_pack.py inspect, verify, and verify --root <original> on the
#       output of `convert <original> --to lz4`. The pak volumes, ampr_assets.index and its
#       .crc sidecar must sit together in <forge_lz4_output_dir>.
#   scripts/check-lz4.sh --reverse
#       Upstream -> Forge: builds a tiny game folder, packs it with ampr_pack.py (index from
#       build_ampr_index.py), runs `ps5-dump-forge convert <ref> --to folder --lz4-unpack`, and
#       diffs the logical files byte for byte.
#   scripts/check-lz4.sh --make-fixture <dir>
#       Writes the tiny game folder (eboot.bin importing libSceAmpr, sce_sys/param.json, assets).
#
# Passing the Python tools alone is NOT enough. They omit the C++ runtime's non-streaming check
# that a RAW chunk is exactly one page, they do not prove that the AMPRIDX3 ids match the
# manifest's ids, and they know nothing of the runtime's source-default limits. This script is a
# second parser, not hardware validation. Needs Docker (python:3 image, `pip install lz4`);
# --reverse also needs cargo.
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
archive=$repo/vendor/ampr_emu/ampr_emu-0.4.2.1-src.tar.gz
tools_dir=ampr_emu-0.4.2.1/tools

make_fixture() {
  local d=$1
  mkdir -p "$d/sce_sys" "$d/data/levels" "$d/data/empty"
  # a minimal eboot.bin that carries the import name Forge looks for
  { printf '\177ELF'; head -c 4096 /dev/zero; printf 'libSceAmpr\0'; head -c 512 /dev/zero; } >"$d/eboot.bin"
  printf '{"titleId":"PPSA00001","contentId":"IV0000-PPSA00001_00-0000000000000000","applicationCategoryType":0,"localizedParameters":{"defaultLanguage":"en-US","en-US":{"titleName":"Fixture"}}}\n' >"$d/sce_sys/param.json"
  # compressible assets (a few chunks each), one incompressible, one tiny
  for i in 1 2 3; do
    awk -v n="$i" 'BEGIN { for (k = 0; k < 40000; k++) printf "level %d line %d some repeating text\n", n, k % 97 }' >"$d/data/levels/level$i.bin"
  done
  head -c 300000 /dev/urandom >"$d/data/random.bin"
  printf 'tiny\n' >"$d/data/tiny.txt"
}

need_docker() { docker info >/dev/null 2>&1 || { echo "check-lz4.sh: Docker is not available" >&2; exit 2; }; }

# Runs `$2...` inside python:3 with lz4 installed. $1 = work dir mounted at /w (tools at /w/tools).
in_docker() {
  local w=$1; shift
  docker run --rm -v "$w":/w -w /w/tools python:3 sh -c 'pip install -q lz4 >/dev/null 2>&1 && "$@"' sh "$@"
}

extract_tools() {
  local w=$1
  mkdir -p "$w/tools"
  tar -xzf "$archive" -C "$w" "$tools_dir"
  cp -R "$w/$tools_dir/." "$w/tools/"
}

case ${1:-} in
--make-fixture)
  make_fixture "${2:?usage: check-lz4.sh --make-fixture <dir>}"
  exit 0 ;;
--reverse)
  need_docker
  work=$(mktemp -d)
  trap 'rm -rf "$work"' EXIT
  extract_tools "$work"
  make_fixture "$work/orig"
  cp -R "$work/orig" "$work/ref"
  # index of the loose game, then upstream packing in place and removal of the packed sources
  in_docker "$work" python3 -B build_ampr_index.py /w/ref -o /w/ampr.index
  in_docker "$work" python3 -B ampr_pack.py pack --root /w/ref --ampr-index /w/ampr.index \
    --output /w/ref --no-progress
  [[ -e $work/ref/ampr_assets.index.crc ]] || { echo "upstream pack wrote no .crc sidecar" >&2; exit 1; }
  # upstream packs in place and leaves the sources; remove the packed ones, as a real deployment
  # has them only inside the volumes (Forge's reader hides packed files by exact name)
  in_docker "$work" python3 -B ampr_pack.py remove-packed-sources --root /w/ref \
    --index /w/ref/ampr_assets.index --confirm
  (cd "$repo" && cargo run --locked --release -q -p ps5-dump-forge-cli -- convert "$work/ref" --to folder --lz4-unpack \
    --output "$work/unpacked" ) || { echo "Forge could not unpack the upstream pack" >&2; exit 1; }
  out=$work/unpacked
  # logical files: everything of the original must come back byte-exact
  diff -r "$work/orig" "$out" -x ampr_emu.index -x libSceAmpr.sprx -x 'ampr_assets*' \
    && echo "reverse: unpacked assets equal the original, byte for byte"
  ;;
*)
  out=${1:?usage: check-lz4.sh <forge_lz4_output_dir> <original_source_dir> | --reverse | --make-fixture <dir>}
  orig=${2:?usage: check-lz4.sh <forge_lz4_output_dir> <original_source_dir>}
  out=$(cd "$out" && pwd); orig=$(cd "$orig" && pwd)
  need_docker
  work=$(mktemp -d)
  trap 'rm -rf "$work"' EXIT
  extract_tools "$work"
  run() { docker run --rm -v "$work":/w -v "$out":/out:ro -v "$orig":/orig:ro -w /w/tools python:3 \
    sh -c 'pip install -q lz4 >/dev/null 2>&1 && "$@"' sh "$@"; }
  run python3 -B ampr_pack.py inspect --index /out/ampr_assets.index
  run python3 -B ampr_pack.py verify --index /out/ampr_assets.index
  run python3 -B ampr_pack.py verify --index /out/ampr_assets.index --root /orig
  echo "forward: upstream inspect and verify accepted the pack"
  ;;
esac
