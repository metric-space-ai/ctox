// Ladeweg-Labor, Monaco (§16.50): Code-Ansicht der Shell über den ECHTEN Service Worker (/sw.js aus v4/), Shell- und
// Bibliotheksdateien nur aus der Kernel-IDB. Ablauf wie in Produktion:
//   1. SW registrieren (db = Kernel-IDB der Attrappe), Seite meldet sich als Kernel-Fenster und Leiter ('ctox:kernel').
//   2. Shell-Version 'lab' als Artefakte installieren (files:shell@lab); die Dateien holt der Labor-Kernel über /__spur4/
//      (Ersatz für Spur 4, der SW sieht diesen Pfad nie).
//   3. lib:monaco@0.0 NICHT vorab: jede Datei fordert der SW per ctox:lib-miss an (auch aus dem Monaco-Worker – der SW
//      leitet Worker-Anforderungen an dieses Fenster weiter); der Labor-Kernel legt Dateiliste und Datei ab.
//   4. Kern-Stub (artifacts.bind, source) baut die Instanz-URL /~a/shell@lab~lib:ctox-office@1.0~lib:monaco@0.0/…; die
//      Seite importiert source-mode.js von dort (SW) und öffnet die Code-Ansicht der App „demo“.
import * as K from './kernel-attrappe.js';

const q = new URLSearchParams(location.search);
const out = window.__mon = { misses: [], aborts: [], hits: 0, errors: [], step: 'start', editor: null };
const LIBS = { 'ctox-office': '1.0', monaco: '0.0' };

let libManifest = null;
async function onLibMiss(msg, port) {
  out.misses.push(`${msg.lib}@${msg.ver}/${msg.path}:${msg.reason}`);
  try {
    if (msg.lib !== 'monaco' || msg.ver !== LIBS.monaco) throw new Error(`nicht im Labor: lib:${msg.lib}@${msg.ver}`);
    libManifest ??= (await (await fetch('/__spur4/v4/manifests/lib-monaco.manifest.json')).json()).files;
    if (msg.reason === 'manifest') await K.putFileList('lib:monaco', msg.ver, libManifest);
    const sha = libManifest[msg.path];
    if (!sha) throw new Error(`nicht im Lib-Manifest: ${msg.path}`);
    const bytes = new Uint8Array(await (await fetch(`/__spur4/vendor/monaco/${msg.path}`)).arrayBuffer());
    const got = await K.putArtifact(bytes);
    if (got !== sha) throw new Error(`sha256 weicht ab: ${msg.path}`);
    port.postMessage({ type: 'ctox:lib-ready' });
  } catch (e) {
    out.aborts.push(String(e.message || e));
    port.postMessage({ type: 'ctox:lib-abort', reason: String(e.message || e) });
  }
}

async function run() {
  await K.openKernelDb();
  navigator.serviceWorker.addEventListener('message', (ev) => {
    const d = ev.data || {};
    if (d.type === 'ctox:lib-miss') onLibMiss(d, ev.ports[0]);
    else if (d.type === 'ctox:lib-hit') out.hits += d.hits.length;
  });
  navigator.serviceWorker.startMessages?.();
  if (!navigator.serviceWorker.controller) {
    const changed = new Promise((res) => navigator.serviceWorker.addEventListener('controllerchange', res, { once: true }));
    const reg = await navigator.serviceWorker.register(`/sw.js?db=${K.DB_NAME}`, { scope: '/' });
    if (reg.active) reg.active.postMessage('ctox:claim');
    await changed;
  }
  navigator.serviceWorker.controller.postMessage({ type: 'ctox:kernel', leader: true });
  out.step = 'sw';

  // 2. Shell-Artefakt 'lab' (Dateiliste vom Messskript: Abschluss der Importe ab v4/source/source-mode.js)
  const paths = JSON.parse(q.get('shellFiles') ? atob(q.get('shellFiles')) : '[]');
  const files = {};
  await Promise.all(paths.map(async (p) => { files[p] = new Uint8Array(await (await fetch(`/__spur4/${p}`)).arrayBuffer()); }));
  // Text bleibt Text (wie der Kernel): Umschreibung im SW braucht nur die Endung, sha256 über die Bytes ist gleich.
  for (const p of paths) if (/\.(m?js|css|json)$/.test(p)) files[p] = new TextDecoder().decode(files[p]);
  await K.installVersion({ app_id: 'shell', app_ver: 'lab', files });
  out.step = 'shell';

  // 4. Kern-Stub: Bibliotheken + Shell veröffentlichen, Instanz-URL binden, App „demo“ mit Quelltext
  const { createStubKernel } = await import('/~stub/ctox-app-api-stub.mjs');
  const perms = ['apps.source.view', 'apps.source.edit'];
  const stub = createStubKernel({ instanceId: 'mon', userId: 'ada', storage: 'memory', latencyMs: 5, actor: { id: 'ada', display_name: 'ada', role: 'admin', permissions: perms } });
  await stub.ready;
  for (const [n, v] of Object.entries(LIBS)) await stub.sim.publishAppVersion(`lib:${n}`, v, { title: `lib:${n}`, styles: [] });
  await stub.sim.publishAppVersion('shell', 'lab', { title: 'Shell', styles: [], manifestExtra: { libs: Object.fromEntries(Object.entries(LIBS).map(([n, v]) => [n, v.split('.')[0]])) } });
  const demo = {};
  for (const p of ['index.js', 'index.css']) demo[p] = await (await fetch(`/__spur4/v4/source/lab/apps/demo/${p}`)).text();
  await stub.sim.publishAppVersion('demo', '1', { files: demo, entry: 'index.js', title: 'Demo' });
  await stub.sim.settle?.();
  const binding = (stub.artifacts ?? stub.shell.artifacts).bind('shell', 'lab');
  out.root = binding.root;
  const { openSourceMode } = await import(binding.url('v4/source/source-mode.js'));
  out.step = 'import';
  const appRecord = stub.shell.apps.get().find((a) => a.id === 'demo');
  const api = stub.appApi('demo', 'shell-mirror');
  const host = document.querySelector('#win > .host');
  window.__view = await openSourceMode({ root: host, appRecord, kernel: stub, api, agentOpen: q.get('agent') !== '0' });
  out.step = 'open';
}

run().then(() => { document.documentElement.dataset.monReady = '1'; }, (e) => { out.errors.push(String(e && e.stack || e)); document.documentElement.dataset.monReady = 'error'; });
