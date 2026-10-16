#!/usr/bin/env node
// Shell v4 – Waechter-Regel „Text- und Gestaltungsdiaet“ (briefs/text-diet-common.md, Regeln 1 und 2).
//
// Meldet in App-Quellen (modules/*, desktop-apps/*, customer-modules/*; ohne Tests, vendor/, handlers/):
//   infobox    – Markup-Klasse einer Hinweis-/Warn-/Erklaerbox im Inhalt: *-notice, *-alert, *-warning,
//                *-hint, *-info-box/*-infobox, *-callout, *-banner (auch nackt: notice, callout, infobox,
//                info-box). Gezaehlt in HTML-class-Attributen, in class="…" innerhalb von JS-Templates und in
//                JS-Strings, die reine Klassenlisten sind (el('div', 'mail-notice'), className = '…').
//                CSS-Selektoren zaehlen nicht (sie rendern nichts).
//   help-text  – Hilfe-/Erklaersatz > 80 Zeichen als sichtbarer Text: Textlaeufe zwischen Tags in HTML und
//                JS-Strings/Templates sowie Werte in locales/*.json. Ausgenommen: Argumente von
//                api.status.report/clear und console.*, new Error(…)/throw/*Error(…)-Melder, Tooltip-Ziele (title,
//                aria-*, tooltip), Agenten-Anweisungen (prompt:, instruction:), Locale-Schluessel mit status/error/tooltip/aria/prompt im Namen.
//
// Freigabe einer bewusst langen Zeile: '// v4-allow: text' bzw. '<!-- v4-allow: text -->' in derselben
// oder der vorigen Zeile.
//
// Ratchet: textdiaet-baseline.json haelt je Datei und Regel die erlaubte Hoechstzahl.
//   node textdiaet-rule.mjs [--root DIR] [--app modules/mail[,…]] [--json] [--summary]
//   node textdiaet-rule.mjs --check            Exit 1, wenn eine Datei/Regel ueber der Baseline liegt
//   node textdiaet-rule.mjs --write-baseline   Baseline auf den aktuellen Stand setzen (nur senken!)
//
// Keine Abhaengigkeiten ausser Node >= 18 und dem Lexer aus guard-v4.mjs.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { lexFile, isTestFile, groupOf, kindOf, statusReportRanges, APP_VENDOR_DIR } from './guard-v4.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const DEFAULT_ROOT = path.resolve(HERE, '../../..');
export const DEFAULT_BASELINE = path.join(HERE, 'textdiaet-baseline.json');
export const MAX_HELP_CHARS = 80;

export const RULES = {
  infobox: 'Hinweis-/Warn-/Erklaerbox-Klasse im App-Markup (*-notice, *-alert, *-warning, *-hint, *-info-box, *-callout, *-banner)',
  'help-text': `Hilfe-/Erklaersatz > ${MAX_HELP_CHARS} Zeichen als sichtbarer Text`,
};

const APP_TOPS = new Set(['modules', 'desktop-apps', 'customer-modules']);
const SCAN_EXT = new Set(['.js', '.mjs', '.html', '.htm', '.json']);

// Klassentoken einer Infobox.
const INFOBOX_SUFFIX = /^[a-z][a-z0-9_]*(?:-[a-z0-9_]+)*-(?:notice|alert|warning|hint|info-box|infobox|callout|banner)$/;
const INFOBOX_BARE = /^(?:notice|infobox|info-box)$/;
// Zustandsmodifikatoren (ctox-badge is-warning) und Attributnamen (data-*-hint, aria-*) sind keine Boxen.
const NOT_A_BOX = /^(?:is|has|data|aria)-/;
export const isInfoboxClass = (tok) => !NOT_A_BOX.test(tok) && (INFOBOX_SUFFIX.test(tok) || INFOBOX_BARE.test(tok));
const CLASS_TOKEN = /^-?[A-Za-z_][A-Za-z0-9_-]*$/;

const ALLOW_TEXT = /v4-allow:\s*text/;

