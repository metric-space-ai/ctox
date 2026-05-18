#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 5 ]]; then
  cat >&2 <<'USAGE'
usage: tools/profile_attention_variant_stages.sh <metalpack-dir> <tokens> <layer> <iterations> <candidate-env KEY=VALUE> [mps-attention-out-sidecar-dir]

Runs serial cumulative stage profiles for accepted qh4 SIMD32 vec8 and one
candidate attention variant. It estimates prepare/project prefix time and
attention-only time as:

  attention_only = profile_stop(attention) - profile_stop(prepare)
USAGE
  exit 2
fi

metalpack="$1"
tokens="$2"
layer="$3"
iterations="$4"
candidate_env="$5"
mps_attention_out="${6:-}"

bench="target/release/bench_metalpack_prefill_attention_core"
if [[ ! -x "$bench" ]]; then
  echo "missing $bench; run cargo build --release --bins first" >&2
  exit 2
fi
if [[ "$candidate_env" != *=* ]]; then
  echo "candidate-env must be KEY=VALUE, got: $candidate_env" >&2
  exit 2
fi

tmpdir="$(mktemp -d /tmp/ctox_qwen35_attention_stage_profile.XXXXXX)"
trap 'rm -rf "$tmpdir"' EXIT

run_one() {
  local label="$1"
  local envspec="$2"
  local stop="$3"
  local out="$tmpdir/${label}_${stop}.out"
  if [[ -n "$mps_attention_out" ]]; then
    env CTOX_QWEN35_ATTENTION_CORE_PROFILE_STOP="$stop" "$envspec" \
      "$bench" "$metalpack" "$layer" "$tokens" "$iterations" 1 "$mps_attention_out" > "$out"
  else
    env CTOX_QWEN35_ATTENTION_CORE_PROFILE_STOP="$stop" "$envspec" \
      "$bench" "$metalpack" "$layer" "$tokens" "$iterations" 1 > "$out"
  fi
  python3 - "$out" <<'PY'
import sys
path = sys.argv[1]
for line in open(path, encoding="utf-8"):
    if line.startswith("median_s:"):
        print(line.split(":", 1)[1].strip())
        break
else:
    raise SystemExit(f"median_s missing in {path}")
PY
}

accepted_prepare="$(run_one accepted CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8=1 prepare)"
accepted_attention="$(run_one accepted CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8=1 attention)"
candidate_prepare="$(run_one candidate "$candidate_env" prepare)"
candidate_attention="$(run_one candidate "$candidate_env" attention)"

python3 - \
  "$metalpack" "$tokens" "$layer" "$iterations" "$candidate_env" "$mps_attention_out" \
  "$accepted_prepare" "$accepted_attention" "$candidate_prepare" "$candidate_attention" <<'PY'
import sys
(
    _,
    metalpack,
    tokens,
    layer,
    iterations,
    candidate_env,
    sidecar,
    ap,
    aa,
    cp,
    ca,
) = sys.argv
ap = float(ap)
aa = float(aa)
cp = float(cp)
ca = float(ca)
print("attention_variant_stage_profile")
print(f"metalpack: {metalpack}")
print(f"tokens: {tokens}")
print(f"layer: {layer}")
print(f"iterations: {iterations}")
print(f"candidate_env: {candidate_env}")
if sidecar:
    print(f"mps_attention_out_sidecar: {sidecar}")
print()
print("record_type,variant,prepare_s,attention_cumulative_s,attention_only_s")
print(f"profile,accepted,{ap:.9f},{aa:.9f},{aa-ap:.9f}")
print(f"profile,candidate,{cp:.9f},{ca:.9f},{ca-cp:.9f}")
print(f"delta,candidate_minus_accepted,{cp-ap:.9f},{ca-aa:.9f},{(ca-cp)-(aa-ap):.9f}")
PY
