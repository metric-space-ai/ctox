#!/usr/bin/env bash
# Build-lane verifier only. Never called by the production app runtime.
set -euo pipefail
case "$(uname -sm)" in
  "Linux x86_64") ;;
  *) echo "This verifier provisions only Linux x64 test Node; use an installed native Node for other hosts." >&2; exit 2 ;;
esac
: "${CARGO_TARGET_DIR:?Run this verifier through gpu-build-run.sh}"
runtime_cache="$CARGO_TARGET_DIR/app-runtime-test-node"
runtime_node="$runtime_cache/bin/node"
if [ ! -x "$runtime_node" ]; then
  mkdir -p "$runtime_cache"
  archive="$runtime_cache/node.tar.xz"
  curl --fail --location --max-time 90 --silent --show-error \
    https://nodejs.org/dist/v26.12.0/node-v26.12.0-linux-x64.tar.xz --output "$archive"
  echo "fd47865ecd6391ebb431fc815e01b9135bf83ded5cf373247e9331e196b7aded  $archive" | sha256sum --check
  tar -xJf "$archive" --strip-components=1 -C "$runtime_cache"
  rm "$archive"
fi
export CTOX_APP_RUNTIME_TEST_NODE="$runtime_node"
if [ "${1:-}" = kit ]; then
  "$runtime_node" --test --test-concurrency=2 src/core/app_runtime/handler-kit/*.test.mjs
  exit
fi
export CTOX_APP_RUNTIME_TEST_ROOT="$CARGO_TARGET_DIR/ipc-tests"
"$runtime_node" --version
sha256sum src/core/app_runtime/{lib.rs,worker.mjs,Cargo.toml,Cargo.lock} \
  src/core/app_runtime/handler-kit/{index.mjs,mango.mjs,handler-kit.test.mjs} \
  src/core/execution/models/local_transport.rs
cargo fmt --manifest-path src/core/app_runtime/Cargo.toml --check
feature_args=()
if [ "${1:-}" = legacy-store ]; then feature_args=(--features legacy-store); fi
cargo test --locked --manifest-path src/core/app_runtime/Cargo.toml "${feature_args[@]}" -- --test-threads=2
"$runtime_node" --test --test-concurrency=2 src/core/app_runtime/handler-kit/handler-kit.test.mjs
git diff --check
