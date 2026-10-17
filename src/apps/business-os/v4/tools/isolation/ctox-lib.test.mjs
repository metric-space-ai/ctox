// node --test src/apps/business-os/v4/tools/isolation/ctox-lib.test.mjs
// ctox:lib/<name>@<major>/… – Aufloesung im Service Worker (v4/sw.js) und im Blob-Ladeweg (v4/blob-loader.js).
// Zielform (Kern §16.37, Minor nach §16.39 Nr. 2): eigenes Plattform-Artefakt /~a/lib:<name>@<major>.<minor>/<pfad>, die
// Minor aus der Bindung der anfragenden URL (beim Mount fixiert); nicht mehr unter shell@<ver>/lib/, keine Major-Form.
// Miss -> Kernel-Anforderung -> Auslieferung im SW: sw-lib-miss.test.mjs.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { createBlobLoader, STANDARD_LIBS as BLOB_LIBS } from '../../blob-loader.js';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const V4 = path.resolve(HERE, '..', '..');
const approved = Object.entries(JSON.parse(readFileSync(path.join(HERE, 'standard-libs.json'), 'utf8')).libs)
  .filter(([, v]) => v.status === 'approved').map(([k]) => k).sort();

// sw.js ist ein klassisches Worker-Skript: in einem vm-Kontext mit Attrappe fuer self laden.
function loadSw() {
  const ctx = { self: { addEventListener() {}, location: { origin: 'https://x' }, clients: {} }, URL, TextDecoder, Response: class {} };
  vm.createContext(ctx);
  vm.runInContext(`${readFileSync(path.join(V4, 'sw.js'), 'utf8')}\n;globalThis.__api = { rewriteShellRefs, rewriteLibRefs, parseBinding, bindingSuffix, STANDARD_LIBS, URL_RE, LIB_URL_RE };`, ctx);
  return ctx.__api;
}
const sw = loadSw();

test('Freigabeliste: sw.js und blob-loader.js = approved-Eintraege aus standard-libs.json', () => {
  assert.deepEqual([...sw.STANDARD_LIBS].sort(), approved);
  assert.deepEqual([...BLOB_LIBS].sort(), approved);
});

const BIND = { 'ctox-office@1': '1.4', 'monaco@0': '0.32' };

