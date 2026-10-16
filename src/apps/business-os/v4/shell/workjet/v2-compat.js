// Shell v4 – Workjet-Brücke: V2-Abhängigkeiten über dem v4-Kernel (WJB-0).
//
// Die portierten V2-Funktionen (actions/*.js) sehen dieselben Abhängigkeiten wie in app.js:
//   state.session, state.db, state.sync, state.syncConfig, state.commandBus, actorContext(session), newId().
// Hier entstehen sie aus der App-API der Shell (kernel.appApi('shell', shellVer), Stub-Pin 8489b4e index.d.ts):
//   state.commandBus.dispatch(command, {until, timeoutMs}) → api.command(type, payload, {record_id}) + Status-LiveDoc
//   state.session / actorContext                          → api.actor + kernel.identity
//   state.sync.requestNative(method, params, opts)        → E25: typisierter Kern-Befehl gleichen Namens
//   state.sync.startCollection(name) / state.db           → api.query / api.get (lokal) + metrics.collections (Liveness)
//   state.syncConfig.instance_id                          → kernel.identity.instance_id
// Fehlt dem Kernel ein Teil, entsteht ein typisierter Fehler {code:'unsupported'} statt eines Absturzes.
//   peer.demandFileLoader.fetchFile(fileId, {range}) → E26 api.files.get({id, range:{start, end}}) (WJB-R, K-WJ-2)
//   dispatch: V2-Befehls-ID → opts.idempotency_key (E26, K-WJ-4); requestNative ohne Schlüssel (WJB-R)
import { sha256Hex } from '../../../shared/file-integrity.js'; // WJB-R: V2-Modul, byte-gleich zum Pin e35a116ae

export const UNSUPPORTED = 'unsupported';

// Workjet erkennt „nicht unterstützt“ je Brücke an unterschiedlichen Merkmalen (Workjet main eb65947d,
// apps/desktop/src/ctox/CtoxGuestManager.ts): Projekt über den Nachrichtentext (Regex „unsupported … action“),
// Rechner über die Nachricht „workjet_computers collection is not registered.“, Gerät über code
// CTOX_WEBRTC_CAPABILITY_MISSING. Sitzung/Ereignisse: nur „Global fehlt“. Der Fehler trägt immer code 'unsupported'
// und zusätzlich das Merkmal, das der jeweilige Workjet-Aufrufer liest.
const UNSUPPORTED_MESSAGES = {
  project: (what) => `Unsupported Workjet project control action: ${what}`,
  computer: () => 'workjet_computers collection is not registered.',
  session: (what) => `Unsupported Workjet session control action: ${what}`,
  device: (what) => `Unsupported Workjet device control action: ${what}`,
};
export function unsupportedError(bridge, what, detail = null) {
  const message = (UNSUPPORTED_MESSAGES[bridge] || UNSUPPORTED_MESSAGES.project)(String(what ?? ''));
  const error = new Error(message);
  error.name = 'WorkjetUnsupported';
  error.code = UNSUPPORTED;
  error.bridge = bridge;
  error.what = String(what ?? '');
  if (detail) error.detail = String(detail);
  if (bridge === 'device') error.nativeCode = 'CTOX_WEBRTC_CAPABILITY_MISSING';
  return error;
}
export const isUnsupported = (error) => error?.code === UNSUPPORTED;

// Kernel antwortet auf eine noch fehlende Route mit unknown_command (E25). Der Stub meldet Fehltexte als
// „<code>: <grund>“; beide Formen werden erkannt.
function isUnknownCommand(error) {
  const text = typeof error === 'string' ? error : (error?.code || error?.error || error?.message || '');
  return /^unknown_command\b/.test(String(text));
}

const TERMINAL = new Set(['done', 'failed', 'cancelled']);
const ACCEPTED = new Set(['accepted', 'running', 'done', 'failed', 'cancelled']);
const V2_STATUS = { done: 'completed', failed: 'failed', cancelled: 'cancelled' };

function timeoutError(what, ms) {
  return Object.assign(new Error(`Native request ${what} exceeded ${ms} ms.`), { code: 'REQUEST_TIMEOUT', retryable: true });
}

