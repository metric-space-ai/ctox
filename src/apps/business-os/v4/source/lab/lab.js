// Labor-Loader Quelltext-Modus: eingefrorener Kern-Stub (/stub/, Pin 9fa9948) + Laborfenster + Quelltext-Modus.
//   ?user=<id>&role=admin|user&editor=auto|textarea
// Alle Tabs teilen eine Stub-DB (dbName) = „ein Core“; der Akteur kommt je Tab aus den Parametern.
// App-Inhalte montiert der Testkit-Host des Stubs (Live-Remount bei Aktivierung, Snapshot, 9-s-Rückfall) – unabhängig
// vom Rahmen. Die Quelltext-API ist die des Kerns (kernel.source, §16.22) ohne Attrappe.
import { createStubKernel } from '/stub/ctox-app-api-stub.mjs';
import { createAppHost } from '/stub/testkit/app-host.mjs';
import { createLabWindow } from './lab-window.js';

const q = new URLSearchParams(location.search);
const user = q.get('user') || 'ada';
const role = q.get('role') || 'admin';
const editor = q.get('editor') || 'auto';
const permissions = role === 'admin' ? ['apps.source.view', 'apps.source.edit'] : [];

const stub = createStubKernel({ instanceId: 'lab', userId: user, dbName: 'ctox-lab-q', latencyMs: 15, actor: { id: user, display_name: user, role, permissions } });
await stub.ready;
const sk = stub.shell;
// Shell-Code (Quelltext-Modus) lädt wie im echten Ladeweg von der Instanz-URL der Shell, die der Kern baut:
// /~a/shell@lab~lib:ctox-office@1.0~lib:monaco@0.0/v4/source/… (artifacts.bind). Der Labor-Server liefert sie mit der
// Umschreibung des SW aus (v4/lab/sw-lab.mjs); so löst editor.js 'ctox:lib/monaco@0/' auf /~a/lib:monaco@0.0…/ auf.
// Ohne diese Bindung bliebe das Literal stehen und der Editor fiele still auf die textarea zurück (MONACO.md).
const LAB_LIBS = { 'ctox-office': '1.0', monaco: '0.0' };
const known = new Set(sk.apps.get().map((a) => a.id));
for (const [name, ver] of Object.entries(LAB_LIBS)) {
  if (!known.has(`lib:${name}`)) await stub.sim.publishAppVersion(`lib:${name}`, ver, { title: `lib:${name}`, styles: [] });
}
if (!known.has('shell')) {
  await stub.sim.publishAppVersion('shell', 'lab', { title: 'Shell', styles: [], manifestExtra: { libs: Object.fromEntries(Object.entries(LAB_LIBS).map(([n, v]) => [n, v.split('.')[0]])) } });
}
await stub.sim.settle?.();
const artifacts = stub.artifacts ?? sk.artifacts;
const shellBinding = artifacts.bind('shell', 'lab');
const shellUrl = (p) => shellBinding.url(p);
const [{ attachSourceToggle }, { openAppUpload }, { parseCollectionsFile, toManifestCollections }] = await Promise.all([
  import(shellUrl('v4/source/chrome-toggle.js')), import(shellUrl('v4/source/upload.js')), import(shellUrl('v4/source/schema-diff.js')),
]);
const host = createAppHost(stub);
const desktop = document.createElement('main');
desktop.className = 'lab-desktop';
document.body.append(desktop);
const windows = new Map();
const mirrors = new Map();
const mirrorFor = (appId) => { if (!mirrors.has(appId)) mirrors.set(appId, stub.appApi(appId, 'shell-mirror')); return mirrors.get(appId); };
const viewport = () => ({ x: 0, y: 0, w: innerWidth, h: innerHeight });
let seq = 0;

async function fetchFiles(dir, paths) {
  const out = {};
  for (const p of paths) out[p] = await (await fetch(`/business-os/v4/source/lab/apps/${dir}/${p}`)).text();
  return out;
}

