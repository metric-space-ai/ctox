// Shell v4 – Quelltext-Modus: App-Upload (Entwurf §8 letzter Punkt, Architektur §9.2 Schritt 1).
// Ganze App als Ordner (input type=file multiple webkitdirectory, ohne ZIP) → Entwurf → Veröffentlichen.
//
//   readUploadedFiles(fileList) → Promise<{ folder, files: {path: text}, appId }>
//   openAppUpload({ root, kernel, api, zone }) → { close() }   (Fläche für „neue App“ im Fenster eines Rahmens)

const SCHEMA_FILE = 'collections.schema.json';
const MANIFEST_FILE = 'manifest.json';
// Vom Kern verwaltete Manifest-Felder: nie per setManifest senden (Schutzfelder → invalid; collections kommt aus dem Schema).
const SYSTEM_KEYS = new Set(['app_id', 'app_ver', 'files', 'collections', 'generations', 'retires', 'provenance', 'author', 'created_at']);
const MAX_FILE_BYTES = 2 * 1024 * 1024;
const SKIP = /(^|\/)(\.|node_modules\/|__MACOSX\/)/;

export async function readUploadedFiles(fileList) {
  const list = [...(fileList ?? [])];
  const rel = list.map((f) => (f.webkitRelativePath || f.name).replace(/\\/g, '/'));
  // Gemeinsamen ersten Ordner abschneiden (webkitdirectory liefert „<ordner>/<pfad>“).
  const firsts = new Set(rel.map((p) => (p.includes('/') ? p.split('/')[0] : '')));
  const folder = firsts.size === 1 && !firsts.has('') ? [...firsts][0] : '';
  const files = {};
  for (let i = 0; i < list.length; i++) {
    const path = folder ? rel[i].slice(folder.length + 1) : rel[i];
    if (!path || SKIP.test(path) || list[i].size > MAX_FILE_BYTES) continue;
    files[path] = await list[i].text();
  }
  let appId = '';
  for (const [p, key] of [['manifest.json', 'app_id'], ['module.json', 'id']]) {
    if (appId || !files[p]) continue;
    try { appId = String(JSON.parse(files[p])[key] ?? ''); } catch { /* kein JSON: Ordnername */ }
  }
  appId = (appId || folder).toLowerCase().replace(/[^a-z0-9_-]+/g, '-').replace(/^-+|-+$/g, '');
  return { folder, files, appId };
}

export function createFolderInput(onFiles) {
  const input = document.createElement('input');
  input.type = 'file';
  input.multiple = true;
  input.webkitdirectory = true;
  input.setAttribute('webkitdirectory', '');
  input.hidden = true;
  input.addEventListener('change', async () => {
    if (input.files?.length) await onFiles(await readUploadedFiles(input.files));
    input.value = '';
  });
  return input;
}

/**
 * Schreibt eine hochgeladene App als eigenen Entwurf und veröffentlicht sie (§16.22 A3: neue App = base_app_ver null).
 * Ein Ordner ist die ganze App: Dateien der Basis, die im Ordner fehlen, werden im Entwurf gelöscht (draft.remove).
 * collections.schema.json geht über draft.setCollections (Kern prüft und schreibt die Datei). manifest.json ist im
 * Entwurf keine Datei (§16.26): ihre Felder gehen per draft.setManifest in den Entwurf, Handler als ganze Karte
 * (Basis-Handler, die der Ordner nicht mehr nennt = null). Verweise auf fehlende Dateien lehnt publish mit detail ab.
 */
export async function publishUpload(kernel, { appId, files }) {
  const src = kernel.source(appId);
  const base = kernel.apps.get().find((a) => a.id === appId)?.active_ver ?? null;
  if (await src.draft.info()) await src.draft.discard(); // sauber auf der aktiven Version beginnen
  if (base) for (const p of await src.list(base)) if (p !== MANIFEST_FILE && !(p in files)) await src.draft.remove(p);
  for (const [p, text] of Object.entries(files)) {
    if (p === MANIFEST_FILE) continue;
    if (p === SCHEMA_FILE) await src.draft.setCollections(text);
    else await src.draft.write(p, text);
  }
  if (typeof files[MANIFEST_FILE] === 'string') {
    const fields = JSON.parse(files[MANIFEST_FILE]);
    if (!fields || typeof fields !== 'object' || Array.isArray(fields)) throw Object.assign(new Error(`${MANIFEST_FILE}: JSON-Objekt erwartet`), { code: 'invalid' });
    const patch = Object.fromEntries(Object.entries(fields).filter(([k]) => !SYSTEM_KEYS.has(k)));
    if (patch.handlers && typeof patch.handlers === 'object') {
      const baseHandlers = base ? (await kernel.artifacts?.manifest?.(appId, String(base)))?.handlers ?? {} : {};
      patch.handlers = { ...Object.fromEntries(Object.keys(baseHandlers).filter((t) => !(t in patch.handlers)).map((t) => [t, null])), ...patch.handlers };
    }
    if (Object.keys(patch).length) await src.draft.setManifest(patch);
  }
  return src.draft.publish({ base_app_ver: base }).result;
}