// Wartet auf einen Befehlsstatus über das gepushte LiveDoc (CommandHandle.status), höchstens timeoutMs.
// onRunning(result) bekommt Zwischenstände (status 'running' mit result, E25 Diktat).
function awaitCommand(handle, { until = 'terminal', timeoutMs = 30_000, what, onRunning } = {}) {
  const wanted = until === 'accepted' ? ACCEPTED : TERMINAL;
  return new Promise((resolve, reject) => {
    let unsub = null;
    let settled = false;
    let lastPartial;
    const finish = (fn, value) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      try { unsub?.(); } catch { /* still */ }
      fn(value);
    };
    const timer = setTimeout(() => finish(reject, timeoutError(what, timeoutMs)), Math.max(1, timeoutMs));
    // Ablehnung schon beim Schreiben (Rechte, Schema): result endet rejected, status bleibt sonst queued.
    Promise.resolve(handle.result).then((r) => {
      if (r?.status === 'rejected') {
        const code = /forbidden/.test(String(r.reason || '')) ? 'forbidden' : (r.code || 'rejected');
        if (isUnknownCommand(r.reason) || isUnknownCommand(r.code)) finish(reject, unsupportedError('project', what, r.reason));
        else finish(reject, Object.assign(new Error(`Native command ${what} rejected: ${r.reason || r.detail || 'rejected'}`), { code }));
      }
    }, () => {});
    unsub = handle.status.subscribe((s) => {
      if (!s) return;
      if (s.status === 'running' && s.result !== undefined && s.result !== lastPartial && typeof onRunning === 'function') {
        lastPartial = s.result;
        try { onRunning(s.result); } catch { /* Beobachter ändern nichts */ }
      }
      if (wanted.has(s.status)) finish(resolve, s);
    });
  });
}

/**
 * @param {object} o
 * @param {object} o.kernel  adaptKernel(...)-Ergebnis (v4/shell/kernel-adapter.js) oder ein Kernel mit appApi
 * @param {string} [o.shellVer]
 * @param {string} [o.appId]  App-ID, unter der die Brücke Befehle schreibt (Standard: shell)
 */
// Lesebefehle ohne Nebenwirkung (gemessene Typliste wjb-command-types: project.list, kpis.read, exit_model.read,
// jour_fixe.meeting.read, presentation.read, supervisor.route.read|capabilities.vN, supervisor.turn.watch|capabilities,
// providers.list, computer.list, session.list, session.transfer.status).
export const READ_ONLY_COMMAND = /(^|\.)(list|read|watch|capabilities|status)(\.v\d+)?$/;

