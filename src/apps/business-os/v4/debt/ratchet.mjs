#!/usr/bin/env node
// Ratchet für die Shell-v4-Löschliste (Strang DEBT).
//
// Fährt guard-v4 (--json) und den Port-Scanner (scan.mjs --json) über alle Apps (modules/, customer-modules/,
// desktop-apps/) und vergleicht die Treffer je Regel und je App mit ratchet-baseline.json.
// Exit 1, wenn irgendeine Zahl über der Baseline liegt oder eine App aus v4/tools/guards/ported.json Treffer hat.
// --update schreibt die neue Baseline, aber nur, wenn keine Zahl gestiegen ist (Ratchet: nur abwärts).
//
//   node ratchet.mjs [--json] [--update [--force]] [--baseline FILE] [--ported FILE] [--current FILE]
//
//   --update      Baseline auf den heutigen (gleichen oder niedrigeren) Stand setzen.
//   --force       mit --update: auch bei gestiegenen Zahlen schreiben (nur nach Owner-Freigabe, z. B. neue Regel).
//   --current F   statt guard/scan einen bereits normalisierten Stand lesen (Tests, Fehlersuche).
//   --json        Ergebnis maschinenlesbar.
//
// Exit 0 = gleich oder besser, 1 = gestiegen oder portierte App mit Treffern, 2 = Werkzeugfehler.
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const BOS = path.resolve(HERE, '..', '..');
export const GUARD = path.join(BOS, 'v4', 'tools', 'guards', 'guard-v4.mjs');
export const SCAN = path.join(BOS, 'v4', 'tools', 'port', 'scan.mjs');
export const DEFAULT_PORTED = path.join(BOS, 'v4', 'tools', 'guards', 'ported.json');
export const DEFAULT_BASELINE = path.join(HERE, 'ratchet-baseline.json');
export const SCHEMA = 'shell-v4-ratchet-v1';
const APP_ROOTS = ['modules', 'customer-modules', 'desktop-apps'];
const MAX_BUFFER = 256 * 1024 * 1024;

function runJson(args) {
  const res = spawnSync(process.execPath, args, { cwd: BOS, encoding: 'utf8', maxBuffer: MAX_BUFFER });
  if (res.error) throw res.error;
  // guard-v4 endet mit 1 bei Treffern in portierten Apps, scan.mjs mit 1 bei offenen Stellen: beides ist Messung.
  if (res.status !== 0 && res.status !== 1) {
    throw new Error(`${path.basename(args[0])} Exit ${res.status}: ${String(res.stderr).slice(0, 400)}`);
  }
  try {
    return JSON.parse(res.stdout);
  } catch (err) {
    throw new Error(`${path.basename(args[0])} lieferte kein JSON: ${err.message}`);
  }
}

// Ein App-Verzeichnis zählt, wenn es module.json hat oder unter desktop-apps/ liegt (dort gibt es keins).
function appDirs(root) {
  const abs = path.join(BOS, root);
  if (!existsSync(abs)) return [];
  return readdirSync(abs)
    .filter((name) => statSync(path.join(abs, name)).isDirectory())
    .filter((name) => root === 'desktop-apps' || existsSync(path.join(abs, name, 'module.json')))
    .sort();
}

export function normalizeScan(results, prefix) {
  const out = {};
  for (const r of results) {
    const rules = {};
    for (const f of r.findings || []) {
      if (f.rule === 'R16-css') continue;
      rules[f.rule] = (rules[f.rule] || 0) + 1;
    }
    if (r.cssUnscoped) rules['R16-css'] = r.cssUnscoped;
    out[`${prefix}/${r.app}`] = rules;
  }
  return out;
}

export function normalizeGuard(g) {
  const byGroup = {};
  for (const [group, entry] of Object.entries(g.byGroup || {})) byGroup[group] = { ...(entry.rules || {}) };
  return {
    total: g.total || 0,
    byRule: { ...(g.byRule || {}) },
    byGroup,
    portedHits: g.portedHits || 0,
    portedHitList: g.portedHitList || [],
  };
}

