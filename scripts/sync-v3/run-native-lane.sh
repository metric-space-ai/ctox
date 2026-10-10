#!/usr/bin/env bash
# Reuse the shared published native artifact; never compile a duplicate baseline.
set -euo pipefail
: "${TMPDIR:?normal lane required}"
: "${BUILD_LANE_BIN:?normal lane required}"
export PATH="$BUILD_LANE_BIN/../cache/node-v24.13.1-linux-x64/bin:$PATH"
export PLAYWRIGHT_BROWSERS_PATH="$CARGO_TARGET_DIR/core-only-browser-cache/browsers"
export BUILD_LANE_HEAD=$(git rev-parse HEAD)
package=${1:?shared native archive required}
playwright_module=${2:?pinned Playwright required}
evidence="$BUILD_LANE_BIN/../evidence/architecture/sync-v3-s0-resources/$(basename "$TMPDIR")"
soak=${3:-20}
[[ "$soak" = 20 || "$soak" = 86400 ]] || exit 64
mkdir -p "$evidence"
# Preserve only bounded evidence, not databases, caches, fixture copies or secrets.
retain() {
  for rtt in 0 300 600; do
    cp "$TMPDIR/sync-v3-native-evidence/rtt-$rtt.log" "$evidence/" 2>/dev/null || true
    for name in sync-v3-scale-result.json sync-v3-scale-partial.json sync-v3-phase-failure.json sync-v3-relay.json sync-v3-visible-data.png process-lifecycle.json runner-owner.json; do
      if [[ -f "$TMPDIR/sync-v3-native-evidence/rtt-$rtt/$name" ]]; then
        cp "$TMPDIR/sync-v3-native-evidence/rtt-$rtt/$name" "$evidence/rtt-$rtt-$name"
      fi
    done
  done
  cp "$TMPDIR/sync-v3-native-evidence/report.json" "$evidence/" 2>/dev/null || true
  printf 'sync_v3_evidence=%s\n' "$evidence"
}
trap retain EXIT
printf '%s  %s\n' 98d8c153022eeb83bc6f84a5fade2d838f7bf3749a19b051ba6fe36adb56e64a "$package" | sha256sum -c -
mkdir "$TMPDIR/native"
tar -xzf "$package" -C "$TMPDIR/native" ./bin/ctox
printf '%s  %s\n' 741b7260925c1e27b11cc8100d7877f1f582325523f6d31d1a69e4dac4c4f8ba "$TMPDIR/native/bin/ctox" | sha256sum -c -
node -e 'const p=require(process.argv[1]+"/package.json");if(p.version!=="1.60.0")throw Error("Pinned Playwright mismatch")' "$playwright_module"
node --test scripts/sync-v3/relay.test.mjs scripts/sync-v3/measurement.test.mjs scripts/sync-v3/phase-analysis.test.mjs scripts/sync-v3/resource-observer.test.mjs
python3 scripts/sync-v3/sqlite-lock-probe.py --self-test
SIGNALING_SELF_TEST=1 node src/core/rxdb/tools/local_signaling_server.js
if [[ "$soak" = 86400 ]]; then
  status="$BUILD_LANE_BIN/../evidence/architecture/sync-v3-s0-resources/soak-20261010/status.json"
  [[ ! -e "$status" ]] || { echo 'Refusing to replace an existing soak observation'; exit 64; }
  mkdir -p "$(dirname "$status")"
  systemd-run --user --unit=ctox-sync-v3-s0-locks-20261010 --wait --quiet \
    --property=Type=exec --property=RuntimeMaxSec=86900 --property=TimeoutStopSec=20 \
    --property=KillMode=control-group --property=CPUQuota=200% --property=MemoryMax=6G \
    --property=MemorySwapMax=0 --property="WorkingDirectory=$(pwd)" \
    --setenv="PATH=$PATH" --setenv="TMPDIR=$TMPDIR" --setenv="BUILD_LANE_HEAD=$BUILD_LANE_HEAD" \
    --setenv="CARGO_TARGET_DIR=$CARGO_TARGET_DIR" --setenv="PLAYWRIGHT_BROWSERS_PATH=$PLAYWRIGHT_BROWSERS_PATH" \
    node scripts/measure-sync-v3-native.mjs --binary "$TMPDIR/native/bin/ctox" \
    --playwright "$playwright_module" --output "$TMPDIR/sync-v3-native-evidence" \
    --soak-seconds 86400 --soak-status "$status"
else
  node scripts/measure-sync-v3-native.mjs --binary "$TMPDIR/native/bin/ctox" --playwright "$playwright_module" \
    --output "$TMPDIR/sync-v3-native-evidence" --soak-seconds 20 --soak-status "$evidence/short-soak.json"
fi
git diff --check
