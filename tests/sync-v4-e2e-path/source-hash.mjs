import {createHash} from 'node:crypto';
import {readdir,readFile} from 'node:fs/promises';
const roots=['src/apps/business-os/kernel','src/apps/business-os/rxdb/src/hello.mjs','src/core/rxdb/src/plugins/replication_webrtc','src/core/store','tests/sync-v4-e2e-path'];
const files=[];
async function walk(path){try{for(const entry of await readdir(path,{withFileTypes:true})){if(entry.name==='target'||entry.name==='node_modules')continue;const next=path+'/'+entry.name;if(entry.isDirectory())await walk(next);else if(/\.(rs|mjs|json|sh|toml|lock)$/.test(entry.name))files.push(next);}}catch(e){if(e.code!=='ENOTDIR')throw e;files.push(path);}}
for(const root of roots)await walk(root);const hash=createHash('sha256');for(const path of files.sort())hash.update(path+'\0').update(await readFile(path));console.log('E2E_SOURCE_SHA256 '+hash.digest('hex'));
