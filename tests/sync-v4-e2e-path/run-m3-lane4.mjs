// One-shot readiness gate and real M3 bench; no polling or synthetic native path.
// Invoke via gpu-build-run.sh. A later invocation picks up a pushed Instances fix.
import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
const root=process.cwd(),out=path.resolve(process.env.CARGO_TARGET_DIR,'evidence/m3-lane4');
fs.mkdirSync(out,{recursive:true});
const report={started:new Date().toISOString(),status:'not_run',baseline:'883ec4c403b181c9c0605bfc77fd5413d3535a94',branch:'v4/lane4-cas',commands:1000,drops:5,forced_turn:true};
let scratch,index,worktree;
function run(command,args,options={}){
 const r=spawnSync(command,args,{cwd:root,encoding:'utf8',timeout:180000,maxBuffer:8*1024*1024,...options});
 if(r.status!==0)throw Error(`${command} ${args.join(' ')}: ${r.error??r.stderr??r.status}`);
 return r.stdout?.trim()??'';
}
try{
 run('git',['fetch','--no-tags','origin',`refs/heads/${report.branch}:refs/remotes/origin/${report.branch}`]);
 report.lane4_sha=run('git',['rev-parse',`refs/remotes/origin/${report.branch}`]);
 run('git',['merge-base','--is-ancestor',report.baseline,report.lane4_sha]);
 report.kernel_head=run('git',['rev-parse','HEAD']);
 report.fix_paths=run('git',['diff','--name-only',report.baseline,report.lane4_sha,'--','src/core/rxdb/patches/rtc/src/peer_connection/mod.rs','src/core/rxdb/patches/rtc/src/peer_connection/handler/datachannel.rs','src/core/rxdb/patches/webrtc/src/peer_connection/mod.rs','src/core/rxdb/patches/webrtc/src/peer_connection/driver.rs']).split('\n').filter(Boolean);
 if(!report.fix_paths.length){
  report.reason='Instances channel fix not pushed on v4/lane4-cas since the measured bootstrap failure; M3 was not started.';
 }else{
  // Capture the exact lane overlay without touching the source index or branch.
  scratch=fs.mkdtempSync(path.join(out,'compose-'));
  index=path.join(scratch,'index');
  const env={...process.env,RUST_TEST_THREADS:'2',GIT_INDEX_FILE:index,GIT_AUTHOR_NAME:'CTOX M3 bench',GIT_AUTHOR_EMAIL:'m3-bench@localhost',GIT_COMMITTER_NAME:'CTOX M3 bench',GIT_COMMITTER_EMAIL:'m3-bench@localhost'};
  report.kernel_head=run('git',['rev-parse','HEAD']);
  run('git',['read-tree','HEAD'],{env});run('git',['add','-A'],{env});
  report.kernel_tree=run('git',['write-tree'],{env});
  const snapshot=run('git',['commit-tree',report.kernel_tree,'-p',report.kernel_head],{env,input:'Disposable exact-source M3 snapshot\n'});
  worktree=path.join(scratch,'source');
  run('git',['worktree','add','--detach',worktree,snapshot]);
  run('git',['merge','--no-commit','--no-ff',report.lane4_sha],{cwd:worktree});
  // Reconcile this fixture's existing lock without upgrading unrelated RTC crates.
  // generate-lockfile discards their compatible pins and can break the patched SDK.
  const fixtureLock='tests/sync-v4-e2e-path/native/Cargo.lock';
  run('cargo',['metadata','--offline','--format-version','1','--manifest-path','tests/sync-v4-e2e-path/native/Cargo.toml'],{cwd:worktree});
  run('git',['add','--',fixtureLock],{cwd:worktree});
  report.fixture_lock_sha256=createHash('sha256').update(fs.readFileSync(path.join(worktree,fixtureLock))).digest('hex');
  report.fixture_lock_file=path.join(out,'fixture.Cargo.lock');
  fs.copyFileSync(path.join(worktree,fixtureLock),report.fixture_lock_file);
  report.composed_tree=run('git',['write-tree'],{cwd:worktree});
  if(run('git',['status','--porcelain'],{cwd:worktree}))run('git',['-c','user.name=CTOX M3 bench','-c','user.email=m3-bench@localhost','commit','-m','Disposable composed M3 source'],{cwd:worktree});
  report.composed_head=run('git',['rev-parse','HEAD'],{cwd:worktree});
  report.status='running';fs.writeFileSync(path.join(out,'readiness.json'),JSON.stringify(report,null,2));
  const result=spawnSync('timeout',['--kill-after=10s','1200s','bash','tests/sync-v4-e2e-path/run-step1.sh'],{cwd:worktree,env:{...process.env,E2E_OUT:path.join(out,'native-path'),E2E_MUTATIONS_DIR:path.join(out,'mutations')},stdio:'inherit',timeout:1220000});
  report.exit=result.status;report.signal=result.signal;
  const measurementFile=path.join(out,'native-path/result.json');
  if(!fs.existsSync(measurementFile))throw Error(`Native build/bench exited ${result.status??result.signal} before producing a measurement; inspect the lane log.`);
  const measurement=JSON.parse(fs.readFileSync(measurementFile,'utf8'));
  report.status=result.status===0&&measurement.pass?'passed':'failed';
  report.measurement=path.join(out,'native-path/result.json');
  report.m3=measurement.m3;report.first=measurement.first;report.push=measurement.push;report.command=measurement.command;report.steadyCommand=measurement.steadyCommand;
  if(report.status!=='passed')process.exitCode=1;
 }
}catch(error){report.status='failed';report.reason=String(error.stack??error);process.exitCode=1;}
finally{
 if(worktree){const cleanup=spawnSync('git',['worktree','remove',worktree],{cwd:root,encoding:'utf8',timeout:30000});report.worktree_removed=cleanup.status===0;if(!report.worktree_removed)report.recovery_path=worktree;}
 if(scratch&&(!worktree||report.worktree_removed))fs.rmSync(scratch,{recursive:true,force:true});
 report.finished=new Date().toISOString();fs.writeFileSync(path.join(out,'readiness.json'),JSON.stringify(report,null,2));
 console.log('M3_READINESS '+JSON.stringify(report));
}
