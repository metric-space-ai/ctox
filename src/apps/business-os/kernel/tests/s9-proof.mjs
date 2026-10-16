// Bounded proof on the admitted GPU snapshot only. Mutations restore in finally.
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {resolve,join} from 'node:path';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
if(!process.version.startsWith('v20.'))throw Error('Node 20 required');
const out=resolve(process.env.CARGO_TARGET_DIR,'evidence/s9-kernel');mkdirSync(out,{recursive:true});
const receipts=[];
function run(name,cmd,args,{expect=0,timeout=120000}={}){
 const r=spawnSync(cmd,args,{encoding:'utf8',timeout,maxBuffer:24*1024*1024,env:{...process.env,KERNEL_EVIDENCE_DIR:out}}),text=(r.stdout??'')+(r.stderr??'');
 writeFileSync(join(out,name+'.log'),text);console.log('S9_STEP '+name+' exit='+r.status);
 if(expect!==null&&r.status!==expect)throw Error(name+' failed: '+text.slice(-10000));
 return {exit:r.status,signal:r.signal,text};
}
const sha=s=>createHash('sha256').update(s).digest('hex');
const tests=['session.test.mjs','s9-delivery.test.mjs','command-replay.test.mjs','command-state.test.mjs','durability.test.mjs'];
run('kernel-contracts',process.execPath,['--test','--test-concurrency=2',...tests.map(n=>'src/apps/business-os/kernel/tests/'+n)]);
run('declarative-migrations',process.execPath,['src/apps/business-os/rxdb/tests/module-migrations-smoke.mjs']);
run('preserved-collection-guards',process.execPath,['--test','--test-concurrency=2','src/apps/business-os/shared/sync-collection-registry.test.mjs','src/apps/business-os/shared/documents-facade.test.mjs']);
run('http-data-boundary',process.execPath,['src/apps/business-os/scripts/assert-rxdb-only.mjs']);
run('harness-syntax',process.execPath,['--check','src/core/rxdb/tools/browser_rust_smoke.js']);
run('browser-kernel',process.execPath,['src/apps/business-os/kernel/tests/s9-browser.mjs']);
const native=resolve(process.env.CARGO_TARGET_DIR,'debug/examples/v15_wire_daemon');
const nativeProof=run('browser-native-sqlite',process.execPath,['src/apps/business-os/scripts/workjet-command-native.browser.mjs','--native',native,'--output',join(out,'native-browser.json')],{expect:null});
const mutations=[
 ['scope-verification','src/apps/business-os/kernel/session.mjs','if(!await verifyScopeToken(token,{instanceId,userId,epoch:value.epoch,rights_ver:value.rights_ver}))','if(false)',['--test','src/apps/business-os/kernel/tests/session.test.mjs']],
 ['same-origin','src/apps/business-os/kernel/session.mjs',"credentials: 'same-origin',","credentials: 'omit',",['--test','src/apps/business-os/kernel/tests/session.test.mjs']],
 ['terminal-authority','src/apps/business-os/kernel/command-observer.mjs','if(!terminal.has(state?.status))return;','if(false)return;',['--test','src/apps/business-os/kernel/tests/s9-delivery.test.mjs']],
 ['truthful-timing','src/apps/business-os/kernel/command-observer.mjs','if(marks[a]!==undefined&&marks[b]!==undefined)','if(true)',['--test','src/apps/business-os/kernel/tests/s9-delivery.test.mjs']],
 ['lost-receipt-progress','src/apps/business-os/kernel/outbox.mjs',"&& !checkCommandTransition({...st.data,status:'queued'},e.body)) continue;","&& false) continue;",['--test','src/apps/business-os/kernel/tests/s9-delivery.test.mjs']],
 ['migration-preserves-input','src/apps/business-os/kernel/migrate-v3.mjs','const data = cloneValue(record.doc);','const data = {};',['src/apps/business-os/rxdb/tests/module-migrations-smoke.mjs']],
];
for(const [name,file,before,after,args]of mutations){
 const original=readFileSync(file,'utf8');if(original.split(before).length!==2)throw Error('Ambiguous mutation '+name);
 const baseline=run(name+'-baseline',process.execPath,args);
 try{
  writeFileSync(file,original.replace(before,after));const mutant=run(name+'-mutant',process.execPath,args,{expect:null,timeout:20000});
  const killed=mutant.exit!==0&&/not ok|AssertionError/.test(mutant.text)&&!/SyntaxError|ERR_MODULE_NOT_FOUND|ReferenceError/.test(mutant.text);
  receipts.push({name,file,line:original.slice(0,original.indexOf(before)).split('\n').length,before,after,baseline_exit:baseline.exit,mutant_exit:mutant.exit,killed,original_sha256:sha(original)});
  if(!killed)throw Error('Mutation survived or failed structurally '+name);
 }finally{writeFileSync(file,original);writeFileSync(join(out,'mutations.json'),JSON.stringify(receipts,null,2));}
}
const script=Buffer.from(process.env.S9_SCAN_BASE64??'','base64');
if(sha(script)!=='f01e78e6d1f342cccdac80decc16291bc63352ba35decd39f98a7120650be2c8')throw Error('S9 scanner must be byte-identical');
const scanner=join(out,'apply-s9.sh');writeFileSync(scanner,script);
const scan=run('s9-dry-run','bash',[scanner],{expect:null,timeout:180000});
if(![0,1].includes(scan.exit))throw Error('S9 scan did not complete');
const owned=new Set(['src/apps/business-os/shared/sync.js','src/apps/business-os/rxdb/tests/module-migrations-smoke.mjs','src/apps/business-os/scripts/workjet-command-native.browser.mjs','src/apps/business-os/scripts/assert-rxdb-only.mjs','scripts/sync-v4/critical-tests.json',...['command-auth-resilience','command-bus-projection','command-plane-observability','command-submit-atomicity','data-plane-progress-health','collection-lease-lifecycle'].map(n=>'src/apps/business-os/rxdb/tests/'+n+'-smoke.mjs')]);
let entry='';const all=[],kernel=[];
for(const line of scan.text.split('\n')){
 if(/^(BLOCKT|FREI|WEG)/.test(line))entry=line;
 const hit=line.match(/^\s+([^:]+):(\d+):(.*)$/);if(!hit)continue;
 const row={entry,file:hit[1],line:Number(hit[2]),text:hit[3]};all.push(row);
 // Shared harness files have other owners' UI/schema edges. This slice owns
 // their L-21 command/session edges and the dynamic module-migration guard.
 if(row.file.startsWith('src/apps/business-os/kernel/') || owned.has(row.file)&&(/L-21/.test(entry)||row.file.endsWith('/module-migrations-smoke.mjs')))kernel.push(row);
}
if(readFileSync('src/apps/business-os/rxdb/tests/module-migrations-smoke.mjs','utf8').includes('await import(url.href)'))kernel.push({entry:'dynamic migration guard',file:'src/apps/business-os/rxdb/tests/module-migrations-smoke.mjs',line:9,text:'V3 schema imports retained: declarative migration metadata is incomplete'});
const report={native_browser_exit:nativeProof.exit,node:process.version,pass:kernel.length===0,scanner_sha256:sha(script),unchanged_scan_exit:scan.exit,kernel_owned_hits:kernel.length,kernel_hits:kernel,all_remaining_hits:all,mutations:receipts,browser_proofs:['s9-browser/result.json','native-browser.json']};
writeFileSync(join(out,'result.json'),JSON.stringify(report,null,2));console.log('S9_RESULT '+JSON.stringify({pass:report.pass,node:report.node,scanner_sha256:report.scanner_sha256,unchanged_scan_exit:scan.exit,kernel_owned_hits:kernel.length}));
if(!report.pass)console.log('S9_FINDING kernel migration dependency remains; deletion is not approved');

if(nativeProof.exit!==0)throw Error('Native browser fixture is red; see native-browser.json, other evidence was preserved');
