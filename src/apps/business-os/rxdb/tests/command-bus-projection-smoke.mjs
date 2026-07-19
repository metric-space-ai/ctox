// REGRESSION: command-bus acknowledgement contract for the two native
// command classes (src/core/business_os/store.rs):
//
// 1. Queue-backed commands (chat tasks, tickets, ...) are admitted by the
//    authoritative command receipt. A ctox_queue_tasks projection may enrich
//    the UI later, but admission must not wait for that secondary projection.
// 2. Control commands (ctox.file.materialize, ctox.module.*, ...) are
//    executed directly and acknowledged via write_rxdb_control_command_outcome
//    with a terminal status 'completed' and an INTENTIONALLY EMPTY task_id.
//    The bus must treat that as success. It used to keep waiting for a queue
//    projection that never comes and threw after 45s — every large-file
//    materialize from the file viewer failed that way (rxdb-soak
//    workspace-large-file-viewer-rust-to-browser).
// 3. status 'failed' rejects with the command error.
// 4. ctox.file.materialize uses desktop_files demand-fetch metadata, not
//    browser-origin desktop_file_chunks upload sync.
//
// Runs the REAL createCommandBus with mock collections; no network.

import { createCommandBus } from '../../shared/command-bus.js';

globalThis.CTOX_BUSINESS_OS_SESSION = {
  capability_token: 'projection-smoke-capability-token',
  capability_expires_at_ms: Date.now() + 60 * 60 * 1000,
};

function mockCollection(docsById, events = null, name = '') {
  return {
    async insert(doc) {
      events?.push(`insert:${name}`);
      docsById.set(doc.id, doc);
    },
    findOne(id) {
      return {
        $: { subscribe() { return { unsubscribe() {} }; } },
        async exec() {
          return docsById.get(id) || null;
        },
      };
    },
  };
}

function makeDb({ commandAck, queueTask = null, events = null }) {
  const commands = new Map();
  const queue = new Map();
  const commandCollection = mockCollection(commands, events, 'business_commands');
  // After the bus inserts the pending command doc, simulate the native
  // acknowledgement by overlaying the accepted fields on the stored doc.
  const originalInsert = commandCollection.insert;
  commandCollection.insert = async (doc) => {
    await originalInsert(doc);
    commands.set(doc.id, { ...doc, ...commandAck });
  };
  if (queueTask) queue.set(queueTask.id, queueTask);
  return {
    raw: {
      business_commands: commandCollection,
      ctox_queue_tasks: mockCollection(queue, events, 'ctox_queue_tasks'),
    },
  };
}

function makeSync(events) {
  const bridgeFor = (name) => ({
    collection: name,
    state: {
      async awaitInSync() {
        events.push(`ready:${name}`);
      },
      async pushToRemotePeers() {
        events.push(`flush:${name}`);
      },
    },
  });
  return {
    async startCollection(name) {
      events.push(`start:${name}`);
      return bridgeFor(name);
    },
    async leaseCollection(name, reason) {
      events.push(`lease:${name}:${reason}`);
      return {
        collection: name,
        bridge: bridgeFor(name),
        async release() {
          events.push(`release:${name}`);
        },
      };
    },
  };
}

const assert = (condition, message) => {
  if (!condition) throw new Error(message);
};

// --- 1. queue-backed command resolves with the projected task --------------
{
  const db = makeDb({
    commandAck: { status: 'accepted', task_id: 'queue:system::abc' },
    queueTask: { id: 'queue:system::abc', status: 'queued' },
  });
  const bus = createCommandBus({ db });
  const result = await bus.dispatch({ type: 'business_os.chat.task', module: 'chat' });
  assert(result.ok === true, 'queue command: ok');
  assert(result.task_id === 'queue:system::abc', 'queue command: task id projected');
  assert(result.task_status === 'accepted', 'queue command: admission does not require queue projection detail');
}

// --- 2. control command: terminal completed WITHOUT task_id is success -----
{
  const db = makeDb({
    commandAck: {
      status: 'completed',
      task_id: '',
      task_status: 'completed',
      result: { outcome: { ok: true, data: { grants: ['/tmp/project'] } } },
    },
  });
  const bus = createCommandBus({ db });
  const result = await bus.dispatch({ type: 'ctox.file.materialize', module: 'desktop' });
  assert(result.ok === true, 'control command: completed ack is success');
  assert(result.status === 'completed', 'control command: status completed');
  assert(result.task_id === '', 'control command: no task id');
  assert(result.result.outcome.data.grants[0] === '/tmp/project', 'control command: structured result preserved');
}

// --- 3. direct control command with file_id does not wake chunks -----------
{
  const events = [];
  const db = makeDb({
    events,
    commandAck: {
      status: 'completed',
      task_id: '',
      task_status: 'completed',
      result: { outcome: { ok: true } },
    },
  });
  const bus = createCommandBus({ db, sync: makeSync(events) });
  const result = await bus.dispatch({
    type: 'ctox.file.materialize',
    module: 'desktop',
    payload: {
      file_id: 'desktop_file_existing',
    },
  });
  assert(result.ok === true, 'materialize command: completed ack is success');
  assert(events.some((event) => event.startsWith('lease:desktop_files:command-dependency:')),
    'materialize command: desktop_files leased');
  assert(events.includes('flush:desktop_files'), 'materialize command: desktop_files flushed');
  assert(!events.some((event) => event.startsWith('lease:desktop_file_chunks:')),
    'materialize command: desktop_file_chunks not leased');
  assert(!events.includes('flush:desktop_file_chunks'), 'materialize command: desktop_file_chunks not flushed');
  assert(events.includes('release:desktop_files'), 'materialize command: desktop_files lease released');
}

