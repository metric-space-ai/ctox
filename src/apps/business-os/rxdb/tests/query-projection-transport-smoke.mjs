import assert from 'node:assert/strict';
import {deflateRawSync} from 'node:zlib';
const {createDemandLoadingTransport}=await import(process.argv.includes('--source')?'../src/index.mjs':'../dist/ctox-rxdb-js.mjs');
const projection=['_deleted','_rev','id','status'];
const requests=[];
const transport=createDemandLoadingTransport({getPeerId:()=> 'native'});
transport.attach({
  connections:new Map([['native',{channel:{readyState:'open'},peer:{connectionState:'connected'}}]]),
  async request(peerId,method,params){requests.push({peerId,method,params});return {ack:true};},
});
async function fetch(requestId,chunks){
  const count=requests.length;
  const pending=transport.requestQueryFetch({requestId,collectionName:'leads',queryFingerprint:'projection-fingerprint',query:{selector:{}},window:{offset:0,limit:200},projection});
  pending.catch(()=>{});
  for(let attempts=0;requests.length===count;attempts+=1){if(attempts>100)throw new Error('transport request deadline');await new Promise(resolve=>setTimeout(resolve,5));}
  for(const [sequence,chunk] of chunks.entries()) await transport.requestHandlers['rxdb.query.chunk']({params:[{requestId,sequence,...chunk}]});
  return pending;
}
await assert.rejects(fetch('legacy',[{documents:[{id:'one',status:'open',private:'full legacy payload'}],complete:true}]),{code:'QUERY_PROJECTION_NOT_SUPPORTED',retryable:false});
await assert.rejects(fetch('mixed',[
  {documents:[{id:'one',status:'open'}],complete:false,appliedProjection:projection},
  {documents:[],complete:true},
]),{code:'QUERY_PROJECTION_NOT_SUPPORTED',retryable:false});
await assert.rejects(fetch('changed-fields',[{documents:[{id:'one'}],complete:true,appliedProjection:['id']}]),{code:'QUERY_PROJECTION_NOT_SUPPORTED',retryable:false});
const document={id:'one',_rev:'1-a',_deleted:false,status:'open'};
const result=await fetch('confirmed',[
  {compressed:'deflate',compressedBase64:deflateRawSync(Buffer.from(JSON.stringify([document]))).toString('base64'),complete:false,appliedProjection:projection},
  {documents:[],complete:true,authoritativeRevision:'confirmed-revision',appliedProjection:projection},
]);
assert.deepEqual(result.documents,[document]);
assert.deepEqual(result.appliedProjection,projection);
assert.equal(result.authoritativeRevision,'confirmed-revision');
assert.equal(requests.length,4,'unconfirmed responses must not retry or fall back to full fetch');
assert.ok(requests.every(request=>request.method==='rxdb.query.fetch'&&JSON.stringify(request.params[0].projection)===JSON.stringify(projection)));
console.log('query projection transport PASS: reject legacy/mixed fields, confirm compressed and terminal chunks without full fallback');
