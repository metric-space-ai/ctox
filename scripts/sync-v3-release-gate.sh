#!/usr/bin/env bash
# S0 first slice: validates the measurement tools, not tenant performance.
# Run through the shared build lane. Fixtures stay in its owned TMPDIR.
set -euo pipefail
cd "$(dirname "$0")/.."
: "${TMPDIR:?The admitted build lane must supply TMPDIR}"
playwright_module=${1:?Supply the pinned Playwright module path}
fixture_parent=$(mktemp -d "$TMPDIR/ctox-sync-v3-s0.XXXXXX")
trap 'rm -rf -- "$fixture_parent"' EXIT
node --test scripts/sync-v3/measurement.test.mjs scripts/sync-v3/relay.test.mjs scripts/sync-v3/phase-analysis.test.mjs scripts/sync-v3/resource-observer.test.mjs
python3 scripts/sync-v3/sqlite-lock-probe.py --self-test
node scripts/measure-sync-v3.mjs --validate-fixture
node scripts/measure-sync-v3.mjs --fixture "$fixture_parent/fixture"
node scripts/measure-sync-v3.mjs --verify-fixture "$fixture_parent/fixture"
# Independent real-browser byte oracle. The existing pinned dependency is input.
node scripts/sync-v3/counter-oracle.browser.mjs "$playwright_module"
