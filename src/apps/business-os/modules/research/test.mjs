import test from 'node:test';
import assert from 'node:assert/strict';

import { __researchTestHooks as hooks } from './index.js';

const bases = [
  { domain: 'research/vendor-ai-agents', title: 'Vendor AI Agents' },
];

test('create dialog validation requires title, local domain, and task prompt', () => {
  assert.equal(hooks.validateResearchTaskInput({ title: '', domain: bases[0].domain, prompt: 'Analyse' }, bases).valid, false);
  assert.equal(hooks.validateResearchTaskInput({ title: 'Vendor Research', domain: 'research/missing', prompt: 'Analyse' }, bases).valid, false);
  assert.equal(hooks.validateResearchTaskInput({ title: 'Vendor Research', domain: bases[0].domain, prompt: '' }, bases).valid, false);
  assert.equal(hooks.validateResearchTaskInput({ title: 'Vendor Research', domain: bases[0].domain, prompt: 'Analyse vendors' }, bases).valid, true);
});

test('run button validation requires a selected task with a loaded knowledge domain', () => {
  assert.equal(hooks.validateSelectedResearchTask(null, bases).valid, false);
  assert.equal(hooks.validateSelectedResearchTask({ id: 'task-1', title: 'Vendor Research', knowledge_domain: '' }, bases).valid, false);
  assert.equal(hooks.validateSelectedResearchTask({ id: 'task-1', title: 'Vendor Research', knowledge_domain: 'research/missing' }, bases).valid, false);
  assert.equal(hooks.validateSelectedResearchTask({ id: 'task-1', title: 'Vendor Research', knowledge_domain: bases[0].domain }, bases).valid, true);
});

test('diagnostic rows distinguish sync failures from local no-data', () => {
  const rows = hooks.collectionDiagnosticRows(['research_runs', 'research_notes', 'knowledge_tables'], {
    research_runs: { sync: { kind: 'failed', message: 'WebRTC replication failed' } },
    research_notes: { sync: { kind: 'local', message: 'Lokaler Modus' } },
    knowledge_tables: { read: { kind: 'ok', message: '0 rows' } },
  });

  assert.deepEqual(rows.map((row) => row.kind), ['failed', 'local', 'ok']);
  assert.match(rows[0].label, /WebRTC/);
});
