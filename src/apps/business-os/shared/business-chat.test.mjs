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
  assert.equal(queue.stats.findCalls, 2);
  assert.equal(commands.stats.findOneCalls, 0);
  assert.equal(queue.stats.findOneCalls, 0);
  assert.deepEqual(commands.stats.requestedIds[0].sort(), Array.from({ length: 40 }, (_, index) => `cmd-${index}`).sort());
  assert.deepEqual(queue.stats.requestedIds[0].sort(), Array.from({ length: 40 }, (_, index) => `task-${index}`).sort());
  assert.equal(state.chats.every((chat, index) => chat.messages[0].taskId === `task-${index}`), true);
  assert.equal(state.chats.every((chat) => chat.messages[0].status === 'completed'), true);
});

test('business chat resolves failed queue task by command id when command has no task id', async () => {
  const createdAt = Date.now();
  const commands = makeBatchCollection([{
    id: 'cmd-usage-limit',
    command_id: 'cmd-usage-limit',
    status: 'accepted',
  }]);
  const queue = makeBatchCollection([{
    id: 'queue:system::usage-limit',
    command_id: 'cmd-usage-limit',
    status: 'failed',
    status_note: 'Usage limit exceeded.',
  }]);
  const state = {
    ownerUserId: 'user-1',
    selectedDate: '2026-06-23',
    dockCollapsed: true,
    activeChatId: 'chat-old',
    chats: [{
      id: 'chat-visible-error',
      createdAt,
      open: true,
      minimized: true,
      messages: [{
        id: 'message-pending',
        commandId: 'cmd-usage-limit',
        status: 'queued',
        createdAt,
      }],
    }],
  };

  const changed = await __businessChatTestInternals.syncTrackedMessages({
    state,
    db: { raw: { business_commands: commands, ctox_queue_tasks: queue } },
  });

  const chat = state.chats[0];
  assert.equal(changed, true);
  assert.equal(chat.messages[0].taskId, 'queue:system::usage-limit');
  assert.equal(chat.messages[0].status, 'failed');
  assert.equal(chat.messages.at(-1).role, 'ctox');
  assert.match(chat.messages.at(-1).text, /Usage limit exceeded/);
  assert.equal(state.activeChatId, 'chat-visible-error');
  assert.equal(state.dockCollapsed, false);
  assert.equal(commands.stats.findCalls, 1);
  assert.equal(queue.stats.findCalls, 1);
  assert.deepEqual(queue.stats.requestedCommandIds[0], ['cmd-usage-limit']);
});

test('business chat focuses the visible chat when CTOX writes a reply', async () => {
  const createdAt = Date.now();
  const state = {
    ownerUserId: 'user-1',
    selectedDate: '2026-06-23',
    dockCollapsed: true,
    activeChatId: 'chat-empty-old',
    chats: [
      {
        id: 'chat-empty-old',
        createdAt: new Date('2026-06-23T08:00:00Z').getTime(),
        messages: [],
        open: true,
      },
      {
        id: 'chat-reply',
        createdAt,
        messages: [{
          id: 'message-pending',
          commandId: 'cmd-visible',
          status: 'queued',
          createdAt,
        }],
        open: true,
        minimized: true,
      },
    ],
  };
  const commands = makeBatchCollection([{
    id: 'cmd-visible',
    task_id: 'task-visible',
    status: 'accepted',
  }]);
  const queue = makeBatchCollection([{
    id: 'task-visible',
    status: 'completed',
    result: { outbound_text: 'CTOX ist verbunden und die Antwort ist sichtbar.' },
  }]);

  const changed = await __businessChatTestInternals.syncTrackedMessages({
    state,
    db: { raw: { business_commands: commands, ctox_queue_tasks: queue } },
  });

  const replyChat = state.chats.find((chat) => chat.id === 'chat-reply');
  assert.equal(changed, true);
  assert.equal(state.activeChatId, 'chat-reply');
  assert.equal(state.dockCollapsed, false);
  assert.equal(state.selectedDate, __businessChatTestInternals.getLocalDateString(createdAt));
  assert.equal(replyChat.minimized, false);
  assert.equal(replyChat.messages.at(-1).role, 'ctox');
  assert.equal(replyChat.messages.at(-1).text, 'CTOX ist verbunden und die Antwort ist sichtbar.');
});

