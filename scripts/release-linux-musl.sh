#!/usr/bin/env bash
# The static Linux CLI (PR #57: a NAS, Alpine, an old glibc): ps5-dump-forge alone, the web UI
# embedded for `serve`. Built in the pinned Rust's Alpine image, a musl host, so the C and C++
# parts (liblz4, libzstd, UnRAR, BLAKE3, ring) link statically with no cross toolchain. Writes
# the bare stripped binary dist/ps5-dump-forge-cli-<version>-linux-<arch> for this machine's arch
# (the release job runs it on an x86-64 and an arm64 runner; musl's MIT notice, in
# THIRD-PARTY-NOTICES.md, ships in the release's source archives). Needs Docker and npm.
set -euo pipefail
cd "$(dirname "$0")/.."

# rust:1.100.0-alpine (rust-toolchain.toml's channel), by digest.
image=rust:1.100.0-alpine@sha256:de1e0058543a81ed7b046629167db587c67a94bdf37cce14eb27f0ec83baaffb
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n1)
[[ -n $version ]] || { echo "release-linux-musl.sh: no version in Cargo.toml" >&2; exit 1; }
case $(uname -m) in
  x86_64) arch=x86-64 ;;
  aarch64 | arm64) arch=arm64 ;;
  *) echo "release-linux-musl.sh: unsupported arch $(uname -m)" >&2; exit 1 ;;
esac
name=ps5-dump-forge-cli-$version-linux-$arch

(cd app && npm ci --no-audit --no-fund && npm run build:http)

mkdir -p dist
# Its own target folder, never mixed with the host's builds; handed back to the caller's user.
docker run --rm -v "$PWD:/src" -w /src \
  -e FORGE_REQUIRE_WEB=1 -e RUSTUP_TOOLCHAIN=1.100.0 -e RUSTUP_AUTO_INSTALL=0 \
  -e CARGO_TARGET_DIR=/src/target/musl -e NAME="$name" -e OWNER="$(id -u):$(id -g)" \
  "$image" sh -euc '
    apk add --no-cache build-base >/dev/null
    cargo build --release --locked -p ps5-dump-forge-cli
    bin=$CARGO_TARGET_DIR/release/ps5-dump-forge
    strip "$bin"
    if readelf -d "$bin" | grep -q NEEDED; then
      readelf -d "$bin" >&2
      echo "release-linux-musl.sh: $bin is not statically linked" >&2
      exit 1
    fi
    "$bin" --help >/dev/null
    # The real web UI for `serve`, not the placeholder page.
    grep -qF forge.appcache "$bin" ||
      { echo "release-linux-musl.sh: $bin does not embed the web UI" >&2; exit 1; }
    cp "$bin" "dist/$NAME"
    chmod 755 "dist/$NAME"
    chown -R "$OWNER" "$CARGO_TARGET_DIR" "dist/$NAME"'

ls -l "dist/$name"
