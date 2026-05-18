#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 ]]; then
  cat >&2 <<'USAGE'
usage: tools/capture_metal_trace.sh <output.trace> <command> [args...]

Example:
  tools/capture_metal_trace.sh /tmp/qwen_delta_block.trace \
    target/release/bench_metalpack_prefill_delta_block \
    /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 5

The script records a Metal System Trace for the launched benchmark and writes a
table-of-contents XML next to the trace. Use Xcode Instruments for detailed GPU
counter inspection when xctrace XML tables do not expose cache counters directly.
USAGE
  exit 2
fi

trace_path="$1"
shift
toc_path="${trace_path%.trace}.toc.xml"
command_path="$1"
shift
if [[ "$command_path" == */* && "$command_path" != /* ]]; then
  command_path="$(pwd)/$command_path"
fi

set +e
xcrun xctrace record \
  --template "Metal System Trace" \
  --output "$trace_path" \
  --no-prompt \
  --launch -- "$command_path" "$@"
record_status=$?
set -e

if [[ $record_status -ne 0 && ! -e "$trace_path" ]]; then
  echo "xctrace record failed with status $record_status and did not create $trace_path" >&2
  exit "$record_status"
fi
if [[ $record_status -ne 0 ]]; then
  echo "xctrace record returned status $record_status, but trace exists; continuing export" >&2
fi

xcrun xctrace export --input "$trace_path" --toc --output "$toc_path"

echo "trace: $trace_path"
echo "toc:   $toc_path"
