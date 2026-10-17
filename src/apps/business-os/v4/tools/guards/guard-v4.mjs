#!/usr/bin/env node
// Shell v4 – CI-Waechter fuer die Loeschliste (Entwurf §7, §11, §12 Punkt 8).
//
// Zaehlt in src/apps/business-os/** (ohne v4/tools, node_modules, vendor, <app>/vendor, rxdb/) jeden
// Treffer einer Loeschlisten-Regel und jedes Zustandsvokabular ausserhalb von
// api.status.report(...). Heute Zaehlmodus (Exit 0); Exit 1 nur bei Treffern in Pfaden,
// die in guards/ported.json als portiert eingetragen sind.
//
// Aufruf:
//   node guard-v4.mjs [--root DIR] [--json] [--summary] [--include-tests]
//                     [--ported FILE] [--rule ID[,ID]] [--app GROUP[,GROUP]]
//
// Keine Abhaengigkeiten ausser Node >= 18.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { englishRawTerms, rawKey } from '../../frame/glossary.js';

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const DEFAULT_ROOT = path.resolve(HERE, '../../..');
export const DEFAULT_PORTED = path.join(HERE, 'ported.json');
export const DEFAULT_VOCABULARY = path.join(HERE, 'vocabulary.json');

// ---------------------------------------------------------------- Dateiauswahl

const EXCLUDED_DIRS = new Set(['node_modules', '.git']);
// kernel/: Kern-Code der App-API (Status- und Vertragswerte, keine UI-Texte) gehört dem Kern-Strang – Bereichsgrenze
// nach Kern-Entscheidung E31.4 (16.10.2026), keine Baseline-Erhöhung.
const EXCLUDED_TOP = new Set(['vendor', 'rxdb', 'kernel']);
const EXCLUDED_PREFIXES = ['v4/tools/'];
// Fremdcode einer App (Vendor-Bibliotheken im App-Ordner, REPORT.md Strang ISO): wird nicht gelesen.
export const APP_VENDOR_DIR = /^(?:modules|desktop-apps|customer-modules)\/[^/]+\/vendor$/;
const CODE_EXT = new Set(['.js', '.mjs', '.cjs', '.ts', '.html', '.htm', '.css', '.json']);
const EXISTENCE_FILES = [
  { rule: 'file-registry-json', test: (rel) => rel === 'modules/registry.json' || /(^|\/)registry\.json$/.test(rel) },
  { rule: 'file-mobile-apps-json', test: (rel) => /(^|\/)mobile-apps\.json$/.test(rel) },
  { rule: 'file-system-apps-json', test: (rel) => /(^|\/)system-apps\.json$/.test(rel) },
  { rule: 'file-schema-js', test: (rel) => /(^|\/)schema\.js$/.test(rel) },
];

// Build-, QA- und Diagnose-Skripte: werden mitgezaehlt, aber als 'tooling' ausgewiesen.
const TOOLING_GROUPS = new Set(['scripts', 'output', 'qa']);

// Messausgaben der v4-Labore (v4/**/lab/**/results*.json): von Messskripten geschriebene Protokolle (Testnamen,
// Playwright-Aufruflogs, zitierte Zonentexte), nie ausgeliefert und nie gerendert. Zaehlen wie Testdateien getrennt.
export const LAB_RESULTS = /^v4\/(?:[^/]+\/)*lab\/(?:[^/]+\/)*results[^/]*\.json$/;

