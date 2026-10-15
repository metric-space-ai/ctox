// Explicit in-memory test core. Never selected implicitly by createKernel().
import { KernelTransport, decimalSeq } from './transport.mjs';
import { cloneValue, CtoxError, fnv1a, mergePatch } from './values.mjs';
import { applyOp } from './outbox.mjs';
const cores = new Map();
export class MemoryCoreTransport extends KernelTransport {
  constructor({ namespace = 'test', latencyMs = 5 } = {}) {
    super(); this.namespace = namespace; this.latencyMs = latencyMs;
    if (!cores.has(namespace)) cores.set(namespace, { head: 0n, docs: new Map(), ops: new Map(), artifacts: new Map(), blobs: new Map(), listeners: new Set() });
    this.core = cores.get(namespace); this.reject = []; this.migrate = []; this.handlers = new Map(); this.failures = new Map();
    this.pending = new Set(); this.online = true; this.closed = false; this.subscriptions = new Map();
  }
  async connect(identity, events) {
    if (this.identity && JSON.stringify(identity) !== JSON.stringify(this.identity)) throw new CtoxError('forbidden', 'Transport identity mismatch');
    this.identity = cloneValue(identity); this.events = events; this.closed = false;
    this.core.listeners.add(this); events.link?.({ state: this.online ? 'online' : 'offline', rtt_ms: this.latencyMs, relay: false });
  }
  close() { this.closed = true; this.core.listeners.delete(this); }
  key(collection,id) { return JSON.stringify([collection,id]); }
  get backlog(){return [...this.core.docs.values()].filter(row=>this.subscriptions.has(row.collection) && BigInt(row.seq)>BigInt(this.subscriptions.get(row.collection).checkpoint_seq??'0')).length;}
  get head() { return decimalSeq(this.core.head); }
  async subscribe(sub) {
    this.subscriptions.set(sub.collection, cloneValue(sub));
    if (this.online) await this.deliver(sub.collection);
  }
  async deliver(collection) {
    if (this.closed || !this.online || !this.events?.page) return;
    const sub = this.subscriptions.get(collection); if (!sub) return;
    const rows = [...this.core.docs.values()].filter(d => d.collection === collection && BigInt(d.seq) > BigInt(sub.checkpoint_seq ?? '0')).sort((a,b) => BigInt(a.seq) < BigInt(b.seq) ? -1 : 1);
    const page = { ...sub, epoch: '1', docs: cloneValue(rows), cursor_to: this.head, head_seq: this.head, backlog: 0, complete: true };
    await this.events.page(page); sub.checkpoint_seq = page.cursor_to;
  }
  async push(collection, docs) {
    const sequences=[];
    for (const data of Array.isArray(docs) ? docs : [docs]) {
      if (!data?.id) throw new CtoxError('invalid', 'Document ID required');
      const seq = decimalSeq(++this.core.head), deleted = !!data._deleted;
      const doc = deleted ? null : cloneValue(data); if(doc) delete doc._deleted;
      const row = { collection, id: data.id, data: doc, deleted, seq, rev: `${seq}-${fnv1a(JSON.stringify(doc))}` };
      this.core.docs.set(this.key(collection,data.id),row); sequences.push(seq);
    }
    for (const peer of this.core.listeners) for(const held of peer.subscriptions.keys()) await peer.deliver(held);
    return sequences;
  }
  async write(entry) {
    if (!this.online || this.closed) throw new CtoxError('unavailable', 'Test core offline');
    const previous = this.core.ops.get(entry.op_id); if (previous) return cloneValue(previous);
    const old = this.core.docs.get(this.key(entry.collection,entry.id));
    let result = this.reject.length ? { status:'rejected', reason:this.reject.shift() } : null;
    if (!result && entry.op === 'patch' && (!old || old.deleted)) result={status:'rejected',reason:'not_found'};
    let baseRev = entry.base_rev;
    if(baseRev?.startsWith('op:')) baseRev=this.core.ops.get(baseRev.slice(3))?.rev ?? baseRev;
    if(!result && entry.op==='delete' && (baseRev??null)!==(old?.rev??null)) result={status:'rejected',reason:'conflict'};
    if (!result) {
      let state = applyOp(old ?? {data:null,deleted:false},entry);
      const migration=this.migrate.shift(); let archived_ref;
      if(migration && state.data){ for(const f of migration.dropFields??[]) if(f!=='id') delete state.data[f]; archived_ref=`core_archive:${entry.op_id}`; }
      const seq=decimalSeq(++this.core.head),rev=`${seq}-${fnv1a(JSON.stringify(state.data))}`;
      const row={collection:entry.collection,id:entry.id,...state,seq,rev};
      this.core.docs.set(this.key(entry.collection,entry.id),row);
      result={status:migration?'migrated':'accepted',seq,rev,...(archived_ref?{archived_ref}:{}),doc:cloneValue(state.data),deleted:state.deleted};
    }
    this.core.ops.set(entry.op_id,cloneValue(result));
    // Pages follow the write result through the SAME stream. No separate live path.
    setTimeout(()=>{for(const peer of this.core.listeners) void peer.deliver(entry.collection);},0);
    if(result.status!=='rejected' && entry.collection==='business_commands' && entry.body?.status==='queued') { const job=this.runCommand(entry.body);this.pending.add(job);job.finally(()=>this.pending.delete(job)); }
    return cloneValue(result);
  }
  async runCommand(doc) {
    const sleep=()=>new Promise(r=>setTimeout(r,this.latencyMs));
    const update=async patch=> { const current=this.core.docs.get(this.key('business_commands',doc.id))?.data??doc; await this.push('business_commands',[mergePatch(current,patch)]); };
    await sleep(); await update({status:'accepted'}); await sleep(); await update({status:'running',progress:0.5});
    let final;
    if(this.failures.has(doc.type)){final={status:'failed',error:this.failures.get(doc.type)};this.failures.delete(doc.type);}
    else try{final={status:'done',progress:1,result:await this.handlers.get(doc.type)?.(cloneValue(doc.payload),{app_id:doc.app_id,command_id:doc.id})??{ok:true}};}catch(e){final={status:'failed',error:String(e.message)}}
    await sleep(); await update(final);
  }
  async artifact(hash) { if(!this.core.artifacts.has(hash)) throw new CtoxError('not_found','Artifact missing'); return this.core.artifacts.get(hash); }
  putArtifact(hash, content) { this.core.artifacts.set(hash, content); }
  async blob(ref) { const blob=this.core.blobs.get(ref); if(!blob) throw new CtoxError('not_found','Blob missing'); return blob; }
  async upload(ref,blob) { this.core.blobs.set(ref,blob); }
}
