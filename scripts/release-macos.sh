#!/usr/bin/env bash
# Build and package the macOS release:
#
#   dist/ps5-dump-forge-<ver>-macos-universal.zip  one folder: PS5 Dump Forge.app, ps5-dump-forge (CLI),
#                                                   README.md, LICENSE, THIRD-PARTY-NOTICES.md; the app and
#                                                   the CLI are arm64+x86_64 universal binaries (lipo-joined
#                                                   for the CLI, `tauri build --target
#                                                   universal-apple-darwin` for the app)
#   dist/ps5-dump-forge-<ver>-source.tar.gz        `git archive HEAD` (GPL: the source ships with it)
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

arm=aarch64-apple-darwin
intel=x86_64-apple-darwin
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n1)
[[ -n $version ]] || { echo "release-macos.sh: no version in Cargo.toml" >&2; exit 1; }
target_dir=${CARGO_TARGET_DIR:-$root/target}
[[ $target_dir == /* ]] || target_dir=$root/$target_dir
export CARGO_TARGET_DIR=$target_dir
dist=${DIST_DIR:-$root/dist}

if [[ ${SKIP_BUILD:-} != 1 ]]; then
  echo "== PS5 Dump Forge.app (universal)"
  # npm ci only into a fresh checkout: it deletes node_modules, which a running dev server uses.
  [[ -d app/node_modules ]] || (cd app && npm ci)
  (cd app && npx tauri build --ci --target universal-apple-darwin --bundles app)
  echo "== CLI ($arm, $intel)"
  cargo build --release --locked -p ps5-dump-forge-cli --target "$arm"
  cargo build --release --locked -p ps5-dump-forge-cli --target "$intel"
fi

app="$target_dir/universal-apple-darwin/release/bundle/macos/PS5 Dump Forge.app"
arm_cli=$target_dir/$arm/release/ps5-dump-forge
intel_cli=$target_dir/$intel/release/ps5-dump-forge
for f in "$app" "$arm_cli" "$intel_cli"; do
  [[ -e "$f" ]] || { echo "release-macos.sh: $f is missing (build first)" >&2; exit 1; }
done

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

cli=$stage/ps5-dump-forge-universal
lipo -create -output "$cli" "$arm_cli" "$intel_cli"

# minos_at_most_11 <bin> <arch>: the slice's minimum OS must be <= 11.0 (tauri.conf.json's
# bundle.macOS.minimumSystemVersion), on top of whatever Rust itself defaults to. The CLI's
# x86_64 slice gets the older LC_VERSION_MIN_MACOSX (its `version` field) instead of
# LC_BUILD_VERSION (`minos`) because its implied deployment target is below 10.14.
minos_at_most_11() {
  local bin=$1 a=$2 ver major minor
  ver=$(vtool -arch "$a" -show-build "$bin" | awk '
    /cmd LC_BUILD_VERSION/     { mode = "build" }
    /cmd LC_VERSION_MIN_MACOSX/ { mode = "min" }
    mode == "build" && /^ *minos /   { print $2; exit }
    mode == "min"   && /^ *version / { print $2; exit }
  ')
  [[ -n $ver ]] || { echo "release-macos.sh: $bin ($a): no minimum OS found" >&2; exit 1; }
  major=${ver%%.*} minor=${ver#*.}
  (( major < 11 || (major == 11 && minor == 0) )) ||
    { echo "release-macos.sh: $bin ($a): minimum OS $ver exceeds 11.0" >&2; exit 1; }
}

# run_cli_slices <cli_bin>: exercise each slice with a real `inspect`, through `arch`, on a tiny
# generated fixture folder. x86_64 only skips (with a note) when Rosetta is not installed.
run_cli_slices() {
  local cli_bin=$1 a fixture
  fixture=$(mktemp -d "$stage/fixture.XXXXXX")
  mkdir "$fixture/sce_sys"
  : > "$fixture/eboot.bin"
  echo '{}' > "$fixture/sce_sys/param.json"
  for a in arm64 x86_64; do
    if [[ $a == x86_64 ]] && ! arch -x86_64 /usr/bin/true &>/dev/null; then
      echo "release-macos.sh: no Rosetta, skipping the x86_64 slice run" >&2
      continue
    fi
    echo "== exec $cli_bin ($a)"
    arch "-$a" "$cli_bin" inspect "$fixture" > /dev/null
  done
}

# check <dir>: codesign + arch checks for the packaged app and CLI, every slice's minimum OS and a
# real run of each slice (see above).
check() {
  local dir=$1
  local expected=(arm64 x86_64)
  echo "== verify $dir"
  codesign --verify --deep --strict --verbose=2 "$dir/PS5 Dump Forge.app"
  codesign --verify --strict --verbose=2 "$dir/ps5-dump-forge"
  codesign -dv "$dir/PS5 Dump Forge.app" 2>&1 | grep -E '^(Identifier|Format|Signature)='
  # Read the bundle's actual executable name instead of hardcoding it, so a mismatched
  # mainBinaryName/productName can't silently pick up a stale binary.
  local exe
  exe=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$dir/PS5 Dump Forge.app/Contents/Info.plist")
  local app_bin="$dir/PS5 Dump Forge.app/Contents/MacOS/$exe"
  local cli_bin="$dir/ps5-dump-forge"
  local want
  want=$(printf '%s\n' "${expected[@]}" | sort | tr '\n' ' ')
  local bin
  for bin in "$app_bin" "$cli_bin"; do
    local actual
    actual=$(lipo -archs "$bin" | tr ' ' '\n' | sort | tr '\n' ' ')
    [[ $actual == "$want" ]] ||
      { echo "release-macos.sh: $bin has archs [$actual], want [$want]" >&2; exit 1; }
  done
  local a
  for bin in "$app_bin" "$cli_bin"; do
    for a in "${expected[@]}"; do
      minos_at_most_11 "$bin" "$a"
    done
  done
  run_cli_slices "$cli_bin"
  for f in README.md LICENSE THIRD-PARTY-NOTICES.md; do
    [[ -s "$dir/$f" ]] || { echo "release-macos.sh: $f missing" >&2; exit 1; }
  done
}

# Stage the folder, sign the CLI, verify it, zip it, then unzip and verify again (what a user
# actually gets).
name=ps5-dump-forge-$version-macos-universal
src=$dist/ps5-dump-forge-$version-source.tar.gz
zip=$dist/$name.zip

mkdir -p "$dist"
rm -f "$dist"/ps5-dump-forge-*-macos-*.zip "$src" "$dist/SHA256SUMS"

pkg=$stage/$name
mkdir "$pkg"
ditto "$app" "$pkg/PS5 Dump Forge.app"
cp "$cli" "$pkg/ps5-dump-forge"
codesign --force -s - "$pkg/ps5-dump-forge"
cp README.md LICENSE THIRD-PARTY-NOTICES.md "$pkg/"
check "$pkg"

# ditto keeps the bundle's symlinks, permissions and signature intact (zip(1) can break them).
# No extended attributes or resource forks: they would land in the zip as `._*` files, and an
# ad-hoc signature does not need them.
(cd "$stage" && ditto -c -k --norsrc --noextattr --noacl --keepParent "$name" "$zip")

mkdir "$stage/unzipped"
ditto -x -k "$zip" "$stage/unzipped"
check "$stage/unzipped/$name"

[[ -z $(git status --porcelain) ]] ||
  echo "release-macos.sh: warning: uncommitted changes; the source tarball holds HEAD only" >&2
git archive --format=tar.gz --prefix="ps5-dump-forge-$version/" -o "$src" HEAD

(cd "$dist" && shasum -a 256 "$name.zip" "$(basename "$src")" > SHA256SUMS)
echo "== $dist"
ls -l "$zip" "$src"
cat "$dist/SHA256SUMS"