test('SW: JS-Import, dynamischer Import und Basis-URL-String -> /~a/lib:<name>@<major>.<minor> aus der Bindung', () => {
  const src = [
    "import { a } from 'ctox:lib/ctox-office@1/ctox-office-document.mjs';",
    'const m = await import("ctox:lib/ctox-office@1/ctox-office-spreadsheet.mjs");',
    "export const MONACO_BASE = 'ctox:lib/monaco@0/';",
    'const t = `ctox:lib/monaco@0/vs/${x}`;',
    "import { s } from 'ctox:shared/i18n.js';",
  ].join('\n');
  const out = sw.rewriteShellRefs(src, 'js', '4.0.1', BIND);
  // Die erzeugte Bibliotheks-URL traegt die uebrige Bindung (sortiert, ohne sich selbst): eine Instanz mischt nie Minors.
  assert.ok(out.includes("from '/~a/lib:ctox-office@1.4~lib:monaco@0.32/ctox-office-document.mjs'"));
  assert.ok(out.includes('import("/~a/lib:ctox-office@1.4~lib:monaco@0.32/ctox-office-spreadsheet.mjs")'));
  assert.ok(out.includes("MONACO_BASE = '/~a/lib:monaco@0.32~lib:ctox-office@1.4/'"));
  assert.ok(out.includes('`/~a/lib:monaco@0.32~lib:ctox-office@1.4/vs/${x}`'));
  // §16.50 Nr. 2: shared auf die exakt gebundene Shell-Wurzel (mit ~lib:-Segmenten, sortiert) = Wurzel des Rahmens
  assert.ok(out.includes("from '/~a/shell@4.0.1~lib:ctox-office@1.4~lib:monaco@0.32/shared/i18n.js'"));
  assert.ok(!out.includes('ctox:lib/'));
  assert.ok(!out.includes('/~a/shell@4.0.1/lib/'), 'alte Form unter dem Shell-Artefakt entfaellt');
  assert.ok(!/\/~a\/lib:[a-z-]+@\d+\//.test(out), 'keine Major-Form ohne Minor');
});

test('SW: CSS url()/@import werden mit Minor umgeschrieben', () => {
  const css = '@import "ctox:lib/ctox-office@1/office.css";\n.a { background: url(ctox:lib/monaco@0/x.svg); }\n.b { background: url("ctox:lib/monaco@0/y.svg"); }';
  const out = sw.rewriteShellRefs(css, 'css', '7', BIND);
  assert.ok(out.includes('@import "/~a/lib:ctox-office@1.4~lib:monaco@0.32/office.css"'));
  assert.ok(out.includes('url(/~a/lib:monaco@0.32~lib:ctox-office@1.4/x.svg)'));
  assert.ok(out.includes('url("/~a/lib:monaco@0.32~lib:ctox-office@1.4/y.svg")'));
});

test('SW: ungebundene Bibliothek bleibt unveraendert (kein Rueckgriff auf eine aktuelle Minor)', () => {
  const src = "import a from 'ctox:lib/ctox-office@1/a.mjs';\nconst B = 'ctox:lib/monaco@0/';";
  assert.equal(sw.rewriteShellRefs(src, 'js', '4', {}), src);
  assert.equal(sw.rewriteShellRefs(src, 'js', '4', undefined), src);
  const half = sw.rewriteShellRefs(src, 'js', '4', { 'monaco@0': '0.32' });
  assert.ok(half.includes("'ctox:lib/ctox-office@1/a.mjs'"), 'nur gebundene Bibliotheken');
  assert.ok(half.includes("'/~a/lib:monaco@0.32/'"));
});

test('SW: nicht freigegebene Bibliothek, falsche Major-Version und Text ohne Anfuehrung bleiben unveraendert', () => {
  const src = "import x from 'ctox:lib/lexical@0/lexical.mjs';\nimport y from 'ctox:lib/monaco@1/vs/loader.js';\n// siehe ctox:lib/monaco@0/ im Kommentar";
  assert.equal(sw.rewriteShellRefs(src, 'js', '1', { ...BIND, 'lexical@0': '0.1', 'monaco@1': '1.0' }), src);
});

test('SW: Bindung aus der URL-Wurzel – Grammatik, Sortierung, mehrdeutige Bindung', () => {
  assert.deepEqual({ ...sw.parseBinding('notes@1~shell@4~lib:monaco@0.32~lib:ctox-office@1.4') }, BIND);
  assert.deepEqual({ ...sw.parseBinding('notes@1') }, {});
  assert.equal(sw.parseBinding('~lib:monaco@0.32~lib:monaco@0.33'), null);
  assert.equal(sw.bindingSuffix(BIND), '~lib:ctox-office@1.4~lib:monaco@0.32');
  assert.equal(sw.bindingSuffix(BIND, 'ctox-office@1'), '~lib:monaco@0.32');
  // URL_RE: Gruppen 1..4 = app, ver, shell, pfad (das Rahmen-Labor liest das Literal) – mit und ohne Bindung
  const m = sw.URL_RE.exec('/~a/documents@3~shell@4.2~lib:ctox-office@1.4~lib:monaco@0.32/ui/a.js');
  assert.deepEqual([m[1], m[2], m[3], m[4]], ['documents', '3', '4.2', 'ui/a.js']);
  const u = sw.URL_RE.exec('/~a/notes@1.0.0/index.js');
  assert.deepEqual([u[1], u[2], u[3], u[4]], ['notes', '1.0.0', undefined, 'index.js']);
  assert.equal(sw.URL_RE.exec('/~a/notes@1~lib:monaco@0/x.js'), null, 'Bindung ohne Minor ist keine gueltige Wurzel');
  const l = sw.LIB_URL_RE.exec('/~a/lib:monaco@0.32~lib:ctox-office@1.4/vs/loader.js');
  assert.deepEqual([l[1], l[2], l[3], l[4]], ['monaco', '0', '32', 'vs/loader.js']);
  assert.equal(sw.LIB_URL_RE.exec('/~a/lib:monaco@0/vs/loader.js'), null, 'Altform ohne Minor');
});

// ---- Blob-Ladeweg (Kernel-IDB-Layout: files:<app>@<ver> = {pfad: sha256}, artifacts.content = Text | Uint8Array;
// Bindung je Mount aus kernel.artifacts.bind = {shellVer, libs: {<name>: <major>.<minor>}})
const sha = (c) => createHash('sha256').update(typeof c === 'string' ? Buffer.from(c, 'utf8') : Buffer.from(c)).digest('hex');
const bind = (libs = {}, shellVer = '9') => ({ appId: 'x', ver: '1', shellVer, libs, root: '', url: () => '' });
// Kernel-Attrappe: Dateilisten je "<id>@<ver>" (meta files:), Artefakte nach sha256 (content wie im Kernel).
// remote = Dateien, die der Kernel erst auf requestLib "holt" (Spur 4 simuliert); asks protokolliert die Anforderungen.
function kernel(files, { remote = {} } = {}) {
  const art = new Map();
  const lists = {};
  const asks = [];
  for (const [key, entries] of Object.entries(files)) {
    lists[key] = {};
    for (const [p, content] of Object.entries(entries)) {
      lists[key][p] = sha(content);
      art.set(lists[key][p], content);
    }
  }
  return {
    asks,
    art,
    lists,
    readFiles: async (id, ver) => lists[`${id}@${ver}`] || null,
    readArtifact: async (h) => art.get(h) ?? null,
    requestLib: async (msg) => {
      asks.push(msg);
      const r = remote[`${msg.lib}@${msg.ver}`];
      if (!r) return { type: 'ctox:lib-abort', reason: 'unbekannt' };
      lists[`lib:${msg.lib}@${msg.ver}`] ||= Object.fromEntries(Object.entries(r.files).map(([p, t]) => [p, sha(t)]));
      const t = r.files[msg.path];
      if (t !== undefined) art.set(sha(t), r.bytesOverride?.[msg.path] ?? t);
      return { type: 'ctox:lib-ready' };
    },
  };
}

test('Blob: ctox:lib-Import zeigt auf das Plattform-Artefakt lib:<name>@<major>.<minor> der Mount-Bindung', async () => {
  const k = kernel({
    'documents@1': { 'index.js': "import { doc } from 'ctox:lib/ctox-office@1/ctox-office-document.mjs'; export default doc;" },
    'lib:ctox-office@1.4': { 'ctox-office-document.mjs': "export { doc } from './runtime/doc.mjs';", 'runtime/doc.mjs': "export const doc = 'office';" },
    'shell@9': { 'lib/ctox-office@1/ctox-office-document.mjs': "export const doc = 'ALTE FORM';" },
  });
  const loader = createBlobLoader(k);
  const url = await loader.entryUrl('documents', '1', 'index.js', bind({ 'ctox-office': '1.4' }));
  const text = await (await fetch(url)).text();
  const libUrl = /from '([^']+)'/.exec(text)[1];
  assert.match(libUrl, /^blob:/);
  const libText = await (await fetch(libUrl)).text();
  const innerUrl = /from '([^']+)'/.exec(libText)[1];
  assert.match(innerUrl, /^blob:/, 'relativer Import innerhalb der Bibliothek');
  assert.equal(await (await fetch(innerUrl)).text(), "export const doc = 'office';");
  assert.equal(k.asks.length, 0, 'alles vorhanden: keine Kernel-Anforderung');
});

