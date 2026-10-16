// node --test src/apps/business-os/v4/debt/ratchet.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { compare, flatten, normalizeScan, normalizeGuard, snapshot, SCHEMA, DEFAULT_BASELINE } from './ratchet.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const RATCHET = path.join(HERE, 'ratchet.mjs');

function snap({ guardGroups = {}, scan = {}, portedHits = 0 } = {}) {
  const byRule = {};
  let total = 0;
  for (const rules of Object.values(guardGroups)) {
    for (const [rule, n] of Object.entries(rules)) { byRule[rule] = (byRule[rule] || 0) + n; total += n; }
  }
  const byGroup = Object.fromEntries(Object.entries(guardGroups).map(([g, rules]) => [g, { rules }]));
  return snapshot(normalizeGuard({ total, byRule, byGroup, portedHits }), scan);
}

const BASE = snap({
  guardGroups: { 'modules/news': { commandBus: 2, 'cache-buster': 1 }, 'shell/app.js': { advancedStatus: 5 } },
  scan: { 'modules/news': { 'R5-command': 2 }, 'customer-modules/olg': { 'R3-db': 4, 'R16-css': 10 } },
});

function tmp() { return mkdtempSync(path.join(tmpdir(), 'ratchet-')); }
function run(args) { return spawnSync(process.execPath, [RATCHET, ...args], { encoding: 'utf8' }); }

test('gleicher Stand: ok, keine Änderung', () => {
  const r = compare(BASE, BASE, []);
  assert.equal(r.ok, true);
  assert.deepEqual(r.increases, []);
  assert.deepEqual(r.decreases, []);
});

test('Anstieg einer Regel in einer App bricht (guard und scan getrennt)', () => {
  const cur = snap({
    guardGroups: { 'modules/news': { commandBus: 3, 'cache-buster': 1 }, 'shell/app.js': { advancedStatus: 4 } },
    scan: { 'modules/news': { 'R5-command': 2 }, 'customer-modules/olg': { 'R3-db': 4, 'R16-css': 10 } },
  });
  const r = compare(BASE, cur, []);
  assert.equal(r.ok, false);
  const keys = r.increases.map((x) => x.key);
  assert.ok(keys.includes('guard.app.modules/news.commandBus'));
  assert.ok(keys.includes('guard.rule.commandBus'));
  // Summe gleich (eine Regel hoch, eine runter) darf den Anstieg nicht verdecken.
  assert.ok(!keys.includes('guard.total'));
  assert.ok(r.decreases.some((x) => x.key === 'guard.app.shell/app.js.advancedStatus'));
});

test('neue Regel oder neue App mit Treffern zählt als Anstieg von 0', () => {
  const cur = snap({
    guardGroups: { 'modules/news': { commandBus: 2, 'cache-buster': 1 }, 'shell/app.js': { advancedStatus: 5 }, 'modules/neu': { 'location-reload': 1 } },
    scan: { 'modules/news': { 'R5-command': 2, 'R12-timer': 1 }, 'customer-modules/olg': { 'R3-db': 4, 'R16-css': 10 } },
  });
  const keys = compare(BASE, cur, []).increases.map((x) => x.key);
  assert.ok(keys.includes('guard.app.modules/neu.location-reload'));
  assert.ok(keys.includes('scan.app.modules/news.R12-timer'));
  assert.ok(keys.includes('scan.total'));
});

test('CSS ungescoped (R16) ist ein eigener Zähler und darf nicht steigen', () => {
  const cur = snap({
    guardGroups: { 'modules/news': { commandBus: 2, 'cache-buster': 1 }, 'shell/app.js': { advancedStatus: 5 } },
    scan: { 'modules/news': { 'R5-command': 2 }, 'customer-modules/olg': { 'R3-db': 4, 'R16-css': 11 } },
  });
  const r = compare(BASE, cur, []);
  assert.deepEqual(r.increases.map((x) => x.key), ['scan.app.customer-modules/olg.R16-css']);
});

test('portierte App mit Treffern bricht, auch ohne Anstieg', () => {
  const r = compare(BASE, BASE, ['modules/news']);
  assert.equal(r.ok, false);
  assert.deepEqual(r.portedViolations.map((v) => `${v.source}:${v.app}`).sort(), ['guard:modules/news', 'scan:modules/news']);
  // Präfix trifft nicht fremde Apps mit gleichem Anfang.
  assert.equal(compare(BASE, BASE, ['modules/new']).ok, true);
});

test('portierte App ohne Treffer ist ok', () => {
  const cur = snap({ guardGroups: { 'shell/app.js': { advancedStatus: 5 } }, scan: { 'modules/news': {} } });
  assert.equal(compare(BASE, cur, ['modules/news']).ok, true);
});

