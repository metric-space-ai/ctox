// #823 adds a static hello-module dependency outside /kernel/. This is fixture
// server glue only; never intercept a record, command or synchronization frame.
import fs from 'node:fs';
import {join} from 'node:path';
import {createHash} from 'node:crypto';
export function bindAssets(runtime){
 const file=join(runtime,'src/apps/business-os/v4/boot/serve.mjs');
 const before=fs.readFileSync(file,'utf8');let after=before;
 const patches=[
  ["const KERNEL = join(BOS, 'kernel');","const KERNEL = join(BOS, 'kernel');\nconst RXDB_SOURCE = join(BOS, 'rxdb/src');"],
  ["else if (path.startsWith('/kernel/')) file = normalize(join(KERNEL, path.slice(8)));","else if (path.startsWith('/kernel/')) file = normalize(join(KERNEL, path.slice(8)));\n      else if (path.startsWith('/rxdb/src/')) file = normalize(join(RXDB_SOURCE, path.slice(10)));"],
  ["file.startsWith(BOOT + '/') || file.startsWith(KERNEL + '/')","file.startsWith(BOOT + '/') || file.startsWith(KERNEL + '/') || file.startsWith(RXDB_SOURCE + '/')"],
 ];
 for(const [a,b]of patches){if(after.split(a).length!==2)throw Error('Bind fixture route anchor missing/ambiguous');after=after.replace(a,b);}
 fs.writeFileSync(file,after);
 const sha=s=>createHash('sha256').update(s).digest('hex');
 return {file:'src/apps/business-os/v4/boot/serve.mjs',beforeSha256:sha(before),afterSha256:sha(after),purpose:'Serve #823 static /rxdb/src/ dependency; no Business OS data over HTTP'};
}
