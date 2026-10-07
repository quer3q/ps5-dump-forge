#!/usr/bin/env bash
# Generate the seed corpora (fuzz/corpus/<target>/) with this project's own writers. Stable
# Rust is enough. Then, for example:
#
#   fuzz/seed.sh
#   cd fuzz && cargo +nightly fuzz run ufs2 -- -max_total_time=60
#
# Findings land in fuzz/artifacts/<target>/; replay one with `cargo +nightly fuzz run <target> <file>`.
set -euo pipefail
cd "$(dirname "$0")"
cargo run --quiet --release -p ps5-dump-forge-fuzz-seedgen -- corpus
