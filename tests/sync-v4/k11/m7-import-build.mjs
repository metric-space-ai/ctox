// Compile an explicit, disposable composition, without changing product source.
// fb2f6ac15 contains #828 files but does not export import or its CAS dependency.
// This limited probe excludes the unused file adapter, and cannot certify files.
import fs from 'node:fs';
import {join,resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
const [directory,archivePath,custodyPath]=process.argv.slice(2),output=resolve(directory);
if(process.platform!=='linux'||!process.env.BUILD_LANE_BIN||!process.env.CARGO_TARGET_DIR)throw Error('Canonical lane required');
const allowed=resolve(process.env.BUILD_LANE_BIN,'../artifacts')+'/';
if(!output.startsWith(allowed))throw Error('Private artifact output required');
fs.mkdirSync(output,{mode:0o700});
const run=(command,args,options={})=>{const p=spawnSync(command,args,{stdio:'inherit',timeout:10*60*1000,...options});if(p.status!==0||p.error)throw Error(command+' failed '+(p.error?.message??p.status));return p;};
const source=join(output,'source');fs.mkdirSync(source);
const custody=JSON.parse(fs.readFileSync(custodyPath)),revision=custody.revision;
if(!/^[a-f0-9]{40}$/.test(revision)||!custody.files.length)throw Error('Pinned source custody required');
const paths=spawnSync('tar',['-tf',archivePath],{encoding:'utf8'}).stdout.trim().split('\n');
if(paths.some(p=>p.startsWith('/')||p.split('/').includes('..')||!custody.files.some(f=>f.path===p)))throw Error('Unlisted archive source');
run('tar',['-xf',archivePath,'-C',source]);
for(const file of custody.files){const path=resolve(source,file.path);if(!path.startsWith(source+'/')||createHash('sha256').update(fs.readFileSync(path)).digest('hex')!==file.sha256)throw Error('Pinned import source differs: '+file.path);}
const store=join(source,'src/core/store'),manifest=join(store,'Cargo.toml'),module=join(store,'import/mod.rs');
const original=fs.readFileSync(module,'utf8'),removed='pub mod file_import;';
if(original.split(removed).length!==2)throw Error('Unexpected import composition');
fs.writeFileSync(module,original.replace(removed,'// file adapter excluded: CAS is not exported by this integration.'));
fs.appendFileSync(join(store,'lib.rs'),'\npub mod import;\n');
fs.writeFileSync(manifest,fs.readFileSync(manifest,'utf8').replace('[dependencies]','[dependencies]\nsha2="0.10"')+'\n[[bin]]\nname="m7-import-profile"\npath="m7-import.rs"\n');
fs.copyFileSync('tests/sync-v4/k11/m7-import.rs',join(store,'m7-import.rs'));
const result={schema:'ctox.m7.import-composition.v1',revision,startedAt:new Date().toISOString(),pass:false,terminal:false,
 changes:['export real import module in private crate','exclude unused file_import because CAS is absent; files not accepted','add sha2 and owned CLI'],sources:[]};
for(const path of ['src/core/store/import/mod.rs','src/core/store/import/base_release.rs','src/core/store/import/core_import.rs','src/core/store/schema.rs'])result.sources.push({path,sha256:createHash('sha256').update(fs.readFileSync(path)).digest('hex')});
try{
 run('rustfmt',['--edition','2021',join(store,'m7-import.rs')]);
 fs.copyFileSync(join(store,'m7-import.rs'),'tests/sync-v4/k11/m7-import.rs');
 run('cargo',['build','--manifest-path',manifest,'--bin','m7-import-profile','-j','2'],{env:{...process.env,CARGO_PROFILE_DEV_DEBUG:'0',CARGO_INCREMENTAL:'0'}});
 result.binary=join(output,'m7-import-profile');fs.copyFileSync(join(process.env.CARGO_TARGET_DIR,'debug/m7-import-profile'),result.binary);fs.chmodSync(result.binary,0o700);
 result.binarySha256=createHash('sha256').update(fs.readFileSync(result.binary)).digest('hex');result.pass=true;
}catch(error){result.error=error.message;process.exitCode=1;}
finally{result.finishedAt=new Date().toISOString();result.terminal=true;fs.writeFileSync(join(output,'build.json'),JSON.stringify(result,null,2));}
