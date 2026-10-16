#!/usr/bin/env node
// Shell v4 – Strang SKL0: Wächter für Agenten-Starts (Kern §16.30).
//
// Meldet in modules/*/ und customer-modules/*/ (ohne Tests, vendor/, dist/):
//   phantom-skill      Skill-Name, zu dem es keine Paketdatei gibt: weder System-Skill des Core
//                      (src/skills/**/SKILL.md) noch `<app>/<name>` mit skills/<name>/SKILL.md im eigenen Paket
//                      (Beispiele heute: knowledge, business-os-support-workflow).
//   foreign-namespace  `<andere-app>/<name>`: Apps dürfen nur Skills des eigenen Namensraums nennen.
//   enqueue-no-callback  agent.task.enqueue ohne on_complete bzw. ohne on_failed (Handler-Code).
//   enqueue-unchecked    agent.task.enqueue, dessen Argument kein Objektliteral ist oder `...` enthält.
// Skill-Verweise erkennt extractSkillRefs (gen-manifest.mjs); enqueue-Aufrufe scanCalls (os-primitives/scan-calls.mjs).
//
// Ratchet wie check-caps.mjs: Befunde in check-skills.baseline.json sind geduldet (Ziel []), jeder neue Befund → Exit 1.
// Schlüssel ohne Zeilennummer (art|app|datei|detail), damit Verschiebungen im Code die Baseline nicht brechen.
//
//   node check-skills.mjs                  prüfen (Baseline beachten)
//   node check-skills.mjs --no-baseline    jeder Befund ist ein Fehler (Zielzustand)
//   node check-skills.mjs --write-baseline Baseline aus dem heutigen Stand schreiben
//   node check-skills.mjs --json           Befunde als JSON
//   Optionen: --root <business-os-Ordner> --repo <Repo-Wurzel> --baseline <datei>

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { extractSkillRefs, isTestPath } from '../manifest/gen-manifest.mjs';
import { scanCalls } from '../../os-primitives/scan-calls.mjs';
import { loadCoreSkillNames } from './agent-task-sim.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const DEFAULT_ROOT = path.resolve(HERE, '../../..');
export const DEFAULT_BASELINE = path.join(HERE, 'check-skills.baseline.json');
const ROOTS = ['modules', 'customer-modules'];
const SKIP_DIRS = new Set(['node_modules', 'vendor', 'dist', '.git']);

function walk(dir, base, acc = []) {
  let entries;
  try { entries = fs.readdirSync(dir, { withFileTypes: true }); } catch { return acc; }
  for (const e of entries.sort((a, b) => a.name.localeCompare(b.name))) {
    if (e.isSymbolicLink()) continue;
    const abs = path.join(dir, e.name);
    if (e.isDirectory()) { if (!SKIP_DIRS.has(e.name)) walk(abs, base, acc); continue; }
    if (/\.(m?js)$/.test(e.name)) acc.push(path.relative(base, abs).split(path.sep).join('/'));
  }
  return acc;
}

// Erstes Argument ab `start` (nach '(') bis Komma/Klammer auf Ebene 0, Strings übersprungen.
function firstArg(src, start) {
  let depth = 0;
  let quote = null;
  let i = start;
  for (; i < src.length; i += 1) {
    const c = src[i];
    if (quote) { if (c === '\\') i += 1; else if (c === quote) quote = null; continue; }
    if (c === '"' || c === "'" || c === '`') { quote = c; continue; }
    if ('({['.includes(c)) depth += 1;
    else if (')}]'.includes(c)) { if (depth === 0) break; depth -= 1; } else if (c === ',' && depth === 0) break;
  }
  return src.slice(start, i).trim();
}