export function createV2Compat({ kernel, shellVer = 'dev', appId = 'shell' } = {}) {
  if (!kernel) throw new Error('createV2Compat: kernel fehlt');
  const gaps = new Set();
  let api = null;
  try { api = typeof kernel.appApi === 'function' ? kernel.appApi(appId, shellVer) : null; } catch { api = null; }
  if (!api) gaps.add('appApi');
  const identity = kernel.identity || {};
  const canCommand = typeof api?.command === 'function';
  const canQuery = typeof api?.query === 'function';
  if (!canCommand) gaps.add('command');
  if (!canQuery) gaps.add('query');

  // ---- Sitzung und Akteur ---------------------------------------------------------------------------------
  // Ein Sitzungsobjekt je Brücken-Lebensdauer. Wechsel von Instanz/Nutzer startet im v4-Kernel eine neue Shell und
  // damit eine neue Brücke; dispose() setzt die Sitzung auf null, damit laufende V2-Zäune („session changed“) greifen.
  const session = Object.freeze({ kernel_instance_id: identity.instance_id ?? null, user_id: identity.user_id ?? null });
  function actorContext(s) {
    if (!s || s !== state.session) return { id: '', email: '', login: '', display_name: '', role: '', is_admin: false };
    const actor = api?.actor || {};
    const id = String(actor.id || identity.user_id || '');
    return {
      id,
      email: '',
      login: '',
      display_name: String(actor.display_name || id),
      role: String(actor.role || ''),
      is_admin: actor.role === 'admin',
    };
  }

  // ---- Lesepfad ---------------------------------------------------------------------------------------------
  // kernel.query ist lokal (nie Netzwerk). „Bereit“ heißt in v4: Collection-Liveness live (§16.45); ohne Liveness-
  // Angabe (Labor) gilt die lokale Sicht als bereit. Abfrage-Generation = collections[name].gen.
  const liveness = (name) => {
    const m = api?.metrics?.get?.();
    return m?.collections?.[name] ?? null;
  };
  const toQuery = (q = {}) => {
    const out = {};
    if (q.selector) out.selector = q.selector;
    if (q.sort) out.sort = q.sort;
    if (Number.isSafeInteger(q.limit)) out.limit = q.limit;
    if (Number.isSafeInteger(q.skip) && q.skip > 0) out.skip = q.skip;
    return out;
  };
  const unsupportedRead = (name) => unsupportedError('project', `query ${name}`, 'kernel.query fehlt');
  function collection(name) {
    if (!canQuery) return null;
    return {
      name,
      demandLoader: Object.freeze({ kernel: true, collection: name }),
      schema: { primaryPath: 'id' },
      find(q = {}) {
        return { async exec() { if (q.signal?.aborted) throw Object.assign(new Error('Query aborted.'), { code: 'QUERY_CANCELLED' }); return api.query(name, toQuery(q)).get(); } };
      },
      findOne(id) {
        return { async exec() { return typeof api.get === 'function' ? api.get(name, String(id)).get() ?? null : null; } };
      },
      $: {
        subscribe(cb) {
          const unsub = api.query(name, {}).subscribe((rows) => { for (const row of rows) cb({ documentData: row }); });
          return { unsubscribe: unsub };
        },
      },
    };
  }
  const collections = new Map();
  const db = canQuery ? {
    collection(name) {
      if (!collections.has(name)) collections.set(name, collection(name));
      return collections.get(name);
    },
  } : null;

  // ---- Dateibereiche (WJB-R, E26 / K-WJ-2) -------------------------------------------------------------------
  // V2 liest Dateibytes über peer.demandFileLoader.fetchFile(fileId, {range}) mit range = {offset, length}
  // (rxdb/src/file-demand-loader.mjs:47 @ e35a116ae) und erhält eine Chunkliste [{sequence, bytesBase64, hash}], die
  // shared/file-integrity.js readStoredFileFromDemandChunks prüft (sequence lückenlos ab 0, hash = sha256 der
  // dekodierten Bytes). v4: api.files.get({id, range:{start, end}}), end exklusiv (index.d.ts 637caf6 Z. ~169). Der Kern
  // prüft die Inhaltsadresse selbst (Stub: ganze Datei vor dem Schnitt, Kern: je Transferchunk); hier entsteht genau
  // EIN Chunk über den ganzen Bereich, sein hash beschreibt die gelieferten Bytes. Unbekannte Datei (found:false) →
  // leere Liste: der V2-Leser meldet dann wie bei fehlenden Chunks „Dateiinhalt fehlt.“ (ctox_file_chunk_missing).
  // Kernfehler (invalid/path range, forbidden) gehen unverändert an den Aufrufer.
  const canFiles = typeof api?.files?.get === 'function';
  if (!canFiles) gaps.add('files');
  const toBase64 = (bytes) => {
    let text = '';
    for (let offset = 0; offset < bytes.length; offset += 16_384) {
      text += String.fromCharCode(...bytes.subarray(offset, offset + 16_384));
    }
    return btoa(text);
  };
  const demandFileLoader = canFiles ? Object.freeze({
    async fetchFile(fileId, { range = null } = {}) {
      const sel = { id: String(fileId ?? '') };
      if (range != null) sel.range = { start: range.offset, end: range.offset + range.length };
      const got = await api.files.get(sel);
      if (!got?.found) return [];
      const bytes = got.bytes instanceof Uint8Array ? got.bytes : new TextEncoder().encode(String(got.text ?? ''));
      return [{ sequence: 0, bytesBase64: toBase64(bytes), hash: await sha256Hex(bytes) }];
    },
  }) : null;

  function startCollection(name) { // v4-allow: debt L-42
    if (!canQuery) return Promise.reject(unsupportedRead(name));
    const coll = db.collection(name);
    const peer = {
      demandFileLoader, // WJB-R: eigenes Feld, collection.demandLoader bleibt die Generationsmarke
      cancelled: false,
      activeRemotePeerId: 'kernel',
      collection: coll,
      collectionQueryGenerationToken() {
        const l = liveness(name);
        // Generation der Collection (§16.4); ohne Liveness-Eintrag (lazy) gilt g1, damit ein spät erscheinender
        // Eintrag keinen Generationswechsel vortäuscht.
        return `${name}@${l?.gen ?? 1}`;
      },
      awaitQueryReady(budgetMs = 15_000) {
        return new Promise((resolve, reject) => {
          const check = (m) => {
            const l = m?.collections?.[name];
            if (!l || l.state === 'live') return 'ready';
            if (l.state === 'error') return 'error';
            return null;
          };
          const now = check(api.metrics?.get?.());
          if (now === 'ready') return resolve();
          if (now === 'error' || !api.metrics?.subscribe) {
            return reject(Object.assign(new Error('Native WebRTC peer is not connected'), { code: 'PEER_UNAVAILABLE' }));
          }
          let unsub = null;
          const timer = setTimeout(() => { try { unsub?.(); } catch { /* still */ } reject(timeoutError(`query ${name}`, budgetMs)); }, Math.max(1, budgetMs));
          unsub = api.metrics.subscribe((m) => {
            const r = check(m);
            if (!r) return;
            clearTimeout(timer);
            queueMicrotask(() => { try { unsub?.(); } catch { /* still */ } });
            if (r === 'ready') resolve(); else reject(Object.assign(new Error('Native WebRTC peer is not connected'), { code: 'PEER_UNAVAILABLE' }));
          });
        });
      },
      // V2 verbietet historische Pulls als Gate; v4 hat keinen.
      async awaitInSync() { return true; },
    };
    return Promise.resolve({ state: peer });
  }

  // ---- Befehlspfad ------------------------------------------------------------------------------------------
  // V2-Quittung: {command_id, ok, status: completed|failed|cancelled, target_record_id, payload, result, error}.
  // command_id ist die vom V2-Aufrufer vergebene ID (Korrelation über den Befehls-Handle; der v4-Kernel vergibt die
  // Dokument-ID selbst und steht in kernel_command_id). payload/record_id kommen aus dem gespeicherten
  // business_commands-Dokument, wenn der Kernel es lesbar macht, sonst aus dem gesendeten Befehl.
  async function dispatch(command, options = {}) {
    if (!canCommand) throw unsupportedError('project', command?.command_type, 'kernel.command fehlt');
    const type = String(command?.command_type || command?.type || '');
    const payload = command?.payload ?? {};
    // record_id null/undefined = ohne Ziel (V2 sendete null nie als Text 'null'; WJB-D Provider).
    const opts = command?.record_id != null ? { record_id: String(command.record_id) } : {};
    // WJB-R (E26, K-WJ-4): die V2-Befehls-ID ist der Idempotenzschlüssel. Workjet wiederholt mit derselben commandId;
    // der Kern liefert dann den vorhandenen Befehl (SHA-256(app|user|key) → UUIDv8), statt einen zweiten anzulegen.
    // Nur Text mit 1–128 Zeichen (Kernvertrag); sonst ohne Schlüssel wie bisher.
    // Reine Lesebefehle (…list, …read, …watch, …capabilities, …status, auch mit .vN) bekommen KEINEN Schlüssel: ohne
    // Nebenwirkung schützt er nichts, und Workjet fragt mit derselben ID wiederholt nach (z. B. retryJourFixeRead bis
    // 5 min) – mit Schlüssel käme immer der erste, veraltete Stand zurück (Workjet-Sitzung 16.10.).
    const idempotencyKey = READ_ONLY_COMMAND.test(type) ? null : (command?.command_id ?? command?.id);
    if (typeof idempotencyKey === 'string' && idempotencyKey.length > 0 && idempotencyKey.length <= 128) {
      opts.idempotency_key = idempotencyKey;
    }
    const handle = api.command(type, JSON.parse(JSON.stringify(payload)), opts);
    const s = await awaitCommand(handle, { until: options.until || 'terminal', timeoutMs: options.timeoutMs ?? 30_000, what: type });
    if (s.status === 'failed' && isUnknownCommand(s.error)) throw unsupportedError('project', type, s.error);
    let stored = null;
    try { stored = typeof api.get === 'function' ? api.get('business_commands', handle.id).get() : null; } catch { stored = null; }
    const sentPayload = JSON.parse(JSON.stringify(payload));
    const echoed = stored?.payload ?? sentPayload;
    return {
      id: command?.id ?? command?.command_id ?? handle.id,
      command_id: command?.command_id ?? command?.id ?? handle.id,
      kernel_command_id: handle.id,
      command_type: type,
      transport: 'rxdb-command-bus', // wie V2-Quittung: kernel.command ist die RxDB/WebRTC-Befehlsebene (WJB-D)
      status: V2_STATUS[s.status] || s.status,
      // wie V2 commandReceipt (command-bus.js:1653-1664): angenommen/laufend bei until 'accepted' ist ok, nur failed/cancelled nicht
      ok: s.status !== 'failed' && s.status !== 'cancelled',
      target_record_id: stored?.record_id ?? opts.record_id ?? null,
      payload: command?.inbound_channel ? { ...echoed, inbound_channel: command.inbound_channel } : echoed,
      ...(s.result !== undefined ? { result: s.result } : {}),
      ...(s.error !== undefined ? { error: s.error } : {}),
    };
  }

  // ---- Nativkanal (E25) ---------------------------------------------------------------------------------------
  // Jede V2-requestNative-Methode ist ein typisierter Kern-Befehl gleichen Namens: command(type=method, payload=params).
  // Antwort = Befehlsvollendung {status: done|failed|cancelled, result?, error?} (§16.48). Zwischenstände (Diktat:
  // status 'running', result {partial…}) gehen an opts.onPartial; das Promise liefert wie in V2 genau die Antwort.
  // Große Ergebnisse/Audio kommen als BlobRef (files.ref) im result; der Aufrufer liest sie über files.get.
  // Unbekannte Route → unknown_command → {code:'unsupported'}. opts.requiredCapability prüft der Kernel selbst
  // (Manifest/Fähigkeit), die Brücke reicht sie nicht weiter.
  const nativeCalls = [];
  // Ohne idempotency_key (WJB-R): Nativaufrufe sind je Aufruf (V2 requestNative entdoppelte nie) und tragen keine
  // V2-commandId als Befehls-ID; ein Schlüssel aus den Parametern würde z. B. Sprachschreibvorgänge verschlucken.
  async function requestNative(method, params = {}, opts = {}) { // v4-allow: debt L-42
    const name = String(method || '');
    nativeCalls.push(name);
    if (!canCommand) throw unsupportedError('project', name, 'kernel.command fehlt');
    const handle = api.command(name, JSON.parse(JSON.stringify(params ?? {})), {});
    const s = await awaitCommand(handle, {
      until: 'terminal', timeoutMs: opts.timeoutMs ?? 30_000, what: name, onRunning: opts.onPartial, // v4-allow: debt L-42
    });
    if (s.status === 'done') return s.result ?? null;
    if (s.status === 'failed' && isUnknownCommand(s.error)) throw unsupportedError('project', name, s.error);
    // Wie V2 (requestNative reichte den Nativfehler roh durch): Nachricht = Kernfehler ohne Präfix `<code>:`, code aus
    // dem Präfix. Workjet vergleicht Texte wörtlich (Gerät: „workjet device management is not allowed“ → forbidden).
    const text = String(s.error || s.status);
    const prefix = /^([a-z_]+):\s*/.exec(text);
    const code = prefix?.[1] || (s.status === 'cancelled' ? 'cancelled' : 'native_failed');
    throw Object.assign(new Error(prefix ? text.slice(prefix[0].length) || text : text), { code, status: s.status, method: name });
  }

  const sync = canCommand || canQuery ? {
    config: { instance_id: identity.instance_id ?? null },
    requestNative, // v4-allow: debt L-42
    startCollection, // v4-allow: debt L-42
  } : null;

  const state = {
    session,
    db,
    sync,
    syncConfig: { instance_id: identity.instance_id ?? null },
    commandBus: canCommand ? { dispatch } : null, // v4-allow: debt L-42
  };

  let seq = 0;
  const newId = () => globalThis.crypto?.randomUUID?.() || `${Date.now()}_${(seq += 1)}_${Math.random().toString(36).slice(2)}`;

  return {
    state,
    actorContext,
    newId,
    now: () => Date.now(),
    gaps,
    api,
    /** Protokoll der Nativ-Methodennamen (Abnahmeliste E25). */
    nativeCalls,
    dispose() {
      state.session = null;
      state.db = null;
      state.sync = null;
      state.commandBus = null; // v4-allow: debt L-42
    },
  };
}
