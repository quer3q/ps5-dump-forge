#!/usr/bin/env bash
# Write the release's catalog for ps5-payload-manager (a custom repository, schema:
# https://github.com/itsPLK/ps5-payload-manager/blob/main/CUSTOM_REPOSITORIES.md): one payload,
# this release's PS5 .elf, with its SHA-256, so the manager can update it.
#
#   scripts/make-payloads-json.sh dist/ps5-dump-forge-<ver>-ps5.elf [dist/payloads.json]
#
# The release uploads the result as `payloads.json`; GitHub serves the latest published release's
# copy at a fixed address, which is the one to add to the manager:
#
#   https://github.com/<owner>/<repo>/releases/latest/download/payloads.json
#
# The version is Cargo.toml's (check-versions.sh makes the other files agree) and the ELF's name
# must be ps5-dump-forge-<version>-ps5.elf, as release-ps5.sh names it. Env: GITHUB_REPOSITORY
# (default quer3q/ps5-dump-forge) names the repository the download URL points into;
# PAYLOADS_DATE (default: today, UTC) is `last_update`.
#
# `filename` is the stable ps5-dump-forge.elf, not the versioned asset name: it is the name the
# manager saves the payload under, so an autoload entry keeps working across updates, while `url`
# names this release's asset. The values are all produced here (digits, hex, a fixed text), so the
# JSON is written with printf and needs no jq; the check below parses it back.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

[[ $# -ge 1 && $# -le 2 ]] || { echo "usage: $0 <ps5 .elf> [output.json]" >&2; exit 2; }
elf=$1
out=${2:-payloads.json}

version=$(tr -d '\r' < Cargo.toml | sed -n 's/^version = "\(.*\)"$/\1/p' | head -n1)
[[ -n $version ]] || { echo "make-payloads-json.sh: no version in Cargo.toml" >&2; exit 1; }
[[ $version =~ ^[0-9A-Za-z.+-]+$ ]] ||
  { echo "make-payloads-json.sh: odd version '$version'" >&2; exit 1; }
[[ -f $elf ]] || { echo "make-payloads-json.sh: $elf is not a file" >&2; exit 1; }
asset=ps5-dump-forge-$version-ps5.elf
[[ $(basename "$elf") == "$asset" ]] ||
  { echo "make-payloads-json.sh: $elf is not named $asset" >&2; exit 1; }

repo=${GITHUB_REPOSITORY:-quer3q/ps5-dump-forge}
[[ $repo =~ ^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$ ]] ||
  { echo "make-payloads-json.sh: odd repository '$repo'" >&2; exit 1; }
date=${PAYLOADS_DATE:-$(date -u +%F)}
[[ $date =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]] ||
  { echo "make-payloads-json.sh: PAYLOADS_DATE '$date' is not YYYY-MM-DD" >&2; exit 1; }
sha=$(sha256sum -- "$elf" | cut -d' ' -f1)
[[ $sha =~ ^[0-9a-f]{64}$ ]] || { echo "make-payloads-json.sh: no SHA-256 for $elf" >&2; exit 1; }

printf '%s\n' \
  '{' \
  '  "name": "PS5 Dump Forge",' \
  '  "payloads": [' \
  '    {' \
  '      "id": "ps5-dump-forge",' \
  '      "name": "PS5 Dump Forge",' \
  '      "filename": "ps5-dump-forge.elf",' \
  "      \"url\": \"https://github.com/$repo/releases/download/v$version/$asset\"," \
  "      \"version\": \"v$version\"," \
  "      \"last_update\": \"$date\"," \
  '      "description": "Converts and edits PS5 game dumps (folders, exfat, ffpkg, ffpfs, ffpfsc, FPKG, archives) and packs them into zstd or LZ4 asset packs. The web UI opens on port 8095.",' \
  '      "category": "Utilities & Tools",' \
  "      \"checksum\": \"$sha\"," \
  '      "author": "quer3q",' \
  "      \"source\": \"https://github.com/$repo\"," \
  "      \"details_url\": \"https://github.com/$repo#readme\"" \
  '    }' \
  '  ]' \
  '}' > "$out"

# Check what was written: it parses, and every field the schema requires is there, the checksum
# is 64 hex characters and the URL names the asset this ELF is.
if command -v jq > /dev/null; then
  jq -e --arg sha "$sha" --arg asset "$asset" '
    (.payloads | length == 1)
    and (.payloads[0] | .name != "" and (.filename | endswith(".elf"))
        and (.url | startswith("https://") and endswith("/" + $asset))
        and .checksum == $sha and (.checksum | test("^[0-9a-f]{64}$")))' "$out" > /dev/null ||
    { echo "make-payloads-json.sh: $out does not check out" >&2; exit 1; }
else
  python3 -I - "$out" "$sha" "$asset" << 'EOF'
import json, re, sys
path, sha, asset = sys.argv[1:]
doc = json.load(open(path, encoding="utf-8"))
(p,) = doc["payloads"]
assert p["name"] and p["filename"].endswith(".elf"), p
assert p["url"].startswith("https://") and p["url"].endswith("/" + asset), p["url"]
assert p["checksum"] == sha and re.fullmatch("[0-9a-f]{64}", sha), p["checksum"]
EOF
fi
echo "wrote $out: $asset, sha256 $sha"
