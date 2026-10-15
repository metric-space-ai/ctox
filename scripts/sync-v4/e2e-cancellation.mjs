#!/usr/bin/env node
// Real daemon/browser teardown check. No substitute peer or store is involved.
import fs from 'node:fs';
import {join,resolve} from 'node:path';
import {spawn} from 'node:child_process';
if(process.platform!=='linux'||!process.env.BUILD_LANE_BIN)throw Error('Canonical Linux lane required');
const args=process.argv.slice(2);
if(args.some(x=>['--output','--scenario','--catalog','--seconds','--profile'].includes(x)))throw Error('Cancellation driver owns bounded scenario options');
const output=resolve(process.env.BUILD_LANE_BIN,'../artifacts/e2e-cancellation-'+Date.now());
const log=fs.openSync(output+'.log','w',0o600);
const child=spawn(process.execPath,['scripts/sync-v4/e2e-run.mjs',...args,'--catalog','yes','--scenario','K01','--profile','small','--seconds','120','--output',output],{stdio:['ignore',log,log]});
fs.closeSync(log);
let code=null,signal=null;const exited=new Promise(r=>child.once('exit',(c,s)=>{code=c;signal=s;r();}));
const until=async(fn,ms)=>{const end=Date.now()+ms;while(Date.now()<end){if(fn())return;await new Promise(r=>setTimeout(r,50));}throw Error('Cancellation lifecycle deadline');};
let cancelledAt;
try{
 await until(()=>{try{return JSON.parse(fs.readFileSync(join(output,'result.json'))).scenarios.some(s=>s.observations?.some(o=>o.phase==='offline-local-writes'));}catch{return false;}},90000);
 cancelledAt=Date.now();child.kill('SIGTERM');
 await Promise.race([exited,new Promise((_,reject)=>setTimeout(()=>reject(Error('Cancellation exceeded 12 seconds')),12000))]);
 const receipt=JSON.parse(fs.readFileSync(join(output,'controller.json')));
 const pass=receipt.groupAbsent===true&&Date.now()-cancelledAt<=12000;
 const result={at:new Date().toISOString(),output,code,signal,cancelledAt,elapsedMs:Date.now()-cancelledAt,groupAbsent:receipt.groupAbsent,pass};
 fs.writeFileSync(output+'-proof.json',JSON.stringify(result,null,2));
 if(!pass)throw Error('Real namespace cancellation cleanup failed');console.log(JSON.stringify(result));
}finally{
 if(code===null&&signal===null){child.kill('SIGTERM');await Promise.race([exited,new Promise(r=>setTimeout(r,12000))]);}
}
