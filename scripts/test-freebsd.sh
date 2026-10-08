#!/usr/bin/env bash
# Run the ps5-dump-forge-core and ps5-dump-forge-server tests on real FreeBSD filesystems: UFS2,
# FAT32 (msdosfs) and a nullfs view over UFS (the PS5's /data can be one), to exercise the core's
# FreeBSD paths and the server's file handling (canonical paths, symlinks, roots) there.
#
# The tests are cross-compiled for aarch64-unknown-freebsd in Docker (Rust + clang/lld against a
# FreeBSD 14.4 base.txz sysroot), packed with a runner into a tar file, and the tar is attached
# as a raw read-only virtio disk to the same FreeBSD 14.4 arm64 VM scripts/fsck-ufs.sh boots
# (qemu TCG in Docker, expect on the serial console, throwaway overlay; no guest networking).
# In the guest each volume is a fresh 1 GiB md device mounted at $T, and the test binaries run
# there with TMPDIR=$T. Integration tests use the compile-time CARGO_TARGET_TMPDIR instead, so
# the build runs with CARGO_TARGET_DIR=/forge and $T is that build's tmp dir,
# /forge/aarch64-unknown-freebsd/tmp (checked against the built binary).
#
#   scripts/test-freebsd.sh
#   FREEBSD_TESTS_UFS='scan' FREEBSD_VOLUMES='ufs nullfs' scripts/test-freebsd.sh
#
# Per volume (UFS, FAT, NULLFS), both optional:
#   FREEBSD_BINS_<VOL>   test binaries to run, space separated: `lib` (core's unit tests),
#                        `server-lib` (the server's), an integration test target name (`core`,
#                        `serve`, ...), or `all` (default for UFS and NULLFS; FAT defaults to
#                        `lib freebsd_volumes`: tests/core.rs and the server's fixtures need
#                        symlinks, hard links and FIFOs, which FAT32 cannot hold).
#   FREEBSD_TESTS_<VOL>  libtest arguments passed to every binary on that volume: name
#                        filters and flags, e.g. 'fat_' or '--skip slow --include-ignored'.
#                        Word-split by the guest's sh; default empty (every test).
# FREEBSD_VOLUMES       volumes to run, in order (default 'ufs fat nullfs').
# FREEBSD_TIMEOUT       seconds for the whole VM session before the run fails (default 3600).
#
# Cache: $PS5_FORGE_CACHE (default ~/.cache/ps5-dump-forge): the ~2 GB VM qcow2 (shared with
# fsck-ufs.sh), base.txz (~150 MB, checked against the release MANIFEST) and the sysroot
# extracted from it. Build output, cargo's registry and the full VM log (vm.log) go to
# target/freebsd/. Exit status 0 only when the guest prints the PASS sentinel, i.e. every
# selected test on every volume passed (never qemu's exit status alone).
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CACHE=${PS5_FORGE_CACHE:-$HOME/.cache/ps5-dump-forge}
OUT=$ROOT/target/freebsd
REL=14.4-RELEASE
BASE=FreeBSD-$REL-arm64-aarch64-ufs.qcow2
URL=https://download.freebsd.org/releases/VM-IMAGES/$REL/aarch64/Latest/$BASE.xz
DIST=https://download.freebsd.org/releases/arm64/aarch64/$REL
SYSROOT=freebsd-$REL-arm64-sysroot
VM_TAG=ps5-dump-forge-ufs-vm
BUILD_TAG=ps5-dump-forge-freebsd-build
TOOLCHAIN=1.99.0
VOLUMES=${FREEBSD_VOLUMES:-ufs fat nullfs}
TIMEOUT=${FREEBSD_TIMEOUT:-3600}

mkdir -p "$CACHE" "$OUT/target" "$OUT/cargo-home" "$OUT/stage"
if [ ! -f "$CACHE/$BASE" ]; then
  curl -fL --retry 3 -o "$CACHE/$BASE.xz" "$URL"
  xz -d "$CACHE/$BASE.xz"
fi
if [ ! -f "$CACHE/$SYSROOT/.done" ]; then
  curl -fsSL --retry 3 -o "$CACHE/$REL-arm64-MANIFEST" "$DIST/MANIFEST"
  [ -f "$CACHE/$REL-arm64-base.txz" ] ||
    curl -fL --retry 3 -o "$CACHE/$REL-arm64-base.txz" "$DIST/base.txz"
fi

