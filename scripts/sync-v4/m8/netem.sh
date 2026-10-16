#!/usr/bin/env bash
# Invoked only by unshare --user --map-root-user --net: lo belongs to THIS namespace.
set -euo pipefail
[[ -n "${M8_PARENT_NETNS:-}" && "$(readlink /proc/self/ns/net)" != "$M8_PARENT_NETNS" ]] || { echo "refusing tc outside a new network namespace" >&2; exit 64; }
ip link set lo up
if [[ "${1:-}" == --probe ]]; then
  tc qdisc add dev lo root netem delay 1ms loss 1%
  tc qdisc show dev lo
  exit 0
fi
binary="$1"; output="$2"; seconds="$3"; rtt="$4"
tc qdisc add dev lo root netem delay "$((rtt/2))ms" loss 1% rate 64mbit limit 8192
set +e
"$binary" "$output" "$seconds" "$rtt" netns-tc
rc=$?
tc -s -j qdisc show dev lo > "$output/netem-$rtt.json"
exit "$rc"
