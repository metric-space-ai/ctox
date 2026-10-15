#!/usr/bin/env bash
set -euo pipefail
# Reuse an existing lane installation, never install packages during acceptance.
export PLAYWRIGHT_MODULE="${PLAYWRIGHT_MODULE:-$(node - <<'JS'
const roots=['/mnt/nvme1/build-lane/deps/shell-browser-collection-auth','/mnt/nvme1/build-lane/deps/n339-workjet-b2ad0199-deps','/mnt/nvme1/build-lane/deps/architecture-context-ci-20261013'];
for(const root of roots)for(const name of ['playwright','playwright-core'])try{process.stdout.write(require.resolve(name,{paths:[root]}));process.exit(0)}catch{}
process.stderr.write('No existing Playwright installation: set PLAYWRIGHT_MODULE to an official lane installation\n');process.exit(66);
JS
)}"
export CHROME_BIN="${CHROME_BIN:-/usr/bin/google-chrome}"
node src/apps/business-os/kernel/tests/browser.mjs
