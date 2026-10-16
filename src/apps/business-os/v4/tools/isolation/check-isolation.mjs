#!/usr/bin/env node
// Shell v4 – App-Isolationspruefung (Strang ISO).
//
// Owner-Vorgabe 15.10.2026: jede App ist ein eigenstaendiges Artefakt (Hot-Plug); die Shell bietet nur
// Standardschnittstellen. Geprueft werden:
//   R1  Eine App laedt nichts ausserhalb ihres eigenen Ordners. Erlaubt: relative Pfade, die im App-Ordner
//       bleiben, 'ctox:shared/…', freigegebene 'ctox:lib/<name>@<major>/…' (standard-libs.json), 'node:*' nur
//       in handlers/. Geprueft: statische/dynamische Importe, export-from, new URL/fetch/Worker/importScripts,
//       Pfad-Literale ('../…', '/vendor/…'), <script>/<link>/src/href in HTML, url()/@import in CSS.
//   R2  Fremd-Collections nur deklariert: foreign_collections-Ops muessen vom Eigentuemer exportiert sein,
//       der Eigentuemer muss die Collection besitzen, Schreib-Evidenz im Code verlangt 'write'.
//   R3  Shell-Code (v4/frame, v4/shell, v4/source, v4/sw.js, v4/blob-loader.js) nennt keine App-ID und
//       keine App-Collection.
//   R4  shared/ enthaelt keine App-Fachlogik: keine App-IDs/App-Collections, keine Importe aus App-Ordnern.
//
// Unterdrueckung einer begruendeten Ausnahme: Kommentar 'isolation-allow: <Grund>' auf derselben oder der
// vorigen Zeile.
//
// Aufruf:
//   node check-isolation.mjs [--root DIR] [--json] [--summary] [--rule R1,R3] [--app ID[,ID]]
//                            [--include-tests] [--include-lab] [--legacy] [--libs FILE]
// Exit 0 = keine Treffer, 1 = Treffer (R1–R4), 2 = Aufruffehler. --legacy meldet zusaetzlich App-IDs in der
// alten Shell (app.js) als 'R3-legacy'; diese Hinweise aendern den Exit-Code nicht.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { lexJs, isTestFile } from '../guards/guard-v4.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const DEFAULT_ROOT = path.resolve(HERE, '../../..');
export const DEFAULT_LIBS = path.join(HERE, 'standard-libs.json');

export const APP_PARENTS = ['modules', 'customer-modules', 'desktop-apps'];
// R3 nennt App-IDs aus modules/* und customer-modules/* (desktop-apps sind Fenster-Apps ohne Manifest).
const ID_PARENTS = ['modules', 'customer-modules'];
export const SHELL_PATHS = ['v4/frame', 'v4/shell', 'v4/source', 'v4/sw.js', 'v4/blob-loader.js'];
export const LEGACY_SHELL_FILES = ['app.js'];
const CODE_EXT = new Set(['.js', '.mjs', '.cjs', '.html', '.htm', '.css']);
const SKIP_DIRS = new Set(['node_modules', '.git']);

export const RULE_TEXT = {
  R1: 'App laedt nur aus dem eigenen Ordner, ctox:shared/ oder freigegebener ctox:lib/',
  R2: 'Fremd-Collections nur ueber deklarierte foreign_collections mit passenden exports',
  R3: 'Shell-Code ohne App-IDs und App-Collections',
  R4: 'shared/ ohne App-Fachlogik (App-IDs, App-Collections, Importe aus Apps)',
  'R3-legacy': 'Altlast: App-ID in der alten Shell (nur Hinweis)',
};

// ------------------------------------------------------------------ Hilfen

const posix = path.posix;
const toPosix = (p) => p.split(path.sep).join('/');
const escRe = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

function listDir(abs) {
  try { return fs.readdirSync(abs, { withFileTypes: true }); } catch { return []; }
}

