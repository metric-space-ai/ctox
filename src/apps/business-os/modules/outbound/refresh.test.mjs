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
// v4 (App-API 4.0): Die Reload-Maschinerie (scheduleDataRefresh/runDataRefresh, 10-s-Operational-Refresh,
// 2,5-s-Knowledge-Watch, 5-min-Scheduler) ist ersatzlos entfallen. Die Regel dahinter bleibt geprüft:
// Bursts erzeugen genau EINE Ableitung + Darstellung, nach unmount nichts mehr, und es gibt keine Schleife.
function runtime() {
  const timers = new Map();
  let nextId = 1;
  const counters = { derive: 0, render: 0, ensureDefault: 0 };
  const state = { liveTimer: null, mountToken: {} };
  const ctx = vm.createContext({
    state,
    window: {
      setTimeout(fn, delay) { const id = nextId++; timers.set(id, { fn, delay }); return id; },
      clearTimeout(id) { timers.delete(id); },
    },
    deriveState: () => { counters.derive++; },
    ensureDefaultCampaign: () => { counters.ensureDefault++; },
    render: () => { counters.render++; },
  });
  vm.runInContext(functionSource('scheduleLiveRender'), ctx);
  return { ctx, state, counters, timers,
    fireTimer() {
      const [id, entry] = timers.entries().next().value;
      timers.delete(id);
      return entry.fn();
    },
  };
}

test('Live-Bursts aus mehreren Collections ergeben genau eine Ableitung und eine Darstellung', () => {
  const rt = runtime();
  for (let i = 0; i < 25; i += 1) rt.ctx.scheduleLiveRender();
  assert.equal(rt.timers.size, 1);
  rt.fireTimer();
  assert.deepEqual(rt.counters, { derive: 1, render: 1, ensureDefault: 1 });
  rt.ctx.scheduleLiveRender();
  assert.equal(rt.timers.size, 1, 'nach dem Lauf wieder planbar');
});

test('nach unmount (mountToken weg) rendert eine anstehende Live-Darstellung nicht mehr', () => {
  const rt = runtime();
  rt.ctx.scheduleLiveRender();
  rt.state.mountToken = null;
  rt.fireTimer();
  assert.deepEqual(rt.counters, { derive: 0, render: 0, ensureDefault: 0 });
});

test('keine Reload-Schleifen, kein Polling, kein Warten auf Befehle im Modulcode', async () => {
  const active = await readFile(new URL('./active-outreach.js', import.meta.url), 'utf8');
  for (const [name, code] of [['index.js', source], ['active-outreach.js', active]]) {
    const live = code.split('\n').filter((line) => !/^\s*\/\//.test(line)).join('\n');
    assert.doesNotMatch(live, /setInterval\(/, `${name}: setInterval`);
    assert.doesNotMatch(live, /function (scheduleDataRefresh|runDataRefresh|startKnowledgeProjectionWatch|refreshOperationalStateInBackground|waitForOutboundCommandProjection|waitForCampaignProjection)\b/, `${name}: Reload-/Poll-Funktion`);
    assert.doesNotMatch(live, /commandBus|\.exec\(\)|ctx\.sync/, `${name}: v3-Datenweg`);
  }
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
