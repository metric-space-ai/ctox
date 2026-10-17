#!/usr/bin/env node
// Shell v4 – Prüfskript „fehlende Übersetzungsschlüssel“ (Strang FIX7).
//
// Sammelt je App die statisch erkennbaren Textschlüssel aus index.js/index.html:
//   state.t('k' …), t('k' …), tr('k' …), labels.de.k / labels.en.k, text.k (Apps mit `let text = COPY…`),
//   data-copy="k", data-i18n="k", data-i18n-placeholder="k", data-i18n-title="k", data-i18n-aria-label="k".
// und prüft jeden Schlüssel gegen vier Quellen:
//   embedded.de / embedded.en – das eingebettete Rückfall-Wörterbuch (const labels = {de,en} bzw. COPY = {de,en}),
//                               das der Kern-Stub und der v4-Rahmen tatsächlich rendern, wenn api.shell.t den
//                               Schlüssel selbst zurückgibt (dann erscheint sonst der rohe Schlüssel, z. B.
//                               „searchOpportunities“),
//   locale.de / locale.en     – locales/de.json und locales/en.json.
// Dynamische Schlüssel (state.t(key …), text[STATUS_KEY[s]]) sind nicht statisch prüfbar und werden nicht gezählt.
//
//   node v4/tools/textdiaet/check-i18n-keys.mjs [--app customers,esign,support] [--json]   Exit 1 bei Fehlschlüsseln
//   … --fix   ergänzt locales/<lang>.json um Schlüssel, die nur im eingebetteten Wörterbuch derselben Sprache stehen
//             (übernimmt dessen Wert, erfindet nichts); Lücken im eingebetteten Wörterbuch bleiben Handarbeit.
//
// Als Modul: checkApp(moduleDir) → { app, used:[…], missing:{ 'embedded.de':[…], … } } (rein lesend).
import { readFileSync, existsSync, writeFileSync } from 'node:fs';
import { join, dirname, basename, normalize } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const MODULES = normalize(join(HERE, '..', '..', '..', 'modules'));
const KEY = '[A-Za-z_][A-Za-z0-9_]*';

const read = (p) => (existsSync(p) ? readFileSync(p, 'utf8') : '');
const readJson = (p) => { try { return JSON.parse(readFileSync(p, 'utf8')); } catch { return null; } };

// Index der zum `{` an `start` passenden `}` (String-, Template- und Kommentar-bewusst).
function matchBrace(src, start) {
  let depth = 0;
  for (let i = start; i < src.length; i += 1) {
    const c = src[i];
    if (c === '/' && src[i + 1] === '/') { i = src.indexOf('\n', i); if (i < 0) return -1; continue; }
    if (c === '/' && src[i + 1] === '*') { i = src.indexOf('*/', i) + 1; if (i <= 0) return -1; continue; }
    if (c === '"' || c === "'" || c === '`') {
      for (i += 1; i < src.length && src[i] !== c; i += 1) if (src[i] === '\\') i += 1;
      continue;
    }
    if (c === '{') depth += 1;
    else if (c === '}') { depth -= 1; if (depth === 0) return i; }
  }
  return -1;
}

