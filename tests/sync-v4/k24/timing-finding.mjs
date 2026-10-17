#!/usr/bin/env node
// Condense actual upstream measures without rerunning or changing its criteria.
import fs from 'node:fs';
import {createHash} from 'node:crypto';
const [input,output]=process.argv.slice(2),bytes=fs.readFileSync(input),row=JSON.parse(bytes);
if(row.id!=='K24'||row.target!=='bind'||row.terminal!==true||row.cleanup!=='owned groups absent')throw Error('Terminal bind evidence required');
const m=row.measured??{},limitMs=2000;
const steps=[
 ['publication-to-remount-A',m.v2?.remountMs?.A,'tests/sync-v4/k24/run.mjs:437-455'],
 ['publication-to-remount-B',m.v2?.remountMs?.B,'tests/sync-v4/k24/run.mjs:437-455'],
 ['command-done-visible-B',m.agent?.commandDoneInB_ms,'tests/sync-v4/k24/run.mjs:585-593'],
].map(([step,ms,location])=>({step,ms,limitMs,status:Number.isFinite(ms)?ms<=limitMs?'green':'red':'not_run',location}));
const measured=steps.filter(r=>Number.isFinite(r.ms)).sort((a,b)=>b.ms-a.ms);
if(!measured.length)throw Error('No measured timing step');
const result={schema:'ctox.k24.timing-finding.v1',sourceSha256:createHash('sha256').update(bytes).digest('hex'),source:input,originCommit:row.originCommit,snapshot:row.snapshot,criterion:'regressionskatalog.md K24 / E2 activation <= 2s; upstream criterion 8 command visibility <= 2s',steps,slowest:measured[0],publication:{acceptedClickToFeedbackMs:m.v2busy?.lagMs,retries:m.v2busy?.retries},owner:'ctox sync v4 (kernel/fake core); Shell v4 receives measured locations',cause:'Not proven by aggregate timings. Kernel 2s reconciliation is a candidate, not a measured attribution.',nativeAcceptance:false};
fs.writeFileSync(output,JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result));