export function isAppSource(rel) {
  const parts = rel.split('/');
  if (parts.length < 3 || !APP_TOPS.has(parts[0])) return false;
  if (isTestFile(rel)) return false;
  if (/(^|\/)(handlers|vendor|node_modules|dist|qa)\//.test(rel)) return false;
  const ext = path.posix.extname(rel).toLowerCase();
  if (!SCAN_EXT.has(ext)) return false;
  if (ext === '.json') return /(^|\/)locales\/[^/]+\.json$/.test(rel);
  return true;
}

function walk(root, sub = '') {
  const out = [];
  const stack = [sub];
  while (stack.length) {
    const relDir = stack.pop();
    let entries;
    try { entries = fs.readdirSync(path.join(root, relDir), { withFileTypes: true }); } catch { continue; }
    for (const ent of entries) {
      const rel = relDir ? `${relDir}/${ent.name}` : ent.name;
      if (ent.isDirectory()) {
        if (ent.name === 'node_modules' || ent.name === '.git' || APP_VENDOR_DIR.test(rel)) continue;
        if (!relDir && !APP_TOPS.has(ent.name)) continue;
        stack.push(rel);
      } else if (ent.isFile() && isAppSource(rel)) out.push(rel);
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
  let lo = 0; let hi = starts.length - 1;
  while (lo < hi) { const mid = (lo + hi + 1) >> 1; if (starts[mid] <= pos) lo = mid; else hi = mid - 1; }
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

// Bereiche, deren Strings kein sichtbarer Inhalt sind.
function excludedRanges(noStr) {
  const ranges = statusReportRanges(noStr);
  // new Error(…), new TypeError(…) und Fehlermelder der App (renderError(…), reportDocumentError(…) → Zone)
  const re = /\b(?:new\s+)?[A-Za-z_$]*Error\s*\(|\bthrow\b/g;
  let m;
  while ((m = re.exec(noStr))) {
    if (m[0] === 'throw') {
      const end = noStr.indexOf(';', m.index);
      const nl = noStr.indexOf('\n', m.index);
      ranges.push([m.index, Math.max(end < 0 ? noStr.length : end, nl < 0 ? noStr.length : nl)]);
      continue;
    }
    ranges.push([m.index, matchParen(noStr, m.index + m[0].length - 1)]);
  }
  return ranges;
}
const inRanges = (ranges, pos) => ranges.some(([a, b]) => pos >= a && pos < b);

// Tooltip-Ziel direkt vor dem String: title = '…', title: '…', setAttribute('title', '…'), aria-*.
// Agenten-Anweisungen (prompt:, instruction:) sind keine Oberflaechentexte.
const TOOLTIP_BEFORE = /(?:\btitle|tooltip|aria[-A-Za-z]*|ariaLabel|ariaDescription|\bprompt|\binstructions?|systemPrompt)['"]?\s*[:=,]\s*["'`]?\s*(?:\$\{)?\s*\(?\s*(?:[A-Za-z_$][\w$.]*\s*\(\s*(?:['"][^'"]*['"]\s*,\s*)?)*$/i;

const collapse = (t) => t.replace(/&nbsp;/g, ' ').replace(/\$\{[^}]*\}/g, ' ').replace(/\s+/g, ' ').trim();

export function looksLikeProse(text) {
  const t = collapse(text);
  if (t.length <= MAX_HELP_CHARS) return false;
  if (/^(?:https?:|data:|\/|\.\/|#)/.test(t)) return false;
  const words = t.split(' ');
  if (words.length < 8) return false;
  const letters = (t.match(/[A-Za-zÄÖÜäöüß]/g) || []).length;
  if (letters / t.length < 0.7) return false;
  // Code-, CSS- und Selektor-Fragmente
  if (/[{};]|=>|\b(?:function|const|return|var|let)\b|--[a-z]|:\s*[0-9.]+(?:px|rem|em|%)/.test(t)) return false;
  return true;
}

// Textlaeufe ausserhalb von Tags (fuer Template- und HTML-Segmente).
function textRuns(seg) {
  const runs = [];
  const re = /<[^>]*>/g;
  let last = 0;
  let m;
  while ((m = re.exec(seg))) {
    if (m.index > last) runs.push({ off: last, text: seg.slice(last, m.index) });
    last = m.index + m[0].length;
  }
  // Segment beginnt mitten in einem Tag (Template nach ${…}): Rest bis '>' ist Attributinhalt
  if (runs.length && runs[0].off === 0) {
    const gt = seg.indexOf('>');
    const lt0 = seg.indexOf('<');
    if (gt >= 0 && (lt0 < 0 || gt < lt0) && /=\s*["']|["']\s*\/?$/.test(seg.slice(0, gt))) runs.shift();
  }
  // offener Tag am Segmentende (Template wird durch ${…} unterbrochen): Rest ist Attributinhalt
  const rest = seg.slice(last);
  const lt = rest.indexOf('<');
  runs.push({ off: last, text: lt < 0 ? rest : rest.slice(0, lt) });
  return runs;
}

function lexLocale(src) {
  // Werte samt Schluessel-Pfad (flach genug fuer i18n-Dateien).
  const out = [];
  let data;
  try { data = JSON.parse(src); } catch { return out; }
  const visit = (node, keyPath) => {
    if (typeof node === 'string') out.push({ key: keyPath, value: node });
    else if (node && typeof node === 'object') for (const [k, v] of Object.entries(node)) visit(v, keyPath ? `${keyPath}.${k}` : k);
  };
  visit(data, '');
  return out;
}
// Zone-/Fehlertexte, Tooltips, Agenten-Anweisungen und die Katalogbeschreibung der App selbst.
const LOCALE_KEY_EXEMPT = /(^|[._-])(status|error|errors|tooltip|aria|prompt|instruction|instructions)([._-]|$)|(?:Tooltip|Error|Failed|Required|NotFound|Missing|Invalid|Unavailable|Prompt|Instruction)$|^(?:description|name|title)$/i;

export function scanSource(rel, src) {
  const hits = [];
  const starts = lineStarts(src);
  const rawLines = src.split('\n');
  const add = (rule, pos, extra = {}) => {
    const line = lineOf(starts, pos);
    if (ALLOW_TEXT.test(rawLines[line - 1] || '') || ALLOW_TEXT.test(rawLines[line - 2] || '')) return;
    hits.push({ rule, line, text: (rawLines[line - 1] || '').trim().slice(0, 200), ...extra });
  };
  const ext = path.posix.extname(rel).toLowerCase();

  if (ext === '.json') {
    for (const { key, value } of lexLocale(src)) {
      if (LOCALE_KEY_EXEMPT.test(key)) continue;
      if (!looksLikeProse(value)) continue;
      const needle = JSON.stringify(value).slice(1, -1);
      let pos = src.indexOf(needle);
      if (pos < 0) pos = src.indexOf(`"${key.split('.').pop()}"`);
      const line = lineOf(starts, Math.max(pos, 0));
      hits.push({ rule: 'help-text', line, key, chars: collapse(value).length, text: collapse(value).slice(0, 200) });
    }
    return hits;
  }

  const lexed = lexFile(rel, src);
  const isHtml = ext === '.html' || ext === '.htm';
  const excluded = excludedRanges(lexed.noStr);

  for (const seg of lexed.strings) {
    const text = src.slice(seg.start, seg.end);
    if (!text.trim()) continue;
    if (inRanges(excluded, seg.start)) continue;
    const before = lexed.code.slice(Math.max(0, seg.start - 80), seg.start - (isHtml ? 0 : 1));

    // ---- infobox
    const classAttrRe = /\bclass(?:Name)?\s*=\s*(["'])([^"'<>]*)\1/g;
    let m;
    const classLists = [];
    while ((m = classAttrRe.exec(text))) classLists.push({ off: m.index + m[0].indexOf(m[2]), list: m[2] });
    if (isHtml) {
      // Attributwert direkt nach class=
      if (/\bclass\s*=\s*["']$/.test(before)) classLists.push({ off: 0, list: text });
    } else if (!/[<>]/.test(text) && !/(?:\.(?:contains|matches|closest|querySelector(?:All)?|getElementsByClassName|remove)|setAttribute|getAttribute|hasAttribute|removeAttribute|toggleAttribute)\s*\(\s*$/.test(before)) {
      const toks = text.trim().split(/\s+/);
      if (toks.length && toks.every((t) => CLASS_TOKEN.test(t)) && toks.some(isInfoboxClass)) classLists.push({ off: 0, list: text });
    }
    const seen = new Set();
    for (const { off, list } of classLists) {
      for (const tok of list.split(/\s+/)) {
        if (!isInfoboxClass(tok) || seen.has(tok)) continue;
        seen.add(tok);
        add('infobox', seg.start + off + Math.max(0, list.indexOf(tok)), { token: tok });
      }
    }

    // ---- help-text
    if (!isHtml && TOOLTIP_BEFORE.test(before)) continue;
    if (isHtml && /\b(?:title|aria-[a-z-]+|content|alt)\s*=\s*["']$/.test(before)) continue;
    if (isHtml && /\b(?:class|id|style|href|src|d|viewBox|data-[a-z-]+)\s*=\s*["']$/.test(before)) continue;
    for (const run of (isHtml || /<[a-z!/]/i.test(text)) ? textRuns(text) : [{ off: 0, text }]) {
      if (!looksLikeProse(run.text)) continue;
      const lead = run.text.length - run.text.trimStart().length;
      add('help-text', seg.start + run.off + lead, { chars: collapse(run.text).length, match: collapse(run.text).slice(0, 200) });
    }
  }
  return hits;
}

export function scan({ root = DEFAULT_ROOT, apps = null } = {}) {
  const files = walk(root).filter((rel) => !apps || apps.some((a) => rel === a || rel.startsWith(`${a.replace(/\/+$/, '')}/`)));
  const hits = [];
  for (const rel of files) {
    let src;
    try { src = fs.readFileSync(path.join(root, rel), 'utf8'); } catch { continue; }
    for (const h of scanSource(rel, src)) hits.push({ ...h, file: rel, group: groupOf(rel) });
  }
  const byRule = {};
  const byGroup = {};
  const counts = {};
  for (const h of hits) {
    byRule[h.rule] = (byRule[h.rule] || 0) + 1;
    const g = (byGroup[h.group] ||= { total: 0, rules: {} });
    g.total++; g.rules[h.rule] = (g.rules[h.rule] || 0) + 1;
    const c = (counts[h.file] ||= {});
    c[h.rule] = (c[h.rule] || 0) + 1;
  }
  return { schema: 'shell-v4-textdiaet-v1', root, filesScanned: files.length, total: hits.length, byRule, byGroup, counts, hits };
}

export function loadBaseline(file = DEFAULT_BASELINE) {
  try { return JSON.parse(fs.readFileSync(file, 'utf8')).files || {}; } catch { return {}; }
}

// Ratchet: jede Datei/Regel darf hoechstens ihren Baseline-Wert haben; neue Dateien 0.
export function ratchet(res, baseline, { apps = null } = {}) {
  const over = [];
  const lowered = [];
  for (const [file, rules] of Object.entries(res.counts)) {
    for (const [rule, n] of Object.entries(rules)) {
      const allowed = baseline[file]?.[rule] ?? 0;
      if (n > allowed) over.push({ file, rule, count: n, allowed });
    }
  }
  for (const [file, rules] of Object.entries(baseline)) {
    if (apps && !apps.some((a) => file.startsWith(`${a.replace(/\/+$/, '')}/`))) continue;
    for (const [rule, allowed] of Object.entries(rules)) {
      const n = res.counts[file]?.[rule] ?? 0;
      if (n < allowed) lowered.push({ file, rule, count: n, allowed });
    }
  }
  return { over, lowered };
}

function writeBaseline(file, res, previous, apps) {
  const files = apps ? { ...previous } : {};
  if (apps) for (const k of Object.keys(files)) if (apps.some((a) => k.startsWith(`${a.replace(/\/+$/, '')}/`))) delete files[k];
  for (const [f, rules] of Object.entries(res.counts)) files[f] = rules;
  const sorted = Object.fromEntries(Object.entries(files).sort((a, b) => a[0].localeCompare(b[0])));
  const total = Object.values(sorted).reduce((s, r) => s + Object.values(r).reduce((x, y) => x + y, 0), 0);
  fs.writeFileSync(file, `${JSON.stringify({ schema: 'shell-v4-textdiaet-baseline-v1', note: 'Nur senken. Hoechstzahl je Datei und Regel; neue Dateien 0.', total, files: sorted }, null, 2)}\n`);
  return total;
}

function parseArgs(argv) {
  const o = { root: DEFAULT_ROOT, baseline: DEFAULT_BASELINE, apps: null, json: false, summary: false, check: false, write: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--root') o.root = path.resolve(argv[++i]);
    else if (a === '--baseline') o.baseline = path.resolve(argv[++i]);
    else if (a === '--app') o.apps = argv[++i].split(',').map((s) => (s.includes('/') ? s : `modules/${s}`));
    else if (a === '--json') o.json = true;
    else if (a === '--summary') o.summary = true;
    else if (a === '--check') o.check = true;
    else if (a === '--write-baseline') o.write = true;
    else if (a === '-h' || a === '--help') o.help = true;
    else throw new Error(`unbekanntes Argument: ${a}`);
  }
  return o;
}

export function main(argv = process.argv.slice(2)) {
  const o = parseArgs(argv);
  if (o.help) { process.stdout.write('node textdiaet-rule.mjs [--root DIR] [--app ID[,ID]] [--json] [--summary] [--check] [--write-baseline] [--baseline FILE]\n'); return 0; }
  const res = scan({ root: o.root, apps: o.apps });
  if (o.write) {
    const total = writeBaseline(o.baseline, res, loadBaseline(o.baseline), o.apps);
    process.stdout.write(`Baseline geschrieben: ${o.baseline} (${total} Treffer)\n`);
    return 0;
  }
  const rat = ratchet(res, loadBaseline(o.baseline), { apps: o.apps });
  if (o.json) { process.stdout.write(`${JSON.stringify({ ...res, ratchet: rat }, null, 2)}\n`); return o.check && rat.over.length ? 1 : 0; }
  const out = [];
  if (!o.summary) {
    for (const h of res.hits) out.push(`  ${h.file}:${h.line}  [${h.rule}]  ${h.token || h.key || ''}${h.chars ? ` (${h.chars} Z.)` : ''}  ${h.match || h.text}`);
    out.push('');
  }
  out.push(`Textdiaet-Waechter: ${res.total} Treffer in ${Object.keys(res.counts).length} Dateien (${res.filesScanned} geprueft) – ` + Object.entries(res.byRule).map(([k, v]) => `${k} ${v}`).join(', '));
  for (const [g, v] of Object.entries(res.byGroup).sort((a, b) => b[1].total - a[1].total)) out.push(`  ${String(v.total).padStart(5)}  ${g}  ${Object.entries(v.rules).map(([k, n]) => `${k}=${n}`).join(' ')}`);
  if (rat.over.length) {
    out.push(`\nUeber der Baseline (${rat.over.length}) -> ${o.check ? 'FEHLER' : 'Hinweis'}:`);
    for (const r of rat.over) out.push(`  ${r.file} [${r.rule}] ${r.count} > ${r.allowed}`);
  }
  if (rat.lowered.length) out.push(`\nUnter der Baseline (${rat.lowered.length}) – mit --write-baseline${o.apps ? ` --app ${o.apps.join(',')}` : ''} senken.`);
  process.stdout.write(`${out.join('\n')}\n`);
  return o.check && rat.over.length ? 1 : 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { process.exitCode = main(); } catch (err) { process.stderr.write(`textdiaet-rule: ${err.message}\n`); process.exitCode = 2; }
}

export { kindOf };
