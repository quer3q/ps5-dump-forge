#!/usr/bin/env bash
# Independent checks of an .exfat image the writer produced: the writer's
# own reader can share its bugs, so two unrelated fsck implementations look at it.
#
#   scripts/check-exfat.sh <image.exfat> [source-dir]
#
# (a) macOS: attach the raw image without mounting, fsck_exfat -n, always detach.
# (b) Docker: Debian's exfatprogs fsck.exfat -n.
# (c) macOS, with a source dir: mount read-only and compare "files bytes alloc" (allocation
#     rounded to 64 KiB clusters, as exfat.sh does), then every file's hash and empty dir.
# Skips (a)/(c) off macOS and (b) without docker. Exits non-zero if any check fails.
set -euo pipefail

img=${1:?usage: check-exfat.sh <image.exfat> [source-dir]}
src=${2:-}
img=$(cd "$(dirname "$img")" && pwd)/$(basename "$img")
status=0
dev=""

# Raw image -> /dev/diskN, read-only, nothing mounted. diskutil's image verb is newer than
# hdiutil's (deprecated) attach; use whichever this macOS has.
attach() {
  if diskutil image attach --help >/dev/null 2>&1; then
    diskutil image attach --noMount --readOnly "$1" | awk 'NR==1 {print $1}'
  else
    hdiutil attach -nomount -readonly -imagekey diskimage-class=CRawDiskImage "$1" | awk 'NR==1 {print $1}'
  fi
}
detach() { [[ -z $dev ]] || diskutil eject "$dev" >/dev/null 2>&1 || hdiutil detach "$dev" >/dev/null 2>&1 || true; dev=""; }
trap detach EXIT INT TERM

# "files bytes alloc", macOS junk skipped — the same numbers exfat.sh compares.
stats() {
  find "$1" -type f ! -name .DS_Store ! -name '._*' ! -path '*/.fseventsd/*' ! -path '*/.Spotlight-V100/*' ! -path '*/.Trashes/*' \
    -exec stat -f %z {} + | awk -v c=65536 '{n++; s+=$1; a+=int(($1+c-1)/c)*c} END {print n+0, s+0, a+0}'
}

if [[ $(uname) == Darwin ]]; then
  echo "== fsck_exfat (macOS)"
  dev=$(attach "$img")
  fsck_exfat -n "/dev/r${dev#/dev/}" || status=1

  if [[ -n $src ]]; then
    echo "== read-only mount + compare"
    # macOS's FSKit exFAT driver mounts under /Volumes only; nobrowse keeps it out of Finder.
    diskutil mount readOnly -mountOptions nobrowse "$dev" >/dev/null
    mnt=$(diskutil info "$dev" | sed -n 's/^ *Mount Point: *//p' | grep .)
    want=$(stats "$src") got=$(stats "$mnt")
    echo "files bytes alloc: source $want, image $got"
    [[ $want == "$got" ]] || status=1
    # The driver hands names out decomposed (NFD) while the image stores them composed,
    # so compare NFC-normalized paths, contents and empty directories.
    python3 - "$src" "$mnt" <<'PY' || status=1
import hashlib, os, sys, unicodedata
skip = {".fseventsd", ".Spotlight-V100", ".Trashes", ".DS_Store"}
def tree(root):
    out = {}
    for d, dirs, files in os.walk(root):
        dirs[:] = [x for x in dirs if x not in skip]
        rel = unicodedata.normalize("NFC", os.path.relpath(d, root))
        if not dirs and not files and rel != ".":
            out[rel + "/"] = "dir"
        for f in files:
            if f in skip or f.startswith("._"):
                continue
            h = hashlib.sha256()
            with open(os.path.join(d, f), "rb") as fh:
                for block in iter(lambda: fh.read(1 << 20), b""):
                    h.update(block)
            out[unicodedata.normalize("NFC", os.path.join(rel, f))] = h.hexdigest()
    return out
a, b = tree(sys.argv[1]), tree(sys.argv[2])
bad = sorted(k for k in a.keys() | b.keys() if a.get(k) != b.get(k))
for k in bad:
    print("differs:", k, a.get(k, "missing"), b.get(k, "missing"))
print(f"{len(a)} entries compared, {len(bad)} differ")
sys.exit(1 if bad else 0)
PY
    diskutil unmount "$dev" >/dev/null
  fi
  detach
fi

if command -v docker >/dev/null 2>&1; then
  echo "== fsck.exfat (exfatprogs, Debian trixie)"
  docker run --rm -v "$(dirname "$img")":/img:ro debian:trixie sh -c \
    "apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq exfatprogs >/dev/null && fsck.exfat -n '/img/$(basename "$img")'" \
    || status=1
fi

[[ $status == 0 ]] && echo "ALL CHECKS PASSED" || echo "SOME CHECKS FAILED"
exit $status