export function isTestFile(rel) {
  const base = path.posix.basename(rel).toLowerCase();
  if (LAB_RESULTS.test(rel)) return true;
  if (/(^|\/)(tests?|__tests__|fixtures?)\//.test(rel)) return true;
  return base.includes('test') || base.includes('smoke');
}

export function groupOf(rel) {
  const parts = rel.split('/');
  if (parts.length === 1) return `shell/${parts[0]}`;
  const [top, second] = parts;
  if (top === 'modules') return parts.length === 2 ? `modules/${second}` : `modules/${second}`;
  if (top === 'desktop-apps' || top === 'customer-modules') return `${top}/${second}`;
  return top;
}

export function kindOf(group) {
  if (group === 'modules/registry.json') return 'catalog';
  if (/^(modules|desktop-apps|customer-modules)\//.test(group)) return 'app';
  if (group.startsWith('shell/')) return 'shell';
  if (group === 'shared') return 'shared';
  if (TOOLING_GROUPS.has(group)) return 'tooling';
  return 'other';
}

function walk(root) {
  const out = [];
  const stack = [''];
  while (stack.length) {
    const relDir = stack.pop();
    const abs = path.join(root, relDir);
    let entries;
    try { entries = fs.readdirSync(abs, { withFileTypes: true }); } catch { continue; }
    for (const ent of entries) {
      const rel = relDir ? `${relDir}/${ent.name}` : ent.name;
      if (ent.isDirectory()) {
        if (EXCLUDED_DIRS.has(ent.name)) continue;
        if (!relDir && EXCLUDED_TOP.has(ent.name)) continue;
        if (APP_VENDOR_DIR.test(rel)) continue;
        if (EXCLUDED_PREFIXES.some((p) => `${rel}/`.startsWith(p))) continue;
        stack.push(rel);
      } else if (ent.isFile()) {
        if (EXCLUDED_PREFIXES.some((p) => rel.startsWith(p))) continue;
        out.push(rel);
      }
    }
  }
  return out.sort();
}

// ---------------------------------------------------------------- Lexer
// Liefert drei gleich lange Sichten auf den Quelltext (Zeilenumbrueche bleiben erhalten):
//   code     – Kommentare geleert, Strings erhalten
//   noStr    – Kommentare und String-/Template-/Regex-Inhalte geleert
//   strings  – Liste der String-Segmente {start, end} (Template-Texte je Abschnitt)

const REGEX_PREV_WORDS = new Set(['return', 'typeof', 'case', 'do', 'else', 'in', 'of', 'new', 'delete', 'void', 'throw', 'yield', 'await', 'instanceof']);
const REGEX_PREV_CHARS = new Set([...'(,=:[!&|?{};+-*%<>~^']);

function blank(arr, a, b) {
  for (let k = a; k < b; k++) if (arr[k] !== '\n' && arr[k] !== '\r') arr[k] = ' ';
}

export function lexJs(src) {
  const n = src.length;
  const code = src.split('');
  const noStr = src.split('');
  const strings = [];
  const tplStack = [];
  let depth = 0;
  let prevChar = '';
  let prevWord = '';
  let i = 0;

  const readTemplate = (j) => {
    const segStart = j;
    while (j < n) {
      const ch = src[j];
      if (ch === '\\') { j += 2; continue; }
      if (ch === '`') {
        strings.push({ start: segStart, end: j });
        blank(noStr, segStart, j);
        return j + 1;
      }
      if (ch === '$' && src[j + 1] === '{') {
        strings.push({ start: segStart, end: j });
        blank(noStr, segStart, j);
        tplStack.push(depth);
        depth++;
        return j + 2;
      }
      j++;
    }
    strings.push({ start: segStart, end: n });
    blank(noStr, segStart, n);
    return n;
  };

  while (i < n) {
    const c = src[i];
    const d = src[i + 1];
    if (c === '/' && d === '/') {
      let j = i;
      while (j < n && src[j] !== '\n') j++;
      blank(code, i, j); blank(noStr, i, j);
      i = j; continue;
    }
    if (c === '/' && d === '*') {
      let j = src.indexOf('*/', i + 2);
      j = j < 0 ? n : j + 2;
      blank(code, i, j); blank(noStr, i, j);
      i = j; continue;
    }
    if (c === '"' || c === "'") {
      let j = i + 1;
      while (j < n && src[j] !== c && src[j] !== '\n') { if (src[j] === '\\') j++; j++; }
      strings.push({ start: i + 1, end: Math.min(j, n) });
      blank(noStr, i + 1, Math.min(j, n));
      i = j + 1; prevChar = 'a'; prevWord = ''; continue;
    }
    if (c === '`') {
      i = readTemplate(i + 1); prevChar = 'a'; prevWord = ''; continue;
    }
    if (c === '{') { depth++; prevChar = c; prevWord = ''; i++; continue; }
    if (c === '}') {
      depth--;
      if (tplStack.length && tplStack[tplStack.length - 1] === depth) {
        tplStack.pop();
        i = readTemplate(i + 1); prevChar = 'a'; prevWord = ''; continue;
      }
      prevChar = c; prevWord = ''; i++; continue;
    }
    if (c === '/') {
      const isRegex = prevChar === '' || REGEX_PREV_CHARS.has(prevChar) || REGEX_PREV_WORDS.has(prevWord);
      if (isRegex) {
        let j = i + 1;
        let inClass = false;
        while (j < n && src[j] !== '\n') {
          const ch = src[j];
          if (ch === '\\') { j += 2; continue; }
          if (ch === '[') inClass = true;
          else if (ch === ']') inClass = false;
          else if (ch === '/' && !inClass) break;
          j++;
        }
        blank(noStr, i + 1, Math.min(j, n));
        j++;
        while (j < n && /[a-z]/i.test(src[j])) j++;
        i = j; prevChar = 'a'; prevWord = ''; continue;
      }
      prevChar = c; prevWord = ''; i++; continue;
    }
    if (/\s/.test(c)) { i++; continue; }
    if (/[A-Za-z0-9_$]/.test(c)) {
      let j = i;
      while (j < n && /[A-Za-z0-9_$]/.test(src[j])) j++;
      prevWord = src.slice(i, j);
      prevChar = 'a';
      i = j; continue;
    }
    prevChar = c; prevWord = ''; i++;
  }
  return { code: code.join(''), noStr: noStr.join(''), strings };
}

export function lexCss(src) {
  const code = src.split('');
  const re = /\/\*[\s\S]*?(?:\*\/|$)/g;
  let m;
  while ((m = re.exec(src))) blank(code, m.index, m.index + m[0].length);
  const joined = code.join('');
  return { code: joined, noStr: ' '.repeat(0) + joined.replace(/[^\n\r]/g, ' '), strings: [] };
}

export function lexHtml(src) {
  const n = src.length;
  const code = src.split('');
  const noStr = src.replace(/[^\n\r]/g, ' ').split('');
  const strings = [];
  const commentRe = /<!--[\s\S]*?(?:-->|$)/g;
  let m;
  while ((m = commentRe.exec(src))) blank(code, m.index, m.index + m[0].length);
  const masked = code.join('');
  let i = 0;
  while (i < n) {
    const lt = masked.indexOf('<', i);
    const textEnd = lt < 0 ? n : lt;
    if (textEnd > i && masked.slice(i, textEnd).trim()) strings.push({ start: i, end: textEnd });
    if (lt < 0) break;
    const gt = findTagEnd(masked, lt);
    const tag = masked.slice(lt, gt);
    const attrRe = /=\s*(["'])([\s\S]*?)\1/g;
    let a;
    while ((a = attrRe.exec(tag))) {
      const start = lt + a.index + a[0].indexOf(a[1]) + 1;
      strings.push({ start, end: start + a[2].length });
    }
    i = gt;
    const nameMatch = /^<\s*(script|style)\b/i.exec(tag);
    if (nameMatch && !/\/\s*>$/.test(tag)) {
      const closeRe = new RegExp(`<\\/\\s*${nameMatch[1]}\\s*>`, 'ig');
      closeRe.lastIndex = gt;
      const close = closeRe.exec(masked);
      const bodyEnd = close ? close.index : n;
      const body = masked.slice(gt, bodyEnd);
      const sub = nameMatch[1].toLowerCase() === 'script' ? lexJs(body) : lexCss(body);
      for (let k = 0; k < body.length; k++) { code[gt + k] = sub.code[k]; noStr[gt + k] = sub.noStr[k]; }
      for (const s of sub.strings) strings.push({ start: gt + s.start, end: gt + s.end });
      i = close ? close.index : n;
    }
  }
  return { code: code.join(''), noStr: noStr.join(''), strings };
}

function findTagEnd(s, lt) {
  let j = lt + 1;
  let q = '';
  while (j < s.length) {
    const ch = s[j];
    if (q) { if (ch === q) q = ''; }
    else if (ch === '"' || ch === "'") q = ch;
    else if (ch === '>') return j + 1;
    j++;
  }
  return s.length;
}

export function lexJson(src) {
  const lexed = lexJs(src);
  // Schluessel (String gefolgt von ':') fallen aus den Vokabel-Segmenten; Werte unter
  // Schluesseln "status.*" / "status_*" gelten als Texte fuer api.status.report.
  const values = [];
  let lastKey = '';
  for (const s of lexed.strings) {
    let j = s.end + 1;
    while (j < src.length && /\s/.test(src[j])) j++;
    if (src[j] === ':') { lastKey = src.slice(s.start, s.end); continue; }
    if (/^status[._]/i.test(lastKey)) continue;
    values.push(s);
  }
  return { ...lexed, strings: values };
}

export function lexFile(rel, src) {
  const ext = path.posix.extname(rel).toLowerCase();
  if (ext === '.html' || ext === '.htm') return lexHtml(src);
  if (ext === '.css') return lexCss(src);
  if (ext === '.json') return lexJson(src);
  return lexJs(src);
}

// ---------------------------------------------------------------- Regeln

// Bezeichner-Regeln (§11): geprueft auf Bezeichner-Token ausserhalb von Strings und
// Kommentaren. 'contains' = Token enthaelt den Namen an einer camelCase-Grenze (Familien wie
// subscribeCollectionReadiness, renderAdvancedStatus; nicht releaseModule fuer leaseModule,
// nicht restartCollection fuer startCollection); laengster Name gewinnt. 'exact' = Token ist
// genau der Name.
export const IDENTIFIER_RULES = [
  { id: 'readNativeCollectionDocument', category: 'sync-api', terms: ['readNativeCollectionDocument'] },
  { id: 'readCollectionNativeDocument', category: 'sync-api', terms: ['readCollectionNativeDocument'] },
  { id: 'startMaintenanceMonitor', category: 'sync-api', terms: ['startMaintenanceMonitor'] },
  { id: 'waitForDataPlaneReady', category: 'sync-api', terms: ['waitForDataPlaneReady'] },
  { id: 'activeModuleSyncLease', category: 'sync-api', terms: ['activeModuleSyncLease'] },
  { id: 'localReplicaComplete', category: 'sync-api', terms: ['localReplicaComplete'] },
  { id: 'collectionReadiness', category: 'sync-api', terms: ['collectionReadiness'] },
  { id: 'restartCollection', category: 'sync-api', terms: ['restartCollection'] },
  { id: 'startCollection', category: 'sync-api', terms: ['startCollection'] },
  { id: 'stopCollection', category: 'sync-api', terms: ['stopCollection'] },
  { id: 'leaseCollection', category: 'sync-api', terms: ['leaseCollection'] },
  { id: 'leaseModule', category: 'sync-api', terms: ['leaseModule'] },
  { id: 'requireRevision', category: 'sync-api', terms: ['requireRevision'] },
  // exakt: requestNativeProviders u. a. gehoeren zur Workjet-Hostbruecke, nicht zum Sync-RPC.
  { id: 'requestNative', category: 'sync-api', terms: ['requestNative', 'requestNativeViaLeader', 'requestNativeDirectly'], match: 'exact' },
  { id: 'dataPlaneReady', category: 'sync-api', terms: ['dataPlaneReady'] },
  { id: 'advancedStatus', category: 'sync-api', terms: ['advancedStatus'] },
  { id: 'recoveryWarning', category: 'sync-api', terms: ['recoveryWarning'] },
  { id: 'syncDiagnostics', category: 'sync-api', terms: ['syncDiagnostics'] },
  { id: 'ctoxHealthTimer', category: 'sync-api', terms: ['ctoxHealthTimer'] },
  { id: 'commandBus', category: 'command-bus', terms: ['commandBus'] },
  { id: 'OFFLINE_FALLBACK_CATALOG', category: 'catalog', terms: ['OFFLINE_FALLBACK_CATALOG'], match: 'exact' },
  { id: 'wireShellV2ModuleTitle', category: 'shell-heuristic', terms: ['wireShellV2ModuleTitle'], match: 'exact' },
  {
    id: 'data-guard',
    category: 'data-guard',
    match: 'exact',
    // Datenschutz-Fassade (Negativ-Abnahme 3; A1 Eintraege 329–344).
    terms: [
      'createDynamicAppDataGuard', 'hasReviewedCollectionDataGrant', 'createGuardedRawDbProxy',
      'createGuardedCollectionsProxy', 'createGuardedCollectionProxy', 'guardedCollectionFor',
      'moduleUsesGuardedDataFacade', 'GUARDED_PACKAGED_DATA_MODULE_IDS',
      'assertGuardedCollectionPermission', 'guardAllowsCollectionPermission',
    ],
  },
];

const ID_TERM_INDEX = IDENTIFIER_RULES
  .flatMap((r) => r.terms.map((t) => ({ rule: r, term: t, lower: t.toLowerCase(), exact: r.match === 'exact' })))
  .sort((a, b) => b.term.length - a.term.length);

export const RULES = {
  ...Object.fromEntries(IDENTIFIER_RULES.map((r) => [r.id, { category: r.category, scope: 'all', describe: `Bezeichner ${r.terms.join(', ')}` }])),
  'until-accepted-terminal': { category: 'command-bus', scope: 'all', describe: "until: 'accepted' | 'terminal'" },
  'location-reload': { category: 'reload', scope: 'all', describe: 'location.reload' },
  'cache-buster': { category: 'reload', scope: 'all', describe: '?v= Cache-Buster (JS, HTML, CSS)' },
  'set-interval-undeclared': { category: 'loop', scope: 'apps', describe: "setInterval in Apps ohne Deklaration '// v4-allow: ui-animation' (gleiche oder vorige Zeile)" },
  'fetch-absolute-or-api': { category: 'http', scope: 'apps', describe: 'fetch( auf absolute (/, //, http) oder /api-URL in Apps' },
  'ref-legacy-catalog-file': { category: 'catalog', scope: 'all', describe: 'Verweis auf registry.json, mobile-apps.json, system-apps.json oder schema.js' },
  'file-registry-json': { category: 'catalog', scope: 'all', describe: 'Datei registry.json existiert' },
  'file-mobile-apps-json': { category: 'catalog', scope: 'all', describe: 'Datei mobile-apps.json existiert' },
  'file-system-apps-json': { category: 'catalog', scope: 'all', describe: 'Datei system-apps.json existiert' },
  'file-schema-js': { category: 'catalog', scope: 'all', describe: 'Datei schema.js existiert (Collections nur aus collections.schema.json)' },
  'english-raw-term': { category: 'i18n', scope: 'all', describe: 'Englischer Rohwert als sichtbarer Text (Glossar v4/frame/glossary.js: Kategorie, Status, App-Titel), z. B. „Open“, „Writeback Pending“, „Analytics“' },
};

// FR5-3: englische Rohwerte aus dem Glossar des Rahmens. Sichtbarer Text = String-Literal, Template-Text, HTML-Text
// oder Wert einer nicht-englischen Locale-Datei (locales/<lang>.json, lang ≠ en), der als Ganzes einem Rohwert
// entspricht und nicht in Schlüsselform steht (looksLikeKey: 'open', 'in_progress' sind Codes, 'Open' ist Text).
// Ausnahme je Zeile: '// v4-allow: en' (englische Wörterbücher im Code) in derselben oder der vorigen Zeile.
// 'error' bleibt draußen: Error ist der Name der JS-Fehlerklasse (error.name), kein Anzeigewort.
export const ENGLISH_RAW_TERMS = new Set([...englishRawTerms()].filter((t) => t !== 'error'));
const ALLOW_EN = /v4-allow:\s*en\b/;
// Das Glossar selbst ist die Übersetzungstabelle (Rohwert → deutscher Begriff): weder Rohwert- noch Vokabelfund.
export const GLOSSARY_REL = 'v4/frame/glossary.js';
const LOCALE_NON_EN = /(^|\/)locales?\/(?!en(?:[-_][^/]*)?\.json$)[^/]+\.json$/i;
export function isEnglishRawTerm(text, terms = ENGLISH_RAW_TERMS) {
  const t = String(text ?? '').trim();
  if (!t || t.length > 40 || looksLikeKey(t)) return false;
  return terms.has(rawKey(t));
}

export function loadVocabulary(file = DEFAULT_VOCABULARY) {
  const raw = JSON.parse(fs.readFileSync(file, 'utf8'));
  return raw.entries.map((e) => ({ ...e, re: new RegExp(e.pattern, (e.flags || '').replace('g', '') + 'g') }));
}

// Literal ohne Leerzeichen in Schluessel-/Codeform: i18n-Schluessel ('sync.loading'),
// Zustandscodes ('syncing'), Bezeichner ('loadingModule'), Pfade, Selektoren.
// Einzelne gross geschriebene Woerter ('Loading', 'Offline', 'Läuft') bleiben Text.
export function looksLikeKey(text) {
  const t = text.trim();
  if (!t) return true;
  if (/\s/.test(t)) return false;
  if (/^[A-ZÄÖÜ][a-zäöüß]+[.…!:]*$/u.test(t)) return false;
  return true;
}

function lineIndex(src) {
  const starts = [0];
  for (let i = 0; i < src.length; i++) if (src[i] === '\n') starts.push(i + 1);
  return starts;
}

function lineOf(starts, pos) {
  let lo = 0;
  let hi = starts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (starts[mid] <= pos) lo = mid; else hi = mid - 1;
  }
  return lo + 1;
}

function matchParen(s, open) {
  let depth = 0;
  for (let j = open; j < s.length; j++) {
    if (s[j] === '(') depth++;
    else if (s[j] === ')') { depth--; if (depth === 0) return j + 1; }
  }
  return s.length;
}

// Erlaubte Bereiche fuer Zustandstexte: Argumente von api.status.report/clear (bzw. der
// Zone selbst, zone.report) und Entwicklerausgaben console.*(...), die nie im UI stehen.
export function statusReportRanges(noStr) {
  const ranges = [];
  const re = /\b(?:status|zone)\s*\.\s*(?:report|clear)\s*\(|\bconsole\s*\.\s*[A-Za-z]+\s*\(/g;
  let m;
  while ((m = re.exec(noStr))) {
    const open = m.index + m[0].length - 1;
    ranges.push([m.index, matchParen(noStr, open)]);
  }
  return ranges;
}

const inRanges = (ranges, pos) => ranges.some(([a, b]) => pos >= a && pos < b);

function containsAtCamelBoundary(tok, lower, term) {
  let idx = lower.indexOf(term);
  while (idx >= 0) {
    if (idx === 0 || tok[idx - 1] === '_' || tok[idx - 1] === '$' || /[A-Z]/.test(tok[idx])) return true;
    idx = lower.indexOf(term, idx + 1);
  }
  return false;
}

const ALLOW_INTERVAL = /v4-allow:\s*ui-animation/;
// Beschriftungen (Reiter, Ansichten), die ein Zustandswort enthalten, aber keinen Zustand melden:
// '// v4-allow: label' bzw. '<!-- v4-allow: label -->' in derselben oder der vorigen Zeile.
const ALLOW_LABEL = /v4-allow:\s*label/;
// Texte der Zustandszone außerhalb von status-zone.js (z. B. v4/shell/sync-state.js SYNC_TEXT, Strang ZONE45): dort
// sind Zustandstexte erlaubt. '// v4-allow: zone' in derselben oder der vorigen Zeile.
const ALLOW_ZONE = /v4-allow:\s*zone\b/;
// Befristete Ausnahme für Bezeichner-Regeln mit Eintrag im Löschplan (v4/debt/LOESCHPLAN.md):
// '// v4-allow: debt L-<nr>' in derselben oder der vorigen Zeile (z. B. V3-Rückfall bis zur Löschung).
const ALLOW_DEBT = /v4-allow:\s*debt\s+L-\d+/;
// Code-Werte mit Zustandswort, die nie als Text erscheinen (Fehlercode 'unavailable' aus dem Kernvertrag in
// Vertragsbeschreibungen, Statusschlüssel in Daten): '// v4-allow: code' in derselben oder der vorigen Zeile.
const ALLOW_CODE = /v4-allow:\s*code\b/;

export function scanSource(rel, src, vocabulary, { statusZone = false } = {}) {
  const hits = [];
  const ext = path.posix.extname(rel).toLowerCase();
  const lexed = lexFile(rel, src);
  const starts = lineIndex(src);
  const rawLines = src.split('\n');
  const group = groupOf(rel);
  const isApp = kindOf(group) === 'app';
  const isJson = ext === '.json';
  const add = (rule, pos, extra = {}) => {
    const line = lineOf(starts, pos);
    hits.push({ rule, line, text: (rawLines[line - 1] || '').trim().slice(0, 200), ...extra });
  };

  if (!isJson) {
    // Bezeichner
    const idRe = /[A-Za-z_$][A-Za-z0-9_$]*/g;
    let m;
    while ((m = idRe.exec(lexed.noStr))) {
      if (m.index > 0 && /[0-9]/.test(lexed.noStr[m.index - 1])) continue;
      const tok = m[0];
      const lower = tok.toLowerCase();
      for (const t of ID_TERM_INDEX) {
        if (t.exact ? tok === t.term : containsAtCamelBoundary(tok, lower, t.lower)) {
          const ln = lineOf(starts, m.index);
          if (!ALLOW_DEBT.test(rawLines[ln - 1] || '') && !ALLOW_DEBT.test(rawLines[ln - 2] || '')) add(t.rule.id, m.index, { token: tok });
          break;
        }
      }
    }
    // until: 'accepted' | 'terminal'
    const untilRe = /\buntil\s*:\s*(['"`])(accepted|terminal)\1/g;
    while ((m = untilRe.exec(lexed.code))) {
      if (lexed.noStr[m.index] === ' ' && lexed.code[m.index] !== ' ') continue; // innerhalb eines Strings
      const uln = lineOf(starts, m.index);
      // Ledger-Ausnahme wie bei den Bezeichnern: '// v4-allow: debt L-<nr>' (z. B. Workjet-V2-Kompatibilität, L-42).
      if (ALLOW_DEBT.test(rawLines[uln - 1] || '') || ALLOW_DEBT.test(rawLines[uln - 2] || '')) continue;
      add('until-accepted-terminal', m.index);
    }
    // location.reload
    const reloadRe = /\blocation\s*\.\s*reload\b/g;
    while ((m = reloadRe.exec(lexed.noStr))) add('location-reload', m.index);
    // ?v= Cache-Buster
    const cbRe = /\?v=/g;
    while ((m = cbRe.exec(lexed.code))) add('cache-buster', m.index);
    if (isApp) {
      const ivRe = /\bsetInterval\s*\(/g;
      while ((m = ivRe.exec(lexed.noStr))) {
        const line = lineOf(starts, m.index);
        if (ALLOW_INTERVAL.test(rawLines[line - 1] || '') || ALLOW_INTERVAL.test(rawLines[line - 2] || '')) continue;
        add('set-interval-undeclared', m.index);
      }
      const fetchRe = /\bfetch\s*\(\s*(['"`])((?:[^'"`\\\n]|\\.)*)/g;
      while ((m = fetchRe.exec(lexed.code))) {
        if (lexed.noStr[m.index] !== lexed.code[m.index]) continue;
        const url = m[2];
        if (/^(?:https?:)?\/\//i.test(url) || url.startsWith('/') || /\/api(?:\/|\b)/.test(url)) add('fetch-absolute-or-api', m.index, { url: url.slice(0, 120) });
      }
    }
  }

  // Verweise auf Altkatalog-/Schema-Dateien und Zustandsvokabular in String-Segmenten
  const allowed = isJson ? [] : statusReportRanges(lexed.noStr);
  const checkEnglish = rel !== GLOSSARY_REL && ext !== '.css' && (!isJson || LOCALE_NON_EN.test(rel)) && !/(^|\/)locales?\/en[^/]*\.json$/i.test(rel);
  const refRe = /(?:^|[\/'"`\s])(registry|mobile-apps|system-apps)\.json\b|(?:^|\/)schema\.js\b/;
  for (const seg of lexed.strings) {
    const text = src.slice(seg.start, seg.end);
    if (!text.trim()) continue;
    if (!isJson && refRe.test(text)) add('ref-legacy-catalog-file', seg.start);
    if (checkEnglish && isEnglishRawTerm(text)) {
      const ln = lineOf(starts, seg.start);
      if (!ALLOW_EN.test(rawLines[ln - 1] || '') && !ALLOW_EN.test(rawLines[ln - 2] || '')) add('english-raw-term', seg.start, { match: text.trim() });
    }
    if (!vocabulary || statusZone || ext === '.css') continue;
    if (inRanges(allowed, seg.start)) continue;
    if (looksLikeKey(text)) continue;
    const seen = new Set();
    for (const v of vocabulary) {
      v.re.lastIndex = 0;
      let vm;
      while ((vm = v.re.exec(text))) {
        const pos = seg.start + vm.index;
        const ln = lineOf(starts, pos);
        const key = `${v.id}:${ln}`;
        const lineText = (i) => (i >= 1 && i <= starts.length ? src.slice(starts[i - 1], i < starts.length ? starts[i] : src.length) : '');
        const labelOk = ALLOW_LABEL.test(lineText(ln)) || ALLOW_LABEL.test(lineText(ln - 1))
          || ALLOW_ZONE.test(lineText(ln)) || ALLOW_ZONE.test(lineText(ln - 1))
          || ALLOW_CODE.test(lineText(ln)) || ALLOW_CODE.test(lineText(ln - 1));
        if (!labelOk && !seen.has(key)) { seen.add(key); add(`vocab:${v.id}`, pos, { match: vm[0] }); }
        if (vm[0] === '') v.re.lastIndex++;
      }
    }
  }
  return hits;
}

// ---------------------------------------------------------------- Lauf

// ported.json: { "ported": [Pfadpraefixe], "statusZoneFiles": [Dateien der Zustandszone selbst] }
export function loadConfig(file = DEFAULT_PORTED) {
  let raw = {};
  try { raw = JSON.parse(fs.readFileSync(file, 'utf8')); } catch { raw = {}; }
  if (Array.isArray(raw)) raw = { ported: raw };
  return {
    ported: Array.isArray(raw.ported) ? raw.ported : [],
    statusZoneFiles: Array.isArray(raw.statusZoneFiles) ? raw.statusZoneFiles : [],
  };
}

export const loadPorted = (file) => loadConfig(file).ported;

const isPorted = (rel, ported) => ported.some((p) => {
  const q = p.replace(/\/+$/, '');
  return rel === q || rel.startsWith(`${q}/`);
});

const EXCLUDED_JSON = /(^|\/)(package(-lock)?\.json|[^/]*\.schema\.json|module\.json|tsconfig[^/]*\.json)$/;

export function scan({ root = DEFAULT_ROOT, vocabularyFile = DEFAULT_VOCABULARY, ported = [], statusZoneFiles = [], includeTests = false } = {}) {
  const vocabulary = loadVocabulary(vocabularyFile);
  const files = walk(root);
  const hits = [];
  let scanned = 0;
  for (const rel of files) {
    const test = isTestFile(rel);
    for (const ex of EXISTENCE_FILES) {
      if (ex.test(rel)) hits.push({ rule: ex.rule, file: rel, line: 1, text: 'Datei existiert', group: groupOf(rel), test });
    }
    const ext = path.posix.extname(rel).toLowerCase();
    if (!CODE_EXT.has(ext)) continue;
    if (ext === '.json' && EXCLUDED_JSON.test(rel)) continue;
    let src;
    try { src = fs.readFileSync(path.join(root, rel), 'utf8'); } catch { continue; }
    if (src.includes('\u0000')) continue;
    scanned++;
    // Vokabular gilt nicht fuer Handler (treu portierte Fehler-/Mailtexte, Anzeige nur ueber die Zone)
    // und nicht fuer Herkunftsdateien von Icons.
    const vocabOff = /(^|\/)handlers\//.test(rel) || /(^|\/)assets\/.*provenance\.json$/.test(rel) || rel === GLOSSARY_REL;
    const statusZone = statusZoneFiles.includes(rel) || vocabOff;
    for (const h of scanSource(rel, src, vocabulary, { statusZone })) hits.push({ ...h, file: rel, group: groupOf(rel), test });
  }
  const counted = includeTests ? hits : hits.filter((h) => !h.test);
  const testHits = hits.filter((h) => h.test).length;
  const byRule = {};
  const byCategory = {};
  const byGroup = {};
  const byFile = {};
  for (const h of counted) {
    const cat = h.rule.startsWith('vocab:') ? 'vocab' : (RULES[h.rule]?.category || 'other');
    byRule[h.rule] = (byRule[h.rule] || 0) + 1;
    byCategory[cat] = (byCategory[cat] || 0) + 1;
    const g = (byGroup[h.group] ||= { kind: kindOf(h.group), total: 0, categories: {}, rules: {} });
    g.total++;
    g.categories[cat] = (g.categories[cat] || 0) + 1;
    g.rules[h.rule] = (g.rules[h.rule] || 0) + 1;
    byFile[h.file] = (byFile[h.file] || 0) + 1;
  }
  const portedHits = counted.filter((h) => isPorted(h.file, ported));
  return {
    schema: 'shell-v4-guard-v1',
    root,
    filesWalked: files.length,
    filesScanned: scanned,
    includeTests,
    total: counted.length,
    testHitsSeparate: includeTests ? 0 : testHits,
    ported,
    statusZoneFiles,
    portedHits: portedHits.length,
    portedHitList: portedHits.map((h) => `${h.file}:${h.line} [${h.rule}]`),
    byCategory,
    byRule,
    byGroup,
    byFile,
    hits: counted,
    testHits: includeTests ? [] : hits.filter((h) => h.test),
  };
}

// ---------------------------------------------------------------- Ausgabe

function sortEntries(obj) {
  return Object.entries(obj).sort((a, b) => (typeof b[1] === 'number' ? b[1] - a[1] : b[1].total - a[1].total) || a[0].localeCompare(b[0]));
}

export function formatHuman(res, { summary = false } = {}) {
  const out = [];
  if (!summary) {
    const byFile = new Map();
    for (const h of res.hits) {
      if (!byFile.has(h.file)) byFile.set(h.file, []);
      byFile.get(h.file).push(h);
    }
    let lastGroup = '';
    for (const [file, list] of [...byFile.entries()].sort((a, b) => a[0].localeCompare(b[0]))) {
      const g = list[0].group;
      if (g !== lastGroup) { out.push(`\n== ${g}`); lastGroup = g; }
      out.push(`-- ${file} (${list.length})`);
      for (const h of list.sort((a, b) => a.line - b.line)) out.push(`  ${file}:${h.line}  [${h.rule}]  ${h.text}`);
    }
    out.push('');
  }
  out.push(`Shell-v4-Waechter: ${res.total} Treffer in ${Object.keys(res.byFile).length} Dateien (${res.filesScanned} geprueft, ${res.filesWalked} durchlaufen)` + (res.includeTests ? ', Testdateien eingerechnet' : `, Testdateien getrennt: ${res.testHitsSeparate}`));
  out.push('\nJe Kategorie:');
  for (const [k, v] of sortEntries(res.byCategory)) out.push(`  ${String(v).padStart(6)}  ${k}`);
  out.push('\nJe Regel:');
  for (const [k, v] of sortEntries(res.byRule)) out.push(`  ${String(v).padStart(6)}  ${k}`);
  out.push('\nJe App/Bereich:');
  for (const [k, v] of sortEntries(res.byGroup)) out.push(`  ${String(v.total).padStart(6)}  ${k}  (${v.kind})`);
  out.push(`\nPortiert (${res.ported.length} Eintraege): ${res.portedHits} Treffer` + (res.portedHits ? ' -> FEHLER' : ''));
  return out.join('\n');
}

function parseArgs(argv) {
  const opts = { root: DEFAULT_ROOT, json: false, summary: false, includeTests: false, portedFile: DEFAULT_PORTED, rules: null, apps: null };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--json') opts.json = true;
    else if (a === '--summary') opts.summary = true;
    else if (a === '--include-tests') opts.includeTests = true;
    else if (a === '--root') opts.root = path.resolve(argv[++i]);
    else if (a === '--ported') opts.portedFile = path.resolve(argv[++i]);
    else if (a === '--rule') opts.rules = argv[++i].split(',');
    else if (a === '--app') opts.apps = argv[++i].split(',');
    else if (a === '-h' || a === '--help') { opts.help = true; }
    else throw new Error(`unbekanntes Argument: ${a}`);
  }
  return opts;
}

function filterResult(res, { rules, apps }) {
  if (!rules && !apps) return res;
  const keep = (h) => (!rules || rules.some((r) => h.rule === r || h.rule.startsWith(`${r}:`) || (RULES[h.rule]?.category === r) || (r === 'vocab' && h.rule.startsWith('vocab:'))))
    && (!apps || apps.includes(h.group));
  const hits = res.hits.filter(keep);
  const sub = { ...res, hits, total: hits.length, byRule: {}, byCategory: {}, byGroup: {}, byFile: {} };
  for (const h of hits) {
    const cat = h.rule.startsWith('vocab:') ? 'vocab' : (RULES[h.rule]?.category || 'other');
    sub.byRule[h.rule] = (sub.byRule[h.rule] || 0) + 1;
    sub.byCategory[cat] = (sub.byCategory[cat] || 0) + 1;
    const g = (sub.byGroup[h.group] ||= { kind: kindOf(h.group), total: 0, categories: {}, rules: {} });
    g.total++; g.categories[cat] = (g.categories[cat] || 0) + 1; g.rules[h.rule] = (g.rules[h.rule] || 0) + 1;
    sub.byFile[h.file] = (sub.byFile[h.file] || 0) + 1;
  }
  return sub;
}

export function main(argv = process.argv.slice(2)) {
  const opts = parseArgs(argv);
  if (opts.help) {
    process.stdout.write('node guard-v4.mjs [--root DIR] [--json] [--summary] [--include-tests] [--ported FILE] [--rule ID[,ID]] [--app GROUP[,GROUP]]\n');
    return 0;
  }
  const { ported, statusZoneFiles } = loadConfig(opts.portedFile);
  const full = scan({ root: opts.root, ported, statusZoneFiles, includeTests: opts.includeTests });
  const res = filterResult(full, opts);
  if (opts.json) process.stdout.write(`${JSON.stringify(res, null, 2)}\n`);
  else process.stdout.write(`${formatHuman(res, opts)}\n`);
  return full.portedHits > 0 ? 1 : 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { process.exitCode = main(); } catch (err) { process.stderr.write(`guard-v4: ${err.message}\n`); process.exitCode = 2; }
}
