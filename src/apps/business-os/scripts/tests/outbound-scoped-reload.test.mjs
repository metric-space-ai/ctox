import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtempSync, mkdirSync, copyFileSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createCollectionReloader } from '../../customer-modules/outbound-lead-generation/collection-reloader.mjs';
import { leadListRow } from '../../customer-modules/outbound-lead-generation/lead-list-loader.mjs';

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
function clock() {
  const jobs = new Map(); let next = 0;
  return {
    jobs,
    setTimer: (fn, delay) => { const id = ++next; jobs.set(id, { fn, delay }); return id; },
    clearTimer: (id) => jobs.delete(id),
    run: () => { const [id, job] = jobs.entries().next().value; jobs.delete(id); return job.fn(); },
  };
}
function collections(names = ['sources', 'leads']) {
  const result = {}, listeners = {}, options = {}, reads = [];
  for (const key of names) result[key] = {
    find: () => { reads.push(key); throw new Error('Subscription must not query'); },
    $: { subscribe: (cb, opts) => {
      listeners[key] = cb; options[key] = opts;
      return { unsubscribe: () => delete listeners[key] };
    } },
  };
  return { result, listeners, options, reads };
}

test('invalidation subscriptions do not materialize any document window', () => {
  const c = collections(['sources', 'adapters', 'imports', 'researchPolicies', 'leads']);
  const timer = clock();
  const reader = createCollectionReloader({ collections: c.result, reload: () => {}, ...timer });
  assert.deepEqual(c.reads, []);
  assert.equal(Object.keys(c.listeners).length, 5);
  for (const value of Object.values(c.options)) assert.deepEqual(value, { invalidateOnly: true });
  reader.dispose();
  assert.equal(Object.keys(c.listeners).length, 0);
});
test('a burst coalesces only affected keys; another change during a read survives', async () => {
  const c = collections(); const timer = clock(); const blocked = deferred(); const calls = [];
  const reader = createCollectionReloader({ collections: c.result, ...timer,
    reload: async (keys) => { calls.push(keys); if (calls.length === 1) await blocked.promise; },
  });
  for (let i = 0; i < 40; i++) c.listeners.sources();
  assert.equal(timer.jobs.size, 1);
  const inFlight = timer.run();
  c.listeners.leads(); c.listeners.leads();
  assert.equal(timer.jobs.size, 0, 'no overlapping read');
  blocked.resolve(); await inFlight;
  await timer.run();
  assert.deepEqual(calls, [['sources'], ['leads']]);
  assert.equal(timer.jobs.size, 0);
  reader.dispose();
});
test('failed reads retain their keys and retry with bounded backoff, including new changes', async () => {
  const c = collections(); const timer = clock(); const calls = []; let reported = 0;
  const reader = createCollectionReloader({ collections: c.result, ...timer,
    reload: async (keys) => { calls.push(keys); if (calls.length === 1) throw new Error('timeout'); },
    onError: () => { reported++; throw new Error('reporting failure'); },
  });
  c.listeners.sources(); await timer.run();
  assert.equal(reported, 1);
  assert.equal([...timer.jobs.values()][0].delay, 2000);
  c.listeners.leads();
  assert.equal(timer.jobs.size, 1);
  await timer.run();
  assert.deepEqual(calls, [['sources'], ['sources', 'leads']]);
  reader.dispose();
});
test('partial failures retry only rejected collections without reloading healthy sources', async () => {
  const c = collections(); const timer = clock(); const calls = [];
  const reader = createCollectionReloader({ collections: c.result, ...timer,
    reload: async keys => {
      calls.push(keys);
      if (calls.length === 1) throw Object.assign(new Error('loader missing'), { failedKeys: ['leads'] });
    },
  });
  reader.request(['sources', 'leads']); await timer.run();
  await timer.run();
  assert.deepEqual(calls, [['sources', 'leads'], ['leads']]);
  reader.dispose();
});
test('closing during a read drops completion, retries and late invalidations', async () => {
  const c = collections(); const timer = clock(); const blocked = deferred(); let after = 0;
  const reader = createCollectionReloader({ collections: c.result, ...timer,
    reload: () => blocked.promise, afterReload: () => after++,
  });
  const late = c.listeners.leads;
  late(); const running = timer.run(); reader.dispose(); late();
  blocked.reject(new Error('closed')); await running;
  assert.equal(after, 0); assert.equal(timer.jobs.size, 0);
  assert.equal(Object.keys(c.listeners).length, 0);
});
test('a missing shell invalidation API fails without falling back to find().$', () => {
  const c = collections(); c.result.leads = { find: c.result.leads.find };
  assert.throws(() => createCollectionReloader({ collections: c.result, reload: () => {} }), /shell collection invalidation API/);
  assert.equal(Object.keys(c.listeners).length, 0);
  assert.deepEqual(c.reads, []);
});