test('Blob: Miss (files:-Liste der gebundenen Minor fehlt) -> genau eine Kernel-Anforderung, danach Auslieferung', async () => {
  const k = kernel({
    'spreadsheets@2': { 'index.js': "import { s } from 'ctox:lib/ctox-office@1/ctox-office-spreadsheet.mjs'; export default s;" },
  }, { remote: { 'ctox-office@1.2': { files: { 'ctox-office-spreadsheet.mjs': "export const s = 'sheet';" } } } });
  const loader = createBlobLoader(k);
  const text = await (await fetch(await loader.entryUrl('spreadsheets', '2', 'index.js', bind({ 'ctox-office': '1.2' })))).text();
  const libUrl = /from '([^']+)'/.exec(text)[1];
  assert.equal(await (await fetch(libUrl)).text(), "export const s = 'sheet';");
  assert.deepEqual(k.asks.map((a) => [a.type, a.lib, a.major, a.path, a.ver, a.reason]), [
    ['ctox:lib-miss', 'ctox-office', '1', 'ctox-office-spreadsheet.mjs', '1.2', 'manifest'],
  ]);
});

test('Blob: Datei fehlt -> Anforderung mit sha256; sha256 falsch -> neu anfordern, bleibt falsch -> Fehler', async () => {
  const good = "export const v = 'ok';";
  const B = bind({ monaco: '0.32' });
  const k = kernel({ 'notes@1': { 'index.js': "import { v } from 'ctox:lib/monaco@0/vs/v.js'; export default v;" } });
  k.lists['lib:monaco@0.32'] = { 'vs/v.js': sha(good) };
  let served = 0;
  k.requestLib = async (msg) => { k.asks.push(msg); served += 1; k.art.set(msg.sha, 'export const v = "manipuliert";'); return { type: 'ctox:lib-ready' }; };
  await assert.rejects(createBlobLoader(k).entryUrl('notes', '1', 'index.js', B), /Hash falsch: lib:monaco@0\.32\/vs\/v\.js/);
  assert.deepEqual(k.asks.map((a) => a.reason), ['artifact']);
  assert.equal(k.asks[0].sha, sha(good));
  assert.equal(k.asks[0].ver, '0.32');
  // vorhandene, manipulierte Datei: Grund 'hash', ein weiterer Leseversuch, dann Auslieferung (kein Endlos-Nachladen)
  const k2 = kernel({ 'notes@1': { 'index.js': "import { v } from 'ctox:lib/monaco@0/vs/v.js';" } });
  k2.lists['lib:monaco@0.32'] = { 'vs/v.js': sha(good) };
  k2.art.set(sha(good), 'kaputt');
  k2.requestLib = async (msg) => { k2.asks.push(msg); k2.art.set(msg.sha, good); return { type: 'ctox:lib-ready' }; };
  const url = await createBlobLoader(k2).entryUrl('notes', '1', 'index.js', B);
  assert.match(url, /^blob:/, 'Kernel hat korrekt nachgeladen');
  assert.deepEqual(k2.asks.map((a) => a.reason), ['hash']);
  assert.equal(served, 1);
});

