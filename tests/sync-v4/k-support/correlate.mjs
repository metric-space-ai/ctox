#!/usr/bin/env node
// Derive command timings from immutable browser evidence and read-only native truth.
import fs from 'node:fs';
import {resolve,join} from 'node:path';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {owner} from '../../sync-v4-e2e/native.mjs';
if(process.platform!=='linux'||!process.env.BUILD_LANE_BIN)throw Error('Canonical Linux lane required');
const dir=resolve(process.argv[2]);
if(!dir.startsWith(resolve(process.env.BUILD_LANE_BIN,'../artifacts')+'/'))throw Error('Only owned lane artifacts');
const raw=fs.readFileSync(join(dir,'result.json')),report=JSON.parse(raw),config=JSON.parse(fs.readFileSync(join(dir,'config.private.json')));
if(!report.synthetic||!report.terminal||config.owner!==owner||!config.root.startsWith(dir+'/tenant-'))throw Error('Terminal owned synthetic run required');
const marker=JSON.parse(fs.readFileSync(join(config.root,'.sync-v4-e2e.json')));
if(marker.owner!==owner||!marker.synthetic)throw Error('Wrong fixture owner');
const native=spawnSync('python3',['tests/sync-v4-e2e/seed.py','--root',config.root,'--profile',config.profile,'--operation','commands'],{encoding:'utf8',timeout:30000,maxBuffer:16*1024*1024});
if(native.status!==0)throw Error('Read-only native command observation failed');
const truth=JSON.parse(native.stdout),scenario=report.scenarios.find(s=>['K07','K15'].includes(s.id)),answers=scenario?.measured?.answers||scenario?.measured?.commands||[];
const correlated=answers.map(a=>{const d=truth.rows.find(d=>d.id===a.commandId),t=d?.nativeTiming;return {id:a.commandId,browserStartedAtMs:a.startedAtMs,browserObservedMs:a.elapsedMs,status:d?.status,nativeTiming:t||null,nativeDispatchMs:Number.isFinite(t?.native_dispatch_entered)&&Number.isFinite(a.startedAtMs)?t.native_dispatch_entered-a.startedAtMs:null};});
const values=correlated.map(r=>r.nativeDispatchMs).filter(Number.isFinite).sort((a,b)=>a-b),pct=p=>values.length?values[Math.ceil(values.length*p)-1]:null;
const result={at:new Date().toISOString(),sourceResultSha256:createHash('sha256').update(raw).digest('hex'),readerSha256:createHash('sha256').update(fs.readFileSync('tests/sync-v4-e2e/seed.py')).digest('hex'),revision:report.revision,binarySha256:report.binarySha256,
 clock:'Chromium and daemon share the same host kernel wall clock; no cross-host offset inferred',metric:'dispatch entry, not a separately persisted accepted timestamp',nativeTable:truth.table||null,correlated,
 nativeDispatch:{p50:pct(.5),p95:pct(.95),p99:pct(.99),complete:answers.length>0&&values.length===answers.length},browserResponse:scenario?.measured?{p50:scenario.measured.p50,p95:scenario.measured.p95,p99:scenario.measured.p99}:null};
const output=join(dir,'command-correlation.json');if(fs.existsSync(output))throw Error('Never overwrite an earlier correlation');
fs.writeFileSync(output,JSON.stringify(result,null,2));console.log(JSON.stringify({output,nativeDispatch:result.nativeDispatch,browserResponse:result.browserResponse}));
