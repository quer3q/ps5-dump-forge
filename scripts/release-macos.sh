#!/usr/bin/env bash
# Build and package the macOS release, one set per architecture (arm64, x86-64):
#
#   dist/ps5-dump-forge-<ver>-macos-<arch>.dmg   PS5 Dump Forge.app + an Applications link (no CLI inside)
#   dist/ps5-dump-forge-cli-<ver>-macos-<arch>   the bare CLI
#   dist/SHA256SUMS                              of the four files above
#
# Ad-hoc signed only (Tauri's `signingIdentity: "-"`, the CLI `codesign -s -`), no notarization.
# Every CLI embeds the web UI (FORGE_REQUIRE_WEB=1). Each dmg is checked attached read-only.
#
#   scripts/release-macos.sh                 # build everything, then package
#   SKIP_BUILD=1 scripts/release-macos.sh    # package what is already built
#
# Env: CARGO_TARGET_DIR (default <repo>/target; made absolute, because `tauri build` resolves a
# relative one from app/src-tauri), DIST_DIR (default <repo>/dist).
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
[[ $(uname -s) == Darwin ]] || { echo "release-macos.sh: needs macOS (codesign, hdiutil)" >&2; exit 1; }

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n1)
[[ -n $version ]] || { echo "release-macos.sh: no version in Cargo.toml" >&2; exit 1; }
target_dir=${CARGO_TARGET_DIR:-$root/target}
[[ $target_dir == /* ]] || target_dir=$root/$target_dir
export CARGO_TARGET_DIR=$target_dir
dist=${DIST_DIR:-$root/dist}
# <lipo arch>:<rust target>:<asset arch>
arches=(arm64:aarch64-apple-darwin:arm64 x86_64:x86_64-apple-darwin:x86-64)

if [[ ${SKIP_BUILD:-} != 1 ]]; then
  # npm ci only into a fresh checkout: it deletes node_modules, which a running dev server uses.
  [[ -d app/node_modules ]] || (cd app && npm ci)
  echo "== web UI (app/dist-http)"
  (cd app && npm run build:http)
  for spec in "${arches[@]}"; do
    IFS=: read -r _ triple _ <<< "$spec"
    echo "== PS5 Dump Forge.app ($triple)"
    (cd app && npx tauri build --ci --target "$triple" --bundles app)
    echo "== CLI ($triple)"
    FORGE_REQUIRE_WEB=1 cargo build --release --locked -p ps5-dump-forge-cli --target "$triple"
  done
fi

stage=$(mktemp -d)
mounted=()
cleanup() {
  local m
  for m in ${mounted[@]+"${mounted[@]}"}; do hdiutil detach -force "$m" &>/dev/null || true; done
  rm -rf "$stage"
}
trap cleanup EXIT

die() { echo "release-macos.sh: $*" >&2; exit 1; }

# minos_at_most_11 <bin>: tauri.conf.json's minimumSystemVersion. The x86_64 CLI carries
# LC_VERSION_MIN_MACOSX (`version`), not LC_BUILD_VERSION (`minos`): its target is below 10.14.
minos_at_most_11() {
  local bin=$1 ver major minor
  ver=$(vtool -show-build "$bin" | awk '
    /cmd LC_BUILD_VERSION/     { mode = "build" }
    /cmd LC_VERSION_MIN_MACOSX/ { mode = "min" }
    mode == "build" && /^ *minos /   { print $2; exit }
    mode == "min"   && /^ *version / { print $2; exit }
  ')
  [[ -n $ver ]] || die "$bin: no minimum OS found"
  major=${ver%%.*} minor=${ver#*.}
  (( major < 11 || (major == 11 && minor == 0) )) || die "$bin: minimum OS $ver exceeds 11.0"
}

# check_cli <bin> <arch>: signed, one arch, min OS, the web UI, `inspect` runs (x86_64: Rosetta).
check_cli() {
  local bin=$1 a=$2 archs fixture
  codesign --verify --strict --verbose=2 "$bin"
  archs=$(lipo -archs "$bin")
  [[ $archs == "$a" ]] || die "$bin has archs [$archs], want [$a]"
  minos_at_most_11 "$bin"
  grep -qa forge.appcache "$bin" || die "$bin: no embedded web UI (forge.appcache not found)"
  if [[ $a == x86_64 ]] && ! arch -x86_64 /usr/bin/true &>/dev/null; then
    echo "release-macos.sh: no Rosetta, skipping the x86_64 run of $bin" >&2
    return
  fi
  fixture=$(mktemp -d "$stage/fixture.XXXXXX")
  mkdir "$fixture/sce_sys"
  : > "$fixture/eboot.bin"
  echo '{}' > "$fixture/sce_sys/param.json"
  echo "== exec $bin ($a)"
  arch "-$a" "$bin" inspect "$fixture" > /dev/null
}

# check_app <app> <arch>: the bundle's signature and its one executable (the CLI ships apart).
check_app() {
  local app=$1 a=$2 exe archs
  echo "== verify $app"
  codesign --verify --deep --strict --verbose=2 "$app"
  codesign -dv "$app" 2>&1 | grep -E '^(Identifier|Format|Signature)='
  # Read the bundle's actual executable name, so a mismatched mainBinaryName can't pick up a stale one.
  exe=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$app/Contents/Info.plist")
  archs=$(lipo -archs "$app/Contents/MacOS/$exe")
  [[ $archs == "$a" ]] || die "$app/Contents/MacOS/$exe has archs [$archs], want [$a]"
  minos_at_most_11 "$app/Contents/MacOS/$exe"
  [[ $(ls "$app/Contents/MacOS") == "$exe" ]] || die "$app/Contents/MacOS holds more than $exe"
}

# make_dmg <src folder> <dmg>: hdiutil sometimes fails with "resource busy" on CI; retry that.
make_dmg() {
  local src=$1 dmg=$2 try out
  for try in 1 2 3 4 5; do
    if out=$(hdiutil create -format ULMO -fs APFS -volname 'PS5 Dump Forge' -srcfolder "$src" -ov "$dmg" 2>&1); then
      return
    fi
    [[ $out == *"esource busy"* ]] || die "hdiutil create $dmg: $out"
    echo "release-macos.sh: hdiutil busy (try $try), retrying" >&2
    sleep $((try * 2))
  done
  die "hdiutil create $dmg: still busy: $out"
}

mkdir -p "$dist"
for spec in "${arches[@]}"; do
  rm -f "$dist/ps5-dump-forge-$version-macos-${spec##*:}.dmg" "$dist/ps5-dump-forge-cli-$version-macos-${spec##*:}"
done
rm -f "$dist/SHA256SUMS"
assets=()

for spec in "${arches[@]}"; do
  IFS=: read -r a triple asset_arch <<< "$spec"
  app="$target_dir/$triple/release/bundle/macos/PS5 Dump Forge.app"
  cli=$target_dir/$triple/release/ps5-dump-forge
  for f in "$app" "$cli"; do [[ -e $f ]] || die "$f is missing (build first)"; done

  bare=ps5-dump-forge-cli-$version-macos-$asset_arch
  cp "$cli" "$dist/$bare"
  chmod 0755 "$dist/$bare"
  codesign --force -s - "$dist/$bare"
  echo "== verify $dist/$bare"
  check_cli "$dist/$bare" "$a"

  # The .app as tauri built and signed it.
  src=$stage/dmg-$a
  mkdir "$src"
  ditto "$app" "$src/PS5 Dump Forge.app"
  check_app "$src/PS5 Dump Forge.app" "$a"
  ln -s /Applications "$src/Applications"

  dmg=ps5-dump-forge-$version-macos-$asset_arch.dmg
  make_dmg "$src" "$dist/$dmg"

  # What a user gets: attach read-only, check the listing and the app on the mounted copy.
  mnt=$stage/mnt-$a
  mkdir "$mnt"
  hdiutil attach -readonly -nobrowse -noautoopen -mountpoint "$mnt" "$dist/$dmg" > /dev/null
  mounted+=("$mnt")
  held=$(cd "$mnt" && find . -mindepth 1 -maxdepth 1 | sed 's|^\./||' | LC_ALL=C sort | tr '\n' '|')
  [[ $held == "Applications|PS5 Dump Forge.app|" ]] ||
    die "$dmg holds [$held], want the app and the Applications link only"
  [[ -L $mnt/Applications && $(readlink "$mnt/Applications") == /Applications ]] ||
    die "$dmg: Applications is not a link to /Applications"
  check_app "$mnt/PS5 Dump Forge.app" "$a"
  hdiutil detach "$mnt" > /dev/null
  mounted=()

  assets+=("$dmg" "$bare")
done

(cd "$dist" && shasum -a 256 "${assets[@]}" > SHA256SUMS)
echo "== $dist"
(cd "$dist" && ls -l "${assets[@]}")
cat "$dist/SHA256SUMS"