test('Blob: Kernel-Abbruch, nicht freigegebene und nicht gebundene ctox:lib werden abgelehnt', async () => {
  const k = kernel({ 'documents@1': { 'index.js': "import x from 'ctox:lib/ctox-office@1/x.mjs';" } });
  await assert.rejects(createBlobLoader(k).entryUrl('documents', '1', 'index.js', bind({ 'ctox-office': '1.4' })), /Kernel bricht ab: lib:ctox-office@1\/x\.mjs \(unbekannt\)/);
  // ungebunden: kein Rueckgriff auf eine aktive Minor (wie der SW), auch keine Kernel-Anforderung
  await assert.rejects(createBlobLoader(k).entryUrl('documents', '1', 'index.js', bind({})), /nicht gebunden: ctox:lib\/ctox-office@1\/x\.mjs/);
  assert.equal(k.asks.length, 1);
  const loader = createBlobLoader(kernel({ 'notes@1': { 'index.js': "import x from 'ctox:lib/lexical@0/lexical.mjs';" } }));
  await assert.rejects(loader.entryUrl('notes', '1', 'index.js', bind({ lexical: '0.1' })), /nicht freigegeben/);
  const loader2 = createBlobLoader(kernel({ 'notes@1': { 'index.js': "import x from 'ctox:lib/ctox-office@2/x.mjs';" } }));
  await assert.rejects(loader2.entryUrl('notes', '1', 'index.js', bind({ 'ctox-office': '2.0' })), /nicht freigegeben/);
});

test('Blob: Bindung je Mount – Pflicht, eingefroren; neue Minor gilt erst mit neuer Bindung; gleiche Bindung -> dieselbe URL', async () => {
  const k = kernel({
    'documents@1': { 'index.js': "import { v } from 'ctox:lib/ctox-office@1/v.mjs'; import { t } from 'ctox:shared/t.js'; export default v;" },
    'lib:ctox-office@1.4': { 'v.mjs': "export const v = '1.4';" },
    'lib:ctox-office@1.5': { 'v.mjs': "export const v = '1.5';" },
    'shell@9': { 'shared/t.js': "export const t = 's9';" },
    'shell@10': { 'shared/t.js': "export const t = 's10';" },
  });
  const loader = createBlobLoader(k);
  await assert.rejects(loader.entryUrl('documents', '1'), /Bindung fehlt/);
  await assert.rejects(loader.entryUrl('documents', '1', 'index.js', { libs: {} }), /Bindung fehlt/);
  const nth = async (entryUrl, n) => (await fetch([...(await (await fetch(entryUrl)).text()).matchAll(/from '([^']+)'/g)][n][1])).text();
  const b1 = bind({ 'ctox-office': '1.4' });
  const first = await loader.entryUrl('documents', '1', 'index.js', b1);
  const again = await loader.resolver('documents', '1', bind({ 'ctox-office': '1.5' }, '10'))('./index.js');
  assert.equal(await nth(first, 0), "export const v = '1.4';", 'laufende Instanz bleibt auf 1.4');
  assert.equal(await nth(first, 1), "export const t = 's9';", 'und auf ihrer Shell');
  assert.equal(await nth(again, 0), "export const v = '1.5';", 'neuer Mount (neue Bindung) bekommt 1.5');
  assert.equal(await nth(again, 1), "export const t = 's10';");
  assert.equal(await loader.entryUrl('documents', '1', 'index.js', bind({ 'ctox-office': '1.4' })), first, 'gleiche Bindung -> dieselbe URL');
});

test('Blob: Binaerartefakt (Uint8Array) und Text, sha256 ueber die Bytes', async () => {
  const png = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0, 255]);
  const k = kernel({ 'notes@1': { 'index.js': "export const icon = new URL('./icon.png', import.meta.url).href;", 'icon.png': png } });
  const loader = createBlobLoader(k);
  const text = await (await fetch(await loader.entryUrl('notes', '1', 'index.js', bind()))).text();
  const iconUrl = /new URL\('([^']+)'\)/.exec(text)[1];
  const got = new Uint8Array(await (await fetch(iconUrl)).arrayBuffer());
  assert.deepEqual([...got], [...png]);
  k.art.set(sha(png), new Uint8Array([1, 2]));
  await assert.rejects(createBlobLoader(k).entryUrl('notes', '1', 'index.js', bind()), /Hash falsch: notes@1\/icon\.png/);
});
