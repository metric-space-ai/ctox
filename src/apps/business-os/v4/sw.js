// CTOX Business OS Shell v4 – Service Worker (Ladeweg aus IndexedDB).
//
// Vertrag (Shell-v4-Entwurf §4, Sync-v4-Architektur §16.15, §16.37, §16.39):
//   - Beantwortet NUR URLs unter /~a/. Grammatik (eine App-Instanz = eine URL-Wurzel):
//       /~a/<app_id>@<app_ver>[~shell@<shell_ver>][~lib:<name>@<major>.<minor>]*/<pfad>   App- und Shell-Dateien
//       /~a/lib:<name>@<major>.<minor>[~lib:<name>@<major>.<minor>]*/<pfad>              Standardbibliotheken
//     Die Wurzel bis zum ersten "/" nach /~a/ ist die Bindung der Instanz: Shell-Version und je Bibliothek die beim Mount
//     fixierte Minor (§16.39 Nr. 1/2; die URL baut kernel.artifacts.url, nie die Shell). Relative Importe erben die
//     Wurzel, also sieht jede Datei einer Instanz dieselbe Bindung: eine Instanz mischt nie Shell-Versionen oder Minors.
//     Weil die URL die Bytes vollstaendig bestimmt, ist jede gebundene Antwort "immutable".
//   - Quelle ist ausschliesslich die IndexedDB des Kernels (Name = Parameter ?db= der Registrierung, z. B.
//     /sw.js?db=ctox-<instanz>-<nutzer>; der SW legt sie nie an). Layout des Kernels (Entscheidung ctox sync v4):
//       Store "artifacts" (keyPath hash): {hash: sha256, content} mit content = Zeichenkette (Text, sha256 ueber die
//         UTF-8-Bytes) oder Uint8Array (Binaerdatei).
//       Store "meta" (keyPath key): {key: "files:<app_id>@<app_ver>", value: {pfad: sha256}} – schreibt der Kernel bei
//         jeder Aktivierung einer App, der Shell und jeder Bibliothek (lib:<name>, app_ver "<major>.<minor>").
//     Einen Store "manifests" gibt es nicht; andere meta-Schluessel (Aktivierung, aktive Minor) liest der SW nie –
//     alles, was eine Antwort bestimmt, steht in der URL. sha256 wird bei JEDER Auslieferung geprueft.
//   - Unbekannt oder Hash falsch -> 404. Es gibt KEINEN Netz-Rueckgriff.
//   - .js/.mjs/.css: "ctox:shared/<x>" (statisches import/export-from und import("…") mit Stringliteral, in CSS
//     @import/url()) -> /~a/shell@<sv><~lib:-Bindung der URL>/shared/<x> (§16.50 Nr. 2). sv = ~shell@ der URL; bei
//     app_id "shell" deren app_ver. Eine App-URL ohne ~shell@ (kernel.artifacts.url bindet immer eine Shell) kann
//     ctox:shared/ nicht aufloesen -> 404; es gibt keinen Rueckgriff auf eine "aktive" Shell (die Antwort haengt nur
//     von der URL ab).
//     Die shared-URL ist die EXAKT gebundene Shell-Wurzel samt ~lib:-Segmenten: der Rahmen laeuft unter
//     /~a/shell@<sv>~lib:…/ (kernel.artifacts.bind('shell', sv)) und importiert ../../shared/<x> relativ, also unter
//     genau dieser Wurzel. Dieselbe Wurzel fuer ctox:shared/ ergibt genau EINE Modulinstanz je shared-Datei. Eine
//     App-Instanz, deren Bindung sich von der der Shell unterscheidet (zusaetzliche Bibliothek, andere Minor), bekommt
//     folgerichtig eigene shared-Instanzen; dass Apps mit der Lib-Bindung der Shell montiert werden, sichert der Kernel.
//     shared/-Dateien selbst duerfen ctox:lib/ verwenden: es wird nach derselben Bindung aufgeloest.
//   - .js/.mjs/.css: Stringliterale bzw. url(...) mit Praefix "ctox:lib/<name>@<major>/" -> /~a/lib:<name>@<major>.<minor>
//     <restliche Bindung>/ (Import-Spezifizierer UND Basis-URLs, z. B. Monacos AMD-Pfad), aber nur fuer freigegebene
//     Standardbibliotheken (STANDARD_LIBS = v4/tools/isolation/standard-libs.json, status "approved"; Test haelt beide
//     gleich) UND nur, wenn die URL der anfragenden Datei eine Minor fuer <name>@<major> bindet. Sonst bleibt der
//     Verweis unveraendert und laedt nicht (kein Rueckgriff auf eine "aktuelle" Minor: das wuerde Minors mischen).
//     In einer Bibliotheksdatei bindet der Kopf (lib:<name>@<major>.<minor>) die eigene Bibliothek, der Rest der
//     Wurzel die uebrigen; die erzeugte Bibliotheks-URL traegt die Bindung ohne sich selbst, sortiert nach Name.
//
// Standardbibliotheken (Kern §16.37, geaendert durch §16.39 Nr. 2): jede Bibliothek ist ein eigenes Plattform-Artefakt
// lib:<name> (major.minor), nicht Teil des Shell-Artefakts. /~a/lib:<name>@<major>.<minor>/<pfad> loest der SW so auf
// (Kernel-IDB wie oben; die Minor steht in der URL):
//   meta "files:lib:<name>@<major>.<minor>" -> value {pfad: sha256}
//   artifacts "<sha256>"                    -> {hash, content}     (sha256 bei JEDER Auslieferung geprueft)
//   Nicht freigegeben -> 404. Dateiliste vorhanden, Pfad nicht darin -> 404 (Datei gibt es nicht).
//   Die Minor waehlt der Kernel beim Mount der App-Instanz (kernel.artifacts.bind: hoechste veroeffentlichte Minor der
//   Major) und schreibt sie in die URL; er bindet dabei jede Bibliothek des Shell-Manifests (libs) und der App.
//   Fehlt die Dateiliste oder Datei, oder ist der Hash falsch: Anforderung an den Kernel (genau diese eine Datei,
//   kein Eager-Laden) und auf seine Antwort warten; danach EIN weiterer Leseversuch, sonst 404. Kein HTTP-Datenweg.
//   Abgebrochen wird, wenn der Kernel es meldet (404) oder die Frist (15 s) ablaeuft (504).
//   Anforderung (SW -> Kernel): an den anfragenden Client, wenn er ein Fenster ist (event.clientId). Kommt die
//     Anforderung aus einem Worker (Monaco-Worker, §16.50 Nr. 1: Worker starten gleich-urspruenglich unter der
//     gebundenen Lib-Wurzel, nie aus data:/blob:), hat der Worker keinen Kernel: sie geht an ein Fenster aus
//     clients.matchAll({type:'window'}) – bevorzugt der Leiter, dann ein anderes Fenster mit Kernel, dann das
//     fokussierte, sonst das erste. Kernel-Fenster melden sich beim SW mit
//     postMessage({type:'ctox:kernel', leader: true|false}) (bei Start und Leiterwechsel; ohne Meldung gilt jedes
//     Fenster). Gibt es (noch) kein Fenster, sucht der SW weiter, bis die Frist ablaeuft.
//     Frist: hoechstens LIB_MISS_TIMEOUT_MS (15 s) ab der Anforderung; danach 504 mit Grund im Body (no-store).
//   Nachricht:
//     client.postMessage({type:'ctox:lib-miss', lib, major, path, ver, sha, reason}, [port])
//     lib/major/path = Bibliothek, Major, Pfad im Artefakt; ver = "<major>.<minor>" aus der URL (genau diese Minor
//     holen, nicht die aktuell aktive); sha = erwartete sha256 oder null; reason = 'manifest' (files:-Liste der Minor fehlt)
//     | 'artifact' (Datei fehlt) | 'hash' (Datei in IDB, sha256 falsch -> neu holen, Arch. §16.15a Nr. 3).
//   Antwort (Kernel -> SW) auf dem uebergebenen MessagePort: {type:'ctox:lib-ready'} = Datei (und ggf. Manifest)
//     (und files:-Liste) liegt in IDB; {type:'ctox:lib-abort', reason} = Abbruch -> 404. Gleichzeitige Anforderungen derselben Datei
//     teilen sich eine Kernel-Anfrage. Keine Antwort / kein Fenster binnen Frist -> 504 (Grund im Body).
//   Treffer (SW -> Kernel, §16.39 Nr. 2): jede ausgelieferte Bibliotheksdatei zaehlt; gebuendelt hoechstens einmal je
//     Sekunde je Client (der anfragende, aus einem Worker wie bei der Anforderung an ein Kernel-Fenster) per client.postMessage({type:'ctox:lib-hit', hits:[
//     {lib, major, ver, path, sha, n, last_ms}]}) ohne Port und ohne Antwort. n = Treffer seit der letzten Meldung,
//     last_ms = Zeit des letzten Treffers. Der Kernel pflegt daraus LRU und Byte-Budget; der SW schreibt nie in IDB.
//   - Nichts sonst: kein Cache-API, keine Logik ausser Ausliefern, Umschreiben, Kernel-Anforderung und Treffermeldung.

