import test from 'node:test';
import assert from 'node:assert/strict';

import { __conversationsTestHooks as hooks } from './index.js';

const t = (key, fallback) => fallback || key;

test('conversation empty state distinguishes filter misses from sync failures', () => {
  const noResults = hooks.conversationEmptyState({
    totalBuckets: 2,
    filteredBuckets: 0,
    hasActiveFilters: true,
    diagnostics: { hasFailure: false, isStarting: false },
    t,
  });

  assert.equal(noResults.kind, 'no-results');

  const syncFailure = hooks.conversationEmptyState({
    totalBuckets: 0,
    filteredBuckets: 0,
    hasActiveFilters: false,
    diagnostics: {
      hasFailure: true,
      isStarting: false,
      problemCollections: ['communication_messages'],
      detail: 'communication_messages: WebRTC replication failed',
    },
    t,
  });

  assert.equal(syncFailure.kind, 'sync-failure');
  assert.match(syncFailure.body, /communication_messages/);
});

test('communication diagnostics include accounts and messages sync errors', () => {
  const diagnostics = hooks.buildConversationDataDiagnostics({
    missingCollections: new Set(['communication_accounts']),
    collectionErrors: new Map([
      ['communication_messages', new Error('load failed')],
    ]),
    syncDiagnostics: {
      collections: {
        communication_messages: {
          status: 'failed',
          lastError: { message: 'WebRTC replication failed' },
        },
      },
    },
  });

  assert.equal(diagnostics.hasFailure, true);
  assert.deepEqual(diagnostics.problemCollections.sort(), [
    'communication_accounts',
    'communication_messages',
  ]);
  assert.match(diagnostics.detail, /communication_accounts/);
  assert.match(diagnostics.detail, /communication_messages/);
});

test('communication diagnostics treats failed sync status as failure without lastError', () => {
  const diagnostics = hooks.buildConversationDataDiagnostics({
    syncDiagnostics: {
      collections: {
        communication_accounts: { status: 'connected' },
        communication_messages: { status: 'failed' },
      },
    },
  });

  assert.equal(diagnostics.hasFailure, true);
  assert.deepEqual(diagnostics.problemCollections, ['communication_messages']);
});

test('bucket filters report active account, channel, direction, date, and search state', () => {
  assert.equal(hooks.hasActiveListFilters({
    channel: 'all',
    account: '',
    direction: 'any',
    dateRange: 'any',
    search: '',
  }), false);

  assert.equal(hooks.hasActiveListFilters({
    channel: 'email',
    account: '',
    direction: 'any',
    dateRange: 'any',
    search: '',
  }), true);
});
