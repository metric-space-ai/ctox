// CTOX Business OS Shell v4 – Blob-Ladeweg (Rueckfall fuer Plattformen ohne Service Worker).
//
// Gleiche Aufgabe wie /sw.js, aber ohne Service Worker: jede Datei einer App-Version wird aus dem
// Kernel (IndexedDB) gelesen, per sha256 geprueft und als Blob-URL bereitgestellt.
//
// Was der Loader automatisch umschreibt (statisch, nur Stringliterale):
//   JS:  import … from './x.js' | '../x.js'      -> Blob-URL der App-Datei (rekursiv)
//        import … from 'ctox:shared/x.js'         -> Blob-URL von shell@<Shell der Instanz>/shared/x.js
//        import … from 'ctox:lib/<name>@<major>/x' -> Blob-URL von lib:<name>@<major>.<minor>/x (eigenes Plattform-
//                                                   Artefakt, §16.37; nur freigegebene STANDARD_LIBS, sonst Fehler)
//
// Bindung je App-Instanz (§16.39, Entscheidung ctox sync v4 „Bindung je Mount“): entryUrl()/resolver() bekommen die
// eingefrorene Bindung aus kernel.artifacts.bind(appId, ver) – {shellVer, libs: {"<name>": "<major>.<minor>"}} – und
// lesen sonst nichts (kein Rueckgriff auf eine aktive Shell oder Minor). Alle Dateien dieser Instanz – auch
// Bibliotheksdateien – nutzen genau diese Bindung; eine spaeter veroeffentlichte Minor gilt erst fuer den naechsten
// Mount (neues bind). ctox:lib/ einer Bibliothek, die die Bindung nicht enthaelt, ist ein Fehler (wie im SW: laedt nicht).
//   CSS: url(ctox:shared/x) / url(ctox:lib/<name>@<major>/x) -> Blob-URL der Shell- bzw. Bibliotheksdatei
//        export … from '…', import '…', import('…') wie oben
//        new URL('./pfad', import.meta.url)       -> new URL('<Blob-URL von pfad>')
//   CSS: url(./x), url('../x'), @import './x.css' -> Blob-URL
//
// Was er NICHT kann (Blob-URLs sind keine hierarchische Basis; new URL('./x', 'blob:…') wirft):
//   - import.meta.url als Wert weiterreichen (z. B. loadModuleMessages(import.meta.url, …) in shared/i18n)
//   - new URL(`./locales/${lang}.json`, import.meta.url) – Template-Literale / berechnete Pfade
//   - import(variable) / import(`…${x}`) – berechnete Modulpfade
//   - relative src/href in Markup, das die App per innerHTML einsetzt (loest gegen das Dokument auf)
//   - Import-Zyklen (eine Blob-URL existiert erst, wenn ihr Inhalt fertig ist)
//   - ctox:lib/… als Basis-URL fuer AMD-Lader (Monaco 'vs'-Pfad): braucht hierarchische URLs, nur im SW-Weg
//
// Kernel-Zugriff (gleiches IDB-Layout wie der SW, v4/sw.js Kopfkommentar):
//   kernel.readFiles(appId, ver)  -> Promise<{pfad: sha256} | null>   (meta "files:<appId>@<ver>"; Bibliothek:
//                                    appId "lib:<name>", ver "<major>.<minor>")
//   kernel.readArtifact(sha256)   -> Promise<string | Uint8Array | null>   (artifacts[sha256].content; sha256 ueber
//                                    die Bytes, Text als UTF-8)
// Standardbibliotheken (§16.37) – gleicher Vertrag wie der SW:
//   kernel.requestLib({type:'ctox:lib-miss', lib, major, path, ver, sha, reason})
//     -> Promise<{type:'ctox:lib-ready'} | {type:'ctox:lib-abort', reason}>   (holt genau diese Datei, Spur 4)
//   Nach einer Anforderung genau EIN weiterer Leseversuch; sha256-Fehler -> neu anfordern, dann Fehler.
//   Diese Stellen muessen beim Port auf ctx.resolve(pfad) -> Promise<url> umgebaut werden; die
//   Funktion liefert loader.resolver(app_id, app_ver). Der Loader meldet solche Stellen in `warnings`.

