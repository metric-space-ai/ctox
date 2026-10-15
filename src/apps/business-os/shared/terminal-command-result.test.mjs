// SPDX-License-Identifier: MIT OR AGPL-3.0-only
import test from 'node:test';
import assert from 'node:assert/strict';
import { readTerminalCommandResult } from './terminal-command-result.mjs';
import { patchOutboundTerminalReads } from '../scripts/stage-outbound-terminal-results.mjs';

const commandId = 'fixture-read';
const receipt = { ok: true, command_id: commandId, status: 'local_receipt' };
const options = (commandBus, accepts = (v) => Array.isArray(v?.targets)) => ({ commandId, commandBus, accepts, timeoutMs: 100 });
const completed = (result, id = commandId) => ({ command_id: id, status: 'completed', result });

test('local receipt reads the existing terminal command before waiting, without redispatch', async () => {
  const targets = [{ target_key: 'fixture-source' }];
  let reads = 0;
  const result = await readTerminalCommandResult(receipt, options({
    getStatus: async (id) => { assert.equal(id, commandId); reads++; return { toJSON: () => completed({ targets }) }; },
    waitForTerminal: () => { assert.fail('never-synced readiness must not hide a completed local document'); },
    dispatch: () => { assert.fail('must not repeat the original command'); },
  }));
  assert.equal(reads, 1);
  assert.deepEqual(result.targets, targets);
});

test('pending command resumes bounded tracking and accepts only the terminal payload', async () => {
  let waits = 0;
  const result = await readTerminalCommandResult(receipt, options({
    getStatus: async () => ({ command_id: commandId, status: 'accepted', result: { targets: [] } }),
    waitForTerminal: async (id, opts) => { assert.equal(id, commandId); assert.ok(opts.timeoutMs <= 100); waits++; return completed({ result: { targets: [] } }); },
  }));
  assert.equal(waits, 1);
  assert.deepEqual(result, { targets: [] });
});

test('receipt or completed command without the expected payload cannot replace a cached registry', async () => {
  let registry = new Map([['old', { target_key: 'old' }]]);
  let stamp = 123;
  await assert.rejects(async () => {
    const result = await readTerminalCommandResult(receipt, options({ getStatus: async () => completed({ ok: true }) }));
    registry = new Map(result.targets.map((row) => [row.target_key, row]));
    stamp = Date.now();
  }, /terminal command result/);
  assert.equal(stamp, 123);
  assert.equal(registry.has('old'), true);
});

test('secret-list receipt is not an empty catalog; terminal catalog and extra metadata are retained', async () => {
  const accepts = (v) => Array.isArray(v?.catalog) && (v.extra === undefined || Array.isArray(v.extra));
  await assert.rejects(readTerminalCommandResult(receipt, options({ getStatus: async () => completed({ ok: true }) }, accepts)));
  const catalog = [{ name: 'GOOGLE_BROWSER_LOGIN', is_set: true, updated_at: 'fixture-date' }];
  const result = await readTerminalCommandResult(receipt, options({ getStatus: async () => completed({ catalog, extra: [] }) }, accepts));
  assert.deepEqual(result.catalog, catalog);
  await assert.rejects(readTerminalCommandResult(completed({ catalog, extra: 'wrong-type' }), options({}, accepts)));
});

test('terminal failures and a foreign command identity cannot become empty success', async () => {
  for (const cmd of [
    { command_id: commandId, status: 'failed', result: { error: 'denied' } },
    completed({ ok: false, targets: [], error: 'denied' }),
    completed({ targets: [] }, 'foreign-command'),
  ]) await assert.rejects(readTerminalCommandResult(receipt, options({ getStatus: async () => cmd })));
});

test('a stalled local read is bounded, with no new command or persistent polling loop', async () => {
  await assert.rejects(readTerminalCommandResult(receipt, { ...options({ getStatus: () => new Promise(() => {}) }), timeoutMs: 10 }), { code: 'command_result_timeout' });
});

test('stage patch updates both read contracts atomically, refuses drift and preserves unrelated code', () => {
  const before = `// unrelated source
async function registry() {
    const result = receipt?.result?.result || receipt?.result || receipt;
    const ziele = Array.isArray(result?.targets) ? result.targets : [];
    state.registryStand = Date.now();
}
async function secrets() {
    const outcome = receipt?.result?.result || receipt?.result || receipt || {};
    const eintraege = [...(outcome.catalog || []), ...(outcome.extra || [])];
    state.secretKatalogStand = Date.now();
}`;
  const after = patchOutboundTerminalReads(before);
  assert.equal(after.split('await readTerminalCommandResult').length - 1, 2);
  assert.ok(after.includes('// unrelated source'));
  assert.ok(after.indexOf('await readTerminalCommandResult') < after.indexOf('state.registryStand'));
  assert.throws(() => patchOutboundTerminalReads(before.replace('result.targets : []', 'result.targets : null')), /source changed/);
  assert.throws(() => patchOutboundTerminalReads(after), /already present/);
});
