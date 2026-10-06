import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

const appSource = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const controlStart = appSource.indexOf('const WORKJET_COMPUTER_CONTROL_MAX_RESULTS');
const controlEnd = appSource.indexOf('async function waitForSyncBridgeReady', controlStart);
const controlSource = appSource.slice(controlStart, controlEnd);
const syncWaitEnd = appSource.indexOf('\n}\n', controlEnd) + 3;
const computerRuntimeSource = controlSource + appSource.slice(controlEnd, syncWaitEnd);

for (const status of ['assigned', 'unassigned']) {
  test(`computer projection waits through nested replication state for ${status}`, async () => {
    let pulled = false;
    const computer = {
      id: 'computer-nested', display_name: 'Recovery computer', hosting_mode: 'workstation',
      status, capabilities: [], owner_user_id: 'owner-1',
    };
    const context = vm.createContext({
      state: { db: { collection: () => ({ findOne: () => ({ exec: async () => pulled ? computer : null }) }) } },
      window: { setTimeout }, setTimeout, clearTimeout, Date,
    });
    vm.runInContext(computerRuntimeSource, context);
    context.bridge = { state: { async awaitInSync() { pulled = true; } } };
    context.expectedStatus = status;
    const result = await vm.runInContext(
      "waitForProjectedWorkjetComputer('computer-nested', 'owner-1', expectedStatus, bridge, 200)", context,
    );
    assert.equal(result.id, computer.id);
    assert.equal(result.status, status);
    assert.equal(pulled, true);
  });
}


test('Workjet guest computer control is installed and WebRTC/RxDB-only', () => {
  assert.match(appSource, /globalThis\.workjetComputerControl = workjetComputerControl/);
  assert.ok(controlStart >= 0 && controlEnd > controlStart, 'computer control implementation exists');
  assert.match(controlSource, /action === 'computer\.list'/);
  assert.match(controlSource, /action === 'computer\.assign'/);
  assert.match(controlSource, /action === 'computer\.unassign'/);
  assert.match(controlSource, /command_type: 'ctox\.workjet\.computer\.list'/);
  assert.match(controlSource, /command_type: 'ctox\.workjet\.computer\.assign'/);
  assert.match(controlSource, /command_type: 'ctox\.workjet\.computer\.unassign'/);
  assert.match(controlSource, /startCollection\?\.\('business_commands'\)/);
  assert.match(controlSource, /startCollection\?\.\('workjet_computers'\)/);
  assert.doesNotMatch(controlSource, /fetch\s*\(|XMLHttpRequest|\/api\/|https?:\/\//);
  assert.doesNotMatch(controlSource, /hostname|presentation|environment/i);
});

test('Workjet guest computer control rejects managed hosts and gates co-location', async () => {
  const records = [];
  const collection = {
    find({ selector = {}, limit = Number.MAX_SAFE_INTEGER } = {}) {
      return {
        async exec() {
          return records.filter((record) => Object.entries(selector).every(
            ([field, condition]) => record[field] === condition?.$eq,
          )).slice(0, limit);
        },
      };
    },
    findOne(id) {
      return { async exec() { return records.find((record) => record.id === id) || null; } };
    },
  };
  const state = {
    session: { id: 'owner-1' },
    db: { collection: (name) => (name === 'workjet_computers' ? collection : {}) },
    sync: { async startCollection() { return { async awaitInSync() {} }; } },
    commandBus: {
      async dispatch(command) {
        if (command.command_type === 'ctox.workjet.computer.assign') {
          const record = {
            id: command.payload.computer_id,
            display_name: command.payload.display_name,
            hosting_mode: command.payload.hosting_mode,
            status: 'assigned',
            capabilities: command.payload.capabilities,
            self_hosted_colocation: command.payload.self_hosted_colocation,
            owner_user_id: 'owner-1',
          };
          const index = records.findIndex((candidate) => candidate.id === record.id);
          if (index >= 0) records[index] = record;
          else records.push(record);
        }
        if (command.command_type === 'ctox.workjet.computer.unassign') {
          records.find((record) => record.id === command.payload.computer_id).status = 'unassigned';
        }
        return { status: 'completed' };
      },
    },
  };
  let nextId = 0;
  const context = {
    state,
    actorContext: (session) => ({ id: session.id }),
    newId: () => String(++nextId),
    waitForSyncBridgeReady: async () => {},
    window: { setTimeout },
    setTimeout,
  };
  vm.runInNewContext(`${controlSource}\nglobalThis.__control = workjetComputerControl;`, context);
  const invoke = async (request) => JSON.parse(JSON.stringify(await context.__control(request)));

  await assert.rejects(invoke({
    action: 'computer.assign',
    commandId: 'managed',
    computerId: 'opaque-1',
    displayName: 'Managed backend',
    hostingMode: 'managed_backend',
  }), /backend-only/);
  await assert.rejects(invoke({
    action: 'computer.assign',
    commandId: 'co-located-without-confirmation',
    computerId: 'opaque-2',
    displayName: 'Self-hosted',
    hostingMode: 'self_hosted',
    selfHostedColocation: true,
  }), /workjet-self-host-colocation\.v1/);

  const assigned = await invoke({
    action: 'computer.assign',
    commandId: 'assign-1',
    computerId: 'opaque-device-key-1',
    displayName: 'Current Mac',
    hostingMode: 'workstation',
    capabilities: ['codex', 'claude', 'codex'],
  });
  assert.deepEqual(assigned.computer, {
    id: 'opaque-device-key-1',
    displayName: 'Current Mac',
    hostingMode: 'workstation',
    status: 'assigned',
    capabilities: ['claude', 'codex'],
    selfHostedColocation: false,
  });
  assert.equal((await invoke({ action: 'computer.list' })).computers.length, 1);

  const unassigned = await invoke({
    action: 'computer.unassign',
    commandId: 'unassign-1',
    computerId: 'opaque-device-key-1',
  });
  assert.equal(unassigned.computer.status, 'unassigned');
  assert.deepEqual((await invoke({ action: 'computer.list' })).computers, []);
});
