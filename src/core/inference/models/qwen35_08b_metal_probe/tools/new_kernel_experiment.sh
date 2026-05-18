#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/new_kernel_experiment.sh <slug> [metalpack-dir]

Creates a timestamped experiment record under docs/kernel-dev/experiments/
using EXPERIMENT_TEMPLATE.md and fills the reproducibility run manifest as far
as possible.

Examples:
  tools/new_kernel_experiment.sh deltanet-chunked-scan
  tools/new_kernel_experiment.sh ffn-mma128-sweep /tmp/ctox_qwen35_08b_real_fp16.metalpack
USAGE
}

if [[ $# -lt 1 || $# -gt 2 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 2
fi

slug="$1"
metalpack="${2:-${CTOX_QWEN35_METALPACK:-}}"

if [[ ! "$slug" =~ ^[A-Za-z0-9._-]+$ ]]; then
  echo "invalid slug '$slug'; use only letters, numbers, dot, underscore, or dash" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
template="$repo_root/docs/kernel-dev/EXPERIMENT_TEMPLATE.md"
accepted_profile="$repo_root/docs/kernel-dev/accepted_profile.env"
out_dir="$repo_root/docs/kernel-dev/experiments"
mkdir -p "$out_dir"

timestamp="$(date -u '+%Y%m%dT%H%M%SZ')"
out="$out_dir/${timestamp}-${slug}.md"

git_commit="$(git -C "$repo_root" rev-parse --short HEAD 2>/dev/null || echo unknown)"
git_dirty_state="$(git -C "$repo_root" status --short -- . 2>/dev/null | sed -n '1,80p' || true)"
if [[ -z "$git_dirty_state" ]]; then
  git_dirty_state="clean"
fi

macos_version="$(sw_vers -productVersion 2>/dev/null || echo unknown)"
device="$(uname -a 2>/dev/null || echo unknown)"
metal_device_name="$(system_profiler SPDisplaysDataType 2>/dev/null \
  | awk -F': ' '/Chipset Model|Metal Support/ {print $2}' \
  | paste -sd ';' - \
  | sed 's/^$/unknown/' || echo unknown)"

metalpack_manifest_hash="n/a"
weights_hash="n/a"
if [[ -n "$metalpack" ]]; then
  if [[ -f "$metalpack/manifest.json" ]]; then
    metalpack_manifest_hash="$(shasum -a 256 "$metalpack/manifest.json" | awk '{print $1}')"
  fi
  if [[ -f "$metalpack/weights.bin" ]]; then
    weights_hash="$(shasum -a 256 "$metalpack/weights.bin" | awk '{print $1}')"
  fi
fi

env_dump_path="/tmp/ctox_qwen35_env_${timestamp}_${slug}.txt"
env | sort > "$env_dump_path"
output_csv="/tmp/ctox_qwen35_${timestamp}_${slug}.csv"
dump_paths="/tmp/ctox_qwen35_${timestamp}_${slug}_*.bin"
accepted_profile_hash="n/a"
if [[ -f "$accepted_profile" ]]; then
  accepted_profile_hash="$(shasum -a 256 "$accepted_profile" | awk '{print $1}')"
fi

{
  cat <<HEADER
# Kernel Experiment: $slug

Generated: $timestamp

HEADER
  cat "$template"
} > "$out"

replace_field() {
  local key="$1"
  local value="$2"
  KEY="$key" VALUE="$value" perl -0pi -e '
    my $key = $ENV{"KEY"};
    my $value = $ENV{"VALUE"};
    s/^(\Q$key\E:).*$/$1 $value/m;
  ' "$out"
}

replace_field "date" "$timestamp"
replace_field "owner" "${USER:-unknown}"
replace_field "model" "Qwen3.5-0.8B"
replace_field "metalpack" "${metalpack:-n/a}"
replace_field "baseline_commit_or_state" "$git_commit"
replace_field "git_commit" "$git_commit"
replace_field "git_dirty_state" "$(printf '%s' "$git_dirty_state" | tr '\n' ';' | sed 's/;*$//')"
replace_field "device" "$device"
replace_field "macos_version" "$macos_version"
replace_field "metal_device_name" "$metal_device_name"
replace_field "accepted_profile_path" "$accepted_profile"
replace_field "accepted_profile_hash" "$accepted_profile_hash"
replace_field "metalpack_path" "${metalpack:-n/a}"
replace_field "metalpack_manifest_hash" "$metalpack_manifest_hash"
replace_field "weights_hash" "$weights_hash"
replace_field "build_profile" "release"
replace_field "full_env_dump" "$env_dump_path"
replace_field "baseline_env" "$accepted_profile"
replace_field "output_csv" "$output_csv"
replace_field "dump_paths" "$dump_paths"
replace_field "reference_impl" "MLX + llama.cpp"

echo "experiment: $out"
echo "env_dump:   $env_dump_path"

if [[ -x "$repo_root/tools/update_kernel_experiment_index.sh" ]]; then
  "$repo_root/tools/update_kernel_experiment_index.sh" >/dev/null
  echo "index:      $repo_root/docs/kernel-dev/experiments/INDEX.md"
fi
