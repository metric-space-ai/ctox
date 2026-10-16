// E38 – echter Startweg der Shell v4 im echten Chromium, ohne Daemon und ohne Ersatz-Kern:
//   GET / (v4/index.html) → /business-os/v4/boot.js → /kernel/boot.mjs bootV4({launchContextUrl}) → echter
//   WebRTC-Transport + Kernel → Aktivierung der Shell-Version (Manifest v4/manifests/shell.manifest.json, Dateien
//   per transfer_get aus dem CAS des nativen Partners) → Loader lädt v4/frame/index.js über v4/sw.js → [data-v4-frame].
//
// Laborserver bildet die Daemon-Routen (E38) nach, alles andere 404, jeder HTTP-Pfad wird protokolliert:
//   /                → v4/index.html          /sw.js → v4/sw.js (der ECHTE Shell-SW)
//   /kernel/*        → src/apps/business-os/kernel/*
//   /business-os/*   → src/apps/business-os/*
//   /api/business-os/launch-context (Sitzungs-Cookie, wie kernel/tests/boot-browser.mjs)
//   /~fixture/*      → NUR Prüfstand: Code des nativen Partners (FakeNativePeer + hello.mjs + values.mjs) und dessen
//                      CAS-Speicher (/~fixture/cas/<sha256>). Diese Pfade kennt weder Kernel noch Shell; der Kernel
//                      selbst bekommt alle Bytes ausschliesslich über WebRTC (transfer_get).
// Die Fixture-Globals (WebSocket-Signalisierungs-Relais, nativer RTCPeerConnection-Partner) setzt page.addInitScript
// VOR jedem Seitenskript; boot.js läuft unverändert und automatisch (kein __CTOX_V4_BOOT_MANUAL__).
//
// Aufruf (aus dem Repo-Wurzelverzeichnis):
//   PLAYWRIGHT_MODULE=$HOME/Documents/ctox/src/apps/business-os/node_modules/playwright \
//     node src/apps/business-os/v4/tools/boot/tests/boot-real.browser.mjs [counter] [--probe=routes,caps,swdb,files]
//   counter  : Gegenprobe – das veröffentlichte Shell-Manifest trägt für v4/frame/index.js eine falsche sha256
//              (der native CAS liefert unter diesem Schlüssel die echten Bytes) → Rahmen darf NICHT erscheinen, der Kern
//              muss die Aktivierung als failed (hash_mismatch:v4/frame/index.js) melden. Lauf ist rot → Exit 1.
//   --probe  : NUR Erkundung hinter einem offenen Bruch (siehe EXPECTED_OPEN), nie Abnahme. Jede Stufe weicht vom
//              echten Weg ab und wird im Protokoll als PROBE ausgewiesen: routes = Wurzelrouten /rxdb/*, /shared/* (OPEN-1),
//              caps = capabilities_required leer (OPEN-2), swdb = SW kennt den Kernel-DB-Namen ohne ?db= (OPEN-3),
//              files = zwei fehlende CSS-Dateien ins Manifest (OPEN-4). Mit allen vier steht der Rahmen; (g) bleibt rot.
//              Die Gegenprobe ist erst mit allen vier aussagekräftig (ohne sie scheitert der Weg schon an OPEN-1).
// Exit: 0 = ganzer Weg grün; 2 = rot genau an einem benannten offenen Bruch (EXPECTED_OPEN); 1 = sonst rot
//       (Gegenprobe: 1 = wie verlangt rot).
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { generateKeyPairSync, createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const BOS = path.resolve(HERE, '../../../..');
const KERNEL = path.join(BOS, 'kernel');
const COUNTER = process.argv.includes('counter');
const PROBE = new Set((process.argv.find((a) => a.startsWith('--probe='))?.slice(8) ?? '').split(',').filter(Boolean));
const EVIDENCE = process.env.CTOX_EVIDENCE_DIR || path.join(os.tmpdir(), 'shell-v4-boot-real');
fs.mkdirSync(EVIDENCE, { recursive: true });
const require = createRequire(path.join(os.homedir(), 'Documents/ctox/src/apps/business-os/package.json'));
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');

// Offene Brüche des echten Wegs (Befund, nicht hier zu beheben – kernel/** und v4/** sind gesperrt).
// signature: wird gegen die gesammelten Fehlertexte/Zustände geprüft; trifft genau eine, endet der Lauf mit Exit 2.
const EXPECTED_OPEN = [
  {
    id: 'OPEN-1 kernel-imports-outside-kernel-route',
    where: 'kernel/boot.mjs:8, kernel/session.mjs:2-3, kernel/transport-webrtc.mjs:3',
    what: "Der Kern importiert relativ '../rxdb/src/hello.mjs' und '../shared/async-timeout.js'. Unter der Daemon-Route "
      + "/kernel/* löst das zu /rxdb/src/hello.mjs und /shared/async-timeout.js auf – Pfade außerhalb von /kernel/* und "
      + "/business-os/* → 404, das Modul /kernel/boot.mjs lädt nicht, bootV4 läuft nie.",
    probe: 'routes',
    match: (s) => s.http.some((r) => r.status === 404 && (r.path.startsWith('/rxdb/') || r.path.startsWith('/shared/'))),
  },
  {
    id: 'OPEN-2 capabilities-not-carried-by-webrtc-transport',
    where: 'kernel/kernel.mjs:1067 (transport.capabilities), kernel/boot.mjs:164-167 (keine capabilities an createLoader)',
    what: "Das Shell-Manifest verlangt capabilities_required ['agent.coding.cancel','agent.coding.turn']. runActivation prüft "
      + 'opts.capabilities ∪ transport.capabilities; WebRTCKernelTransport hat kein capabilities-Feld und bootV4 reicht '
      + 'keine (auch nicht die aus hello_ok) weiter → Aktivierung failed needs_update:… → Rahmen lädt nie.',
    probe: 'caps',
    match: (s) => s.activation.some((a) => a.state === 'failed' && String(a.error ?? '').startsWith('needs_update')),
  },
  {
    id: 'OPEN-3 sw-registered-without-db',
    where: 'kernel/loader.mjs:10,17-18 (register /sw.js ohne ?db=) ↔ v4/sw.js:76 (dbName aus ?db=), v4/sw.js:181 notFound',
    what: 'Der Loader registriert genau /sw.js; v4/sw.js liest die Kernel-IDB nur aus dem Parameter ?db= der Registrierung. '
      + 'Ohne ihn beantwortet der SW jede /~a/-Anfrage mit 404 → import(/~a/shell@…/v4/frame/index.js) scheitert.',
    probe: 'swdb',
    match: (s) => s.browserRequests.some((r) => r.url.includes('/~a/shell@') && r.url.endsWith('/v4/frame/index.js') && r.status !== 200)
      && s.activation.some((a) => a.state === 'failed' && String(a.error ?? '').includes('load_failed')),
  },
  {
    id: 'OPEN-4 shell-manifest-misses-frame-css',
    where: 'v4/manifests/shell.manifest.json (files) ↔ v4/frame/index.js:200,204 (cssLinkFor ../../shared/shell-v2-chrome.css, ../../shared/shell-v2-topbar.css)',
    what: 'Der Rahmen lädt shared/shell-v2-chrome.css und shared/shell-v2-topbar.css relativ unter seiner /~a/shell@<ver>/-Wurzel; '
      + 'beide Dateien fehlen in manifest.files → SW 404 (kein Netz-Rückgriff) → Fensterchrome und Kopfleiste ungestylt.',
    probe: 'files',
    match: (s) => s.browserRequests.some((r) => /\/~a\/shell@[^/]+\/shared\/shell-v2-(chrome|topbar)\.css$/.test(r.url) && r.status !== 200),
  },
];

// ---------------------------------------------------------------- Shell-Version aus dem Manifest
const manifestFile = JSON.parse(fs.readFileSync(path.join(BOS, 'v4/manifests/shell.manifest.json'), 'utf8'));
const staleFiles = [];
const cas = new Map(); // sha256 → absoluter Pfad (Speicher des nativen Partners)
for (const [rel, sha] of Object.entries(manifestFile.files)) {
  const abs = path.join(BOS, rel);
  const actual = fs.existsSync(abs) ? createHash('sha256').update(fs.readFileSync(abs)).digest('hex') : null;
  if (actual !== sha) staleFiles.push({ rel, manifest: sha, actual });
  cas.set(sha, abs);
}
const SHELL_VER = `v4lab-${createHash('sha256').update(JSON.stringify(manifestFile.files)).digest('hex').slice(0, 10)}`;
const manifest = { ...manifestFile, app_id: 'shell', app_ver: SHELL_VER, author: 'boot-real-lab', created_at: '2026-10-16T00:00:00Z', files: { ...manifestFile.files } };
const WRONG_SHA = 'f'.repeat(64);
if (COUNTER) { manifest.files['v4/frame/index.js'] = WRONG_SHA; cas.set(WRONG_SHA, path.join(BOS, 'v4/frame/index.js')); }
if (PROBE.has('caps')) manifest.capabilities_required = []; // PROBE: weicht vom Manifest ab (OPEN-2)
if (PROBE.has('files')) for (const rel of ['shared/shell-v2-chrome.css', 'shared/shell-v2-topbar.css']) { // PROBE: OPEN-4
  const abs = path.join(BOS, rel), sha = createHash('sha256').update(fs.readFileSync(abs)).digest('hex');
  manifest.files[rel] = sha; cas.set(sha, abs);
}

// Zweite App 'notes' (Prüfstand, eine Datei), damit der Starter etwas zeigt; ihre Bytes liegen ebenfalls im nativen CAS.
const notesFile = path.join(HERE, '../fixtures/notes/index.js');
const notesSha = createHash('sha256').update(fs.readFileSync(notesFile)).digest('hex');
cas.set(notesSha, notesFile);
const EXTRA_APPS = [{ id: 'notes', title: 'Notizen', ver: '1.0.0', manifest: { app_id: 'notes', app_ver: '1.0.0', title: 'Notizen', entry: 'index.js',
  files: { 'index.js': notesSha }, min_app_api: '4.0', collections: {}, capabilities_required: [], layout: { category: 'Arbeit' }, styles: [], author: 'boot-real-lab', created_at: '2026-10-16T00:00:00Z' } }];
// E30/E35/E48: der Kern liest Fremd-Collections nur mit aktivem Eigentümer-Manifest (collections + exports). Die Eigentümer
// der Shell-Fremd-Collections werden mit ihren echten Manifesten veröffentlicht – ohne Dateien und Einstieg, nichts wird
// geladen; ctox-platform aus dem Kern-Manifest (src/core/business_os/v4/ctox-platform.manifest.json, #904).
const OWNER_IDS = [...new Set(Object.values(manifestFile.foreign_collections ?? {}).map((f) => f?.owner_app).filter((id) => id && id !== 'shell'))].sort();
for (const id of OWNER_IDS) {
  const file = id === 'ctox-platform' ? path.join(BOS, '../../core/business_os/v4/ctox-platform.manifest.json') : path.join(BOS, `v4/manifests/${id}.manifest.json`);
  const m = JSON.parse(fs.readFileSync(file, 'utf8'));
  const ver = String(m.app_ver ?? '1');
  // E55: vollständige Eigentümer-Manifeste wie auf einer echten Instanz; ihre Collections deckt je App ein Grant, den der
  // Kern bei der Aktivierung über die Control-Lane anfordert (scope.grant.request), nicht das hello.
  EXTRA_APPS.push({ id, title: m.title ?? id, ver, manifest: { app_id: id, app_ver: ver, title: m.title ?? id, entry: null, files: {}, min_app_api: '4.0',
    collections: m.collections ?? {}, exports: m.exports ?? {}, capabilities_required: [], styles: [], author: 'boot-real-lab', created_at: '2026-10-16T00:00:00Z' } });
}

// ---------------------------------------------------------------- Startkontext (wie kernel/tests/boot-browser.mjs)
const INSTANCE = 'boot-real', USER = 'alice';
const { privateKey, publicKey } = generateKeyPairSync('ed25519');
const signingJwk = privateKey.export({ format: 'jwk' }), publicRaw = publicKey.export({ format: 'jwk' }).x;
// Der Kern abonniert immer business_apps, business_app_versions, business_commands, files, shell_preferences, app_drafts
// (kernel.mjs:450, 2116); dazu die foreign_collections des Shell-Manifests.
const COLLECTIONS = [...new Set(['business_apps', 'business_app_versions', 'business_commands', 'files', 'shell_preferences', 'app_drafts', ...Object.keys(manifestFile.foreign_collections ?? {})])];
let contextFetches = 0;
function launchContext() {
  const nonce = Buffer.from(String(++contextFetches).padStart(32, '0')).toString('base64url').slice(0, 43);
  return { session: {}, config: null, designTemplates: [], v4: { instanceId: INSTANCE, userId: USER, actor: { id: USER, role: 'owner' },
    signaling: { urls: ['wss://signal.invalid/ws'], room: `ctox-v4:${INSTANCE}`, peerId: 'browser-1', nativePeerId: 'native-1', token: 'signal-token', iceServers: [] },
    hello: { offer: { type: 'hello', protocol_ver: 4, bootstrap_ver: 1, app_api_ver: { min: '4.0', max: '4.0' }, instance_id: INSTANCE, capabilities: DAEMON_CAPABILITIES,
      device_proof: { device_id: 'browser-device', public_key: 'A'.repeat(43), nonce, signature: 'C'.repeat(86) }, scopes_wanted: COLLECTIONS,
      transport: { codecs: ['gzip'], max_frame_bytes: 16384, fragmentation: true, reassembly: true } } },
    packageKeys: [{ keyId: 'instance-key-1', alg: 'Ed25519', publicKey: publicRaw }] } };
}

// ---------------------------------------------------------------- Laborserver (Daemon-Routen E38)
const MIME = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png', '.woff2': 'font/woff2' };
// Wie ein echter Daemon (Kern E43): Angebot und Gewährung enthalten die capabilities_required der aktiven Manifeste.
const DAEMON_CAPABILITIES = ['agent.coding.cancel', 'agent.coding.turn'];
const FIXTURE_FILES = new Set(['kernel/tests/fake-native-peer.mjs', 'rxdb/src/hello.mjs', 'kernel/vendor/hello.mjs', 'kernel/values.mjs']);
const http = []; // {path, status, kind}
function route(p) {
  if (p === '/') return { file: path.join(BOS, 'v4/index.html'), kind: 'app' };
  if (p === '/sw.js') return { file: path.join(BOS, 'v4/sw.js'), kind: 'app' };
  if (p.startsWith('/kernel/')) return { file: path.join(KERNEL, p.slice(8)), root: KERNEL, kind: 'app' };
  if (p.startsWith('/business-os/')) return { file: path.join(BOS, p.slice(13)), root: BOS, kind: 'app' };
  if (p.startsWith('/~fixture/cas/')) { const f = cas.get(p.slice(14)); return f ? { file: f, kind: 'fixture' } : { kind: 'fixture' }; }
  if (p.startsWith('/~fixture/') && FIXTURE_FILES.has(p.slice(10))) return { file: path.join(BOS, p.slice(10)), kind: 'fixture' };
  // PROBE routes (OPEN-1): root-level /rxdb/* and /shared/* like the V2 daemon tree – NOT part of the E38 route table.
  if (PROBE.has('routes') && (p.startsWith('/rxdb/') || p.startsWith('/shared/'))) return { file: path.join(BOS, p.slice(1)), root: BOS, kind: 'probe' };
  return { kind: p.startsWith('/~fixture/') ? 'fixture' : 'app' };
}
const server = createServer((req, res) => {
  const p = decodeURIComponent(new URL(req.url, 'http://x').pathname);
  const entry = { path: p, status: 404, kind: 'app' };
  http.push(entry);
  if (p === '/api/business-os/launch-context') {
    if (req.headers.cookie !== 'ctox_session=fixture') { entry.status = 401; res.writeHead(401).end('{}'); return; }
    entry.status = 200; res.writeHead(200, { 'content-type': 'application/json', 'cache-control': 'no-store' }).end(JSON.stringify(launchContext())); return;
  }
  const r = route(p);
  entry.kind = r.kind;
  if (!r.file || (r.root && !path.resolve(r.file).startsWith(r.root + path.sep)) || !fs.existsSync(r.file) || !fs.statSync(r.file).isFile()) { res.writeHead(404).end('not found'); return; }
  let body = fs.readFileSync(r.file);
  // PROBE swdb (OPEN-3): der SW bekommt den Kernel-DB-Namen, obwohl die Registrierung kein ?db= traegt.
  if (p === '/sw.js' && PROBE.has('swdb')) { entry.kind = 'probe'; body = Buffer.from(String(body).replace("searchParams.get('db') || ''", `searchParams.get('db') || 'ctox-${INSTANCE}-${USER}'`)); }
  const headers = { 'content-type': MIME[path.extname(r.file)] || 'application/octet-stream', 'cache-control': 'no-store' };
  if (p === '/') headers['set-cookie'] = 'ctox_session=fixture; Path=/; SameSite=Strict';
  entry.status = 200;
  res.writeHead(200, headers).end(body);
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${server.address().port}`;

// ---------------------------------------------------------------- Fixture im Browser (vor jedem Seitenskript)
function installFixture({ signingJwk, collections, instanceId, userId, shell, extraApps }) {
  if (window !== window.top || location.pathname !== '/') return;
  const lab = globalThis.__bootLab = { signalLog: [], labels: [], nativeErrors: [], native: null, subscribed: [], transfers: 0, setupError: null, grantRequests: [], helloOkBytes: 0 };
  const STREAM_DOMAIN = 'ctox.sync.v4.stream\0', SCOPE_DOMAIN = 'ctox:v4:scope\0';
  const rec = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
  const canonicalJson = (v) => Array.isArray(v) ? '[' + v.map(canonicalJson).join(',') + ']' : rec(v) ? '{' + Object.keys(v).sort().map((k) => JSON.stringify(k) + ':' + canonicalJson(v[k])).join(',') + '}' : JSON.stringify(v);
  const enc = (s) => new TextEncoder().encode(s);
  const b64url = (bytes) => btoa(String.fromCharCode(...bytes)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  const NativeRTC = globalThis.RTCPeerConnection;
  const setup = (async () => {
    const { FakeNativePeer } = await import('/~fixture/kernel/tests/fake-native-peer.mjs');
    const { decodeHelloFrame, negotiateHello } = await import('/~fixture/rxdb/src/hello.mjs');
    const key = await crypto.subtle.importKey('jwk', signingJwk, { name: 'Ed25519' }, false, ['sign']);
    const signBytes = async (s) => new Uint8Array(await crypto.subtle.sign({ name: 'Ed25519' }, key, enc(s)));
    // E55: ein Grant-Token je App (claims wie kernel/index.d.ts AppScopeGrantClaims, Signaturdomäne E22). Die Shell bekommt
    // ihres im hello_ok, jede weitere App auf Anfrage über die Control-Lane.
    const shellCollections = new Set(collections);
    const grantCollections = { shell: [...shellCollections] };
    for (const a of extraApps) grantCollections[a.id] = Object.keys(a.manifest.collections ?? {});
    const tokens = {};
    for (const [appId, cols] of Object.entries(grantCollections)) {
      const claims = { instance_id: instanceId, user_id: userId, app_id: appId, scope_hash: `scope:${appId}`, rights_ver: '1', device_id: 'browser-device',
        exp: Math.floor(Date.now() / 1000) + 3600, collections: cols.map((c) => ({ collection: c, gen: 1, rights: ['read', 'write'], field_mask_digest: 'fixture:all' })) };
      tokens[appId] = b64url(enc(canonicalJson({ claims, signature: b64url(await signBytes(SCOPE_DOMAIN + canonicalJson(claims))) })));
    }
    const ownerOf = (c) => (shellCollections.has(c) ? 'shell' : (extraApps.find((a) => a.manifest.collections?.[c])?.id ?? 'shell'));
    const rows = {
      business_apps: [{ id: 'shell', title: 'Shell', active_ver: shell.ver }, ...extraApps.map((a) => ({ id: a.id, title: a.title, active_ver: a.ver }))],
      business_app_versions: [{ id: `shell@${shell.ver}`, app_id: 'shell', app_ver: shell.ver, manifest: shell.manifest }, ...extraApps.map((a) => ({ id: `${a.id}@${a.ver}`, app_id: a.id, app_ver: a.ver, manifest: a.manifest }))],
    };
    class SignedNative extends FakeNativePeer {
      async receive(data) {
        if (typeof data === 'string' && JSON.parse(data).type !== 'ack') {
          // E14 (zuverlässige Sitzung): der Kern hängt session_id + resume_from ans hello; wie FakeNativePeer.receive
          // abtrennen und prüfen, das hello selbst streng dekodieren und beide im hello_ok zurückgeben.
          const { session_id, resume_from, ...p4 } = JSON.parse(data); this.native = session_id !== undefined;
          if (this.native) {
            if (typeof session_id !== 'string' || !Array.isArray(resume_from) || resume_from.length !== 6) throw Error('Invalid E14 hello');
            this.sessionId = session_id;
          }
          const hello = decodeHelloFrame(JSON.stringify(p4)); this.messages.push(hello);
          const selected = negotiateHello({ instance_id: this.instanceId, bootstrap_min: 1, bootstrap_max: 1, app_api_ver: { min: '4.0', max: '4.0' }, capabilities: ['agent.coding.cancel', 'agent.coding.turn'] /* = DAEMON_CAPABILITIES (Seitenkontext) */, required_capabilities: [], transport: hello.transport }, hello);
          const okFrame = JSON.stringify({ type: 'hello_ok', protocol_ver: 4, instance_id: this.instanceId, ...selected, epoch: this.epoch, head_seq: this.head, scope_tokens: [tokens.shell], rights_ver: '1', core_time_ms: Date.now() , ...(this.native ? { session_id: this.sessionId, resume_from: this.received.map(String) } : {})});
          lab.helloOkBytes = new TextEncoder().encode(okFrame).byteLength;
          this.channels[0].send(okFrame);
          return;
        }
        return super.receive(data);
      }
      async apply(m, laneNo) {
        if (m.type === 'scope.grant.request') {
          lab.grantRequests.push({ app_id: m.app_id, lane: laneNo });
          if (laneNo !== 0) throw Error('E55: scope.grant.request must use the control lane');
          return tokens[m.app_id] ? this.send('control', { type: 'scope.grant', token: tokens[m.app_id] })
            : this.send('control', { type: 'forbidden', app_id: m.app_id, reason: 'no grant' });
        }
        if (m.type === 'transfer_get' && !this.blobs.has(m.ref)) {
          lab.transfers++;
          const r = await fetch(`/~fixture/cas/${m.ref}`); // nativer CAS-Speicher (Prüfstand), nicht der Kernel
          if (r.ok) this.blobs.set(m.ref, new Blob([await r.arrayBuffer()]));
        }
        return super.apply(m, laneNo);
      }
      async page(collection, docs = []) {
        const m = { type: 'page', instance_id: this.instanceId, key_id: 'instance-key-1', participant: userId, collection, scope_hash: `scope:${ownerOf(collection)}`, gen: 1, epoch: '1', package_id: `pkg-${Math.random()}`, seq_from: '0', seq_to: this.head, cursor_to: this.head, head_seq: this.head, complete: true, backlog: 0, core_sent_at_ms: Date.now(), docs };
        return this.send('initial', { ...m, sig: [...await signBytes(STREAM_DOMAIN + canonicalJson(m))] });
      }
    }
    return { SignedNative, rows };
  })();
  setup.catch((e) => { lab.setupError = String(e?.stack ?? e); });
  const lanes = { 'ctox-v4-control': 0, 'ctox-v4-commands': 1, 'ctox-v4-interactive': 2, 'ctox-v4-live': 3, 'ctox-v4-initial-sync': 4, 'ctox-v4-initial': 4, 'ctox-v4-transfer': 5 };
  let nativePc = null, toBrowser = null;
  function startNative() {
    nativePc = new NativeRTC({ iceServers: [] });
    const incoming = {}, buffered = [];
    nativePc.ondatachannel = ({ channel }) => {
      lab.labels.push(channel.label); incoming[lanes[channel.label]] = channel; channel.binaryType = 'arraybuffer';
      const hold = (e) => buffered.push([channel, e.data, hold]); channel.addEventListener('message', hold);
      if (Object.keys(incoming).length === 6) setup.then(({ SignedNative, rows }) => {
        lab.native = new SignedNative(incoming, { instanceId, onSubscribe: async (sub, p) => { lab.subscribed.push(sub.collection); await p.page(sub.collection, (rows[sub.collection] ?? []).map((d) => ({ id: d.id, data: d, seq: p.head, rev: 'r1' }))); } });
        lab.native.errors = lab.nativeErrors;
        for (const [c, data, fn] of buffered.splice(0)) { c.removeEventListener('message', fn); c.dispatchEvent(new MessageEvent('message', { data })); }
      });
    };
    nativePc.onicecandidate = (e) => { if (e.candidate) toBrowser?.({ type: 'signal', senderPeerId: 'native-1', data: { type: 'candidate', candidate: e.candidate.toJSON() } }); };
  }
  // Signalisierungs-Relais mit der WebSocket-Oberfläche, die der Signalisierungs-Client in kernel/boot.mjs nutzt.
  globalThis.WebSocket = class {
    constructor(url) { this.url = url; this.readyState = 0; lab.socketUrl = url; toBrowser = (m) => queueMicrotask(() => this.onmessage?.({ data: JSON.stringify(m) })); setTimeout(() => { this.readyState = 1; this.onopen?.(); }, 0); }
    async send(raw) {
      const m = JSON.parse(raw); lab.signalLog.push(m.type + (m.data?.type ? ':' + m.data.type : ''));
      if (m.type === 'join') { toBrowser({ type: 'joined', peerId: 'browser-1' }); return; }
      if (m.type !== 'signal' || m.receiverPeerId !== 'native-1') return;
      if (m.data.type === 'offer') { startNative(); await nativePc.setRemoteDescription(m.data); const answer = await nativePc.createAnswer(); await nativePc.setLocalDescription(answer); toBrowser({ type: 'signal', senderPeerId: 'native-1', data: { type: 'answer', sdp: answer.sdp } }); }
      else if (m.data.type === 'candidate') await nativePc.addIceCandidate(m.data.candidate);
    }
    close() { this.readyState = 3; }
  };
}

// ---------------------------------------------------------------- Lauf
const results = [];
const tag = `boot-real${COUNTER ? '-counter' : ''}${PROBE.size ? `-probe(${[...PROBE]})` : ''}`;
const check = (name, ok, detail = '') => { results.push({ name, ok }); console.log(`${ok ? 'PASS' : 'FAIL'} [${tag}] ${name}${detail ? ` – ${detail}` : ''}`); };
if (staleFiles.length) console.log(`MANIFEST STALE: ${JSON.stringify(staleFiles)}`);
const browserRequests = [], pageErrors = [], consoleErrors = [];
let browser, state = null, shot = null, launcherNotes = null;
try {
  browser = await chromium.launch({ headless: true, ...(process.env.CHROME_BIN ? { executablePath: process.env.CHROME_BIN } : {}) });
  const context = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  context.on('requestfinished', async (rq) => { const rs = await rq.response().catch(() => null); browserRequests.push({ url: rq.url(), status: rs?.status() ?? null, sw: rs?.fromServiceWorker?.() ?? false, worker: !!rq.serviceWorker?.() }); });
  context.on('requestfailed', (rq) => browserRequests.push({ url: rq.url(), status: null, failure: rq.failure()?.errorText, worker: !!rq.serviceWorker?.() }));
  await context.addInitScript(installFixture, { signingJwk, collections: COLLECTIONS, instanceId: INSTANCE, userId: USER, shell: { ver: SHELL_VER, manifest }, extraApps: EXTRA_APPS });
  const page = await context.newPage();
  page.on('pageerror', (e) => pageErrors.push(String(e?.stack ?? e)));
  page.on('console', (m) => { if (m.type() === 'error') consoleErrors.push(m.text()); });
  await page.goto(`${base}/`);
  // Warten bis Rahmen steht oder ein terminaler Fehler feststeht (Bootfehler, Aktivierung failed), höchstens 60 s.
  const t0 = Date.now();
  const snapshot = () => page.evaluate(async ({ dbName }) => {
    const lab = globalThis.__bootLab ?? {};
    const activation = (lab.native?.messages ?? []).filter((m) => m.type === 'app_activation').map((m) => ({ app_id: m.app_id, app_ver: m.app_ver, state: m.state, error: m.error ?? null }));
    let idb = null;
    try {
      idb = await new Promise((resolve) => {
        const req = indexedDB.open(dbName);
        req.onupgradeneeded = () => { req.transaction.abort(); resolve(null); };
        req.onerror = () => resolve(null);
        req.onsuccess = () => { const db = req.result; try { const g = db.transaction('meta', 'readonly').objectStore('meta').get('activation:shell'); g.onsuccess = () => { resolve(g.result ?? null); db.close(); }; g.onerror = () => { resolve(null); db.close(); }; } catch { resolve(null); db.close(); } };
      });
    } catch { idb = null; }
    const frame = document.querySelector('[data-v4-frame]');
    const rect = frame?.getBoundingClientRect();
    return {
      frame: !!frame, frameVisible: !!rect && rect.width > 0 && rect.height > 0 && getComputedStyle(frame).visibility !== 'hidden',
      activation, idbActivation: idb ? { state: idb.state, activated_ver: idb.activated_ver, target_ver: idb.target_ver, error: idb.error } : null,
      sw: navigator.serviceWorker?.controller?.scriptURL ?? null,
      kernelLoaded: performance.getEntriesByType('resource').some((e) => new URL(e.name).pathname === '/kernel/boot.mjs'),
      hello: (lab.native?.messages ?? []).some((m) => m.type === 'hello'), helloOk: !!lab.native,
      labels: lab.labels?.length ?? 0, signal: lab.signalLog ?? [], subscribed: lab.subscribed ?? [], transfers: lab.transfers ?? 0,
      nativeErrors: (lab.nativeErrors ?? []).map(String), setupError: lab.setupError ?? null,
      grantRequests: lab.grantRequests ?? [], helloOkBytes: lab.helloOkBytes ?? 0,
      body: [...document.body.children].map((e) => e.tagName.toLowerCase() + (e.id ? '#' + e.id : '') + (e.dataset.v4Frame !== undefined ? '[data-v4-frame]' : '')),
    };
  }, { dbName: `ctox-${INSTANCE}-${USER}` });
  for (;;) {
    state = await snapshot();
    const bootFailed = consoleErrors.some((t) => t.includes('[ctox v4 boot]'));
    const actFailed = state.activation.some((a) => a.app_id === 'shell' && a.state === 'failed') || state.idbActivation?.state === 'failed';
    const swFrame404 = browserRequests.some((r) => r.url.includes('/~a/shell@') && r.url.endsWith('/v4/frame/index.js') && r.status !== 200);
    if (state.frameVisible || bootFailed || actFailed || (swFrame404 && Date.now() - t0 > 5000) || Date.now() - t0 > 60000) break;
    await new Promise((r) => setTimeout(r, 250));
  }
  await new Promise((r) => setTimeout(r, 1500)); // Nachlauf: späte Fehler und Anfragen einsammeln
  state = await snapshot();
  shot = path.join(EVIDENCE, `${tag}-1280.png`);
  await page.screenshot({ path: shot });
  if (state.frameVisible) { // Starter öffnen (Rasterknopf links oben), zweites Bild; nur Auskunft, keine Abnahme
    await page.mouse.click(32, 24);
    await new Promise((r) => setTimeout(r, 800));
    launcherNotes = await page.evaluate(() => [...document.querySelectorAll('[data-v4-frame] *')].some((e) => e.childElementCount === 0 && /Notizen/.test(e.textContent ?? '') && e.getBoundingClientRect().width > 0));
    await page.screenshot({ path: shot.replace('-1280.png', '-launcher-1280.png') });
  }
  fs.writeFileSync(path.join(EVIDENCE, `${tag}-network.json`), JSON.stringify({ http, browserRequests }, null, 1));
} finally {
  await browser?.close();
  server.close();
}

// ---------------------------------------------------------------- Abnahme
const appHttp = http.filter((r) => r.kind !== 'fixture');
const allowed = (p) => p === '/' || p === '/sw.js' || p.startsWith('/kernel/') || p.startsWith('/business-os/') || p === '/api/business-os/launch-context';
const frameReq = browserRequests.filter((r) => /\/v4\/frame\/index\.js$/.test(r.url) || r.url.startsWith('blob:'));
const frameViaSw = browserRequests.find((r) => r.url.includes(`/~a/shell@${SHELL_VER}`) && r.url.endsWith('/v4/frame/index.js'));
check('(a) GET / → /business-os/v4/boot.js → /kernel/boot.mjs geladen',
  http[0]?.path === '/' && http.some((r) => r.path === '/business-os/v4/boot.js' && r.status === 200) && http.some((r) => r.path === '/kernel/boot.mjs' && r.status === 200) && state?.kernelLoaded,
  JSON.stringify(appHttp.slice(0, 4).map((r) => `${r.status} ${r.path}`)));
check('(b) bootV4: Startkontext, Signalisierung, 6 Spuren, hello → hello_ok',
  http.some((r) => r.path === '/api/business-os/launch-context' && r.status === 200) && state?.hello && state?.labels === 6,
  JSON.stringify({ signal: state?.signal, labels: state?.labels, hello: state?.hello }));
const shellAct = state?.activation.filter((a) => a.app_id === 'shell') ?? [];
check(`(c) Shell aktiviert: activation('shell') = activated @${SHELL_VER}`,
  state?.idbActivation?.state === 'activated' && state.idbActivation.activated_ver === SHELL_VER && shellAct.some((a) => a.state === 'activated' && a.app_ver === SHELL_VER),
  JSON.stringify({ idb: state?.idbActivation, reports: shellAct.map((a) => `${a.state}${a.error ? ':' + a.error : ''}`), transfers: state?.transfers }));
check('(d) Loader importiert v4/frame/index.js (mit SW-Controller über /~a/shell@<ver>…)',
  state?.sw ? !!frameViaSw && frameViaSw.status === 200 : frameReq.some((r) => r.url.startsWith('blob:')),
  JSON.stringify({ controller: state?.sw, frameRequests: frameReq.slice(0, 3) }));
check('(e) [data-v4-frame] sichtbar', !!state?.frameVisible, JSON.stringify(state?.body));
check('(f) keine Seitenfehler', pageErrors.length === 0 && consoleErrors.length === 0 && !state?.nativeErrors.length && !state?.setupError,
  JSON.stringify({ pageErrors: pageErrors.slice(0, 3), consoleErrors: consoleErrors.slice(0, 5), nativeErrors: state?.nativeErrors?.slice(0, 3), setupError: state?.setupError }));
const offRoute = appHttp.filter((r) => !allowed(r.path));
check('(g) HTTP nur /, /sw.js, /kernel/*, /business-os/*, Startkontext; kein anderes /api/',
  offRoute.length === 0 && appHttp.filter((r) => r.path.startsWith('/api/')).every((r) => r.path === '/api/business-os/launch-context'),
  JSON.stringify(offRoute.map((r) => `${r.status} ${r.path}${r.kind === 'probe' ? ' (PROBE)' : ''}`)));

// E55: hello_ok trägt nur das Shell-Grant (≤ 16384 B); jede aktivierte Eigentümer-App holt genau einmal ihr Grant über die
// Control-Lane (scope.grant.request, Lane 0).
{
  const wanted = EXTRA_APPS.map((a) => a.id).sort();
  const asked = (state?.grantRequests ?? []).map((g) => g.app_id).sort();
  // Kernel-WIP 4d5921435 fragt zusätzlich das Shell-Grant (steht schon im hello_ok) und ctox-platform doppelt an – als
  // Befund an den Kern, hier nur protokolliert (duplicates); geprüft wird Vollständigkeit, Lane und Grösse.
  const duplicates = asked.filter((id, i) => asked.indexOf(id) !== i);
  check('(h) E55: hello_ok nur Shell-Grant (< 16384 B), jede Eigentümer-App holt ihr Grant über die Control-Lane',
    state?.helloOkBytes > 0 && state.helloOkBytes < 16384 && wanted.every((id) => asked.includes(id))
      && (state?.grantRequests ?? []).every((g) => g.lane === 0),
    JSON.stringify({ helloOkBytes: state?.helloOkBytes, asked, wanted, duplicates }));
}check('Manifest aktuell (alle sha256 = Dateibytes)', staleFiles.length === 0, staleFiles.length ? `${staleFiles.length} veraltet` : `${Object.keys(manifestFile.files).length} Dateien`);

console.log(`[${tag}] HTTP-Protokoll (Laborserver, ${http.length}):`);
for (const r of http) if (r.kind !== 'fixture' || r.status !== 200 || !r.path.startsWith('/~fixture/cas/')) console.log(`  ${r.status} ${r.path}${r.kind !== 'app' ? ` [${r.kind}]` : ''}`);
console.log(`  (+ ${http.filter((r) => r.path.startsWith('/~fixture/cas/') && r.status === 200).length}× 200 /~fixture/cas/<sha> – nativer CAS-Speicher des Prüfstands)`);
const swSide = browserRequests.filter((r) => r.url.includes('/~a/'));
console.log(`[${tag}] /~a/-Anfragen (SW, ${swSide.length}): ${JSON.stringify(swSide.slice(0, 8).map((r) => `${r.status ?? r.failure} ${new URL(r.url).pathname}`))}`);
console.log(`[${tag}] Zustand: ${JSON.stringify({ sw: state?.sw, subscribed: state?.subscribed, activation: state?.activation, idb: state?.idbActivation })}`);
console.log(`[${tag}] Screenshot: ${shot}${launcherNotes !== null ? ` (+ Starter: ${shot.replace('-1280.png', '-launcher-1280.png')}, 'Notizen' sichtbar: ${launcherNotes})` : ''}`);
const passed = results.filter((r) => r.ok).length;
console.log(`[${tag}] ${passed}/${results.length} PASS`);
if (COUNTER) {
  // Der WebRTC-Transport prüft sha256 schon selbst (kernel/transport-webrtc.mjs:127 'Transfer hash mismatch'); runActivation
  // macht daraus artifact_missing:<pfad> (kernel/kernel.mjs:1057-1058). hash_mismatch:<pfad> wäre der Zweig ohne Transportprüfung.
  const failure = state?.activation.find((a) => a.app_id === 'shell' && a.state === 'failed' && /^(artifact_missing|hash_mismatch):v4\/frame\/index\.js$/.test(a.error ?? ''));
  console.log(`[${tag}] Gegenprobe: Rahmen ${state?.frameVisible ? 'ERSCHIENEN (Prüfung zahnlos!)' : 'fehlt'}; Kern meldet: ${failure?.error ?? 'nichts zu v4/frame/index.js'}`);
  process.exit(!state?.frameVisible && passed < results.length ? 1 : 0);
}
if (passed === results.length) process.exit(0);
const snap = { http, activation: state?.activation ?? [], browserRequests };
const open = EXPECTED_OPEN.filter((o) => o.match(snap) && !(o.probe && PROBE.has(o.probe)));
if (open.length) {
  for (const o of open) console.log(`EXPECTED_OPEN ${o.id}\n  wo: ${o.where}\n  was: ${o.what}`);
  process.exit(2);
}
process.exit(1);
