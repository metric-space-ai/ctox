#!/usr/bin/env node
// Local Shell-K24 adapter. Env values are explicitly forwarded as arguments,
// not via SSH AcceptEnv or string concatenation of untrusted shell fragments.
import {spawnSync} from 'node:child_process';
const [descriptor,operation]=process.argv.slice(2);
const {readFileSync}=await import('node:fs');
const c=JSON.parse(readFileSync(descriptor,'utf8'));
if(!['query','fingerprint'].includes(operation)||c.owner!=='01a087a0-169a-7e23-ba3d-71352256cbfb'||c.synthetic!==true)throw Error('Owned K24 target descriptor required');
if(!['ts-gpu3','lan-gpu3'].includes(c.host))throw Error('K24 target is private gpu3 only');
if(!c.root.startsWith('/mnt/nvme1/build-lane/artifacts/'))throw Error('Private lane profile only');
const quote=s=>"'"+String(s).replaceAll("'","'\\''")+"'";
const cmd=['env'];
if(operation==='query'){
 for(const key of ['K24_QUERY','K24_APP','K24_RECORD'])if(process.env[key]!==undefined){
  if(process.env[key].length>256)throw Error('Oversized K24 selector');
  cmd.push(key+'='+process.env[key]);
 }
}
cmd.push('python3',c.adapter,operation,'--root',c.root);
if(operation==='query'){
 cmd.push('--database',c.database);
 if(c.auditDatabase)cmd.push('--audit-database',c.auditDatabase);
}
else{
 cmd.push('--pid',String(c.pid));
 for(const path of c.configs)cmd.push('--config',path);
}
const r=spawnSync('ssh',['-T','-o','BatchMode=yes','-o','ConnectTimeout=10',c.host,cmd.map(quote).join(' ')],{encoding:'utf8',timeout:20000,stdio:['ignore','pipe','pipe']});
if(r.error)throw r.error;
if(r.status!==0){process.stderr.write(r.stderr);process.exit(r.status??1);}
const value=JSON.parse(r.stdout);
process.stdout.write(JSON.stringify(value)+'\n');
