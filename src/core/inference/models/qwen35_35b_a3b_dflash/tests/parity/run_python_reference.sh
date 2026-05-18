#!/usr/bin/env bash
# Byte-for-byte parity harness — step 1.
#
# Runs the vendored dflash-mlx Python reference for Qwen3.5-35B-A3B and
# writes prompt + generated token IDs as flat int32 little-endian bytes.
#
# Usage:
#   tests/parity/run_python_reference.sh <prompt.txt> <n_gen> <out.bin>
#
# Required environment:
#   CTOX_DFLASH_MLX_PYTHON  Python with the vendored dflash-mlx installed.
#                           Defaults to /tmp/ctox-dflash-mlx-ref-venv/bin/python.
#   CTOX_QWEN35_TARGET_DIR  Complete mlx-community/Qwen3.5-35B-A3B-4bit dir.
#   CTOX_QWEN35_DRAFT_DIR   Complete z-lab/Qwen3.5-35B-A3B-DFlash dir.

set -euo pipefail

if [[ "${1:-}" == "" || "${2:-}" == "" || "${3:-}" == "" ]]; then
    echo "usage: $0 <prompt.txt> <n_gen> <out.bin>" >&2
    exit 2
fi

PROMPT_FILE=$1
N_GEN=$2
OUT_BIN=$3

CRATE_ROOT=$(cd "$(dirname "$0")/../.." && pwd)
VENDOR_SRC="$CRATE_ROOT/vendor/metal/dflash-mlx-ref"

PY_BIN="${CTOX_DFLASH_MLX_PYTHON:-/tmp/ctox-dflash-mlx-ref-venv/bin/python}"
TARGET_DIR="${CTOX_QWEN35_TARGET_DIR:-/Volumes/Models/huggingface/local/Qwen3.5-35B-A3B-4bit}"
DRAFT_DIR="${CTOX_QWEN35_DRAFT_DIR:-/Volumes/Models/huggingface/local/Qwen3.5-35B-A3B-DFlash}"

if [[ ! -x "$PY_BIN" ]]; then
    echo "error: Python reference interpreter not executable: $PY_BIN" >&2
    echo "hint: create one with: uv venv --python 3.12 /tmp/ctox-dflash-mlx-ref-venv && uv pip install --python /tmp/ctox-dflash-mlx-ref-venv/bin/python -e $VENDOR_SRC" >&2
    exit 1
fi
if [[ ! -d "$TARGET_DIR" ]]; then
    echo "error: target dir missing: $TARGET_DIR" >&2
    exit 1
fi
if [[ ! -d "$DRAFT_DIR" ]]; then
    echo "error: draft dir missing: $DRAFT_DIR" >&2
    exit 1
fi

PROMPT=$(cat "$PROMPT_FILE")

PYTHONPATH="$VENDOR_SRC${PYTHONPATH:+:$PYTHONPATH}" "$PY_BIN" - "$PROMPT" "$N_GEN" "$OUT_BIN" "$TARGET_DIR" "$DRAFT_DIR" <<'PYEOF'
import struct
import sys

from dflash_mlx.generate import get_stop_token_ids
from dflash_mlx.runtime import (
    generate_dflash_once,
    load_draft_bundle,
    load_target_bundle,
)

prompt = sys.argv[1]
n_gen = int(sys.argv[2])
out = sys.argv[3]
target_dir = sys.argv[4]
draft_dir = sys.argv[5]

target_model, tokenizer, _ = load_target_bundle(target_dir, lazy=True)
draft_model, _ = load_draft_bundle(draft_dir, lazy=True)

prompt_ids = list(tokenizer.encode(prompt))
summary = generate_dflash_once(
    target_model=target_model,
    tokenizer=tokenizer,
    draft_model=draft_model,
    prompt=prompt,
    max_new_tokens=n_gen,
    use_chat_template=False,
    stop_token_ids=get_stop_token_ids(tokenizer),
    prompt_tokens_override=prompt_ids,
)

ids = prompt_ids + list(summary.get("generated_token_ids", []))
with open(out, "wb") as f:
    for token_id in ids:
        f.write(struct.pack("<i", int(token_id)))

elapsed_us = float(summary.get("elapsed_us", 0.0))
prefill_us = float(summary.get("phase_timings_us", {}).get("prefill", 0.0))
gen_us = max(0.0, elapsed_us - prefill_us)
gen_tps = (len(ids) - len(prompt_ids)) / (gen_us / 1_000_000.0) if gen_us > 0 else 0.0
print(
    f"[parity] wrote {len(ids)} tokens to {out}; "
    f"generated={len(ids) - len(prompt_ids)} decode_tok_s={gen_tps:.6f} "
    f"acceptance={float(summary.get('acceptance_ratio', 0.0)):.6f}"
)
PYEOF

echo "[parity] python reference run finished"
