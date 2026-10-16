#!/usr/bin/env node
// Origin: CTOX
// License: Apache-2.0
//
// Shell-v4 CI guard "keine App-Logik im Core". Reads every v4 app manifest and
// reports each capabilities_required entry that is NOT a generic CTOX-OS primitive
// (catalog.mjs / CATALOG.md). App-logic and platform capabilities are violations:
// in the target they must become app handlers over collections (app logic) or move
// to the ctox-platform manifest (platform), never a core ctx.cap.* the app requests.
//
// Exit 0 only when there are no violations outside the ratchet baseline.
// Exit 1 otherwise (prints the offenders and the primitive/home they must move to).
//
//   node check-caps.mjs                 guard all manifests, honour baseline
//   node check-caps.mjs --summary       print OS/app/platform counts, no failure
//   node check-caps.mjs --no-baseline   fail on every non-OS capability (Z9 target)
//   node check-caps.mjs --write-baseline rewrite check-caps.baseline.json from today
//   node check-caps.mjs --manifests DIR override manifest directory
//   node check-caps.mjs --calls         Aufrufprüfung: jeder ctx.cap.*/callCap-Aufruf in
//                                       modules|customer-modules/*/handlers gegen den
//                                       Kernvertrag (core-contract.mjs, CAP-1+CAP-2).
//                                       Exit 1 bei ABWEICHUNG (mit --strict auch bei HUELLE).
//   node check-caps.mjs --calls-md FILE schreibt die Aufruftabelle als Markdown
//
// The baseline mirrors guard-v4.mjs: a ratchet of today's tolerated app-logic caps
// that must shrink to empty by Z9. A cap NOT in the baseline fails immediately.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { classify, PRIMITIVE_ORDER } from './catalog.mjs';
import { scanCalls, renderCallsMarkdown } from './scan-calls.mjs';
import { CONTRACT_SOURCE } from './core-contract.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_MANIFESTS = path.resolve(HERE, '../manifests');
const BASELINE_FILE = path.join(HERE, 'check-caps.baseline.json');

function arg(flag) {
  const i = process.argv.indexOf(flag);
  return i >= 0 ? (process.argv[i + 1] ?? true) : null;
}
const manifestDir = arg('--manifests') && arg('--manifests') !== true ? String(arg('--manifests')) : DEFAULT_MANIFESTS;
const summaryOnly = Boolean(arg('--summary'));
const noBaseline = Boolean(arg('--no-baseline'));
const writeBaseline = Boolean(arg('--write-baseline'));

// ---- Aufrufprüfung gegen den Kernvertrag (eigener Modus, eigener Exit-Code) ----
if (arg('--calls') || arg('--calls-md')) {
  const bosRoot = path.resolve(HERE, '../..');
  const rows = scanCalls(bosRoot);
  const mdTarget = arg('--calls-md');
  if (mdTarget && mdTarget !== true) {
    const file = path.resolve(process.cwd(), String(mdTarget));
    const head = fs.existsSync(file) ? fs.readFileSync(file, 'utf8') : '';
    const i = head.indexOf('<!-- BEGIN GENERATED');
    let snapshot = '';
    try {
      const { execSync } = await import('node:child_process');
      const rev = execSync('git rev-parse --short HEAD', { cwd: HERE, stdio: ['ignore', 'pipe', 'ignore'] }).toString().trim();
      snapshot = `Klon-HEAD ${rev}, Arbeitsbaum zum Zeitpunkt des Laufs (Handler ändern sich laufend; neu erzeugen mit --calls-md)`;
    } catch { snapshot = 'Arbeitsbaum zum Zeitpunkt des Laufs'; }
    fs.writeFileSync(file, (i >= 0 ? head.slice(0, i) : head) + renderCallsMarkdown(rows, CONTRACT_SOURCE, snapshot));
    console.log(`wrote ${rows.length} calls to ${path.relative(process.cwd(), file)}`);
  }
  const count = (c) => rows.filter((r) => r.cls === c).length;
  const strict = Boolean(arg('--strict'));
  const failing = rows.filter((r) => r.cls === 'ABWEICHUNG' || (strict && r.cls === 'HUELLE'));
  if (!mdTarget || mdTarget === true) {
    for (const r of failing) console.error(`  ${r.cls.padEnd(10)} ${r.file}:${r.line}  ${r.ist}\n             Soll: ${r.soll}`);
  }
  console.log(`calls: ${rows.length} · ABWEICHUNG ${count('ABWEICHUNG')} · HUELLE ${count('HUELLE')} · UNGEPRUEFT ${count('UNGEPRUEFT')} · OK ${count('OK')} · OHNE_VERTRAG ${count('OHNE_VERTRAG')}`);
  process.exit(failing.length ? 1 : 0);
}

