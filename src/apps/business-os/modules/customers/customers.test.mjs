import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { Buffer } from 'node:buffer';
import { fileURLToPath } from 'node:url';

import { build } from 'esbuild';

const schemaUrl = new URL('./schema.js', import.meta.url);
const schemaSource = await readFile(schemaUrl, 'utf8');
const schemaModule = await import(`data:text/javascript;base64,${Buffer.from(schemaSource).toString('base64')}`);
const { collections, migrationStrategies } = schemaModule;

// Collection entries may be either a bare schema or a `{ schema,
// conflictStrategy }` wrapper — the shell, hash generator, and native peer
// all read `definition.schema || definition` (see schema.js).
const schemaOf = (definition) => definition.schema || definition;

const expectedCollections = [
  'business_commands',
  'customer_accounts',
  'customer_contacts',
  'customer_opportunities',
  'customer_tasks',
  'customer_notes',
  'customer_activities',
  'customer_files',
  'customer_views',
  'customer_view_filters',
  'customer_view_sorts',
  'customer_import_batches',
  'customer_dedupe_candidates',
];

assert.deepEqual(Object.keys(collections).sort(), expectedCollections.sort());

for (const [name, definition] of Object.entries(collections)) {
  const schema = schemaOf(definition);
  assert.equal(schema.primaryKey, 'id', `${name} primary key`);
  assert.equal(schema.type, 'object', `${name} schema type`);
  assert.equal(schema.additionalProperties, true, `${name} allows forward-compatible properties`);
  assert.ok(Number.isInteger(schema.version), `${name} schema version`);
  assert.ok(schema.properties.id, `${name} id property`);
  assert.ok(schema.properties.updated_at_ms, `${name} updated_at_ms property`);
  assert.ok(schema.required.includes('id'), `${name} requires id`);
  assert.ok(schema.required.includes('updated_at_ms'), `${name} requires updated_at_ms`);
}

for (const name of expectedCollections.filter((collection) => collection.startsWith('customer_'))) {
  const schema = schemaOf(collections[name]);
  assert.ok(schema.properties.is_deleted, `${name} is soft-delete capable`);
  assert.ok(schema.required.includes('is_deleted'), `${name} requires is_deleted`);
  assert.ok(
    schema.indexes.some((index) => Array.isArray(index) && index.join('|') === 'is_deleted|updated_at_ms'),
    `${name} has soft-delete freshness index`,
  );
}

assert.ok(schemaOf(collections.customer_accounts).indexes.includes('domain'));
assert.ok(schemaOf(collections.customer_contacts).indexes.includes('email'));
assert.ok(schemaOf(collections.customer_opportunities).indexes.some((index) => Array.isArray(index) && index.join('|') === 'stage|position'));
assert.ok(schemaOf(collections.customer_activities).indexes.some((index) => Array.isArray(index) && index.join('|') === 'account_id|happens_at_ms'));
assert.ok(schemaOf(collections.customer_dedupe_candidates).indexes.some((index) => Array.isArray(index) && index.join('|') === 'object_type|match_key'));
assert.equal(typeof migrationStrategies.business_commands[1], 'function');

const moduleJson = JSON.parse(await readFile(new URL('./module.json', import.meta.url), 'utf8'));
const registryJson = JSON.parse(await readFile(new URL('../registry.json', import.meta.url), 'utf8'));
const registryEntry = registryJson.modules.find((mod) => mod.id === 'customers');

assert.equal(moduleJson.id, 'customers');
assert.equal(moduleJson.entry, 'modules/customers/index.html');
assert.equal(moduleJson.layout.shell, 'windowed');
assert.ok(moduleJson.layout.icon_svg.includes('svg-customers'));
assert.ok(registryEntry, 'customers registry entry exists');
assert.deepEqual(registryEntry.collections, moduleJson.collections);
assert.equal(registryEntry.entry, moduleJson.entry);
assert.equal(registryEntry.category, 'Sales');

const bundledModule = await build({
  entryPoints: [fileURLToPath(new URL('./index.js', import.meta.url))],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  write: false,
});
const [{ text: bundledSource }] = bundledModule.outputFiles;
const { __customersTestHooks: hooks } = await import(
  `data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`
);

