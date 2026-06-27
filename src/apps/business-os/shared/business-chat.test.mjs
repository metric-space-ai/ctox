import test from 'node:test';
import assert from 'node:assert/strict';

import {
  __businessChatTestInternals,
  chatAgentScopeViewFromMeta,
  renderChatAgentScopeHtml,
} from './business-chat.js';

const visibleScope = {
  rows: [
    { key: 'actor', label: 'Nutzer', value: 'Mira Team · user' },
    { key: 'app', label: 'App', value: 'Inventory · v1.0.0 · Team' },
    { key: 'data', label: 'Daten', value: 'Freigegeben: Inventory Items' },
    { key: 'external', label: 'Externe Aktionen', value: 'In diesem Schritt aus' },
  ],
  app: {
    module_id: 'inventory',
    module_title: 'Inventory',
    version: 'v1.0.0',
    visibility: 'team',
  },
};

test('business chat renders no agent scope panel without visible scope context', () => {
  assert.equal(chatAgentScopeViewFromMeta({}), null);
  assert.equal(renderChatAgentScopeHtml({}), '');
  assert.equal(renderChatAgentScopeHtml({ client_context: { module: 'inventory' } }), '');
});

test('business chat renders business-facing visible scope rows from client context', () => {
  const html = renderChatAgentScopeHtml({
    client_context: {
      visible_scope: {
        ...visibleScope,
        rows: [
          ...visibleScope.rows,
          { key: 'unsafe', label: '<script>x</script>', value: 'A & B' },
        ],
      },
    },
  });

  assert.match(html, /CTOX Zugriff/);
  assert.match(html, /Nutzer/);
  assert.match(html, /App/);
  assert.match(html, /Daten/);
  assert.match(html, /Externe Aktionen/);
  assert.match(html, /Inventory · v1\.0\.0 · Team/);
  assert.match(html, /&lt;script&gt;x&lt;\/script&gt;/);
  assert.match(html, /A &amp; B/);
});

test('business chat accepts normalized command scope visible scope fallback', () => {
  const view = chatAgentScopeViewFromMeta({
    client_context: {
      scope: {
        visible_scope: visibleScope,
      },
    },
  });

  assert.equal(view?.app.module_id, 'inventory');
  assert.deepEqual(view?.rows.map((row) => row.key), ['actor', 'app', 'data', 'external']);
});

test('business chat tracking sync batches command and queue lookups', async () => {
  const commands = makeBatchCollection(Array.from({ length: 40 }, (_, index) => ({
    id: `cmd-${index}`,
    task_id: `task-${index}`,
    status: 'accepted',
  })));
  const queue = makeBatchCollection(Array.from({ length: 40 }, (_, index) => ({
    id: `task-${index}`,
    status: 'completed',
  })));
  const state = {
    chats: Array.from({ length: 40 }, (_, index) => ({
      id: `chat-${index}`,
      messages: [{
        id: `message-${index}`,
        commandId: `cmd-${index}`,
        status: 'queued',
        createdAt: Date.now(),
      }],
    })),
  };

  const changed = await __businessChatTestInternals.syncTrackedMessages({
    state,
    db: { raw: { business_commands: commands, ctox_queue_tasks: queue } },
  });

  assert.equal(changed, true);
  assert.equal(commands.stats.findCalls, 1);
  assert.equal(queue.stats.findCalls, 1);
  assert.equal(commands.stats.findOneCalls, 0);
  assert.equal(queue.stats.findOneCalls, 0);
  assert.deepEqual(commands.stats.requestedIds[0].sort(), Array.from({ length: 40 }, (_, index) => `cmd-${index}`).sort());
  assert.deepEqual(queue.stats.requestedIds[0].sort(), Array.from({ length: 40 }, (_, index) => `task-${index}`).sort());
  assert.equal(state.chats.every((chat, index) => chat.messages[0].taskId === `task-${index}`), true);
  assert.equal(state.chats.every((chat) => chat.messages[0].status === 'completed'), true);
});

