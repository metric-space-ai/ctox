#!/usr/bin/env bash
# Reuses DevOps bench protocol; all processes bind loopback, no host route/qdisc changes.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="${1:?output directory required}"
DURATION="${2:-600}"
[[ "$DURATION" == 600 || "$DURATION" == 35 ]] || exit 64
mkdir -p "$OUT/deps" "$OUT/debs" "$OUT/coturn"
npm install --prefix "$OUT/deps" --ignore-scripts --no-audit --no-fund playwright ws
npm ls --prefix "$OUT/deps" --depth=0 --json > "$OUT/test-dependencies.json"
mapfile -t turn_deps < <(apt-cache depends coturn | sed -n 's/^  Depends: //p' | sed '/[<>]/d')
(cd "$OUT/debs" && apt-get download coturn "${turn_deps[@]}")
for deb in "$OUT"/debs/*.deb; do dpkg-deb -x "$deb" "$OUT/coturn"; done
export M3_ROOT="$ROOT" M3_OUT="$OUT" M3_DURATION="$DURATION"
node --input-type=module <<'NODE'
import fs from 'node:fs';
import path from 'node:path';
const root=process.env.M3_ROOT, out=process.env.M3_OUT;
fs.writeFileSync(path.join(out,'config.json'), JSON.stringify({
 out, durationSeconds:Number(process.env.M3_DURATION), peers:3, dropMode:'channel',
 nativeBin:path.join(process.env.CARGO_TARGET_DIR,'debug/ctox-sync-v4-m3-peer'),
 nativeRevision:process.env.M3_REVISION || 'uncommitted', nativeBuildProfileDeclared:'debug',
 browserRoot:path.join(root,'src/apps/business-os/rxdb/src'),
 browserRevision:process.env.M3_REVISION || 'uncommitted',
 browserAdapter:path.join(root,'tests/sync-v4-m3/browser.mjs'),
 modulesRoot:path.join(out,'deps/node_modules'), chrome:'/opt/google/chrome/chrome',
 browserTmpRoot:path.join(path.dirname(process.env.TMPDIR),'m3'),
 turnserver:path.join(out,'coturn/usr/bin/turnserver'),
 turnLibs:path.join(out,'coturn/usr/lib/x86_64-linux-gnu'),
 turnPort:34791,
 netem:{enabled:false,reason:'M3 needs forced relay and 30s closes; no RTT/netem profile claimed'},
}));
NODE
timeout 750s node "$ROOT/tests/sync-v4-m3/bench-runner.mjs" "$OUT/config.json"
node "$ROOT/tests/sync-v4-m3/assert-result.mjs" "$OUT/result.json"
