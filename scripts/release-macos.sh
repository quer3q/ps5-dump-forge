#!/usr/bin/env bash
# Build and package the macOS arm64 release:
#
#   dist/ps5-dump-forge-<ver>-macos-arm64.zip     one folder: PS5 Dump Forge.app, ps5-dump-forge (CLI),
#                                                 README.md, LICENSE, THIRD-PARTY-NOTICES.md
#   dist/ps5-dump-forge-<ver>-source.tar.gz       `git archive HEAD` (GPL: the source ships with it)
#   dist/SHA256SUMS
#
# Ad-hoc signed only (no Developer ID, no notarization): Tauri signs "PS5 Dump Forge.app" with
# `signingIdentity: "-"` (tauri.conf.json), the CLI is signed here with `codesign -s -`, and
# both must pass `codesign --verify --strict` before and after a round trip through the zip.
#
#   scripts/release-macos.sh                 # build everything, then package
#   SKIP_BUILD=1 scripts/release-macos.sh    # package what is already built
#
# Env: CARGO_TARGET_DIR (default <repo>/target; made absolute, because `tauri build` resolves a
# relative one from app/src-tauri), DIST_DIR (default <repo>/dist).
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
[[ $(uname -s) == Darwin ]] || { echo "release-macos.sh: needs macOS (codesign, ditto)" >&2; exit 1; }

target=aarch64-apple-darwin
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n1)
[[ -n $version ]] || { echo "release-macos.sh: no version in Cargo.toml" >&2; exit 1; }
target_dir=${CARGO_TARGET_DIR:-$root/target}
[[ $target_dir == /* ]] || target_dir=$root/$target_dir
export CARGO_TARGET_DIR=$target_dir
dist=${DIST_DIR:-$root/dist}
name=ps5-dump-forge-$version-macos-arm64

if [[ ${SKIP_BUILD:-} != 1 ]]; then
  echo "== PS5 Dump Forge.app ($target)"
  # npm ci only into a fresh checkout: it deletes node_modules, which a running dev server uses.
  [[ -d app/node_modules ]] || (cd app && npm ci)
  (cd app && npx tauri build --ci --target "$target" --bundles app)
  echo "== CLI ($target)"
  cargo build --release --locked -p ps5-dump-forge-cli --target "$target"
fi

app="$target_dir/$target/release/bundle/macos/PS5 Dump Forge.app"
cli=$target_dir/$target/release/ps5-dump-forge
for f in "$app" "$cli"; do
  [[ -e "$f" ]] || { echo "release-macos.sh: $f is missing (build first)" >&2; exit 1; }
done

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
pkg=$stage/$name
mkdir "$pkg"
ditto "$app" "$pkg/PS5 Dump Forge.app"
cp "$cli" "$pkg/ps5-dump-forge"
cp README.md LICENSE THIRD-PARTY-NOTICES.md "$pkg/"
codesign --force -s - "$pkg/ps5-dump-forge"

check() {
  local dir=$1
  echo "== verify $dir"
  codesign --verify --deep --strict --verbose=2 "$dir/PS5 Dump Forge.app"
  codesign --verify --strict --verbose=2 "$dir/ps5-dump-forge"
  codesign -dv "$dir/PS5 Dump Forge.app" 2>&1 | grep -E '^(Identifier|Format|Signature)='
  # Read the bundle's actual executable name instead of hardcoding it, so a mismatched
  # mainBinaryName/productName can't silently pick up a stale binary.
  local exe
  exe=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$dir/PS5 Dump Forge.app/Contents/Info.plist")
  for bin in "$dir/PS5 Dump Forge.app/Contents/MacOS/$exe" "$dir/ps5-dump-forge"; do
    [[ $(lipo -archs "$bin") == arm64 ]] || { echo "release-macos.sh: $bin is not arm64-only" >&2; exit 1; }
  done
  for f in README.md LICENSE THIRD-PARTY-NOTICES.md; do
    [[ -s "$dir/$f" ]] || { echo "release-macos.sh: $f missing" >&2; exit 1; }
  done
}
check "$pkg"

mkdir -p "$dist"
zip=$dist/$name.zip
src=$dist/ps5-dump-forge-$version-source.tar.gz
rm -f "$zip" "$src" "$dist/SHA256SUMS"
# ditto keeps the bundle's symlinks, permissions and signature intact (zip(1) can break them).
# No extended attributes or resource forks: they would land in the zip as `._*` files, and an
# ad-hoc signature does not need them.
(cd "$stage" && ditto -c -k --norsrc --noextattr --noacl --keepParent "$name" "$zip")

# What a user gets: unpack the zip again and check it the same way.
mkdir "$stage/unzipped"
ditto -x -k "$zip" "$stage/unzipped"
check "$stage/unzipped/$name"

[[ -z $(git status --porcelain) ]] ||
  echo "release-macos.sh: warning: uncommitted changes; the source tarball holds HEAD only" >&2
git archive --format=tar.gz --prefix="ps5-dump-forge-$version/" -o "$src" HEAD

(cd "$dist" && shasum -a 256 "$(basename "$zip")" "$(basename "$src")" > SHA256SUMS)
echo "== $dist"
ls -l "$zip" "$src"
cat "$dist/SHA256SUMS"