test('business chat tracking watch only pins command and queue collections while active tracking exists', () => {
  const timers = [];
  const commands = makeSubscriptionCollection();
  const queue = makeSubscriptionCollection();
  const state = {
    chats: [{
      id: 'chat-tracking',
      messages: [{
        id: 'message-terminal',
        commandId: 'cmd-terminal',
        status: 'completed',
      }],
    }],
  };
  let syncCalls = 0;
  const watch = __businessChatTestInternals.createTrackedMessageWatch({
    state,
    db: { raw: { business_commands: commands, ctox_queue_tasks: queue } },
    scheduleSync: () => { syncCalls += 1; },
    timerWindow: makeTimerWindow(timers),
  });

  assert.equal(watch.refresh({ schedule: true }), false);
  assert.equal(watch.isWatching(), false);
  assert.equal(commands.stats.subscribeCalls, 0);
  assert.equal(queue.stats.subscribeCalls, 0);
  assert.equal(timers.length, 0);
  assert.equal(syncCalls, 0);

  state.chats[0].messages[0].status = 'queued';
  assert.equal(watch.refresh({ schedule: true }), true);
  assert.equal(watch.isWatching(), true);
  assert.equal(commands.stats.subscribeCalls, 1);
  assert.equal(queue.stats.subscribeCalls, 1);
  assert.equal(timers.filter((timer) => timer.kind === 'interval').length, 1);
  assert.equal(syncCalls, 1);

  commands.emit();
  assert.equal(syncCalls, 2);

  state.chats[0].messages[0].status = 'completed';
  assert.equal(watch.refresh(), false);
  assert.equal(watch.isWatching(), false);
  assert.equal(commands.stats.unsubscribeCalls, 1);
  assert.equal(queue.stats.unsubscribeCalls, 1);
  assert.equal(timers.find((timer) => timer.kind === 'interval')?.cleared, true);
});

test('business chat scheduler stays unarmed when no messages are scheduled', () => {
  const timers = [];
  const previousWindow = globalThis.window;
  globalThis.window = makeTimerWindow(timers);
  try {
    const root = makeSchedulerRoot();
    __businessChatTestInternals.initSchedulerLoop({
      root,
      state: { chats: [] },
      commandBus: null,
      db: null,
      sync: null,
      getActiveModule: null,
    });

    assert.equal(timers.length, 0);
    assert.equal(root.__ctoxChatScheduler, undefined);
  } finally {
    if (previousWindow === undefined) {
      delete globalThis.window;
    } else {
      globalThis.window = previousWindow;
    }
  }
});

test('business chat scheduler arms only while scheduled messages exist', () => {
  const timers = [];
  const previousWindow = globalThis.window;
  globalThis.window = makeTimerWindow(timers);
  try {
    const root = makeSchedulerRoot();
    const state = {
      chats: [{
        id: 'chat-scheduled',
        createdAt: Date.now() + 60_000,
        messages: [{
          id: 'status-cmd-scheduled',
          role: 'ctox',
          commandId: 'cmd-scheduled',
          status: 'scheduled',
        }],
      }],
    };

    __businessChatTestInternals.initSchedulerLoop({
      root,
      state,
      commandBus: null,
      db: null,
      sync: null,
      getActiveModule: null,
    });

    assert.equal(timers.length, 1);
    assert.equal(root.__ctoxChatScheduler?.running, false);
    assert.ok(timers[0].delayMs > 0);

    state.chats[0].messages[0].status = 'queued';
    __businessChatTestInternals.initSchedulerLoop({
      root,
      state,
      commandBus: null,
      db: null,
      sync: null,
      getActiveModule: null,
    });

    assert.equal(timers[0].cleared, true);
    assert.equal(root.__ctoxChatScheduler, undefined);
  } finally {
    if (previousWindow === undefined) {
      delete globalThis.window;
    } else {
      globalThis.window = previousWindow;
    }
  }
});

test('business chat attachment staging does not directly start desktop chunks without a lease API', async () => {
  const files = makeUpsertCollection();
  const chunks = makeUpsertCollection();
  const calls = [];
  const sync = {
    async startCollection(name) {
      calls.push(`start:${name}`);
      return {
        state: {
          async awaitInSync() {
            calls.push(`in-sync:${name}`);
          },
        },
      };
    },
    async stopCollection(name) {
      calls.push(`stop:${name}`);
    },
  };

  await assert.rejects(
    () => __businessChatTestInternals.stageChatAttachments({
      db: {
        collection(name) {
          if (name === 'desktop_files') return files;
          if (name === 'desktop_file_chunks') return chunks;
          return null;
        },
      },
      sync,
      chat: { id: 'chat-attachment', owner_user_id: 'user-1' },
      commandId: 'cmd-attachment',
      messageId: 'msg-attachment',
      attachments: [{
        fileId: 'chat-file-1',
        generationId: 'gen-test',
        name: 'hello.txt',
        mimeType: 'text/plain',
        size: 2,
        extension: 'txt',
        contentHash: 'content-hash',
        base64Data: 'data:text/plain;base64,SGk=',
      }],
    }),
    /requires sync\.leaseCollection/,
  );

  assert.equal(calls.includes('start:desktop_files'), true);
  assert.equal(calls.includes('start:desktop_file_chunks'), false);
  assert.equal(calls.includes('stop:desktop_file_chunks'), false);
});

