#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/validate_kernel_experiment.sh [--strict] <experiment.md>

Checks a kernel experiment record for required reproducibility fields.

Default mode:
  verifies scaffold/run-manifest fields are present.

--strict:
  additionally rejects template placeholders and requires benchmark/candidate
  fields that are needed before an experiment can support an accept/reject
  decision.
USAGE
}

strict=0
if [[ "${1:-}" == "--strict" ]]; then
  strict=1
  shift
fi

if [[ $# -ne 1 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 2
fi

record="$1"
if [[ ! -f "$record" ]]; then
  echo "missing experiment record: $record" >&2
  exit 2
fi

failures=()
warnings=()

field_value() {
  local key="$1"
  grep -m1 -E "^${key}:" "$record" \
    | sed -E "s/^${key}:[[:space:]]*//" \
    | sed -E 's/[[:space:]]+$//' || true
}

require_field() {
  local key="$1"
  local value
  value="$(field_value "$key")"
  if [[ -z "$value" ]]; then
    failures+=("missing required field: $key")
  fi
}

require_strict_field() {
  local key="$1"
  local value
  value="$(field_value "$key")"
  if [[ -z "$value" || "$value" == "n/a" || "$value" == "-" ]]; then
    failures+=("missing strict field: $key")
  fi
}

base_required=(
  date
  owner
  model
  metalpack
  baseline_commit_or_state
  git_commit
  git_dirty_state
  device
  macos_version
  metal_device_name
  accepted_profile_path
  accepted_profile_hash
  metalpack_path
  full_env_dump
  reference_impl
)

for key in "${base_required[@]}"; do
  require_field "$key"
done

metalpack_path="$(field_value metalpack_path)"
if [[ -n "$metalpack_path" && "$metalpack_path" != "n/a" ]]; then
  for key in metalpack_manifest_hash weights_hash; do
    value="$(field_value "$key")"
    if [[ -z "$value" || "$value" == "n/a" ]]; then
      failures+=("metalpack provided but $key is missing")
    fi
  done
fi

env_dump="$(field_value full_env_dump)"
if [[ -n "$env_dump" && "$env_dump" != "n/a" && ! -f "$env_dump" ]]; then
  warnings+=("env dump path does not exist locally: $env_dump")
fi

accepted_profile_path="$(field_value accepted_profile_path)"
accepted_profile_hash="$(field_value accepted_profile_hash)"
if [[ -n "$accepted_profile_path" && "$accepted_profile_path" != "n/a" ]]; then
  if [[ ! -f "$accepted_profile_path" ]]; then
    warnings+=("accepted profile path does not exist locally: $accepted_profile_path")
  elif [[ -n "$accepted_profile_hash" && "$accepted_profile_hash" != "n/a" ]]; then
    actual_profile_hash="$(shasum -a 256 "$accepted_profile_path" | awk '{print $1}')"
    if [[ "$actual_profile_hash" != "$accepted_profile_hash" ]]; then
      warnings+=("accepted profile hash mismatch: recorded=$accepted_profile_hash actual=$actual_profile_hash")
    fi
  fi
fi

if [[ "$strict" -eq 1 ]]; then
  strict_required=(
    target_path
    env_flag
    binary_path
    candidate_env
  )
  for key in "${strict_required[@]}"; do
    require_strict_field "$key"
  done

  if grep -nE '<[^>]+>' "$record" >/tmp/ctox_kernel_experiment_placeholders.$$; then
    while IFS= read -r line; do
      failures+=("template placeholder still present: $line")
    done </tmp/ctox_kernel_experiment_placeholders.$$
  fi
  rm -f /tmp/ctox_kernel_experiment_placeholders.$$

  if grep -nE '^[[:space:]]*-[[:space:]]*$' "$record" >/tmp/ctox_kernel_experiment_empty_bullets.$$; then
    while IFS= read -r line; do
      warnings+=("empty checklist bullet remains: $line")
    done </tmp/ctox_kernel_experiment_empty_bullets.$$
  fi
  rm -f /tmp/ctox_kernel_experiment_empty_bullets.$$
fi

if [[ "${#warnings[@]}" -gt 0 ]]; then
  echo "warnings:"
  for warning in "${warnings[@]}"; do
    echo "  - $warning"
  done
fi

if [[ "${#failures[@]}" -gt 0 ]]; then
  echo "validation: FAIL"
  for failure in "${failures[@]}"; do
    echo "  - $failure"
  done
  exit 1
fi

echo "validation: PASS"
if [[ "$strict" -eq 1 ]]; then
  echo "mode: strict"
else
  echo "mode: default"
fi
