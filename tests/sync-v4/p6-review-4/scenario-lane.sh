#!/usr/bin/env bash
# Reuse a real large-binary receipt only for checksum positive/negative controls;
# run.py independently rebuilds and binds the requested scenario train.
set -euo pipefail
export PATH=/home/metricspace/.nvm/versions/node/v22.23.1/bin:$PATH
export RUST_TEST_THREADS=2 PYTHONPYCACHEPREFIX="$CARGO_TARGET_DIR/p6-pycache"
receipt="${1:?real producer receipt required}"
train="${2:?immutable scenario train required}"
deadline="${3:?absolute Unix deadline required}"
ctox-prep.sh
node --input-type=module - "$receipt" <<'JS'
import fs from 'node:fs';
import assert from 'node:assert/strict';
import {join} from 'node:path';
import {verifiedBuild} from './tests/sync-v4/k13/build-receipt.mjs';
const receipt=process.argv[2],build=verifiedBuild(receipt),out=join(process.env.CARGO_TARGET_DIR,'p6-review5-checksum');
fs.mkdirSync(out,{recursive:true});
const mutated={...build,binarySha256:(build.binarySha256[0]==='0'?'1':'0')+build.binarySha256.slice(1)};
const bad=join(out,'bad-digest.json');fs.writeFileSync(bad,JSON.stringify(mutated));
assert.throws(()=>verifiedBuild(bad),/Daemon binary checksum differs from producer/);
fs.writeFileSync(join(out,'result.json'),JSON.stringify({receipt,bytes:fs.statSync(build.binary).size,
 binarySha256:build.binarySha256,positiveVerified:true,mutatedDigestRejected:true},null,2));
console.log('P6 large-binary checksum verified; digest mutation rejected');
JS
if [ "${4:-scenarios}" = all ]; then
  python3 tests/sync-v4/p6-review-4/run.py --only all --expect-red --train "$train" --deadline "$deadline"
else
  python3 tests/sync-v4/p6-review-4/run.py --only scenarios --train "$train" --deadline "$deadline"
fi