test('business chat does not defer remote hydration while a tracked command is active', () => {
  const previousDocument = globalThis.document;
  globalThis.document = {
    activeElement: {
      tagName: 'TEXTAREA',
      closest(selector) {
        return selector === '[data-chat-id]' ? {} : null;
      },
    },
  };
  try {
    assert.equal(
      __businessChatTestInternals.shouldDeferRemoteChatHydration(null, {
        chats: [{
          id: 'chat-active',
          messages: [{ id: 'status-cmd', commandId: 'cmd-active', status: 'queued' }],
        }],
      }),
      false,
    );
    assert.equal(
      __businessChatTestInternals.shouldDeferRemoteChatHydration(null, {
        chats: [{
          id: 'chat-idle',
          messages: [],
        }],
      }),
      true,
    );
  } finally {
    if (previousDocument === undefined) {
      delete globalThis.document;
    } else {
      globalThis.document = previousDocument;
    }
  }
});

test('business chat hydration focuses a newly replicated CTOX reply', async () => {
  const previousLocalStorage = globalThis.localStorage;
  const store = new Map();
  globalThis.localStorage = {
    getItem(key) {
      return store.has(key) ? store.get(key) : null;
    },
    setItem(key, value) {
      store.set(key, String(value));
    },
    removeItem(key) {
      store.delete(key);
    },
  };
  const createdAt = Date.now();
  const state = {
    ownerUserId: 'user-1',
    selectedDate: '2026-06-23',
    activeChatId: 'chat-empty-old',
    dockCollapsed: true,
    deletedChatIds: {},
    chats: [
      {
        id: 'chat-empty-old',
        owner_user_id: 'user-1',
        createdAt: new Date('2026-06-23T08:00:00Z').getTime(),
        messages: [],
        open: true,
      },
      {
        id: 'chat-replicated',
        owner_user_id: 'user-1',
        createdAt,
        updated_at_ms: createdAt,
        open: true,
        minimized: true,
        messages: [{
          id: 'status-cmd-replicated',
          role: 'ctox',
          text: 'Task angelegt und in der CTOX Queue.',
          commandId: 'cmd-replicated',
          taskId: 'queue-replicated',
          status: 'queued',
          createdAt,
        }],
      },
    ],
  };
  const remoteChat = {
    id: 'chat-replicated',
    owner_user_id: 'user-1',
    title: 'Matching Frage',
    createdAt,
    updated_at_ms: createdAt + 1000,
    open: true,
    minimized: false,
    messages: [
      {
        id: 'chatmsg-user',
        role: 'user',
        text: 'Bitte antworten.',
        createdAt,
      },
      {
        id: 'reply-cmd-replicated',
        role: 'ctox',
        text: 'CTOX ist verbunden und antwortet sichtbar im Chat.',
        replyFor: 'queue-replicated',
        commandId: 'cmd-replicated',
        taskId: 'queue-replicated',
        status: 'completed',
        createdAt: createdAt + 1000,
      },
    ],
  };

  try {
    const changed = await __businessChatTestInternals.hydrateChatsFromRxDb({
      state,
      session: { user: { id: 'user-1' } },
      db: {
        raw: {
          business_chats: makeFindCollection([remoteChat]),
        },
      },
    });

    const chat = state.chats.find((item) => item.id === 'chat-replicated');
    assert.equal(changed, true);
    assert.equal(state.activeChatId, 'chat-replicated');
    assert.equal(state.dockCollapsed, false);
    assert.equal(state.selectedDate, __businessChatTestInternals.getLocalDateString(createdAt));
    assert.equal(chat.minimized, false);
    assert.equal(chat.messages.at(-1).role, 'ctox');
    assert.equal(chat.messages.at(-1).text, 'CTOX ist verbunden und antwortet sichtbar im Chat.');
  } finally {
    if (previousLocalStorage === undefined) {
      delete globalThis.localStorage;
    } else {
      globalThis.localStorage = previousLocalStorage;
    }
  }
});