window.__lab = {
  user, role, stub, sk, host, windows, shellRoot: shellBinding.root,
  toggles: { get: (id) => windows.get(id)?.toggle },
  isLeader: () => sk.leader.get().isLeader,
  apps: () => sk.apps.get().map((a) => ({ id: a.id, title: a.title, active_ver: a.active_ver, activated_ver: a.activated_ver, state: a.state })),
  async seed() {
    const files = await fetchFiles('demo', ['index.js', 'index.css', 'collections.schema.json', 'manifest.json', 'handlers/recount.js']);
    // Release wie aus einem Paket: manifest.json liegt als Datei bei UND ihre Felder stehen im Versions-Manifest. Im
    // Entwurf ist manifest.json keine Datei (§16.26) – der Kern trägt sie beim Anlegen als gelöscht ein.
    const m = JSON.parse(files['manifest.json']);
    const r = await stub.sim.publishAppVersion('demo', '1', {
      files, entry: m.entry, title: m.title, handlers: m.handlers, minAppApi: m.min_app_api, capabilitiesRequired: m.capabilities_required,
      collections: toManifestCollections(parseCollectionsFile(files['collections.schema.json'])),
    });
    await stub.appApi('demo', '1').put('items', { id: 'i1', title: 'Erster', count: 3 }).result;
    return r?.state;
  },
  async openApp(appId, geometry = viewport()) {
    const appRecord = sk.apps.get().find((a) => a.id === appId);
    const id = `w${++seq}`;
    const win = createLabWindow(desktop, { id, title: appRecord?.title ?? appId, geometry });
    win.mounted = await host.mount(win.content, appId);
    win.toggle = attachSourceToggle({ controls: win.controls, content: win.content, zone: win.zone, appRecord, kernel: stub, api: mirrorFor(appId), editor });
    windows.set(id, win);
    return id;
  },
  openUpload(geometry = viewport()) {
    const win = createLabWindow(desktop, { id: 'upload', title: 'App hochladen', icon: '⇪', geometry });
    win.content.style.display = 'flex';
    win.panel = openAppUpload({ root: win.content, kernel: stub, api: mirrorFor('__upload'), zone: win.zone });
    windows.set('upload', win);
    return 'upload';
  },
  fit(id) { windows.get(id).place(viewport()); },
  win: (id) => windows.get(id),
  winInfo(id) {
    const w = windows.get(id);
    const t = w.toggle;
    return {
      ver: w.mounted?.ver ?? null, label: w.content.querySelector('.demo__label')?.textContent ?? null,
      toggleVisible: !!t && !t.button.hidden && t.button.getBoundingClientRect().width > 0,
      sourceOpen: !!t?.isOpen(), zone: w.zone.entries().map((e) => ({ level: e.level, code: e.code, text: e.text })),
      zoneHidden: w.zone.el.hidden, source: t?.handle ? t.handle.state : null,
      zoneAction: !w.zone.el.hidden && !w.zone.el.querySelector('[data-zone-action]').hidden ? w.zone.el.querySelector('[data-zone-action]').textContent : null,
    };
  },
  draftInfo: (appId) => stub.source(appId).draft.info(),
  // Rohzeile in app_drafts (am Kern vorbei, Sim) – belegt getrennte Entwürfe je (App, Nutzer).
  draftRow: async (id) => { const r = await stub.sim.inspect('app_drafts', id); return r && !r.deleted ? r.data : null; },
  // Direkter Kernaufruf (umgeht die Shell-Sperre) – prüft, dass der Kern selbst die Migration verlangt.
  publishRaw: (appId, base) => stub.source(appId).draft.publish({ base_app_ver: base }).result,
  setManifestRaw: (appId, fields) => stub.source(appId).draft.setManifest(fields).then(() => ({ ok: true }), (e) => ({ ok: false, code: e.code, message: e.message })),
};
document.documentElement.dataset.labReady = '1';