// Eingebettetes Wörterbuch {de:{…}, en:{…}} aus `const labels = {` oder `COPY = {` (reines Literal) auswerten.
export function embeddedDictionary(js) {
  const m = /(?:const|let|var)\s+labels\s*=\s*\{|(?:export\s+)?const\s+COPY\s*=\s*\{/.exec(js);
  if (!m) return null;
  const open = m.index + m[0].length - 1;
  const close = matchBrace(js, open);
  if (close < 0) return null;
  try {
    // eslint-disable-next-line no-new-func
    const dict = new Function(`return (${js.slice(open, close + 1)});`)();
    return dict && typeof dict === 'object' ? dict : null;
  } catch { return null; }
}

// known: Schlüssel, die irgendeine Quelle kennt. Ein String-Literal mit genau diesem Namen gilt als benutzt
// (Schlüsseltabellen wie `{ opportunity: 'selectOpportunity' }`, die später an state.t gehen).
export function usedKeys(js, html = '', known = new Set()) {
  const keys = new Set();
  for (const m of js.matchAll(new RegExp(`['"\`](${KEY})['"\`]`, 'g'))) if (known.has(m[1]) && /[A-Z]/.test(m[1])) keys.add(m[1]);
  const add = (re, src, group = 1) => { for (const m of src.matchAll(re)) keys.add(m[group]); };
  add(new RegExp(`\\bstate\\.t\\(\\s*['"\`](${KEY})['"\`]`, 'g'), js);
  add(new RegExp(`(?<![\\w.$])(?:t|tr)\\(\\s*['"\`](${KEY})['"\`]`, 'g'), js);
  add(new RegExp(`\\blabels\\.(?:de|en)\\.(${KEY})`, 'g'), js);
  // `text.k` nur in Apps, deren Laufzeittexte in `let text = COPY…` liegen (esign-Muster).
  if (/let\s+text\s*=\s*COPY\b/.test(js)) add(new RegExp(`(?<![\\w.$])text\\.(${KEY})`, 'g'), js);
  const attr = new RegExp(`data-(?:copy|copy-placeholder|i18n|i18n-placeholder|i18n-title|i18n-aria-label)="(${KEY})"`, 'g');
  add(attr, html);
  add(attr, js);
  return [...keys].sort();
}

export function checkApp(moduleDir) {
  const js = read(join(moduleDir, 'index.js'));
  const html = read(join(moduleDir, 'index.html'));
  const embedded = embeddedDictionary(js) || {};
  const sources = {
    'embedded.de': embedded.de || {},
    'embedded.en': embedded.en || {},
    'locale.de': readJson(join(moduleDir, 'locales', 'de.json')) || {},
    'locale.en': readJson(join(moduleDir, 'locales', 'en.json')) || {},
  };
  const known = new Set(Object.values(sources).flatMap((d) => Object.keys(d)));
  const used = usedKeys(js, html, known);
  const missing = {};
  for (const [name, dict] of Object.entries(sources)) {
    missing[name] = used.filter((k) => typeof dict[k] !== 'string' || !dict[k]);
  }
  return { app: basename(moduleDir), used, missing, sources };
}

export function fixLocales(moduleDir) {
  const r = checkApp(moduleDir);
  const added = {};
  for (const lang of ['de', 'en']) {
    const file = join(moduleDir, 'locales', `${lang}.json`);
    const dict = { ...r.sources[`locale.${lang}`] };
    const emb = r.sources[`embedded.${lang}`];
    added[lang] = r.missing[`locale.${lang}`].filter((k) => typeof emb[k] === 'string' && emb[k]);
    if (!added[lang].length) continue;
    for (const k of added[lang]) dict[k] = emb[k];
    writeFileSync(file, JSON.stringify(dict, null, 2) + '\n');
  }
  return added;
}

function main() {
  const args = process.argv.slice(2);
  const i = args.indexOf('--app');
  const apps = (i >= 0 ? args[i + 1] : 'customers,esign,support').split(',').filter(Boolean);
  if (args.includes('--fix')) for (const a of apps) console.log('ergänzt', a, JSON.stringify(fixLocales(join(MODULES, a))));
  const results = apps.map((a) => checkApp(join(MODULES, a)));
  let total = 0;
  for (const r of results) for (const list of Object.values(r.missing)) total += list.length;
  if (args.includes('--json')) {
    process.stdout.write(JSON.stringify({ total, results: results.map(({ sources, ...rest }) => rest) }, null, 2) + '\n');
  } else {
    for (const r of results) {
      const n = Object.values(r.missing).reduce((s, l) => s + l.length, 0);
      console.log(`${n ? 'FEHLT' : 'OK   '} ${r.app.padEnd(12)} ${r.used.length} Schlüssel, ${n} Lücken`);
      for (const [src, list] of Object.entries(r.missing)) if (list.length) console.log(`       ${src.padEnd(12)} ${list.join(', ')}`);
    }
  }
  process.exitCode = total ? 1 : 0;
}

if (import.meta.url === pathToFileURL(process.argv[1] || '').href) main();
