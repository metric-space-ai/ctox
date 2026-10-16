// Labor-Loader: übernimmt die Rolle des Kern-Loaders (Kernel öffnen, Shell-Version importieren, startShell).
// Seit Strang I gibt es genau EINE Shell: den Rahmen /~a/shell@<ver>/v4/frame/index.js, der den App-Host
// (v4/shell/app-host.js) je Fenster einbindet. Stellt window.__lab als Messschnittstelle für measure.mjs bereit.
import { createKernel } from './api-attrappe.js';

const params = new URLSearchParams(location.search);
const tabId = params.get('tab') || `tab-${Math.random().toString(36).slice(2, 8)}`;
const events = [];
const shellUrl = (ver) => `/~a/shell@${ver}/v4/frame/index.js`;
// ?kernel=stub → verbindlicher Kern-Stub (createStubKernel) über shell/kernel-adapter.js, sonst Attrappe. Der Blob-Ladeweg
// für Kernel ohne artifacts.url ist gelöscht (L-34); ein Stub ohne artifacts.url steht in describeShell().gaps.
const useStub = params.get('kernel') === 'stub';
let stubKernel = null;
let kernel;
if (useStub) {
  const stubMod = await import('/stub/ctox-app-api-stub.mjs');
  const { adaptKernel } = await import('/~a/shell@1/v4/shell/kernel-adapter.js');
  stubKernel = stubMod.createStubKernel({ instanceId: 'lab', userId: 'u1', latencyMs: Number(params.get('latency') || 20) });
  await stubKernel.ready;
  // K-1d: artifacts.bind/url nehmen die aktive Shell des Kernels; die Shell-Version dieses Labors dort veröffentlichen.
  const labShell = params.get('shell') || '1';
  if (!stubKernel.apps.get().some((a) => a.id === 'shell')) await stubKernel.sim.publishAppVersion('shell', labShell, { title: 'Shell', styles: [] });
  kernel = adaptKernel(stubKernel);
  kernel.lab = { now: () => performance.timeOrigin + performance.now() };
} else {
  kernel = await createKernel({ tabId });
}
let shellVer = params.get('shell') || '1';
let shellMod = await import(shellUrl(shellVer));
const onEvent = (e) => events.push(e);
const mountPoint = document.body;
await shellMod.startShell(kernel, { root: mountPoint, onEvent });

const handle = () => shellMod.shellHandle();
// Sicht eines Rahmen-Fensters für die Messung (Fenster-ID = ownerId, bleibt über den Shell-Tausch stabil).
function view(rec) {
  if (!rec) return null;
  const el = rec.handle.element;
  return {
    id: rec.ownerId,
    appId: rec.appId,
    el,
    zone: rec.host.zone,
    titleEl: el.querySelector('.shell-window-title-text'),
    headerSlot: el.querySelector('[data-window-actions]'),
    get instance() { return rec.host.instance; },
    get ver() { return rec.host.ver; },
    get state() { return rec.host.state; },
  };
}
const windows = () => (handle()?.windows() || []).map(view);

function zoneInfo(win) {
  const z = win.zone.el;
  const r = z.getBoundingClientRect();
  const line = z.querySelector('.v4-status-zone__line');
  const list = win.zone.list;
  const lr = list.getBoundingClientRect();
  const wr = win.el.getBoundingClientRect();
  return {
    hidden: z.hidden, height: r.height, visible: !!z.offsetParent && r.height > 0,
    text: line.textContent, level: z.dataset.level || null,
    lineHeight: line.getBoundingClientRect().height, lineScrollWidth: line.scrollWidth, lineClientWidth: line.clientWidth,
    rowsInZone: z.querySelectorAll('.v4-status-zone__line').length,
    entries: win.zone.entries().map((e) => ({ level: e.level, code: e.code, text: e.text })),
    list: { hidden: list.hidden, items: list.querySelectorAll('.v4-status-zone__item').length, rect: [lr.left, lr.top, lr.right, lr.bottom], parentIsWindow: list.parentElement === win.el },
    windowRect: [wr.left, wr.top, wr.right, wr.bottom],
    inChrome: !win.el.querySelector('[data-window-content]').contains(z),
  };
}