function readManifests(dir) {
  const rows = [];
  for (const f of fs.readdirSync(dir).sort()) {
    if (!f.endsWith('.manifest.json')) continue;
    const m = JSON.parse(fs.readFileSync(path.join(dir, f), 'utf8'));
    for (const cap of m.capabilities_required || []) {
      rows.push({ app: m.app_id ?? f.replace('.manifest.json', ''), cap });
    }
  }
  return rows;
}

const rows = readManifests(manifestDir);
const osByCap = new Map();
const violations = []; // {app, cap, kind, home}
const seen = new Set();

for (const { app, cap } of rows) {
  const c = classify(cap);
  if (c.kind === 'os') {
    osByCap.set(cap, c.primitive);
  } else {
    violations.push({ app, cap, kind: c.kind, home: c.kind === 'app' ? (c.app || '?') : c.kind === 'platform' ? 'ctox-platform' : 'UNKNOWN' });
  }
  seen.add(cap);
}

// Summary counts (distinct capabilities).
const distinct = [...seen];
const osCaps = distinct.filter((c) => classify(c).kind === 'os');
const appCaps = distinct.filter((c) => classify(c).kind === 'app');
const platCaps = distinct.filter((c) => classify(c).kind === 'platform');
const unknownCaps = distinct.filter((c) => classify(c).kind === 'unknown');

if (summaryOnly) {
  const byPrim = {};
  for (const c of osCaps) { const p = classify(c).primitive; (byPrim[p] = byPrim[p] || []).push(c); }
  console.log(`manifests: ${manifestDir}`);
  console.log(`distinct capabilities: ${distinct.length}`);
  console.log(`  OS primitive:  ${osCaps.length}`);
  console.log(`  app logic:     ${appCaps.length}`);
  console.log(`  platform:      ${platCaps.length}`);
  console.log(`  UNKNOWN:       ${unknownCaps.length}`);
  console.log('OS primitives in use:');
  for (const p of PRIMITIVE_ORDER) if (byPrim[p]) console.log(`  ${p.padEnd(14)} ${byPrim[p].length}`);
  process.exit(0);
}

if (writeBaseline) {
  const base = [...new Set(violations.map((v) => v.cap))].sort();
  fs.writeFileSync(BASELINE_FILE, JSON.stringify({ _comment: 'Ratchet: tolerated non-OS capabilities pending app-port. Target Z9 = []', tolerated: base }, null, 2) + '\n');
  console.log(`wrote ${base.length} tolerated capabilities to ${path.relative(HERE, BASELINE_FILE)}`);
  process.exit(0);
}

let tolerated = new Set();
if (!noBaseline && fs.existsSync(BASELINE_FILE)) {
  tolerated = new Set((JSON.parse(fs.readFileSync(BASELINE_FILE, 'utf8')).tolerated) || []);
}

const hard = violations.filter((v) => v.kind === 'unknown' || !tolerated.has(v.cap));
const byCap = new Map();
for (const v of hard) {
  if (!byCap.has(v.cap)) byCap.set(v.cap, { kind: v.kind, home: v.home, apps: new Set() });
  byCap.get(v.cap).apps.add(v.app);
}

if (byCap.size === 0) {
  console.log(`check-caps: OK — ${osCaps.length} OS-primitive capabilities, ${tolerated.size} app/platform caps within ratchet baseline.`);
  process.exit(0);
}

console.error('check-caps: FAIL — App-Logik/Plattform-Capability im Core angefordert (muss in App-Handler bzw. ctox-platform):');
for (const cap of [...byCap.keys()].sort()) {
  const v = byCap.get(cap);
  const tag = v.kind === 'app' ? `app:${v.home}` : v.kind === 'platform' ? 'platform:ctox-platform' : 'UNKNOWN';
  console.error(`  ${cap}  [${tag}]  <- ${[...v.apps].sort().join(', ')}`);
}
console.error(`\n${byCap.size} capability(ies) outside the baseline. Port to app handlers / platform, or (transition only) add to check-caps.baseline.json.`);
process.exit(1);