// Exercise the shipped App reload path; only its Shell imports are replaced.
const fixture = mkdtempSync(join(tmpdir(), 'outbound-scoped-reload-'));
const source = fileURLToPath(new URL('../../customer-modules/outbound-lead-generation/', import.meta.url));
mkdirSync(join(fixture, 'modules', 'olg'), { recursive: true });
mkdirSync(join(fixture, 'shared'), { recursive: true });
writeFileSync(join(fixture, 'package.json'), '{"type":"module"}');
for (const name of ['index.js', 'collection-reloader.mjs', 'lead-revision-loader.mjs', 'lead-list-loader.mjs', 'import-preview-groups.js', 'current-state-export.mjs', 'required-field-selection.mjs']) copyFileSync(join(source, name), join(fixture, 'modules', 'olg', name));
writeFileSync(join(fixture, 'shared', 'universal-importer.js'), [
  'extractCompanyRowsFromWorkbookFile', 'extractCompanyRowsFromText', 'normalizeCompanyRow', 'openUniversalImporter', 'parseDelimitedText',
].map((name) => `export function ${name}() {}`).join('\n'));
writeFileSync(join(fixture, 'shared', 'dialogs.js'),
  'export async function showBusinessAlert() {}\nexport async function showBusinessConfirm() { return false; }\nexport async function showBusinessPrompt() { return null; }');