# Same image as fsck-ufs.sh (identical Dockerfile, so the layer cache is shared).
docker build -q -t "$VM_TAG" - >/dev/null <<'DOCKER'
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
      qemu-system-arm qemu-efi-aarch64 qemu-utils expect && rm -rf /var/lib/apt/lists/*
DOCKER

docker build -q -t "$BUILD_TAG" - >/dev/null <<DOCKER
FROM rust:1.99.0-slim-trixie@sha256:24e632c09342c20abf8312cf4f61430a911c01ed3a5e4c02b87292b1c39c5273
RUN apt-get update && apt-get install -y --no-install-recommends \
      clang-19 lld-19 llvm-19 jq xz-utils && rm -rf /var/lib/apt/lists/*
RUN rustup target add --toolchain $TOOLCHAIN aarch64-unknown-freebsd && chmod -R a+rX "\$RUSTUP_HOME"
DOCKER

# The guest runner. Config comes from config.sh next to it (written below).
cat > "$OUT/stage/run.sh" <<'RUNNER'
#!/bin/sh
# Sums each binary's libtest "test result:" line per volume; a binary with no such line
# (crash, signal) or a nonzero exit with 0 failures counts as one failure.
. /forge/bin/config.sh
mkdir -p $T
total_fail=0
for vol in $VOLUMES; do
  case $vol in
    ufs) bins=$BINS_UFS; args=$TESTS_UFS ;;
    fat) bins=$BINS_FAT; args=$TESTS_FAT ;;
    nullfs) bins=$BINS_NULLFS; args=$TESTS_NULLFS ;;
    *) echo "unknown volume: $vol"; total_fail=$((total_fail + 1)); continue ;;
  esac
  [ "$bins" = all ] && bins=$ALL_BINS
  echo "===== forge-tests volume $vol (bins: $bins; args: $args)"
  md=$(mdconfig -a -t swap -s 1g) || { echo "mdconfig failed"; total_fail=$((total_fail + 1)); continue; }
  ok=1
  case $vol in
    ufs) newfs -U /dev/$md >/dev/null && mount /dev/$md $T || ok=0 ;;
    nullfs) mkdir -p /forge/ufs && newfs -U /dev/$md >/dev/null && mount /dev/$md /forge/ufs &&
              mount -t nullfs /forge/ufs $T || ok=0 ;;
    fat) newfs_msdos -F 32 -c 8 -h 64 -u 32 /dev/$md >/dev/null && mount -t msdosfs /dev/$md $T || ok=0 ;;
  esac
  mount | grep ' /forge/'
  passed=0; failed=0; ignored=0; names=""
  if [ $ok = 0 ]; then echo "volume setup failed"; failed=1; else
    for b in $bins; do
      [ -x /forge/bin/$b ] || { echo "no test binary: $b"; failed=$((failed + 1)); continue; }
      echo "--- $vol: $b $args"
      (cd $T && TMPDIR=$T /forge/bin/$b $args) >/tmp/out 2>&1
      rc=$?
      cat /tmp/out
      r=$(grep '^test result: ' /tmp/out | tail -1)
      if [ -z "$r" ]; then
        echo "--- $vol: $b exited $rc without a test result"; failed=$((failed + 1)); names="$names $b(crashed)"
      else
        p=$(echo "$r" | sed -n 's/.* \([0-9]*\) passed.*/\1/p')
        f=$(echo "$r" | sed -n 's/.* \([0-9]*\) failed.*/\1/p')
        i=$(echo "$r" | sed -n 's/.* \([0-9]*\) ignored.*/\1/p')
        passed=$((passed + p)); failed=$((failed + f)); ignored=$((ignored + i))
        [ "$f" = 0 ] && [ $rc != 0 ] && { failed=$((failed + 1)); names="$names $b(exit-$rc)"; }
        for n in $(sed -n 's/^test \(.*\) \.\.\. FAILED$/\1/p' /tmp/out); do names="$names $b:$n"; done
      fi
      # Tests keep their temp folders; empty the volume so the next binary gets all of it.
      rm -rf $T/* $T/.[!.]* 2>/dev/null
    done
  fi
  umount $T 2>/dev/null; umount /forge/ufs 2>/dev/null; mdconfig -d -u $md
  echo "FORGE-TESTS SUMMARY $vol: $passed passed, $failed failed, $ignored ignored"
  [ -n "$names" ] && echo "FORGE-TESTS FAILED $vol:$names"
  total_fail=$((total_fail + failed))
done
if [ $total_fail = 0 ]; then verdict=PASS; else verdict=FAIL; fi
echo "FORGE-TESTS RESULT: $verdict"
RUNNER

q() { printf "'%s'" "$(printf %s "$1" | sed "s/'/'\\\\''/g")"; }
{
  echo "VOLUMES=$(q "$VOLUMES")"
  echo "BINS_UFS=$(q "${FREEBSD_BINS_UFS:-all}")"
  echo "BINS_FAT=$(q "${FREEBSD_BINS_FAT:-lib freebsd_volumes}")"
  echo "BINS_NULLFS=$(q "${FREEBSD_BINS_NULLFS:-all}")"
  echo "TESTS_UFS=$(q "${FREEBSD_TESTS_UFS:-}")"
  echo "TESTS_FAT=$(q "${FREEBSD_TESTS_FAT:-}")"
  echo "TESTS_NULLFS=$(q "${FREEBSD_TESTS_NULLFS:-}")"
} > "$OUT/stage/config.sh"

# Cross-build, then pack the test binaries (named lib / <test target>) with the runner.
docker run --rm -i --user "$(id -u):$(id -g)" -v "$ROOT:/src" -v "$OUT/target:/forge" \
  -v "$CACHE:/cache" -e SYSROOT="/cache/$SYSROOT" -e REL="$REL" "$BUILD_TAG" bash -s <<'BUILD'
set -euo pipefail
if [ ! -f "$SYSROOT/.done" ]; then
  want=$(awk '$1 == "base.txz" { print $2 }' "/cache/$REL-arm64-MANIFEST")
  echo "$want  /cache/$REL-arm64-base.txz" | sha256sum -c --quiet
  rm -rf "$SYSROOT" && mkdir -p "$SYSROOT"
  tar -xJf "/cache/$REL-arm64-base.txz" -C "$SYSROOT" ./lib ./usr/lib ./usr/include
  chmod -R u+w "$SYSROOT" && touch "$SYSROOT/.done"
fi
export RUSTUP_TOOLCHAIN=1.99.0 RUSTUP_AUTO_INSTALL=0
export CARGO_HOME=/src/target/freebsd/cargo-home CARGO_TARGET_DIR=/forge
t="--target=aarch64-unknown-freebsd14.4 --sysroot=$SYSROOT"
export CARGO_TARGET_AARCH64_UNKNOWN_FREEBSD_LINKER=clang-19
export CARGO_TARGET_AARCH64_UNKNOWN_FREEBSD_RUSTFLAGS="-C link-arg=--target=aarch64-unknown-freebsd14.4 -C link-arg=--sysroot=$SYSROOT -C link-arg=-fuse-ld=lld"
export CC_aarch64_unknown_freebsd=clang-19 CFLAGS_aarch64_unknown_freebsd="$t"
export AR_aarch64_unknown_freebsd=llvm-ar-19
cd /src
cargo test --release --locked -p ps5-dump-forge-core -p ps5-dump-forge-server --no-run \
  --target aarch64-unknown-freebsd \
  --message-format=json-render-diagnostics >/tmp/build.json
stage=/src/target/freebsd/stage
find "$stage" -type f ! -name run.sh ! -name config.sh -delete
all=""
while read -r kind name exe; do
  if [ "$kind" = lib ]; then
    case $name in ps5_dump_forge_core) name=lib ;; *) name=server-lib ;; esac
  fi
  cp "$exe" "$stage/$name"; all="$all $name"
done < <(jq -r 'select(.reason == "compiler-artifact" and .executable != null and .profile.test)
  | "\(.target.kind[0]) \(.target.name) \(.executable)"' /tmp/build.json)
[ -n "$all" ] || { echo "no test executables built" >&2; exit 1; }
# Integration tests put their fixtures under the compile-time CARGO_TARGET_TMPDIR: the
# volume under test is mounted exactly there.
T=/forge/aarch64-unknown-freebsd/tmp
for b in core serve; do
  grep -qa "$T" "$stage/$b" || { echo "$b test binary does not embed $T" >&2; exit 1; }
done
echo "ALL_BINS='${all# }'; T=$T" >> "$stage/config.sh"
echo "built test binaries:$all"
tar -cf /src/target/freebsd/tests.tar -C "$stage" .
BUILD

docker run --rm -i -v "$CACHE:/cache:ro" -v "$OUT/tests.tar:/img/tests.tar:ro" -v "$OUT:/out" \
  -e BASE="$BASE" -e TIMEOUT="$TIMEOUT" "$VM_TAG" bash -s <<'INNER'
set -euo pipefail
qemu-img create -q -f qcow2 -F qcow2 -b "/cache/$BASE" /tmp/boot.qcow2
cat > /tmp/run.exp <<'EXP'
set timeout 600
log_user 1
spawn {*}$env(QEMU)
expect "login:"
send "root\r"
expect -re {# $}
send "export PS1='VM# '\r"
expect "VM# "
send "mkdir -p /forge/bin && tar -xf /dev/vtbd1 -C /forge/bin && sh /forge/bin/run.sh\r"
set timeout $env(TIMEOUT)
expect {
  "FORGE-TESTS RESULT: " { expect "VM# " }
  timeout { puts "\nforge-tests: guest timed out after $env(TIMEOUT)s"; exit 1 }
}
set timeout 600
send "shutdown -p now\r"
expect eof
EXP
export QEMU="qemu-system-aarch64 -M virt -cpu cortex-a57 -smp 4 -m 3072 -nographic \
  -bios /usr/share/qemu-efi-aarch64/QEMU_EFI.fd \
  -drive file=/tmp/boot.qcow2,format=qcow2,if=virtio \
  -drive file=/img/tests.tar,format=raw,if=virtio,readonly=on -nic none"
echo "booting the FreeBSD VM (full log: target/freebsd/vm.log)"
# Stream our part of the session, not the boot noise; keep the whole log.
timeout $((TIMEOUT + 1200)) expect /tmp/run.exp | sed -u 's/\r//g' | tee /out/vm.log |
  sed -u -n '/^===== forge-tests/,/^FORGE-TESTS RESULT/p' || true
INNER

grep -E '^FORGE-TESTS (SUMMARY|FAILED)' "$OUT/vm.log" || true
if grep -qx 'FORGE-TESTS RESULT: PASS' "$OUT/vm.log"; then
  echo "test-freebsd: every selected test passed"
else
  echo "test-freebsd: FAILED (no PASS sentinel; see target/freebsd/vm.log)" >&2
  exit 1
fi
