// Explicit in-memory test core. Never selected implicitly by createKernel().
// K-1c: mehrere Nutzer je Instanz teilen einen Core (namespace), jeder Nutzer hat seine eigene Kernel-DB (§16.7);
// Rechte, Handler-Laufzeit, Trigger, Katalog-Primitive und ctox.app.* liegen in fake-core-runtime.mjs.
import { KernelTransport, decimalSeq } from './transport.mjs';
import { cloneValue, CtoxError, fnv1a, mergePatch } from './values.mjs';
import { applyOp } from './outbox.mjs';
import { createCoreRuntime, FAKE_CORE_CAPABILITIES } from './fake-core-runtime.mjs';
import { checkCommandTransition, COMMAND_IMMUTABLE_FIELDS } from './command-state.mjs';
export { FAKE_CORE_CAPABILITIES };
const cores = new Map();
function coreFor(namespace) {
  if (cores.has(namespace)) return cores.get(namespace);
  const core = { head: 0n, docs: new Map(), ops: new Map(), artifacts: new Map(), blobs: new Map(), listeners: new Set(), users: new Map(), pending: new Set() };
  const key = (collection, id) => JSON.stringify([collection, id]);
  core.headSeq = () => core.head;
  /** E27.8: nur die Abos der geschriebenen Collection beliefern, alle Teilnehmer gleichzeitig. Je Teilnehmer läuft höchstens
   *  eine Seite (MemoryCoreTransport.deliver), damit dessen eigene Writes in IndexedDB nie hinter einem Stapel Seiten warten. */
  core.fanOut = (collection) => Promise.all([...core.listeners].filter((peer) => peer.subscriptions.has(collection)).map((peer) => peer.deliver(collection)));
  /** Einziger Commit-Weg im Core: Dokumente schreiben, an alle Teilnehmer ausliefern, Trigger auslösen. */
  core.commit = async (collection, docs, depth = 0) => {
    const sequences = [], changes = [];
    for (const data of Array.isArray(docs) ? docs : [docs]) {
      if (!data?.id) throw new CtoxError('invalid', 'Document ID required');
      const old = core.docs.get(key(collection, data.id));
      // K-1c Punkt 10: der Core schreibt Befehlszustände nur vorwärts; ein Terminalzustand entsteht genau einmal
      if (collection === 'business_commands' && !data._deleted && old?.data && checkCommandTransition(old.data, data)) { core.refused = (core.refused ?? 0) + 1; continue; }
      const seq = decimalSeq(++core.head), deleted = !!data._deleted;
      const doc = deleted ? null : cloneValue(data); if (doc) delete doc._deleted;
      const row = { collection, id: data.id, data: doc, deleted, seq, rev: `${seq}-${fnv1a(JSON.stringify(doc))}` };
      core.docs.set(key(collection, data.id), row); sequences.push(seq);
      changes.push({ id: data.id, op: deleted ? 'delete' : old && !old.deleted ? 'update' : 'insert', doc });
    }
    await core.fanOut(collection);
    core.runtime.onCommit(collection, changes, depth);
    return sequences;
  };
  /** Befehl im Core ausführen (Hintergrund); Status queued → accepted → running → done|failed. */
  core.spawnCommand = (doc, depth = 0, via = null) => {
    const job = runCommand(core, doc, depth, via);
    core.pending.add(job); job.finally(() => core.pending.delete(job));
    return job;
  };
  core.runtime = createCoreRuntime(core);
  cores.set(namespace, core);
  return core;
}
async function runCommand(core, doc, depth, via) {
  const latency = via?.latencyMs ?? 5;
  const sleep = () => new Promise((r) => setTimeout(r, latency));
  // E27.8: simulierte Leitungslatenz nur einmal vor der Annahme; danach startet der Handler ohne Timer-Verzug.
  // result ist ein Wert, kein Merge-Patch: null-Felder (z. B. exit_code bei Zeitüberschreitung) bleiben erhalten
  const update = async (patch) => {
    const current = core.docs.get(JSON.stringify(['business_commands', doc.id]))?.data ?? doc;
    const next = mergePatch(current, patch);
    if (Object.prototype.hasOwnProperty.call(patch, 'result')) next.result = cloneValue(patch.result);
    await core.commit('business_commands', [next]);
  };
  await sleep(); await update({ status: 'accepted' }); await update({ status: 'running', progress: 0.5 });
  const user = core.users.get(doc.actor_id) ?? null;
  let final;
  const route=core.runtime.resolveCommand(doc,user,via?.handlers.has(doc.type)||via?.failures.has(doc.type));
  if(route.error)final={status:'failed',error:route.error};
  else if (!route.platform && route.target===doc.app_id && via?.failures.has(doc.type)) { final = { status: 'failed', error: via.failures.get(doc.type) }; via.failures.delete(doc.type); }
  else if (!route.platform && route.target===doc.app_id && via?.handlers.has(doc.type)) {
    try { final = { status: 'done', progress: 1, result: await via.handlers.get(doc.type)(cloneValue(doc.payload), { app_id: doc.app_id, command_id: doc.id }) ?? { ok: true } }; } catch (e) { final = { status: 'failed', error: String(e.message) }; }
  } else {
    try { final = await core.runtime.execute(user, doc, depth); } catch (e) { final = { status: 'failed', error: String(e?.message ?? e) }; }
    // Ohne Handler gibt es kein Ergebnis mehr (K-1c Punkt 6): Befehl scheitert sichtbar statt {ok:true}.
    final ??= { status: 'failed', error: `no_handler: ${doc.type} hat keinen Handler in ${doc.app_id}` };
  }
  await update(final);
}
export class MemoryCoreTransport extends KernelTransport {
  get kind() { return 'memory'; }
  /** user = {id, role, permissions}: der Core kennt den Nutzer (Rechte wirken); ohne user prüft der Core keine Rechte. */
  constructor({ namespace = 'test', latencyMs = 5, user = null, everyMs, agentDelayMs, deviceLabel, codingDelayMs } = {}) {
    super(); this.namespace = namespace; this.latencyMs = latencyMs;
    this.core = coreFor(namespace); this.reject = []; this.rejectUpload = []; this.migrate = []; this.handlers = new Map(); this.failures = new Map();
    if (everyMs !== undefined) this.core.everyMs = everyMs;
    if (agentDelayMs !== undefined) this.core.agentDelayMs = agentDelayMs;
    if (codingDelayMs !== undefined) this.core.codingDelayMs = codingDelayMs;
    this.user = user ? cloneValue(user) : null;
    this.deviceLabel = deviceLabel ?? 'browser';
    this.pending = this.core.pending; this.online = true; this.closed = false; this.subscriptions = new Map();
    // E27.8: eine Zustellspur je Teilnehmer. waiting: Collection → Wartende (zusammengelegt, bis die Seite startet);
    // sent: Abos, die ihre erste Seite haben – danach gehen leere Seiten nicht mehr hinaus (jede kostet eine DB-Transaktion).
    this.waiting = new Map(); this.draining = null; this.sent = new Set();
  }
  /** Katalog-Primitive, die dieser Core anbietet (§16.29); die Aktivierung prüft capabilities_required dagegen. */
  get capabilities() { return this.core.runtime.capabilities; }
  get runtime() { return this.core.runtime; }
  setUser(user) { this.core.users.set(user.id, cloneValue(user)); }
  /** §16.43: device_pairing_id dieses Geräts, deterministisch je (Instanz, Nutzer, Gerät). */
  deviceId(instanceId, userId) { return `dev-${fnv1a(`${instanceId}|${userId}|${this.deviceLabel}`)}`; }
  async connect(identity, events) {
    if (this.identity && JSON.stringify(identity) !== JSON.stringify(this.identity)) throw new CtoxError('forbidden', 'Transport identity mismatch');
    this.identity = cloneValue(identity); this.events = events; this.closed = false; this.connects = (this.connects ?? 0) + 1;
    this.sent.clear(); // neue Verbindung: jedes Abo bekommt wieder eine erste (ggf. leere) vollständige Seite
    if (this.user && this.user.id === identity.userId && !this.core.users.has(this.user.id)) this.core.users.set(this.user.id, this.user);
    this.core.listeners.add(this); events.link?.({ state: this.online ? 'online' : 'offline', rtt_ms: this.latencyMs, relay: false });
    const known = this.core.users.get(identity.userId);
    if (known) events.actor?.({ id: known.id, role: known.role, permissions: [...(known.permissions ?? [])] }); // Rolle/Rechte kommen vom Core
    this.core.runtime.refreshEvery();
  }
  close() { this.closed = true; this.core.listeners.delete(this); if (!this.core.listeners.size) this.core.runtime.idle(); }
  get currentUser() { return this.identity ? this.core.users.get(this.identity.userId) ?? null : null; }
  key(collection,id) { return JSON.stringify([collection,id]); }
  get backlog(){return [...this.core.docs.values()].filter(row=>this.subscriptions.has(row.collection) && BigInt(row.seq)>BigInt(this.subscriptions.get(row.collection).checkpoint_seq??'0')).length;}
  get head() { return decimalSeq(this.core.head); }
  async subscribe(sub) {
    this.subscriptions.set(sub.collection, cloneValue(sub)); this.sent.delete(sub.collection);
    if (this.online) await this.deliver(sub.collection, { priority: true }); // erste Seite eines neuen Abos vor Rückstand
  }
  /** Höchstens eine Seite je Teilnehmer gleichzeitig, Abos in Anforderungsreihenfolge. Eine noch nicht gestartete
   *  Zustellung desselben Abos wird geteilt (sie liest beim Start alle Zeilen über dem Checkpoint). Teilnehmer parallel. */
  deliver(collection, { priority = false } = {}) {
    let w = this.waiting.get(collection);
    if (!w) { w = {}; w.promise = new Promise((resolve, reject) => { w.resolve = resolve; w.reject = reject; }); this.waiting.set(collection, w); }
    // Write-Vorrang: die Seite zum eigenen Write dieses Teilnehmers überholt wartende Zustellungen (nicht die laufende).
    if (priority && this.waiting.size > 1) this.waiting = new Map([[collection, w], ...[...this.waiting].filter(([c]) => c !== collection)]);
    if (!this.draining) this.draining = this.drain();
    return w.promise;
  }
  async drain() {
    try {
      while (this.waiting.size) {
        const [collection, w] = this.waiting.entries().next().value;
        this.waiting.delete(collection);
        try { w.resolve(await this.deliverNow(collection)); } catch (error) { w.reject(error); }
      }
    } finally { this.draining = null; }
  }
  async deliverNow(collection) {
    if (this.closed || !this.online || !this.events?.page) return;
    const sub = this.subscriptions.get(collection); if (!sub) return;
    const user = this.currentUser;
    // Zeilenrechte: nicht lesbare Zeilen erreichen den Client nicht (Lesbarkeit kann sich ändern → als gelöscht liefern)
    const rows = [...this.core.docs.values()].filter(d => d.collection === collection && BigInt(d.seq) > BigInt(sub.checkpoint_seq ?? '0')).sort((a,b) => BigInt(a.seq) < BigInt(b.seq) ? -1 : 1)
      .map((r) => (this.core.runtime.canRead(user, r) ? r : { ...r, data: null, deleted: true }));
    if (!rows.length && this.sent.has(collection)) return; // nichts Neues: keine leere Seite
    const page = { ...sub, epoch: '1', docs: cloneValue(rows), cursor_to: this.head, head_seq: this.head, backlog: 0, complete: true };
    // K-BF1: Checkpoint VOR der Übergabe vorziehen. Der Leiter meldet seinen Rückstand (this.backlog) noch während
    // events.page an die Folger; mit dem alten Checkpoint trug diese letzte Meldung N > 0, und nichts korrigierte sie.
    // Scheitert die Annahme, wird der Checkpoint zurückgesetzt, solange keine spätere Seite ihn weitergeschoben hat.
    const previous = sub.checkpoint_seq; sub.checkpoint_seq = page.cursor_to;
    try { await this.events.page(page); this.sent.add(collection); }
    catch (error) { if (sub.checkpoint_seq === page.cursor_to) sub.checkpoint_seq = previous; throw error; }
  }
  async push(collection, docs) { return this.core.commit(collection, docs); }
  async write(entry) {
    if (!this.online || this.closed) throw new CtoxError('unavailable', 'Test core offline');
    const previous = this.core.ops.get(entry.op_id); if (previous) return cloneValue(previous);
    const old = this.core.docs.get(this.key(entry.collection,entry.id));
    const rejection = this.reject.shift();
    let result = rejection === undefined ? null : {status:'rejected', ...(typeof rejection === 'object' ? rejection : {reason:rejection})};
    // K-WJ-B2 (E27.2): Befehlstyp ohne veröffentlichte Route → beim Schreiben abgelehnt, nie erst accepted und später failed
    if (!result && entry.collection === 'business_commands' && entry.op === 'put' && entry.body?.status === 'queued' && !(old?.data && !old.deleted) && !this.commandRouteKnown(entry.body)) {
      result = { status: 'rejected', code: 'unknown_command', reason: `unknown_command:${entry.body.type}` };
    }
    if (!result) {
      const code = this.core.runtime.checkWrite(this.currentUser, {...entry,test_route:this.handlers.has(entry.body?.type)||this.failures.has(entry.body?.type)}, old);
      if (code) result = typeof code === 'object' ? { status: 'rejected', code: code.code, reason: code.reason } : { status: 'rejected', reason: code };
    }
    if (!result && entry.expected_revision !== undefined && entry.expected_revision !== old?.rev) result={status:'rejected',code:'revision_conflict',reason:'Expected revision differs from current revision'};
    if (!result && entry.op === 'patch' && (!old || old.deleted)) result={status:'rejected',reason:'not_found'};
    // Doppelter Befehl (gleiche id, neue op): geänderte Absicht → idempotency_conflict; gleiche Absicht → angenommen ohne Rückschritt
    let duplicateCommand = false;
    if (!result && entry.collection === 'business_commands' && entry.op === 'put' && old?.data && !old.deleted) {
      const intent = Object.fromEntries(COMMAND_IMMUTABLE_FIELDS.filter((f) => f in (entry.body ?? {})).map((f) => [f, entry.body[f]]));
      const bad = checkCommandTransition(old.data, { ...old.data, ...intent });
      if (bad) result = { status: 'rejected', reason: bad.code };
      else { duplicateCommand = true; result = { status: 'accepted', seq: old.seq, rev: old.rev, doc: cloneValue(old.data), deleted: false }; }
    }
    let baseRev = entry.base_rev;
    if(baseRev?.startsWith('op:')) baseRev=this.core.ops.get(baseRev.slice(3))?.rev ?? baseRev;
    if(!result && entry.op==='delete' && (baseRev??null)!==(old?.rev??null)) result={status:'rejected',reason:'conflict'};
    if (!result && !duplicateCommand) {
      let state = applyOp(old ?? {data:null,deleted:false},entry);
      const migration=this.migrate.shift(); let archived_ref;
      if(migration && state.data){ for(const f of migration.dropFields??[]) if(f!=='id') delete state.data[f]; archived_ref=`core_archive:${entry.op_id}`; }
      const seq=decimalSeq(++this.core.head),rev=`${seq}-${fnv1a(JSON.stringify(state.data))}`;
      const row={collection:entry.collection,id:entry.id,...state,seq,rev};
      this.core.docs.set(this.key(entry.collection,entry.id),row);
      result={status:migration?'migrated':'accepted',seq,rev,...(archived_ref?{archived_ref}:{}),doc:cloneValue(state.data),deleted:state.deleted};
      this.core.runtime.onCommit(entry.collection, [{ id: entry.id, op: state.deleted ? 'delete' : old && !old.deleted ? 'update' : 'insert', doc: state.data }], 0);
    }
    this.core.ops.set(entry.op_id,cloneValue(result));
    // Pages follow the write result through the SAME stream. No separate live path.
    setTimeout(()=>{for(const peer of this.core.listeners) void peer.deliver(entry.collection, { priority: peer === this });},0);
    // §16.44: Turn-Annahme (Prüfung + Reservierung des Entwurfs) im selben Schritt wie der Write
    if (result.status !== 'rejected' && !duplicateCommand && entry.collection === 'business_commands' && entry.body?.type === 'ctox.coding.turn' && entry.body?.status === 'queued') {
      this.core.runtime.acceptCodingTurn(this.currentUser ?? { id: entry.body.actor_id ?? this.identity?.userId }, entry.body);
    }
    if(result.status!=='rejected' && !duplicateCommand && entry.collection==='business_commands' && entry.body?.status==='queued') this.core.spawnCommand({ ...entry.body, actor_id: entry.body.actor_id ?? this.identity?.userId }, 0, this);
    return cloneValue(result);
  }
  runCommand(doc) { return this.core.spawnCommand(doc, 0, this); }
  /** Veröffentlichte Befehls-Registry dieses Cores: Plattformbefehle, die Handler-Routen (app_id, type) der aktiven
   *  App-Versionen und die an diesem Transport registrierten Test-Routen (sim.onCommand / sim.failCommand). */
  commandRouteKnown(body) {
    const type = String(body?.type ?? '');
    return this.handlers.has(type) || this.failures.has(type) || this.core.runtime.hasCommandRoute(body?.app_id, type, body);
  }
  async artifact(hash) { if(!this.core.artifacts.has(hash)) throw new CtoxError('not_found','Artifact missing'); return this.core.artifacts.get(hash); }
  putArtifact(hash, content) { this.core.artifacts.set(hash, content); }
  async blob(ref, range) { const blob=this.core.blobs.get(ref); if(!blob) throw new CtoxError('not_found','Blob missing'); return range === undefined ? blob : blob.slice(range.start, range.end, blob.type); }
  async upload(ref,blob) { const failure=this.rejectUpload.shift();if(failure){const e=new CtoxError(failure.code ?? 'forbidden',failure.reason ?? String(failure),{reason:failure.reason ?? String(failure)});e.core_rejection=true;throw e;}this.core.blobs.set(ref,blob); }
}
