#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/run_measurement_pack.sh [--dry-run] [--capture] <pack> <metalpack-dir>

Runs one of the standardized kernel-dev measurement packs. The accepted
baseline profile is applied through tools/run_accepted_profile.sh.

Packs:
  smoke          128 tokens, 1 iter, 0 warmup, 1 Delta layer, 1 tune pass
  candidate      4096 tokens, 3 iters, 1 warmup, 18 Delta layers, 2 passes
  candidate-7    4096 tokens, 7 iters, 1 warmup, 18 Delta layers, 2 passes
  acceptance     512,4096,16384 sweep, 3 iters, 1 warmup, 18 Delta layers
  long-context   32768,65536,131072 sweep, 3 iters, 1 warmup, 18 Delta layers

Examples:
  tools/run_measurement_pack.sh --dry-run acceptance /tmp/ctox_qwen35_08b_real_fp16.metalpack
  tools/run_measurement_pack.sh --capture candidate /tmp/ctox_qwen35_08b_real_fp16.metalpack
  tools/run_measurement_pack.sh smoke /tmp/ctox_qwen35_08b_real_fp16.metalpack
USAGE
}

dry_run=0
capture=0
while [[ $# -gt 0 ]]; do
  case "${1:-}" in
    --dry-run)
      dry_run=1
      shift
      ;;
    --capture)
      capture=1
      shift
      ;;
    *)
      break
      ;;
  esac
done

if [[ $# -ne 2 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 2
fi

pack="$1"
metalpack="$2"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
runner="$repo_root/tools/run_accepted_profile.sh"

if [[ ! -d "$metalpack" ]]; then
  echo "missing metalpack directory: $metalpack" >&2
  exit 2
fi

case "$pack" in
  smoke)
    cmd=(
      "$runner"
      "$repo_root/target/release/autotune_metalpack_prefill_delta_stack"
      "$metalpack" 128 1 0 1 0 1
    )
    ;;
  candidate)
    cmd=(
      "$runner"
      "$repo_root/target/release/autotune_metalpack_prefill_delta_stack"
      "$metalpack" 4096 3 1 18 0 2
    )
    ;;
  candidate-7)
    cmd=(
      "$runner"
      "$repo_root/target/release/autotune_metalpack_prefill_delta_stack"
      "$metalpack" 4096 7 1 18 0 2
    )
    ;;
  acceptance)
    cmd=(
      "$runner"
      "$repo_root/target/release/sweep_metalpack_prefill_delta_autotune"
      "$metalpack" 512,4096,16384 3 1 18 0 2
    )
    ;;
  long-context)
    cmd=(
      "$runner"
      "$repo_root/target/release/sweep_metalpack_prefill_delta_autotune"
      "$metalpack" 32768,65536,131072 3 1 18 0 2
    )
    ;;
  *)
    echo "unknown measurement pack: $pack" >&2
    usage
    exit 2
    ;;
esac

if [[ "$dry_run" -eq 1 ]]; then
  printf 'measurement_pack: %s\n' "$pack"
  printf 'capture: %s\n' "$capture"
  printf 'command:'
  printf ' %q' "${cmd[@]}"
  printf '\n'
  exit 0
fi

if [[ "$capture" -eq 1 ]]; then
  exec "$repo_root/tools/capture_measurement_output.sh" --label "pack-${pack}" -- "${cmd[@]}"
fi

exec "${cmd[@]}"
