#!/usr/bin/env bash
set -euo pipefail
export PATH=/mnt/nvme1/build-lane/cache/node-v24.13.1-linux-x64/bin:$PATH
export PLAYWRIGHT_BROWSERS_PATH=/mnt/nvme1/build-lane/deps/shell-browser-collection-auth/browsers
export CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2 UV_THREADPOOL_SIZE=2
node --test scripts/installed-sync-acceptance-telemetry.test.mjs
node --check scripts/run-installed-sync-browser.mjs
python3 -c 'import ast,pathlib; ast.parse(pathlib.Path("scripts/prepare-installed-sync-tenant.py").read_text()); print("preparation syntax valid")'
base=/mnt/nvme1/ctox-acceptance/devops-sync-c7-recheck-20261012-r4
publication=/mnt/nvme1/ctox-acceptance/devops-published-c7-20261012
mkdir -p "$publication"
url=https://github.com/metric-space-ai/ctox/releases/download/native-main-c7ff9ac3af41
curl --fail --location --silent --show-error --connect-timeout 10 --max-time 180 "$url/ctox-linux-x64.tar.gz" -o "$publication/ctox-linux-x64.tar.gz"
curl --fail --location --silent --show-error --connect-timeout 10 --max-time 30 "$url/build-provenance.json" -o "$publication/build-provenance.json"
python3 - "$publication" <<'PY'
import hashlib,json,pathlib,sys,datetime
p=pathlib.Path(sys.argv[1]);a=p/'ctox-linux-x64.tar.gz';v=json.loads((p/'build-provenance.json').read_text())
source='c7ff9ac3af415d7007b1a9fde57ba274e4ee7f65';binary='3d0034fbf94447bfd65e3a4b68d34788fa7ca81914c7eb32e67d53855192155b'
d=hashlib.sha256()
with a.open('rb') as f:
 for b in iter(lambda:f.read(1048576),b''):d.update(b)
sha=d.hexdigest();assert sha=='6b32483036c2da5ec2f6c9431db12e00e7b0d518410625e319760751165b3cae'
assert v['source']==source and v['native_binary_sha256']==binary
r={'kind':'published-release','release':'native-main-c7ff9ac3af41','source':source,'native_binary_sha256':binary,'publicationVerified':True,'provenance':v,'measuredAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'verification':'Downloaded canonical release assets; archive digest matches GitHub asset digest. Original build checks not rerun.','artifacts':{'ctox-linux-x64.tar.gz':{'path':str(a),'sha256':sha}}}
(p/'publication.json').write_text(json.dumps(r,indent=2)+'\n')
print(json.dumps({'packageSha256':sha,'nativeSha256':binary,'source':source}))
PY
python3 scripts/prepare-installed-sync-tenant.py --producer "$publication/publication.json" --acceptance-base "$base" --port 19867 --host gpu3 --workjet-revision '0.0.89 (independent Info.plist read 2026-10-12)' --source c7ff9ac3af415d7007b1a9fde57ba274e4ee7f65
cp "$publication/publication.json" "$base/publication.json"
python3 - "$base/runner.private.json" <<'PY'
import json,pathlib,sys,subprocess
p=pathlib.Path(sys.argv[1]);c=json.loads(p.read_text());c['goals']=[5];c['runnerRevision']=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();c['runnerIncludesWorkingDiff']=True
p.write_text(json.dumps(c,indent=2)+'\n');p.chmod(0o600)
PY
exec timeout --signal=TERM --kill-after=15s 1200s node scripts/run-installed-sync-browser.mjs "$base/runner.private.json" /mnt/nvme1/build-lane/deps/shell-browser-collection-auth/node_modules/playwright --sync-only
