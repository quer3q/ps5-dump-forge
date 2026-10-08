#!/usr/bin/env bash
# Build and package the Windows releases (Git Bash, e.g. on GitHub's windows-latest, windows-11-arm):
#
#   dist/ps5-dump-forge-<ver>-windows-<arch>.zip      one folder: PS5 Dump Forge.exe, ps5-dump-forge.exe
#                                                    (CLI), README.md, LICENSE, THIRD-PARTY-NOTICES.md;
#                                                    uses the installed WebView2 Runtime (Evergreen)
#   dist/ps5-dump-forge-<ver>-windows-x86-64-webview2.zip
#                                                    x86-64 only: the same plus WebView2/, Microsoft's
#                                                    Fixed Version runtime (x86-64), which the app
#                                                    picks up when present
#
# <arch> is x86-64 or arm64: every build is for its host, the one in `rustc -vV` (not `uname -m`,
# which an emulated x86-64 Git Bash on an arm64 machine gets wrong), so even SKIP_BUILD=1 needs
# rustc. arm64 gets no webview2 zip: Windows 11 ships the runtime.
#
# One exe for both x86-64 zips (app/src-tauri/src/main.rs looks for WebView2/ next to it). Unsigned.
# Both exes link the VC runtime statically via `+crt-static` (tauri-build's own static VC runtime only
# covers x86/x86-64) and use the UCRT that Windows 10/11 ship, so no VC++ redistributable is needed.
#
# The Fixed Version .cab is pinned by URL and SHA-256. Microsoft only serves its latest builds, so
# when the pinned link stops working, take the current stable x86-64 one from
# https://developer.microsoft.com/microsoft-edge/webview2/ (Fixed Version, "x64") and update all
# three values below together. Each webview2 zip is the archived copy of the runtime it shipped.
#
#   scripts/release-windows.sh                 # build everything, then package
#   SKIP_BUILD=1 scripts/release-windows.sh    # package what is already built
#
# Env: CARGO_TARGET_DIR (default <repo>/target; made absolute, because `tauri build` resolves a
# relative one from app/src-tauri), DIST_DIR (default <repo>/dist); x86-64 only: WEBVIEW2_VERSION +
# WEBVIEW2_CAB_URL + WEBVIEW2_CAB_SHA256 (override the pin), WEBVIEW2_CAB (a local copy of the .cab;
# still hash-checked; default: downloaded once into <target>/webview2/).
set -euo pipefail

case $(uname -s) in
  MINGW* | MSYS*) ;;
  *) echo "release-windows.sh: needs Git Bash on Windows (7z; expand.exe on x86-64)" >&2; exit 1 ;;
esac