test('business chat dock opens the latest substantive chat instead of an old empty day', () => {
  const oldEmpty = {
    id: 'chat-empty-old',
    createdAt: new Date('2026-06-23T08:00:00Z').getTime(),
    messages: [],
    open: true,
  };
  const visible = {
    id: 'chat-visible',
    createdAt: Date.now(),
    updated_at_ms: Date.now() + 10,
    messages: [{ id: 'message-1', role: 'ctox', text: 'Antwort vorhanden.' }],
    open: true,
    minimized: true,
  };
  const state = { selectedDate: '2026-06-23', activeChatId: oldEmpty.id, chats: [oldEmpty, visible] };

  assert.equal(__businessChatTestInternals.preferredChatForDockOpen(state), visible);
  __businessChatTestInternals.focusChatForUser(state, visible);

  assert.equal(state.activeChatId, visible.id);
  assert.equal(state.selectedDate, __businessChatTestInternals.getLocalDateString(visible.createdAt));
  assert.equal(visible.minimized, false);
  assert.equal(state.dockCollapsed, false);
});

test('business chat persistence timeout is treated as volatile', async () => {
  const startedAt = Date.now();
  await assert.rejects(
    () => __businessChatTestInternals.withChatPersistenceTimeout(new Promise(() => {}), 5),
    /Business chat persistence timed out locally/,
  );
  assert.ok(Date.now() - startedAt < 1000);
});

test('business chat treats IDB closing during command tracking as transient', () => {
  assert.equal(
    __businessChatTestInternals.isTransientCommandTrackingError(
      new Error("Failed to execute 'transaction' on 'IDBDatabase': The database connection is closing."),
    ),
    true,
  );
});

test('business chat keeps local state when remote chat persistence is volatile', async () => {
  const previousLocalStorage = globalThis.localStorage;
  const store = new Map();
  globalThis.localStorage = {
    getItem(key) {
      return store.has(key) ? store.get(key) : null;
    },
    setItem(key, value) {
      store.set(key, String(value));
    },
    removeItem(key) {
      store.delete(key);
    },
  };
  try {
    const state = {
      ownerUserId: 'user-1',
      selectedDate: '2026-06-29',
      activeChatId: 'chat-stalled',
      chats: [{
        id: 'chat-stalled',
        owner_user_id: 'user-1',
        messages: [],
        createdAt: Date.now(),
      }],
    };
    await __businessChatTestInternals.persistChatState({
      state,
      db: {
        raw: {
          business_chats: {
            findOne() {
              return {
                async exec() {
                  throw new Error('Timed out waiting for WebRTC response');
                },
              };
            },
          },
        },
      },
    });

    assert.match(store.get('ctox.businessOs.chat.v1'), /chat-stalled/);
  } finally {
    if (previousLocalStorage === undefined) {
      delete globalThis.localStorage;
    } else {
      globalThis.localStorage = previousLocalStorage;
    }
  }
});

test('business chat starts live sync for chat and tracking collections only', async () => {
  const calls = [];
  const sync = {
    async startCollection(name) {
      calls.push(name);
      return {
        state: {
          async awaitInSync() {},
          async awaitInitialReplication() {},
        },
      };
    },
  };

  const results = await __businessChatTestInternals.startChatLiveCollections({
    sync,
    db: {
      raw: {
        business_chats: {},
        business_commands: {},
        ctox_queue_tasks: {},
        desktop_file_chunks: {},
      },
    },
  });

  assert.deepEqual(calls.sort(), ['business_chats', 'business_commands', 'ctox_queue_tasks'].sort());
  assert.equal(results.every((result) => result.ok), true);
});