const customerFixtures = {
  business_commands: [
    { id: 'cmd-a', module: 'customers', command_type: 'customers.account.update', record_id: 'acct-a', payload: { account_id: 'acct-a' }, status: 'completed', updated_at_ms: 11 },
    { id: 'cmd-b', module: 'customers', command_type: 'customers.opportunity.close_won', payload: { opportunity_id: 'opp-a' }, status: 'failed', error: 'Projection rejected', updated_at_ms: 12 },
    { id: 'cmd-c', module: 'customers', command_type: 'customers.import.from_outbound', payload: { outbound_company_id: 'co-ready' }, status: 'pending_sync', updated_at_ms: 13 },
    { id: 'cmd-x', module: 'tickets', command_type: 'tickets.update', payload: { ticket_id: 'ticket-a' }, status: 'completed', updated_at_ms: 14 },
  ],
  customer_accounts: [
    { id: 'acct-a', name: 'Acme GmbH', domain: 'acme.test', customer_stage: 'active', health_status: 'healthy', updated_at_ms: 3 },
    { id: 'acct-b', name: 'Beta AG', domain: 'beta.test', customer_stage: 'renewal', health_status: 'at_risk', updated_at_ms: 2 },
  ],
  customer_contacts: [
    { id: 'contact-a', account_id: 'acct-a', first_name: 'Ada', last_name: 'Lovelace', email: 'ada@acme.test', job_title: 'CTO', is_primary_contact: true, updated_at_ms: 3 },
    { id: 'contact-b', account_id: 'acct-b', first_name: 'Bernd', email: 'bernd@beta.test', job_title: 'CFO', updated_at_ms: 2 },
  ],
  customer_opportunities: [
    { id: 'opp-a', account_id: 'acct-a', name: 'Renewal 2026', opportunity_type: 'renewal', stage: 'proposal', amount_cents: 1000000, currency: 'EUR', probability: 40, close_date_ms: Date.parse('2026-05-30T00:00:00.000Z'), updated_at_ms: 3 },
    { id: 'opp-b', account_id: 'acct-b', name: 'Expansion', opportunity_type: 'expansion', stage: 'negotiation', amount_cents: 500000, currency: 'EUR', probability: 60, close_date_ms: Date.parse('2026-06-15T00:00:00.000Z'), updated_at_ms: 2 },
  ],
  customer_tasks: [
    { id: 'task-a', account_id: 'acct-a', title: 'Renewal vorbereiten', status: 'open', due_at_ms: Date.parse('2026-05-28T00:00:00.000Z'), updated_at_ms: 3 },
    { id: 'task-b', account_id: 'acct-a', contact_id: 'contact-a', title: 'CFO informieren', status: 'completed', completed_at_ms: Date.parse('2026-05-24T00:00:00.000Z'), updated_at_ms: 2 },
    { id: 'task-c', account_id: 'acct-a', opportunity_id: 'opp-a', title: 'Proposal senden', status: 'in_progress', due_at_ms: Date.parse('2026-05-27T00:00:00.000Z'), updated_at_ms: 4 },
  ],
  customer_notes: [
    { id: 'note-a', account_id: 'acct-a', linked_note_id: 'note-external-a', title: 'Buying committee', body: 'Legal ist eingebunden.', body_format: 'markdown', updated_at_ms: Date.parse('2026-05-25T00:00:00.000Z') },
    { id: 'note-b', account_id: 'acct-a', opportunity_id: 'opp-a', title: 'Preisanker', body: 'Expansion Bundle.', body_format: 'markdown', updated_at_ms: Date.parse('2026-05-29T00:00:00.000Z') },
  ],
  customer_activities: [
    { id: 'act-a', account_id: 'acct-a', name: 'Kunde erstellt', happens_at_ms: Date.parse('2026-05-20T00:00:00.000Z') },
    { id: 'act-b', account_id: 'acct-a', opportunity_id: 'opp-a', name: 'Stage geaendert', happens_at_ms: Date.parse('2026-05-26T00:00:00.000Z') },
  ],
  customer_files: [
    { id: 'file-a', account_id: 'acct-a', document_id: 'doc-a', name: 'MSA.pdf', mime_type: 'application/pdf', updated_at_ms: 7 },
  ],
  customer_import_batches: [
    { id: 'batch-imported', source: 'outbound', source_record_id: 'co-imported', status: 'completed', object_type: 'account', imported_count: 1, skipped_count: 0, failed_count: 0, dedupe_count: 0, updated_at_ms: 8 },
  ],
  customer_dedupe_candidates: [
    { id: 'dedupe-a', object_type: 'account', match_key: 'acme.test', match_type: 'domain', source_record_id: 'co-dup', existing_record_id: 'acct-a', import_batch_id: 'batch-a', status: 'open', confidence: 0.95, updated_at_ms: 8 },
    { id: 'dedupe-b', object_type: 'account', match_key: 'old.test', match_type: 'domain', source_record_id: 'co-old', existing_record_id: 'acct-b', import_batch_id: 'batch-b', status: 'resolved', confidence: 0.9, decision: 'keep_existing', updated_at_ms: 7 },
  ],
  outbound_companies: [
    { id: 'co-ready', campaign_id: 'camp-a', name: 'Gamma GmbH', domain: 'gamma.test', website: 'https://gamma.test', city: 'Berlin', country: 'DE', qualification_status: 'qualified', research_status: 'done', pipeline_status: 'pipeline', fit_score: 91, payload: {}, updated_at_ms: 9 },
    { id: 'co-imported', campaign_id: 'camp-a', name: 'Imported GmbH', domain: 'imported.test', qualification_status: 'qualified', research_status: 'done', pipeline_status: 'pipeline', fit_score: 80, payload: {}, updated_at_ms: 8 },
    { id: 'co-dup', campaign_id: 'camp-a', name: 'Acme Duplicate', domain: 'acme.test', qualification_status: 'qualified', research_status: 'done', pipeline_status: 'pipeline', fit_score: 77, payload: {}, updated_at_ms: 7 },
  ],
  outbound_pipeline_items: [
    { id: 'pipe-ready', campaign_id: 'camp-a', company_id: 'co-ready', company_name: 'Gamma GmbH', stage: 'qualified', contact_research_status: 'done', outreach_status: 'ready', contacts: [{ name: 'Grace Hopper', email: 'grace@gamma.test', job_title: 'CTO' }], payload: {}, updated_at_ms: 9 },
  ],
  communication_messages: [
    { message_key: 'msg-a', sender_address: 'ada@acme.test', recipient_addresses_json: ['owner@ctox.test'], subject: 'Renewal', preview: 'Acme Renewal', external_created_at: '2026-05-24T10:00:00.000Z', observed_at: '2026-05-24T10:00:00.000Z' },
  ],
  calendar_events: [
    { id: 'event-a', title: 'Acme QBR', description: 'Renewal 2026', attendees: [{ email: 'ada@acme.test', name: 'Ada Lovelace' }], start_time: Date.parse('2026-05-31T10:00:00.000Z'), updated_at_ms: 9 },
  ],
  documents: [
    { id: 'doc-a', title: 'Acme MSA', filename: 'MSA.pdf', linked_records: [{ type: 'customer_account', id: 'acct-a' }], updated_at_ms: 8 },
  ],
  notes: [
    { id: 'note-external-a', title: 'Acme Account Plan', content: 'Account plan for acme.test', tags: 'customer', updated_at_ms: 7 },
  ],
  spreadsheets: [
    { id: 'sheet-a', title: 'Acme ARR Model', linked_records: [{ type: 'customer_account', id: 'acct-a' }], updated_at_ms: 6 },
  ],
};

