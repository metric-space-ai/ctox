#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/capture_measurement_output.sh [--accepted-profile] [--output-dir DIR] [--label LABEL] -- <command> [args...]

Runs one measurement command under an exclusive local lock and captures:
  - stdout.txt
  - stderr.txt
  - normalized.txt
  - manifest.txt
  - exit_code.txt

Use this for real benchmark runs so raw output, normalized evidence fields, git
state, accepted-profile hash, and command line stay together.
USAGE
}

accepted_profile=0
output_dir="/tmp/ctox_qwen35_measurements"
label="measurement"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --accepted-profile)
      accepted_profile=1
      shift
      ;;
    --output-dir)
      if [[ $# -lt 2 ]]; then
        usage
        exit 2
      fi
      output_dir="$2"
      shift 2
      ;;
    --label)
      if [[ $# -lt 2 ]]; then
        usage
        exit 2
      fi
      label="$2"
      shift 2
      ;;
    --)
      shift
      break
      ;;
    -h|--help)
      usage
      exit 2
      ;;
    *)
      break
      ;;
  esac
done

if [[ $# -lt 1 ]]; then
  usage
  exit 2
fi

if [[ ! "$label" =~ ^[A-Za-z0-9._-]+$ ]]; then
  echo "invalid label '$label'; use only letters, numbers, dot, underscore, or dash" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

lock_dir="/tmp/ctox_qwen35_measurement.lockdir"
if ! mkdir "$lock_dir" 2>/dev/null; then
  echo "measurement lock is held: $lock_dir" >&2
  echo "another benchmark may be running; do not run parallel measurements" >&2
  exit 1
fi

cleanup() {
  rm -rf "$lock_dir"
}
trap cleanup EXIT INT TERM

timestamp="$(date -u '+%Y%m%dT%H%M%SZ')"
run_dir="$output_dir/${timestamp}-${label}"
mkdir -p "$run_dir"

stdout_path="$run_dir/stdout.txt"
stderr_path="$run_dir/stderr.txt"
normalized_path="$run_dir/normalized.txt"
manifest_path="$run_dir/manifest.txt"
exit_code_path="$run_dir/exit_code.txt"

profile_path="$repo_root/docs/kernel-dev/accepted_profile.env"
profile_hash="n/a"
if [[ -f "$profile_path" ]]; then
  profile_hash="$(shasum -a 256 "$profile_path" | awk '{print $1}')"
fi

git_commit="n/a"
if git rev-parse --verify HEAD >/dev/null 2>&1; then
  git_commit="$(git rev-parse HEAD)"
fi
git_dirty_state="dirty"
if git diff --quiet -- . && git diff --cached --quiet -- .; then
  git_dirty_state="clean"
fi

command_line=""
for arg in "$@"; do
  printf -v quoted "%q" "$arg"
  command_line="${command_line}${command_line:+ }${quoted}"
done

{
  echo "timestamp_utc: $timestamp"
  echo "label: $label"
  echo "repo_root: $repo_root"
  echo "cwd: $(pwd)"
  echo "git_commit: $git_commit"
  echo "git_dirty_state: $git_dirty_state"
  echo "accepted_profile: $accepted_profile"
  echo "accepted_profile_path: $profile_path"
  echo "accepted_profile_hash: $profile_hash"
  echo "command: $command_line"
  echo "stdout: $stdout_path"
  echo "stderr: $stderr_path"
  echo "normalized: $normalized_path"
  echo "exit_code: $exit_code_path"
  echo
  echo "ctox_qwen35_env_before:"
  env | sort | grep '^CTOX_QWEN35_' || true
} > "$manifest_path"

cmd=("$@")
if [[ "$accepted_profile" -eq 1 ]]; then
  cmd=("$repo_root/tools/run_accepted_profile.sh" "$@")
fi

set +e
"${cmd[@]}" >"$stdout_path" 2>"$stderr_path"
status=$?
set -e

echo "$status" > "$exit_code_path"
tools/normalize_benchmark_output.sh "$stdout_path" > "$normalized_path" 2>/dev/null || true

{
  echo
  echo "exit_code_value: $status"
  echo "completed_utc: $(date -u '+%Y%m%dT%H%M%SZ')"
} >> "$manifest_path"

echo "measurement_dir: $run_dir"
echo "exit_code: $status"
echo "stdout: $stdout_path"
echo "stderr: $stderr_path"
echo "normalized: $normalized_path"
echo "manifest: $manifest_path"

exit "$status"
