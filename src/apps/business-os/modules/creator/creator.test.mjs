import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { fileURLToPath } from 'node:url';

import { build } from 'esbuild';

const bundledModule = await build({
  entryPoints: [fileURLToPath(new URL('./index.js', import.meta.url))],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  write: false,
});

const [{ text: bundledSource }] = bundledModule.outputFiles;
const {
  computeCreatorActionState,
  deriveSpecFromPrompt,
  normalizeCreatorInstalledApps,
  normalizeCreatorPromptSuggestions,
  normalizeCollectionName,
  normalizeModuleId,
  validateCreatorSpec,
} = await import(
  `data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`
);

test('empty prompt blocks optimize and deploy', () => {
  const state = computeCreatorActionState({
    prompt: '',
    specPrompt: '',
    appId: 'lagerverwaltung',
    appTitle: 'Lagerverwaltung',
    appDesc: 'Beschreibung',
    appCollections: ['inventory_records'],
  });

  assert.equal(state.optimizeReady, false);
  assert.equal(state.deployReady, false);
  assert.match(state.diagnostic, /Prompt fehlt/);
});

test('stale prompt blocks install until spec is regenerated', () => {
  const state = computeCreatorActionState({
    prompt: 'Erstelle eine Zeiterfassung',
    specPrompt: 'Erstelle eine Lagerverwaltung',
    appId: 'zeiterfassung',
    appTitle: 'Zeiterfassung',
    appDesc: 'Beschreibung',
    appCollections: ['time_logs'],
  });

  assert.equal(state.optimizeReady, true);
  assert.equal(state.deployReady, false);
  assert.match(state.diagnostic, /nicht aktuell/);
});

test('fresh valid spec enables deploy', () => {
  const prompt = 'Erstelle eine App fuer Support Tickets';
  const state = computeCreatorActionState({
    prompt,
    specPrompt: prompt,
    appId: 'supportdesk',
    appTitle: 'Support Desk',
    appDesc: 'Beschreibung',
    appCollections: ['tickets', 'ticket_comments'],
  });

  assert.equal(state.optimizeReady, true);
  assert.equal(state.deployReady, true);
  assert.equal(state.validationErrors.length, 0);
});

test('manual advanced edits keep a fresh spec but still surface validation errors', () => {
  const prompt = 'Erstelle eine Support Desk App';
  const state = computeCreatorActionState({
    prompt,
    specPrompt: prompt,
    appId: 'supportdesk',
    appTitle: '',
    appDesc: 'Beschreibung',
    appCollections: ['tickets'],
  });

  assert.equal(state.deployReady, false);
  assert.equal(state.validationErrors[0], 'Titel fehlt.');
  assert.match(state.diagnostic, /Titel fehlt/);
});

test('prompt derivation normalizes generated ids and collections', () => {
  const spec = deriveSpecFromPrompt('Eine CRM App fuer Kunden und Kontakte');

  assert.equal(spec.id, 'crm-kontakte');
  assert.equal(spec.category, 'Finance');
  assert.deepEqual(spec.collections, ['customers', 'interactions']);
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
    [
      'Modul-ID fehlt oder ist ungültig.',
      'Titel fehlt.',
      'Beschreibung fehlt.',
      'Mindestens eine Datentabelle ist erforderlich.',
    ],
  );
});

test('creator right rail only treats generated installed modules as custom apps', () => {
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

test('creator prompt suggestions are sorted and trimmed to actionable prompts', () => {
  const prompts = normalizeCreatorPromptSuggestions([
    { id: 'ignore', module: 'notes', command_type: 'notes.create', payload: { prompt: 'Keine Creator App' }, updated_at_ms: 10 },
    { id: 'old', module: 'creator', command_type: 'business_os.chat.task', payload: { prompt: 'Alte App', title: 'Alt' }, status: 'done', updated_at_ms: 20 },
    { id: 'new', command_type: 'ctox.business_os.app.modify', payload: { instruction: 'Neue App bauen', title: 'Neu' }, status: 'pending', updated_at_ms: 30 },
  ]);

  assert.deepEqual(prompts.map((item) => item.id), ['new', 'old']);
  assert.equal(prompts[0].prompt, 'Neue App bauen');
});
