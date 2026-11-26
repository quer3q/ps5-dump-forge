#!/usr/bin/env bash
# Build the Windows releases (Git Bash, e.g. on GitHub's windows-latest, windows-11-arm). Writes into
# dist/ two bare files, no archive, plus their SHA256SUMS:
#
#   dist/ps5-dump-forge-<ver>-windows-<arch>.exe       the app (PS5 Dump Forge.exe); uses the installed
#                                                     WebView2 Runtime (Evergreen)
#   dist/ps5-dump-forge-cli-<ver>-windows-<arch>.exe   the CLI, the web UI embedded for `serve`
#
# <arch> is the host's from `rustc -vV` (an emulated x86-64 Git Bash on arm64 gets `uname -m`
# wrong), so even SKIP_BUILD=1 needs rustc. Unsigned. `+crt-static` (tauri-build's covers only
# x86/x86-64) and Windows' UCRT: no VC++ redistributable needed.
#
#   scripts/release-windows.sh                 # build everything, then check and copy
#   SKIP_BUILD=1 scripts/release-windows.sh    # check and copy what is already built
#
# Env: CARGO_TARGET_DIR (default <repo>/target; made absolute, because `tauri build` resolves a
# relative one from app/src-tauri), DIST_DIR (default <repo>/dist).
set -euo pipefail

case $(uname -s) in
  MINGW* | MSYS*) ;;
  *) echo "release-windows.sh: needs Git Bash on Windows" >&2; exit 1 ;;
esac

# Mixed paths (C:/a/b) work for bash and for the native tools alike.
root=$(cygpath -am "$(cd "$(dirname "$0")/.." && pwd)")
cd "$root"

triple=$(rustc -vV | sed -n 's/^host: //p')
case $triple in
  x86_64-pc-windows-msvc) arch=x86-64 machine=6486 ;;
  aarch64-pc-windows-msvc) arch=arm64 machine=64aa ;;
  *) echo "release-windows.sh: unsupported host triple [$triple]" >&2; exit 1 ;;
esac
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n1)
[[ -n $version ]] || { echo "release-windows.sh: no version in Cargo.toml" >&2; exit 1; }
target_dir=$(cygpath -am "${CARGO_TARGET_DIR:-$root/target}")
export CARGO_TARGET_DIR=$target_dir
dist=$(cygpath -am "${DIST_DIR:-$root/dist}")

if [[ ${SKIP_BUILD:-} != 1 ]]; then
  # Static C runtime for both exes; with --target, RUSTFLAGS skip build scripts and proc macros.
  export RUSTFLAGS="${RUSTFLAGS:-} -C target-feature=+crt-static"
  echo "== PS5 Dump Forge.exe ($triple)"
  # npm ci only into a fresh checkout: it deletes node_modules, which a running dev server uses.
  [[ -d app/node_modules ]] || (cd app && npm ci)
  # --no-bundle: no installer, just the exe (frontend embedded), renamed by mainBinaryName.
  (cd app && npx tauri build --ci --no-bundle --target "$triple")
  echo "== web UI for the CLI's serve (app/dist-http)"
  (cd app && npm run build:http)
  echo "== CLI ($triple)"
  # FORGE_REQUIRE_WEB=1: the server's build.rs fails rather than embed its placeholder page.
  FORGE_REQUIRE_WEB=1 cargo build --release --locked -p ps5-dump-forge-cli --target "$triple"
fi

app_exe="$target_dir/$triple/release/PS5 Dump Forge.exe"
cli_exe=$target_dir/$triple/release/ps5-dump-forge.exe
for f in "$app_exe" "$cli_exe"; do
  [[ -f "$f" ]] || { echo "release-windows.sh: $f is missing (build first)" >&2; exit 1; }
done

# pe_machine <file> <machine>: COFF machine as little-endian hex (6486 x86-64, 64aa arm64).
pe_machine() {
  local off
  [[ $(od -An -tx1 -N2 "$1" | tr -d ' \n') == 4d5a ]] || return 1
  off=$(od -An -tu4 -j60 -N4 "$1" | tr -d ' \n')
  [[ $(od -An -tx1 -j"$off" -N6 "$1" | tr -d ' \n') == "50450000$2" ]]
}

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

# A tiny game folder for a real `inspect` run of the CLI.
fixture=$stage/fixture
mkdir -p "$fixture/sce_sys"
: > "$fixture/eboot.bin"
echo '{}' > "$fixture/sce_sys/param.json"

app_name=ps5-dump-forge-$version-windows-$arch.exe
cli_name=ps5-dump-forge-cli-$version-windows-$arch.exe

# Every earlier Windows asset goes (the old zips too), so a reused dist/ keeps none under an old name.
mkdir -p "$dist"
rm -f "$dist"/ps5-dump-forge-*-windows-*.zip "$dist"/ps5-dump-forge-*-windows-*.exe \
  "$dist"/ps5-dump-forge-cli-*-windows-*.exe "$dist/SHA256SUMS"
cp "$app_exe" "$dist/$app_name"
cp "$cli_exe" "$dist/$cli_name"

for f in "$app_name" "$cli_name"; do
  echo "== verify $f"
  pe_machine "$dist/$f" "$machine" ||
    { echo "release-windows.sh: $f is not an $arch PE" >&2; exit 1; }
  # Statically linked C runtime: no import of the VC++ redistributable's DLL.
  ! grep -aqi 'vcruntime140' "$dist/$f" ||
    { echo "release-windows.sh: $f imports the VC++ runtime" >&2; exit 1; }
done
# The CLI's serve has the real web UI (its AppCache manifest), not the placeholder page.
grep -aq 'forge.appcache' "$dist/$cli_name" ||
  { echo "release-windows.sh: $cli_name has no web UI embedded (forge.appcache)" >&2; exit 1; }
"$dist/$cli_name" inspect "$(cygpath -w "$fixture")" > /dev/null

# Bare names, so `sha256sum -c` works from dist/.
(cd "$dist" && sha256sum "$app_name" "$cli_name" > SHA256SUMS)
echo "== $dist"
ls -l "$dist/$app_name" "$dist/$cli_name"
cat "$dist/SHA256SUMS"