// Doppelpuffer-Probe: kleinste Zahl sichtbarer Fenster mit App-Inhalt über alle Mutationen und Frames.
function startProbe() {
  const visibleWithContent = () => {
    let best = 0;
    for (const root of mountPoint.querySelectorAll(':scope > [data-v4-frame]')) {
      if (getComputedStyle(root).opacity === '0') continue;
      best = Math.max(best, [...root.querySelectorAll('.shell-window')].filter((w) => w.style.display !== 'none' && w.querySelector('.v4-app-root')?.childElementCount > 0).length);
    }
    return best;
  };
  const p = { moSamples: 0, moMin: Infinity, rafSamples: 0, rafMin: Infinity, run: true };
  const mo = new MutationObserver(() => { p.moSamples++; p.moMin = Math.min(p.moMin, visibleWithContent()); });
  mo.observe(mountPoint, { childList: true, subtree: true, attributes: true, attributeFilter: ['style', 'data-v4-retiring', 'data-v4-building'] });
  const raf = () => { if (!p.run) return; p.rafSamples++; p.rafMin = Math.min(p.rafMin, visibleWithContent()); requestAnimationFrame(raf); };
  requestAnimationFrame(raf);
  return () => { p.run = false; mo.disconnect(); return { moSamples: p.moSamples, moMin: p.moMin, rafSamples: p.rafSamples, rafMin: p.rafMin }; };
}

// Stub 67229d2 (§16.19 Nr. 6): App-Styles kommen aus manifest.styles, nicht mehr aus den *.css-Dateien.
const stubOpts = (files) => ({ files, entry: 'index.js', styles: Object.keys(files).filter((p) => p.endsWith('.css')), layout: { category: 'Labor' } });
window.__lab = {
  sentinel: Math.random(),
  tabId, events, kernel,
  get shell() { return { host: { windows: new Map(windows().map((w) => [w.id, w])) } }; },
  get shellVer() { return shellVer; },
  now: () => performance.timeOrigin + performance.now(),
  isLeader: () => kernel.leader.get().isLeader,
  stubKernel,
  async stubPublish(appId, ver, files) {
    const t0 = performance.timeOrigin + performance.now();
    const p = stubKernel.sim.publishAppVersion(appId, String(ver), stubOpts(files));
    return { t0, done: await p.then((r) => r?.state ?? null, (e) => `error: ${e.message}`) };
  },
  stubPublishNoWait(appId, ver, files) {
    const t0 = performance.timeOrigin + performance.now();
    window.__lastPublish = stubKernel.sim.publishAppVersion(appId, String(ver), stubOpts(files)).catch((e) => e.message);
    return t0;
  },
  async openWindow(appId, opts = {}) { const id = handle().openWindow(appId, opts); await handle().win(id)?.host?.ready; return id; },
  publish: (appId, ver, opts) => kernel.lab.publish(appId, ver, opts),
  win: (id) => view(handle()?.win(id)),
  windows,
  winInfo(id) {
    const w = view(handle()?.win(id));
    if (!w) return null;
    return { state: w.state, ver: w.ver, title: w.titleEl.textContent, geometry: handle().geometry(id), zone: zoneInfo(w), headerItems: w.headerSlot.children.length };
  },
  describe: () => shellMod.describeShell(),
  report: (id, item) => view(handle().win(id)).instance.api.status.report(item),
  clear: (id, code) => view(handle().win(id)).instance.api.status.clear(code),
  put: (doc) => kernel.appApi('formular', 'lab').put('eintraege', doc),
  command: (type, payload) => kernel.appApi('formular', 'lab').command(type, payload),
  async swapShell(nextVer) {
    const stopProbe = startProbe();
    const t0 = performance.now();
    const snap = await shellMod.stopShell();
    const t1 = performance.now();
    const next = await import(shellUrl(nextVer));
    const t2 = performance.now();
    await next.startShell(kernel, { restored: snap, root: mountPoint, onEvent });
    const t3 = performance.now();
    shellMod = next;
    shellVer = String(nextVer);
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    return { stopMs: t1 - t0, importMs: t2 - t1, startMs: t3 - t2, totalMs: t3 - t0, windows: snap.windows.length, snapshotBytes: JSON.stringify(snap).length, probe: stopProbe() };
  },
};
document.documentElement.dataset.labReady = '1';
