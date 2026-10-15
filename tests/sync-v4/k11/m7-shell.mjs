// Exercise the production signed-release stager against an owned installation.
// beta.106 is a reference for the delivery mechanism, not a signed V4 app set.
import fs from 'node:fs';
import {join,resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {owner} from '../../sync-v4-e2e/native.mjs';
const [evidence,directory,version='0.1.46-beta.106']=process.argv.slice(2);
if(process.platform!=='linux'||!process.env.BUILD_LANE_BIN)throw Error('Canonical Linux lane required');
const input=JSON.parse(fs.readFileSync(evidence)),root=resolve(input.root),output=resolve(directory);
const allowed=resolve(process.env.BUILD_LANE_BIN,'../artifacts')+'/';
const mark=JSON.parse(fs.readFileSync(join(root,'.sync-v4-e2e.json')));
if(!root.startsWith(allowed)||!output.startsWith(allowed)||mark.owner!==owner||mark.synthetic!==true||!input.installerExecuted||!input.controller?.groupAbsent)throw Error('Quiescent owned installed profile required');
if(!/^\d+\.\d+\.\d+-beta\.\d+$/.test(version))throw Error('Exact beta version required');
fs.mkdirSync(output,{mode:0o700});
const managed=join(root,'managed'),current=fs.realpathSync(join(managed,'current')),binary=join(current,'bin/ctox');
const sha=path=>createHash('sha256').update(fs.readFileSync(path)).digest('hex');
if(sha(binary)!==input.installer.activeBinarySha256)throw Error('Installed binary differs');
const home=join(output,'home');fs.mkdirSync(home,{mode:0o700});
const env={PATH:process.env.PATH,HOME:home,TMPDIR:process.env.TMPDIR,LANG:'C.UTF-8',CTOX_ROOT:current,CTOX_INSTALL_ROOT:managed,CTOX_STATE_ROOT:join(root,'runtime'),XDG_CONFIG_HOME:join(home,'.config'),XDG_CACHE_HOME:join(home,'.cache'),XDG_RUNTIME_DIR:join(home,'run'),DBUS_SESSION_BUS_ADDRESS:'unix:path=/nonexistent-m7-private-bus'};
const result={schema:'ctox.m7.signed-shell.v1',owner,startedAt:new Date().toISOString(),binarySha256:sha(binary),version,referenceOnly:true,v4ShellAccepted:false,calls:[],pass:false,terminal:false};
const save=()=>fs.writeFileSync(join(output,'shell.json'),JSON.stringify(result,null,2));save();
function cli(args){const at=Date.now(),index=result.calls.length,p=spawnSync(binary,['business-os','shell-update',...args,'--root',current],{cwd:current,env,encoding:'utf8',timeout:240000,maxBuffer:4*1024*1024});
 fs.writeFileSync(join(output,'cli-'+index+'.log'),(p.stdout??'')+(p.stderr??''),{mode:0o600});
 const call={args,startedAtMs:at,durationMs:Date.now()-at,exit:p.status,error:p.error?.message};result.calls.push(call);save();
 if(p.status!==0||p.error)throw Error('Official shell '+args.join(' ')+' failed');
 const start=p.stdout.indexOf('{');if(start<0)throw Error('Shell CLI returned no JSON status');return JSON.parse(p.stdout.slice(start));}
try{
 result.before=cli(['status']);
 result.staged=cli(['stage','--version',version]);
 result.activated=cli(['activate']);
 result.after=cli(['status']);
 const active=result.after.activeVersion??result.after.active_version;
 if(active!==version)throw Error('Official status did not select the requested signed slot');
 // The selected state root is explicit; no direct slot-state SQL mutation.
 const candidates=[join(root,'runtime/business-os-shell/slots',version),join(root,'runtime/business-os/shell/slots',version)];
 const slot=candidates.find(p=>fs.existsSync(join(p,'.ctox-shell-release.v2.json')));
 if(!slot)throw Error('Verified signed slot not found in the isolated state root');
 const manifest=join(slot,'.ctox-shell-release.v2.json'),release=JSON.parse(fs.readFileSync(manifest));
 result.slot={path:slot,manifestSha256:sha(manifest),artifact:release.payload.artifact,signingKeyId:release.payload.signing_key_id,sourceCommit:release.payload.source_commit??null};
 result.pass=true;
}catch(error){result.error=error.message;}
finally{result.finishedAt=new Date().toISOString();result.terminal=true;save();}
console.log(JSON.stringify({output,pass:result.pass,version,referenceOnly:true,error:result.error}));process.exitCode=result.pass?0:1;
