#!/usr/bin/env bash
# Run FreeBSD's own `fsck_ufs -n` and `dumpfs` on raw UFS2 images (the
# independent check, because a writer and its own reader can share bugs).
#
# macOS and current Debian/Ubuntu ship no UFS tooling, so this boots an official FreeBSD
# arm64 VM image under qemu (TCG) inside a Docker container, attaches every image as an
# extra read-only virtio disk (vtbd1, vtbd2, ...) and drives the serial console with expect.
# The VM boots from a throwaway overlay, so the cached base image is never modified.
#
#   scripts/fsck-ufs.sh a.ffpkg b.ffpkg
#
# FSCK_UFS_EXTRA: one more shell line run as root in the VM after the checks, e.g. a
# reference layout to diff dumpfs against (newfs on a sparse md of the same size):
#   FSCK_UFS_EXTRA='truncate -s 136511488 /tmp/r; md=$(mdconfig -f /tmp/r);
#     newfs -O 2 -b 65536 -f 65536 -m 0 -S 4096 -i 65536 /dev/$md >/dev/null; dumpfs /dev/$md | head -45'
#
# Cache: $PS5_FORGE_CACHE (default ~/.cache/ps5-dump-forge), the ~2 GB qcow2. A run takes
# about a minute on Apple silicon. Exit status 0 only when every image is explicitly clean
# (see chk below): fsck_ufs -n exits 0 even when it reports and declines a repair.
set -euo pipefail

[ $# -ge 1 ] || { echo "usage: $0 <image>..." >&2; exit 2; }
CACHE=${PS5_FORGE_CACHE:-$HOME/.cache/ps5-dump-forge}
REL=14.4-RELEASE
BASE=FreeBSD-$REL-arm64-aarch64-ufs.qcow2
URL=https://download.freebsd.org/releases/VM-IMAGES/$REL/aarch64/Latest/$BASE.xz
TAG=ps5-dump-forge-ufs-vm

mkdir -p "$CACHE"
if [ ! -f "$CACHE/$BASE" ]; then
  curl -fL --retry 3 -o "$CACHE/$BASE.xz" "$URL"
  xz -d "$CACHE/$BASE.xz"
fi

docker build -q -t "$TAG" - >/dev/null <<'DOCKER'
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
      qemu-system-arm qemu-efi-aarch64 qemu-utils expect && rm -rf /var/lib/apt/lists/*
DOCKER

# Each image is bind-mounted read-only at /img/N.
mounts=() ; n=0
for img in "$@"; do
  n=$((n + 1))
  mounts+=(-v "$(cd "$(dirname "$img")" && pwd)/$(basename "$img"):/img/$n:ro")
done

docker run --rm -i -v "$CACHE:/cache:ro" "${mounts[@]}" -e N="$n" \
  -e BASE="$BASE" -e EXTRA="${FSCK_UFS_EXTRA:-}" "$TAG" bash -s <<'INNER'
set -euo pipefail
qemu-img create -q -f qcow2 -F qcow2 -b "/cache/$BASE" /tmp/boot.qcow2
drives=""
for i in $(seq 1 "$N"); do drives="$drives -drive file=/img/$i,format=raw,if=virtio,readonly=on"; done
# The check run per disk inside the VM. Clean means: exit status 0, the final
# "N files, N used, N free" summary present, and no line other than the phase headers and
# that summary (fsck_ufs -n answers NO to every repair, so any finding is an extra line).
cat > /tmp/chk.sh <<'CHK'
chk() {
d=/dev/vtbd$1; echo "===== $d"
fsck_ufs -n $d >/tmp/f 2>&1; rc=$?; cat /tmp/f; echo "FSCK_RC=$rc"
if [ $rc = 0 ] && grep -qE '^[0-9]+ files, ' /tmp/f && ! grep -vqE '^\*\* (/dev/|Last Mounted|Phase [1-5] )|^[0-9]+ files, ' /tmp/f
then echo "VERDICT $d IS CLEAN"; else echo "VERDICT $d NOT CLEAN"; fi
dumpfs $d | head -40
}
CHK
cat > /tmp/run.exp <<'EXP'
set timeout 1200
log_user 1
spawn {*}$env(QEMU)
expect "login:"
send "root\r"
expect -re {# $}
send "export PS1='VM# '\r"
expect "VM# "
foreach line [split [read [open /tmp/chk.sh]] "\n"] {
  if {$line eq ""} continue
  send -- "$line\r"
  expect -re {(VM# |> )$}
}
for {set i 1} {$i <= $env(N)} {incr i} {
  send "chk $i\r"
  expect "VM# "
}
if {$env(EXTRA) ne ""} { send -- "$env(EXTRA)\r"; expect "VM# " }
send "shutdown -p now\r"
expect eof
EXP
export QEMU="qemu-system-aarch64 -M virt -cpu cortex-a57 -smp 4 -m 2048 -nographic \
  -bios /usr/share/qemu-efi-aarch64/QEMU_EFI.fd \
  -drive file=/tmp/boot.qcow2,format=qcow2,if=virtio $drives -nic none"
expect /tmp/run.exp | tr -d '\r' | tee /tmp/log >/dev/null
# Print only our part of the session, not the boot noise.
sed -n '/^===== \/dev\/vtbd1/,$p' /tmp/log
clean=$(grep -c '^VERDICT .* IS CLEAN' /tmp/log || true)
[ "$clean" = "$N" ] || { echo "fsck-ufs: only $clean of $N image(s) clean" >&2; exit 1; }
echo "fsck-ufs: all $N image(s) clean"
INNER
