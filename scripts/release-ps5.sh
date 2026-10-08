#!/usr/bin/env bash
# Build and check the PS5 release (on macOS or GitHub's ubuntu-latest; needs Docker and Node):
#
#   dist/ps5-dump-forge-<ver>-ps5.elf  the CLI as a PS5 payload (ps5/build.sh), the web UI embedded
#
# Just the ELF: no zip, no PS5 README, no notices (the release's source tarball carries those).
# The checks run on the copy in dist/ (B) and on ps5/build.sh's stage 1 (A, which B embeds and
# saves on the console): llvm-readelf from the ps5-dump-forge-ps5 Docker image (built here if
# missing) for the payload's shape (ELF64 little-endian x86-64, a PIE DYN, NEEDED exactly the
# SDK's four modules, no PT_INTERP or TLS, the entry point in an executable PT_LOAD), then
# python3 on the host: every file of app/dist-http is in it byte for byte (not the placeholder
# page), no debug routes, and its version marker (VERSION_MARKER in the server) is the workspace
# version. Then the self copy (crates/ps5-dump-forge-server/src/self_copy.rs): A carries none,
# and B exactly one, which decompresses to A byte for byte, its length and SHA-256 matching.
# The release asset is B alone.
#
#   scripts/release-ps5.sh                 # build the web UI and the ELF, then copy and check
#   SKIP_BUILD=1 scripts/release-ps5.sh    # copy and check what is already built
#
# Env: DIST_DIR (default <repo>/dist). FORGE_SKIP_WEB=1 is refused: the release always embeds a
# freshly built web UI.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

scripts/check-versions.sh
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n1)
[[ -n $version ]] || { echo "release-ps5.sh: no version in Cargo.toml" >&2; exit 1; }
dist=${DIST_DIR:-$root/dist}
[[ $dist == /* ]] || dist=$root/$dist
image=ps5-dump-forge-ps5
want_needed="libSceAppInstUtil.sprx libSceLibcInternal.sprx libSceSystemService.sprx libkernel_web.sprx"

# The debug routes (crates/ps5-dump-forge-server/src/debug.rs) are for bring-up only; any value
# of FORGE_DEBUG_API compiles them in, and the check below rejects an ELF that has them.
[[ -z ${FORGE_DEBUG_API+x} ]] ||
  { echo "release-ps5.sh: FORGE_DEBUG_API is set: a release has no debug routes; unset it" >&2; exit 1; }
if [[ ${SKIP_BUILD:-} != 1 ]]; then
  [[ ${FORGE_SKIP_WEB:-} != 1 ]] ||
    { echo "release-ps5.sh: FORGE_SKIP_WEB=1: a release must embed the built web UI; unset it" >&2; exit 1; }
  echo "== PS5 payload (web UI + ELF)"
  # npm ci only into a fresh checkout: it deletes node_modules, which a running dev server uses.
  [[ -d app/node_modules ]] || (cd app && npm ci)
  ps5/build.sh
fi

src=target/ps5/ps5-dump-forge.elf
stage1=target/ps5/stage1.elf
for f in "$src" "$stage1"; do
  [[ -f $f ]] || { echo "release-ps5.sh: $f is missing (build first)" >&2; exit 1; }
done
[[ -f app/dist-http/index.html ]] ||
  { echo "release-ps5.sh: app/dist-http/index.html is missing (build first)" >&2; exit 1; }

name=ps5-dump-forge-$version-ps5.elf
elf=$dist/$name
mkdir -p "$dist"
rm -f "$dist"/ps5-dump-forge-*-ps5.elf
# A copy that fails, or fails a check, is not left behind for the upload.
trap '[[ $? == 0 ]] || rm -f "$elf"' EXIT
cp "$src" "$elf"

docker image inspect "$image" > /dev/null 2>&1 || docker build -q -t "$image" ps5 > /dev/null

# check <elf> [<stage 1 elf>]: every check on one ELF. With a stage 1, the ELF is the final one,
# which must carry that stage 1 as its self copy; without, it must carry none.
check() {
  local file=$1 a=${2:-}
  echo "== verify $file"
  fail() {
    echo "release-ps5.sh: $file: $*" >&2
    exit 1
  }
  # The ELF goes in on stdin, not a bind mount: Docker Desktop's file sharing can serve a stale
  # view of a file just replaced.
  readelf=$(docker run --rm -i "$image" sh -c "cat > /tmp/check.elf && llvm-readelf --wide -h -l -d /tmp/check.elf" \
    < "$file") || fail "llvm-readelf failed"

  # field <label>: the value of an ELF header line (`  Label:   value`).
  field() {
    sed -n "s/^  $1: *//p" <<< "$readelf" | head -n1
  }

  [[ $(field Class) == ELF64 ]] || fail "class is [$(field Class)], want ELF64"
  [[ $(field Data) == "2's complement, little endian" ]] || fail "data is [$(field Data)], want little endian"
  [[ $(field Machine) == "Advanced Micro Devices X86-64" ]] || fail "machine is [$(field Machine)], want X86-64"
  [[ $(field Type) == "DYN (Shared object file)" ]] || fail "type is [$(field Type)], want DYN"
  flags_1=$(sed -n 's/.*(FLAGS_1) *//p' <<< "$readelf")
  [[ " $flags_1 " == *" PIE "* ]] || fail "DT_FLAGS_1 is [$flags_1], want PIE"
  needed=$(sed -n 's/.*(NEEDED) *Shared library: \[\(.*\)\]$/\1/p' <<< "$readelf" | LC_ALL=C sort | xargs)
  [[ $needed == "$want_needed" ]] || fail "NEEDED is [$needed], want [$want_needed]"

  # Program headers: `Type Offset VirtAddr PhysAddr FileSiz MemSiz Flg Align`, where Flg may hold
  # a space ("R E"): the flags are everything between MemSiz and Align.
  entry=$(field 'Entry point address')
  [[ $entry == 0x* ]] || fail "no entry point address"
  size=$(wc -c < "$file")
  in_exec_load=0
  while read -r type offset vaddr _ filesz _ rest; do
    case $type in
      INTERP) fail "has a PT_INTERP program header" ;;
      TLS) fail "has a PT_TLS program header" ;;
      LOAD)
        (( offset + filesz <= size )) || fail "a PT_LOAD runs past the end of the file"
        flags=${rest% *}
        # FileSiz, not MemSiz: an entry in a segment's zero-filled tail has no code behind it.
        if [[ $flags == *E* ]] && (( vaddr <= entry && entry < vaddr + filesz )); then
          in_exec_load=1
        fi ;;
    esac
  done < <(sed -n '/^Program Headers:/,/^$/p' <<< "$readelf" | sed '1,2d')
  [[ $in_exec_load == 1 ]] || fail "entry point $entry is not inside an executable PT_LOAD"

  # Embedded web UI (build.rs's rule: every file, dotfiles left out), the version marker and the
  # self copy.
  python3 -I - "$file" app/dist-http "$version" "$a" <<'EOF'
