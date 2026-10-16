#!/usr/bin/env bash
# Core counterpart checks. This is a build-lane entry point, not a GitHub gate.
set -euo pipefail
cd "$(dirname "$0")/../.."
export RUST_TEST_THREADS=2
node --version
node --input-type=module -e 'if (typeof WebSocket !== "function" || typeof [][Symbol.iterator]().map !== "function") throw new Error("S9 verification requires Node.js >=22 with WebSocket and iterator helpers")'
proof="${CARGO_TARGET_DIR:?run through the build lane}/s9-proof"
mkdir -p "$proof"
export S9_PROOF_DIR="$proof"

verify_js() {
  cargo build --locked --manifest-path src/core/rxdb/Cargo.toml --example v15_wire_daemon > "$proof/wire-build.log" 2>&1 || { tail -100 "$proof/wire-build.log"; return 1; }
  if [[ ! -e runtime/build/cargo-target ]]; then
    mkdir -p runtime/build
    ln -s "$CARGO_TARGET_DIR" runtime/build/cargo-target
  fi
  export PLAYWRIGHT_BROWSERS_PATH="$CARGO_TARGET_DIR/s9-browsers"
  if [[ ! -e src/apps/business-os/node_modules ]]; then
    ln -s "$CARGO_TARGET_DIR/s9-js/node_modules" src/apps/business-os/node_modules
  fi
  node src/apps/business-os/rxdb/tests/run-all.mjs --require-wire-daemon > "$proof/rxdb-js.log" 2>&1 || { cat "$proof/rxdb-js.log"; return 1; }
}

if [[ "${1:-}" == "--js" ]]; then
  proof="$CARGO_TARGET_DIR/s9-proof-node22"
  mkdir -p "$proof"
  node --version > "$proof/node-version.txt"
  verify_js
  git diff --check
  exit 0
fi

printf 'S9 phase: format and declarative artifact contracts\n'
rustfmt --check --edition 2021 --config skip_children=true src/core/business_os/server.rs src/core/business_os/shell_update.rs
node --check src/core/rxdb/tools/browser_rust_smoke.js
node --test src/core/rxdb/tools/module_schema_artifact.test.mjs > "$proof/artifact.log" 2>&1 || { cat "$proof/artifact.log"; exit 1; }
node src/core/rxdb/tools/build_business_os_module_schema_files.mjs > "$proof/module-artifacts.log" 2>&1 || { cat "$proof/module-artifacts.log"; exit 1; }
node src/core/rxdb/tools/build_business_os_schema_contract.mjs > "$proof/schema-contract.log" 2>&1 || { cat "$proof/schema-contract.log"; exit 1; }
for fixture in workjet-computer-migration workjet-project-migration workjet-project-configuration crew-identity module-demand-only-collections module-migrations; do
  node "src/apps/business-os/rxdb/tests/$fixture-smoke.mjs" > "$proof/$fixture.log" 2>&1 || { cat "$proof/$fixture.log"; exit 1; }
done
node --test src/core/rxdb/tools/business_os_ui_catalog_fixture.test.js > "$proof/catalog.log" 2>&1 || { cat "$proof/catalog.log"; exit 1; }
python3 scripts/sync-v4/s9-mutations.py schema
python3 scripts/sync-v4/s9-mutations.py reader

# Keep test dependencies and the browser under this task's disposable target.
printf 'S9 phase: real IndexedDB freshness\n'
deps="$CARGO_TARGET_DIR/s9-js"
mkdir -p "$deps"
cp src/apps/business-os/package.json src/apps/business-os/package-lock.json "$deps/"
export npm_config_cache="$CARGO_TARGET_DIR/s9-npm-cache"
export PLAYWRIGHT_BROWSERS_PATH="$CARGO_TARGET_DIR/s9-browsers"
npm ci --prefix "$deps" --ignore-scripts --no-audit --no-fund > "$proof/npm.log" 2>&1
if [[ ! -e src/apps/business-os/node_modules ]]; then
  ln -s "$deps/node_modules" src/apps/business-os/node_modules
fi
node "$deps/node_modules/playwright/cli.js" install chromium > "$proof/browser-install.log" 2>&1
node src/apps/business-os/rxdb/tests/current-pull-freshness-browser-smoke.mjs > "$proof/freshness.log" 2>&1 || { cat "$proof/freshness.log"; exit 1; }

ctox-prep.sh
printf 'S9 phase: native check\n'
cargo check --locked --bin ctox > "$proof/cargo-check.log" 2>&1 || { tail -120 "$proof/cargo-check.log"; exit 1; }
printf 'S9 phase: signed inventory, static authorization and native role policy\n'
python3 scripts/sync-v4/s9_native.py native
printf 'S9 phase: inventory mutation and restored native source\n'
python3 scripts/sync-v4/s9-mutations.py inventory
python3 scripts/sync-v4/s9_native.py restored
printf 'S9 phase: native RxDB and full browser-runtime regressions\n'
cargo test --locked --manifest-path src/core/rxdb/Cargo.toml -- --test-threads=2 > "$proof/native-rxdb.log" 2>&1 || { tail -140 "$proof/native-rxdb.log"; exit 1; }
verify_js
git diff --check
printf 'S9 counterpart checks passed; evidence %s\n' "$proof"