// Icon im hochgeladenen Paket: manifest.json icon_svg/icon_asset (bzw. layout.*) und, bei icon_asset, die Datei selbst.
export function hasIcon(files) {
  let m = null;
  try { m = typeof files?.[MANIFEST_FILE] === 'string' ? JSON.parse(files[MANIFEST_FILE]) : null; } catch { m = null; }
  const layout = { ...(m?.layout || {}), ...(m || {}) };
  if (typeof layout.icon_svg === 'string' && /<svg\b/i.test(layout.icon_svg)) return true;
  const asset = typeof layout.icon_asset === 'string' ? layout.icon_asset.replace(/^\.?\/+/, '') : '';
  return !!asset && asset in (files || {});
}

export function openAppUpload({ root, kernel, api, zone }) {
  const sink = zone ?? { report: (i) => api?.status?.report(i), clear: (c) => api?.status?.clear(c) };
  const el = document.createElement('div');
  el.className = 'v4-src v4-src-upload';
  el.dataset.v4SourceUpload = '';
  el.innerHTML = `
    <div class="v4-src__bar">
      <button type="button" class="v4-src__btn" data-pick>App-Ordner wählen</button>
      <input class="v4-src__input" data-app-id aria-label="App-ID" placeholder="app-id" spellcheck="false" autocomplete="off" hidden>
      <span class="v4-src__spacer"></span>
      <button type="button" class="v4-src__btn v4-src__btn--primary" data-publish disabled>Veröffentlichen</button>
    </div>
    <ul class="v4-src-upload__files" data-files></ul>`;
  let pending = null;
  const pick = el.querySelector('[data-pick]');
  const idInput = el.querySelector('[data-app-id]');
  const publishBtn = el.querySelector('[data-publish]');
  const filesEl = el.querySelector('[data-files]');
  const input = createFolderInput(async (up) => {
    pending = up;
    idInput.hidden = false;
    idInput.value = up.appId;
    filesEl.replaceChildren(...Object.keys(up.files).sort().map((p) => {
      const li = document.createElement('li');
      li.textContent = p;
      return li;
    }));
    sync();
  });
  el.append(input);
  const canEdit = () => (api?.actor?.permissions?.get?.() ?? []).includes('apps.source.edit');
  const sync = () => { publishBtn.disabled = !pending || !idInput.value.trim() || !canEdit(); };
  pick.addEventListener('click', () => input.click());
  idInput.addEventListener('input', sync);
  publishBtn.addEventListener('click', async () => {
    publishBtn.disabled = true;
    const appId = idInput.value.trim();
    const published = pending.files;
    // M-09 (Bedienkonzept §Icons: „Jede App hat genau ein Icon im App-Paket, Pflicht beim Hochladen“): ohne Icon kein
    // Veröffentlichen – sonst zeigt die Shell einen Buchstaben statt des Logos. Grund in der Zone, Auswahl bleibt stehen.
    if (!hasIcon(published)) {
      sink.report({ level: 'error', code: 'source.upload', text: `App „${appId}“ nicht veröffentlicht: Icon fehlt (manifest.json: icon_svg oder icon_asset auf eine Datei im Paket).` });
      sync();
      return;
    }
    const r = await publishUpload(kernel, { appId, files: pending.files }).catch((e) => ({ status: 'rejected', reason: e?.code ?? 'error', detail: e?.message }));
    if (r.status === 'accepted') {
      sink.clear('source.upload');
      pending = null;
      filesEl.replaceChildren();
      idInput.hidden = true;
      el.dataset.lastPublished = `${appId}@${r.rev}`;
      sink.clear('source.icon');
    } else {
      sink.report({ level: 'error', code: 'source.upload', text: `App „${appId}“ nicht veröffentlicht (${r.reason})${r.detail ? `: ${r.detail}` : ''}` });
    }
    sync();
  });
  const off = api?.actor?.permissions?.subscribe?.(sync) ?? (() => {});
  root.append(el);
  return { el, close() { off(); el.remove(); } };
}
