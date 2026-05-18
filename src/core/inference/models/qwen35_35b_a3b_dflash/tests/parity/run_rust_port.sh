#!/usr/bin/env bash
# Byte-for-byte parity harness — step 2.
#
# Runs the Rust port of dflash-mlx on the same input as
# `run_python_reference.sh` and produces a token-ID binary in the
# identical format.
#
# Usage:
#   tests/parity/run_rust_port.sh <prompt_ids.bin> <n_gen> <out.bin>
#
# `prompt_ids.bin` should be produced by running
# `run_python_reference.sh` first, then truncating its output to
# the prefix before the generated tokens (which is also the input
# ID sequence — the reference encodes the prompt identically).
#
# The caller is expected to have downloaded the target + draft models
# to a local path (e.g. via `huggingface-cli download`) and to set:
#
#   CTOX_QWEN35_TARGET_DIR   path to mlx-community/Qwen3.5-35B-A3B-4bit
#   CTOX_QWEN35_DRAFT_PATH   path to z-lab/Qwen3.5-35B-A3B-DFlash safetensors

set -euo pipefail

if [[ "${1:-}" == "" || "${2:-}" == "" || "${3:-}" == "" ]]; then
    echo "usage: $0 <prompt_ids.bin> <n_gen> <out.bin>" >&2
    exit 2
fi

PROMPT_BIN=$1
N_GEN=$2
OUT_BIN=$3

if [[ -z "${CTOX_QWEN35_TARGET_DIR:-}" ]]; then
    echo "error: CTOX_QWEN35_TARGET_DIR must point at the mlx-community 4-bit export" >&2
    exit 1
fi
if [[ -z "${CTOX_QWEN35_DRAFT_PATH:-}" ]]; then
    echo "error: CTOX_QWEN35_DRAFT_PATH must point at the draft safetensors file" >&2
    exit 1
fi

CRATE_ROOT=$(cd "$(dirname "$0")/../.." && pwd)

cd "$CRATE_ROOT"
cargo build --release --bin qwen35-35b-a3b-dflash-bench-metal

./target/release/qwen35-35b-a3b-dflash-bench-metal \
    "$CTOX_QWEN35_TARGET_DIR" \
    "$CTOX_QWEN35_DRAFT_PATH" \
    "$PROMPT_BIN" \
    "$N_GEN" \
    "$OUT_BIN"

echo "[parity] rust port run finished → $OUT_BIN"