wv_version=${WEBVIEW2_VERSION:-154.0.4258.62}
wv_url=${WEBVIEW2_CAB_URL:-https://msedge.sf.dl.delivery.mp.microsoft.com/filestreamingservice/files/b92cd7d9-6976-4f34-9708-47e80937c287/Microsoft.WebView2.FixedVersionRuntime.154.0.4258.62.x64.cab}
wv_sha256=${WEBVIEW2_CAB_SHA256:-e8f55a4bde27c7f82512402b56a58539b5ec8928be4e500e077b6f66c9ef4668}
wv_top=Microsoft.WebView2.FixedVersionRuntime.$wv_version.x64

# Mixed paths (C:/a/b) work for bash and for the native tools alike.
root=$(cygpath -am "$(cd "$(dirname "$0")/.." && pwd)")
cd "$root"

# The host triple picks the label and the COFF machine every packaged exe must have (little-endian
# bytes, as pe_machine reads them).
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
  echo "== CLI ($triple)"
  cargo build --release --locked -p ps5-dump-forge-cli --target "$triple"
fi

app_exe="$target_dir/$triple/release/PS5 Dump Forge.exe"
cli_exe=$target_dir/$triple/release/ps5-dump-forge.exe
for f in "$app_exe" "$cli_exe"; do
  [[ -f "$f" ]] || { echo "release-windows.sh: $f is missing (build first)" >&2; exit 1; }
done

# pe_machine <file> <machine>: an MZ/PE image whose COFF machine is <machine>, as little-endian hex
# bytes (6486: x86-64, 64aa: arm64).
pe_machine() {
  local off
  [[ $(od -An -tx1 -N2 "$1" | tr -d ' \n') == 4d5a ]] || return 1
  off=$(od -An -tu4 -j60 -N4 "$1" | tr -d ' \n')
  [[ $(od -An -tx1 -j"$off" -N6 "$1" | tr -d ' \n') == "50450000$2" ]]
}

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

if [[ $arch == x86-64 ]]; then
  # The pinned .cab, downloaded once, checked every time.
  cab=${WEBVIEW2_CAB:-$target_dir/webview2/$wv_top.cab}
  if [[ ! -f $cab ]]; then
    echo "== download $wv_url"
    mkdir -p "$(dirname "$cab")"
    curl -fL --retry 3 -o "$cab.part" "$wv_url"
    mv "$cab.part" "$cab"
  fi
  actual=$(sha256sum "$cab" | cut -d' ' -f1)
  [[ $actual == "$wv_sha256" ]] ||
    { echo "release-windows.sh: $cab has SHA-256 $actual, want $wv_sha256" >&2; exit 1; }

  # Expand as Microsoft documents (`expand <cab> -F:* <dir>`; System32's, not coreutils' expand),
  # then check it is the pinned x86-64 runtime, whole: one top folder named after the version, an
  # x86-64 msedgewebview2.exe, and the full tree (about 250 files) rather than a stub.
  echo "== expand $wv_top"
  mkdir "$stage/cab"
  MSYS_NO_PATHCONV=1 "$(cygpath -S)/expand.exe" "$(cygpath -w "$cab")" '-F:*' "$(cygpath -w "$stage/cab")" > /dev/null
  top=$(ls -A "$stage/cab")
  [[ $top == "$wv_top" ]] ||
    { echo "release-windows.sh: the .cab holds [$top], want [$wv_top]" >&2; exit 1; }
  runtime=$stage/cab/$wv_top
  pe_machine "$runtime/msedgewebview2.exe" 6486 ||
    { echo "release-windows.sh: $runtime/msedgewebview2.exe is not an x86-64 PE" >&2; exit 1; }
  files=$(find "$runtime" -type f | wc -l)
  (( files >= 100 )) || { echo "release-windows.sh: only $files files in $wv_top" >&2; exit 1; }
fi

# A tiny game folder for a real `inspect` run of the packaged CLI.
fixture=$stage/fixture
mkdir -p "$fixture/sce_sys"
: > "$fixture/eboot.bin"
echo '{}' > "$fixture/sce_sys/param.json"

# check <dir> <variant>: what a zip's folder must hold; the CLI runs from it.
check() {
  local dir=$1 variant=$2 f
  echo "== verify $dir ($variant)"
  for f in "PS5 Dump Forge.exe" ps5-dump-forge.exe; do
    pe_machine "$dir/$f" "$machine" ||
      { echo "release-windows.sh: $dir/$f is not an $arch PE" >&2; exit 1; }
    # Statically linked C runtime: no import of the VC++ redistributable's DLL.
    ! grep -aqi 'vcruntime140' "$dir/$f" ||
      { echo "release-windows.sh: $dir/$f imports the VC++ runtime" >&2; exit 1; }
  done
  for f in README.md LICENSE THIRD-PARTY-NOTICES.md; do
    [[ -s "$dir/$f" ]] || { echo "release-windows.sh: $f missing" >&2; exit 1; }
  done
  if [[ $variant == webview2 ]]; then
    pe_machine "$dir/WebView2/msedgewebview2.exe" 6486 ||
      { echo "release-windows.sh: $dir/WebView2/msedgewebview2.exe missing or not x86-64" >&2; exit 1; }
    [[ $(find "$dir/WebView2" -type f | wc -l) == "$files" ]] ||
      { echo "release-windows.sh: $dir/WebView2 is incomplete" >&2; exit 1; }
  else
    [[ ! -e "$dir/WebView2" ]] || { echo "release-windows.sh: $dir has a WebView2 folder" >&2; exit 1; }
  fi
  "$dir/ps5-dump-forge.exe" inspect "$(cygpath -w "$fixture")" > /dev/null
}

# package_and_check <name> <variant>: stage the folder, check it, zip it, then unzip it and check
# again (what a user actually gets): the zip holds that one folder and nothing beside it.
package_and_check() {
  local name=$1 variant=$2
  local pkg=$stage/$name zip=$dist/$name.zip
  mkdir "$pkg"
  cp "$app_exe" "$pkg/PS5 Dump Forge.exe"
  cp "$cli_exe" "$pkg/ps5-dump-forge.exe"
  cp README.md LICENSE THIRD-PARTY-NOTICES.md "$pkg/"
  if [[ $variant == webview2 ]]; then
    cp -R "$runtime" "$pkg/WebView2"
  fi
  check "$pkg" "$variant"

  # 7z: on both runners' images (windows-latest, windows-11-arm); missing, it fails here under set -e.
  (cd "$stage" && 7z a -tzip -bd "$(cygpath -w "$zip")" "$name" > /dev/null)

  local out=$stage/unzipped-$name top
  mkdir "$out"
  7z x -bd "-o$(cygpath -w "$out")" "$(cygpath -w "$zip")" > /dev/null
  top=$(ls -A "$out")
  [[ $top == "$name" ]] || { echo "release-windows.sh: $zip holds [$top], want [$name]" >&2; exit 1; }
  check "$out/$name" "$variant"
}

name=ps5-dump-forge-$version-windows-$arch

# Every earlier Windows zip goes, so a reused dist/ keeps none under an old name.
mkdir -p "$dist"
rm -f "$dist"/ps5-dump-forge-*-windows-*.zip

package_and_check "$name" plain
zips=("$dist/$name.zip")
if [[ $arch == x86-64 ]]; then
  package_and_check "$name-webview2" webview2
  zips+=("$dist/$name-webview2.zip")
fi

# No SHA256SUMS here: the release workflow writes one over every platform's archives.
echo "== $dist"
ls -l "${zips[@]}"
sha256sum "${zips[@]}"
