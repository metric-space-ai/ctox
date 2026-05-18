#!/usr/bin/env bash
# Origin: CTOX
# License: Apache-2.0
#
# Capture the GGML/llama.cpp Q4_K_M Metal **baseline** for
# Qwen3.6-35B-A3B on this Apple-Silicon host.
#
# Per the docs/kernel-dev/BENCHMARK_PROTOCOL.md contract: this is the
# yardstick the optimization compares to. The crate's own engine is
# the "ours" side; llama.cpp is the "baseline" side. ggml is *only*
# a measurement tool here, never a runtime dependency of the crate.
#
# Usage:
#   tools/run_baseline_llama_bench.sh [GGUF_PATH] [OUT_DIR]
#
# Defaults:
#   GGUF_PATH = $CTOX_QWEN36_Q4_K_M_GGUF or
#               runtime/models/qwen36_35b_a3b_gguf/Qwen_Qwen3.6-35B-A3B-Q4_K_M.gguf
#   OUT_DIR   = docs/kernel-dev/baselines/<UTC date>/

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

GGUF="${1:-${CTOX_QWEN36_Q4_K_M_GGUF:-${REPO_ROOT}/../../../../runtime/models/qwen36_35b_a3b_gguf/Qwen_Qwen3.6-35B-A3B-Q4_K_M.gguf}}"
DATE_TAG="$(date -u +%Y-%m-%dT%H%MZ)"
OUT_DIR="${2:-${REPO_ROOT}/docs/kernel-dev/baselines/${DATE_TAG}}"

mkdir -p "$OUT_DIR"

if [[ ! -f "$GGUF" ]]; then
  echo "ERROR: GGUF not found at $GGUF" >&2
  exit 2
fi

if ! command -v llama-bench >/dev/null 2>&1; then
  echo "ERROR: llama-bench not on PATH (try \`brew install llama.cpp\`)" >&2
  exit 3
fi

echo "==> Capturing GGML baseline for Qwen3.6-35B-A3B Q4_K_M"
echo "    GGUF      : $GGUF"
echo "    OUT_DIR   : $OUT_DIR"
echo "    llama-bench: $(command -v llama-bench)"

# Host facts the baseline numbers are conditional on.
{
  echo "# host facts captured ${DATE_TAG}"
  sysctl -n machdep.cpu.brand_string
  sysctl -n hw.memsize
  sw_vers
  echo "--- ggml/llama.cpp version ---"
  llama-bench --version 2>&1 | head -3
  echo "--- power source ---"
  pmset -g batt 2>&1 | head -2
  echo "--- gguf hash ---"
  shasum -a 256 "$GGUF"
} > "$OUT_DIR/host_facts.txt"

# Acceptance pack per docs/kernel-dev/BENCHMARK_PROTOCOL.md:
#   prompt sizes 512, 4096, 16384  (prefill)
#   gen lengths  128, 512          (decode)
# Repetitions: 3 (raise to 5–7 for promotion gates in stage 5).
#
# Output: stable JSON via --output json so the optimizer can diff
# baseline vs. ours mechanically.
echo
echo "==> llama-bench main run"
llama-bench \
  --model "$GGUF" \
  --n-prompt 512,4096,16384 \
  --n-gen 128,512 \
  --n-gpu-layers 99 \
  --threads 8 \
  --repetitions 3 \
  --output json \
  --output-err md \
  2> "$OUT_DIR/llama_bench.stderr.md" \
  > "$OUT_DIR/llama_bench.json"

echo
echo "==> Result summary"
python3 - "$OUT_DIR/llama_bench.json" <<'PY'
import json, sys
with open(sys.argv[1]) as f:
    rows = json.load(f)
print(f"{'phase':<10}{'n_prompt':>10}{'n_gen':>8}{'avg t/s':>14}{'stddev':>10}")
for r in rows:
    n_p = r.get('n_prompt', 0)
    n_g = r.get('n_gen', 0)
    if n_g == 0:
        phase = "prefill"
    elif n_p == 0:
        phase = "decode"
    else:
        phase = "mixed"
    print(f"{phase:<10}{n_p:>10}{n_g:>8}{r.get('avg_ts',0):>14.2f}{r.get('stddev_ts',0):>10.2f}")
PY

echo
echo "==> Baseline captured under $OUT_DIR/"
echo "    - host_facts.txt"
echo "    - llama_bench.json"
echo "    - llama_bench.stderr.md"