export function collect() {
  const guard = normalizeGuard(runJson([GUARD, '--json']));
  const scan = {};
  for (const root of APP_ROOTS) {
    const dirs = appDirs(root);
    if (!dirs.length) continue;
    if (root === 'desktop-apps') {
      for (const app of dirs) Object.assign(scan, normalizeScan(runJson([SCAN, '--modules', root, '--app', app, '--json']), root));
    } else {
      Object.assign(scan, normalizeScan(runJson([SCAN, '--modules', root, '--json']), root));
    }
  }
  return snapshot(guard, scan);
}

export function snapshot(guard, scan) {
  const scanTotal = Object.values(scan).reduce((a, rules) => a + Object.entries(rules)
    .filter(([rule]) => rule !== 'R16-css').reduce((b, [, n]) => b + n, 0), 0);
  return {
    schema: SCHEMA,
    guard: { total: guard.total, byRule: guard.byRule, byGroup: guard.byGroup },
    scan: { total: scanTotal, byApp: scan },
    _ported: { guardHits: guard.portedHits || 0, guardHitList: guard.portedHitList || [] },
  };
}

// Alle Zähler als flache Liste [schlüssel, wert]. Fehlende Zähler gelten als 0.
export function flatten(snap) {
  const out = new Map();
  out.set('guard.total', snap.guard.total || 0);
  for (const [rule, n] of Object.entries(snap.guard.byRule || {})) out.set(`guard.rule.${rule}`, n);
  for (const [group, rules] of Object.entries(snap.guard.byGroup || {})) {
    for (const [rule, n] of Object.entries(rules)) out.set(`guard.app.${group}.${rule}`, n);
  }
  out.set('scan.total', snap.scan.total || 0);
  for (const [app, rules] of Object.entries(snap.scan.byApp || {})) {
    for (const [rule, n] of Object.entries(rules)) out.set(`scan.app.${app}.${rule}`, n);
  }
  return out;
}

const isPorted = (key, ported) => ported.some((p) => {
  const pre = String(p).replace(/\/+$/, '');
  return key === pre || key.startsWith(`${pre}/`);
});

export function compare(baseline, current, ported = []) {
  const base = flatten(baseline);
  const cur = flatten(current);
  const increases = [];
  const decreases = [];
  for (const key of new Set([...base.keys(), ...cur.keys()])) {
    const b = base.get(key) || 0;
    const c = cur.get(key) || 0;
    if (c > b) increases.push({ key, baseline: b, current: c });
    else if (c < b) decreases.push({ key, baseline: b, current: c });
  }
  const portedViolations = [];
  for (const [group, rules] of Object.entries(current.guard.byGroup || {})) {
    const n = Object.values(rules).reduce((a, v) => a + v, 0);
    if (n && isPorted(group, ported)) portedViolations.push({ source: 'guard', app: group, hits: n });
  }
  for (const [app, rules] of Object.entries(current.scan.byApp || {})) {
    const n = Object.values(rules).reduce((a, v) => a + v, 0);
    if (n && isPorted(app, ported)) portedViolations.push({ source: 'scan', app, hits: n });
  }
  if (current._ported?.guardHits && !portedViolations.some((v) => v.source === 'guard')) {
    portedViolations.push({ source: 'guard', app: '(ported.json)', hits: current._ported.guardHits });
  }
  const sortKey = (a, b) => a.key.localeCompare(b.key);
  return {
    ok: increases.length === 0 && portedViolations.length === 0,
    increases: increases.sort(sortKey),
    decreases: decreases.sort(sortKey),
    portedViolations,
  };
}

export function loadPorted(file) {
  if (!existsSync(file)) return [];
  const raw = JSON.parse(readFileSync(file, 'utf8'));
  return Array.isArray(raw) ? raw : (Array.isArray(raw.ported) ? raw.ported : []);
}

