#!/usr/bin/env node
// Ship only test code for remote measurements; real daemon assets have separate custody.
import fs from 'node:fs';
import { join, resolve } from 'node:path';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
const stage=resolve(process.argv[2]);
if(!stage.startsWith('/Volumes/tmp/dev-artifacts/ctox/'))throw Error('Disposable stage must be on designated volume');
fs.mkdirSync(stage,{recursive:true});
const files=[];
for(const dir of ['tests/sync-v4-e2e','scripts/sync-v4'])for(const name of fs.readdirSync(dir)){
 if(dir.startsWith('scripts')&&!name.startsWith('e2e-'))continue;
 const path=join(dir,name);if(!fs.statSync(path).isFile())continue;
 const to=join(stage,path);fs.mkdirSync(resolve(to,'..'),{recursive:true});
 const bytes=fs.readFileSync(path);fs.writeFileSync(to,bytes);
 files.push({path,sha256:createHash('sha256').update(bytes).digest('hex')});
}
fs.writeFileSync(join(stage,'source-manifest.json'),JSON.stringify({head:execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim(),scope:'test code only; daemon receipt is separate',files},null,2));
console.log(stage);
