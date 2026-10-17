// Fachregeln der ausgelieferten OLG-App (App-API 4.0), die früher in outbound-scoped-reload.test.mjs neben
// der gelöschten v3-Lade-Maschinerie standen: Pflichtfeld-Häkchen und der Excel-Export mit Spreadsheet-Übergabe.
// Echte index.js; nur `ctox:shared/` wird umgelenkt und xlsx-export.js durch eine Sonde ersetzt.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtempSync, mkdirSync, cpSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const fixture = mkdtempSync(join(tmpdir(), 'outbound-app-actions-'));
const source = fileURLToPath(new URL('../../customer-modules/outbound-lead-generation/', import.meta.url));
const app = join(fixture, 'customer-modules', 'olg');
mkdirSync(join(fixture, 'shared'), { recursive: true });
writeFileSync(join(fixture, 'package.json'), '{"type":"module"}');
cpSync(source, app, { recursive: true, filter: (path) => !/\/(tests|vendor)(\/|$)/.test(path.slice(source.length - 1)) });
writeFileSync(join(app, 'index.js'), readFileSync(join(app, 'index.js'), 'utf8').replace(/(['"])ctox:shared\//g, '$1../../shared/'));
writeFileSync(join(fixture, 'shared', 'universal-importer.js'), [
  'extractCompanyRowsFromWorkbookFile', 'extractCompanyRowsFromText', 'normalizeCompanyRow', 'openUniversalImporter', 'parseDelimitedText',
].map((name) => `export function ${name}() {}`).join('\n'));
writeFileSync(join(app, 'xlsx-export.js'), `
export async function buildResearchWorkbook(leads, options) { return globalThis.__outboundExportProbe.build(leads, options); }
export function downloadBlob(blob, name) { return globalThis.__outboundExportProbe.download(blob, name); }
export function exportFileName(label) { return String(label || 'Recherche') + '.xlsx'; }
`);

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

try {
  const hooks = (await import(pathToFileURL(join(app, 'index.js')).href)).__leadgenOutboundTestHooks;
  const state = hooks.testState();
  // Ohne Montage: render() kehrt bei uiMounted === false sofort zurück; Port und API als Sonden.
  function setup() {
    const port = {
      hinweise: [], meldungen: [], befehle: [],
      melde(code, text) { this.hinweise.push({ code, text }); },
      raeume() {},
      async meldung(text) { this.meldungen.push(String(text)); },
      befehl() { throw Error('Export must not dispatch'); },
      auftrag() { throw Error('Export must not dispatch'); },
      aendereDok: async () => { throw Error('no write expected'); },
    };
    Object.assign(state, {
      uiMounted: false, root: { querySelector: () => null, querySelectorAll: () => [] },
      port, leads: [], selectedLeadIds: new Set(), recipientEligibility: new Map(), recipientEligibilityReady: new Set(),
      exportXlsxBusy: false,
    });
    return port;
  }

  await test('actual required checkbox UI, click, denied save and reload retain persisted field semantics', async () => {
    const port = setup(); state.optionalFieldsDraft = null; state.optionalFieldsSaved = null; state.fieldCatalogDraft = null;
    state.researchPolicyRecord = { optional_field_keys: ['firma_fax', 'person_email', 'future_field'], updated_at_ms: 1 };
    const checked = (html, key) => {
      const match = html.match(new RegExp('<input[^>]*data-field="' + key + '"[^>]*>'));
      assert.ok(match, key); return / checked/.test(match[0]);
    };
    let html = hooks.renderOptionalFieldSettings();
    assert.equal(checked(html, 'firma_name'), true); assert.equal(checked(html, 'firma_fax'), false);
    assert.match(html, /angehakt = Pflicht/); assert.match(html, /Pflichtfelder speichern/);
    const click = async (key, value) => {
      const trigger = { dataset: { action: 'toggle-optional-field', field: key }, checked: value, closest: () => null };
      await hooks.handleClick({ target: { closest: () => trigger } });
    };
    await click('person_email', true); await click('firma_name', false);
    html = hooks.renderOptionalFieldSettings();
    assert.equal(checked(html, 'person_email'), true); assert.equal(checked(html, 'firma_name'), false);
    assert.equal(state.optionalFieldsDraft.has('future_field'), true);
    const previous = state.researchPolicyRecord, draft = state.optionalFieldsDraft;
    let fail = true, saved;
    const policy = { id: 'policy' };
    state.collections = { ...(state.collections || {}), researchPolicies: { eins: async () => policy } };
    port.aendereDok = async (doc, patch) => { assert.equal(doc, policy); if (fail) throw Error('permission denied'); saved = patch; };
    await assert.rejects(hooks.saveOptionalFields(), /permission denied/);
    assert.equal(state.researchPolicyRecord, previous); assert.equal(state.optionalFieldsDraft, draft);
    fail = false; await hooks.saveOptionalFields();
    assert.deepEqual(saved.optional_field_keys, ['firma_fax', 'firma_name', 'future_field']);
    assert.deepEqual(Object.keys(saved).sort(), ['optional_field_keys', 'updated_at_ms']);
    state.optionalFieldsSaved = null; state.optionalFieldsDraft = null;
    state.researchPolicyRecord = saved;
    html = hooks.renderOptionalFieldSettings();
    assert.equal(checked(html, 'person_email'), true); assert.equal(checked(html, 'firma_name'), false);
    const total = (html.match(/<input[^>]*data-action="toggle-optional-field"/g) || []).length;
    assert.equal(hooks.requiredResearchFieldCount(), total - 2, 'unknown persisted key must not subtract from visible required count');
  });

  await test('actual 139-lead export does not dispatch checks, captures before await and downloads before Spreadsheet', async () => {
    const port = setup();
    const oldTimeout = globalThis.setTimeout;
    const timeouts = [], downloaded = [], opened = [], stored = [], blocked = deferred();
    let workbook, probe;
    const leads = Array.from({ length: 139 }, (_, index) => ({
      id: 'lead_' + index, name: 'Firma ' + index, _rev: '1-' + index,
      research_status: 'needs_review', data: { firma_name: 'Firma ' + index },
      contacts: [{ id: 'contact_' + index, person_key: 'person-' + index,
        person_vorname: 'A', person_nachname: 'B', person_email: 'a' + index + '@example.test' }],
    }));
    const first = leads[0].contacts[0];
    state.recipientEligibility = new Map([['lead_0|contact_0', { status: 'free', reason: 'saved' }]]);
    state.recipientEligibilityReady = new Set(['lead_0']);
    // App-API 4.0: Datei über api.files, Spreadsheet-Übergabe über api.shell.navigate.
    state.api = {
      files: { put: (file, meta) => {
        assert.equal(downloaded.length, stored.length + 1, 'download precedes receiver handoff');
        stored.push({ file, meta });
        return { local: Promise.resolve(), ref: Promise.resolve('ref_' + stored.length) };
      } },
      shell: { navigate: (target, params) => { opened.push({ target, params }); return true; } },
    };
    globalThis.setTimeout = (fn, ms) => { timeouts.push(ms); return 0; };
    globalThis.__outboundExportProbe = probe = {
      build: async (rows, options) => { workbook = { rows, options }; await blocked.promise; return new Blob(['xlsx']); },
      download: (blob, name) => downloaded.push({ blob, name }),
    };
    try {
      const pending = hooks.exportResearchXlsx(leads, 'K');
      leads[0].data.firma_name = 'Later edit';
      state.recipientEligibility.get('lead_0|contact_0').status = 'blocked';
      for (let turn = 0; !workbook && turn < 100; turn++) await new Promise((resolve) => oldTimeout(resolve, 1));
      assert.ok(workbook, 'export reaches workbook creation without a provider roundtrip');
      assert.equal(workbook.rows.length, 139);
      assert.equal(workbook.rows[0].data.firma_name, 'Firma 0');
      assert.equal(workbook.options.recipientStatus(workbook.rows[0], first).status, 'free');
      assert.equal(workbook.options.recipientStatus(workbook.rows[1], workbook.rows[1].contacts[0]), null);
      assert.ok(!timeouts.includes(20_000), 'no remark-check timeout');
      await hooks.exportResearchXlsx(leads, 'duplicate');
      blocked.resolve(); await pending;
      assert.equal(downloaded.length, 1); assert.equal(opened.length, 1);
      assert.equal(state.exportXlsxBusy, false);
      assert.equal(stored[0].file.name, 'K.xlsx');
      assert.equal(opened[0].target, 'spreadsheets');
      assert.equal(opened[0].params.file_ref, 'ref_1');
      assert.equal(opened[0].params.report_snapshot.source_record_ids.length, 139);
      probe.build = async () => { throw Error('zip failed'); };
      await hooks.exportResearchXlsx(leads, 'failed');
      assert.equal(state.exportXlsxBusy, false);
      assert.match(port.meldungen.at(-1), /Excel-Export ist fehlgeschlagen: zip failed/);
      probe.build = async () => new Blob(['retry']);
      state.api.shell.navigate = () => { throw Error('receiver missing'); };
      await hooks.exportResearchXlsx(leads, 'retry');
      assert.equal(downloaded.length, 2, 'receiver failure does not discard successful download');
      assert.equal(state.exportXlsxBusy, false);
      assert.match(port.hinweise.at(-1).text, /Excel heruntergeladen.*nicht geöffnet/);
    } finally {
      globalThis.setTimeout = oldTimeout;
      delete globalThis.__outboundExportProbe;
    }
  });
} finally { rmSync(fixture, { recursive: true, force: true }); }
