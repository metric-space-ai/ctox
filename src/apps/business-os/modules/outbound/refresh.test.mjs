import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('./index.js', import.meta.url), 'utf8');
function functionSource(name) {
  const body = source.match(new RegExp(`(?:async )?function ${name}\\([^\\n]*\\) \\{[\\s\\S]*?\\n\\}`))?.[0];
  assert.ok(body, `real module function ${name}`);
  return body;
}
function deferred() {
  let resolve;
  const promise = new Promise((r) => { resolve = r; });
  return { promise, resolve };
}
function runtime(overrides = {}) {
  const timers = new Map();
  const intervals = new Map();
  const events = new Map();
  let nextId = 1;
  const counters = { load: 0, active: 0, render: 0, focus: 0, operational: 0, warnings: 0 };
  const state = { refreshEnabled: true, cleanup: [], lastOperationalRefreshMs: 0 };
  const ctx = vm.createContext({
    state, Date,
    window: {
      setTimeout(fn, delay) { const id = nextId++; timers.set(id, { fn, delay }); return id; },
      clearTimeout(id) { timers.delete(id); },
      setInterval(fn) { const id = nextId++; intervals.set(id, fn); return id; },
      clearInterval(id) { intervals.delete(id); },
    },
    console: { warn() { counters.warnings++; } },
    loadAll: async () => { counters.load++; },
    loadActiveOutreachData: async () => { counters.active++; },
    render: () => { counters.render++; },
    reportUnavailableOutboundFocus: () => { counters.focus++; },
    refreshOperationalStateInBackground: () => { counters.operational++; },
    refreshKnowledgeProjectionIfChanged: async () => false,
    outboundCollection(name) {
      return { $: { subscribe(fn) { events.set(name, fn); return { unsubscribe() { events.delete(name); } }; } } };
    },
    optionalReadableCollection(name) { return ctx.outboundCollection(name); },
    ...overrides,
  });
  vm.runInContext([
    'wireRealtime', 'scheduleDataRefresh', 'runDataRefresh',
    'scheduleOperationalRefresh', 'startKnowledgeProjectionWatch',
  ].map(functionSource).join('\n'), ctx);
  return { ctx, state, counters, timers, intervals, events,
    async fireTimer() {
      const [id, entry] = timers.entries().next().value;
      timers.delete(id);
      return entry.fn();
    },
  };
}

test('replication bursts during a slow read create one trailing reload without overlap', async () => {
  const blocked = deferred();
  let calls = 0;
  const r = runtime({ loadAll: async () => { calls++; if (calls === 1) await blocked.promise; } });
  const first = r.ctx.runDataRefresh();
  await r.ctx.runDataRefresh();
  await r.ctx.runDataRefresh();
  assert.equal(calls, 1);
  assert.equal(r.timers.size, 0);
  blocked.resolve();
  await first;
  assert.equal(r.timers.size, 1);
  await r.fireTimer();
  assert.equal(calls, 2);
  assert.equal(r.timers.size, 0);
  assert.equal(r.counters.focus, 2, 'preserve main navigation receipts');
});

test('a rejected read releases the flight and the next invalidation retries', async () => {
  let calls = 0;
  const r = runtime({ loadAll: async () => { if (++calls === 1) throw new Error('offline'); } });
  await r.ctx.runDataRefresh();
  assert.equal(r.state.refreshInFlight, false);
  assert.equal(r.counters.warnings, 1);
  assert.equal(r.counters.render, 0, 'failed reads cannot report fresh results');
  r.ctx.scheduleDataRefresh(0);
  await r.fireTimer();
  assert.equal(calls, 2);
  assert.equal(r.counters.render, 1);
});

test('command/queue writes refresh activity once and never reload company data', async () => {
  const r = runtime();
  r.ctx.wireRealtime();
  for (let i = 0; i < 30; i++) {
    r.events.get('business_commands')();
    r.events.get('ctox_queue_tasks')();
  }
  assert.equal(r.timers.size, 1);
  await r.fireTimer();
  assert.equal(r.counters.operational, 1);
  assert.equal(r.counters.load, 0);
  r.events.get('outbound_companies')();
  r.events.get('outbound_research_runs')();
  assert.equal(r.timers.size, 1);
  await r.fireTimer();
  assert.equal(r.counters.load, 1);
  r.state.cleanup.forEach((fn) => fn());
  assert.equal(r.events.size, 0);
  assert.equal(r.intervals.size, 0);
});

test('disposing while a read is pending prevents rendering and a trailing reload', async () => {
  const blocked = deferred();
  const r = runtime({ loadAll: () => blocked.promise });
  r.ctx.wireRealtime();
  const first = r.ctx.runDataRefresh();
  await r.ctx.runDataRefresh();
  r.state.cleanup.forEach((fn) => fn());
  blocked.resolve();
  await first;
  assert.equal(r.counters.active, 0);
  assert.equal(r.counters.render, 0);
  assert.equal(r.timers.size, 0);
  r.ctx.scheduleDataRefresh(0);
  assert.equal(r.timers.size, 0);
});

test('slow knowledge reads do not overlap interval ticks', async () => {
  const blocked = deferred();
  let calls = 0;
  const r = runtime({ refreshKnowledgeProjectionIfChanged: async () => { calls++; await blocked.promise; return true; } });
  r.ctx.startKnowledgeProjectionWatch();
  const tick = r.intervals.values().next().value;
  const first = tick();
  await tick();
  assert.equal(calls, 1);
  blocked.resolve();
  await first;
  assert.equal(r.counters.render, 1);
  await tick();
  assert.equal(calls, 2);
});

test('indexed lookups preserve first/newest/tie semantics and refresh after array replacement', () => {
  const state = {
    commands: [{ id: 'c', status: 'old' }],
    queueTasks: [
      { id: 't1', command_id: 'c', updated_at_ms: 5 },
      { id: 't2', client_command_id: 'c', updated_at_ms: 8 },
      { id: 't3', command_id: 'c', updated_at_ms: 8 },
    ],
    runs: [{ id: 'r1', run_type: 'research', company_id: 'a', updated_at_ms: 3 }],
  };
  const ctx = vm.createContext({ state, commandStatusForCommand: (row) => row.status });
  vm.runInContext('const positionsCache = new WeakMap(); const firstIndexCache = new WeakMap();\n'
    + ['positionsBy', 'newestRowAt', 'firstIndexBy', 'firstRowForIds',
      'commandStatusForRun', 'queueTaskForCommand', 'latestAutomationRun'].map(functionSource).join('\n'), ctx);
  assert.equal(ctx.commandStatusForRun({ command_id: 'c' }), 'old');
  assert.equal(ctx.queueTaskForCommand({ command_id: 'c' }).id, 't2');
  assert.equal(ctx.latestAutomationRun('research', ['a']).id, 'r1');
  state.commands = [{ id: 'c', status: 'new' }];
  state.queueTasks = [{ id: 't4', command_id: 'c', updated_at_ms: 9 }];
  state.runs = [{ id: 'r2', run_type: 'research', company_id: 'a', updated_at_ms: 4 }];
  assert.equal(ctx.commandStatusForRun({ command_id: 'c' }), 'new');
  assert.equal(ctx.queueTaskForCommand({ command_id: 'c' }).id, 't4');
  assert.equal(ctx.latestAutomationRun('research', ['a']).id, 'r2');
  assert.equal(ctx.latestAutomationRun('other', ['a']), null);
  const rows = [{ company_id: 'b' }, { company_id: 'a' }];
  assert.equal(ctx.firstRowForIds(rows, 'company_id', ['a', 'b']), rows[0]);
});