import hashlib, os, re, struct, sys, zlib
elf, web, version, stage1 = sys.argv[1:]
data = open(elf, "rb").read()
def fail(why):
    sys.exit(f"release-ps5.sh: {elf}: {why}")

# Every valid blob: `PDFGSELF`, u64 LE raw length, u64 LE compressed length, SHA-256, then a zlib
# stream that decompresses to exactly that length and hash. The magic also sits in the code that
# reads it, with no such descriptor behind it.
def blobs(data):
    found, at = [], data.find(b"PDFGSELF")
    while at >= 0:
        if at + 56 <= len(data):
            raw_len, packed_len = struct.unpack_from("<QQ", data, at + 8)
            digest, end = data[at + 24:at + 56], at + 56 + packed_len
            if end <= len(data) and raw_len <= 256 << 20:
                try:
                    raw = zlib.decompressobj().decompress(data[at + 56:end], raw_len + 1)
                except zlib.error:
                    raw = None
                if raw is not None and len(raw) == raw_len and hashlib.sha256(raw).digest() == digest:
                    found.append((at, end, raw))
        at = data.find(b"PDFGSELF", at + 1)
    return found

missing = []
for d, dirs, files in os.walk(web):
    dirs[:] = [x for x in dirs if not x.startswith(".")]
    for f in files:
        path = os.path.join(d, f)
        if not f.startswith(".") and open(path, "rb").read() not in data:
            missing.append(path)
if missing:
    fail(f"not embedded byte for byte: {', '.join(sorted(missing))}")
# debug.rs's JSON keys: present only in a build with the debug routes.
if b"std_canonicalize" in data:
    fail("built with FORGE_DEBUG_API (debug routes); rebuild without it")

found = blobs(data)
rest = data
if not stage1:
    if found:
        fail(f"carries {len(found)} self-copy blob(s); stage 1 must carry none")
else:
    if len(found) != 1:
        fail(f"carries {len(found)} self-copy blobs, want one (ps5/build.sh's stage 2)")
    at, end, raw = found[0]
    a = open(stage1, "rb").read()
    if raw != a:
        fail(f"its self copy ({len(raw)} bytes) is not {stage1} ({len(a)} bytes) byte for byte")
    if struct.unpack_from("<Q", data, at + 8)[0] != len(a) or data[at + 24:at + 56] != hashlib.sha256(a).digest():
        fail(f"its self copy's length or SHA-256 is not {stage1}'s")
    print(f"self copy: a {end - at} byte blob at {at:#x}, {stage1} byte for byte "
          f"({len(a)} bytes, SHA-256 {hashlib.sha256(a).hexdigest()})")
    # The marker is counted outside the blob, whose compressed bytes are A's, not this ELF's.
    rest = data[:at] + data[end:]
found = re.findall(rb"ps5-dump-forge-version:([^\0]*)\0", rest)
if found != [version.encode()]:
    found = [x.decode(errors="replace") for x in found]
    fail(f"version markers {found}, want one, {version}")
EOF
  echo "ELF64 x86-64 PIE DYN, NEEDED [$needed], no INTERP/TLS, entry $entry executable," \
    "app/dist-http embedded, version $version"
}

check "$stage1"
check "$elf" "$stage1"
ls -l "$stage1" "$elf"
if command -v sha256sum > /dev/null; then sha256sum "$elf"; else shasum -a 256 "$elf"; fi