// Name der Kernel-Datenbank aus der Registrierungs-URL (/sw.js?db=…); ohne Angabe gibt es keine Quelle (-> 404).
const dbName = () => { try { return new URL(self.location.href).searchParams.get('db') || ''; } catch { return ''; } };
const FILES_KEY = (appId, ver) => `files:${appId}@${ver}`;
// Gruppen: 1 app_id, 2 app_ver, 3 shell_ver (optional), 4 pfad. Die ~lib:-Bindung liest parseBinding aus der Wurzel
// (nicht gefangen, damit die Gruppen stabil bleiben; das Rahmen-Labor liest dieses Literal).
const URL_RE = /^\/~a\/([a-z0-9][a-z0-9._-]*)@([A-Za-z0-9._+-]+)(?:~shell@([A-Za-z0-9._+-]+))?(?:~lib:[a-z0-9][a-z0-9._-]*@\d+\.\d+)*\/(.+)$/;
// /~a/lib:<name>@<major>.<minor>[~lib:…]*/<pfad> – eigenes Plattform-Artefakt der Bibliothek (§16.37, Minor §16.39).
// Gruppen: 1 name, 2 major, 3 minor, 4 pfad.
const LIB_URL_RE = /^\/~a\/lib:([a-z0-9][a-z0-9._-]*)@(\d+)\.(\d+)(?:~lib:[a-z0-9][a-z0-9._-]*@\d+\.\d+)*\/(.+)$/;
const LIB_BIND_RE = /~lib:([a-z0-9][a-z0-9._-]*)@(\d+)\.(\d+)/g;
// import x from '…' | import '…' | export … from '…' | import('…')  – nur Stringliterale.
const SHARED_RE = /(\bfrom\s*|\bimport\s*\(\s*|\bimport\s*)(['"])ctox:shared\/([^'"\r\n]+)\2/g;
// CSS: @import "ctox:shared/x.css" / @import url(ctox:shared/x.css) / url("ctox:shared/a.svg")
const SHARED_CSS_RE = /(['"]?)ctox:shared\/([^'")\s]+)\1/g;
// Freigegebene Standardbibliotheken der Shell (Quelle: v4/tools/isolation/standard-libs.json).
const STANDARD_LIBS = ['ctox-office@1', 'monaco@0'];
// 'ctox:lib/<name>@<major>/…' in JS-Strings (' " `) und CSS (url(…), @import "…"): nur das Praefix wird ersetzt.
const LIB_RE = /(['"`(])ctox:lib\/([a-z0-9][a-z0-9._-]*@\d+)\//g;
// Treffermeldung an den Kernel: hoechstens einmal je Sekunde (§16.39 Nr. 2).
const LIB_HIT_INTERVAL_MS = 1000;
// Hoechstens so lange wartet eine Bibliotheksanforderung auf Fenster und Kernel-Antwort (§16.50 Nr. 1), dann 504.
const LIB_MISS_TIMEOUT_MS = 15000;
const LIB_WINDOW_POLL_MS = 250;
const IMMUTABLE = 'public, max-age=31536000, immutable';
const TYPES = {
  js: 'text/javascript; charset=utf-8', mjs: 'text/javascript; charset=utf-8',
  css: 'text/css; charset=utf-8', html: 'text/html; charset=utf-8',
  json: 'application/json; charset=utf-8', svg: 'image/svg+xml', png: 'image/png',
  jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp', ico: 'image/x-icon',
  woff2: 'font/woff2', woff: 'font/woff', ttf: 'font/ttf', otf: 'font/otf',
  txt: 'text/plain; charset=utf-8', md: 'text/markdown; charset=utf-8', wasm: 'application/wasm',
  map: 'application/json; charset=utf-8',
};

// Lebenszyklus: sofort uebernehmen, damit der Loader nicht auf einen zweiten Seitenaufruf wartet.
self.addEventListener('install', () => self.skipWaiting());
self.addEventListener('activate', (event) => event.waitUntil(self.clients.claim()));
// Harter Neuladen (Shift+Reload) laesst die Seite unkontrolliert, obwohl der SW aktiv ist (gemessen,
// RESULTS.md "Harter Neuladen"). Der Loader schickt dann 'ctox:claim'; claim() uebernimmt die Seite.
// Kernel-Fenster melden sich ('ctox:kernel', leader); nur Ziel fuer weitergeleitete Anforderungen aus Workern.
const kernelWindows = new Map(); // clientId -> {leader}
self.addEventListener('message', (event) => {
  if (event.data === 'ctox:claim') { event.waitUntil(self.clients.claim()); return; }
  const d = event.data;
  if (d && d.type === 'ctox:kernel' && event.source && event.source.id) kernelWindows.set(event.source.id, { leader: !!d.leader });
});

self.addEventListener('fetch', (event) => {
  const url = new URL(event.request.url);
  if (url.origin !== self.location.origin || !url.pathname.startsWith('/~a/')) return; // nicht unser Pfad
  // Treffermeldung haelt den SW am Leben, bis sie zugestellt ist (FetchEvent ist ein ExtendableEvent).
  const keepAlive = (p) => { if (typeof event.waitUntil === 'function') event.waitUntil(p); };
  const done = url.pathname.startsWith('/~a/lib:') ? serveLib(url.pathname, event.clientId, keepAlive) : serve(url.pathname);
  event.respondWith(done.catch(() => notFound()));
});

let dbPromise = null;
function openDb() {
  if (dbPromise) return dbPromise;
  dbPromise = new Promise((resolve, reject) => {
    const name = dbName();
    if (!name) { dbPromise = null; reject(new Error('db fehlt')); return; }
    const req = indexedDB.open(name);
    // Der SW legt die Datenbank nie an (das ist Sache des Kernels): Upgrade abbrechen.
    req.onupgradeneeded = () => req.transaction.abort();
    req.onsuccess = () => {
      const db = req.result;
      db.onversionchange = () => { db.close(); dbPromise = null; };
      db.onclose = () => { dbPromise = null; };
      resolve(db);
    };
    req.onerror = () => { dbPromise = null; reject(req.error); };
    req.onblocked = () => { dbPromise = null; reject(new Error('blocked')); };
  });
  return dbPromise;
}

function get(db, store, key) {
  return new Promise((resolve, reject) => {
    const req = db.transaction(store, 'readonly').objectStore(store).get(key);
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

async function sha256Hex(bytes) {
  const d = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
  let s = '';
  for (const b of d) s += b.toString(16).padStart(2, '0');
  return s;
}

// Dateiliste {pfad: sha256} einer Version (meta files:<app>@<ver>) oder null.
async function filesOf(db, appId, ver) {
  const row = await get(db, 'meta', FILES_KEY(appId, ver));
  const files = row && row.value;
  return files && typeof files === 'object' ? files : null;
}

// Inhalt eines Artefakts als Bytes (Zeichenkette -> UTF-8, Uint8Array unveraendert) oder null.
async function artifactBytes(db, sha) {
  const row = await get(db, 'artifacts', sha);
  const c = row ? row.content : undefined;
  if (typeof c === 'string') return new TextEncoder().encode(c);
  if (c && ArrayBuffer.isView(c)) return new Uint8Array(c.buffer, c.byteOffset, c.byteLength);
  return null;
}

function notFound() {
  return new Response('', { status: 404, headers: { 'Cache-Control': 'no-store' } });
}

// Frist der Kernel-Anforderung abgelaufen (§16.50 Nr. 1): 504, Grund als Text im Body, nie zwischengespeichert.
function gatewayTimeout(reason) {
  return new Response(String(reason || 'Zeitueberschreitung'), { status: 504, headers: { 'Cache-Control': 'no-store', 'Content-Type': 'text/plain; charset=utf-8' } });
}

// ---------------------------------------------------------------- Bindung (reine Funktionen, in Node pruefbar)

// Wurzel der Instanz: "/~a/<wurzel>/<pfad>" -> "<wurzel>".
function rootOf(pathname) {
  const end = pathname.indexOf('/', 4);
  return end < 0 ? '' : pathname.slice(4, end);
}

// "~lib:a@1.4~lib:b@0.32" -> {"a@1": "1.4", "b@0": "0.32"}; doppelte Bindung derselben <name>@<major> -> null
// (mehrdeutig). Nicht freigegebene Namen werden ignoriert (sie werden nie umgeschrieben).
function parseBinding(suffix) {
  const out = {};
  for (const m of String(suffix || '').matchAll(LIB_BIND_RE)) {
    const key = `${m[1]}@${m[2]}`;
    if (key in out) return null;
    out[key] = `${m[2]}.${m[3]}`;
  }
  return out;
}

// {"b@0": "0.32", "a@1": "1.4"} -> "~lib:a@1.4~lib:b@0.32" (sortiert, ohne exceptKey).
function bindingSuffix(binding, exceptKey) {
  return Object.keys(binding || {}).filter((k) => k !== exceptKey).sort()
    .map((k) => `~lib:${k.split('@')[0]}@${binding[k]}`).join('');
}

// Reine Textumschreibung (ohne IndexedDB), damit sie in Node pruefbar ist (v4/tools/isolation/ctox-lib.test.mjs).
// binding = Bibliotheks-Bindung der anfragenden URL ({"<name>@<major>": "<major>.<minor>"}).
function rewriteShellRefs(text, ext, shellVer, binding) {
  let out = text;
  if (shellVer != null && text.includes('ctox:shared/')) {
    // Exakt gebundene Shell-Wurzel samt ~lib:-Segmenten (§16.50 Nr. 2): eine Modulinstanz mit den Rahmen-Importen.
    const root = `/~a/shell@${shellVer}${bindingSuffix(binding)}/shared/`;
    out = ext === 'css'
      ? text.replace(SHARED_CSS_RE, (_all, q, rest) => `${q}${root}${rest}${q}`)
      : text.replace(SHARED_RE, (_all, lead, q, rest) => `${lead}${q}${root}${rest}${q}`);
  }
  return rewriteLibRefs(out, binding);
}

// Nur ctox:lib/ – auch fuer Dateien der Bibliotheken selbst. Ungebunden oder nicht freigegeben -> unveraendert.
function rewriteLibRefs(text, binding) {
  if (!binding || !text.includes('ctox:lib/')) return text;
  return text.replace(LIB_RE, (all, lead, lib) => {
    if (!STANDARD_LIBS.includes(lib) || !binding[lib]) return all;
    return `${lead}/~a/lib:${lib.split('@')[0]}@${binding[lib]}${bindingSuffix(binding, lib)}/`;
  });
}

function decodePath(rawPath) {
  let path;
  try { path = decodeURIComponent(rawPath); } catch { return null; }
  return path.split('/').some((seg) => seg === '..' || seg === '.' || seg === '') ? null : path;
}

function okResponse(body, ext, cacheControl) {
  return new Response(body, {
    status: 200,
    headers: {
      'Content-Type': TYPES[ext] || 'application/octet-stream',
      'Cache-Control': cacheControl,
      'X-Content-Type-Options': 'nosniff',
    },
  });
}

async function serve(pathname) {
  const m = URL_RE.exec(pathname);
  if (!m) return notFound();
  const [, appId, appVer, pinnedShell, rawPath] = m;
  const binding = parseBinding(rootOf(pathname));
  if (!binding) return notFound();
  const path = decodePath(rawPath);
  if (!path) return notFound();

  const db = await openDb();
  const files = await filesOf(db, appId, appVer);
  const sha = files && files[path];
  if (!sha) return notFound();
  const bytes = await artifactBytes(db, sha);
  if (!bytes) return notFound();
  if ((await sha256Hex(bytes)) !== sha) return notFound(); // manipuliert -> nie ausfuehren

  const ext = (path.split('.').pop() || '').toLowerCase();
  let body = bytes;
  if (ext === 'js' || ext === 'mjs' || ext === 'css') {
    const text = new TextDecoder().decode(body);
    if (text.includes('ctox:shared/')) {
      // Shell-Dateien gehoeren zu ihrer eigenen Version, Apps zur ~shell@-Bindung ihrer URL; ungebunden -> 404.
      const shellVer = appId === 'shell' ? appVer : pinnedShell;
      if (!shellVer) return notFound();
      body = rewriteShellRefs(text, ext, shellVer, binding);
    } else if (text.includes('ctox:lib/')) body = rewriteLibRefs(text, binding);
  }
  return okResponse(body, ext, IMMUTABLE);
}

// ---------------------------------------------------------------- Standardbibliotheken (§16.37, §16.39)

// Ein Leseversuch aus IDB: {bytes, sha} oder {miss: {ver, sha, reason}} oder {absent: true} (Pfad nicht in files:).
async function readLib(db, name, ver, path) {
  const files = await filesOf(db, `lib:${name}`, ver);
  if (!files) return { miss: { ver, sha: null, reason: 'manifest' } };
  const sha = files[path];
  if (!sha) return { absent: true };
  const bytes = await artifactBytes(db, sha);
  if (!bytes) return { miss: { ver, sha, reason: 'artifact' } };
  if ((await sha256Hex(bytes)) !== sha) return { miss: { ver, sha, reason: 'hash' } };
  return { bytes, sha };
}

// Fenster-Client mit Kernel fuer eine Anforderung/Meldung des Clients clientId: der Client selbst, wenn er ein Fenster
// ist; sonst (Worker, unbekannt) ein Fenster – Leiter vor gemeldetem Kernel-Fenster vor fokussiertem vor erstem.
async function kernelClient(clientId) {
  if (clientId) {
    const own = await self.clients.get(clientId);
    if (own && (own.type === 'window' || own.type === undefined)) return own;
  }
  const wins = (await self.clients.matchAll({ type: 'window' })).filter((c) => !c.type || c.type === 'window');
  for (const id of [...kernelWindows.keys()]) if (!wins.some((c) => c.id === id)) kernelWindows.delete(id);
  const known = wins.filter((c) => kernelWindows.has(c.id));
  return known.find((c) => kernelWindows.get(c.id).leader) || known[0] || wins.find((c) => c.focused) || wins[0] || null;
}

// Wie kernelClient, wartet aber bis zur Frist auf ein Fenster (ein Worker kann vor seinem Kernel-Fenster anfragen).
async function awaitKernelClient(clientId, deadline) {
  for (;;) {
    const client = await kernelClient(clientId);
    if (client || Date.now() >= deadline) return client;
    await new Promise((r) => setTimeout(r, Math.min(LIB_WINDOW_POLL_MS, Math.max(0, deadline - Date.now()))));
  }
}

// Eine Anfrage je Datei, gleichzeitige Misses warten auf dieselbe Kernel-Antwort.
const pendingLib = new Map();
function requestLib(clientId, msg) {
  const key = `${msg.lib}@${msg.ver}/${msg.path}`;
  if (!pendingLib.has(key)) {
    const deadline = Date.now() + LIB_MISS_TIMEOUT_MS;
    const timeout = (why) => ({ type: 'ctox:lib-timeout', reason: `${why} binnen ${LIB_MISS_TIMEOUT_MS} ms (lib:${key})` });
    const p = (async () => {
      const client = await awaitKernelClient(clientId, deadline);
      if (!client) return timeout('kein Fenster mit Kernel');
      return new Promise((resolve) => {
        const channel = new MessageChannel();
        const timer = setTimeout(() => { channel.port1.close(); resolve(timeout('keine Kernel-Antwort')); }, Math.max(0, deadline - Date.now()));
        channel.port1.onmessage = (ev) => { clearTimeout(timer); channel.port1.close(); resolve(ev.data || {}); };
        client.postMessage(msg, [channel.port2]);
      });
    })();
    pendingLib.set(key, p);
    const clear = () => { pendingLib.delete(key); };
    p.then(clear, clear);
  }
  return pendingLib.get(key);
}

// Treffer je Client sammeln; die erste Meldung nach einer Pause geht eine Sekunde spaeter raus, also hoechstens
// eine Meldung je Sekunde. Geht ein Treffer verloren (SW beendet), ist das nur eine ungenauere LRU.
const libHits = new Map(); // clientId ('' = unbekannt) -> Map("<lib>@<ver>/<pfad>" -> hit)
let libHitFlush = null;
function recordLibHit(clientId, hit) {
  const cid = clientId || '';
  if (!libHits.has(cid)) libHits.set(cid, new Map());
  const per = libHits.get(cid);
  const key = `${hit.lib}@${hit.ver}/${hit.path}`;
  const prev = per.get(key);
  per.set(key, { ...hit, n: (prev ? prev.n : 0) + 1, last_ms: Date.now() });
  if (!libHitFlush) {
    libHitFlush = new Promise((resolve) => {
      setTimeout(() => { libHitFlush = null; flushLibHits().then(resolve, resolve); }, LIB_HIT_INTERVAL_MS);
    });
  }
  return libHitFlush;
}
async function flushLibHits() {
  const batch = [...libHits];
  libHits.clear();
  for (const [cid, per] of batch) {
    const client = await kernelClient(cid || null);
    if (client) client.postMessage({ type: 'ctox:lib-hit', hits: [...per.values()] });
  }
}

async function serveLib(pathname, clientId, keepAlive = () => {}) {
  const m = LIB_URL_RE.exec(pathname);
  if (!m) return notFound(); // auch die Altform /~a/lib:<name>@<major>/ ohne Minor
  const [, name, major, minor, rawPath] = m;
  const key = `${name}@${major}`;
  if (!STANDARD_LIBS.includes(key)) return notFound(); // nicht freigegeben
  const rest = parseBinding(rootOf(pathname).slice(`lib:${key}.${minor}`.length));
  if (!rest || key in rest) return notFound(); // mehrdeutige Bindung
  const ver = `${major}.${minor}`;
  const binding = { ...rest, [key]: ver };
  const path = decodePath(rawPath);
  if (!path) return notFound();

  const db = await openDb();
  let got = await readLib(db, name, ver, path);
  if (got.miss) {
    const reply = await requestLib(clientId, { type: 'ctox:lib-miss', lib: name, major, path, ...got.miss });
    if (reply && reply.type === 'ctox:lib-timeout') return gatewayTimeout(reply.reason);
    if (!reply || reply.type !== 'ctox:lib-ready') return notFound();
    got = await readLib(await openDb(), name, ver, path);
  }
  if (!got.bytes) return notFound();
  keepAlive(recordLibHit(clientId, { lib: name, major, ver, path, sha: got.sha }));

  const ext = (path.split('.').pop() || '').toLowerCase();
  let body = got.bytes;
  if (ext === 'js' || ext === 'mjs' || ext === 'css') {
    const text = new TextDecoder().decode(body);
    if (text.includes('ctox:lib/')) body = rewriteLibRefs(text, binding);
  }
  // Die URL traegt Minor und Bindung: dieselbe URL liefert immer dieselben Bytes.
  return okResponse(body, ext, IMMUTABLE);
}
