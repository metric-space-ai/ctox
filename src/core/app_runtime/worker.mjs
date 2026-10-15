// Embedded by Core. App modules never import Node modules or access host files.
import { SourceTextModule, createContext } from 'node:vm';
import { createInterface } from 'node:readline';
import { webcrypto, randomUUID } from 'node:crypto';
const LIMIT = 1024 * 1024;
const ERROR_CODES = new Set(['invalid','forbidden','not_found','conflict','revoked','expired','quota','unavailable']);
const send = frame => process.stdout.write(JSON.stringify(frame)+'\n');
const permission=process.permission;
if (!permission || !process.execArgv.includes('--permission') || !process.execArgv.includes('--max-old-space-size=256') || !process.execArgv.includes('--experimental-vm-modules') || Number(process.versions.node.split('.')[0])<25 || ['fs.read','fs.write','net','child','worker','addons'].some(name=>permission.has(name))) process.exit(78);
send({type:'ready',version:process.versions.node,permissions:'empty'});
const pending=new Map();
let active=null,sequence=0;
function rpc(callId,method,args) {
  if(active!==callId)return Promise.reject(new Error('Call no longer active'));
  const callbackId=++sequence;
  return new Promise((resolve,reject)=>{pending.set(callbackId,{callId,resolve,reject});send({type:'ctx',call_id:callId,callback_id:callbackId,method,args});});
}
function context(frame) {
  const call=(method,args)=>rpc(frame.id,method,args),logs=[];
  const bindings={
    async read(collection,query={selector:{}}) {
      const normalized=normalizeMango(query);mango([],normalized);
      const result=await call('read',[collection,normalized]);
      if(Array.isArray(result))return result;
      if(result?.kind==='ctox.authorized_snapshot.v1' && Array.isArray(result.documents))return mango(result.documents,normalized);
      throw new CtoxError('invalid','Invalid Core read response');
    },
    write:(collection,op)=>call('write',[collection,op]),
    asset:path=>call('asset',[path]),
    command:(type,payload,options)=>call('command',[type,payload,options]),
    capability:(name,args)=>call('capability',[name,args]),
    now:()=>frame.core_now_ms,
    uuid:()=>randomUUID(),
    cursor:(op,key,value)=>call('cursor',[op,key,value ?? null]),
    log(level,message,fields={}) {const log=call('log',[level,message,fields]);log.catch(()=>{});logs.push(log);},
    async transaction(fn) {
      const snapshot=await call('transaction.begin',[]);
      const plan=bufferedTransaction(snapshot,{query:(rows,query)=>mango(rows,normalizeMango(query)),error:(code,msg)=>{throw new CtoxError(code,msg);}});
      try {
        const result=await fn(plan.bindings);
        const committed=await call('transaction.commit',[snapshot.token,plan.operations()]);
        plan.settle(committed);
        return result;
      } catch(error) {
        await call('transaction.abort',[snapshot.token]).catch(()=>{});
        throw error;
      }
    },
  };
  return{logs,ctx:handlerContext(bindings,{migration:frame.kind==='migration'})};
}
const modules=new Map();
const sandbox=createContext({URL,URLSearchParams,TextEncoder,TextDecoder,CtoxError,crypto:Object.freeze({subtle:Object.freeze({digest:webcrypto.subtle.digest.bind(webcrypto.subtle)}),randomUUID})},{codeGeneration:{strings:false,wasm:false}});
async function load(sources,reference) {
  const [entry,exportName='default']=reference.split('#');
  if(entry.split('/').at(-1).startsWith('_'))throw new CtoxError('invalid','Library cannot be a handler');
  function moduleFor(name) {
    if(!Object.hasOwn(sources,name))throw new CtoxError('invalid','Undeclared module: '+name);
    if(!modules.has(name))modules.set(name,new SourceTextModule(sources[name],{context:sandbox,identifier:'ctox-app:/'+name,importModuleDynamically(){throw new CtoxError('forbidden','Dynamic import unavailable');}}));
    return modules.get(name);
  }
  const module=moduleFor(entry);
  if(module.status==='unlinked')await module.link((specifier,parent)=>{
    if(!specifier.startsWith('./')&&!specifier.startsWith('../'))throw new CtoxError('forbidden','Only relative handler imports allowed');
    const url=new URL(specifier,parent.identifier),name=decodeURIComponent(url.pathname.slice(1));
    const root=parent.identifier.startsWith('ctox-app:/handlers/')?'handlers/':'';
    if(url.protocol!=='ctox-app:'||!name.startsWith(root)||name.split('/').some(p=>p==='..'||p===''))throw new CtoxError('forbidden','Import escapes handlers');
    return moduleFor(name);
  });
  if(module.status==='linked')await module.evaluate();
  if(module.status==='errored')throw module.error;
  const fn=exportName==='default'?module.namespace.default??module.namespace.handle??module.namespace.migrate:module.namespace[exportName];
  if(typeof fn!=='function')throw new CtoxError('invalid','Handler export must be a function');
  return fn;
}
async function call(frame) {
  if(active!==null)throw new Error('Concurrent app call');
  active=frame.id;
  try {
    const fn=await load(frame.sources,frame.path),{ctx,logs}=context(frame);
    let value;
    if(frame.kind==='migration') {
      value=[];
      for(const original of frame.input) {
        const doc=await fn(structuredClone(original),ctx);
        if(doc?.archive===true&&Object.keys(doc).length===1)value.push({id:original.id,base_rev:original.rev,action:'archive',archived_doc:original});
        else {if(!doc||doc.id!==original.id)throw new CtoxError('invalid','Migration cannot change identity');value.push({id:original.id,base_rev:original.rev,action:'write',doc});}
      }
    } else value=await fn(frame.input,ctx);
    await Promise.all(logs);
    const encoded=JSON.stringify(value);
    if(encoded===undefined||Buffer.byteLength(encoded)>LIMIT)throw new CtoxError('quota','Result exceeds JSON/1 MiB contract');
    send({type:'result',id:frame.id,value:JSON.parse(encoded)});
  } catch(error) {
    send({type:'error',id:frame.id,error:{code:ERROR_CODES.has(error.code)?error.code:'invalid',message:String(error.message).slice(0,2048),...(error.uncertain?{uncertain:true}:{}),...(error.path?{path:error.path}:{})}});
  } finally {
    active=null;
    for(const[id,request]of pending)if(request.callId===frame.id){pending.delete(id);request.reject(new Error('Handler returned before callback completed'));}
  }
}
const lines=createInterface({input:process.stdin,crlfDelay:Infinity});
lines.on('line',line=>{
  if(Buffer.byteLength(line)>8*LIMIT)process.exit(78);
  try {
    const frame=JSON.parse(line);
    if(frame.type==='ctx_result') {
      const request=pending.get(frame.callback_id);
      if(!request||request.callId!==frame.call_id)throw Error('Unmatched callback');
      pending.delete(frame.callback_id);
      if(frame.error)request.reject(Object.assign(new Error(frame.error.message),frame.error));else request.resolve(frame.value);
    } else if(frame.type==='call')call(frame).catch(()=>process.exit(78));
    else throw Error('Unknown protocol frame');
  } catch {process.exit(78);}
});
lines.on('close',()=>process.exit(0));
