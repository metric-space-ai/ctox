import assert from 'node:assert/strict';
import test from 'node:test';

import {
  CTOX_MAINTENANCE_SYNC_MESSAGE,
  isDataEmptyStateText,
  maintenanceRequiredCollections,
  normalizeMaintenancePayload,
} from './maintenance-state.js';

test('normalizes an instance-scoped active maintenance lease', () => {
  const state = normalizeMaintenancePayload({
    active: true,
    message: 'CTOX wird aktualisiert – Daten bleiben erhalten',
    state: {
      lease_id: 'upgrade-1',
      phase: 'waiting_collections',
      status: 'active',
      service_active: true,
      replication_up: true,
      progress: { percent: 96, message: CTOX_MAINTENANCE_SYNC_MESSAGE },
    },
  });
  assert.equal(state.active, true);
  assert.equal(state.leaseId, 'upgrade-1');
  assert.equal(state.percent, 96);
  assert.equal(state.replicationUp, true);
});

test('module readiness excludes demand-only blob collections', () => {
  assert.deepEqual(maintenanceRequiredCollections({
    collections: ['research_runs', 'document_blob_chunks', 'research_runs', 'knowledge_tables'],
  }), ['knowledge_tables', 'research_runs']);
});

test('maintenance guard recognizes true data-empty states but not no-selection hints', () => {
  assert.equal(isDataEmptyStateText('Noch keine Knowledge Base verfügbar'), true);
  assert.equal(isDataEmptyStateText('No documents available'), true);
  assert.equal(isDataEmptyStateText('Keine Datei ausgewählt.'), false);
});
