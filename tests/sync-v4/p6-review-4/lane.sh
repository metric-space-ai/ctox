#!/usr/bin/env bash
# Order-specific qualification. Reuses the executed bare-train compile result;
# regression counterexamples use the separately recorded #827 composition.
set -euo pipefail
export PATH=/home/metricspace/.nvm/versions/node/v22.23.1/bin:$PATH
export RUST_TEST_THREADS=2 PYTHONPYCACHEPREFIX="$CARGO_TARGET_DIR/p6-pycache"
ctox-prep.sh
proof="$CARGO_TARGET_DIR/p6-train7-precondition"
mkdir -p "$proof"
python3 - "$proof" <<'PY'
import hashlib,json,pathlib,subprocess,sys
out=pathlib.Path(sys.argv[1]);root=pathlib.Path.cwd()
train='59849cee53a9919e657e3a2ed82c8fd1f75595fa'
log=out.parent/'p6-review4-20261016T003521Z/train-daemon-build.log'
text=log.read_text();assert 'error[E0308]' in text and 'expected an array with a size of 36' in text
relative='src/core/business_os/policy.rs'
assert (root/relative).read_bytes()==subprocess.check_output(['git','show',train+':'+relative])
subprocess.run(['git','diff','--quiet',train,'--','src','Cargo.toml','Cargo.lock'],check=True)
(out/'build-unverified.json').write_text(json.dumps({'pass':False,'terminal':True,'revision':train,
 'reason':'bare train daemon build E0308: permission array 36 vs 37','compiler_log':str(log),
 'compiler_log_sha256':hashlib.sha256(log.read_bytes()).hexdigest()},indent=2))
PY
set +e
node tests/sync-v4/k13/run.mjs --build "$proof/build-unverified.json" --expected-revision 59849cee53a9919e657e3a2ed82c8fd1f75595fa
k_exit=$?
python3 tests/sync-v4/p6-review-4/run.py --only regressions --expect-red --deadline "$(date -u -d '2026-10-16 01:45:00 UTC' +%s)"
regression_exit=$?
python3 - "$proof" "$k_exit" "$regression_exit" <<'PY'
import json,pathlib,sys
pathlib.Path(sys.argv[1],'qualification.json').write_text(json.dumps({
 'incident_launcher_exit':int(sys.argv[2]),'expected_regression_counterexamples_exit':int(sys.argv[3]),
 'product_acceptance':False,'note':'a qualified red target is not a product pass'},indent=2))
PY
if [ "$regression_exit" -ne 0 ]; then exit "$regression_exit"; fi
exit "$k_exit"
