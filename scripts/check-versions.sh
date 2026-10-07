#!/usr/bin/env bash
# Check that Cargo.toml, app/src-tauri/tauri.conf.json and app/package.json carry the same version,
# and, on a tag build (GITHUB_REF_TYPE=tag), that the tag is v<version>. Errors use GitHub's
# `::error::` annotation; any other run just prints them.
#
#   scripts/check-versions.sh
#
# Every value has its `\r` stripped: on Windows, a CRLF checkout and native jq.exe's CRLF output
# would otherwise make equal versions differ.
set -euo pipefail

cd "$(dirname "$0")/.."

version=$(tr -d '\r' < Cargo.toml | sed -n 's/^version = "\(.*\)"$/\1/p' | head -n1)
tauri=$(jq -r .version app/src-tauri/tauri.conf.json | tr -d '\r')
npm=$(jq -r .version app/package.json | tr -d '\r')
echo "Cargo.toml $version, tauri.conf.json $tauri, package.json $npm"
if [[ -z $version || $tauri != "$version" || $npm != "$version" ]]; then
  echo "::error::versions differ: Cargo.toml $version, tauri.conf.json $tauri, app/package.json $npm"
  exit 1
fi
if [[ ${GITHUB_REF_TYPE:-} == tag && ${GITHUB_REF_NAME:-} != "v$version" ]]; then
  echo "::error::tag ${GITHUB_REF_NAME:-} does not match version $version"
  exit 1
fi
