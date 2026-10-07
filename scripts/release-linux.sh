#!/usr/bin/env bash
# Build and package the Linux x64 release (e.g. on GitHub's ubuntu-22.04):
#
#   dist/ps5-dump-forge-<ver>-linux-x64.tar.gz  one folder: PS5 Dump Forge.AppDir (Tauri's AppImage,
#                                                extracted whole), forge.sh (the launcher), ps5-dump-forge
#                                                (CLI), README.md, LICENSE, THIRD-PARTY-NOTICES.md
#
# An extracted AppDir instead of the AppImage: it needs no FUSE, and the app keeps its WebView data
# in data/ next to forge.sh (core's app_dir: the folder holding the *.AppDir). The AppDir is kept
# exactly as `--appimage-extract` wrote it (AppRun, its hooks, the wrapped binary, symlinks); tar keeps
# the symlinks and modes. The binaries need the build host's glibc or newer (ubuntu-22.04: 2.35).
# Unsigned. The arch label comes from `uname -m`, so an arm64 host builds a linux-arm64 tarball, but
# only x64 ships.
#
# app/src-tauri/tauri.linux.conf.json names the Linux app binary ps5-dump-forge-gui: the AppImage
# bundler copies a main binary whose name has a space ("PS5 Dump Forge") to its kebab-case name,
# target/release/ps5-dump-forge, which is the CLI's (a hard link to cargo's own copy, so it would
# overwrite the CLI there too).
#
# The check after the round trip through the tarball runs the CLI and, when xvfb-run is installed,
# starts the GUI through forge.sh under Xvfb from an unrelated directory and requires it to stay up
# for 10 s.
#
#   scripts/release-linux.sh                 # build everything, then package
#   SKIP_BUILD=1 scripts/release-linux.sh    # package what is already built
#
# Env: CARGO_TARGET_DIR (default <repo>/target; made absolute, because `tauri build` resolves a
# relative one from app/src-tauri), DIST_DIR (default <repo>/dist).
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
[[ $(uname -s) == Linux ]] || { echo "release-linux.sh: needs Linux" >&2; exit 1; }

case $(uname -m) in
  x86_64) arch=x64 elf_machine=3e00 ;;
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
  echo "== CLI"
  cargo build --release --locked -p ps5-dump-forge-cli
fi

