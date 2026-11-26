#!/usr/bin/env bash
# Build the Linux GUI release (e.g. on GitHub's ubuntu-22.04 and ubuntu-22.04-arm):
#
#   dist/ps5-dump-forge-<ver>-linux-<arch>.AppImage  the AppImage Tauri builds, as it is (one file;
#                                                    the static CLI is release-linux-musl.sh's)
#
# The app keeps its WebView data in data/ next to the .AppImage. Needs the build host's glibc or
# newer (ubuntu-22.04: 2.35). Unsigned; built for the host's arch (`uname -m`).
# tauri.linux.conf.json names the binary ps5-dump-forge-gui: the bundler would copy "PS5 Dump
# Forge" over target/release/ps5-dump-forge, the CLI's.
# The check extracts the AppImage and, with xvfb-run, runs it 10 s from another directory.
#
#   scripts/release-linux.sh                 # build, then check and copy into dist/
#   SKIP_BUILD=1 scripts/release-linux.sh    # check and copy what is already built
#
# Env: CARGO_TARGET_DIR (default <repo>/target; made absolute, because `tauri build` resolves a
# relative one from app/src-tauri), DIST_DIR (default <repo>/dist).
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
[[ $(uname -s) == Linux ]] || { echo "release-linux.sh: needs Linux" >&2; exit 1; }

case $(uname -m) in
  x86_64) arch=x86-64 elf_machine=3e00 ;;
  aarch64) arch=arm64 elf_machine=b700 ;;
  *) echo "release-linux.sh: unsupported machine $(uname -m)" >&2; exit 1 ;;
esac
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n1)
[[ -n $version ]] || { echo "release-linux.sh: no version in Cargo.toml" >&2; exit 1; }
target_dir=${CARGO_TARGET_DIR:-$root/target}
[[ $target_dir == /* ]] || target_dir=$root/$target_dir
export CARGO_TARGET_DIR=$target_dir
dist=${DIST_DIR:-$root/dist}

if [[ ${SKIP_BUILD:-} != 1 ]]; then
  echo "== PS5 Dump Forge AppImage"
  # npm ci only into a fresh checkout: it deletes node_modules, which a running dev server uses.
  [[ -d app/node_modules ]] || (cd app && npm ci)
  (cd app && npx tauri build --ci --bundles appimage)
fi

shopt -s nullglob
appimages=("$target_dir"/release/bundle/appimage/*_"$version"_*.AppImage)
shopt -u nullglob
[[ ${#appimages[@]} == 1 ]] || {
  echo "release-linux.sh: want one $version AppImage in $target_dir/release/bundle/appimage," \
    "found ${#appimages[@]} (build first)" >&2
  exit 1
}

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

# elf_native <file>: an ELF whose e_machine (offset 18, little-endian) is this host's.
elf_native() {
  [[ $(od -An -tx1 -N4 "$1" | tr -d ' \n') == 7f454c46 ]] &&
    [[ $(od -An -tx1 -j18 -N2 "$1" | tr -d ' \n') == "$elf_machine" ]]
}

name=ps5-dump-forge-$version-linux-$arch.AppImage
run=$stage/run
mkdir "$run" "$stage/cwd"
image=$run/$name
cp "${appimages[0]}" "$image"
chmod 755 "$image"

echo "== verify $image"
elf_native "$image" || { echo "release-linux.sh: $image is not a $arch ELF" >&2; exit 1; }
# --appimage-extract writes squashfs-root/ into the working directory.
(cd "$stage" && "$image" --appimage-extract > /dev/null)
appdir=$stage/squashfs-root
[[ -f $appdir/AppRun && -x $appdir/AppRun ]] ||
  { echo "release-linux.sh: $name has no executable AppRun" >&2; exit 1; }
# The app binary AppRun starts: the .desktop file's Exec (ps5-dump-forge-gui, see above).
exe=$(sed -n 's/^Exec=\([^ ]*\).*/\1/p' "$appdir"/*.desktop | head -n1)
[[ -n $exe ]] || { echo "release-linux.sh: no Exec= in $name's .desktop file" >&2; exit 1; }
elf_native "$appdir/usr/bin/$exe" || { echo "release-linux.sh: usr/bin/$exe is not a $arch ELF" >&2; exit 1; }

# The GUI from another working directory; it must still be running after 10 s (timeout's 124).
if command -v xvfb-run > /dev/null; then
  echo "== launch $name (10 s, Xvfb, --appimage-extract-and-run)"
  rc=0
  (cd "$stage/cwd" && xvfb-run -a timeout 10 "$image" --appimage-extract-and-run) || rc=$?
  [[ $rc == 124 ]] || { echo "release-linux.sh: the GUI exited early (status $rc)" >&2; exit 1; }
  [[ -d $run/data/webview ]] ||
    { echo "release-linux.sh: no WebView data in data/webview next to the .AppImage" >&2; exit 1; }
else
  echo "release-linux.sh: no xvfb-run, skipping the GUI launch" >&2
fi

mkdir -p "$dist"
rm -f "$dist"/ps5-dump-forge-*-linux-*.AppImage
cp "$image" "$dist/$name"
cmp "$image" "$dist/$name"

# No SHA256SUMS here: the release workflow writes one over every platform's assets.
ls -l "$dist/$name"
sha256sum "$dist/$name"