test('business chat remote persistence merges terminal native replies', async () => {
  const createdAt = Date.now();
  const remote = {
    id: 'chat-merge',
    owner_user_id: 'user-1',
    title: 'Matching Frage',
    createdAt,
    updated_at_ms: createdAt + 10,
    open: true,
    minimized: false,
    messages: [
      {
        id: 'status-cmd-merge',
        role: 'ctox',
        text: 'Task angelegt und in der CTOX Queue.',
        commandId: 'cmd-merge',
        taskId: 'queue-merge',
        status: 'queued',
        createdAt,
      },
      {
        id: 'reply-cmd-merge',
        role: 'ctox',
        text: 'Fertige native Antwort.',
        replyFor: 'queue-merge',
        commandId: 'cmd-merge',
        taskId: 'queue-merge',
        status: 'completed',
        createdAt: createdAt + 10,
      },
    ],
  };
  const local = {
    id: 'chat-merge',
    owner_user_id: 'user-1',
    title: 'Matching Frage',
    createdAt,
    updated_at_ms: createdAt + 20,
    open: true,
    minimized: true,
    messages: [
      {
        id: 'status-cmd-merge',
        role: 'ctox',
        text: 'Task angelegt und in der CTOX Queue.',
        commandId: 'cmd-merge',
        taskId: 'queue-merge',
        status: 'queued',
        createdAt,
      },
    ],
  };
  const patches = [];
  const collection = {
    findOne(id) {
      assert.equal(id, 'chat-merge');
      return {
        async exec() {
          return {
            toJSON: () => ({ ...remote, messages: remote.messages.map((message) => ({ ...message })) }),
            async incrementalPatch(patch) {
              patches.push(patch);
            },
          };
        },
      };
    },
    async insert() {
      throw new Error('existing chat should be patched, not inserted');
    },
  };

  await __businessChatTestInternals.persistChatDocsRemote(collection, [local]);

  assert.equal(patches.length, 1);
  assert.equal(patches[0].messages.some((message) => message.id === 'reply-cmd-merge'), true);
  assert.equal(patches[0].messages.find((message) => message.id === 'reply-cmd-merge')?.status, 'completed');
  assert.equal(patches[0].messages.find((message) => message.id === 'status-cmd-merge')?.taskId, 'queue-merge');
});

test('business chat treats only disposable empty chats as deletion-empty', () => {
  const { isChatEmptyForDeletion } = __businessChatTestInternals;

  assert.equal(isChatEmptyForDeletion({ messages: [] }), true);
  assert.equal(isChatEmptyForDeletion({ messages: [{ id: 'msg-1', text: 'hi' }] }), false);
  assert.equal(isChatEmptyForDeletion({ messages: [], draft: ' noch nicht senden ' }), false);
  assert.equal(isChatEmptyForDeletion({ messages: [], lastTrackingId: 'task-1' }), false);
  assert.equal(isChatEmptyForDeletion({ messages: [], attachments: [{ fileId: 'file-1' }] }), false);
  assert.equal(isChatEmptyForDeletion({
    messages: [],
    scheduledAttachmentsByCommand: { 'cmd-1': [{ fileId: 'scheduled-file-1' }] },
  }), false);
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
    requestedCommandIds: [],
  };
  return {
    stats,
    find(query = {}) {
      stats.findCalls += 1;
      const ids = Array.isArray(query?.selector?.id?.$in)
        ? query.selector.id.$in.map(String)
        : [];
      const commandIds = Array.isArray(query?.selector?.command_id?.$in)
        ? query.selector.command_id.$in.map(String)
        : [];
      stats.requestedIds.push(ids);
      if (commandIds.length) stats.requestedCommandIds.push(commandIds);
      return {
        async exec() {
          const docsById = ids
            .map((id) => byId.get(id))
            .filter(Boolean);
          const docsByCommandId = commandIds.length
            ? rows.filter((row) => commandIds.includes(String(row.command_id || row.commandId || '')))
            : [];
          return [...docsById, ...docsByCommandId]
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

function makeFindCollection(rows) {
  return {
    find() {
      return {
        async exec() {
          return rows.map((row) => ({ toJSON: () => ({ ...row }) }));
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
