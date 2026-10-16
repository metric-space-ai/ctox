// Bounded real M3 against the exact source with the published DCEP fix merged.
// Fetch/merge is explicit before admission; no stale branch gate or polling.
import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
const root=process.cwd(),base=path.resolve(process.env.CARGO_TARGET_DIR,'evidence/m3-lane4');
fs.mkdirSync(base,{recursive:true});
const out=fs.mkdtempSync(path.join(base,'run-'));
const report={started:new Date().toISOString(),status:'running',required_dcep_fix:'2425e9292',commands:1000,drops:5,forced_turn:true,out};
let scratch,worktree;
function run(command,args,options={}){
 const r=spawnSync(command,args,{cwd:root,encoding:'utf8',timeout:180000,maxBuffer:8*1024*1024,...options});
 if(r.status!==0)throw Error(`${command} ${args.join(' ')}: ${r.error??r.stderr??r.status}`);
 return r.stdout?.trim()??'';
}
try{
 report.kernel_head=run('git',['rev-parse','HEAD']);
 run('git',['merge-base','--is-ancestor',report.required_dcep_fix,report.kernel_head]);
 // Isolate mutation runs from the admitted source and capture the full overlay.
 scratch=fs.mkdtempSync(path.join(out,'compose-'));
 const env={...process.env,RUST_TEST_THREADS:'2',GIT_INDEX_FILE:path.join(scratch,'index'),GIT_AUTHOR_NAME:'CTOX M3 bench',GIT_AUTHOR_EMAIL:'m3-bench@localhost',GIT_COMMITTER_NAME:'CTOX M3 bench',GIT_COMMITTER_EMAIL:'m3-bench@localhost'};
 run('git',['read-tree','HEAD'],{env});run('git',['add','-A'],{env});
 report.kernel_tree=run('git',['write-tree'],{env});
 const snapshot=run('git',['commit-tree',report.kernel_tree,'-p',report.kernel_head],{env,input:'Disposable exact-source M3 snapshot\n'});
 worktree=path.join(scratch,'source');run('git',['worktree','add','--detach',worktree,snapshot]);
 const fixtureLock='tests/sync-v4-e2e-path/native/Cargo.lock';
 // Preserve compatible RTC pins while reconciling new path-crate dependencies.
 run('cargo',['metadata','--offline','--format-version','1','--manifest-path','tests/sync-v4-e2e-path/native/Cargo.toml'],{cwd:worktree});
 run('git',['add','--',fixtureLock],{cwd:worktree});
 report.fixture_lock_sha256=createHash('sha256').update(fs.readFileSync(path.join(worktree,fixtureLock))).digest('hex');
 report.fixture_lock_file=path.join(out,'fixture.Cargo.lock');fs.copyFileSync(path.join(worktree,fixtureLock),report.fixture_lock_file);
 report.composed_tree=run('git',['write-tree'],{cwd:worktree});
 if(run('git',['status','--porcelain'],{cwd:worktree}))run('git',['-c','user.name=CTOX M3 bench','-c','user.email=m3-bench@localhost','commit','-m','Disposable M3 fixture lock reconciliation'],{cwd:worktree});
 report.composed_head=run('git',['rev-parse','HEAD'],{cwd:worktree});
 fs.writeFileSync(path.join(base,'readiness.json'),JSON.stringify(report,null,2));
 const result=spawnSync('timeout',['--kill-after=10s','1200s','bash','tests/sync-v4-e2e-path/run-step1.sh'],{cwd:worktree,env:{...process.env,E2E_OUT:path.join(out,'native-path'),E2E_MUTATIONS_DIR:path.join(out,'mutations')},stdio:'inherit',timeout:1220000});
 report.exit=result.status;report.signal=result.signal;
 const measurementFile=path.join(out,'native-path/result.json');
 if(!fs.existsSync(measurementFile))throw Error(`Native build/bench exited ${result.status??result.signal} before producing a measurement; inspect the lane log.`);
 const measurement=JSON.parse(fs.readFileSync(measurementFile,'utf8'));
 report.status=result.status===0&&measurement.pass?'passed':'failed';report.measurement=measurementFile;
 for(const key of ['m3','first','push','command','steadyCommand','control'])report[key]=measurement[key];
 if(report.status!=='passed'){report.reason=measurement.error??'Mutation or source validation failed; inspect lane evidence.';process.exitCode=1;}
}catch(error){report.status='failed';report.reason=String(error.stack??error);process.exitCode=1;}
finally{
 if(worktree){const cleanup=spawnSync('git',['worktree','remove',worktree],{cwd:root,encoding:'utf8',timeout:30000});report.worktree_removed=cleanup.status===0;if(!report.worktree_removed)report.recovery_path=worktree;}
 if(scratch&&(!worktree||report.worktree_removed))fs.rmSync(scratch,{recursive:true,force:true});
 report.finished=new Date().toISOString();for(const dest of [out,base])fs.writeFileSync(path.join(dest,'readiness.json'),JSON.stringify(report,null,2));
 // Keep per-command receipts in the evidence file, not in the console summary.
 const {ids,results,dropResults,...m3}=report.m3??{};
 console.log('M3_READINESS '+JSON.stringify({...report,m3:report.m3?m3:undefined}));
}
