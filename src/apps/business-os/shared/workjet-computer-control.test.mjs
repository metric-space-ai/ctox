import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

const appSource = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const controlStart = appSource.indexOf('const WORKJET_COMPUTER_CONTROL_MAX_RESULTS');
const controlEnd = appSource.indexOf('async function waitForSyncBridgeReady', controlStart);
const controlSource = appSource.slice(controlStart, controlEnd);
test('computer list opts into bounded operational details without changing legacy replies or owner filtering', async () => {
  const gpu = { kind: 'gpu', model: 'A4500', vram_gib: 20 };
  const own = { id: 'gpu3', display_name: 'gpu3', hosting_mode: 'workstation',
    status: 'assigned', capabilities: ['gpu'], capability_config: [gpu], agentless: false,
    owner_user_id: 'owner-1', self_hosted_colocation: false, private_key: 'never-project' };
  const foreign = { ...own, id: 'foreign', owner_user_id: 'other-owner' };
  const deleted = { ...own, id: 'deleted', is_deleted: true };
  const records = [own, foreign, deleted];
  const commands = [];
  const collection = { find({ selector }) { return { async exec() {
    return records.filter((record) => Object.entries(selector).every(
      ([key, condition]) => record[key] === condition.$eq));
  } }; } };
  const context = {
    state: { session: { id: 'owner-1' },
      db: { collection: (name) => name === 'workjet_computers' ? collection : {} },
      sync: { async startCollection() { return { async awaitInSync() {} }; } },
      commandBus: { async dispatch(command) { commands.push(command); return { status: 'completed' }; } },
    },
    actorContext: (session) => ({ id: session.id }), newId: () => 'id',
    waitForSyncBridgeReady: async () => {}, window: { setTimeout }, setTimeout,
  };
  vm.runInNewContext(`${controlSource}\nglobalThis.__control = workjetComputerControl;`, context);
  const plain = (value) => JSON.parse(JSON.stringify(value));
  const legacy = plain(await context.__control({ action: 'computer.list' }));
  assert.equal(legacy.computers.length, 1);
  assert.equal(legacy.computers[0].id, 'gpu3');
  assert.equal('capabilityConfig' in legacy.computers[0], false);
  assert.equal('agentless' in legacy.computers[0], false);
  const detailed = plain(await context.__control({ action: 'computer.list', includeOperationalDetails: true }));
  assert.deepEqual(detailed.computers, [{ ...legacy.computers[0], capabilityConfig: [gpu], agentless: false }]);
  assert.equal('private_key' in detailed.computers[0], false);
  for (const command of commands) {
    assert.equal(command.command_type, 'ctox.workjet.computer.list');
    assert.deepEqual(plain(command.payload), { limit: 100 });
    assert.equal(command.client_context.actor.id, 'owner-1');
  }
  await assert.rejects(context.__control({ action: 'computer.list', includeOperationalDetails: 'yes' }),
    /Invalid Workjet computer includeOperationalDetails/);
  own.capability_config = [{ ...gpu, private_key: 'secret' }];
  await assert.rejects(context.__control({ action: 'computer.list', includeOperationalDetails: true }),
    /Unsupported Workjet computer payload field/);
});

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

function capabilityControlFixture(receiptTransform = (receipt) => receipt) {
  const commands = [];
  const context = {
    state: {
      session: { id: 'owner-1' },
      db: { collection: () => ({}) },
      sync: { async startCollection() { return {}; } },
      commandBus: {
        async dispatch(command) {
          commands.push(JSON.parse(JSON.stringify(command)));
          const payload = command.payload;
          const computer = {
            id: payload.computer_id, owner_user_id: 'owner-1', display_name: payload.display_name,
            hosting_mode: payload.hosting_mode, status: 'assigned', self_hosted_colocation: false,
            capability_config: payload.capability_config, agentless: payload.agentless,
            capabilities: payload.capability_config?.map((entry) => entry.kind) || [],
          };
          return receiptTransform({
            ok: true, status: 'completed', command_id: command.command_id,
            result: { ok: true, computer, endpoint: {
              id: payload.endpoint_ref, owner_user_id: 'owner-1',
              computer_id: payload.computer_id || 'nas-1',
              enabled: command.command_type.endsWith('.upsert'),
            } },
          });
        },
      },
    },
    actorContext: (session) => ({ id: session.id }),
    newId: () => 'command',
    waitForSyncBridgeReady: async () => {},
    window: { setTimeout }, setTimeout,
  };
  vm.runInNewContext(`${controlSource}\nglobalThis.__control = workjetComputerControl;`, context);
  return { commands,
    invoke: async (request) => JSON.parse(JSON.stringify(await context.__control(request))) };
}

const storageGrant = { kind: 'storage', endpoint_ref: 'endpoint-nas', protocol: 'ssh',
  root: '/volume1/artifacts', quota_gib: null, purposes: ['exchange', 'artifacts', 'artifacts'] };
const nasAssignment = { action: 'computer.assign', commandId: 'assign-nas', computerId: 'nas-1',
  displayName: 'NAS', hostingMode: 'self_hosted', capabilities: [], selfHostedColocation: false,
  agentless: true, capabilityConfig: [storageGrant] };
