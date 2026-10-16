// node --test src/apps/business-os/v4/tools/guards/guard-v4.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { scan, scanSource, loadVocabulary, loadConfig, lexJs, looksLikeKey, groupOf, kindOf, IDENTIFIER_RULES, APP_VENDOR_DIR, isEnglishRawTerm, ENGLISH_RAW_TERMS, isTestFile, LAB_RESULTS } from './guard-v4.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const TREE = path.join(HERE, 'fixtures', 'tree');
const GUARD = path.join(HERE, 'guard-v4.mjs');
const cfg = loadConfig(path.join(HERE, 'fixtures', 'ported.json'));
const res = scan({ root: TREE, ...cfg });

const count = (rule, filePrefix) => res.hits.filter((h) => (h.rule === rule || (rule === 'vocab' && h.rule.startsWith('vocab:'))) && (!filePrefix || h.file.startsWith(filePrefix))).length;
const negative = 'modules/negative/';
const positive = 'modules/positive/';

// ------------------------------------------------ Bezeichner-Regeln (§11), je Regel pos/neg

for (const rule of IDENTIFIER_RULES) {
  test(`Regel ${rule.id}: Positivfall zaehlt`, () => {
    const min = rule.id === 'data-guard' ? 2 : 1;
    assert.ok(count(rule.id, positive) >= min, `${rule.id} fehlt in ${positive}`);
  });
  test(`Regel ${rule.id}: Negativfall (Kommentar, String, falsche Wortgrenze) zaehlt nicht`, () => {
    assert.equal(count(rule.id, negative), 0);
  });
}

test('Familien zaehlen genau einmal je Token (restartCollection nicht auch als startCollection)', () => {
  const line8 = res.hits.filter((h) => h.file === `${positive}index.js` && h.line === 8);
  assert.deepEqual(line8.map((h) => h.rule), ['restartCollection']);
  const line12 = res.hits.filter((h) => h.file === `${positive}index.js` && h.line === 12);
  assert.deepEqual(line12.map((h) => h.rule), ['collectionReadiness']);
});

test('Inline-Skript in HTML wird als JS gelesen, Kommentare darin nicht', () => {
  assert.equal(count('syncDiagnostics', 'index.html'), 1);
  assert.equal(count('commandBus', 'index.html'), 0);
});

// ------------------------------------------------ Muster-Regeln

test("until: 'accepted' | 'terminal' – positiv beide Quotes, negativ until: 'local' und Kommentar", () => {
  assert.equal(count('until-accepted-terminal', positive), 2);
  assert.equal(count('until-accepted-terminal', negative), 0);
});

test('location.reload – positiv in App und Shell, negativ im Kommentar', () => {
  assert.equal(count('location-reload', positive), 1);
  assert.equal(count('location-reload', 'app.js'), 1);
  assert.equal(count('location-reload', negative), 0);
});

test('?v= Cache-Buster in JS, HTML und CSS; nicht in Kommentaren', () => {
  assert.equal(count('cache-buster', `${positive}index.js`), 1);
  assert.equal(count('cache-buster', `${positive}index.html`), 1);
  assert.equal(count('cache-buster', `${positive}index.css`), 1);
  assert.equal(count('cache-buster', 'index.html'), 1);
  assert.equal(count('cache-buster', 'app.js'), 1);
  assert.equal(count('cache-buster', negative), 0);
});

test('setInterval: in Apps ohne Deklaration Treffer, mit v4-allow nicht, in shared nicht', () => {
  assert.equal(count('set-interval-undeclared', positive), 1);
  assert.equal(count('set-interval-undeclared', 'desktop-apps/legacy/'), 1);
  assert.equal(count('set-interval-undeclared', negative), 0);
  assert.equal(count('set-interval-undeclared', 'shared/'), 0);
});

test('fetch auf /api, http(s), // in Apps; relative Wege und shared nicht', () => {
  assert.equal(count('fetch-absolute-or-api', positive), 3);
  assert.equal(count('fetch-absolute-or-api', negative), 0);
  assert.equal(count('fetch-absolute-or-api', 'shared/'), 0);
});

test('Altkatalog-Dateien: Existenz je Datei, Verweise in Strings', () => {
  assert.equal(count('file-registry-json'), 1);
  assert.equal(count('file-mobile-apps-json'), 1);
  assert.equal(count('file-system-apps-json'), 1);
  assert.equal(count('file-schema-js'), 1);
  assert.equal(count('ref-legacy-catalog-file', positive), 2);
  assert.equal(count('ref-legacy-catalog-file', negative), 0);
});

