import test from 'node:test';
import assert from 'node:assert/strict';

const {
  buildAppCreateCommand,
  computeCreatorActionState,
  deriveModuleIdFromRequest,
  titleFromModuleId,
  normalizeCreatorInstalledApps,
  normalizeCreatorRequestSuggestions,
  normalizeCollectionName,
  normalizeModuleId,
  validateCreatorSpec,
} = await import('./index.js');

test('empty request blocks app creation', () => {
  const state = computeCreatorActionState({
    request: '',
    appId: '',
    appTitle: '',
    appDesc: '',
    appCollections: [],
  });

  assert.equal(state.deployReady, false);
  assert.match(state.diagnostic, /Auftrag fehlt/);
});

test('plain request enables CTOX app creation without a local planning step', () => {
  const request = 'Erstelle eine App fuer Support Tickets';
  const state = computeCreatorActionState({
    request,
    appId: '',
    appTitle: '',
    appDesc: '',
    appCollections: [],
  });

  assert.equal(state.deployReady, true);
  assert.equal(state.validationErrors.length, 0);
});

test('manual advanced edits only add optional hints and still surface validation errors', () => {
  const request = 'Erstelle eine Support Desk App';
  const state = computeCreatorActionState({
    request,
    appId: 'supportdesk',
    appTitle: 'x'.repeat(121),
    appDesc: '',
    appCollections: [],
  });

  assert.equal(state.deployReady, false);
  assert.equal(state.validationErrors[0], 'Titel ist zu lang.');
  assert.match(state.diagnostic, /Titel ist zu lang/);
});

test('request metadata derivation only creates a neutral module id and title', () => {
  const moduleId = deriveModuleIdFromRequest('Eine CRM App fuer Kunden und Kontakte', 1234);

  assert.equal(moduleId, 'eine-crm-app-fuer-kunden');
  assert.equal(titleFromModuleId(moduleId), 'Eine Crm App Fuer Kunden');
});

test('slug and collection normalization reject unsafe names', () => {
  assert.equal(normalizeModuleId('  Meine App!! '), 'meine-app');
  assert.equal(normalizeCollectionName(' Neue Collection!! '), 'neue_collection');
  assert.deepEqual(
    validateCreatorSpec({
      appId: '',
      appTitle: '',
      appDesc: '',
      appCollections: [],
    }),
    [],
  );
});

test('creator right rail only treats installed modules as custom apps', () => {
  const apps = normalizeCreatorInstalledApps({
    modules: [
      { id: 'creator', title: 'App Creator', entry: 'modules/creator/index.html', source: 'core' },
      { id: 'notes', title: 'Notizen', entry: 'modules/notes/index.html', source: 'starter' },
      { id: 'crm-kontakte', title: 'CRM Kontakte', entry: 'installed-modules/crm-kontakte/index.html', source: 'installed', version: 'v2' },
    ],
  });

  assert.deepEqual(apps.map((app) => app.id), ['crm-kontakte']);
  assert.equal(apps[0].version, 'v2');
});

test('creator request suggestions are sorted and trimmed to actionable requests', () => {
  const requests = normalizeCreatorRequestSuggestions([
    { id: 'ignore', module: 'notes', command_type: 'notes.create', payload: { instruction: 'Keine Creator App' }, updated_at_ms: 10 },
    { id: 'old', module: 'creator', command_type: 'business_os.chat.task', payload: { instruction: 'Alte App', title: 'Alt' }, status: 'done', updated_at_ms: 20 },
    { id: 'new', command_type: 'ctox.business_os.app.modify', payload: { instruction: 'Neue App bauen', title: 'Neu' }, status: 'pending', updated_at_ms: 30 },
  ]);

  assert.deepEqual(requests.map((item) => item.id), ['new', 'old']);
  assert.equal(requests[0].request, 'Neue App bauen');
});

test('creator builds an app create command instead of writing module source directly', () => {
  const command = buildAppCreateCommand({
    appId: 'Meine Inventar App',
    appTitle: 'Inventar',
    appDesc: 'Bestand verwalten',
    appCategory: 'Management',
    appLayout: 'full-workspace',
    appCollections: ['items', 'stock events'],
    appVersion: '0.2.0',
    instruction: 'Baue eine Inventar App.',
    actor: { id: 'admin' },
    now: 1234,
  });

  assert.equal(command.type, 'ctox.business_os.app.create');
  assert.equal(command.command_type, 'ctox.business_os.app.create');
  assert.equal(command.record_id, 'meine-inventar-app');
  assert.equal(command.payload.module_id, 'meine-inventar-app');
  assert.equal(command.payload.install_target, 'runtime-installed-module');
  assert.equal(command.payload.target, 'app');
  assert.deepEqual(command.payload.required_skills, ['business-os-app-module-development']);
  assert.deepEqual(command.payload.collections_hint, ['items', 'stock_events']);
  assert.equal(command.client_context.source, 'business-os-creator');
  assert.equal(command.client_context.install_target, 'runtime-installed-module');
});

test('creator command keeps app structure agent-led when only a request is provided', () => {
  const command = buildAppCreateCommand({
    instruction: 'Baue eine Vertragsverwaltung mit Fristen und CTOX Follow-up.',
    now: 1234,
  });

  assert.equal(command.type, 'ctox.business_os.app.create');
  assert.equal(command.record_id, 'baue-eine-vertragsverwaltung-mit-fristen');
  assert.equal(command.payload.module_id, 'baue-eine-vertragsverwaltung-mit-fristen');
  assert.equal(command.payload.instruction, 'Baue eine Vertragsverwaltung mit Fristen und CTOX Follow-up.');
  assert.equal(command.payload.install_target, 'runtime-installed-module');
  assert.deepEqual(command.payload.collections_hint, []);
  assert.equal(command.payload.layout_hint, '');
  assert.deepEqual(command.payload.required_skills, ['business-os-app-module-development']);
});