// --- 4. direct coding-agent status outcome is success ---------------------
{
  const db = makeDb({
    commandAck: {
      status: 'accepted',
      task_id: '',
      task_status: '',
      result: { outcome: { ok: true, data: { provider: 'codex', auth: { ready: true } } } },
    },
  });
  const bus = createCommandBus({ db });
  const result = await bus.dispatch({ type: 'ctox.coding_agent.status', module: 'coding-agents' });
  assert(result.ok === true, 'coding-agent direct outcome: ok');
  assert(result.task_id === '', 'coding-agent direct outcome: no task id');
  assert(result.result.outcome.data.provider === 'codex', 'coding-agent direct outcome: structured data preserved');
}

// --- 5. file-backed commands flush dependencies before command insert ------
{
  const events = [];
  const db = makeDb({
    events,
    commandAck: { status: 'accepted', task_id: 'queue:system::sync-order' },
    queueTask: { id: 'queue:system::sync-order', status: 'queued' },
  });
  const bus = createCommandBus({ db, sync: makeSync(events) });
  const result = await bus.dispatch({
    type: 'business_os.chat.task',
    module: 'cv-print-builder',
    sync_collections: ['documents', 'desktop_file_chunks', 'desktop_files'],
    payload: {
      attachments: [{
        kind: 'desktop_file',
        file_id: 'desktop_file_cv',
        generation_id: 'desktop_file_cv_g1',
      }],
    },
  });
  assert(result.ok === true, 'dependency sync command: ok');
  const commandInsert = events.indexOf('insert:business_commands');
  assert(commandInsert >= 0, 'dependency sync command: command inserted');
  for (const name of ['documents', 'desktop_file_chunks', 'desktop_files']) {
    const flush = events.indexOf(`flush:${name}`);
    assert(flush >= 0, `dependency sync command: ${name} flushed`);
    assert(flush < commandInsert, `dependency sync command: ${name} flushed before command insert`);
  }
  assert(!events.includes('start:desktop_file_chunks'), 'dependency sync command: desktop_file_chunks not directly started');
  assert(
    events.some((event) => event.startsWith('lease:desktop_file_chunks:command-dependency:')),
    'dependency sync command: desktop_file_chunks leased',
  );
  assert(
    events.indexOf('flush:business_commands') > commandInsert,
    'dependency sync command: business_commands flushed after command insert',
  );
  assert(events.includes('release:desktop_file_chunks'), 'dependency sync command: desktop_file_chunks lease released');
}

// --- 6. failed command rejects with the command error ----------------------
{
  const db = makeDb({
    commandAck: {
      status: 'failed',
      result: { outcome: { ok: false, stderr: 'provider rejected workspace' } },
    },
  });
  const bus = createCommandBus({ db });
  let thrown = null;
  try {
    await bus.dispatch({ type: 'ctox.file.materialize', module: 'desktop' });
  } catch (error) {
    thrown = error;
  }
  assert(thrown, 'failed command: dispatch rejects');
  assert(
    String(thrown.message).includes('provider rejected workspace'),
    `failed command: error message propagated (got: ${thrown.message})`,
  );
}

// --- 7. follower bridges own a bounded multi-tab failover deadline ----------
{
  const events = [];
  const db = makeDb({
    events,
    commandAck: {
      status: 'completed',
      task_id: '',
      task_status: 'completed',
      result: { outcome: { ok: true } },
    },
  });
  const sync = {
    async startCollection(name) {
      events.push(`start:${name}`);
      return {
        mode: 'follower',
        collection: name,
        flushTimeoutMs: 40,
        async flush() {
          events.push(`follower-flush:${name}`);
        },
      };
    },
  };
  const bus = createCommandBus({ db, sync });
  const result = await bus.dispatch({
    type: 'ctox.app_store.install',
    module: 'app-store',
    payload: { module_id: 'multi-tab-test' },
  });
  assert(result.ok === true, 'multi-tab follower: bounded bridge flush succeeds');
  assert(events.includes('follower-flush:business_commands'),
    'multi-tab follower: command collection uses follower-owned flush');
}

{
  const db = makeDb({
    commandAck: {
      status: 'completed',
      task_id: '',
      task_status: 'completed',
      result: { outcome: { ok: true } },
    },
  });
  const sync = {
    async startCollection(name) {
      return {
        mode: 'follower',
        collection: name,
        flushTimeoutMs: 40,
        flush() {
          return new Promise(() => {});
        },
      };
    },
  };
  const bus = createCommandBus({ db, sync });
  const startedAt = Date.now();
  let thrown = null;
  try {
    await bus.dispatch({
      type: 'ctox.app_store.install',
      module: 'app-store',
      payload: { module_id: 'multi-tab-timeout' },
    });
  } catch (error) {
    thrown = error;
  }
  assert(thrown?.code === 'sync_unavailable', 'multi-tab follower: timeout remains typed and retryable');
  assert(Date.now() - startedAt < 1_000, 'multi-tab follower: bridge-owned test deadline is honored');
  assert(
    String(thrown.message).includes('multi-tab command failover'),
    'multi-tab follower: failure describes the complete failover rather than only the stale leader',
  );
}

console.log('ctox-rxdb command-bus projection smoke OK');
process.exit(0);
