#!/usr/bin/env bash
set -euo pipefail
export E2E_OUT="${E2E_OUT:-$CARGO_TARGET_DIR/evidence/native-path}"
mkdir -p "$E2E_OUT"
PLAYWRIGHT_MODULE="${PLAYWRIGHT_MODULE:-$(node - <<'JS'
const roots=['/mnt/nvme1/build-lane/deps/shell-browser-collection-auth','/mnt/nvme1/build-lane/deps/n339-workjet-b2ad0199-deps','/mnt/nvme1/build-lane/deps/architecture-context-ci-20261013'];
for(const root of roots)try{process.stdout.write(require.resolve('playwright',{paths:[root]}));process.exit(0)}catch{}
throw Error('Existing Playwright required: set PLAYWRIGHT_MODULE');
JS
)}"
WS_MODULE="${WS_MODULE:-$(node - <<'JS'
const roots=['/mnt/nvme1/build-lane/deps/shell-browser-collection-auth','/mnt/nvme1/build-lane/deps/architecture-context-ci-20261013','/mnt/nvme1/build-lane/src/milton-appsec-pdf-html-escape'];
for(const root of roots)try{process.stdout.write(require.resolve('ws',{paths:[root]}));process.exit(0)}catch{}
throw Error('Existing ws required: set WS_MODULE');
JS
)}"
export PLAYWRIGHT_MODULE WS_MODULE
if [[ -z "${TURN_BIN:-}" ]]; then
 for candidate in /mnt/nvme1/build-lane/artifacts/*/coturn/usr/bin/turnserver /mnt/nvme1/build-lane/target/v4-*/evidence/*/coturn/usr/bin/turnserver /usr/bin/turnserver; do
  if [[ -x "$candidate" ]]; then export TURN_BIN="$candidate";break;fi
 done
fi
if [[ -z "${TURN_BIN:-}" ]]; then
 mkdir -p "$E2E_OUT/debs" "$E2E_OUT/coturn"
 mapfile -t deps < <(apt-cache depends coturn | sed -n 's/^  Depends: //p' | sed '/[<>]/d')
 (cd "$E2E_OUT/debs" && apt-get download coturn "${deps[@]}")
 for deb in "$E2E_OUT"/debs/*.deb; do dpkg-deb -x "$deb" "$E2E_OUT/coturn";done
 export TURN_BIN="$E2E_OUT/coturn/usr/bin/turnserver"
fi
export TURN_LIBS="${TURN_LIBS:-$(dirname "$(dirname "$TURN_BIN")")/lib/x86_64-linux-gnu}"
export E2E_NATIVE="${E2E_NATIVE:-$CARGO_TARGET_DIR/debug/ctox-sync-v4-e2e-peer}"
export CHROME_BIN="${CHROME_BIN:-/usr/bin/google-chrome}"
timeout --kill-after=5s 300s node tests/sync-v4-e2e-path/run.mjs
