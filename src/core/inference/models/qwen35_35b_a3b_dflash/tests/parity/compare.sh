#!/usr/bin/env bash
# Byte-for-byte parity harness — step 3.
#
# Trivially `cmp(1)`s the Python reference output against the Rust
# port output. Exits 0 iff the files are byte-identical.
#
# Usage:
#   tests/parity/compare.sh <reference.bin> <port.bin>

set -euo pipefail

if [[ "${1:-}" == "" || "${2:-}" == "" ]]; then
    echo "usage: $0 <reference.bin> <port.bin>" >&2
    exit 2
fi

REF=$1
PORT=$2

if ! cmp --silent "$REF" "$PORT"; then
    echo "[parity] FAIL — outputs differ"
    cmp "$REF" "$PORT" | head -5
    exit 1
fi

echo "[parity] OK — byte-identical ($(wc -c < "$REF") bytes)"