// Oberste Schlüssel eines Objektliterals (Kurzschreibweise eingeschlossen); null, wenn kein Literal.
export function objectKeys(text) {
  if (!text.startsWith('{')) return null;
  const body = text.slice(1, text.lastIndexOf('}'));
  const keys = [];
  let spread = false;
  let depth = 0;
  let quote = null;
  let seg = '';
  const flush = () => {
    const t = seg.trim();
    seg = '';
    if (!t) return;
    if (t.startsWith('...')) { spread = true; return; }
    const m = /^(?:['"]([\w$-]+)['"]|([A-Za-z_$][\w$]*))\s*(?::|$|\()/.exec(t);
    if (m) keys.push(m[1] ?? m[2]);
  };
  for (let i = 0; i < body.length; i += 1) {
    const c = body[i];
    if (quote) { if (c === '\\') { seg += c + (body[i + 1] ?? ''); i += 1; continue; } if (c === quote) quote = null; seg += c; continue; }
    if (c === '"' || c === "'" || c === '`') { quote = c; seg += c; continue; }
    if ('({['.includes(c)) depth += 1;
    else if (')}]'.includes(c)) depth -= 1;
    if (c === ',' && depth === 0) { flush(); continue; }
    seg += c;
  }
  flush();
  return { keys, spread };
}

// Position des Aufrufs in der Zeile `line` finden und das erste Argument lesen.
function enqueueArgAt(src, line, via) {
  const lines = src.split('\n');
  const offset = lines.slice(0, line - 1).reduce((n, l) => n + l.length + 1, 0);
  const rest = src.slice(offset);
  const re = via === 'callCap' ? /['"]agent\.task\.enqueue['"]\s*,/ : /\benqueue\s*\(|\]\s*\)\s*\(/;
  const m = re.exec(rest);
  if (!m) return null;
  return firstArg(src, offset + m.index + m[0].length);
}

/** Alle Befunde unter `root` (business-os-Ordner). */
export function scanSkills(root = DEFAULT_ROOT, { coreSkills } = {}) {
  const core = coreSkills ? new Set(coreSkills) : loadCoreSkillNames(path.resolve(root, '../../..'));
  const findings = [];
  const seen = new Set();
  const add = (f) => {
    const key = [f.kind, f.app, f.file, f.detail].join('|');
    if (seen.has(`${key}@${f.line}`)) return;
    seen.add(`${key}@${f.line}`);
    findings.push({ ...f, key });
  };
  for (const top of ROOTS) {
    const base = path.join(root, top);
    if (!fs.existsSync(base)) continue;
    for (const app of fs.readdirSync(base).sort()) {
      const appDir = path.join(base, app);
      if (!fs.statSync(appDir).isDirectory()) continue;
      for (const rel of walk(appDir, appDir)) {
        if (isTestPath(rel)) continue;
        const file = `${top}/${app}/${rel}`;
        for (const r of extractSkillRefs(fs.readFileSync(path.join(appDir, rel), 'utf8'))) {
          if (r.ref.includes('/')) {
            const [ns, name] = r.ref.split('/');
            if (ns !== app) add({ kind: 'foreign-namespace', app, file, line: r.line, detail: r.ref });
            else if (!fs.existsSync(path.join(appDir, 'skills', name, 'SKILL.md'))) add({ kind: 'phantom-skill', app, file, line: r.line, detail: r.ref });
          } else if (!core.has(r.ref)) {
            add({ kind: 'phantom-skill', app, file, line: r.line, detail: r.ref });
          }
        }
      }
    }
  }
  for (const row of scanCalls(root, ROOTS)) {
    if (row.path !== 'agent.task.enqueue') continue;
    const src = fs.readFileSync(path.join(root, row.file), 'utf8');
    const arg = enqueueArgAt(src, row.line, row.via);
    const shape = arg ? objectKeys(arg) : null;
    const base = { app: row.app, file: row.file, line: row.line };
    if (!shape || shape.spread) { add({ ...base, kind: 'enqueue-unchecked', detail: shape ? 'spread' : 'kein Objektliteral' }); continue; }
    for (const k of ['on_complete', 'on_failed']) if (!shape.keys.includes(k)) add({ ...base, kind: 'enqueue-no-callback', detail: k });
  }
  findings.sort((a, b) => a.key.localeCompare(b.key) || a.line - b.line);
  return findings;
}

export function readBaseline(file = DEFAULT_BASELINE) {
  return fs.existsSync(file) ? new Set(JSON.parse(fs.readFileSync(file, 'utf8')).tolerated || []) : new Set();
}

/** Ratchet: neu = Befund außerhalb der Baseline; erledigt = Baseline-Eintrag ohne Befund. */
export function judge(findings, tolerated) {
  const keys = new Set(findings.map((f) => f.key));
  return {
    fresh: findings.filter((f) => !tolerated.has(f.key)),
    resolved: [...tolerated].filter((k) => !keys.has(k)).sort(),
  };
}

function parseArgs(argv) {
  const a = { root: DEFAULT_ROOT, baseline: DEFAULT_BASELINE };
  for (let i = 0; i < argv.length; i += 1) {
    const x = argv[i];
    if (x === '--root') a.root = path.resolve(argv[++i]);
    else if (x === '--baseline') a.baseline = path.resolve(argv[++i]);
    else if (x === '--no-baseline') a.noBaseline = true;
    else if (x === '--write-baseline') a.write = true;
    else if (x === '--json') a.json = true;
    else throw new Error(`unbekannte Option ${x}`);
  }
  return a;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const findings = scanSkills(args.root);
  if (args.write) {
    const tolerated = [...new Set(findings.map((f) => f.key))].sort();
    fs.writeFileSync(args.baseline, `${JSON.stringify({ _comment: 'Ratchet check-skills.mjs (Kern §16.30): geduldete Befunde art|app|datei|detail. Ziel [].', tolerated }, null, 2)}\n`);
    console.log(`check-skills: ${tolerated.length} Befunde als Baseline geschrieben`);
    return 0;
  }
  const { fresh, resolved } = judge(findings, args.noBaseline ? new Set() : readBaseline(args.baseline));
  if (args.json) console.log(JSON.stringify({ findings, fresh, resolved }, null, 2));
  const count = (k) => findings.filter((f) => f.kind === k).length;
  for (const f of fresh) console.error(`  NEU ${f.kind.padEnd(19)} ${f.file}:${f.line}  ${f.detail}`);
  for (const k of resolved) console.log(`  erledigt (Baseline verkleinern mit --write-baseline): ${k}`);
  console.log(`check-skills: ${findings.length} Befunde (phantom-skill ${count('phantom-skill')}, foreign-namespace ${count('foreign-namespace')}, enqueue-no-callback ${count('enqueue-no-callback')}, enqueue-unchecked ${count('enqueue-unchecked')}) · neu ${fresh.length}`);
  return fresh.length ? 1 : 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) process.exit(main());