function walkFiles(root, relStart, { includeLab = false } = {}) {
  const out = [];
  const absStart = path.join(root, relStart);
  let st;
  try { st = fs.statSync(absStart); } catch { return out; }
  if (st.isFile()) return [relStart];
  const stack = [relStart];
  while (stack.length) {
    const rel = stack.pop();
    for (const ent of listDir(path.join(root, rel))) {
      const r = `${rel}/${ent.name}`;
      if (ent.isDirectory()) {
        if (SKIP_DIRS.has(ent.name)) continue;
        if (!includeLab && ent.name === 'lab') continue;
        stack.push(r);
      } else if (ent.isFile() && CODE_EXT.has(posix.extname(ent.name).toLowerCase())) out.push(r);
    }
  }
  return out.sort();
}

function lineStarts(src) {
  const s = [0];
  for (let i = 0; i < src.length; i++) if (src[i] === '\n') s.push(i + 1);
  return s;
}
function lineOf(starts, pos) {
  let lo = 0, hi = starts.length - 1;
  while (lo < hi) { const mid = (lo + hi + 1) >> 1; if (starts[mid] <= pos) lo = mid; else hi = mid - 1; }
  return lo + 1;
}
function allowed(lines, line) {
  const re = /isolation-allow:/;
  return re.test(lines[line - 1] || '') || re.test(lines[line - 2] || '');
}

function readJson(abs) {
  try { return JSON.parse(fs.readFileSync(abs, 'utf8')); } catch { return null; }
}

// ------------------------------------------------------------------ Inventar

export function inventory(root) {
  const apps = []; // { id, dir }
  for (const parent of APP_PARENTS) {
    for (const ent of listDir(path.join(root, parent))) {
      if (ent.isDirectory() && !SKIP_DIRS.has(ent.name)) apps.push({ id: ent.name, dir: `${parent}/${ent.name}`, parent });
    }
  }
  const appIds = new Set(apps.filter((a) => ID_PARENTS.includes(a.parent)).map((a) => a.id));
  // App-Collections: massgeblich ist der Besitz laut v4-Manifest ('collections' einer App). Fuer Apps ohne
  // Manifest gilt ersatzweise collections.schema.json, ausser die Collection gehoert laut Manifesten der
  // Plattform (foreign owner_app ohne App-Manifest, z. B. ctox-platform) oder einer anderen App.
  const manifests = new Map(); // app_id -> { rel, json }
  for (const ent of listDir(path.join(root, 'v4/manifests'))) {
    if (!ent.isFile() || !ent.name.endsWith('.manifest.json')) continue;
    const rel = `v4/manifests/${ent.name}`;
    const json = readJson(path.join(root, rel));
    if (json?.app_id) manifests.set(json.app_id, { rel, json });
  }
  const collections = new Map(); // name -> owner app id
  const platform = new Set();
  for (const [appId, { json }] of manifests) {
    if (appIds.has(appId)) for (const name of Object.keys(json.collections || {})) if (!collections.has(name)) collections.set(name, appId);
    for (const [name, decl] of Object.entries(json.foreign_collections || {})) {
      if (decl?.owner_app && !manifests.has(decl.owner_app)) platform.add(name);
    }
  }
  for (const a of apps) {
    if (manifests.has(a.id)) continue;
    const schema = readJson(path.join(root, a.dir, 'collections.schema.json'));
    for (const name of Object.keys(schema?.collections || {})) {
      if (!collections.has(name) && !platform.has(name)) collections.set(name, a.id);
    }
  }
  const topLevel = new Set(listDir(root).map((e) => e.name));
  return { apps, appIds, collections, platform, manifests, topLevel };
}

export function loadLibs(file = DEFAULT_LIBS) {
  const json = readJson(file);
  const libs = new Map();
  for (const [key, val] of Object.entries(json?.libs || {})) if (val?.status === 'approved') libs.set(key, val);
  return libs;
}

// ------------------------------------------------------------------ R1: Spezifizierer einstufen

function describeTarget(target, inv) {
  const [top, second] = target.split('/');
  if (APP_PARENTS.includes(top) && second) return `fremde App ${second}`;
  if (top === 'shared') return 'Shell-shared per Pfad (statt ctox:shared/)';
  if (top === 'vendor') return 'Vendor ausserhalb der App';
  if (top === 'office-engine') return 'office-engine ausserhalb der App';
  if (inv.topLevel.has(top)) return `Shell-Baum ${top}/`;
  return 'ausserhalb des App-Ordners';
}

