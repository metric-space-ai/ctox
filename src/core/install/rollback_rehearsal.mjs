#!/usr/bin/env node
// Real installer regression: no mocked lease, restore, CLI or service manager.
// Reuses the frozen, previously exercised namespace fixture from PR #859.
import fs from 'node:fs';
import {join,resolve,dirname} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {spawn,spawnSync,execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import http from 'node:http';
const self=fileURLToPath(import.meta.url),hash=p=>createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const args=process.argv.slice(2),mode=args.shift();
if(process.platform!=='linux'||!process.env.BUILD_LANE_BIN)throw Error('Canonical Linux lane only');
const run=(cmd,argv,options={})=>{const p=spawnSync(cmd,argv,{stdio:'inherit',timeout:660000,...options});if(p.status!==0||p.error)throw Error(cmd+' failed '+(p.error?.message??p.status));return p;};
const json=(p,v)=>fs.writeFileSync(p,JSON.stringify(v,null,2));
if(mode==='rollback'){
 const [fixture,oldReceipt,candidate,output]=args;
 if(!resolve(output).startsWith(resolve(process.env.BUILD_LANE_BIN,'../artifacts')+'/'))throw Error('Private artifact root required');
 fs.mkdirSync(output,{recursive:false,mode:0o700});
 const harness=join(output,'harness');fs.mkdirSync(harness);
 fs.cpSync(join(fixture,'tests'),join(harness,'tests'),{recursive:true});
 const upgrade=join(harness,'tests/sync-v4/k11/upgrade.mjs');let text=fs.readFileSync(upgrade,'utf8');
 const change=(a,b)=>{if(text.split(a).length!==2)throw Error('Frozen fixture drift: '+a);text=text.replace(a,b);};
 // The rollback MUST execute the candidate installer, not the historical CLI.
 change('spawn(before.binary,','spawn(after.binary,');
 change("quiesce();\n   await cli(['update','rollback']);",`quiesce();
   execFileSync('python3',['-c',${JSON.stringify("import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.execute(\"UPDATE restore_probe SET value='after'\"); c.commit()")},join(stateRoot,'m7-restore-probe.sqlite3')]);
   await cli(['update','rollback']);
   const restored=execFileSync('python3',['-c',"import sqlite3,sys; c=sqlite3.connect('file:'+sys.argv[1]+'?mode=ro',uri=True); print(c.execute('SELECT value FROM restore_probe').fetchone()[0])",join(stateRoot,'m7-restore-probe.sqlite3')],{encoding:'utf8'}).trim();
   if(restored!=='before')throw Error('Rollback did not restore the actual SQLite value');
   record.restoredDatabaseValue=restored;`);
 fs.writeFileSync(upgrade,text);
 const smoke=join(harness,'tests/sync-v4/k11/installer-smoke.mjs');let source=fs.readFileSync(smoke,'utf8');
 const marker='record.root=root;';if(source.split(marker).length!==2)throw Error('Frozen smoke drift');
 source=source.replace(marker,`record.root=root;
 runProbe();
 function runProbe(){const p=spawnSync('python3',['-c',${JSON.stringify("import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.executescript(\"CREATE TABLE restore_probe(value TEXT); INSERT INTO restore_probe VALUES ('before');\"); c.commit()")},join(root,'runtime/m7-restore-probe.sqlite3')]);if(p.status!==0)throw Error('SQLite probe setup failed');}`);
 fs.writeFileSync(smoke,source);
 const log=fs.openSync(join(output,'rehearsal.log'),'w',0o600),startedAt=new Date().toISOString();
 const p=spawnSync(process.execPath,[smoke,oldReceipt,candidate,join(output,'run'),'yes'],{stdio:['ignore',log,log],timeout:660000});fs.closeSync(log);
 const result=JSON.parse(fs.readFileSync(join(output,'run/result.json')));
 const pass=p.status===0&&result.installer?.roundtripExecuted===true&&result.installer?.restoredDatabaseValue==='before'&&result.dataPreserved===true&&result.controller?.groupAbsent===true;
 json(join(output,'verdict.json'),{startedAt,finishedAt:new Date().toISOString(),exit:p.status,pass,result:join(output,'run/result.json'),candidateSha256:hash(JSON.parse(fs.readFileSync(candidate)).binary),scope:'real rollback, SQLite restore and forward recovery; not full M7'});
 console.log(JSON.stringify({output,pass,exit:p.status}));process.exitCode=pass?0:1;
}else if(mode==='static'){
 const [fixture,profileReceipt,output,slot]=args,profile=JSON.parse(fs.readFileSync(profileReceipt));
 if(!resolve(output).startsWith(resolve(process.env.BUILD_LANE_BIN,'../artifacts')+'/'))throw Error('Private artifact root required');
 fs.mkdirSync(output,{recursive:false,mode:0o700});
 const config={slot:slot?resolve(slot):null,owner:'01a087a0-169a',uid:process.getuid(),gid:process.getgid(),root:profile.root,output,fixture:resolve(fixture),profileReceipt:resolve(profileReceipt),runner:self,node:process.execPath,path:process.env.PATH,tmpdir:process.env.TMPDIR,laneBin:process.env.BUILD_LANE_BIN,host:Object.fromEntries(['net','mnt','pid','cgroup'].map(n=>[n,fs.readlinkSync('/proc/self/ns/'+n)]))};
 // This file is invoked with the config path by the frozen namespace helper.
 config.runner=join(output,'entry.mjs');fs.writeFileSync(config.runner,`process.argv.splice(2,0,'inside-static'); await import(${JSON.stringify(pathToFileURL(self).href)});`);
 const file=join(output,'config.json');json(file,config);
 const child=spawn('sudo',['-n','unshare','--net','--mount','--pid','--fork','--mount-proc','python3',join(fixture,'tests/sync-v4/k11/m7-manager-probe.py'),'--setup',file],{stdio:'inherit',detached:true});
 const receipt={owner:config.owner,pid:child.pid,pgid:child.pid,startedAt:new Date().toISOString(),stop:'terminal or 180 seconds'};json(join(output,'controller.json'),receipt);
 const timer=setTimeout(()=>{try{process.kill(-child.pid,'SIGKILL');}catch{}},180000);
 const code=await new Promise((r,j)=>{child.once('exit',r);child.once('error',j);});clearTimeout(timer);
 try{process.kill(-child.pid,'SIGKILL');}catch(e){if(e.code!=='ESRCH')throw e;}
 receipt.exit=code;receipt.finishedAt=new Date().toISOString();
 for(let i=0;i<100;i++){try{process.kill(-child.pid,0);}catch(e){if(e.code==='ESRCH'){receipt.groupAbsent=true;break;}}await new Promise(r=>setTimeout(r,50));}
 json(join(output,'controller.json'),receipt);process.exitCode=code===0&&receipt.groupAbsent?0:1;
}else if(mode==='inside-static'){
 const c=JSON.parse(fs.readFileSync(args[0])),profile=JSON.parse(fs.readFileSync(c.profileReceipt));
 const {Native,until}=await import(pathToFileURL(join(c.fixture,'tests/sync-v4-e2e/native.mjs')));
 const root=join(profile.root,'managed/current'),binary=join(root,'bin/ctox');
 const config={root,binary,source:root,output:c.output,port:18766,signalingPort:18767};
 const native=new Native(config);native.env.CTOX_STATE_ROOT=join(profile.root,'runtime');native.env.CTOX_INSTALL_ROOT=join(profile.root,'managed');native.env.CTOX_WEBRTC_UDP_BIND_ADDR='127.0.0.1:0';native.env.HOME=process.env.HOME;
 if(c.slot){
  const version='0.1.46-beta.106',dest=join(profile.root,'runtime/business-os-shell/slots',version);fs.mkdirSync(dirname(dest),{recursive:true});if(!fs.existsSync(dest))fs.cpSync(c.slot,dest,{recursive:true});
  // Private fixture metadata only; signed bytes are still verified by the real server.
  execFileSync('python3',['-c',"import sqlite3,json,sys; c=sqlite3.connect(sys.argv[1]); c.execute(\"INSERT INTO business_os_shell_update_state(singleton,state_json,updated_at) VALUES (1,?,datetime('now')) ON CONFLICT(singleton) DO UPDATE SET state_json=excluded.state_json,updated_at=excluded.updated_at\",(json.dumps(json.load(open(sys.argv[2]))),)); c.commit()",join(profile.root,'runtime/business-os.sqlite3'),join(c.slot,'../fixture-state.json')]);
 }

 const result={startedAt:new Date().toISOString(),binarySha256:hash(binary),rows:[],pass:false};
 const request=path=>new Promise(resolve=>{
  const start=performance.now(),row={path,startedAt:new Date().toISOString(),headersMs:null,bodyMs:null,bytes:0};
  const req=http.get('http://127.0.0.1:18766'+path,{agent:false},res=>{row.status=res.statusCode;row.headersMs=performance.now()-start;res.on('data',b=>row.bytes+=b.length);res.on('end',()=>{row.bodyMs=performance.now()-start;resolve(row);});res.on('error',e=>{row.error=e.message;resolve(row);});});
  req.setTimeout(12000,()=>req.destroy(Error('HTTP phase deadline')));req.on('error',e=>{row.error=e.message;row.totalMs=performance.now()-start;resolve(row);});
 });
 try{
  native.start('signaling',process.execPath,[resolve(dirname(self),'../rxdb/tools/local_signaling_server.js'),'18767'],{...native.env,SIGNALING_HOST:'127.0.0.1',SIGNALING_PORT:'18767'});native.serve();await until(async()=>{try{const r=await fetch('http://127.0.0.1:18766/app.js?v=m7-phase',{signal:AbortSignal.timeout(2000)});await r.arrayBuffer();return r.ok;}catch{return false;}},30000,'isolated static server');
  for(let i=0;i<20;i++){const row=await request('/');result.rows.push(row);json(join(c.output,'static.json'),result);if(row.error){
    // Stack names only, no argument/credential values. Exact owned child only.
    const pid=native.daemon.pid;const diag=spawnSync('sudo',['-n','gdb','-q','-batch','-ex','set print frame-arguments none','-ex','thread apply all bt 12','-p',String(pid)],{encoding:'utf8',timeout:10000,maxBuffer:2*1024*1024});fs.writeFileSync(join(c.output,'stacks.log'),diag.stdout+'\n'+diag.stderr);break;
   }}
  result.asset=await request('/app.js?v=m7-phase');
  const times=result.rows.filter(r=>r.bodyMs!=null).map(r=>r.bodyMs).sort((a,b)=>a-b);result.p99Ms=result.rows.length===20&&times.length===20?times[Math.ceil(times.length*.99)-1]:null;result.p99Reason=result.p99Ms==null?'incomplete or right-censored sample':null;
  result.pass=result.rows.length===20&&result.rows.every(r=>r.status===200&&!r.error&&r.bodyMs<500);
 }catch(error){result.error=error.message;}finally{await native.close();result.finishedAt=new Date().toISOString();json(join(c.output,'static.json'),result);}
 process.exitCode=result.pass?0:1;
}else throw Error('Expected rollback|static|inside-static');
