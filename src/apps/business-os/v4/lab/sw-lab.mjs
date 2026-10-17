// Labor-Hilfe: gebundene Instanz-URLs /~a/… ohne Service Worker so ausliefern, wie der SW (v4/sw.js) es tut.
//
// Wofür: Im echten Ladeweg montiert der Kernel die Shell unter ihrer Instanz-URL
//   /~a/shell@<sv>~lib:ctox-office@<M.m>~lib:monaco@<M.m>/<pfad>      (kernel.artifacts.bind('shell', sv).url(pfad))
// und der SW schreibt in jeder .js/.mjs/.css-Datei 'ctox:lib/<name>@<major>/' nach der Bindung DIESER URL auf
//   /~a/lib:<name>@<M.m>[~lib:…]/
// um. Labore ohne SW laden die Shell aus Rohpfaden (/v4/…): dort bleibt 'ctox:lib/monaco@0/' stehen, fetch() lehnt das
// Schema ctox: ab und editor.js fällt still auf die textarea zurück. Diese Hilfe liefert stattdessen dieselben URLs mit
// denselben Bytes wie der SW: die Umschreibung ist die des SW selbst (sw.js per vm geladen, keine Kopie), die Bindung
// kommt aus der URL-Wurzel (parseBinding aus sw.js), die URL selbst baut der Kern-Stub (artifacts.bind). Nicht
// nachgebildet sind nur IDB-Quelle, sha256-Prüfung und Kernel-Anforderung (die prüfen sw-lib-miss.test.mjs und
// v4/lab/ladeweg); Quelle ist hier der Quellbaum bzw. der Quellordner der Bibliothek aus standard-libs.json.
//
//   resolveInstance(urlPath, bosRoot) -> {file, ext, shellVer, binding} | null   (nur /~a/shell@… und /~a/lib:…)
//   instanceBody(hit, bytes) -> Buffer                                           (Umschreibung wie der SW)
//   LAB_LIBS = {"<name>": "<major>.0"} – was die Laborseite als lib:<name>@<major>.0 veröffentlicht
import { readFileSync, existsSync, statSync } from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const V4 = path.resolve(HERE, '..');
const BOS = path.resolve(V4, '..');

// sw.js ist ein klassisches Worker-Skript: in einem vm-Kontext mit Attrappe für self laden (wie ctox-lib.test.mjs).
function loadSw() {
  const ctx = { self: { addEventListener() {}, location: { origin: 'http://lab' }, clients: {} }, URL, TextDecoder, Response: class {} };
  vm.createContext(ctx);
  vm.runInContext(`${readFileSync(path.join(V4, 'sw.js'), 'utf8')}\n;globalThis.__api = { rewriteShellRefs, rewriteLibRefs, parseBinding, rootOf, decodePath, STANDARD_LIBS, URL_RE, LIB_URL_RE };`, ctx);
  return ctx.__api;
}
export const SW = loadSw();

// Freigegebene Bibliotheken: "<name>@<major>" -> Quellordner (erstes Wort von "source", z. B. vendor/monaco/).
const LIBS_JSON = JSON.parse(readFileSync(path.join(V4, 'tools', 'isolation', 'standard-libs.json'), 'utf8')).libs || {};
export const LIB_DIRS = Object.fromEntries(Object.entries(LIBS_JSON)
  .filter(([k, v]) => v.status === 'approved' && SW.STANDARD_LIBS.includes(k))
  .map(([k, v]) => [k, path.join(BOS, String(v.source || '').split(/\s+/)[0])]));
// Laborseiten veröffentlichen je Bibliothek lib:<name>@<major>.0 und die Shell mit libs {<name>: <major>}.
export const LAB_LIBS = Object.freeze(Object.fromEntries(Object.keys(LIB_DIRS).map((k) => { const [n, m] = k.split('@'); return [n, `${m}.0`]; })));

const inside = (file, dir) => file === dir || file.startsWith(dir.endsWith(path.sep) ? dir : dir + path.sep);
const isFile = (f) => existsSync(f) && statSync(f).isFile();

// /~a/shell@<sv>[~lib:…]*/<pfad> -> <bosRoot>/<pfad>; /~a/lib:<name>@<M.m>[~lib:…]*/<pfad> -> Bibliotheksordner/<pfad>.
// Grammatik und Bindung wie im SW (URL_RE / LIB_URL_RE / parseBinding); mehrdeutige Bindung oder unbekannte
// Bibliothek -> null (der SW antwortet dann 404).
export function resolveInstance(urlPath, bosRoot = BOS) {
  if (!urlPath.startsWith('/~a/')) return null;
  const root = SW.rootOf(urlPath);
  if (urlPath.startsWith('/~a/lib:')) {
    const m = SW.LIB_URL_RE.exec(urlPath);
    if (!m) return null;
    const [, name, major, minor, raw] = m;
    const key = `${name}@${major}`;
    const dir = LIB_DIRS[key];
    const rest = SW.parseBinding(root.slice(`lib:${key}.${minor}`.length));
    const rel = SW.decodePath(raw);
    if (!dir || !rest || !rel || key in rest) return null;
    const file = path.join(dir, rel);
    if (!inside(file, dir) || !isFile(file)) return null;
    return { file, ext: path.extname(file).slice(1).toLowerCase(), shellVer: null, binding: { ...rest, [key]: `${major}.${minor}` } };
  }
  const m = SW.URL_RE.exec(urlPath);
  if (!m || m[1] !== 'shell' || m[3]) return null; // nur die Shell-Instanz; Apps bleiben beim Labor-Server
  const binding = SW.parseBinding(root);
  const rel = SW.decodePath(m[4]);
  if (!binding || !rel) return null;
  const file = path.join(bosRoot, rel);
  if (!inside(file, bosRoot) || !isFile(file)) return null;
  return { file, ext: path.extname(file).slice(1).toLowerCase(), shellVer: m[2], binding };
}

// Antwortkörper wie serve()/serveLib() im SW: nur .js/.mjs/.css, nur ctox:shared/ und ctox:lib/.
export function instanceBody(hit, bytes = readFileSync(hit.file)) {
  if (hit.ext !== 'js' && hit.ext !== 'mjs' && hit.ext !== 'css') return bytes;
  const text = bytes.toString('utf8');
  if (text.includes('ctox:shared/') && hit.shellVer) return Buffer.from(SW.rewriteShellRefs(text, hit.ext, hit.shellVer, hit.binding));
  if (text.includes('ctox:lib/')) return Buffer.from(SW.rewriteLibRefs(text, hit.binding));
  return bytes;
}
