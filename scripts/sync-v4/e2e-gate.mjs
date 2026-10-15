#!/usr/bin/env node
import fs from 'node:fs';
import {join,resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {catalogAccepted} from '../../tests/sync-v4-e2e/criteria.mjs';
if(process.platform!=='linux'||!process.env.BUILD_LANE_BIN)throw Error('Canonical Linux lane required');
const expected=['K01','K07','K11','K15','K02','K24'];
const declared=fs.readdirSync('tests/sync-v4').filter(n=>/^k[0-9]+$/.test(n)).map(n=>'K'+n.slice(1));
if(!declared.length)throw Error('ZERO scenarios discovered');
if(expected.some(id=>!declared.includes(id)))throw Error('Missing mandatory catalog scenario');
const output=resolve(process.env.BUILD_LANE_BIN,'../artifacts/e2e-k-'+Date.now());
const args=process.argv.slice(2);
if(args.includes('--output')||args.includes('--scenario')||args.includes('--catalog'))throw Error('Gate owns catalog/output; no scenario subset');
const p=spawnSync(process.execPath,['scripts/sync-v4/e2e-run.mjs',...args,'--catalog','yes','--output',output],{stdio:'inherit',timeout:35*60*1000});
const file=join(output,'result.json');
if(!fs.existsSync(file))throw Error('ZERO scenarios ran: no result.json');
const report=JSON.parse(fs.readFileSync(file));
if(!report.scenarios?.length)throw Error('ZERO scenarios ran');
console.log('K-no. | red/green | evidence');
for(const id of expected){const row=report.scenarios.find(s=>s.id===id);const ok=row?.status==='measured'&&row.pass&&row.acceptanceComplete;
 console.log(`${id} | ${ok?'green':'red'} | ${file}#${id} (${row?.status||'missing'}; complete=${row?.acceptanceComplete===true})`);}
const accepted=catalogAccepted(report,expected)&&p.status===0;
fs.writeFileSync(join(output,'gate.json'),JSON.stringify({at:new Date().toISOString(),expected,actuallyStarted:report.scenarios.map(s=>s.id),accepted,processExit:p.status,evidence:file},null,2));
process.exitCode=accepted?0:1;
