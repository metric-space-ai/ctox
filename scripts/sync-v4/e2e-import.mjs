#!/usr/bin/env node
import fs from 'node:fs';
import { resolve, join } from 'node:path';
import { createHash } from 'node:crypto';
const root=resolve(process.argv[2]), receipt=JSON.parse(fs.readFileSync(join(root,'portable.json')));
if(process.platform!=='linux'||!process.env.CARGO_TARGET_DIR||!process.env.TMPDIR)throw Error('Lane required');
if(receipt.version!==1||!receipt.producer?.terminal||!receipt.producer.pass)throw Error('Terminal producer missing');
for(const row of receipt.files){
 const path=resolve(root,row.path);
 if(!path.startsWith(root+'/')||!fs.lstatSync(path).isFile())throw Error('Custody path escape or special file');
 const bytes=fs.readFileSync(path);
 if(bytes.length!==row.bytes||createHash('sha256').update(bytes).digest('hex')!==row.sha256)throw Error(`Asset custody mismatch ${row.path}`);
}
if(!receipt.files.some(x=>x.path==='ctox'&&x.sha256===receipt.binarySha256))throw Error('Binary custody absent');
fs.writeFileSync(join(root,'build.json'),JSON.stringify({...receipt.producer,source:join(root,'source'),binary:join(root,'ctox'),custody:{verifiedAt:new Date().toISOString(),portableManifestSha256:createHash('sha256').update(fs.readFileSync(join(root,'portable.json'))).digest('hex')}},null,2));
console.log(join(root,'build.json'));
