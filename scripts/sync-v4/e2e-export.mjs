#!/usr/bin/env node
// Portable custody of ONE verified real-daemon build and its tracked browser assets.
import fs from 'node:fs';
import { spawnSync } from 'node:child_process';
import { resolve, join } from 'node:path';
import { createHash } from 'node:crypto';
import { verifiedBuild } from '../../tests/sync-v4-e2e/native.mjs';
const [receipt, directory] = process.argv.slice(2), build = verifiedBuild(receipt), output = resolve(directory);
if(process.platform!=='linux'||!process.env.CARGO_TARGET_DIR||!process.env.TMPDIR) throw Error('Lane required');
fs.mkdirSync(output,{mode:0o700}); fs.mkdirSync(join(output,'source'));
const result = spawnSync('git',['-C',build.source,'archive','--format=tar',build.revision,'install.sh','Cargo.toml','contracts','src/apps/business-os','src/core/harness/Cargo.toml','src/core/rxdb/Cargo.toml','src/core/rxdb/tools/local_signaling_server.js','src/skills/system'],{maxBuffer:512*1024*1024});
if(result.status!==0) throw Error('Tracked source asset export failed');
const untar = spawnSync('tar',['-xf','-','-C',join(output,'source')],{input:result.stdout,maxBuffer:1024*1024});
if(untar.status!==0) throw Error('Source asset extraction failed');
const kernelRevision=process.argv[4]??null;
if(kernelRevision){
 if(!/^[a-f0-9]{40}$/.test(kernelRevision))throw Error('Kernel revision must be immutable');
 const overlay=spawnSync('git',['archive',kernelRevision,'src/apps/business-os/kernel','src/apps/business-os/rxdb/src'],{maxBuffer:32*1024*1024});
 if(overlay.status!==0)throw Error('Pinned #823 kernel export failed');
 const extraction=spawnSync('tar',['-xf','-','-C',join(output,'source')],{input:overlay.stdout});if(extraction.status!==0)throw Error('Kernel extraction failed');
}
fs.copyFileSync(build.binary,join(output,'ctox')); fs.chmodSync(join(output,'ctox'),0o700);
const files=[];
function walk(dir,prefix='') { for(const name of fs.readdirSync(dir).sort()) { const relative=prefix?`${prefix}/${name}`:name, path=join(dir,name), st=fs.lstatSync(path);
  if(st.isDirectory())walk(path,relative); else if(st.isFile()) files.push({path:relative,bytes:st.size,sha256:createHash('sha256').update(fs.readFileSync(path)).digest('hex')});
  else throw Error(`Unsupported source asset type ${relative}`);
} }
walk(output);
fs.writeFileSync(join(output,'portable.json'),JSON.stringify({version:1,kernelRevision,revision:build.revision,binarySha256:build.binarySha256,producer:build,files},null,2));
console.log(output);
