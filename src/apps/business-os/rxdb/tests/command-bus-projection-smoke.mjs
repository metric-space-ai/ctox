// REGRESSION: command-bus acknowledgement contract for the two native
// command classes (src/core/business_os/store.rs):
//
// 1. Queue-backed commands (chat tasks, tickets, ...) are acknowledged with a
//    task_id + a replicated ctox_queue_tasks doc — the bus must wait for BOTH
//    and report the task.
// 2. Control commands (ctox.file.materialize, ctox.module.*, ...) are
//    executed directly and acknowledged via write_rxdb_control_command_outcome
//    with a terminal status 'completed' and an INTENTIONALLY EMPTY task_id.
//    The bus must treat that as success. It used to keep waiting for a queue
//    projection that never comes and threw after 45s — every large-file
//    materialize from the file viewer failed that way (rxdb-soak
//    workspace-large-file-viewer-rust-to-browser).
// 3. status 'failed' rejects with the command error.
//
// Runs the REAL createCommandBus with mock collections; no network.

import { createCommandBus } from '../../shared/command-bus.js';

function mockCollection(docsById) {
  return {
    async insert(doc) {
      docsById.set(doc.id, doc);
    },
    findOne(id) {
      return {
        async exec() {
          return docsById.get(id) || null;
        },
      };
    },
  };
}

function makeDb({ commandAck, queueTask = null }) {
  const commands = new Map();
  const queue = new Map();
  const commandCollection = mockCollection(commands);
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
      ctox_queue_tasks: mockCollection(queue),
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
  assert(result.task_status === 'queued', 'queue command: task status from queue doc');
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

// --- 3. failed command rejects with the command error ----------------------
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

// --- 4. failed command rejects with the command error ----------------------
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

console.log('ctox-rxdb command-bus projection smoke OK');
process.exit(0);
