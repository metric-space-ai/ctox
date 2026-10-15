import assert from 'node:assert/strict';

// #810 calls each default export's run(h). The optional native OS bootstrap
// supplies only authenticated fixture/control operations; all acceptance
// assertions below read the real kernel, IndexedDB and WebRTC receive path.
// No fake core, HTTP record bridge, SQL write or role override is permitted.
export const CONTROL_CONTRACT = 'ctox.sync-v4.regression-control.v1';
export function notRun(reason, measured = {}) {
  return {status:'not_run',pass:false,acceptanceComplete:false,reason,measured};
}
export async function connect(h, id) {
  const page = await h.page();
  const availability = await h.os(page);
  if (!availability.available) return {result:notRun(
    'Native OS bootstrap unavailable; incident lifecycle was not executed',
    {availability,expectedLegacyFailure:availability.v3Shell && availability.kernelStaticAvailable === false})};
  const proof = await page.evaluate(({contract,id}) => {
    const os = globalThis.__e2eOS, c = os.regression;
    if (os.transportKind !== 'native-webrtc' || !os.nativePeerId) throw Error('Native transport proof absent');
    if (!c || c.contract !== contract) return {available:false,missing:'__e2eOS.regression '+contract};
    if (typeof c.prepare !== 'function' || typeof c.control !== 'function') throw Error('Native fixture/control interface incomplete');
    return {available:true,scenario:id};
  },{contract:CONTROL_CONTRACT,id});
  if (!proof.available) return {result:notRun('Native fixture/identity control seam is not bound',proof)};
  const fixture = await control(page,'prepare',{scenario:id});
  assert.equal(fixture.scenario,id);
  assert.equal(fixture.isolated,true,'Only the runner-owned synthetic tenant may be modified');
  assert.equal(fixture.revision,h.config.revision,'Fixture must use the checksum-bound daemon revision');
  assert.equal(fixture.root,h.config.root,'Fixture control must target this runner-owned tenant');
  assert.equal(fixture.instanceId,await page.evaluate(()=>globalThis.__e2eOS.kernel.instanceId));
  return {page,fixture};
}
export async function control(page,operation,args = {}) {
  return page.evaluate(async ({operation,args}) => {
    const c = globalThis.__e2eOS.regression;
    return operation === 'prepare' ? c.prepare(args) : c.control(operation,args);
  },{operation,args});
}
export async function participant(h, fixture, identity) {
  const page = await h.newClient();
  const availability = await h.os(page);
  assert.equal(availability.available,true,'Every participant needs the real native OS bootstrap');
  const actual = await control(page,'authenticate',{userId:identity.id,deviceId:identity.deviceId});
  assert.equal(actual.userId,identity.id,'Server-authenticated identity mismatch');
  assert.equal(actual.role,identity.role,'Role must come from the Core, never a browser override');
  assert.equal(actual.instanceId,fixture.instanceId);
  await page.evaluate(() => {
    const os=globalThis.__e2eOS, t=os.transport;
    if (!t || !t.channels || !t.session || typeof t.receive !== 'function' || !t.events)
      throw Error('Expose the actual WebRTCKernelTransport for wire observation');
    if (os.kernel.storageKind !== 'indexeddb') throw Error('Real IndexedDB required');
    const trace={frame:0,pages:[],revokes:[],activations:[],navigation:performance.timeOrigin};
    os.shell.activation.onChange(event=>trace.activations.push({...event,at:Date.now()}));
    const receive=t.receive, page=t.events.page, revoke=t.events.revoke;
    t.receive=async function(data){trace.frame++;return receive.call(this,data);};
    t.events.page=async message=>{
      trace.pages.push({frame:trace.frame,at:Date.now(),collection:message.collection,
        scope_hash:message.scope_hash,cursor_to:message.cursor_to,
        docs:(message.docs??[]).map(d=>({id:d.id,seq:d.seq,op:d.op}))});
      return page(message);
    };
    t.events.revoke=async message=>{
      trace.revokes.push({frame:trace.frame,at:Date.now(),rights_ver:message.rights_ver,collections:message.collections});
      return revoke?.(message);
    };
    globalThis.__p6Trace=trace;
  });
  return {page,identity};
}
export async function rawStore(page, store) {
  return page.evaluate(async store => {
    const name=globalThis.__e2eOS.kernel.dbName;
    if (!(await indexedDB.databases()).some(db=>db.name===name)) throw Error('Existing real kernel IDB missing');
    const db=await new Promise((resolve,reject)=>{
      const request=indexedDB.open(name);
      request.onupgradeneeded=()=>{request.transaction.abort();reject(Error('Never create an acceptance database'));};
      request.onsuccess=()=>resolve(request.result);request.onerror=()=>reject(request.error);
    });
    try {
      return await new Promise((resolve,reject)=>{
        const tx=db.transaction(store,'readonly'), request=tx.objectStore(store).getAll();
        let rows;request.onsuccess=()=>{rows=request.result;};
        tx.oncomplete=()=>resolve(rows);tx.onabort=()=>reject(tx.error);tx.onerror=()=>reject(tx.error);
      });
    } finally { db.close(); }
  },store);
}
export function collectionRecords(records, collection) {
  return records.filter(r=>r.collection===collection || r.collection.startsWith(collection+'@g'));
}
export function assertExactScope(records, collection, expectedIds, forbiddenFields = []) {
  const rows=collectionRecords(records,collection).filter(r=>!r.deleted);
  assert.deepEqual(rows.map(r=>r.id).sort(),[...expectedIds].sort(),'IDB contains a foreign/missing row or stale generation');
  for (const row of rows) for (const field of forbiddenFields) {
    const has=(value,path)=>{
      for (const key of path.split('.')) {
        if (value == null || !Object.hasOwn(value,key)) return false;
        value=value[key];
      }
      return true;
    };
    for (const body of [row.data,row.base?.data]) assert.equal(has(body,field),false,'Masked field persisted in raw IDB: '+field);
  }
}
export async function publish(page, appId, version, base) {
  const result=await page.evaluate(async ({appId,version,base})=>{
    const draft=globalThis.__e2eOS.shell.source(appId).draft;
    for (const [path,text] of Object.entries(version.files)) await draft.write(path,text);
    await draft.setManifest(version.manifest);
    await draft.setCollections(version.collections);
    const write=draft.publish({base_app_ver:base});await write.local;return write.result;
  },{appId,version,base});
  const stamp=await control(page,'commitReceipt',{seq:result.seq,appId,version:result.rev});
  assert.equal(stamp.source,'store-commit');assert.ok(Number.isFinite(stamp.committedAtMs));
  return {...result,committedAtMs:stamp.committedAtMs};
}
export async function activation(page, appId, version, committedAtMs, timeout = 10000) {
  assert.ok(Number.isFinite(committedAtMs),'Activation must be timed from the actual Core commit');
  await page.waitForFunction(({appId,version})=>{
    const a=globalThis.__e2eOS.shell.activation.get(appId).get();
    return a?.state==='activated' && a.activated_ver===version;
  },{appId,version},{timeout});
  const at=await page.evaluate(({appId,version})=>globalThis.__p6Trace.activations
    .find(e=>e.app_id===appId && e.ver===version && e.state==='activated')?.at,{appId,version});
  assert.ok(Number.isFinite(at),'Missing measured activation event');
  assert.ok(at>=committedAtMs,'Core/browser clocks disagree');
  return at-committedAtMs;
}
export async function write(page, appId, collection, doc, patch = false) {
  return page.evaluate(async ({appId,collection,doc,patch})=>{
    const os=globalThis.__e2eOS, version=os.shell.apps.get().find(a=>a.id===appId)?.activated_ver;
    if (!version) throw Error('App is not actually activated');
    const api=os.kernel.appApi(appId,version);
    const handle=patch ? api.patch(collection,doc.id,doc.body) : api.put(collection,doc);
    await handle.local;return {op_id:handle.op_id,result:await handle.result};
  },{appId,collection,doc,patch});
}
export async function queueOffline(page, appId, collection, doc) {
  return page.evaluate(async ({appId,collection,doc})=>{
    const os=globalThis.__e2eOS, version=os.shell.apps.get().find(a=>a.id===appId)?.activated_ver;
    const handle=os.kernel.appApi(appId,version).put(collection,doc);
    await handle.local;
    globalThis.__p6Pending ||= new Map();globalThis.__p6Pending.set(handle.op_id,handle.result);
    return handle.op_id;
  },{appId,collection,doc});
}
export async function pendingResult(page, opId) {
  return page.evaluate(opId=>globalThis.__p6Pending.get(opId),opId);
}
export async function trace(page) { return page.evaluate(()=>structuredClone(globalThis.__p6Trace)); }
export function assertRevocation(before,after,rights,collection,sendTrace) {
  assert.match(String(rights.seq),/^[0-9]+$/);
  const revoke=after.revokes.find(r=>String(r.rights_ver)===String(rights.rights_ver) && r.collections.includes(collection));
  assert.ok(revoke,'No authenticated scope_revoke received');
  // Receive callbacks on independent lanes can overlap. Their callback order
  // cannot prove a server-side one-frame fence: require the final native send
  // log anchored at the actual Store commit instead of inferring it from RTT.
  assert.equal(sendTrace?.source,'native-final-send');
  assert.equal(sendTrace.rights_seq,String(rights.seq));
  assert.match(sendTrace.commit_frame,/^[0-9]+$/);
  const limit=BigInt(sendTrace.commit_frame)+1n;
  const notice=sendTrace.frames.find(f=>f.type==='scope_revoke' &&
    String(f.rights_ver)===String(rights.rights_ver) && f.collections.includes(collection));
  assert.ok(notice && BigInt(notice.frame)<=limit,'Rights change took more than one native send frame');
  const stale=sendTrace.frames.filter(f=>BigInt(f.frame)>limit && f.collection===collection &&
    String(f.rights_ver)!==String(rights.rights_ver) && f.type==='page');
  assert.deepEqual(stale,[],'Old rights remained active beyond the next native frame');
  const leaked=after.pages.filter(p=>p.frame>revoke.frame && p.collection===collection)
    .flatMap(p=>p.docs).filter(d=>d.op!=='unproject' && d.op!=='delete' && BigInt(d.seq)>BigInt(rights.seq));
  assert.deepEqual(leaked,[],'Post-revocation changes reached the old subscription');
}
export async function until(h, predicate, timeout, label) {
  const start=Date.now();
  // Bounded scenario measurement, never a supervisor/lane retry loop.
  do { if (await predicate()) return Date.now()-start;await h.pause(25); } while(Date.now()-start<timeout);
  throw Error(label+' exceeded '+timeout+'ms');
}