assert.deepEqual(hooks.summarizeCustomersData(customerFixtures), {
  accounts: 2,
  contacts: 2,
  opportunities: 2,
  tasks: 3,
  activities: 2,
  stageCounts: { active: 1, renewal: 1 },
  healthCounts: { healthy: 1, at_risk: 1 },
});
assert.equal(hooks.relatedRecords('acct-a', customerFixtures).openTasks.length, 2);
assert.equal(hooks.validateAccountDraft({ name: '  ' }).valid, false);
assert.equal(hooks.validateContactDraft({ account_id: 'acct-a', first_name: '', last_name: '', email: '' }).valid, false);
assert.equal(hooks.normalizeDomain('https://www.example.com/path'), 'example.com');
assert.deepEqual(hooks.buildCreateAccountCommand({
  name: ' Acme GmbH ',
  domain: 'https://www.acme.test',
  industry: 'SaaS',
  account_status: 'active',
  customer_stage: 'active',
  health_status: 'healthy',
}).payload, {
  name: 'Acme GmbH',
  domain: 'acme.test',
  industry: 'SaaS',
  account_status: 'active',
  customer_stage: 'active',
  health_status: 'healthy',
  source: 'business-os-customers-ui',
});
assert.deepEqual(
  hooks.filterAndSortAccounts(customerFixtures.customer_accounts, {
    search: 'beta',
    stage: 'all',
    health: 'all',
    sort: { field: 'name', direction: 'asc' },
  }).map((item) => item.id),
  ['acct-b'],
);
assert.deepEqual(
  hooks.filterAndSortContacts(customerFixtures.customer_contacts, customerFixtures.customer_accounts, {
    search: 'acme',
    sort: { field: 'email', direction: 'asc' },
  }).map((item) => item.id),
  ['contact-a'],
);
assert.deepEqual(
  hooks.filterAndSortOpportunities(customerFixtures.customer_opportunities, customerFixtures.customer_accounts, {
    search: 'beta',
    preset: 'all',
    sort: { field: 'amount_cents', direction: 'desc' },
  }).map((item) => item.id),
  ['opp-b'],
);
assert.deepEqual(Object.keys(hooks.groupOpportunitiesByStage(customerFixtures.customer_opportunities)).sort(), [
  'closed_lost',
  'closed_won',
  'committed',
  'negotiation',
  'proposal',
  'qualification',
]);
assert.deepEqual(hooks.summarizeOpportunityPipeline(customerFixtures.customer_opportunities), {
  total_cents: 1500000,
  weighted_cents: 700000,
  currency: 'EUR',
  stageCounts: { proposal: 1, negotiation: 1 },
});
const accountContext = {
  type: 'account',
  id: 'acct-a',
  account: customerFixtures.customer_accounts[0],
};
const opportunityContext = {
  type: 'opportunity',
  id: 'opp-a',
  account: customerFixtures.customer_accounts[0],
  opportunity: customerFixtures.customer_opportunities[0],
};
const contactContext = {
  type: 'contact',
  id: 'contact-a',
  account: customerFixtures.customer_accounts[0],
  contact: customerFixtures.customer_contacts[0],
};
assert.deepEqual(hooks.filterRelatedTasks(accountContext, customerFixtures).map((item) => item.id), ['task-c', 'task-a', 'task-b']);
assert.deepEqual(hooks.filterRelatedTasks(opportunityContext, customerFixtures).map((item) => item.id), ['task-c']);
assert.deepEqual(hooks.filterRelatedNotes(opportunityContext, customerFixtures).map((item) => item.id), ['note-b']);
assert.deepEqual(hooks.filterRelatedTasks(contactContext, customerFixtures).map((item) => item.id), ['task-b']);
assert.deepEqual(hooks.filterRelatedFiles(accountContext, customerFixtures).map((item) => item.id), ['file-a']);
assert.deepEqual(hooks.buildTimelineRows(opportunityContext, customerFixtures).map((item) => item.id), ['note-b', 'task-c', 'act-b']);
assert.equal(hooks.canMutateCustomersContext({ session: { user: { role: 'business_os_readonly' } } }), false);
assert.equal(hooks.canMutateCustomersContext({ canModifyModule: () => false, session: { user: { role: 'admin' } } }), true);
assert.equal(hooks.canMutateCustomersContext({ session: { user: { role: 'sales_admin' } } }), true);
assert.equal(hooks.commandStatusTone('pending_sync'), 'pending');
assert.equal(hooks.commandStatusTone('completed'), 'completed');
assert.equal(hooks.commandStatusTone('failed'), 'failed');
assert.equal(hooks.isClosedOpportunity({ stage: 'closed_won' }), true);
assert.equal(hooks.isActivationKey('Enter'), true);
assert.equal(hooks.isActivationKey(' '), true);
assert.equal(hooks.isActivationKey('Escape'), false);
assert.equal(hooks.nextDetailTab('overview', 1), 'tasks');
assert.equal(hooks.nextDetailTab('apps', 1), 'overview');
assert.equal(hooks.nextDetailTab('overview', -1), 'apps');
assert.deepEqual(hooks.filterCustomerCommands(customerFixtures.business_commands).map((item) => item.id), ['cmd-c', 'cmd-b', 'cmd-a']);
assert.deepEqual(hooks.filterCustomerCommands(customerFixtures.business_commands, accountContext).map((item) => item.id), ['cmd-a']);
assert.deepEqual(hooks.filterCustomerCommands(customerFixtures.business_commands, opportunityContext).map((item) => item.id), ['cmd-b', 'cmd-a']);
assert.deepEqual(hooks.summarizeCustomerCommands(hooks.filterCustomerCommands(customerFixtures.business_commands)), {
  pending: 1,
  completed: 1,
  failed: 1,
});
assert.deepEqual(hooks.buildOutboundHandoffRows(customerFixtures).map((item) => [item.company.id, item.status]), [
  ['co-ready', 'ready'],
  ['co-dup', 'needs_review'],
  ['co-imported', 'imported'],
]);
assert.deepEqual(hooks.filterOutboundHandoffRows(hooks.buildOutboundHandoffRows(customerFixtures), { search: 'gamma' }).map((item) => item.company.id), ['co-ready']);
assert.deepEqual(hooks.filterDedupeCandidates(customerFixtures.customer_dedupe_candidates, customerFixtures.customer_accounts, { status: 'open', search: 'acme' }).map((item) => item.id), ['dedupe-a']);
assert.deepEqual(hooks.buildImportFromOutboundCommand(hooks.buildOutboundHandoffRows(customerFixtures)[0]).payload, {
  source_record_id: 'co-ready',
  outbound_company_id: 'co-ready',
  pipeline_id: 'pipe-ready',
  source: 'business-os-customers-ui',
});
assert.deepEqual(hooks.buildDedupeResolveCommand('dedupe-a', 'keep_existing').payload, {
  candidate_id: 'dedupe-a',
  decision: 'keep_existing',
});
assert.deepEqual(hooks.recordLinkParams(contactContext), {
  source_module: 'customers',
  customer_type: 'contact',
  account_id: 'acct-a',
  contact_id: 'contact-a',
  customer_name: 'Ada Lovelace',
  domain: 'acme.test',
  email: 'ada@acme.test',
});
const linkedAppRows = hooks.buildLinkedAppRows(accountContext, customerFixtures);
assert.deepEqual(linkedAppRows.map((item) => [item.moduleId, item.count]), [
  ['conversations', 1],
  ['calendar', 1],
  ['documents', 1],
  ['notes', 1],
  ['spreadsheets', 1],
  ['outbound', 1],
]);
assert.equal(hooks.buildCrossAppHref('documents', hooks.recordLinkParams(accountContext)), '#documents?source_module=customers&customer_type=account&account_id=acct-a&customer_name=Acme+GmbH&domain=acme.test');
assert.equal(hooks.moneyToCents('1.234,56'), 123456);
assert.deepEqual(hooks.buildAccountUpdateCommand('acct-a', {
  name: 'Acme GmbH',
  domain: 'acme.test',
  account_status: 'active',
  customer_stage: 'renewal',
  health_status: 'neutral',
}).payload, {
  account_id: 'acct-a',
  name: 'Acme GmbH',
  domain: 'acme.test',
  account_status: 'active',
  customer_stage: 'renewal',
  health_status: 'neutral',
  source: 'business-os-customers-ui',
});
assert.deepEqual(hooks.buildContactCreateCommand({
  account_id: 'acct-a',
  first_name: 'Ada',
  last_name: 'Lovelace',
  email: 'ada@acme.test',
  is_primary_contact: 'true',
}).payload, {
  account_id: 'acct-a',
  first_name: 'Ada',
  last_name: 'Lovelace',
  email: 'ada@acme.test',
  is_primary_contact: true,
  source: 'business-os-customers-ui',
});
assert.deepEqual(hooks.buildAccountArchiveCommand('acct-a').payload, { account_id: 'acct-a' });
assert.deepEqual(hooks.buildContactArchiveCommand('contact-a').payload, { contact_id: 'contact-a' });
assert.equal(hooks.validateOpportunityDraft({ account_id: 'acct-a', name: '' }).valid, false);
assert.deepEqual(hooks.buildOpportunityCreateCommand({
  account_id: 'acct-a',
  name: 'Renewal 2026',
  opportunity_type: 'renewal',
  stage: 'proposal',
  amount: '10.000',
  probability: '40',
  close_date: '2026-05-30',
}).payload, {
  account_id: 'acct-a',
  name: 'Renewal 2026',
  opportunity_type: 'renewal',
  stage: 'proposal',
  amount_cents: 1000000,
  currency: 'EUR',
  close_date_ms: Date.parse('2026-05-30T00:00:00.000Z'),
  probability: 40,
  source: 'business-os-customers-ui',
});
assert.deepEqual(hooks.buildOpportunityMoveStageCommand('opp-a', 'committed').payload, { opportunity_id: 'opp-a', stage: 'committed' });
assert.equal(hooks.buildOpportunityCloseCommand('opp-a', 'won').command_type, 'customers.opportunity.close_won');
assert.equal(hooks.buildOpportunityCloseCommand('opp-a', 'lost').command_type, 'customers.opportunity.close_lost');
assert.equal(hooks.buildOpportunityCloseCommand('opp-a', 'lost').payload.lost_reason, 'Closed lost');
assert.equal(hooks.validateTaskDraft({ account_id: 'acct-a', title: '' }).valid, false);
assert.deepEqual(hooks.buildTaskCreateCommand({
  account_id: 'acct-a',
  opportunity_id: 'opp-a',
  title: 'Follow up',
  body: 'Call Ada',
  status: 'in_progress',
  due_at: '2026-05-28',
  assignee_id: 'user-a',
}).payload, {
  title: 'Follow up',
  body: 'Call Ada',
  status: 'in_progress',
  due_at_ms: Date.parse('2026-05-28T00:00:00.000Z'),
  assignee_id: 'user-a',
  account_id: 'acct-a',
  opportunity_id: 'opp-a',
  source: 'business-os-customers-ui',
});
assert.deepEqual(hooks.buildTaskUpdateCommand('task-a', { account_id: 'acct-a', title: 'Done', status: 'completed' }).payload, {
  task_id: 'task-a',
  title: 'Done',
  status: 'completed',
  account_id: 'acct-a',
  source: 'business-os-customers-ui',
});
assert.deepEqual(hooks.buildTaskCompleteCommand('task-a').payload, { task_id: 'task-a' });
assert.equal(hooks.validateNoteDraft({ account_id: 'acct-a', title: '', body: '' }).valid, false);
assert.deepEqual(hooks.buildNoteCreateCommand({
  account_id: 'acct-a',
  contact_id: 'contact-a',
  title: 'Meeting',
  body: 'Budget confirmed',
  body_format: 'markdown',
}).payload, {
  title: 'Meeting',
  body: 'Budget confirmed',
  body_format: 'markdown',
  account_id: 'acct-a',
  contact_id: 'contact-a',
  source: 'business-os-customers-ui',
});
assert.deepEqual(hooks.buildNoteUpdateCommand('note-a', { account_id: 'acct-a', title: 'Updated', body_format: 'plain' }).payload, {
  note_id: 'note-a',
  title: 'Updated',
  body_format: 'plain',
  account_id: 'acct-a',
  source: 'business-os-customers-ui',
});
assert.equal(hooks.buildSaveViewCommand({
  stage: 'renewal',
  health: 'at_risk',
  search: 'Beta',
  sort: { field: 'name', direction: 'asc' },
}).payload.filters.length, 3);

console.log('customers schema smoke OK');