writeFileSync(join(fixture, 'shared', 'i18n.js'), 'export async function loadModuleMessages() { return {}; }');
writeFileSync(join(fixture, 'modules', 'olg', 'xlsx-export.js'), `
export async function buildResearchWorkbook(leads, options) { return globalThis.__outboundExportProbe.build(leads, options); }
export function downloadBlob(blob, name) { return globalThis.__outboundExportProbe.download(blob, name); }
export function exportFileName(label) { return String(label || 'Recherche') + '.xlsx'; }
`);
try {
  const hooks = (await import(pathToFileURL(join(fixture, 'modules', 'olg', 'index.js')).href)).__leadgenOutboundTestHooks;
  const state = hooks.testState();
  function setup(overrides = {}) {
    const reads = [];
    const existingLead = { id: 'lead_a', _rev: '1-a', name: 'Firma', campaign: 'K', updated_at_ms: 1, contacts: [], selected_contact_ids: [] };
    const values = {
      sources: [{ id: 'source_a', label: 'Register', enabled: true }], adapters: [], imports: [], researchPolicies: [], leads: [existingLead],
    };
    const db = Object.fromEntries(Object.keys(values).map((key) => [key, {
      find: (query = {}) => ({ exec: async () => {
        reads.push(key);
        if (overrides[key]) return overrides[key](query);
        return values[key].filter((doc) => !query.selector?.id?.$gt || doc.id > query.selector.id.$gt)
          .map((value) => ({ toJSON: () => query.projection ? leadListRow(value) : structuredClone(value) }));
      } }),
    }]));
    Object.assign(state, {
      ctx: { host: { querySelector: () => null }, sync: {
        leaseCollection: async (name, reason, options) => {
          assert.ok(['sources', 'adapters', 'imports', 'research_policies', 'leads'].some(key => name === `outbound_lead_generation_${key}`));
          assert.equal(options.forceDirect, true);
          const replication = {
            awaitQueryReady: async () => 'native-generation',
            collectionQueryGenerationToken: () => 'native-generation',
          };
          return { bridge: { state: replication }, release: async () => {} };
        },
      } },
      collections: db, sources: [], adapters: [], imports: [], leads: [existingLead], leadListRows: null,
      recipientEligibilityReady: new Set(['lead_a']), selectedDetailLoadingKey: '', selectedDetailRequestedKey: '',
      fullLeadReadSequence: 0, fullLeadAppliedSequence: new Map(),
      collectionBindingGeneration: 1, leadHydrationBindingGeneration: 1, reloadAngewendetJeSammlung: new Map(),
      uiMounted: true,
      sourceToggleIntent: new Map(), pendingLeadPatches: new Map(),
      selectedCampaign: 'K', selectedLeadId: 'lead_a', selectedLeadIds: new Set(['lead_a']),
      researchPolicyLoaded: true, researchPolicy: 'saved', researchPolicyDraft: 'unsaved',
      syncPending: false, syncWaitingCollections: new Set(),
      collectionReadErrors: new Map(),
    });
    return { reads, existingLead };
  }
  await test('channel recovery replaces subscriptions and disposes the cancelled handles', async () => {
    const names = ['sources', 'adapters', 'imports', 'researchPolicies', 'leads'];
    const old = collections(names), recovered = collections(names);
    setup(); state.collections = old.result; state.uiMounted = true;
    state.ctx = { sync: { restartCollection: async () => {} },
      db: { collection: (name) => recovered.result[name.replace('outbound_lead_generation_', '').replace('research_policies', 'researchPolicies')] } };
    hooks.bindCollections(); const generation = state.collectionBindingGeneration;
    try {
      await hooks.recoverCommandChannel('regression');
      assert.equal(Object.keys(old.listeners).length, 0);
      assert.equal(Object.keys(recovered.listeners).length, 5);
      assert.equal(state.collectionBindingGeneration, generation + 1);
      assert.deepEqual(old.reads, []); assert.deepEqual(recovered.reads, []);
    } finally { state.collectionReloader.dispose(); state.collectionReloader = null; state.uiMounted = undefined; }
  });
  await test('actual source/adapters/policy/import reloads never read the leads collection', async () => {
    for (const key of ['sources', 'adapters', 'researchPolicies', 'imports']) {
      const { reads, existingLead } = setup();
      await hooks.reload([key]);
      assert.deepEqual(reads, [key]);
      assert.equal(state.leads[0], existingLead, 'other collections preserve the complete lead object');
      assert.equal(state.selectedLeadId, 'lead_a');
      assert.equal(state.researchPolicyDraft, 'unsaved');
    }
  });
  await test('rejected lead reads preserve full data and publish healthy collections with an explicit error', async () => {
    let failed = true;
    const { existingLead } = setup({
      sources: async () => [{ toJSON: () => ({ id: 'source_b', label: 'New source', enabled: true }) }],
      leads: async query => {
        if (failed) throw Error('QUERY_GENERATION_REQUIRED: strict demand read has no loader');
        return query.selector.id?.$gt ? [] : [{ toJSON: () => leadListRow(existingLead) }];
      },
    });
    const line = { innerHTML: '', textContent: '', className: '' };
    state.ctx.host.querySelector = selector => selector === '[data-sync-line]' ? line : null;
    state.syncPending = true;
    await assert.rejects(hooks.reload(['sources', 'leads']), error => {
      assert.deepEqual(error.failedKeys, ['leads']); return true;
    });
    assert.equal(state.sources[0].id, 'source_b', 'healthy source read is not discarded');
    assert.equal(state.leads[0], existingLead, 'a rejected query cannot erase saved full details');
    assert.equal(state.leadListRows, null, 'a rejected query is never an empty list');
    assert.deepEqual([...state.collectionReadErrors.keys()], ['leads']);
    hooks.renderSyncLine();
    assert.match(line.innerHTML, /Leads konnten nicht geladen werden/);
    assert.equal(line.className, 'is-error', 'read failure takes priority over the sync spinner');
    failed = false;
    await hooks.reload(['leads']);
    assert.equal(state.collectionReadErrors.size, 0);
    hooks.renderSyncLine();
    assert.equal(line.className, 'is-syncing', 'current success clears only its own failure');
  });
  await test('a hung non-lead query times out without discarding current lead results', async () => {
    let signal;
    setup({ adapters: async () => new Promise(() => {}) });
    const originalFind = state.collections.adapters.find;
    state.collections.adapters.find = query => {
      const request = originalFind(query);
      return { exec: options => { signal = options.signal; return request.exec(options); } };
    };
    await assert.rejects(hooks.reload(['leads', 'adapters']), error => {
      assert.deepEqual(error.failedKeys, ['adapters']); return true;
    });
    assert.equal(signal.aborted, true, 'timed-out collection query is cancelled');
    assert.equal(state.leadListRows.length, 1, 'healthy lead query survives the other collection timeout');
    assert.deepEqual([...state.collectionReadErrors.keys()], ['adapters']);
    assert.match(state.collectionReadErrors.get('adapters'), /nicht rechtzeitig/);
  });
  await test('an older rejected read cannot invalidate a newer successful collection read', async () => {
    const blocked = deferred(); let reads = 0;
    setup({ sources: async () => {
      if (++reads === 1) return blocked.promise;
      return [{ toJSON: () => ({ id: 'new', label: 'Current', enabled: true }) }];
    } });
    const old = hooks.reload(['sources']);
    await hooks.reload(['sources']);
    blocked.reject(Error('obsolete loader generation'));
    await old;
    assert.equal(state.sources[0].id, 'new');
    assert.equal(state.collectionReadErrors.size, 0, 'obsolete failure is not a current error');
  });
  await test('pagination retrieves all leads, not just the default 200-window', async () => {
    const rows = Array.from({ length: 351 }, (_, i) => ({ id: 'lead_' + String(i).padStart(4, '0'), campaign: 'K', updated_at_ms: i, _rev: '1-' + i }));
    const { reads } = setup({ leads: async (query) => rows
      .filter((row) => !query.selector.id || (query.selector.id.$in ? query.selector.id.$in.includes(row.id) : row.id > query.selector.id.$gt))
      .slice(0, query.limit).map((row) => ({ toJSON: () => row })) });
    state.recipientEligibilityReady = new Set(rows.map(row => row.id));
    await hooks.reload(['leads']);
    assert.equal(state.leadListRows.length, 351);
    assert.equal(new Set(state.leadListRows.map((row) => row.id)).size, 351);
    assert.ok(state.leads.length <= 1, 'cold list never hydrates every lead');
    assert.ok(reads.every((key) => key === 'leads'));
  });
  await test('the real App hydrates only explicitly chosen leads; summaries never enter full state', async () => {
    const full = id => ({ id, _rev: `1-${id}`, name: id, campaign: 'K', contacts: [{ id: 'person-' + id }], evidence: [{ quote: 'proof' }] });
    const requests = [];
    setup({ leads: async query => {
      requests.push(query);
      return ['a', 'b'].filter(id => !query.selector.id || query.selector.id.$in?.includes(id))
        .map(id => ({ toJSON: () => query.projection ? leadListRow(full(id)) : full(id) }));
    } });
    state.leads = []; state.leadListRows = ['a', 'b'].map(id => leadListRow(full(id)));
    const result = await hooks.ensureFullLeads(['b']);
    assert.deepEqual(state.leads.map(row => row.id), ['b']);
    assert.equal(result[0].evidence[0].quote, 'proof');
    assert.equal(state.leadListRows[1].evidence, undefined);
    assert.deepEqual(requests.map(q => q.selector.id.$in), [['b']]);
  });
  await test('a replaced App binding rejects full-data hydration without publishing it', async () => {
    const blocked = deferred(); setup({ leads: () => blocked.promise });
    state.leads = []; state.leadListRows = [{ id: 'a', _rev: '1-a' }];
    const pending = hooks.ensureFullLeads(['a']);
    state.collectionBindingGeneration++;
    blocked.resolve([{ toJSON: () => ({ id: 'a', _rev: '1-a', contacts: [], evidence: [] }) }]);
    await assert.rejects(pending, /Verbindung/);
    assert.deepEqual(state.leads, []);
  });
  await test('a late full read cannot replace a newer compact revision', async () => {
    const blocked = deferred(); setup({ leads: () => blocked.promise });
    state.leads = []; state.leadListRows = [{ id: 'a', _rev: '1-a' }];
    const pending = hooks.ensureFullLeads(['a']);
    state.leadListRows[0] = { id: 'a', _rev: '2-a', research_status: 'needs_review' };
    blocked.resolve([{ toJSON: () => ({ id: 'a', _rev: '1-a', evidence: [{ quote: 'old' }] }) }]);
    await assert.rejects(pending, /aktualisiert/);
    assert.deepEqual(state.leads, []);
    assert.equal(state.leadListRows[0]._rev, '2-a');
  });
  await test('selected detail reads coalesce live changes and hydrate the latest selection once', async () => {
    const blocked = deferred(), latest = deferred(); const ids = [];
    const firstStarted = deferred(), latestStarted = deferred();
    setup({ leads: query => {
      ids.push(query.selector.id.$in);
      (ids.length === 1 ? firstStarted : latestStarted).resolve();
      return ids.length === 1 ? blocked.promise : latest.promise;
    } });
    state.leads = []; state.leadListRows = [{ id: 'a', _rev: '1-a' }, { id: 'b', _rev: '1-b' }];
    state.selectedLeadId = 'a'; state.recipientEligibilityReady = new Set(['a', 'b']);
    const first = hooks.loadSelectedLeadDetails();
    await firstStarted.promise;
    for (let i = 0; i < 10; i++) await hooks.loadSelectedLeadDetails();
    state.selectedLeadId = 'b'; await hooks.loadSelectedLeadDetails();
    assert.deepEqual(ids, [['a']], 'no overlapping full reads');
    blocked.resolve([{ toJSON: () => ({ id: 'a', _rev: '1-a', contacts: [] }) }]);
    await first;
    await latestStarted.promise;
    assert.deepEqual(ids, [['a'], ['b']], 'latest selection is read after the first finishes');
    latest.resolve([{ toJSON: () => ({ id: 'b', _rev: '1-b', contacts: [], evidence: [{ quote: 'latest' }] }) }]);
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(state.selectedDetailLoadingKey, '');
    assert.equal(state.leads.find(row => row.id === 'b').evidence[0].quote, 'latest');
  });
  await test('a newer read of another collection does not suppress a delayed result', async () => {
    const blocked = deferred();
    setup({ sources: () => blocked.promise });
    const sources = hooks.reload(['sources']);
    await hooks.reload(['adapters']);
    blocked.resolve([{ toJSON: () => ({ id: 'late_source', label: 'Fresh' }) }]);
    await sources;
    assert.equal(state.sources[0].id, 'late_source');
  });
  await test('an older read of the same collection cannot overwrite a newer result', async () => {
    const blocked = deferred(); let reads = 0;
    setup({ sources: () => ++reads === 1 ? blocked.promise : [{ toJSON: () => ({ id: 'new', label: 'New' }) }] });
    const old = hooks.reload(['sources']); await hooks.reload(['sources']);
    blocked.resolve([{ toJSON: () => ({ id: 'old', label: 'Old' }) }]); await old;
    assert.equal(state.sources[0].id, 'new');
  });
  await test('a closed or recovered binding cannot apply its delayed documents', async () => {
    const blocked = deferred(); setup({ sources: () => blocked.promise });
    const old = hooks.reload(['sources']); state.collectionBindingGeneration++;
    blocked.resolve([{ toJSON: () => ({ id: 'old', label: 'Old' }) }]); await old;
    assert.deepEqual(state.sources, []);
  });
  await test('a late render after unmount cannot query or recreate the old module host', () => {
    const previous = state.ctx;
    state.ctx = { host: new Proxy({}, { get: () => { throw new Error('closed host accessed'); } }) };
    state.uiMounted = false;
    try { hooks.__render.render(); } finally { state.ctx = previous; state.uiMounted = undefined; }
  });
  await test('demand-only is wrapper metadata; persisted lead schema version stays zero', () => {
    const manifest = JSON.parse(readFileSync(join(source, 'collections.schema.json'), 'utf8'));
    const leads = manifest.collections.outbound_lead_generation_leads;
    assert.equal(leads.syncProfile, 'demand-only');
    assert.equal(leads.schema.version, 0);
    assert.equal(leads.schema.primaryKey, 'id');
    assert.equal(leads.schema.syncProfile, undefined);
    const original = JSON.parse(readFileSync(new URL('./fixtures/outbound-lead-schema-v0.json', import.meta.url), 'utf8'));
    assert.deepEqual(leads.schema, original, 'demand-only must not migrate the installed lead schema');
    assert.equal(leads.schema.additionalProperties, true, 'field_status remains permitted as an additional property');
  });
  await test('actual required checkbox UI, click, denied save and reload retain persisted field semantics', async () => {
    setup(); state.optionalFieldsDraft = null; state.optionalFieldsSaved = null;
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
    state.collections.researchPolicies = { findOne: () => ({ exec: async () => ({
      incrementalPatch: async patch => { if (fail) throw Error('permission denied'); saved = patch; },
    }) }) };
    await assert.rejects(hooks.saveOptionalFields(), /permission denied/);
    assert.equal(state.researchPolicyRecord, previous); assert.equal(state.optionalFieldsDraft, draft);
    fail = false; await hooks.saveOptionalFields();
    assert.deepEqual(saved.optional_field_keys, ['firma_fax', 'firma_name', 'future_field']);
    assert.deepEqual(Object.keys(saved).sort(), ['optional_field_keys', 'updated_at_ms']);
    state.optionalFieldsSaved = null; state.optionalFieldsDraft = null;
    state.researchPolicyRecord = saved;
    html = hooks.renderOptionalFieldSettings();
    assert.equal(checked(html, 'person_email'), true); assert.equal(checked(html, 'firma_name'), false);
    const total = (html.match(/<input/g) || []).length;
    assert.equal(hooks.requiredResearchFieldCount(), total - 2, 'unknown persisted key must not subtract from visible required count');
  });
  await test('actual 139-lead export does not dispatch checks, captures before await and downloads before Spreadsheet', async () => {
    setup();
    const oldDocument = globalThis.document, oldTimeout = globalThis.setTimeout;
    const timeouts = [], downloaded = [], opened = [], blocked = deferred();
    let workbook, probe;
    globalThis.document = { body: {} };
    globalThis.setTimeout = (fn, ms) => { timeouts.push(ms); return 0; };
    const leads = Array.from({ length: 139 }, (_, index) => ({
      id: 'lead_' + index, name: 'Firma ' + index, _rev: '1-' + index,
      research_status: 'needs_review', data: { firma_name: 'Firma ' + index },
      contacts: [{ id: 'contact_' + index, person_key: 'person-' + index,
        person_vorname: 'A', person_nachname: 'B', person_email: 'a' + index + '@example.test' }],
    }));
    const first = leads[0].contacts[0];
    state.recipientEligibility = new Map([['lead_0|contact_0', { status: 'free', reason: 'saved' }]]);
    state.recipientEligibilityReady = new Set(['lead_0']);
    state.ctx = { ...state.ctx, actions: { openApp: (app, args) => {
      assert.equal(downloaded.length, 1, 'download precedes receiver handoff');
      opened.push({ app, args });
    } }, commands: { execute: () => { throw Error('Export must not dispatch'); } } };
    globalThis.__outboundExportProbe = probe = {
      build: async (rows, options) => { workbook = { rows, options }; await blocked.promise; return new Blob(['xlsx']); },
      download: (blob, name) => downloaded.push({ blob, name }),
    };
    try {
      const pending = hooks.exportResearchXlsx(leads, 'K');
      leads[0].data.firma_name = 'Later edit';
      state.recipientEligibility.get('lead_0|contact_0').status = 'blocked';
      for (let turn = 0; !workbook && turn < 100; turn++) await new Promise(resolve => oldTimeout(resolve, 1));
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
      assert.equal(opened[0].args.openFile.report_snapshot.source_record_ids.length, 139);
      probe.build = async () => { throw Error('zip failed'); };
      await hooks.exportResearchXlsx(leads, 'failed');
      assert.equal(state.exportXlsxBusy, false);
      probe.build = async () => new Blob(['retry']);
      state.ctx.actions.openApp = () => { throw Error('receiver missing'); };
      await hooks.exportResearchXlsx(leads, 'retry');
      assert.equal(downloaded.length, 2, 'receiver failure does not discard successful download');
      assert.equal(state.exportXlsxBusy, false); assert.match(state.notice, /Excel heruntergeladen.*nicht geöffnet/);
    } finally {
      globalThis.document = oldDocument; globalThis.setTimeout = oldTimeout;
      delete globalThis.__outboundExportProbe;
    }
  });
} finally {
  const state = (await import(pathToFileURL(join(fixture, 'modules', 'olg', 'index.js')).href)).__leadgenOutboundTestHooks.testState();
  state.uiMounted = false;
  state.selectedLeadId = '';
  state.collectionBindingGeneration++;
  for (const timer of state.sperrpruefungNeu?.values?.() || []) clearTimeout(timer);
  state.sperrpruefungNeu?.clear?.();
  rmSync(fixture, { recursive: true, force: true });
}
