import test from 'node:test';
import assert from 'node:assert/strict';
import { __ctoxTestHooks as hooks } from '../index.js';

const state = { lang: 'de', model: { timeline: [] } };
const reviewed = {
  status: 'completed', routeStatus: 'handled',
  executionProgress: hooks.normalizeExecutionProgress({ version: 1, phase: 'completed', percent: 100 }),
};

test('reviewed task endpoint stays closed without retained flow events', () => {
  const endpoint = hooks.outboundEndpointForTask(reviewed, null, state);
  assert.equal(endpoint.fromNodeId, 'passed');
  assert.equal(endpoint.closed, true);
  assert.equal(endpoint.label, 'Ausgeliefert / geschlossen');
});

test('durable task state overrides a stale selected terminal node', () => {
  const oldSuccess = { id: 'passed', status: 'done' };
  const failed = hooks.outboundEndpointForTask({ ...reviewed, routeStatus: 'failed' }, oldSuccess, state);
  assert.equal(failed.fromNodeId, 'model-failed');
  assert.equal(failed.label, 'Fehlgeschlagen');
  for (const phase of ['accepted', 'running', 'awaiting_review', 'validating', 'blocked']) {
    const endpoint = hooks.outboundEndpointForTask({ ...reviewed, executionPhase: phase }, oldSuccess, state);
    assert.equal(endpoint.closed, false, phase);
  }
});

test('handled routing alone does not prove completion', () => {
  const endpoint = hooks.outboundEndpointForTask({ status: 'handled', routeStatus: 'handled' }, null, state);
  assert.equal(endpoint.closed, false);
  assert.equal(endpoint.label, 'Abschluss nicht belegt');
});