function toBaseline(snap) {
  const { _ported, ...rest } = snap;
  return { ...rest, measuredAt: new Date().toISOString() };
}

function parseArgs(argv) {
  const opts = { json: false, update: false, force: false, baseline: DEFAULT_BASELINE, ported: DEFAULT_PORTED, current: null };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--json') opts.json = true;
    else if (a === '--update') opts.update = true;
    else if (a === '--force') opts.force = true;
    else if (a === '--baseline') opts.baseline = path.resolve(argv[++i]);
    else if (a === '--ported') opts.ported = path.resolve(argv[++i]);
    else if (a === '--current') opts.current = path.resolve(argv[++i]);
    else if (a === '--help' || a === '-h') opts.help = true;
    else throw new Error(`unbekannte Option ${a}`);
  }
  return opts;
}

function formatHuman(result, current, baselineFile, wrote) {
  const out = [];
  out.push(`ratchet: guard ${current.guard.total} · scan ${current.scan.total} (Baseline ${path.basename(baselineFile)})`);
  for (const x of result.increases) out.push(`  GESTIEGEN ${x.key}: ${x.baseline} -> ${x.current}`);
  for (const v of result.portedViolations) out.push(`  PORTIERT MIT TREFFERN ${v.app} [${v.source}]: ${v.hits}`);
  if (result.decreases.length) out.push(`  gesunken: ${result.decreases.length} Zähler (mit --update festschreiben)`);
  if (wrote) out.push(`  Baseline geschrieben: ${baselineFile}`);
  out.push(result.ok ? 'ratchet: OK' : 'ratchet: FEHLER');
  return out.join('\n') + '\n';
}

export function main(argv = process.argv.slice(2)) {
  const opts = parseArgs(argv);
  if (opts.help) {
    process.stdout.write('node ratchet.mjs [--json] [--update [--force]] [--baseline FILE] [--ported FILE] [--current FILE]\n');
    return 0;
  }
  const current = opts.current ? JSON.parse(readFileSync(opts.current, 'utf8')) : collect();
  current._ported ||= { guardHits: 0, guardHitList: [] };
  const hasBaseline = existsSync(opts.baseline);
  const baseline = hasBaseline ? JSON.parse(readFileSync(opts.baseline, 'utf8')) : null;
  if (baseline && baseline.schema !== SCHEMA) throw new Error(`Baseline-Schema ${baseline.schema} != ${SCHEMA}`);
  const ported = loadPorted(opts.ported);
  const result = baseline
    ? compare(baseline, current, ported)
    : { ...compare({ guard: { total: 0, byRule: {}, byGroup: {} }, scan: { total: 0, byApp: {} } }, current, ported), increases: [], ok: true };
  if (!baseline && !opts.update) {
    result.ok = false;
    result.error = `keine Baseline unter ${opts.baseline} (erst --update)`;
  }
  let wrote = false;
  if (opts.update) {
    if (result.increases.length && !opts.force) {
      result.ok = false;
      result.error = 'Baseline nicht geschrieben: Zahlen gestiegen (nur abwärts, --force nur nach Owner-Freigabe)';
    } else {
      writeFileSync(opts.baseline, JSON.stringify(toBaseline(current), null, 2) + '\n');
      wrote = true;
      if (opts.force) result.increases = [];
      result.ok = result.portedViolations.length === 0;
    }
  }
  if (opts.json) process.stdout.write(JSON.stringify({ ...result, wrote, guardTotal: current.guard.total, scanTotal: current.scan.total }, null, 2) + '\n');
  else process.stdout.write(formatHuman(result, current, opts.baseline, wrote) + (result.error ? `  ${result.error}\n` : ''));
  return result.ok ? 0 : 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    process.exitCode = main();
  } catch (err) {
    process.stderr.write(`ratchet: ${err.message}\n`);
    process.exitCode = 2;
  }
}
