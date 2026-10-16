// Ladeweg-Labor: Seite, die Apps auf einem der beiden Wege montiert (?mode=sw | ?mode=blob).
// Nur Laborcode; die Produktion baut den App-Host (Strang P2) auf diesen Wegen auf.
import * as K from './kernel-attrappe.js';
import { createBlobLoader } from './blob-loader.js';

const mode = new URLSearchParams(location.search).get('mode') || 'sw';
const now = () => performance.timeOrigin + performance.now();
const slots = new Map(); // slot -> { mod, handle, root, app, ver }
let loader = null;
const events = [];

// Bindung je Mount wie kernel.artifacts.bind (K-1d): die beim Mount aktive Shell, eingefroren fuer die Instanz.
// Eine ungebundene URL /~a/<app>@<ver>/… gibt es nicht mehr (der SW loest ctox:shared/ nur ueber ~shell@ auf);
// opts.pinShell ist damit der Regelfall und bleibt nur fuer measure.mjs (c) als Name stehen.
async function entryUrl(app, ver) {
  const shellVer = await K.getActive('shell');
  if (mode === 'blob') return loader.entryUrl(app, ver, 'index.js', { shellVer, libs: {} });
  return `/~a/${app}@${ver}~shell@${shellVer}/index.js`;
}

async function mountInto(app, ver, restored, opts) {
  const t0 = performance.now();
  const url = await entryUrl(app, ver, opts);
  const mod = await import(url);
  const t1 = performance.now();
  const root = document.createElement('div');
  root.className = 'app-root';
  document.getElementById('lab').append(root);
  const handle = await mod.mount(root, { version: '4.0', app: { id: app, ver } }, { restored });
  return { mod, handle, root, app, ver, importMs: t1 - t0, totalMs: performance.now() - t0 };
}

window.lab = {
  mode,
  events,
  async init() {
    const t0 = performance.now();
    await K.openKernelDb();
    if (mode === 'sw') {
      if (!('serviceWorker' in navigator)) throw new Error('kein navigator.serviceWorker');
      if (!navigator.serviceWorker.controller) {
        const changed = new Promise((res) => navigator.serviceWorker.addEventListener('controllerchange', res, { once: true }));
        const reg = await navigator.serviceWorker.register(`/sw.js?db=${K.DB_NAME}`, { scope: '/' });
        // Harter Neuladen: SW aktiv, Seite trotzdem unkontrolliert -> um claim() bitten.
        if (reg.active) reg.active.postMessage('ctox:claim');
        await changed;
      }
    } else {
      loader = createBlobLoader({ readFiles: K.readFiles, readArtifact: K.readArtifact });
    }
    return { initMs: performance.now() - t0, controlled: !!navigator.serviceWorker?.controller };
  },
  async install(spec) {
    const t0 = performance.now();
    // Inhalte kommen als base64 (Binaerdateien) oder Text aus dem Messskript.
    const files = {};
    for (const [p, v] of Object.entries(spec.files)) files[p] = v.b64 ? Uint8Array.from(atob(v.b64), (c) => c.charCodeAt(0)) : v.text;
    const manifest = await K.installVersion({ app_id: spec.app_id, app_ver: spec.app_ver, files });
    return { ms: performance.now() - t0, manifest };
  },
  setActive: (app, ver) => K.setActive(app, ver),
  tamper: (sha, text) => K.tamperArtifact(sha, text),
  async mount(slot, app, ver, opts = {}) {
    const inst = await mountInto(app, ver, undefined, opts);
    slots.set(slot, inst);
    return { importMs: inst.importMs, totalMs: inst.totalMs, probe: inst.handle.probe() };
  },
  /** Live-Remount nach §5: snapshot alt -> neu montieren (neues CSS zuerst) -> alt unmount. */
  async swap(slot, app, ver, opts = {}) {
    const t0 = performance.now();
    const old = slots.get(slot);
    const restored = old?.handle.snapshot?.();
    const inst = await mountInto(app, ver, restored, opts);
    if (old) { await old.handle.unmount(); old.root.remove(); }
    slots.set(slot, inst);
    return { ms: performance.now() - t0, importMs: inst.importMs, probe: inst.handle.probe(), oldModule: old?.mod.moduleState?.() };
  },
  setNote(slot, text) { slots.get(slot).root.querySelector('input[name="note"]').value = text; },
  probe: (slot) => slots.get(slot).handle.probe(),
  moduleState: (slot) => slots.get(slot).mod.moduleState?.(),
  async unmount(slot) { const s = slots.get(slot); if (s) { await s.handle.unmount(); s.root.remove(); slots.delete(slot); if (mode === 'blob') await loader.revoke(s.app, s.ver); } },
  async status(path) { try { return (await fetch(path)).status; } catch (e) { return `error:${e.message}`; } },
  async tryMount(slot, app, ver, opts = {}) {
    try { return { ok: true, ...(await this.mount(slot, app, ver, opts)) }; } catch (e) { return { ok: false, error: String(e && e.message || e) }; }
  },
  warnings: () => (loader ? [...loader.warnings] : []),
  blobUrlCount: () => (loader ? loader.size() : 0),
  // Mehr-Tab: Leiter-Tab setzt Aktivierung und meldet sie; andere Tabs remounten.
  listen(slot) {
    const bc = new BroadcastChannel('ctox-v4-lab');
    bc.onmessage = async ({ data }) => {
      const r = await this.swap(slot, data.app, data.ver);
      events.push({ ...data, doneAt: now(), version: r.probe.version, greet: r.probe.greet });
    };
  },
  async activateAndAnnounce(app, ver) {
    const t = now();
    await K.setActive(app, ver);
    new BroadcastChannel('ctox-v4-lab').postMessage({ app, ver, sentAt: t });
    return t;
  },
  now,
};
document.documentElement.dataset.labReady = '1';
