#!/usr/bin/env bash
# Independent checks retain their own exit codes; one failed invariant cannot hide compile diagnostics.
set -uo pipefail
ctox-prep.sh || exit $?
evidence="$CARGO_TARGET_DIR/evidence/e2e-1c-checks"
mkdir -p "$evidence"
export RUST_TEST_THREADS=2
# ctox-prep installs the pinned Node in its child shell; select it for this runner too.
export PATH="/mnt/nvme1/build-lane/cache/node-v24.13.1-linux-x64/bin:$PATH"
if [[ -z "${PLAYWRIGHT_MODULE_PATH:-}" ]]; then
 PLAYWRIGHT_MODULE_PATH="$(node - <<'JS'
const path=require('node:path');
for(const root of ['/mnt/nvme1/build-lane/deps/shell-browser-collection-auth','/mnt/nvme1/build-lane/deps/architecture-context-ci-20261013']){try{process.stdout.write(path.dirname(require.resolve('playwright',{paths:[root]})));process.exit(0);}catch{}}
process.exit(1);
JS
)" || PLAYWRIGHT_MODULE_PATH=""
 export PLAYWRIGHT_MODULE_PATH
fi
failed=0
step() {
 local name="$1"; shift
 "$@" >"$evidence/$name.log" 2>&1
 local status=$?
 printf '%s\t%s\n' "$name" "$status" >>"$evidence/exits.tsv"
 printf 'E2E_CHECK %s exit=%s\n' "$name" "$status"
 if [[ "$status" != 0 ]]; then tail -n 65 "$evidence/$name.log"; failed=1; fi
 return 0
}
: >"$evidence/exits.tsv"
step kernel node --test --test-concurrency=1 src/apps/business-os/kernel/tests/*.test.mjs tests/sync-v4-e2e-path/*.test.mjs
step kernel-mutations node tests/sync-v4-e2e-path/mutation-proof.mjs
step rxdb-js timeout --kill-after=5s 480s node src/apps/business-os/rxdb/tests/run-all.mjs
step rxdb cargo test --locked --manifest-path src/core/rxdb/Cargo.toml -j2
step native-build cargo build --locked --manifest-path tests/sync-v4-e2e-path/native/Cargo.toml -j2
step glue-mutations node tests/sync-v4-e2e-path/mutation-glue.mjs
step workspace cargo check --workspace --all-targets -j2
node tests/sync-v4-e2e-path/source-hash.mjs >"$evidence/source.json" || failed=1
exit "$failed"