const TYPES = {
  js: 'text/javascript', mjs: 'text/javascript', css: 'text/css', html: 'text/html',
  json: 'application/json', svg: 'image/svg+xml', png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg',
  gif: 'image/gif', webp: 'image/webp', woff2: 'font/woff2', woff: 'font/woff', ttf: 'font/ttf',
  txt: 'text/plain', wasm: 'application/wasm',
};
const JS_SPEC_RE = /(\bfrom\s*|\bimport\s*\(\s*|\bimport\s*)(['"])([^'"\r\n]+)\2/g;
const JS_NEWURL_RE = /new URL\(\s*(['"])([^'"\r\n]+)\1\s*,\s*import\.meta\.url\s*\)/g;
// Freigegebene Standardbibliotheken der Shell (Quelle: v4/tools/isolation/standard-libs.json; gleich wie sw.js).
export const STANDARD_LIBS = ['ctox-office@1', 'monaco@0'];
const LIB_SPEC_RE = /^ctox:lib\/([a-z0-9][a-z0-9._-]*@\d+)\/(.+)$/;
const CSS_URL_RE = /url\(\s*(['"]?)([^'")\s]+)\1\s*\)|@import\s+(['"])([^'"]+)\3/g;

async function sha256Hex(bytes) {
  const d = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
  let s = '';
  for (const b of d) s += b.toString(16).padStart(2, '0');
  return s;
}

function isRelative(spec) { return spec.startsWith('./') || spec.startsWith('../'); }

function joinPath(fromPath, rel) {
  const parts = fromPath.split('/');
  parts.pop();
  for (const seg of rel.split('/')) {
    if (seg === '.' || seg === '') continue;
    if (seg === '..') { if (!parts.length) throw new Error(`Pfad verlaesst die App: ${rel}`); parts.pop(); } else parts.push(seg);
  }
  return parts.join('/');
}

// artifacts.content -> Bytes (Text als UTF-8, Uint8Array unveraendert); andere Formen gibt es im Kernel nicht.
function contentBytes(content) {
  if (typeof content === 'string') return new TextEncoder().encode(content);
  if (content && ArrayBuffer.isView(content)) return new Uint8Array(content.buffer, content.byteOffset, content.byteLength);
  return null;
}

/**
 * @param {{readFiles:(id:string,ver:string)=>Promise<Record<string,string>|null>,
 *          readArtifact:(sha:string)=>Promise<string|Uint8Array|null>,
 *          requestLib?:(msg:object)=>Promise<{type:string,reason?:string}>}} kernel
 */
export function createBlobLoader(kernel) {
  const urls = new Map();     // "<Bindung>|<app>@<ver>/<pfad>" -> Promise<blobUrl>
  const bindings = new Map(); // Bindungsschluessel -> {key, shellVer, libs}
  const fileLists = new Map();
  const warnings = [];

  function filesOf(appId, appVer) {
    const k = `${appId}@${appVer}`;
    if (!fileLists.has(k)) {
      fileLists.set(k, Promise.resolve(kernel.readFiles(appId, appVer)).then((f) => {
        if (!f || typeof f !== 'object') { fileLists.delete(k); throw new Error(`Unbekannte Version ${k}`); }
        return f;
      }));
    }
    return fileLists.get(k);
  }

  async function askKernel(msg) {
    if (typeof kernel.requestLib !== 'function') throw new Error('Kernel liefert keine Standardbibliotheken (requestLib fehlt)');
    const reply = await kernel.requestLib({ type: 'ctox:lib-miss', ...msg });
    if (!reply || reply.type !== 'ctox:lib-ready') {
      throw new Error(`Kernel bricht ab: lib:${msg.lib}@${msg.major}/${msg.path}${reply && reply.reason ? ` (${reply.reason})` : ''}`);
    }
  }

  async function verifiedBytes(sha) {
    const bytes = contentBytes(await kernel.readArtifact(sha));
    if (!bytes) return { reason: 'artifact' };
    if ((await sha256Hex(bytes)) !== sha) return { reason: 'hash' };
    return { bytes };
  }

  async function libBytes(appId, ver, path) {
    const name = appId.slice('lib:'.length);
    const major = ver.split('.')[0];
    for (let attempt = 0; ; attempt += 1) {
      const files = await kernel.readFiles(appId, ver);
      let miss;
      if (!files || typeof files !== 'object') miss = { sha: null, reason: 'manifest' };
      else {
        const sha = files[path];
        if (!sha) throw new Error(`Datei fehlt im Manifest: ${appId}@${ver}/${path}`);
        const got = await verifiedBytes(sha);
        if (got.bytes) return got.bytes;
        miss = { sha, reason: got.reason };
      }
      if (attempt) throw new Error(`${miss.reason === 'hash' ? 'Hash falsch' : 'Artefakt fehlt'}: ${appId}@${ver}/${path}`);
      await askKernel({ lib: name, major, path, ver, ...miss });
    }
  }

  // Bindung einer Instanz aus kernel.artifacts.bind (eingefroren); gleiche Bindung -> dasselbe Objekt.
  function bindingFor(bind) {
    if (!bind || typeof bind !== 'object' || typeof bind.shellVer !== 'string' || !bind.shellVer) {
      throw new Error('Bindung fehlt: kernel.artifacts.bind(appId, ver) beim Mount uebergeben');
    }
    const fixed = {};
    for (const lib of STANDARD_LIBS) {
      const [name, major] = lib.split('@');
      const ver = bind.libs && bind.libs[name] != null ? String(bind.libs[name]) : null;
      if (ver != null && ver.split('.')[0] === major) fixed[lib] = ver;
    }
    const key = `${bind.shellVer}${Object.keys(fixed).sort().map((k) => `~lib:${k.split('@')[0]}@${fixed[k]}`).join('')}`;
    if (!bindings.has(key)) bindings.set(key, Object.freeze({ key, shellVer: bind.shellVer, libs: Object.freeze(fixed) }));
    return bindings.get(key);
  }

  async function bytesOf(appId, appVer, path) {
    if (appId.startsWith('lib:')) return libBytes(appId, appVer, path);
    const sha = (await filesOf(appId, appVer))[path];
    if (!sha) throw new Error(`Datei fehlt im Manifest: ${appId}@${appVer}/${path}`);
    const got = await verifiedBytes(sha);
    if (got.reason === 'artifact') throw new Error(`Artefakt fehlt: ${sha}`);
    if (got.reason === 'hash') throw new Error(`Hash falsch: ${appId}@${appVer}/${path}`);
    return got.bytes;
  }

  function fileUrl(appId, appVer, path, binding, stack) {
    const key = `${binding.key}|${appId}@${appVer}/${path}`;
    if (stack.includes(key)) throw new Error(`Import-Zyklus im Blob-Weg nicht darstellbar: ${[...stack, key].join(' -> ')}`);
    if (!urls.has(key)) {
      const p = build(appId, appVer, path, binding, [...stack, key]);
      urls.set(key, p);
      p.catch(() => urls.delete(key));
    }
    return urls.get(key);
  }

  async function build(appId, appVer, path, binding, stack) {
    const shellVer = binding.shellVer;
    const ext = (path.split('.').pop() || '').toLowerCase();
    const bytes = await bytesOf(appId, appVer, path);
    let body = bytes;
    const target = async (spec) => {
      if (spec.startsWith('ctox:shared/')) {
        return fileUrl('shell', shellVer, `shared/${spec.slice('ctox:shared/'.length)}`, binding, stack);
      }
      if (spec.startsWith('ctox:lib/')) {
        const lib = LIB_SPEC_RE.exec(spec);
        if (!lib || !STANDARD_LIBS.includes(lib[1])) throw new Error(`Standardbibliothek nicht freigegeben: ${spec}`);
        const ver = binding.libs[lib[1]];
        if (!ver) throw new Error(`Standardbibliothek nicht gebunden: ${spec}`);
        return fileUrl(`lib:${lib[1].split('@')[0]}`, ver, lib[2], binding, stack);
      }
      if (isRelative(spec)) return fileUrl(appId, appVer, joinPath(path, spec), binding, stack);
      return null; // absolute URL / Blob / bare: unveraendert
    };
    if (ext === 'js' || ext === 'mjs') {
      const text = new TextDecoder().decode(bytes);
      const specs = new Set();
      for (const m of text.matchAll(JS_SPEC_RE)) specs.add(m[3]);
      for (const m of text.matchAll(JS_NEWURL_RE)) specs.add(m[2]);
      const map = new Map();
      await Promise.all([...specs].map(async (s) => { const t = await target(s); if (t) map.set(s, t); }));
      let out = text.replace(JS_NEWURL_RE, (all, q, s) => (map.has(s) ? `new URL(${q}${map.get(s)}${q})` : all));
      out = out.replace(JS_SPEC_RE, (all, lead, q, s) => (map.has(s) ? `${lead}${q}${map.get(s)}${q}` : all));
      const leftover = (out.match(/import\.meta\.url/g) || []).length;
      if (leftover) warnings.push(`${appId}@${appVer}/${path}: ${leftover}x import.meta.url nicht umschreibbar`);
      body = out;
    } else if (ext === 'css') {
      const text = new TextDecoder().decode(bytes);
      const specs = new Set();
      for (const m of text.matchAll(CSS_URL_RE)) specs.add(m[2] || m[4]);
      const map = new Map();
      await Promise.all([...specs].map(async (s) => { if (isRelative(s) || s.startsWith('ctox:shared/') || s.startsWith('ctox:lib/')) map.set(s, await target(s)); }));
      body = text.replace(CSS_URL_RE, (all, q, s, q2, s2) => {
        if (s && map.has(s)) return `url(${q}${map.get(s)}${q})`;
        if (s2 && map.has(s2)) return `@import ${q2}${map.get(s2)}${q2}`;
        return all;
      });
    }
    return URL.createObjectURL(new Blob([body], { type: TYPES[ext] || 'application/octet-stream' }));
  }

  return {
    warnings,
    /**
     * Blob-URL des Einstiegsmoduls = Mount einer App-Instanz. bind = kernel.artifacts.bind(appId, appVer) (Pflicht):
     * {shellVer, libs: {"<name>": "<major>.<minor>"}}; ein neuer Mount bringt eine neue Bindung mit.
     */
    async entryUrl(appId, appVer, entry, bind) {
      return fileUrl(appId, appVer, entry || 'index.js', bindingFor(bind), []);
    },
    /** Aufloesefunktion fuer berechnete Pfade (Port-Ersatz fuer new URL(x, import.meta.url)); Bindung wie entryUrl. */
    resolver(appId, appVer, bind) {
      const binding = bindingFor(bind);
      return async (rel) => fileUrl(appId, appVer, joinPath('index.js', rel), binding, []);
    },
    /** Gibt alle Blob-URLs einer App-Version frei (nach unmount; Module bleiben in der Modul-Map). */
    async revoke(appId, appVer) {
      for (const [k, p] of urls) {
        if (k.split('|')[1].startsWith(`${appId}@${appVer}/`)) { urls.delete(k); try { URL.revokeObjectURL(await p); } catch {} }
      }
    },
    size() { return urls.size; },
  };
}