test('business chat attachment staging scopes desktop chunk sync with a lease when available', async () => {
  const files = makeUpsertCollection();
  const chunks = makeUpsertCollection();
  const calls = [];
  const sync = {
    async startCollection(name) {
      calls.push(`start:${name}`);
      return {
        collection: name,
        state: {
          async awaitInSync() {
            calls.push(`in-sync:${name}`);
          },
        },
      };
    },
    async leaseCollection(name, reason) {
      calls.push(`lease:${name}:${reason}`);
      return {
        collection: name,
        bridge: {
          collection: name,
          state: {
            async awaitInSync() {
              calls.push(`in-sync:${name}`);
            },
          },
        },
        async release() {
          calls.push(`release:${name}`);
        },
      };
    },
    async stopCollection(name) {
      calls.push(`stop:${name}`);
    },
  };

  await __businessChatTestInternals.stageChatAttachments({
    db: {
      collection(name) {
        if (name === 'desktop_files') return files;
        if (name === 'desktop_file_chunks') return chunks;
        return null;
      },
    },
    sync,
    chat: { id: 'chat-lease', owner_user_id: 'user-1' },
    commandId: 'cmd-lease',
    messageId: 'msg-lease',
    attachments: [{
      fileId: 'chat-file-lease',
      generationId: 'gen-lease',
      name: 'lease.txt',
      mimeType: 'text/plain',
      size: 2,
      extension: 'txt',
      contentHash: 'content-hash',
      base64Data: 'data:text/plain;base64,SGk=',
    }],
  });

  assert.equal(calls.includes('lease:desktop_file_chunks:business-chat-attachment'), true);
  assert.equal(calls.includes('start:desktop_file_chunks'), false);
  assert.equal(calls.includes('stop:desktop_file_chunks'), false);
  assert.equal(calls.at(-1), 'release:desktop_file_chunks');
});

function makeBatchCollection(rows) {
  const byId = new Map(rows.map((row) => [row.id, row]));
  const stats = {
    findCalls: 0,
    findOneCalls: 0,
    requestedIds: [],
  };
  return {
    stats,
    find(query = {}) {
      stats.findCalls += 1;
      const ids = Array.isArray(query?.selector?.id?.$in)
        ? query.selector.id.$in.map(String)
        : [];
      stats.requestedIds.push(ids);
      return {
        async exec() {
          return ids
            .map((id) => byId.get(id))
            .filter(Boolean)
            .map((doc) => ({ toJSON: () => ({ ...doc }) }));
        },
      };
    },
    findOne(id) {
      stats.findOneCalls += 1;
      return {
        async exec() {
          const doc = byId.get(String(id));
          return doc ? { toJSON: () => ({ ...doc }) } : null;
        },
      };
    },
  };
}

function makeUpsertCollection() {
  const docs = [];
  return {
    docs,
    async upsert(doc) {
      docs.push({ ...doc });
      return doc;
    },
  };
}

function makeSubscriptionCollection() {
  const listeners = new Set();
  const stats = {
    subscribeCalls: 0,
    unsubscribeCalls: 0,
  };
  return {
    stats,
    $: {
      subscribe(listener) {
        stats.subscribeCalls += 1;
        listeners.add(listener);
        return {
          unsubscribe() {
            if (listeners.delete(listener)) stats.unsubscribeCalls += 1;
          },
        };
      },
    },
    emit() {
      for (const listener of listeners) listener({ documents: [] });
    },
  };
}

function makeSchedulerRoot(countdownEls = []) {
  return {
    querySelectorAll(selector) {
      return selector === '[data-countdown-timer]' ? countdownEls : [];
    },
  };
}

function makeTimerWindow(timers) {
  return {
    setTimeout(fn, delayMs) {
      const timer = { kind: 'timeout', fn, delayMs, cleared: false };
      timers.push(timer);
      return timer;
    },
    setInterval(fn, delayMs) {
      const timer = { kind: 'interval', fn, delayMs, cleared: false };
      timers.push(timer);
      return timer;
    },
    clearTimeout(timer) {
      if (timer) timer.cleared = true;
    },
    clearInterval(timer) {
      if (timer) timer.cleared = true;
    },
  };
}
