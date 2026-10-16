// App-API-4.0-Anbindung des outbound-Moduls (Shell v4, Strang UD1).
// Einzige Stelle, an der index.js und active-outreach.js die App-API berühren: lokaler Spiegel (api.query/get),
// Schreiben (api.put/patch/remove), Befehle (api.command, nie warten), Zustandszone, Dialoge im App-Fenster.

let api = null;

export function bindApi(next) {
  api = next || null;
}

export function currentApi() {
  return api;
}

function byUpdatedDesc(a, b) {
  return (b?.updated_at_ms || 0) - (a?.updated_at_ms || 0);
}

// Aktueller lokaler Stand einer Collection (kein Netz, kein Zeilendeckel), sortiert wie früher findAll().
export function readAll(collection) {
  if (!api || !collection) return [];
  try {
    return api.query(collection, {}).get().map((doc) => ({ ...doc })).sort(byUpdatedDesc);
  } catch (_error) {
    return [];
  }
}

export function readOne(collection, id) {
  if (!api || !collection || !id) return null;
  try {
    const doc = api.get(collection, String(id)).get();
    return doc ? { ...doc } : null;
  } catch (_error) {
    return null;
  }
}

// Live-Abo einer Collection; liefert die Abmeldung. Zeilen kommen sortiert wie readAll().
export function liveCollection(collection, cb) {
  if (!api || !collection) return () => {};
  try {
    return api.query(collection, {}).subscribe((rows) => cb(rows.map((doc) => ({ ...doc })).sort(byUpdatedDesc)));
  } catch (_error) {
    cb([]);
    return () => {};
  }
}

function watchWrite(handle, code) {
  handle?.local?.catch?.((error) => {
    reportStatus('error', code, error?.message || String(error));
  });
  return handle;
}

function cleanDoc(doc) {
  return JSON.parse(JSON.stringify(doc ?? {}));
}

export function putDoc(collection, doc) {
  if (!api || !collection || !doc?.id) return null;
  return watchWrite(api.put(collection, cleanDoc(doc)), `outbound.write.${collection}`);
}

// Wie früher incrementalPatch: nur bestehende Dokumente; fehlt das Dokument, passiert nichts.
export function patchDocById(collection, id, patch) {
  if (!api || !collection || !id) return null;
  if (!readOne(collection, id)) return null;
  return watchWrite(api.patch(collection, String(id), cleanDoc(patch)), `outbound.write.${collection}`);
}

// Wie früher upsert: bestehend → zusammenführen (created_at_ms bleibt), sonst anlegen.
export function upsertDocById(collection, doc) {
  if (!doc?.id) return null;
  const existing = readOne(collection, doc.id);
  if (existing) return patchDocById(collection, doc.id, { ...doc, created_at_ms: existing.created_at_ms || doc.created_at_ms });
  return putDoc(collection, doc);
}

export function removeDocById(collection, id) {
  if (!api || !collection || !id) return null;
  if (!readOne(collection, id)) return null;
  return watchWrite(api.remove(collection, String(id)), `outbound.write.${collection}`);
}

// ---------------------------------------------------------------- Zustandszone

export function reportStatus(level, code, text, ref) {
  if (!api || !code || !text) return;
  const item = { level, code, text: String(text) };
  if (ref) item.ref = String(ref);
  api.status.report(item);
}

export function clearStatus(code) {
  if (!api || !code) return;
  api.status.clear(code);
}

// ---------------------------------------------------------------- Befehle

const openCommandSubs = new Set();

export function disposeCommandSubscriptions() {
  for (const off of openCommandSubs) {
    try { off(); } catch (_error) { /* Abo schon beendet */ }
  }
  openCommandSubs.clear();
}

// Schickt einen Befehl ab, ohne zu warten. Ergebnis und Fehler kommen über den gepushten Status:
// failed → genau ein Zoneneintrag `outbound.command.<type>`, done → Eintrag geräumt.
// `settled` ist ein Hintergrund-Promise (done → Zustand, failed → Fehler) für fachlich abhängige Folgebefehle;
// die UI wartet nie darauf.
export function sendCommand(type, payload = {}, options = {}) {
  const code = options.code || `outbound.command.${type}`;
  if (!api) {
    const error = new Error('App-API fehlt');
    return { id: '', handle: null, settled: Promise.reject(error) };
  }
  const recordId = options.record_id ?? payload.record_id;
  const body = cleanDoc({
    ...payload,
    ...(recordId !== undefined ? { record_id: String(recordId || '') } : {}),
    client_context: { source_module: 'outbound', ...(options.client_context || {}) },
  });
  const handle = api.command(type, body, recordId ? { record_id: String(recordId) } : {});
  let off = () => {};
  const settled = new Promise((resolve, reject) => {
    handle.local.catch((error) => {
      reportStatus('error', code, error?.message || String(error), handle.id);
      reject(error);
    });
    off = handle.status.subscribe((state) => {
      if (!state) return;
      if (state.status === 'failed') {
        reportStatus('error', code, state.error || `${type} fehlgeschlagen`, handle.id);
        stop();
        reject(new Error(state.error || `${type} fehlgeschlagen`));
      } else if (state.status === 'done') {
        clearStatus(code);
        stop();
        resolve(state);
      }
    });
    openCommandSubs.add(off);
  });
  function stop() {
    try { off(); } catch (_error) { /* schon beendet */ }
    openCommandSubs.delete(off);
  }
  settled.catch(() => {});
  if (typeof options.onDone === 'function') settled.then(options.onDone, () => {});
  if (typeof options.onFailed === 'function') settled.catch(options.onFailed);
  return { id: handle.id, handle, settled };
}

// ---------------------------------------------------------------- Dialoge (nur im App-Fenster)

export async function confirmDialog(text, options = {}) {
  if (!api) return false;
  const answer = await api.shell.dialog({
    kind: 'confirm',
    title: options.title || 'Outbound',
    text: String(text || ''),
    confirm_label: options.confirm_label || (options.danger ? 'Löschen' : 'OK'),
    danger: Boolean(options.danger),
    default: false,
  });
  return answer === true;
}

export async function promptDialog(label, defaultValue = '') {
  if (!api) return null;
  const answer = await api.shell.dialog({
    kind: 'prompt',
    title: 'Outbound',
    text: String(label || ''),
    value: String(defaultValue || ''),
    default: null,
  });
  if (answer === null || answer === undefined || answer === false) return null;
  if (typeof answer === 'object' && 'value' in answer) return answer.value == null ? null : String(answer.value);
  return String(answer);
}
