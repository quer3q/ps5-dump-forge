#!/usr/bin/env bash
# Builds target/ps5/ps5-dump-forge.elf, the CLI as a PS5 payload, in Docker. See ps5/README.md.
# The web UI it serves is built on the host first (node isn't in the image) and embedded;
# FORGE_SKIP_WEB=1 skips that (app/dist-http as it is, or a placeholder page).
# Two stages, since a running payload can't read its own file: stage 1 (target/ps5/stage1.elf)
# has no copy of itself; stage 2, the final ELF, embeds stage 1, which it saves as
# /data/ps5-dump-forge/ps5-dump-forge.elf (crates/ps5-dump-forge-server/src/self_copy.rs).
# A debug build (FORGE_DEBUG_API set) stops after stage 1: the final ELF is stage 1, no copy.
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ "${1:-}" != --inside ]]; then
  require_web=1
  if [[ "${FORGE_SKIP_WEB:-}" == 1 ]]; then
    echo "ps5/build.sh: FORGE_SKIP_WEB=1: not building the web UI; embedding app/dist-http as it is, or a placeholder page" >&2
    require_web=0
  else
    (cd app && npm run build:http)
  fi
  docker build -t ps5-dump-forge-ps5 ps5
  exec docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp \
    -e CARGO_HOME=/work/target/ps5/cargo -e FORGE_DEBUG_API -e FORGE_REQUIRE_WEB="$require_web" \
    -v "$PWD:/work" ps5-dump-forge-ps5 ps5/build.sh --inside
fi

out=target/ps5
cc="$PS5_PAYLOAD_SDK/bin/prospero-clang"
mkdir -p "$out/shim"
for src in ps5/shim/freebsd11.c ps5/shim/syscalls.S ps5/entry.c ps5/compat.c ps5/launcher.c; do
  name=$(basename "${src%.*}")
  "$cc" -O2 -Wall -Wextra -Werror -c "$src" -o "$out/shim/$name.o"
done

export RUSTC_BOOTSTRAP=1
# The PS5 kernel fills FreeBSD 11 structs (stat 120 bytes, dirent 264, old statfs); libc and the
# std built here default to FreeBSD 12's. libc >= 0.2.187 reads this cfg (it ignores the old
# RUST_LIBC_UNSTABLE_FREEBSD_VERSION env var); rustflags reach std too under -Z build-std.
# The CLI asserts the struct sizes at compile time, so losing this fails the build.
export CARGO_TARGET_X86_64_PS5_FREEBSD_RUSTFLAGS='--cfg libc_unstable_freebsd_version="11"'
export CARGO_TARGET_DIR="$out"
export CARGO_TARGET_X86_64_PS5_FREEBSD_LINKER=/work/ps5/linker.sh
export CC_x86_64_ps5_freebsd="$cc" AR_x86_64_ps5_freebsd="$PS5_PAYLOAD_SDK/bin/prospero-ar"
# cc-rs (blake3's SIMD assembly) passes --target=x86_64-unknown-freebsd-ps5, which clang refuses;
# a later --target wins, so put the SDK's own target back (as ps5-ai-cli's check-codex.sh does).
export CFLAGS_x86_64_ps5_freebsd='--target=x86_64-sie-ps5 -mno-red-zone -femulated-tls'
# Cargo doesn't see the C objects or linker.sh; dropping the CLI's linked output makes it relink
# (about 8 s) so a shim or entry change always lands in the ELF.
build() {
  rm -f "$out"/x86_64-ps5-freebsd/release/deps/ps5_dump_forge-*
  cargo build --release --locked -p ps5-dump-forge-cli \
    --target ps5/x86_64-ps5-freebsd.json -Z json-target-spec \
    -Z build-std=std,panic_abort -Z build-std-features=panic-unwind
}

# Stage 1 is copied out (stage 2's build writes the same cargo output) before stage 2 reads it.
unset FORGE_SELF_ELF
build
cp "$out/x86_64-ps5-freebsd/release/ps5-dump-forge" "$out/stage1.elf"
if [[ -n ${FORGE_DEBUG_API+x} ]]; then
  echo "ps5/build.sh: FORGE_DEBUG_API is set: no stage 2; the ELF carries no copy of itself" >&2
  cp "$out/stage1.elf" "$out/ps5-dump-forge.elf"
else
  FORGE_SELF_ELF="$PWD/$out/stage1.elf" build
  cp "$out/x86_64-ps5-freebsd/release/ps5-dump-forge" "$out/ps5-dump-forge.elf"
fi
llvm-readelf -h -d "$out/ps5-dump-forge.elf" > "$out/ps5-dump-forge.elf.txt"
ls -l "$out/stage1.elf" "$out/ps5-dump-forge.elf"