function stripQuery(spec) {
  return spec.replace(/[?#].*$/, '');
}

// Liefert null (in Ordnung) oder { why, target }.
export function classifySpecifier(spec, { kind, rel, appDir, appId, inv, libs, dynamic = false }) {
  const isImport = kind === 'import';
  const inHandlers = rel.startsWith(`${appDir}/handlers/`);
  if (isImport && !dynamic) {
    if (spec.startsWith('ctox:shared/')) return null;
    if (spec.startsWith('ctox:lib/')) {
      const m = /^ctox:lib\/([a-z0-9._-]+@\d+)\//.exec(spec);
      if (m && libs.has(m[1])) return null;
      return { why: `ctox:lib nicht freigegeben (${m ? m[1] : 'ohne <name>@<major>/'})`, target: spec };
    }
    if (spec.startsWith('ctox:')) return { why: 'unbekanntes ctox:-Schema', target: spec };
    if (spec.startsWith('node:')) return inHandlers ? null : { why: 'node:-Import ausserhalb handlers/', target: spec };
    if (/^(https?:)?\/\//.test(spec)) return { why: 'externer Import', target: spec };
    if (/^(data|blob):/.test(spec)) return null;
    if (!spec.startsWith('.') && !spec.startsWith('/')) return { why: 'nackter Spezifizierer (nicht aufloesbar ohne Import-Map)', target: spec };
  }
  const clean = stripQuery(spec);
  if (clean.startsWith('./') || clean.startsWith('../') || ((isImport || kind === 'url') && clean.startsWith('.'))) {
    const target = posix.normalize(posix.join(posix.dirname(rel), clean || '.'));
    if (target === appDir || target.startsWith(`${appDir}/`)) return null;
    const what = describeTarget(target, inv).replace(/ __dyn__$/, ' (per Ausdruck)');
    return { why: dynamic ? `dynamischer Pfad: ${what}` : what, target: target.replace(/__dyn__$/, '…') };
  }
  if (clean.startsWith('/') && !clean.startsWith('//')) {
    let segs = clean.slice(1).split('/');
    if (segs[0] === 'business-os') segs = segs.slice(1);
    const top = segs[0] || '';
    if (top === '~a') {
      const owner = (segs[1] || '').split('@')[0];
      if (owner === appId) return null;
      return { why: owner ? `Artefakt-URL einer fremden App/Shell (${owner})` : 'Artefakt-URL', target: clean };
    }
    if (segs.length > 1 && inv.topLevel.has(top) && (isImport || kind === 'url' || /\.[a-z0-9]+$|\/$/i.test(clean))) {
      const target = segs.join('/');
      return { why: `absoluter Pfad: ${describeTarget(target, inv)}`, target };
    }
    if (isImport) return { why: 'absoluter Import-Pfad', target: clean };
  }
  return null;
}

// ------------------------------------------------------------------ R1: Dateien lesen

const CTX_RES = [
  { kind: 'import', re: /\bfrom\s*$/ },
  { kind: 'import', re: /(?:^|[^.\w$])import\s*$/ },
  { kind: 'import', re: /(?:^|[^.\w$])import\s*\(\s*$/, dyn: true },
  { kind: 'url', re: /\bnew\s+URL\s*\(\s*$/ },
  { kind: 'url', re: /\bfetch\s*\(\s*$/ },
  { kind: 'url', re: /\bnew\s+(?:Shared)?Worker\s*\(\s*$/ },
  { kind: 'url', re: /\bimportScripts\s*\(\s*$/ },
];

function jsSpecifiers(src, offset = 0) {
  const { code, strings } = lexJs(src);
  const out = [];
  for (const s of strings) {
    const open = src[s.start - 1];
    if (open === '}') continue; // Fortsetzung eines Template-Literals
    const content = src.slice(s.start, s.end);
    if (!content || content.length > 400) continue;
    const isTemplate = open === '`';
    const hasExpr = isTemplate && src[s.end] === '$';
    const before = code.slice(Math.max(0, s.start - 1 - 60), s.start - 1);
    let kind = 'literal';
    let dynImport = false;
    for (const c of CTX_RES) if (c.re.test(before)) { kind = c.kind; dynImport = !!c.dyn; break; }
    out.push({ spec: content, kind, dynImport, hasExpr, pos: offset + s.start - 1 });
  }
  return out;
}

function htmlSpecifiers(src) {
  const masked = src.replace(/<!--[\s\S]*?(?:-->|$)/g, (m) => m.replace(/[^\n]/g, ' '));
  const out = [];
  const tagRe = /<\s*([a-zA-Z][\w-]*)\b([^>]*)>/g;
  let m;
  while ((m = tagRe.exec(masked))) {
    const tag = m[1].toLowerCase();
    const attrRe = /\b(src|href)\s*=\s*(["'])([^"']*)\2/gi;
    let a;
    while ((a = attrRe.exec(m[2]))) {
      const isLoad = tag === 'script' || tag === 'link';
      out.push({ spec: a[3], kind: isLoad ? 'url' : 'literal', hasExpr: false, pos: m.index });
    }
    if (tag === 'script' && !/\bsrc\s*=/.test(m[2])) {
      const start = m.index + m[0].length;
      const close = masked.indexOf('</script', start);
      const body = src.slice(start, close < 0 ? src.length : close);
      out.push(...jsSpecifiers(body, start));
    }
  }
  return out;
}

function cssSpecifiers(src) {
  const masked = src.replace(/\/\*[\s\S]*?(?:\*\/|$)/g, (m) => m.replace(/[^\n]/g, ' '));
  const out = [];
  const re = /\burl\(\s*(["']?)([^"')]+)\1\s*\)|@import\s+(["'])([^"']+)\3/g;
  let m;
  while ((m = re.exec(masked))) out.push({ spec: (m[2] || m[4]).trim(), kind: 'url', hasExpr: false, pos: m.index });
  return out;
}

function specifiersOf(rel, src) {
  const ext = posix.extname(rel).toLowerCase();
  if (ext === '.html' || ext === '.htm') return htmlSpecifiers(src);
  if (ext === '.css') return cssSpecifiers(src);
  return jsSpecifiers(src);
}

function scanAppFile(rel, src, app, inv, libs) {
  const hits = [];
  const starts = lineStarts(src);
  const lines = src.split('\n');
  // Fremdcode der App (<app>/vendor/**, Strang VENDOR): gebuendelte Bibliotheken tragen Pfad-Literale aus dem
  // Build (esbuild-__commonJS-Schluessel) und Build-Quellen nackte Spezifizierer. Gezaehlt werden dort nur echte
  // Lade-Spezifizierer (import/export/import()/new URL/fetch), die den App-Ordner verlassen oder absolut sind.
  const inVendor = rel.startsWith(`${app.dir}/vendor/`);
  for (const sp of specifiersOf(rel, src)) {
    let spec = sp.spec.trim();
    let dynamic = false;
    if (inVendor && (sp.kind === 'literal' || !/^[./]/.test(spec))) continue;
    if (sp.hasExpr) {
      // Nur der statische Praefix zaehlt; '../' + Ausdruck verlaesst den Ordner der Datei.
      if (!/^\.\.?\//.test(spec)) continue;
      spec = `${spec}__dyn__`;
      dynamic = true;
    }
    if (sp.kind === 'literal' && !/^(?:(?:\.\.\/)+[A-Za-z0-9_.~$-]|\/[A-Za-z~])/.test(spec)) continue;
    if (sp.kind === 'literal' && /\s/.test(spec)) continue;
    const kind = sp.kind === 'import' ? 'import' : sp.kind;
    const res = classifySpecifier(spec, { kind, rel, appDir: app.dir, appId: app.id, inv, libs, dynamic: dynamic || (sp.dynImport && sp.hasExpr) });
    if (!res) continue;
    const line = lineOf(starts, sp.pos);
    if (allowed(lines, line)) continue;
    hits.push({ rule: 'R1', app: app.id, file: rel, line, spec: sp.hasExpr ? `${sp.spec}\${…}` : sp.spec, context: sp.kind, target: res.target.replace(/__dyn__/, '…'), message: res.why });
  }
  return hits;
}

// ------------------------------------------------------------------ R3/R4: App-Bezuege in Strings

// App-IDs, die zugleich Plattformbegriffe sind ('ctox' = Produkt/Akteur, 'desktop' = Oberflaeche, 'browser' =
// Laufzeit): in shared/ (R4) nur als Pfad-/Artefaktbezug gewertet, nicht als nackter String. R3 bleibt streng.
export const AMBIGUOUS_IDS = new Set(['ctox', 'desktop', 'browser']);

export function buildMatcher(inv, { lenient = false } = {}) {
  const ids = [...inv.appIds].sort((a, b) => b.length - a.length);
  const cols = [...inv.collections.keys()].sort((a, b) => b.length - a.length);
  // Pfad-/Artefakt-Bezuege: modules/<id>, ~a/<id>@…, <id>@<version>, '<id>/…' am Anfang, data-app="<id>".
  const idPath = ids.map((id) => {
    const e = escRe(id);
    const end = '(?![A-Za-z0-9_-])';
    return { id, re: new RegExp(`(?:^|/)(?:modules|customer-modules|desktop-apps|~a|apps?)/${e}${end}|(?:^|[^A-Za-z0-9_.-])${e}@(?=[0-9<$])|^${e}/|data-app\\s*=\\s*["']?${e}${end}`) };
  });
  const colRe = cols.filter((c) => c.includes('_')).map((c) => ({ c, re: new RegExp(`(?:^|[^A-Za-z0-9_])${escRe(c)}(?![A-Za-z0-9_])`) }));
  return function match(str) {
    const t = str.trim();
    const found = [];
    if (inv.appIds.has(t) && !(lenient && AMBIGUOUS_IDS.has(t))) found.push({ kind: 'App-ID', name: t });
    else if (inv.collections.has(t)) found.push({ kind: 'App-Collection', name: t, owner: inv.collections.get(t) });
    else {
      for (const { id, re } of idPath) if (re.test(str)) { found.push({ kind: 'App-ID', name: id }); break; }
      for (const { c, re } of colRe) if (re.test(str)) { found.push({ kind: 'App-Collection', name: c, owner: inv.collections.get(c) }); break; }
    }
    return found;
  };
}

function stringSegments(rel, src) {
  const ext = posix.extname(rel).toLowerCase();
  if (ext === '.css') {
    const masked = src.replace(/\/\*[\s\S]*?(?:\*\/|$)/g, (m) => m.replace(/[^\n]/g, ' '));
    const out = [];
    const re = /(["'])([^"'\n]*)\1/g;
    let m;
    while ((m = re.exec(masked))) out.push({ text: m[2], pos: m.index });
    return out;
  }
  if (ext === '.html' || ext === '.htm') {
    const out = [];
    const re = /(["'])([^"'\n]*)\1/g;
    const masked = src.replace(/<!--[\s\S]*?(?:-->|$)/g, (m) => m.replace(/[^\n]/g, ' '));
    let m;
    while ((m = re.exec(masked))) out.push({ text: m[2], pos: m.index });
    return out;
  }
  const { strings } = lexJs(src);
  return strings.map((s) => ({ text: src.slice(s.start, s.end), pos: s.start }));
}

function scanRefs(rel, src, rule, match, { skipRelative = false } = {}) {
  const hits = [];
  const starts = lineStarts(src);
  const lines = src.split('\n');
  for (const seg of stringSegments(rel, src)) {
    if (!seg.text || seg.text.length > 300) continue;
    if (skipRelative && /^\.\.?\//.test(seg.text)) continue; // relative Pfade prueft scanSharedImports
    for (const f of match(seg.text)) {
      const line = lineOf(starts, seg.pos);
      if (allowed(lines, line)) continue;
      hits.push({ rule, file: rel, line, spec: seg.text.length > 80 ? `${seg.text.slice(0, 77)}…` : seg.text, message: `${f.kind} ${f.name}${f.owner ? ` (App ${f.owner})` : ''}` });
    }
  }
  return hits;
}

function scanSharedImports(rel, src, inv) {
  const hits = [];
  const starts = lineStarts(src);
  const lines = src.split('\n');
  for (const sp of specifiersOf(rel, src)) {
    if (sp.kind === 'literal' || sp.hasExpr) continue;
    const clean = stripQuery(sp.spec.trim());
    if (!clean.startsWith('.')) continue;
    const target = posix.normalize(posix.join(posix.dirname(rel), clean));
    const top = target.split('/')[0];
    if (!APP_PARENTS.includes(top)) continue;
    const line = lineOf(starts, sp.pos);
    if (allowed(lines, line)) continue;
    hits.push({ rule: 'R4', file: rel, line, spec: sp.spec, target, message: `shared/ laedt aus App-Ordner (${describeTarget(target, inv)})` });
  }
  return hits;
}

// ------------------------------------------------------------------ R2: Manifeste

function jsonLine(text, needle, from = 0) {
  const at = text.indexOf(needle, from);
  if (at < 0) return 1;
  return text.slice(0, at).split('\n').length;
}

export function scanManifests(root, inv) {
  const hits = [];
  for (const [appId, { rel, json }] of inv.manifests) {
    const foreign = json.foreign_collections || {};
    const text = fs.readFileSync(path.join(root, rel), 'utf8');
    const fcAt = text.indexOf('"foreign_collections"');
    for (const [name, decl] of Object.entries(foreign)) {
      const line = jsonLine(text, `"${name}"`, Math.max(0, fcAt));
      const ops = Array.isArray(decl?.ops) ? decl.ops : [];
      const ownerId = decl?.owner_app;
      const writes = decl?._evidence?.code_write || [];
      if (writes.length && !ops.includes('write')) {
        hits.push({ rule: 'R2', app: appId, file: rel, line, spec: name, message: `Code schreibt (${writes.slice(0, 3).join(', ')}), ops ohne 'write'` });
      }
      const owner = ownerId && inv.manifests.get(ownerId);
      if (!owner) continue; // Plattform-/Core-Collections (ctox-platform) stehen nicht in App-Manifesten
      if (ownerId === appId) {
        hits.push({ rule: 'R2', app: appId, file: rel, line, spec: name, message: 'foreign_collections nennt die eigene App als Eigentuemer' });
        continue;
      }
      if (!Object.prototype.hasOwnProperty.call(owner.json.collections || {}, name)) {
        hits.push({ rule: 'R2', app: appId, file: rel, line, spec: name, message: `Eigentuemer ${ownerId} besitzt die Collection nicht` });
        continue;
      }
      const exported = (owner.json.exports || {})[name] || [];
      const missing = ops.filter((op) => !exported.includes(op));
      if (missing.length) {
        hits.push({ rule: 'R2', app: appId, file: rel, line, spec: name, message: `${ownerId} exportiert ${missing.join('+')} nicht (exports: ${exported.length ? exported.join('+') : 'keine'})` });
      }
    }
  }
  return hits;
}

// ------------------------------------------------------------------ Gesamtlauf

export function scan({ root = DEFAULT_ROOT, libsFile = DEFAULT_LIBS, includeTests = false, includeLab = false, legacy = false } = {}) {
  const inv = inventory(root);
  const libs = loadLibs(libsFile);
  const match = buildMatcher(inv);
  const matchShared = buildMatcher(inv, { lenient: true });
  const hits = [];
  const stats = { appFiles: 0, shellFiles: 0, sharedFiles: 0, manifests: inv.manifests.size, apps: inv.apps.length, appIds: inv.appIds.size, collections: inv.collections.size };
  const keep = (rel) => includeTests || !isTestFile(rel);

  for (const app of inv.apps) {
    for (const rel of walkFiles(root, app.dir, { includeLab: true })) {
      if (!keep(rel)) continue;
      stats.appFiles++;
      hits.push(...scanAppFile(rel, fs.readFileSync(path.join(root, rel), 'utf8'), app, inv, libs));
    }
  }
  hits.push(...scanManifests(root, inv));
  for (const start of SHELL_PATHS) {
    for (const rel of walkFiles(root, start, { includeLab })) {
      if (!keep(rel)) continue;
      stats.shellFiles++;
      hits.push(...scanRefs(rel, fs.readFileSync(path.join(root, rel), 'utf8'), 'R3', match));
    }
  }
  for (const rel of walkFiles(root, 'shared', { includeLab })) {
    if (!keep(rel)) continue;
    stats.sharedFiles++;
    const src = fs.readFileSync(path.join(root, rel), 'utf8');
    hits.push(...scanRefs(rel, src, 'R4', matchShared, { skipRelative: true }), ...scanSharedImports(rel, src, inv));
  }
  if (legacy) {
    for (const rel of LEGACY_SHELL_FILES) {
      const abs = path.join(root, rel);
      if (fs.existsSync(abs)) hits.push(...scanRefs(rel, fs.readFileSync(abs, 'utf8'), 'R3-legacy', match));
    }
  }
  hits.sort((a, b) => a.rule.localeCompare(b.rule) || a.file.localeCompare(b.file) || a.line - b.line);
  return { root, hits, stats };
}

export function countByRule(hits) {
  const out = {};
  for (const h of hits) out[h.rule] = (out[h.rule] || 0) + 1;
  return out;
}

export function failing(hits) {
  return hits.filter((h) => h.rule !== 'R3-legacy');
}

export function formatHuman(res, { summary = false } = {}) {
  const lines = [];
  if (!summary) {
    for (const h of res.hits) lines.push(`${h.file}:${h.line}  ${h.rule}  ${h.message}  [${h.spec}]`);
    if (res.hits.length) lines.push('');
  }
  const counts = countByRule(res.hits);
  lines.push(`Isolation: ${res.stats.appFiles} App-Dateien, ${res.stats.shellFiles} Shell-Dateien, ${res.stats.sharedFiles} shared-Dateien, ${res.stats.manifests} Manifeste`);
  for (const rule of Object.keys(RULE_TEXT)) if (counts[rule]) lines.push(`  ${rule.padEnd(9)} ${String(counts[rule]).padStart(4)}  ${RULE_TEXT[rule]}`);
  lines.push(failing(res.hits).length ? `FEHLER: ${failing(res.hits).length} Treffer` : 'OK: keine Treffer');
  return lines.join('\n');
}

function parseArgs(argv) {
  const o = { root: DEFAULT_ROOT, libsFile: DEFAULT_LIBS, json: false, summary: false, rules: null, apps: null, includeTests: false, includeLab: false, legacy: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--root') o.root = path.resolve(argv[++i]);
    else if (a === '--libs') o.libsFile = path.resolve(argv[++i]);
    else if (a === '--json') o.json = true;
    else if (a === '--summary') o.summary = true;
    else if (a === '--include-tests') o.includeTests = true;
    else if (a === '--include-lab') o.includeLab = true;
    else if (a === '--legacy') o.legacy = true;
    else if (a === '--rule') o.rules = new Set(argv[++i].split(','));
    else if (a === '--app') o.apps = new Set(argv[++i].split(','));
    else if (a === '-h' || a === '--help') o.help = true;
    else throw new Error(`unbekanntes Argument: ${a}`);
  }
  return o;
}

export function main(argv = process.argv.slice(2)) {
  let o;
  try { o = parseArgs(argv); } catch (e) { console.error(e.message); return 2; }
  if (o.help) { console.log(fs.readFileSync(fileURLToPath(import.meta.url), 'utf8').split('\n').slice(1, 24).map((l) => l.replace(/^\/\/ ?/, '')).join('\n')); return 0; }
  const res = scan(o);
  if (o.rules) res.hits = res.hits.filter((h) => o.rules.has(h.rule));
  if (o.apps) res.hits = res.hits.filter((h) => h.app && o.apps.has(h.app));
  if (o.json) console.log(JSON.stringify({ stats: res.stats, counts: countByRule(res.hits), hits: res.hits }, null, 2));
  else console.log(formatHuman(res, o));
  return failing(res.hits).length ? 1 : 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) process.exitCode = main();