// ------------------------------------------------ Zustandsvokabular

test('Vokabular: Positivfaelle aus der Ist-Aufnahme werden gefunden', () => {
  const rules = new Set(res.hits.filter((h) => h.file.startsWith(positive)).map((h) => h.rule));
  for (const id of ['de-synchronisier', 'de-abgleich', 'de-unbestaetigt', 'en-sync', 'de-geladen', 'en-loading', 'de-veraltet-offline', 'de-nicht-rechtzeitig', 'de-laeuft']) {
    assert.ok(rules.has(`vocab:${id}`), `vocab:${id} fehlt`);
  }
});

test('Vokabular: locales-Werte zaehlen, Schluessel und status.*-Schluessel nicht', () => {
  assert.equal(count('vocab', `${positive}locales/de.json`), 2);
  assert.equal(count('vocab', `${negative}locales/de.json`), 0);
});

test('Vokabular: api.status.report, console.*, Kommentare, Schluesselform, HTML-Kommentar, CSS -> keine Treffer', () => {
  assert.equal(count('vocab', negative), 0);
});

test('Vokabular: Dateien der Zustandszone (statusZoneFiles) sind ausgenommen', () => {
  assert.equal(count('vocab', 'v4/shell/status-zone.js'), 0);
  const without = scan({ root: TREE, ported: [], statusZoneFiles: [] });
  assert.ok(without.hits.some((h) => h.file === 'v4/shell/status-zone.js' && h.rule.startsWith('vocab:')));
});

test('looksLikeKey: Schluessel/Codes ja, Saetze und gross geschriebene Einzelwoerter nein', () => {
  for (const k of ['syncing', 'sync.loading', 'loadingModule', 'read-only', 'ctox-syncing']) assert.equal(looksLikeKey(k), true, k);
  for (const t of ['Loading', 'Offline', 'Läuft', 'Daten werden synchronisiert.']) assert.equal(looksLikeKey(t), false, t);
});

// ------------------------------------------------ Dateiauswahl, Gruppen, Tests, Exitcode

