#!/usr/bin/env node
import fs from 'node:fs';
import { execFileSync } from 'node:child_process';
if(process.platform!=='linux'||!process.env.CARGO_TARGET_DIR||!process.env.TMPDIR)throw Error('Linux lane required');
for(const dir of ['tests/sync-v4-e2e','scripts/sync-v4'])for(const file of fs.readdirSync(dir))
 if(file.endsWith('.mjs') && (dir.startsWith('tests')||file.startsWith('e2e-')))execFileSync(process.execPath,['--check',dir+'/'+file],{stdio:'inherit'});
execFileSync('python3',['-c','import ast,pathlib; ast.parse(pathlib.Path("tests/sync-v4-e2e/seed.py").read_text())'],{stdio:'inherit'});
for(const dir of fs.readdirSync('tests/sync-v4').filter(n=>/^k(?:[0-9]+|-support)$/.test(n)))for(const f of fs.readdirSync('tests/sync-v4/'+dir).filter(f=>/\.(mjs|cjs)$/.test(f)))execFileSync(process.execPath,['--check','tests/sync-v4/'+dir+'/'+f],{stdio:'inherit'});
execFileSync('bash',['-n','scripts/sync-v4/gate-regression.sh'],{stdio:'inherit'});
execFileSync('python3',['-c','import ast,pathlib; ast.parse(pathlib.Path("tests/sync-v4/k11/migration-proof.py").read_text())'],{stdio:'inherit'});
console.log('Harness source syntax is valid; no product acceptance claimed');
