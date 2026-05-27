import test from 'node:test';
import assert from 'node:assert/strict';

import {
  computeCreatorActionState,
  deriveSpecFromPrompt,
  normalizeCollectionName,
  normalizeModuleId,
  validateCreatorSpec,
} from './index.js';

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
      'Mindestens eine RxDB Collection ist erforderlich.',
    ],
  );
});