test('vendor/, rxdb/ und v4/tools/ werden nicht gelesen', () => {
  assert.equal(res.hits.filter((h) => /^(vendor|rxdb|v4\/tools)\//.test(h.file)).length, 0);
});

test('App-Vendor modules/<id>/vendor/** und desktop-apps/<id>/vendor/** (Fremdcode) werden nicht gelesen', () => {
  assert.equal(res.hits.filter((h) => /^(modules|desktop-apps|customer-modules)\/[^/]+\/vendor\//.test(h.file)).length, 0);
  assert.equal(APP_VENDOR_DIR.test('modules/notes/vendor'), true);
  assert.equal(APP_VENDOR_DIR.test('customer-modules/outbound-lead-generation/vendor'), true);
  assert.equal(APP_VENDOR_DIR.test('modules/notes/ui/vendor'), false);
  assert.equal(APP_VENDOR_DIR.test('shared/vendor'), false);
});

test('Testdateien zaehlen getrennt und mit --include-tests mit', () => {
  assert.equal(res.hits.filter((h) => h.file.endsWith('.test.js')).length, 0);
  assert.equal(res.testHitsSeparate, 1);
  const withTests = scan({ root: TREE, ...cfg, includeTests: true });
  assert.equal(withTests.total, res.total + 1);
});

test('Gruppen: modules/<id>, desktop-apps/<id>, shell/<datei>, shared', () => {
  assert.equal(groupOf('modules/mail/ui/x.js'), 'modules/mail');
  assert.equal(groupOf('desktop-apps/code-editor/app.js'), 'desktop-apps/code-editor');
  assert.equal(groupOf('app.js'), 'shell/app.js');
  assert.equal(groupOf('shared/sync.js'), 'shared');
  assert.equal(kindOf('modules/mail'), 'app');
  assert.equal(kindOf('modules/registry.json'), 'catalog');
  assert.equal(kindOf('scripts'), 'tooling');
});

test('Lexer: Template mit Ausdruck, Regex-Literal, Division', () => {
  const src = "const a = `x ${b ? 'Loading' : `y ${c}`} z`; const r = /'\\/\\//g; const d = e / f / g; // commandBus";
  const lx = lexJs(src);
  assert.equal(lx.code.length, src.length);
  assert.ok(!lx.code.includes('commandBus'));
  const texts = lx.strings.map((s) => src.slice(s.start, s.end));
  assert.ok(texts.includes('Loading'));
  assert.ok(texts.includes('x '));
  assert.ok(lx.noStr.includes('e / f / g'));
});

test('Exitcode: 1 bei Treffern in portierten Pfaden, 0 wenn portierte Pfade sauber sind', () => {
  const bad = spawnSync(process.execPath, [GUARD, '--root', TREE, '--ported', path.join(HERE, 'fixtures', 'ported.json'), '--summary'], { encoding: 'utf8' });
  assert.equal(bad.status, 1, bad.stderr);
  const ok = spawnSync(process.execPath, [GUARD, '--root', TREE, '--ported', path.join(HERE, 'fixtures', 'ported-clean.json'), '--summary'], { encoding: 'utf8' });
  assert.equal(ok.status, 0, ok.stderr);
});

test('--json liefert Summen je Regel und je App sowie Treffer mit Zeilennummer', () => {
  const run = spawnSync(process.execPath, [GUARD, '--root', TREE, '--ported', path.join(HERE, 'fixtures', 'ported-clean.json'), '--json'], { encoding: 'utf8' });
  const out = JSON.parse(run.stdout);
  assert.equal(out.schema, 'shell-v4-guard-v1');
  assert.equal(out.total, out.hits.length);
  assert.ok(out.byRule.commandBus >= 1);
  assert.ok(out.byGroup['modules/positive'].total > 0);
  assert.ok(out.hits.every((h) => Number.isInteger(h.line) && h.line >= 1 && h.file && h.rule));
});

// ------------------------------------------------ FR5-3 english-raw-term (Glossar v4/frame/glossary.js)

test('english-raw-term: Positivfall – JS-Literal, Template, HTML-Text, Wert in locales/de.json', () => {
  const hits = res.hits.filter((h) => h.rule === 'english-raw-term' && h.file.startsWith(positive)).map((h) => `${h.file.slice(positive.length)}:${h.line}:${h.match}`);
  assert.deepEqual(hits.sort(), ['raw-terms.html:2:Won', 'raw-terms.js:2:Open', 'raw-terms.js:3:Writeback Pending', 'raw/locales/de.json:1:Analytics']);
});

test('english-raw-term: Negativfall – Schlüsselform, Kommentar, deutscher Satz, v4-allow: en, Error, locales/en.json', () => {
  assert.equal(count('english-raw-term', negative), 0);
});

test('english-raw-term: Rohwerte kommen aus dem Glossar (Kategorie, Status, App-Titel), identische Wörter nicht', () => {
  for (const t of ['Open', 'Analytics', 'Writeback Pending', 'Conversations', 'Won']) assert.equal(isEnglishRawTerm(t), true, t);
  for (const t of ['open', 'System', 'Marketing', 'Offen', 'Error', 'Mail']) assert.equal(isEnglishRawTerm(t), false, t);
  assert.ok(ENGLISH_RAW_TERMS.size > 50);
});

// ------------------------------------------------ Ausnahmen 'v4-allow: zone' und 'v4-allow: debt L-<nr>' (ZONE45)
test('v4-allow: zone hebt Zustandsvokabular auf (gleiche/vorige Zeile), ohne Marker Treffer', () => {
  const vocab = loadVocabulary();
  const rel = 'v4/shell/zone-probe.js';
  const without = scanSource(rel, "export const T = { warn: 'Verbindung wird wiederhergestellt' };\n", vocab);
  const same = scanSource(rel, "export const T = { warn: 'Verbindung wird wiederhergestellt' }; // v4-allow: zone\n", vocab);
  const prev = scanSource(rel, "// v4-allow: zone\nexport const T = { warn: 'Verbindung unterbrochen' };\n", vocab);
  const wrong = scanSource(rel, "export const T = { warn: 'Verbindung unterbrochen' }; // v4-allow: zonen\n", vocab);
  assert.ok(without.some((h) => h.rule === 'vocab:de-verbindung'));
  assert.equal(same.filter((h) => h.rule.startsWith('vocab:')).length, 0);
  assert.equal(prev.filter((h) => h.rule.startsWith('vocab:')).length, 0);
  assert.ok(wrong.some((h) => h.rule === 'vocab:de-verbindung'), 'nur genau die Kategorie zone');
});
test('v4-allow: debt L-<nr> hebt Bezeichner-Regel auf (gleiche/vorige Zeile), ohne Löschplan-Nummer nicht', () => {
  const rel = 'v4/shell/v3-probe.js';
  const hit = (src) => scanSource(rel, src, null).filter((h) => h.rule === 'syncDiagnostics').length;
  assert.equal(hit('const d = window.ctoxBusinessOsSyncDiagnostics;\n'), 1);
  assert.equal(hit('const d = window.ctoxBusinessOsSyncDiagnostics; // v4-allow: debt L-41\n'), 0);
  assert.equal(hit('// v4-allow: debt L-41 (V3 bis Sa)\nconst d = window.ctoxBusinessOsSyncDiagnostics;\n'), 0);
  assert.equal(hit('const d = window.ctoxBusinessOsSyncDiagnostics; // v4-allow: debt\n'), 1);
  assert.equal(hit('// v4-allow: debt L-41\n\nconst d = window.ctoxBusinessOsSyncDiagnostics;\n'), 1, 'nur gleiche oder vorige Zeile');
});
test("v4-allow: debt L-<nr> hebt until: 'terminal' auf (WJB, L-42), ohne Nummer und mit Leerzeile nicht", () => {
  const rel = 'v4/shell/workjet/probe.js';
  const hit = (src) => scanSource(rel, src, null).filter((h) => h.rule === 'until-accepted-terminal').length;
  assert.equal(hit("d({ until: 'terminal' });\n"), 1);
  assert.equal(hit("d({ until: 'terminal' }); // v4-allow: debt L-42\n"), 0);
  assert.equal(hit("// v4-allow: debt L-42\nd({ until: 'accepted' });\n"), 0);
  assert.equal(hit("d({ until: 'terminal' }); // v4-allow: debt\n"), 1);
  assert.equal(hit("// v4-allow: debt L-42\n\nd({ until: 'terminal' });\n"), 1);
});

// ------------------------------------------------ Ausnahme 'v4-allow: code' und Labor-Messausgaben (GATE)
test('v4-allow: code hebt Zustandsvokabular auf (gleiche/vorige Zeile), ohne Marker und mit falscher Kategorie Treffer', () => {
  const vocab = loadVocabulary();
  const rel = 'v4/os-primitives/code-probe.mjs';
  const line = "  'pentest.exploit_get': 'kein OS-Primitiv (bis Cutover unavailable)',";
  const without = scanSource(rel, `${line}\n`, vocab);
  const same = scanSource(rel, `${line} // v4-allow: code\n`, vocab);
  const prev = scanSource(rel, `// v4-allow: code\n${line}\n`, vocab);
  const wrong = scanSource(rel, `${line} // v4-allow: codes\n`, vocab);
  const far = scanSource(rel, `// v4-allow: code\n\n${line}\n`, vocab);
  assert.ok(without.some((h) => h.rule === 'vocab:en-unavailable'));
  assert.equal(same.filter((h) => h.rule.startsWith('vocab:')).length, 0);
  assert.equal(prev.filter((h) => h.rule.startsWith('vocab:')).length, 0);
  assert.ok(wrong.some((h) => h.rule === 'vocab:en-unavailable'), 'nur genau die Kategorie code');
  assert.ok(far.some((h) => h.rule === 'vocab:en-unavailable'), 'nur gleiche oder vorige Zeile');
});
test('Labor-Messausgaben v4/**/lab/**/results*.json zaehlen als Testdateien, andere Labordateien nicht', () => {
  assert.equal(isTestFile('v4/lab/app-host/results.json'), true);
  assert.equal(isTestFile('v4/lab/app-host/results-stub.json'), true);
  assert.equal(isTestFile('v4/frame/lab/shots/results-mock.json'), true);
  assert.equal(isTestFile('v4/lab/app-host/measure.mjs'), false);
  assert.equal(isTestFile('v4/lab/app-host/api-attrappe.js'), false);
  assert.equal(isTestFile('v4/manifests/results.json'), false, 'nur unter lab/');
  assert.equal(isTestFile('modules/notes/lab/results.json'), false, 'nur unter v4/');
  assert.equal(LAB_RESULTS.test('v4/lab/results.js'), false, 'nur JSON');
});
