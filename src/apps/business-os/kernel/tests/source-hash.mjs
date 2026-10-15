import {createHash} from 'node:crypto';
import {readdir,readFile} from 'node:fs/promises';
import {resolve,relative} from 'node:path';
const root=resolve('src/apps/business-os/kernel');
async function files(dir){const entries=await readdir(dir,{withFileTypes:true});return (await Promise.all(entries.map(e=>e.isDirectory()?files(resolve(dir,e.name)):[resolve(dir,e.name)]))).flat();}
const hash=createHash('sha256');
for(const file of (await files(root)).sort()){
 hash.update(relative(root,file));hash.update('\0');hash.update(await readFile(file));hash.update('\0');
}
hash.update('../rxdb/src/hello.mjs');hash.update('\0');hash.update(await readFile(resolve(root,'../rxdb/src/hello.mjs')));hash.update('\0');
console.log('KERNEL_SOURCE_SHA256 '+hash.digest('hex'));