test('normalizeScan zählt Regeln je App ohne R16-Funde, R16 aus cssUnscoped', () => {
  const out = normalizeScan([{ app: 'x', cssUnscoped: 3, findings: [{ rule: 'R3-db' }, { rule: 'R3-db' }, { rule: 'R16-css' }] }], 'modules');
  assert.deepEqual(out, { 'modules/x': { 'R3-db': 2, 'R16-css': 3 } });
  assert.equal(flatten(snapshot(normalizeGuard({}), out)).get('scan.total'), 2);
});

test('CLI: Anstieg → Exit 1, --update verweigert; Rückgang → --update schreibt niedrigere Baseline', () => {
  const dir = tmp();
  const baseline = path.join(dir, 'base.json');
  const ported = path.join(dir, 'ported.json');
  const up = path.join(dir, 'up.json');
  const down = path.join(dir, 'down.json');
  writeFileSync(ported, JSON.stringify({ ported: [] }));
  writeFileSync(up, JSON.stringify(snap({ guardGroups: { 'modules/news': { commandBus: 3, 'cache-buster': 1 }, 'shell/app.js': { advancedStatus: 5 } }, scan: BASE.scan.byApp })));
  writeFileSync(down, JSON.stringify(snap({ guardGroups: { 'modules/news': { commandBus: 1 }, 'shell/app.js': { advancedStatus: 5 } }, scan: BASE.scan.byApp })));
  const init = path.join(dir, 'init.json');
  writeFileSync(init, JSON.stringify(BASE));

  let r = run(['--baseline', baseline, '--ported', ported, '--current', init]);
  assert.equal(r.status, 1, 'ohne Baseline kein grünes Ergebnis');
  r = run(['--update', '--baseline', baseline, '--ported', ported, '--current', init]);
  assert.equal(r.status, 0, r.stdout + r.stderr);
  assert.equal(JSON.parse(readFileSync(baseline, 'utf8')).schema, SCHEMA);

  r = run(['--baseline', baseline, '--ported', ported, '--current', up]);
  assert.equal(r.status, 1);
  assert.match(r.stdout, /GESTIEGEN guard\.app\.modules\/news\.commandBus: 2 -> 3/);
  r = run(['--update', '--baseline', baseline, '--ported', ported, '--current', up]);
  assert.equal(r.status, 1);
  assert.equal(JSON.parse(readFileSync(baseline, 'utf8')).guard.total, 8, 'Baseline unverändert');

  r = run(['--update', '--json', '--baseline', baseline, '--ported', ported, '--current', down]);
  assert.equal(r.status, 0, r.stdout + r.stderr);
  assert.equal(JSON.parse(r.stdout).wrote, true);
  assert.equal(JSON.parse(readFileSync(baseline, 'utf8')).guard.total, 6);
  r = run(['--baseline', baseline, '--ported', ported, '--current', init]);
  assert.equal(r.status, 1, 'nach dem Absenken bricht der alte (höhere) Stand');
});

test('CLI: portierte App mit Treffern → Exit 1', () => {
  const dir = tmp();
  const baseline = path.join(dir, 'base.json');
  const ported = path.join(dir, 'ported.json');
  const cur = path.join(dir, 'cur.json');
  writeFileSync(cur, JSON.stringify(BASE));
  writeFileSync(baseline, JSON.stringify(BASE));
  writeFileSync(ported, JSON.stringify({ ported: ['customer-modules/olg'], statusZoneFiles: [] }));
  const r = run(['--baseline', baseline, '--ported', ported, '--current', cur]);
  assert.equal(r.status, 1);
  assert.match(r.stdout, /PORTIERT MIT TREFFERN customer-modules\/olg \[scan\]: 14/);
});

test('eingecheckte Baseline ist gültig und deckt guard und alle App-Wurzeln ab', () => {
  assert.ok(existsSync(DEFAULT_BASELINE));
  const b = JSON.parse(readFileSync(DEFAULT_BASELINE, 'utf8'));
  assert.equal(b.schema, SCHEMA);
  assert.ok(b.guard.total > 0 && Object.keys(b.guard.byGroup).length > 0);
  const apps = Object.keys(b.scan.byApp);
  assert.ok(apps.some((a) => a.startsWith('modules/')));
  assert.ok(apps.some((a) => a.startsWith('customer-modules/')));
  assert.ok(apps.some((a) => a.startsWith('desktop-apps/')));
});

test('voller Lauf gegen den Baum: Werkzeuge liefern JSON, Ergebnis ist 0 oder 1, nie Werkzeugfehler', () => {
  const r = run(['--json']);
  assert.ok(r.status === 0 || r.status === 1, r.stderr);
  const out = JSON.parse(r.stdout);
  assert.equal(typeof out.guardTotal, 'number');
  assert.equal(typeof out.scanTotal, 'number');
});
