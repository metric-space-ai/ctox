#!/usr/bin/env bash
# Install no host service: extract test-only coturn into this order's target.
set -euo pipefail
export PATH=/home/metricspace/.nvm/versions/node/v22.23.1/bin:$PATH
export RUST_TEST_THREADS=2 PYTHONPYCACHEPREFIX="$CARGO_TARGET_DIR/p6-pycache"
train="${1:?immutable product train required}"
deadline="${2:?absolute Unix deadline required}"
tools="$CARGO_TARGET_DIR/p6-review5-tools"
mkdir -p "$tools"
(
  cd "$tools"
  apt-get download coturn libhiredis0.14 libmysqlclient21 libpq5 libevent-core-2.1-7 libevent-extra-2.1-7 libevent-openssl-2.1-7 libevent-pthreads-2.1-7
  for package in ./*.deb; do dpkg-deb -x "$package" root; done
  sha256sum ./*.deb > packages.sha256
  for package in ./*.deb; do dpkg-deb -f "$package" Package Version; done > packages.txt
)
turn="$tools/root/usr/bin/turnserver"
lib="$tools/root/usr/lib/x86_64-linux-gnu"
LD_LIBRARY_PATH="$lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" ldd "$turn" > "$tools/ldd.txt"
if grep -q 'not found' "$tools/ldd.txt"; then cat "$tools/ldd.txt"; exit 1; fi
ctox-prep.sh
python3 tests/sync-v4/p6-review-4/run.py --only scenarios --train "$train" --deadline "$deadline" --turn-bin "$turn" --turn-lib "$lib"
