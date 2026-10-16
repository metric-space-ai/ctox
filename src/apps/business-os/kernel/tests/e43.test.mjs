// E43: kernel/ is self-contained under /kernel/*, SW registration carries the kernel DB name,
// transfer hash failures surface as hash_mismatch. (Capabilities from hello_ok: boot.test.mjs.)
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {readFile,readdir} from 'node:fs/promises';
import {dirname,extname,join,relative,resolve,sep} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createKernel,createLoader,CtoxError} from '../index.mjs';
import {MemoryCoreTransport} from '../testkit.mjs';
import {WebRTCKernelTransport} from '../transport-webrtc.mjs';
import {protocolChannels,FakeNativePeer,offer,scopeFor} from './fake-native-peer.mjs';
import {sha256Hex} from '../values.mjs';

const kernelRoot=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const sha=async path=>createHash('sha256').update(await readFile(path)).digest('hex');
export const VENDORED=Object.freeze({'vendor/hello.mjs':'../rxdb/src/hello.mjs','vendor/async-timeout.js':'../shared/async-timeout.js'});

test('vendored kernel dependencies are byte-identical to their sources (sha256 pin)',async()=>{
 for(const [copy,source] of Object.entries(VENDORED))assert.equal(await sha(join(kernelRoot,copy)),await sha(resolve(kernelRoot,source)),`${copy} drifted from ${source}: re-copy it unchanged`);
});

async function walk(dir){const out=[];for(const e of await readdir(dir,{withFileTypes:true})){const p=join(dir,e.name);if(e.isDirectory())out.push(...await walk(p));else if(['.mjs','.js'].includes(extname(e.name)))out.push(p);}return out;}
/** Every module specifier: static import/export-from, side-effect import and literal dynamic import(). */
export function specifiers(source){
 const out=[];
 for(const re of [/\bimport\s+[^'"`;()]*?\bfrom\s*(['"])([^'"]+)\1/g,/\bexport\s+[^'"`;()]*?\bfrom\s*(['"])([^'"]+)\1/g,/(?:^|[;{}])\s*import\s*(['"])([^'"]+)\1/gm,/\bimport\s*\(\s*(['"])([^'"]+)\1\s*\)/g])for(const m of source.matchAll(re))out.push(m[2]);
 return out;
}
export async function importsLeavingKernel(root=kernelRoot){
 const offenders=[];
 for(const file of await walk(root)){
  const rel=relative(root,file),isTest=rel.startsWith(`tests${sep}`)||rel.startsWith(`testkit${sep}`);
  for(const spec of specifiers(await readFile(file,'utf8'))){
   if(spec.startsWith('./')||spec.startsWith('../')){const target=resolve(dirname(file),spec);if(target!==root&&!target.startsWith(root+sep))offenders.push(`${rel}: ${spec}`);}
   else if(isTest&&(spec.startsWith('node:')||spec.startsWith('/')))continue; // Node built-ins and in-page URLs under the kernel route
   else offenders.push(`${rel}: ${spec}`);
  }
 }
 return offenders;
}

test('no import in kernel/ leaves kernel/ (E43.1: served as /kernel/* without /rxdb or /shared routes)',async()=>{
 assert.deepEqual(await importsLeavingKernel(),[]);
 assert.deepEqual(specifiers("import {a} from '../x.mjs';import '../y.mjs';export {b} from \"../z.mjs\";await import('../w.mjs');"),['../x.mjs','../z.mjs','../y.mjs','../w.mjs']);
});

test('loader registers /sw.js?db=<kernel IDB name>, keeps the same-origin /sw.js pathname check',async()=>{
 const desc={navigator:Object.getOwnPropertyDescriptor(globalThis,'navigator'),location:Object.getOwnPropertyDescriptor(globalThis,'location')};
 const registered=[];
 const set=(name,value)=>Object.defineProperty(globalThis,name,{value,configurable:true,writable:true});
 set('location',{href:'https://tenant.example/',origin:'https://tenant.example'});
 set('navigator',{serviceWorker:{controller:null,register:async(url,options)=>{registered.push({url,options});return {active:null};},addEventListener(){},removeEventListener(){}}});
 try{
  const loader=await createLoader({instanceId:`e43-sw-${process.pid}`,userId:'alice',storage:'memory',multiTab:false,transport:new MemoryCoreTransport({namespace:`e43-sw-${process.pid}`})});
  try{
   assert.equal(registered.length,1);const url=new URL(registered[0].url);
   assert.equal(url.pathname,'/sw.js');assert.equal(url.origin,'https://tenant.example');assert.equal(url.searchParams.get('db'),loader.kernel.dbName);
   assert.equal(loader.kernel.dbName,`ctox-e43-sw-${process.pid}-alice`);assert.deepEqual(registered[0].options,{scope:'/'});
  }finally{await loader.close();}
  for(const serviceWorker of ['/other.js','https://evil.example/sw.js']){
   registered.length=0;
   const bad=await createLoader({instanceId:`e43-sw-bad-${process.pid}`,userId:'alice',storage:'memory',multiTab:false,serviceWorker,transport:new MemoryCoreTransport({namespace:`e43-sw-bad-${process.pid}`})});
   try{assert.equal(registered.length,0);assert.ok(bad.kernel.statusZone.get().some(e=>e.code==='shell.service_worker'));}finally{await bad.close();}
  }
 }finally{for(const [name,d] of Object.entries(desc)){if(d)Object.defineProperty(globalThis,name,d);else delete globalThis[name];}}
});

test('transfer hash mismatch is typed and fails activation as hash_mismatch, not artifact_missing',async()=>{
 // Transport level: the CAS download names the failure.
 const pair=protocolChannels(),peer=new FakeNativePeer(pair.server),transport=new WebRTCKernelTransport({channels:pair.client,hello:offer('instance'),resolveScope:scopeFor,verifyPackage:async p=>p.sig==='test-signature'});
 try{
  await transport.connect({instanceId:'instance',userId:'alice'},{link(){},page(){},write_result(){}});
  const ref=await sha256Hex(new Blob(['expected']));peer.blobs.set(ref,new Blob(['tampered']));
  await assert.rejects(transport.artifact(ref),e=>e instanceof CtoxError&&e.code==='invalid'&&e.reason==='hash_mismatch');
 }finally{transport.close();peer.close();}
 // Kernel level: one refetch, then failed hash_mismatch:<path>.
 let fetches=0;
 class TamperedCas extends MemoryCoreTransport {async artifact(){fetches++;throw new CtoxError('invalid','Transfer hash mismatch',{reason:'hash_mismatch'});}}
 const k=createKernel({instanceId:`e43-hash-${process.pid}`,userId:'alice',storage:'memory',multiTab:false,transport:new TamperedCas({namespace:`e43-hash-${process.pid}`})});
 await k.ready;
 try{
  const result=await k.sim.publishAppVersion('x','1',{files:{'index.js':'export const a = 1;'}});
  assert.equal(result.state,'failed');assert.equal(result.error,'hash_mismatch:index.js');assert.equal(fetches,2);
 }finally{await k.close();}
});