shopt -s nullglob
appimages=("$target_dir"/release/bundle/appimage/*_"$version"_*.AppImage)
shopt -u nullglob
[[ ${#appimages[@]} == 1 ]] || {
  echo "release-linux.sh: want one $version AppImage in $target_dir/release/bundle/appimage," \
    "found ${#appimages[@]} (build first)" >&2
  exit 1
}
appimage=${appimages[0]}
cli=$target_dir/release/ps5-dump-forge
[[ -f $cli ]] || { echo "release-linux.sh: $cli is missing (build first)" >&2; exit 1; }

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

# elf_native <file>: an ELF whose e_machine (offset 18, little-endian) is this host's.
elf_native() {
  [[ $(od -An -tx1 -N4 "$1" | tr -d ' \n') == 7f454c46 ]] &&
    [[ $(od -An -tx1 -j18 -N2 "$1" | tr -d ' \n') == "$elf_machine" ]]
}

# tree <dir>: every entry's type, mode, path and symlink target, for comparing two copies.
tree() {
  (cd "$1" && find . -printf '%y %m %p %l\n' | LC_ALL=C sort)
}

# A tiny game folder for a real `inspect` run of the packaged CLI.
fixture=$stage/fixture
mkdir -p "$fixture/sce_sys"
: > "$fixture/eboot.bin"
echo '{}' > "$fixture/sce_sys/param.json"

appdir_name="PS5 Dump Forge.AppDir"

# check <dir>: what the tarball's folder must hold; the CLI runs from it.
check() {
  local dir=$1 f exe
  echo "== verify $dir"
  for f in forge.sh "$appdir_name/AppRun"; do
    [[ -f "$dir/$f" && -x "$dir/$f" ]] || { echo "release-linux.sh: $dir/$f is not executable" >&2; exit 1; }
  done
  # The app binary AppRun starts: the .desktop file's Exec (ps5-dump-forge-gui, see above).
  exe=$(sed -n 's/^Exec=\([^ ]*\).*/\1/p' "$dir/$appdir_name"/*.desktop | head -n1)
  [[ -n $exe ]] || { echo "release-linux.sh: no Exec= in $dir/$appdir_name/*.desktop" >&2; exit 1; }
  for f in ps5-dump-forge "$appdir_name/usr/bin/$exe"; do
    elf_native "$dir/$f" || { echo "release-linux.sh: $dir/$f is not a $arch ELF" >&2; exit 1; }
  done
  # forge.sh's FORGE_HOST_VARS must hold every variable AppRun (its C wrapper's NAME=%s formats and
  # its hooks' exports) sets.
  local set_vars v
  set_vars=$( { LC_ALL=C tr '\0' '\n' < "$dir/$appdir_name/AppRun.wrapped" |
    LC_ALL=C sed -nE 's/^([A-Z][A-Z0-9_]{3,})=(%s.*|1)$/\1/p'
    sed -nE 's/^export ([A-Z][A-Z0-9_]*)=.*/\1/p' "$dir/$appdir_name"/apprun-hooks/*.sh; } | sort -u)
  [[ $set_vars == *LD_LIBRARY_PATH* ]] ||
    { echo "release-linux.sh: found no variables AppRun sets ($set_vars)" >&2; exit 1; }
  for v in $set_vars; do
    sed -n '/^FORGE_HOST_VARS="/,/"$/p' "$dir/forge.sh" | grep -qw -- "$v" ||
      { echo "release-linux.sh: AppRun sets $v, but forge.sh does not keep it for host tools" >&2; exit 1; }
  done
  for f in README.md LICENSE THIRD-PARTY-NOTICES.md; do
    [[ -s "$dir/$f" ]] || { echo "release-linux.sh: $f missing" >&2; exit 1; }
  done
  "$dir/ps5-dump-forge" inspect "$fixture" > /dev/null
}

# run_gui <dir>: start the app through forge.sh under Xvfb, from another working directory; it must
# still be running after 10 s (timeout's 124) and have put its WebView data next to forge.sh.
run_gui() {
  local dir=$1 rc=0
  if ! command -v xvfb-run > /dev/null; then
    echo "release-linux.sh: no xvfb-run, skipping the GUI launch" >&2
    return
  fi
  echo "== launch $dir/forge.sh (10 s, Xvfb)"
  (cd "$fixture" && xvfb-run -a timeout 10 "$dir/forge.sh") || rc=$?
  [[ $rc == 124 ]] || { echo "release-linux.sh: the GUI exited early (status $rc)" >&2; exit 1; }
  [[ -d "$dir/data/webview" ]] ||
    { echo "release-linux.sh: no WebView data in $dir/data/webview" >&2; exit 1; }
}

name=ps5-dump-forge-$version-linux-$arch
tgz=$dist/$name.tar.gz
pkg=$stage/$name

echo "== extract $appimage"
mkdir "$pkg"
# --appimage-extract writes squashfs-root/ into the working directory, every folder 0700; the
# bundler's AppDir has them 0755 (files keep their modes).
(cd "$stage" && "$appimage" --appimage-extract > /dev/null)
mv "$stage/squashfs-root" "$pkg/$appdir_name"
find "$pkg/$appdir_name" -type d -exec chmod 755 {} +
cp "$cli" "$pkg/ps5-dump-forge"
cp README.md LICENSE THIRD-PARTY-NOTICES.md "$pkg/"
cat > "$pkg/forge.sh" <<'EOF'
#!/bin/sh
# Starts PS5 Dump Forge from the AppDir next to this script (its WebView data goes to data/ here).
set -e
here=$(dirname "$(readlink -f "$0")")
# Every variable this script, AppRun and AppRun's GTK hook set, pointed at the AppDir's libraries
# and data. Host tools the app starts (xdg-open for "Show in folder") get these values back: with
# the bundled GLib they fail (tauri-apps/tauri#10617). FORGE_HOST_<name> holds a set one's value.
FORGE_HOST_VARS="WEBKIT_DISABLE_DMABUF_RENDERER LD_PRELOAD \
PATH LD_LIBRARY_PATH PYTHONHOME PYTHONPATH PYTHONDONTWRITEBYTECODE XDG_DATA_DIRS PERLLIB \
GSETTINGS_SCHEMA_DIR QT_PLUGIN_PATH GST_PLUGIN_SYSTEM_PATH GST_PLUGIN_SYSTEM_PATH_1_0 \
APPDIR GTK_DATA_PREFIX GTK_THEME GI_TYPELIB_PATH GIO_MODULE_DIR GTK_EXE_PREFIX GTK_PATH \
GTK_IM_MODULE_FILE GDK_PIXBUF_MODULE_FILE"
for v in $FORGE_HOST_VARS; do
  eval "[ -z \"\${$v+x}\" ] || export FORGE_HOST_$v=\"\$$v\""
done
export FORGE_HOST_VARS
# WebKitGTK's DMA-BUF renderer shows a blank window on many GPU/driver setups.
export WEBKIT_DISABLE_DMABUF_RENDERER="${WEBKIT_DISABLE_DMABUF_RENDERER:-1}"
# On Wayland the bundled libwayland-client can crash against the host's driver stack (NVIDIA,
# Mesa): prefer the host's own x86-64 copy when ldconfig knows one.
if [ -n "${WAYLAND_DISPLAY:-}" ] || [ "${XDG_SESSION_TYPE:-}" = wayland ]; then
  ldconfig=$(command -v ldconfig || echo /sbin/ldconfig)
  lib=$("$ldconfig" -p 2>/dev/null |
    awk '$1 == "libwayland-client.so.0" && /x86-64/ { print $NF; exit }') || lib=
  if [ -n "$lib" ] && [ -e "$lib" ]; then
    export LD_PRELOAD="$lib${LD_PRELOAD:+:$LD_PRELOAD}"
  fi
fi
exec "$here/PS5 Dump Forge.AppDir/AppRun" "$@"
EOF
chmod 755 "$pkg/forge.sh"
check "$pkg"

mkdir -p "$dist"
rm -f "$tgz"
# tar keeps the AppDir's symlinks and modes; owner and group are not the builder's.
tar -czf "$tgz" -C "$stage" --owner=0 --group=0 --numeric-owner "$name"

echo "== $tgz"
listed=$(tar -tzvf "$tgz" | grep -E "^-rwx.* $name/(forge\.sh|ps5-dump-forge|$appdir_name/AppRun)$" || true)
echo "$listed"
[[ $(grep -c . <<< "$listed") == 3 ]] ||
  { echo "release-linux.sh: $tgz does not list forge.sh, the CLI and AppRun as executables" >&2; exit 1; }
out=$stage/untarred
mkdir "$out"
tar -xzf "$tgz" -C "$out"
top=$(ls -A "$out")
[[ $top == "$name" ]] || { echo "release-linux.sh: $tgz holds [$top], want [$name]" >&2; exit 1; }
[[ $(tree "$pkg") == "$(tree "$out/$name")" ]] ||
  { echo "release-linux.sh: $tgz does not restore the staged tree (types, modes, symlinks)" >&2; exit 1; }
check "$out/$name"
run_gui "$out/$name"

# No SHA256SUMS here: the release workflow writes one over every platform's archives.
ls -l "$tgz"
sha256sum "$tgz"