const sshEndpoint = { protocol: 'ssh', host: 'nas.example.test', port: 22, username: 'admin',
  root: '/volume1/artifacts', host_key_sha256: 'SHA256:example-pin',
  private_key: { scope: 'computer-access', name: 'nas-key' }, passphrase: null };
const endpointRequest = { action: 'computer.endpoint.upsert', commandId: 'endpoint-save',
  computerId: 'nas-1', endpointRef: 'endpoint-nas', connection: sshEndpoint };

test('typed NAS grant uses the native command receipt without fabricating an agent connection', async () => {
  const { commands, invoke } = capabilityControlFixture();
  const response = await invoke(nasAssignment);
  assert.deepEqual(response.computer.capabilities, ['storage']);
  assert.equal(response.computer.id, 'nas-1');
  assert.equal(commands[0].payload.agentless, true);
  assert.deepEqual(commands[0].payload.capability_config[0].purposes, ['artifacts', 'exchange']);
  assert.equal(commands[0].client_context.actor.id, 'owner-1');
});

test('typed capability confirmation rejects failed, foreign, mismatched and uncorrelated receipts', async () => {
  const transforms = [
    (receipt) => ({ ...receipt, ok: false }),
    (receipt) => ({ ...receipt, status: 'failed' }),
    (receipt) => ({ ...receipt, command_id: 'other-command' }),
    (receipt) => ({ ...receipt, result: { ...receipt.result, ok: false } }),
    (receipt) => { receipt.result.computer.owner_user_id = 'foreign'; return receipt; },
    (receipt) => { receipt.result.computer.capability_config = []; return receipt; },
    (receipt) => { receipt.result.computer.agentless = false; return receipt; },
  ];
  for (const transform of transforms) {
    await assert.rejects(capabilityControlFixture(transform).invoke(nasAssignment), /not confirm|did not complete/);
  }
});

test('endpoint enrollment forwards only credential references and exposes a confirmed binding', async () => {
  const { commands, invoke } = capabilityControlFixture();
  assert.deepEqual(await invoke(endpointRequest), {
    action: 'computer.endpoint.upsert', endpointRef: 'endpoint-nas', computerId: 'nas-1', enabled: true,
  });
  assert.equal(commands[0].command_type, 'ctox.workjet.computer.endpoint.upsert');
  assert.deepEqual(commands[0].payload.connection.private_key, { scope: 'computer-access', name: 'nas-key' });
  assert.deepEqual(await invoke({ action: 'computer.endpoint.disable', commandId: 'disable-endpoint',
    endpointRef: 'endpoint-nas' }), {
    action: 'computer.endpoint.disable', endpointRef: 'endpoint-nas', computerId: 'nas-1', enabled: false,
  });
});

test('inline credentials, unknown descriptor fields and ownership injection never reach command dispatch', async () => {
  const { commands, invoke } = capabilityControlFixture();
  const requests = [
    { ...endpointRequest, connection: { ...sshEndpoint, private_key: 'private-key-bytes' } },
    { ...endpointRequest, connection: { ...sshEndpoint, private_key: { ...sshEndpoint.private_key, value: 'secret' } } },
    { ...endpointRequest, connection: { ...sshEndpoint, password: 'secret' } },
    { ...endpointRequest, ownerUserId: 'foreign' },
    { ...nasAssignment, capabilityConfig: [{ ...storageGrant, password: 'secret' }] },
    { ...nasAssignment, capabilityConfig: [storageGrant, storageGrant] },
    { ...nasAssignment, agentless: 'true' },
  ];
  for (const request of requests) await assert.rejects(invoke(request));
  assert.equal(commands.length, 0);
});

test('endpoint confirmation rejects a changed owner, binding, result or command identity', async () => {
  const transforms = [
    (receipt) => ({ ...receipt, command_id: 'different' }),
    (receipt) => { receipt.result.endpoint.owner_user_id = 'foreign'; return receipt; },
    (receipt) => { receipt.result.endpoint.computer_id = 'other-computer'; return receipt; },
    (receipt) => { receipt.result.endpoint.enabled = false; return receipt; },
  ];
  for (const transform of transforms) {
    await assert.rejects(capabilityControlFixture(transform).invoke(endpointRequest), /not confirm|did not complete/);
  }
});

test('build/GPU descriptors and SMB references retain their typed native shape', async () => {
  const { commands, invoke } = capabilityControlFixture();
  await invoke({ ...nasAssignment, hostingMode: 'workstation', agentless: false,
    capabilityConfig: [
      { kind: 'gpu', model: 'Test GPU', vram_gib: 20 },
      { kind: 'build', ssh_endpoint_ref: 'endpoint-build', slots: 3, jobs: 6,
        lane_root: '/srv/build-lane', disk_floor_gib: 60, toolchains: ['rust-stable', 'node-24'] },
    ] });
  assert.deepEqual(commands[0].payload.capability_config.map((entry) => entry.kind), ['build', 'gpu']);
  assert.deepEqual(commands[0].payload.capability_config[0].toolchains, ['node-24', 'rust-stable']);
  await invoke({ ...endpointRequest, connection: { protocol: 'smb', host: 'nas.example.test',
    port: 445, username: 'admin', root: '/artifacts', share: 'build',
    password: { scope: 'computer-access', name: 'nas-password' } } });
  assert.deepEqual(commands[1].payload.connection.password,
    { scope: 'computer-access', name: 'nas-password' });
});
