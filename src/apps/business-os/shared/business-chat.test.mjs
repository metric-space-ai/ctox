import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

import {
  __businessChatTestInternals,
  chatAgentScopeViewFromMeta,
  crewAppPresenceFromTasks,
  renderChatAgentScopeHtml,
} from './business-chat.js';

const businessChatSource = readFileSync(new URL('./business-chat.js', import.meta.url), 'utf8');

test('inspection separates system history from real replies and keeps an input in every task state', () => {
  const previousDocument = globalThis.document;
  globalThis.document = { documentElement: { lang: 'de' } };
  try {
    const hooks = __businessChatTestInternals;
    const user = { role: 'user', text: 'Bitte rechne 2 + 2.' };
    const receipt = { id: 'status_cmd-test', role: 'ctox', text: 'Aufgabe wird an die Crew gesendet.', commandId: 'cmd-test', status: 'pending_sync' };
    const reply = { role: 'ctox', kind: 'reply', text: '2 + 2 = 4.', commandId: 'cmd-test', taskId: 'task-test', status: 'completed' };
    const question = { role: 'ctox', kind: 'question', text: 'Welche Einheit?' };
    const legacyReply = { role: 'ctox', replyFor: 'old-command', text: 'Hier ist die Antwort.' };
    const chat = { id: 'chat-test', createdAt: 1, messages: [user, receipt], lastTrackingId: 'cmd-test' };
    assert.doesNotMatch(hooks.chatMessagesMarkup(chat.messages), /Crew gesendet/);
    assert.doesNotMatch(hooks.chatInspectionMarkup(chat), /data-track-task/);
    chat.messages.push(reply, question, legacyReply);
    const conversation = hooks.chatMessagesMarkup(chat.messages);
    for (const text of ['2 + 2 = 4.', 'Welche Einheit?', 'Hier ist die Antwort.']) assert.ok(conversation.includes(text));
    assert.match(hooks.chatInspectionMarkup(chat), /data-task-id="task-test"/);
    assert.equal(hooks.chatInspectionContent(chat).title, 'Aufgabe abgeschlossen', 'final reply supersedes old receipt in current status');
    assert.doesNotMatch(hooks.chatInspectionMarkup(chat), /2 \+ 2 = 4/);
    for (const status of ['pending', 'running', 'blocked', 'failed', 'completed']) {
      chat.messages = [user, { ...receipt, status, taskId: 'task-test' }];
      const html = hooks.chatWindow(chat, chat.id);
      assert.match(html, /<textarea name="message"/, status);
      assert.equal(hooks.chatComposerSignature(chat), 'conversation', status);
    }
    assert.equal(hooks.isChatInspectionMessage({ ...receipt, kind: 'reply', text: 'Aufgabe wird morgen erledigt.' }), false);
    assert.match(hooks.chatMessagesMarkup([{ role: 'ctox', kind: 'reply', text: 'CTOX konnte die Aufgabe nicht ausführen.' }]), /CTOX konnte die Aufgabe nicht ausführen\./);
  } finally {
    globalThis.document = previousDocument;
  }
});

test('a status-only projection retains this task plan without leaking it into the next task', () => {
  const { executionProgressForChat } = __businessChatTestInternals;
  const plan = { version: 1, revision: 2, phase: 'working', percent: 30,
    current_step: 2, completed_steps: 1, total_steps: 3,
    steps: [
      { position: 1, label: 'Daten laden', status: 'completed' },
      { position: 2, label: 'Daten prüfen', status: 'in_progress' },
      { position: 3, label: 'Ergebnis schreiben', status: 'pending' },
    ] };
  const chat = { messages: [
    { taskId: 'task-one', commandId: 'command-one', executionProgress: plan },
    { taskId: 'task-one', commandId: 'command-one', kind: 'status', text: 'Werkzeug gestartet' },
  ] };
  assert.equal(executionProgressForChat(chat).steps.length, 3);
  assert.equal(executionProgressForChat(chat).current_step, 2);
  chat.messages.push({ commandId: 'command-two', status: 'pending_sync' });
  assert.equal(executionProgressForChat(chat), null, 'unaccepted follow-up cannot inherit the preceding task plan');
  chat.messages.push({ commandId: 'command-two', taskId: 'task-two', status: 'running' });
  assert.equal(executionProgressForChat(chat), null, 'new task starts with no fabricated steps');
});

test('saved crew status messages use plain language without rewriting user instructions', () => {
  const previousDocument = globalThis.document;
  globalThis.document = { documentElement: { lang: 'de' } };
  try {
    for (const text of [
      'Task angelegt und in der CTOX Queue.',
      'Task angelegt und in der CTOX Queue. Antwort erscheint hier, sobald CTOX ihn verarbeitet.',
      'Task angelegt und in der CTOX Queue. Antwort erscheint hier, sobald der CTOX Service ihn verarbeitet.',
    ]) {
      const message = { role: 'ctox', text, status: 'queued', taskId: 'queue:system::real-42' };
      const html = __businessChatTestInternals.messageMarkup(message);
      assert.match(html, /Deine Aufgabe steht auf der Aufgabenliste/);
      assert.doesNotMatch(html, /CTOX Queue|CTOX Service|sobald CTOX/);
      assert.match(html, /data-task-id="queue:system::real-42"/);
      assert.equal(message.text, text, 'presentation must not rewrite stored evidence');
      assert.match(__businessChatTestInternals.messageMarkup({ role: 'user', text }), /CTOX Queue/);
    }
    const failure = __businessChatTestInternals.messageMarkup({
      role: 'ctox', status: 'failed',
      text: 'CTOX konnte die Aufgabe nicht ausführen: technical:worker-runtime-api-failure — 2026-09-08T04:37:18Z',
    });
    assert.match(failure, /Anfrage an den Modelldienst ist fehlgeschlagen/);
    assert.doesNotMatch(failure, /technical:|worker-runtime|CTOX/);
    for (const text of [
      'Aufgabe in der CTOX Queue angelegt. Fortschritt und Antwort erscheinen hier.',
      'CTOX konnte die Aufgabe nicht ausführen: CTOX chat could not continue because the model API is temporarily unavailable. The task must stay open and retry after cooldown.',
      'Der Versuch wird wiederholt: Harness retry feedback injected after runtime failure: direct session error: ErrorEvent { message: unexpected status 502 Bad Gateway }',
    ]) {
      const message = { role: 'ctox', text };
      assert.doesNotMatch(__businessChatTestInternals.messageMarkup(message), /CTOX|Harness|ErrorEvent|Queue|cooldown/);
      assert.equal(message.text, text);
      assert.match(__businessChatTestInternals.messageMarkup({ role: 'user', text }), /CTOX|Harness/);
    }
  } finally {
    globalThis.document = previousDocument;
  }
});

test('external chat submit confirms queue acceptance before remote chat persistence', () => {
  const resolveIndex = businessChatSource.indexOf('detail.resolveSubmission?.(submission)');
  const persistIndex = businessChatSource.indexOf('await persistChatState({ state, db });', resolveIndex);
  assert.ok(resolveIndex >= 0);
  assert.ok(persistIndex > resolveIndex);
});

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

test('tracked crew messages expose a compact task id that deep-links to CTOX', () => {
  const taskId = 'queue:system::task_1234567890abcdef';
  const previousDocument = globalThis.document;
  globalThis.document = { documentElement: { lang: 'de' } };
  const html = __businessChatTestInternals.messageMarkup({
      id: 'status-1',
      role: 'ctox',
      text: 'Recherche gestartet.',
      taskId,
      commandId: 'cmd-42',
      status: 'running',
    });
  globalThis.document = previousDocument;
  assert.match(html, /data-track-task/);
  assert.match(html, new RegExp(`data-task-id="${taskId}"`));
  // The id stays in the tooltip; the bar shows only the link icon (Owner 08.09.).
  assert.match(html, /title="[^"]*567890abcdef/);
  assert.match(html, /<code class="ctox-chat-track-id">…567890abcdef<\/code>/);
  assert.match(html, new RegExp(`aria-label="[^"]+${taskId}`));
});

test('crew identity follows the command id across chat and CTOX task projections', () => {
  const chat = { id: 'chat-random', messages: [{ commandId: 'cmd-shared', taskId: 'task-shared' }] };
  const task = { id: 'task-shared', commandId: 'cmd-shared' };
  assert.deepEqual(
    __businessChatTestInternals.crewIdentity(chat),
    __businessChatTestInternals.crewIdentity(task),
  );
});

test('chat merge deduplicates the same tracked event across optimistic and RxDB ids', () => {
  const shared = {
    role: 'ctox',
    text: 'Recherche wurde gestartet.',
    taskId: 'task-42',
    commandId: 'command-42',
  };
  const merged = __businessChatTestInternals.mergeChatMessages(
    [{ ...shared, id: 'local-status', status: 'queued', createdAt: 10 }],
    [{ ...shared, id: 'remote-projection', status: 'running', createdAt: 20 }],
  );
  assert.equal(merged.length, 1);
  assert.equal(merged[0].id, 'remote-projection');
  assert.equal(merged[0].status, 'running');
});

test('background control commands cannot steal focus when their result arrives', () => {
  assert.equal(__businessChatTestInternals.chatAllowsAutoFocus({
    contextMeta: {
      client_context: { business_chat_auto_focus: false },
    },
  }), false);
  assert.equal(__businessChatTestInternals.chatAllowsAutoFocus({
    contextMeta: {
      client_context: { business_chat_auto_focus: true },
    },
  }), true);
  assert.equal(__businessChatTestInternals.chatAllowsAutoFocus({}), true);
});

test('business crew stage keeps expanded windows together and caps the gallery', () => {
  const expanded = Array.from({ length: 16 }, (_, index) => ({ id: `chat-${index}` }));
  assert.deepEqual(__businessChatTestInternals.stageWindowChats(expanded.slice(0, 3), expanded[1]), expanded.slice(0, 3));
  assert.equal(__businessChatTestInternals.stageWindowChats(expanded, expanded[8]).length, 12);
  assert.ok(__businessChatTestInternals.stageWindowChats(expanded, expanded[8]).includes(expanded[8]));
  assert.deepEqual(__businessChatTestInternals.stageWindowChats([], null), []);
});

test('crew identities and SVG bodies are stable per work stream', () => {
  const chat = { id: 'chat-stable', title: 'Rechnungen prüfen', messages: [] };
  const identity = __businessChatTestInternals.crewIdentity(chat);
  assert.deepEqual(__businessChatTestInternals.crewIdentity(chat), identity);
  assert.ok(['round', 'blob', 'square', 'triangle'].includes(identity.shape));
  assert.match(__businessChatTestInternals.crewCreatureHtml(chat, 'running', 'window'), /ctox-crew-creature is-running/);
  assert.match(__businessChatTestInternals.crewCreatureHtml(chat, 'running', 'window'), /<svg viewBox="0 0 64 64"/);
});

test('crew pool members read and learn from projection stamps, then settle', () => {
  const { crewMemberExpression, crewPoolSlotHtml } = __businessChatTestInternals;
  const now = Date.now();
  const base = { id: 'crew:milo', name: 'Milo', shape: 'round', color: '#7c6df2', state: 'home', domain: [] };
  assert.equal(crewMemberExpression({ ...base, state: 'on_duty', last_memory_read_at_ms: now - 2000 }, now), 'reading');
  assert.equal(crewMemberExpression({ ...base, state: 'on_duty', last_memory_read_at_ms: now - 30000 }, now), 'running');
  assert.equal(crewMemberExpression({ ...base, last_learning_at_ms: now - 2000 }, now), 'learning');
  assert.equal(crewMemberExpression({ ...base, last_learning_at_ms: now - 120000 }, now), 'idle');
  assert.equal(crewMemberExpression({ ...base, state: 'resting_after_failure', last_learning_at_ms: now - 2000 }, now), 'failed');
  const reading = crewPoolSlotHtml({ ...base, state: 'on_duty', last_memory_read_at_ms: now - 2000 });
  assert.match(reading, /liest sein Gedächtnis|reading its memory/);
  assert.match(reading, /is-reading/);
  assert.match(reading, /ctox-crew-eyes-reading/);
  const learning = crewPoolSlotHtml({ ...base, last_learning_at_ms: now - 2000 });
  assert.match(learning, /is-learning/);
  assert.match(learning, /ctox-crew-eyes-learning/);
});

test('crew creatures sleep when not working and use X eyes only for failures', () => {
  const chat = { id: 'chat-resting', title: 'Warten', messages: [] };
  const idle = __businessChatTestInternals.crewCreatureHtml(chat, 'idle');
  const queued = __businessChatTestInternals.crewCreatureHtml(chat, 'queued');
  const scheduled = __businessChatTestInternals.crewCreatureHtml(chat, 'scheduled');
  const completed = __businessChatTestInternals.crewCreatureHtml(chat, 'success');
  for (const markup of [idle, queued, scheduled, completed]) {
    assert.match(markup, /is-sleeping/);
    assert.match(markup, /ctox-crew-eyes-sleeping/);
    assert.doesNotMatch(markup, /ctox-crew-eyes-x/);
  }

  const failed = __businessChatTestInternals.crewCreatureHtml(chat, 'failed');
  assert.match(failed, /is-failed/);
  assert.match(failed, /ctox-crew-eyes-x/);
  assert.doesNotMatch(failed, /ctox-crew-eyes-sleeping/);
});

test('review progress selects a distinct creature mode with durable activity telemetry', () => {
  const reviewChat = {
    id: 'chat-review',
    title: 'Prüfen',
    lastTrackingId: 'task-review',
    messages: [{
      taskId: 'task-review',
      status: 'running',
      executionProgress: {
        phase: 'review',
        percent: 82,
        steps: [{ position: 1, label: 'Prüfen', status: 'completed', activity_turns: 4 }],
        review: { status: 'in_progress' },
        activity_turns: { total: 6, thinking: 4, tools: 2, last_kind: 'thinking' },
        updated_at_ms: 1720000000000,
      },
    }],
  };
  assert.equal(__businessChatTestInternals.crewCreatureMode(reviewChat, 'running'), 'review');
  const review = __businessChatTestInternals.crewCreatureHtml(reviewChat, 'running', 'window');
  assert.match(review, /is-running is-review/);
  assert.match(review, /data-crew-mode="review"/);
  assert.match(review, /data-activity-turns="6"/);
  assert.match(review, /data-activity-kind="thinking"/);
  assert.match(review, /data-activity-updated-at="1720000000000"/);
  assert.doesNotMatch(review, /ctox-crew-eyes-sleeping|ctox-crew-eyes-x/);

  const workChat = { ...reviewChat, id: 'chat-working' };
  workChat.messages = [{
    ...reviewChat.messages[0],
    executionProgress: { ...reviewChat.messages[0].executionProgress, phase: 'work' },
  }];
  const work = __businessChatTestInternals.crewCreatureHtml(workChat, 'running', 'window');
  assert.match(work, /is-running is-working/);
  assert.doesNotMatch(work, /is-review/);
  assert.equal(review, __businessChatTestInternals.crewCreatureHtml(reviewChat, 'running', 'window'));
});

test('CTOX normalized camel-case telemetry drives map creature turns and progress', () => {
  const mapTask = {
    id: 'queue-active',
    status: 'running',
    executionProgress: {
      phase: 'working',
      percent: 60,
      currentStep: 2,
      completedSteps: 2,
      totalSteps: 3,
      steps: [
        { position: 1, label: 'Lesen', status: 'completed', activityTurns: 2 },
        { position: 2, label: 'Prüfen', status: 'in_progress', activityTurns: 5 },
        { position: 3, label: 'Schreiben', status: 'pending', activityTurns: 0 },
      ],
      activityTurns: { total: 7, thinking: 4, tools: 3, lastKind: 'tool' },
      lastActivityKind: 'tool',
      updatedAtMs: 1720000000123,
    },
  };
  const creature = __businessChatTestInternals.crewCreatureHtml(mapTask, 'running', 'map');
  assert.match(creature, /is-running is-working/);
  assert.match(creature, /data-activity-turns="7"/);
  assert.match(creature, /data-activity-kind="tool"/);
  assert.match(creature, /data-activity-updated-at="1720000000123"/);
  assert.match(creature, /--ctox-progress-angle:216deg/);
});

test('crew motion is triggered only by durable turns, finite, and reduced-motion safe', () => {
  assert.match(businessChatSource, /function syncCrewProceduralMotion/);
  assert.match(businessChatSource, /now - state\.lastFrameAt < 33/);
  assert.match(businessChatSource, /\.slice\(0, 36\)/);
  assert.match(businessChatSource, /document\.visibilityState === 'hidden'/);
  assert.match(businessChatSource, /total > \(previousTotal \?\? total\)/);
  assert.match(businessChatSource, /!freshInitialEvent && !modeChanged/);
  assert.match(businessChatSource, /nowMs - updatedAt <= 8000/);
  assert.match(businessChatSource, /duration = mode === 'review' \? 2200 : kind === 'thinking' \? 1800 : 1400/);
  assert.doesNotMatch(businessChatSource, /frequencyB: .*Math\.SQRT2/);
  assert.match(businessChatSource, /\.ctox-crew-creature\.is-working[\s\S]*?animation: none/);
  assert.match(businessChatSource, /\.ctox-crew-creature\.is-review[\s\S]*?animation: none/);
  assert.match(businessChatSource, /\.ctox-crew-creature\.is-failed[\s\S]*?animation: ctoxCrewOops 860ms[^;]* 1 both/);
  assert.match(businessChatSource, /\.ctox-crew-creature,\n\s+\.ctox-crew-creature \*/);
  assert.doesNotMatch(businessChatSource, /\.ctox-crew-creature\.is-(idle|queued|scheduled|success|blocked)[^}]*animation:/);
});

test('routine status updates cannot restart dock or window entry animations', () => {
  assert.doesNotMatch(
    businessChatSource,
    /\.ctox-chat-chip\s*\{[^}]*animation:/,
    'dock chips must not replay an entry animation after a reactive render',
  );
  assert.doesNotMatch(
    businessChatSource,
    /\.ctox-chat-window\s*\{[^}]*animation:/,
    'chat windows must not replay an entry animation after a reactive render',
  );
  assert.match(businessChatSource, /forceDock:\s*false,\s*forceMessages:\s*true/);
  assert.match(businessChatSource, /previousStripScrollLeft/);
});

test('progress instruments render only durable evidence and keep task links delegated', () => {
  assert.match(businessChatSource, /\.ctox-progress-track::before \{[\s\S]*?var\(--crew-color/);
  assert.match(businessChatSource, /\.ctox-progress-planning-line \{\s*width: 0;[\s\S]*?animation: none;/);
  assert.match(businessChatSource, /\.ctox-progress-segment\.is-in_progress,[\s\S]*?animation: none;/);
  assert.match(businessChatSource, /function ensureTaskTrackingDelegation\(root\)/);
  assert.match(businessChatSource, /event\.target\?\.closest\?\.\('\[data-track-task\]'\)/);
  assert.doesNotMatch(businessChatSource, /node\.querySelectorAll\('\[data-track-task\]'\)/);
});

test('business chat does not restore terminal task windows over app content', () => {
  const state = {
    activeChatId: 'chat-terminal',
    dockCollapsed: false,
    preCollapseExpandedChatIds: ['chat-terminal'],
    chats: [{
      id: 'chat-terminal',
      open: true,
      minimized: false,
      lastTrackingId: 'task-failed',
      messages: [{
        id: 'status-task-failed',
        taskId: 'task-failed',
        status: 'failed',
      }],
    }],
  };

  __businessChatTestInternals.collapseRestoredTerminalChat(state);

  assert.equal(state.activeChatId, '');
  assert.equal(state.dockCollapsed, true);
  assert.equal(state.preCollapseExpandedChatIds.length, 0);
  assert.equal(state.chats[0].minimized, true);
});

test('business chat keeps running restored task windows visible', () => {
  const state = {
    activeChatId: 'chat-running',
    dockCollapsed: false,
    preCollapseExpandedChatIds: ['chat-running'],
    chats: [{
      id: 'chat-running',
      open: true,
      minimized: false,
      lastTrackingId: 'task-running',
      messages: [{
        id: 'status-task-running',
        taskId: 'task-running',
        status: 'running',
      }],
    }],
  };

  __businessChatTestInternals.collapseRestoredTerminalChat(state);

  assert.equal(state.activeChatId, 'chat-running');
  assert.equal(state.dockCollapsed, false);
  assert.equal(state.chats[0].minimized, false);
});

test('business chat forces a full window render when terminal tracking changes the task state', () => {
  const chat = {
    lastTrackingId: 'task-1',
    messages: [{ taskId: 'task-1', status: 'completed' }],
  };
  const staleWindow = { classList: { contains: (name) => name === 'is-task-running' } };
  const currentWindow = { classList: { contains: (name) => name === 'is-task-success' } };

  assert.equal(__businessChatTestInternals.windowTaskStateMatches(staleWindow, chat), false);
  assert.equal(__businessChatTestInternals.windowTaskStateMatches(currentWindow, chat), true);
});

test('business chat task submission returns the real queue id after rendering pending feedback', async () => {
  const previousDocument = globalThis.document;
  const previousLocation = globalThis.location;
  globalThis.document = { documentElement: { lang: 'de' } };
  globalThis.location = { href: 'https://customer.example.test/#desktop' };
  const state = { ownerUserId: 'user-1', chats: [] };
  const chat = { id: 'chat-1', title: 'Recherche', messages: [], contextMeta: {} };
  let pendingRendered = false;
  try {
    const submission = await __businessChatTestInternals.submitChatMessage({
      state,
      chat,
      text: 'Recherchiere Example Industries GmbH.',
      commandBus: {
        async dispatch(command) {
          assert.equal(pendingRendered, true);
          assert.equal(command.payload.prompt, 'Recherchiere Example Industries GmbH.');
          return { status: 'queued', command_id: command.id, task_id: 'queue-real-42' };
        },
      },
      db: null,
      sync: null,
      getActiveModule: () => ({ id: 'private-outbound', title: 'Private Outbound' }),
      meta: { command_id: 'cmd-research-42' },
      onPending: () => { pendingRendered = true; },
    });
    assert.deepEqual(submission, {
      status: 'queued',
      command_id: 'cmd-research-42',
      task_id: 'queue-real-42',
      queue_id: 'queue-real-42',
    });
    assert.equal(chat.messages[0].role, 'user');
    assert.equal(chat.messages[0].text, 'Recherchiere Example Industries GmbH.');
    assert.equal(chat.messages[1].taskId, 'queue-real-42');
  } finally {
    globalThis.document = previousDocument;
    globalThis.location = previousLocation;
  }
});

test('business chat reports an unconfirmed connection timeout without claiming rejection or acceptance', async () => {
  const previousDocument = globalThis.document;
  const previousLocation = globalThis.location;
  globalThis.document = { documentElement: { lang: 'de' } };
  globalThis.location = { href: 'https://customer.example.test/#desktop' };
  try {
    for (const code of ['peer_connect_timeout', undefined]) {
      const chat = { id: 'chat-timeout', title: 'CTOX', messages: [], contextMeta: {} };
      const submission = await __businessChatTestInternals.submitChatMessage({
        state: { ownerUserId: 'user-1', chats: [] }, chat,
        text: 'Kontrolltest', db: null, sync: null,
        meta: { command_id: 'cmd-timeout' },
        commandBus: { async dispatch() {
          throw Object.assign(new Error('WebRTC native peer did not open for business_commands within 25000ms; reconnect repair is scheduled.'), { code });
        } },
      });
      assert.equal(submission.status, 'pending_sync');
      assert.equal(submission.command_id, 'cmd-timeout');
      assert.equal(submission.task_id, '');
      assert.equal(chat.messages[1].trackable, true);
      assert.match(chat.messages[1].text, /noch nicht bestätigt/);
      assert.doesNotMatch(chat.messages[1].text, /Task an CTOX übergeben|konnte nicht an CTOX übergeben/);
      assert.equal(chat.lastTrackingId, 'cmd-timeout');
    }
  } finally {
    globalThis.document = previousDocument;
    globalThis.location = previousLocation;
  }
});

test('business chat keeps the execution prompt intact while showing compact app copy', async () => {
  const previousDocument = globalThis.document;
  const previousLocation = globalThis.location;
  globalThis.document = { documentElement: { lang: 'de' } };
  globalThis.location = { href: 'https://customer.example.test/#desktop' };
  const state = { ownerUserId: 'user-1', chats: [] };
  const chat = { id: 'chat-display-copy', title: 'CTOX', messages: [], contextMeta: {} };
  const fullPrompt = 'SYSTEM_PAYLOAD::{"record_ids":["123"],"operation":"reconcile"}';
  try {
    await __businessChatTestInternals.submitChatMessage({
      state,
      chat,
      text: fullPrompt,
      commandBus: {
        async dispatch(command) {
          assert.equal(command.payload.prompt, fullPrompt);
          assert.equal(command.payload.instruction, fullPrompt);
          assert.equal(command.payload.display_title, 'Rechnungen abgleichen');
          assert.equal(command.payload.display_prompt, 'Drei offene Rechnungen mit dem Bankkonto abgleichen.');
          return { status: 'queued', command_id: command.id, task_id: 'queue-display-copy' };
        },
      },
      db: null,
      sync: null,
      getActiveModule: () => ({ id: 'finance', title: 'Finanzen' }),
      meta: {
        command_id: 'cmd-display-copy',
        display_title: 'Rechnungen abgleichen',
        display_prompt: 'Drei offene Rechnungen mit dem Bankkonto abgleichen.',
      },
    });
    assert.equal(chat.title, 'Rechnungen abgleichen');
    assert.equal(chat.messages[0].text, 'Drei offene Rechnungen mit dem Bankkonto abgleichen.');
  } finally {
    globalThis.document = previousDocument;
    globalThis.location = previousLocation;
  }
});

test('business chat collapses long visible prompts without losing the full text', () => {
  const longPrompt = `Erstelle einen vollständigen Bericht. ${'Viele wichtige Details. '.repeat(16)}`;
  const html = __businessChatTestInternals.messageMarkup({ role: 'user', text: longPrompt });
  assert.match(html, /ctox-chat-prompt/);
  assert.match(html, />Mehr</);
  assert.match(html, />Weniger</);
  assert.match(html, /Viele wichtige Details/);
});

test('business chat tracks a terminal native control command without inventing a queue task', async () => {
  const previousDocument = globalThis.document;
  const previousLocation = globalThis.location;
  globalThis.document = { documentElement: { lang: 'de' } };
  globalThis.location = { href: 'https://customer.example.test/#desktop' };
  const state = { ownerUserId: 'user-1', chats: [] };
  const chat = { id: 'chat-control', title: 'Nachrecherche', messages: [], contextMeta: {} };
  try {
    const submission = await __businessChatTestInternals.submitChatMessage({
      state,
      chat,
      text: 'Recherchiere Example Industries GmbH.',
      commandBus: {
        async dispatch(command) {
          assert.equal(command.type, 'web_stack.person_research');
          return {
            status: 'completed',
            terminal_status: 'completed',
            execution_mode: 'control',
            command_id: command.id,
            task_id: '',
          };
        },
      },
      db: null,
      sync: null,
      getActiveModule: () => ({ id: 'private-outbound', title: 'Private Outbound' }),
      meta: {
        command_id: 'cmd-control-research',
        command_type: 'web_stack.person_research',
      },
    });
    assert.deepEqual(submission, {
      status: 'completed',
      command_id: 'cmd-control-research',
      task_id: '',
      queue_id: '',
    });
    assert.equal(chat.messages[1].text, 'Die Crew hat den Auftrag ausgeführt.');
    assert.equal(chat.messages[1].commandId, 'cmd-control-research');
    assert.equal(chat.messages[1].taskId, '');
    assert.equal(chat.messages[1].status, 'completed');
  } finally {
    globalThis.document = previousDocument;
    globalThis.location = previousLocation;
  }
});

test('business chat acknowledges a declared long-running control command locally', async () => {
  const previousDocument = globalThis.document;
  const previousLocation = globalThis.location;
  globalThis.document = { documentElement: { lang: 'de' } };
  globalThis.location = { href: 'https://customer.example.test/#desktop' };
  const state = { ownerUserId: 'user-1', chats: [] };
  const chat = { id: 'chat-control-local', title: 'Nachrecherche', messages: [], contextMeta: {} };
  try {
    const submission = await __businessChatTestInternals.submitChatMessage({
      state,
      chat,
      text: 'Recherchiere Example Industries GmbH.',
      commandBus: {
        async dispatch(command, options) {
          assert.equal(command.type, 'web_stack.person_research');
          assert.deepEqual(options, { until: 'local' });
          return { status: 'pending_sync', command_id: command.id };
        },
      },
      db: null,
      sync: null,
      getActiveModule: () => ({ id: 'private-outbound', title: 'Private Outbound' }),
      meta: {
        command_id: 'cmd-control-local',
        command_type: 'web_stack.person_research',
        control_command: true,
      },
    });
    assert.deepEqual(submission, {
      status: 'pending_sync',
      command_id: 'cmd-control-local',
      task_id: '',
      queue_id: '',
    });
    assert.equal(chat.messages[1].text, 'Die Crew führt den Auftrag aus.');
    assert.equal(chat.messages[1].commandId, 'cmd-control-local');
  } finally {
    globalThis.document = previousDocument;
    globalThis.location = previousLocation;
  }
});

test('business chat resolves a queue id that is projected after command dispatch', async () => {
  const commands = makeBatchCollection([{ id: 'cmd-delayed', task_id: '' }]);
  const queue = makeBatchCollection([{ id: 'queue-delayed', command_id: 'cmd-delayed', status: 'queued' }]);
  const taskId = await __businessChatTestInternals.waitForSubmittedTaskId({
    raw: { business_commands: commands, ctox_queue_tasks: queue },
  }, 'cmd-delayed', { timeoutMs: 0 });
  assert.equal(taskId, 'queue-delayed');
});

test('business chat starts command and queue replication before dispatch', async () => {
  const previousDocument = globalThis.document;
  const previousLocation = globalThis.location;
  globalThis.document = { documentElement: { lang: 'de' } };
  globalThis.location = { href: 'https://customer.example.test/#desktop' };
  const events = [];
  const state = { ownerUserId: 'user-1', chats: [] };
  const chat = { id: 'chat-tracked', title: 'Recherche', messages: [], contextMeta: {} };
  const commands = makeBatchCollection([{ id: 'cmd-tracked', task_id: 'queue-tracked' }]);
  const queue = makeBatchCollection([
    { id: 'queue-tracked', command_id: 'cmd-tracked', status: 'queued' },
  ]);
  try {
    const submission = await __businessChatTestInternals.submitChatMessage({
      state,
      chat,
      text: 'Recherchiere Example Industries GmbH.',
      commandBus: {
        async dispatch(command) {
          events.push(`dispatch:${command.id}`);
          return { status: 'queued', command_id: command.id };
        },
      },
      db: { raw: { business_commands: commands, ctox_queue_tasks: queue } },
      sync: {
        async startCollection(name) {
          events.push(`sync:${name}`);
          return { ready: Promise.resolve() };
        },
      },
      getActiveModule: () => ({ id: 'outbound', title: 'Outbound' }),
      meta: {
        command_id: 'cmd-tracked',
        command_type: 'web_stack.person_research',
      },
    });

    assert.deepEqual(events, [
      'sync:business_commands',
      'sync:ctox_queue_tasks',
      'dispatch:cmd-tracked',
    ]);
    assert.equal(submission.task_id, 'queue-tracked');
    assert.equal(chat.messages[1].trackable, undefined);
  } finally {
    globalThis.document = previousDocument;
    globalThis.location = previousLocation;
  }
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

  assert.match(html, /Crew-Zugriff/);
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

test('business chat tracking sync follows business command execution phase', async () => {
  const commands = makeBatchCollection([{
    id: 'cmd-running',
    task_id: 'queue:system::running',
    execution_phase: 'running',
    terminal_status: 'none',
  }]);
  const queue = makeBatchCollection([{
    id: 'queue:system::running',
    command_id: 'cmd-running',
    status: 'queued',
  }]);
  const state = {
    chats: [{
      id: 'chat-running',
      lastTrackingId: 'cmd-running',
      messages: [{
        id: 'message-running',
        commandId: 'cmd-running',
        status: 'queued',
        createdAt: Date.now(),
      }],
    }],
  };

  const changed = await __businessChatTestInternals.syncTrackedMessages({
    state,
    db: { raw: { business_commands: commands, ctox_queue_tasks: queue } },
  });

  assert.equal(changed, true);
  assert.equal(state.chats[0].messages[0].taskId, 'queue:system::running');
  assert.equal(state.chats[0].messages[0].status, 'running');
});

test('business chat projects durable execution progress into the tracked crew message', async () => {
  const progress = {
    version: 1,
    revision: 2,
    phase: 'work',
    percent: 30,
    current_step: 2,
    completed_steps: 1,
    total_steps: 3,
    steps: [
      { position: 1, label: 'Daten laden', status: 'completed', activity_turns: 2 },
      { position: 2, label: 'Daten prüfen', status: 'in_progress', activity_turns: 4 },
      { position: 3, label: 'Ergebnis schreiben', status: 'pending', activity_turns: 0 },
    ],
    review: { status: 'pending' },
    activity_turns: { total: 7, thinking: 3, tools: 4, last_kind: 'tool' },
    updated_at_ms: 1234,
  };
  const commands = makeBatchCollection([{
    id: 'cmd-progress',
    task_id: 'task-progress',
    execution_phase: 'running',
    execution_progress: progress,
  }]);
  const queue = makeBatchCollection([{
    id: 'task-progress',
    command_id: 'cmd-progress',
    status: 'running',
    execution_progress: progress,
  }]);
  const state = {
    chats: [{
      id: 'chat-progress',
      messages: [{ id: 'message-progress', commandId: 'cmd-progress', status: 'queued', createdAt: Date.now() }],
    }],
  };

  assert.equal(await __businessChatTestInternals.syncTrackedMessages({
    state,
    db: { raw: { business_commands: commands, ctox_queue_tasks: queue } },
  }), true);
  const message = state.chats[0].messages[0];
  assert.equal(message.executionProgress.percent, 30);
  assert.equal(message.executionProgress.activity_turns.total, 7);
  const card = __businessChatTestInternals.delegationProgressCardHtml(state.chats[0], {
    taskId: 'task-progress',
    commandId: 'cmd-progress',
    taskStatus: 'running',
  });
  assert.match(card, /30%/);
  assert.match(card, /4\/7 Aktivitäten/);
  assert.match(card, /Daten prüfen/);
  assert.match(card, /→ Ergebnis schreiben/);
  assert.match(card, /Planstand 2/);
  assert.match(card, /Denkschritte 3 · Werkzeugeinsätze 4/);
  assert.match(card, /ctox-progress-activity/);
  assert.match(card, /--ctox-turn-angle:24deg/);
  assert.doesNotMatch(card, /ctox-progress-summary/);
  assert.doesNotMatch(card, /ctox-progress-current-copy/);
  assert.doesNotMatch(card, /Live-Harness/);
});

test('business chat tracking sync flushes command collections before reading status', async () => {
  const calls = [];
  const commands = makeBatchCollection([{
    id: 'cmd-flushed',
    task_id: 'queue:system::flushed',
    execution_phase: 'running',
    terminal_status: 'none',
  }]);
  const queue = makeBatchCollection([{
    id: 'queue:system::flushed',
    command_id: 'cmd-flushed',
    status: 'queued',
  }]);
  const sync = {
    async startCollection(collection) {
      calls.push(collection);
      return {
        collection,
        state: {
          async awaitInSync() {
            calls.push(`${collection}:awaitInSync`);
          },
        },
      };
    },
  };
  const state = {
    chats: [{
      id: 'chat-flushed',
      messages: [{
        id: 'message-flushed',
        commandId: 'cmd-flushed',
        status: 'queued',
        createdAt: Date.now(),
      }],
    }],
  };

  const changed = await __businessChatTestInternals.syncTrackedMessages({
    state,
    sync,
    db: { raw: { business_commands: commands, ctox_queue_tasks: queue } },
  });

  assert.equal(changed, true);
  assert.deepEqual(calls, [
    'business_commands',
    'ctox_queue_tasks',
    'business_commands:awaitInSync',
    'ctox_queue_tasks:awaitInSync',
  ]);
  assert.equal(state.chats[0].messages[0].status, 'running');
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
    assert.equal(
      __businessChatTestInternals.shouldDeferRemoteChatHydration(null, {
        chats: [{
          id: 'chat-terminal-awaiting-reply',
          messages: [{
            id: 'status-cmd-terminal',
            commandId: 'cmd-terminal',
            taskId: 'task-terminal',
            status: 'completed',
          }],
        }],
      }),
      false,
    );
    assert.equal(
      __businessChatTestInternals.shouldDeferRemoteChatHydration(null, {
        chats: [{
          id: 'chat-terminal-with-reply',
          messages: [
            {
              id: 'status-cmd-terminal',
              commandId: 'cmd-terminal',
              taskId: 'task-terminal',
              status: 'completed',
            },
            {
              id: 'reply-cmd-terminal',
              role: 'ctox',
              text: 'Fertige Antwort.',
              replyFor: 'task-terminal',
              commandId: 'cmd-terminal',
              taskId: 'task-terminal',
              status: 'completed',
            },
          ],
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

test('business chat hydration focuses a newly replicated CTOX reply inside an open dock', async () => {
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
    dockCollapsed: false,
    remoteHydrationComplete: true,
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

test('business chat hydration never reopens a dock the user collapsed', async () => {
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
    selectedDate: __businessChatTestInternals.getLocalDateString(createdAt),
    activeChatId: 'chat-collapsed',
    dockCollapsed: true,
    remoteHydrationComplete: true,
    deletedChatIds: {},
    chats: [{
      id: 'chat-collapsed',
      owner_user_id: 'user-1',
      createdAt,
      open: true,
      minimized: true,
      messages: [{
        id: 'status-collapsed',
        role: 'ctox',
        commandId: 'cmd-collapsed',
        taskId: 'queue-collapsed',
        status: 'queued',
        createdAt,
      }],
    }],
  };
  const remoteChat = {
    ...state.chats[0],
    updated_at_ms: createdAt + 1000,
    minimized: false,
    messages: [{
      id: 'reply-collapsed',
      role: 'ctox',
      text: 'Fertig, ohne den Arbeitsbereich zu stehlen.',
      replyFor: 'queue-collapsed',
      commandId: 'cmd-collapsed',
      taskId: 'queue-collapsed',
      status: 'completed',
      createdAt: createdAt + 1000,
    }],
  };

  try {
    const changed = await __businessChatTestInternals.hydrateChatsFromRxDb({
      state,
      session: { user: { id: 'user-1' } },
      db: { raw: { business_chats: makeFindCollection([remoteChat]) } },
    });

    assert.equal(changed, true);
    assert.equal(state.dockCollapsed, true);
    assert.equal(state.activeChatId, 'chat-collapsed');
  } finally {
    if (previousLocalStorage === undefined) {
      delete globalThis.localStorage;
    } else {
      globalThis.localStorage = previousLocalStorage;
    }
  }
});

test('business chat first remote hydration keeps historical replies collapsed', async () => {
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
  const createdAt = Date.now() - 60_000;
  const state = {
    ownerUserId: 'user-1',
    selectedDate: __businessChatTestInternals.getLocalDateString(createdAt),
    activeChatId: '',
    dockCollapsed: true,
    remoteHydrationComplete: false,
    deletedChatIds: {},
    chats: [],
  };
  const historical = {
    id: 'chat-historical-reply',
    owner_user_id: 'user-1',
    title: 'Historische QA Antwort',
    createdAt,
    updated_at_ms: createdAt + 1000,
    open: true,
    minimized: false,
    maximized: true,
    messages: [{
      id: 'reply-historical',
      role: 'ctox',
      text: 'Dieser bereits vorhandene Abschluss darf kein frisches Profil aufklappen.',
      replyFor: 'queue-historical',
      commandId: 'cmd-historical',
      taskId: 'queue-historical',
      status: 'completed',
      createdAt: createdAt + 1000,
    }],
  };

  try {
    const changed = await __businessChatTestInternals.hydrateChatsFromRxDb({
      state,
      session: { user: { id: 'user-1' } },
      db: { raw: { business_chats: makeFindCollection([historical]) } },
    });

    assert.equal(changed, true);
    assert.equal(state.remoteHydrationComplete, true);
    assert.equal(state.dockCollapsed, true);
    assert.equal(state.activeChatId, '');
    assert.equal(state.chats.length, 1);
    assert.equal(state.chats[0].minimized, true);
    assert.equal(state.chats[0].maximized, false);
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

test('business chat open resolves the already submitted task instead of creating a duplicate chat', () => {
  const submitted = {
    id: 'chat-current-task',
    lastTrackingId: 'queue-current-task',
    messages: [{
      id: 'status-current-task',
      commandId: 'cmd-current-task',
      taskId: 'queue-current-task',
      status: 'queued',
    }],
  };
  const historical = {
    id: 'chat-historical',
    lastTrackingId: 'queue-historical',
    messages: [{ commandId: 'cmd-historical', taskId: 'queue-historical' }],
  };
  const state = {
    ownerUserId: 'user-1',
    selectedDate: __businessChatTestInternals.getLocalDateString(Date.now()),
    chats: [historical, submitted],
  };

  const byTask = __businessChatTestInternals.resolveChatForOpenDetail(
    state,
    { user: { id: 'user-1' } },
    { focus: { task_id: 'queue-current-task' } },
  );
  assert.equal(byTask, submitted);
  assert.equal(state.chats.length, 2);

  const byCommand = __businessChatTestInternals.resolveChatForOpenDetail(
    state,
    { user: { id: 'user-1' } },
    { commandId: 'cmd-current-task' },
  );
  assert.equal(byCommand, submitted);
  assert.equal(state.chats.length, 2);

  const created = __businessChatTestInternals.resolveChatForOpenDetail(
    state,
    { user: { id: 'user-1' } },
    { command_id: 'cmd-missing' },
  );
  assert.notEqual(created, submitted);
  assert.equal(state.chats.length, 3);
});

test('first remote hydration keeps a newly submitted chat focused', async () => {
  const previousLocalStorage = globalThis.localStorage;
  globalThis.localStorage = {
    getItem: () => null,
    setItem: () => {},
    removeItem: () => {},
  };
  const now = Date.now();
  const active = {
    id: 'chat-new-task',
    owner_user_id: 'user-1',
    createdAt: now,
    updated_at_ms: now,
    open: true,
    minimized: false,
    messages: [{
      id: 'status-new-task',
      commandId: 'cmd-new-task',
      status: 'pending_sync',
      createdAt: now,
    }],
  };
  const historical = {
    id: 'chat-historical-task',
    owner_user_id: 'user-1',
    createdAt: now - 60_000,
    updated_at_ms: now - 60_000,
    open: true,
    minimized: false,
    messages: [{ id: 'old-message', role: 'ctox', text: 'Alt', createdAt: now - 60_000 }],
  };
  const state = {
    ownerUserId: 'user-1',
    activeChatId: active.id,
    selectedDate: __businessChatTestInternals.getLocalDateString(now),
    lastUiMutationMs: now,
    remoteHydrationComplete: false,
    chats: [historical, active],
  };
  const docs = [historical].map((row) => ({ toJSON: () => ({ ...row }) }));

  try {
    await __businessChatTestInternals.hydrateChatsFromRxDb({
      state,
      db: { raw: { business_chats: { find: () => ({ exec: async () => docs }) } } },
      session: { user: { id: 'user-1' } },
    });

    assert.equal(state.activeChatId, active.id);
    assert.equal(state.chats.find((chat) => chat.id === active.id)?.minimized, false);
    assert.equal(state.chats.find((chat) => chat.id === historical.id)?.minimized, true);
    assert.equal(state.dockCollapsed, false);
  } finally {
    globalThis.localStorage = previousLocalStorage;
  }
});

test('remote reply from an old task cannot steal focus from a newly submitted chat', async () => {
  const previousLocalStorage = globalThis.localStorage;
  globalThis.localStorage = {
    getItem: () => null,
    setItem: () => {},
    removeItem: () => {},
  };
  const now = Date.now();
  const active = {
    id: 'chat-avt-new-task',
    owner_user_id: 'user-1',
    createdAt: now,
    updated_at_ms: now,
    open: true,
    minimized: false,
    messages: [{
      id: 'status-avt-new-task',
      role: 'ctox',
      commandId: 'cmd-avt-new-task',
      status: 'queued',
      createdAt: now,
    }],
  };
  const historicalLocal = {
    id: 'chat-example-old-task',
    owner_user_id: 'user-1',
    createdAt: now - 60_000,
    updated_at_ms: now - 60_000,
    open: true,
    minimized: true,
    messages: [{
      id: 'status-example-old-task',
      role: 'ctox',
      commandId: 'cmd-example-old-task',
      status: 'queued',
      createdAt: now - 60_000,
    }],
  };
  const historicalRemote = {
    ...historicalLocal,
    updated_at_ms: now + 1,
    messages: [
      ...historicalLocal.messages,
      {
        id: 'reply-example-old-task',
        role: 'ctox',
        text: 'Der alte Task ist fehlgeschlagen.',
        commandId: 'cmd-example-old-task',
        status: 'failed',
        createdAt: now,
      },
    ],
  };
  const state = {
    ownerUserId: 'user-1',
    activeChatId: active.id,
    selectedDate: __businessChatTestInternals.getLocalDateString(now),
    lastUiMutationMs: now,
    remoteHydrationComplete: true,
    chats: [historicalLocal, active],
  };
  const docs = [historicalRemote].map((row) => ({ toJSON: () => ({ ...row }) }));

  try {
    await __businessChatTestInternals.hydrateChatsFromRxDb({
      state,
      db: { raw: { business_chats: { find: () => ({ exec: async () => docs }) } } },
      session: { user: { id: 'user-1' } },
    });

    assert.equal(state.activeChatId, active.id);
    assert.equal(state.chats.find((chat) => chat.id === active.id)?.minimized, false);
    assert.equal(state.chats.find((chat) => chat.id === historicalLocal.id)?.minimized, true);
  } finally {
    globalThis.localStorage = previousLocalStorage;
  }
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

test('business chat does not resync messages explicitly marked untrackable', () => {
  const state = {
    chats: [{
      id: 'chat-static-status',
      messages: [{
        id: 'message-static',
        commandId: 'cmd-static',
        taskId: 'task-static',
        status: 'running',
        trackable: false,
      }],
    }],
  };

  assert.deepEqual(__businessChatTestInternals.collectTrackedMessages(state), []);
  assert.equal(__businessChatTestInternals.hasActiveTrackedMessages(state), false);
});

test('business chat tracking watch pins command and queue collections until terminal replies exist', () => {
  const timers = [];
  const commands = makeSubscriptionCollection();
  const queue = makeSubscriptionCollection();
  const state = {
    chats: [{
      id: 'chat-tracking',
      messages: [{
        id: 'message-terminal',
        commandId: 'cmd-terminal',
        taskId: 'task-terminal',
        status: 'completed',
      }, {
        id: 'reply-terminal',
        role: 'ctox',
        text: 'Erledigt.',
        replyFor: 'task-terminal',
        commandId: 'cmd-terminal',
        taskId: 'task-terminal',
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
  state.chats[0].messages.pop();
  assert.equal(watch.refresh({ schedule: true }), true);
  assert.equal(watch.isWatching(), true);
  assert.equal(commands.stats.subscribeCalls, 1);
  assert.equal(queue.stats.subscribeCalls, 1);
  assert.equal(timers.filter((timer) => timer.kind === 'interval').length, 1);
  assert.equal(syncCalls, 1);

  commands.emit();
  assert.equal(syncCalls, 2);

  state.chats[0].messages[0].status = 'completed';
  assert.equal(watch.refresh(), true);
  assert.equal(watch.isWatching(), true);
  state.chats[0].messages.push({
    id: 'reply-terminal-next',
    role: 'ctox',
    text: 'Erledigt.',
    replyFor: 'task-terminal',
    commandId: 'cmd-terminal',
    taskId: 'task-terminal',
    status: 'completed',
  });
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

// REGRESSION: blocked work must not read as failed work.
//
// `blocked` and `stale_missing_native` mean "waiting" — for an approval, or
// for the native peer to come back. Every other Business OS surface reports
// them as their own state; the chat folded them into failure, so a command
// that was still alive showed a red failure badge, pushed a "CTOX konnte die
// Aufgabe nicht ausführen" message, and — because failure counted as
// terminal — stopped being tracked, so the correction never arrived.
{
  const {
    isBlockedTrackingStatus,
    isFailureStatus,
    isTerminalTrackingStatus,
    isActiveTrackingStatus,
    getTaskState,
  } = __businessChatTestInternals;

  test('blocked statuses are their own state, not failures', () => {
    for (const status of ['blocked', 'stale_missing_native']) {
      assert.equal(isBlockedTrackingStatus(status), true, status);
      assert.equal(isFailureStatus(status), false, `${status} must not count as failure`);
    }
    for (const status of ['failed', 'error']) {
      assert.equal(isFailureStatus(status), true, status);
      assert.equal(isBlockedTrackingStatus(status), false, `${status} is a failure, not a block`);
    }
  });

  test('blocked work stays tracked instead of being closed out as terminal', () => {
    for (const status of ['blocked', 'stale_missing_native']) {
      assert.equal(isTerminalTrackingStatus(status), false, `${status} must stay trackable`);
      assert.equal(isActiveTrackingStatus(status), true, `${status} must keep polling alive`);
    }
    // Real terminals are unaffected.
    for (const status of ['completed', 'failed', 'cancelled', 'error']) {
      assert.equal(isTerminalTrackingStatus(status), true, status);
    }
  });

  test('a blocked command renders as blocked, not as failed', () => {
    const chatWith = (status) => ({
      id: 'chat_1',
      lastTrackingId: 'cmd_1',
      messages: [{ role: 'user', commandId: 'cmd_1', status }],
    });
    assert.equal(getTaskState(chatWith('blocked')), 'blocked');
    assert.equal(getTaskState(chatWith('stale_missing_native')), 'blocked');
    assert.equal(getTaskState(chatWith('failed')), 'failed');
    assert.equal(getTaskState(chatWith('completed')), 'success');
  });
}

// REGRESSION C1/C2/C4: a status tick with no content change must not rebuild the
// chat window DOM, rewrite identical text/class attributes, or yank the message
// list back to the bottom while the user is reading higher up.
{
  const {
    isScrolledToBottom,
    renderChatRoot,
    setAttrIfChanged,
    setClassNameIfChanged,
    setTextIfChanged,
  } = __businessChatTestInternals;

  test('setTextIfChanged / setClassNameIfChanged write only on real change', () => {
    const textEl = { textContent: 'Queue' };
    assert.equal(setTextIfChanged(textEl, 'Queue'), false);
    assert.equal(textEl.textContent, 'Queue');
    assert.equal(setTextIfChanged(textEl, 'Aktiv'), true);
    assert.equal(textEl.textContent, 'Aktiv');

    const classEl = { className: 'ctox-chat-chip is-task-queued is-active is-expanded' };
    assert.equal(
      setClassNameIfChanged(classEl, 'ctox-chat-chip is-task-queued is-active is-expanded'),
      false,
    );
    assert.equal(
      setClassNameIfChanged(classEl, 'ctox-chat-chip is-task-running is-active is-expanded'),
      true,
    );
    assert.equal(classEl.className, 'ctox-chat-chip is-task-running is-active is-expanded');

    const attrEl = {
      attrs: { 'aria-label': 'CTOX: Queue, geöffnet' },
      getAttribute(name) { return this.attrs[name] ?? null; },
      setAttribute(name, value) { this.attrs[name] = String(value); },
    };
    assert.equal(setAttrIfChanged(attrEl, 'aria-label', 'CTOX: Queue, geöffnet'), false);
    assert.equal(setAttrIfChanged(attrEl, 'aria-label', 'CTOX: Aktiv, geöffnet'), true);
    assert.equal(attrEl.getAttribute('aria-label'), 'CTOX: Aktiv, geöffnet');
  });

  test('isScrolledToBottom only counts a view that is already near the end', () => {
    assert.equal(isScrolledToBottom({
      scrollHeight: 1000,
      scrollTop: 980,
      clientHeight: 200,
    }), true);
    assert.equal(isScrolledToBottom({
      scrollHeight: 1000,
      scrollTop: 100,
      clientHeight: 200,
    }), false);
  });

  test('status tick without change produces no chat DOM replacement', () => {
    const previousDocument = globalThis.document;
    const previousWindow = globalThis.window;
    const previousRequestAnimationFrame = globalThis.requestAnimationFrame;
    const previousCancelAnimationFrame = globalThis.cancelAnimationFrame;
    const previousInnerWidth = globalThis.innerWidth;
    const mutations = [];

    globalThis.document = {
      documentElement: { lang: 'de' },
      getElementById() { return null; },
      createElement() {
        return {
          id: '',
          textContent: '',
          setAttribute() {},
        };
      },
      head: { appendChild() {} },
    };
    globalThis.requestAnimationFrame = (fn) => {
      fn();
      return 1;
    };
    globalThis.cancelAnimationFrame = () => {};
    globalThis.innerWidth = 1200;
    globalThis.window = {
      innerWidth: 1200,
      setTimeout() { return 1; },
      clearTimeout() {},
      setInterval() { return 1; },
      clearInterval() {},
      requestAnimationFrame: globalThis.requestAnimationFrame,
      cancelAnimationFrame: globalThis.cancelAnimationFrame,
      addEventListener() {},
      removeEventListener() {},
      dispatchEvent() { return true; },
    };

    try {
      const chat = {
        id: 'chat-stable',
        title: 'Recherche',
        open: true,
        minimized: false,
        maximized: false,
        createdAt: Date.now(),
        draft: '',
        attachments: [],
        lastTrackingId: 'task-stable',
        messages: [{
          id: 'status-stable',
          role: 'ctox',
          text: 'Task angelegt und in der CTOX Queue.',
          commandId: 'cmd-stable',
          taskId: 'task-stable',
          status: 'queued',
          createdAt: Date.now(),
        }],
      };
      const state = {
        ownerUserId: 'user-1',
        selectedDate: __businessChatTestInternals.getLocalDateString(chat.createdAt),
        activeChatId: chat.id,
        dockCollapsed: false,
        chatListOpen: false,
        dateWorkloadOpen: false,
        chats: [chat],
      };

      const root = makeChatRootFixture({ chat, mutations });
      // First pass: shape already matches, so the in-place path runs.
      renderChatRoot({
        root,
        state,
        commandBus: null,
        db: null,
        getActiveModule: () => ({ id: 'outbound', title: 'Outbound' }),
      });
      const afterFirst = mutations.length;

      // Second pass: identical status tick. Must not replace DOM or rewrite text.
      renderChatRoot({
        root,
        state,
        commandBus: null,
        db: null,
        getActiveModule: () => ({ id: 'outbound', title: 'Outbound' }),
      });
      const secondPassMutations = mutations.slice(afterFirst);
      assert.deepEqual(
        secondPassMutations,
        [],
        `status tick without change must not touch the chat DOM, got: ${JSON.stringify(secondPassMutations)}`,
      );

      // Reader scrolled up: a later message rewrite must preserve that position.
      // Keep the task state queued so the in-place path stays active (a terminal
      // status would flip the composer signature and force a full rebuild).
      const messages = root.querySelector('.ctox-chat-messages');
      messages.scrollTop = 12;
      messages.clientHeight = 200;
      messages.scrollHeight = 800;
      const mutationsBeforeRead = mutations.length;
      chat.messages.push({
        id: 'progress-stable',
        role: 'ctox',
        text: 'Zwischenschritt sichtbar, ohne den Leser nach unten zu ziehen.',
        commandId: 'cmd-stable',
        taskId: 'task-stable',
        status: 'queued',
        createdAt: Date.now() + 1,
      });
      renderChatRoot({
        root,
        state,
        commandBus: null,
        db: null,
        getActiveModule: () => ({ id: 'outbound', title: 'Outbound' }),
      });
      const readPassMutations = mutations.slice(mutationsBeforeRead);
      assert.equal(
        readPassMutations.some((entry) => entry.type === 'innerHTML' && entry.target === 'messages'),
        true,
        'new message content must still update the message list',
      );
      const yanked = readPassMutations.some((entry) => (
        entry.type === 'scrollTop'
        && entry.target === 'messages'
        && entry.value >= 800
      ));
      assert.equal(yanked, false, 'reader who scrolled up must keep their position');
      assert.equal(messages.scrollTop, 12, 'scrollTop stays where the reader left it');
    } finally {
      if (previousDocument === undefined) delete globalThis.document;
      else globalThis.document = previousDocument;
      if (previousWindow === undefined) delete globalThis.window;
      else globalThis.window = previousWindow;
      if (previousRequestAnimationFrame === undefined) delete globalThis.requestAnimationFrame;
      else globalThis.requestAnimationFrame = previousRequestAnimationFrame;
      if (previousCancelAnimationFrame === undefined) delete globalThis.cancelAnimationFrame;
      else globalThis.cancelAnimationFrame = previousCancelAnimationFrame;
      if (previousInnerWidth === undefined) delete globalThis.innerWidth;
      else globalThis.innerWidth = previousInnerWidth;
    }
  });
}

function makeChatRootFixture({ chat, mutations }) {
  const classListFor = (initial = []) => {
    const set = new Set(initial);
    return {
      contains(name) { return set.has(name); },
      add(...names) { names.forEach((name) => set.add(name)); },
      remove(...names) { names.forEach((name) => set.delete(name)); },
      toggle(name, force) {
        if (force === true) set.add(name);
        else if (force === false) set.delete(name);
        else if (set.has(name)) set.delete(name);
        else set.add(name);
        return set.has(name);
      },
      toString() { return [...set].join(' '); },
    };
  };

  const track = (type, target, value) => {
    mutations.push({ type, target, value: value === undefined ? true : value });
  };

  const makeTextNode = (initial, label) => {
    let text = String(initial ?? '');
    return {
      get textContent() { return text; },
      set textContent(value) {
        const next = String(value ?? '');
        if (text === next) return;
        text = next;
        track('textContent', label, next);
      },
    };
  };

  const makeAttrNode = (initialAttrs = {}, label = 'node') => {
    const attrs = { ...initialAttrs };
    let html = '';
    return {
      attrs,
      getAttribute(name) { return Object.prototype.hasOwnProperty.call(attrs, name) ? attrs[name] : null; },
      setAttribute(name, value) {
        const next = String(value ?? '');
        if (attrs[name] === next) return;
        attrs[name] = next;
        track('setAttribute', `${label}.${name}`, next);
      },
      removeAttribute(name) {
        if (!Object.prototype.hasOwnProperty.call(attrs, name)) return;
        delete attrs[name];
        track('removeAttribute', `${label}.${name}`);
      },
      hasAttribute(name) { return Object.prototype.hasOwnProperty.call(attrs, name); },
      get innerHTML() { return html; },
      set innerHTML(value) {
        const next = String(value ?? '');
        if (html === next) return;
        html = next;
        track('innerHTML', label, next);
      },
    };
  };

  const messagesHtml = `<article class="ctox-chat-message is-ctox"><div class="ctox-chat-body"><span class="ctox-chat-text">Task angelegt und in der CTOX Queue.</span></div><footer><button class="ctox-chat-track" type="button">Fortschritt ansehen</button><span>queued</span></footer></article>`;
  let messagesInner = messagesHtml;
  const messages = {
    className: 'ctox-chat-messages',
    scrollTop: 0,
    scrollHeight: 400,
    clientHeight: 400,
    get innerHTML() { return messagesInner; },
    set innerHTML(value) {
      const next = String(value ?? '');
      if (messagesInner === next) return;
      messagesInner = next;
      track('innerHTML', 'messages', next);
    },
    querySelector() { return null; },
    querySelectorAll() { return []; },
  };
  // Proxy scrollTop writes for C4 assertions.
  let scrollTopValue = 0;
  Object.defineProperty(messages, 'scrollTop', {
    configurable: true,
    get() { return scrollTopValue; },
    set(value) {
      const next = Number(value) || 0;
      if (scrollTopValue === next) return;
      scrollTopValue = next;
      track('scrollTop', 'messages', next);
    },
  });

  const titleStrong = makeTextNode(chat.title || 'CTOX', 'titleStrong');
  const maxBtn = makeAttrNode({ 'aria-label': 'Arbeitsfenster maximieren' }, 'maxBtn');
  maxBtn.querySelectorAll = () => [];
  const markEl = {
    className: 'ctox-chat-chip-mark is-queued',
    classList: classListFor(['ctox-chat-chip-mark', 'is-queued']),
    get outerHTML() { return '<span class="ctox-chat-chip-mark is-queued"></span>'; },
    set outerHTML(value) { track('outerHTML', 'chipMark', value); },
  };
  const chipSmall = makeTextNode('Queue', 'chipSmall');
  const chipStrong = makeTextNode(chat.title || 'CTOX', 'chipStrong');
  const chip = {
    className: 'ctox-chat-chip is-task-queued is-active is-expanded',
    dataset: { chatFocus: chat.id },
    classList: classListFor(['ctox-chat-chip', 'is-task-queued', 'is-active', 'is-expanded']),
    ...makeAttrNode({
      'aria-label': 'Recherche: Queue, geöffnet',
      title: 'Recherche: Queue, geöffnet',
    }, 'chip'),
    querySelector(selector) {
      if (selector === '.ctox-chat-chip-copy small') return chipSmall;
      if (selector === '.ctox-chat-chip-copy strong') return chipStrong;
      if (selector === '.ctox-chat-chip-mark') return markEl;
      return null;
    },
    getBoundingClientRect() {
      return { left: 20, right: 140, width: 120, top: 0, bottom: 28, height: 28 };
    },
    scrollIntoView() { track('scrollIntoView', 'chip'); },
  };

  const interactiveNodes = [];
  const win = {
    className: 'ctox-chat-window is-active is-task-queued',
    dataset: {
      chatId: chat.id,
      chatRel: 'center',
      chatAttachmentSignature: '',
      chatComposerSignature: 'conversation',
    },
    classList: classListFor(['ctox-chat-window', 'is-active', 'is-task-queued']),
    style: {},
    querySelector(selector) {
      if (selector === '.ctox-chat-title strong') return titleStrong;
      if (selector === '[data-chat-maximize]') return maxBtn;
      if (selector === '.ctox-chat-messages') return messages;
      if (selector === '[name="message"]') return null;
      if (selector === '[data-chat-form]') return null;
      return null;
    },
    querySelectorAll(selector) {
      if (selector === 'button, input, textarea, select, a, summary') return interactiveNodes;
      return [];
    },
    getBoundingClientRect() {
      return { left: 40, right: 360, width: 320, top: 100, bottom: 500, height: 400 };
    },
    addEventListener() {},
  };
  Object.defineProperty(win, 'className', {
    configurable: true,
    get() { return win._className || 'ctox-chat-window is-active is-task-queued'; },
    set(value) {
      const next = String(value ?? '');
      if (win._className === next) return;
      win._className = next;
      track('className', 'window', next);
    },
  });
  win._className = 'ctox-chat-window is-active is-task-queued';
  Object.defineProperty(win.dataset, 'chatRel', {
    configurable: true,
    get() { return win._chatRel || 'center'; },
    set(value) {
      const next = String(value ?? '');
      if (win._chatRel === next) return;
      win._chatRel = next;
      track('dataset', 'window.chatRel', next);
    },
  });
  win._chatRel = 'center';
  win.style = new Proxy({}, {
    get(target, prop) { return target[prop] || ''; },
    set(target, prop, value) {
      const next = String(value ?? '');
      if (target[prop] === next) return true;
      target[prop] = next;
      track('style', `window.${String(prop)}`, next);
      return true;
    },
  });

  Object.defineProperty(chip, 'className', {
    configurable: true,
    get() { return chip._className || 'ctox-chat-chip is-task-queued is-active is-expanded'; },
    set(value) {
      const next = String(value ?? '');
      if (chip._className === next) return;
      chip._className = next;
      track('className', 'chip', next);
    },
  });
  chip._className = 'ctox-chat-chip is-task-queued is-active is-expanded';

  const dock = {
    className: 'ctox-chat-dock has-visible-chats has-one-chat has-no-nav',
    classList: classListFor(['ctox-chat-dock', 'has-visible-chats', 'has-one-chat', 'has-no-nav']),
    querySelector(selector) {
      if (selector === '[data-chat-strip]') return strip;
      return null;
    },
    getBoundingClientRect() {
      return { left: 20, right: 480, width: 460, top: 640, bottom: 700, height: 60 };
    },
  };
  Object.defineProperty(dock, 'className', {
    configurable: true,
    get() { return dock._className || 'ctox-chat-dock has-visible-chats has-one-chat has-no-nav'; },
    set(value) {
      const next = String(value ?? '');
      if (dock._className === next) return;
      dock._className = next;
      track('className', 'dock', next);
    },
  });
  dock._className = 'ctox-chat-dock has-visible-chats has-one-chat has-no-nav';

  const fabBadge = makeTextNode('1', 'fabBadge');
  const datePicker = { value: __businessChatTestInternals.getLocalDateString(chat.createdAt), addEventListener() {} };
  const strip = {
    className: 'ctox-chat-strip',
    classList: classListFor(['ctox-chat-strip']),
    scrollLeft: 0,
    scrollWidth: 120,
    clientWidth: 400,
    querySelector(selector) {
      if (selector === `[data-chat-focus="${chat.id}"]`) return chip;
      return null;
    },
    querySelectorAll(selector) {
      if (selector === '.ctox-chat-chip' || selector === '[data-chat-focus]') return [chip];
      return [];
    },
    getBoundingClientRect() {
      return { left: 20, right: 420, width: 400, top: 640, bottom: 680, height: 40 };
    },
  };
  const stageInner = {
    className: 'ctox-chat-stage-inner',
    classList: classListFor(['ctox-chat-stage-inner']),
    querySelector(selector) {
      if (selector === '.ctox-chat-stage-spacer') return spacer;
      return null;
    },
    querySelectorAll(selector) {
      if (selector === '.ctox-chat-window') return [win];
      return [];
    },
    getBoundingClientRect() {
      return { left: 0, right: 800, width: 800, top: 80, bottom: 600, height: 520 };
    },
  };
  const spacer = { style: new Proxy({}, {
    get(target, prop) { return target[prop] || ''; },
    set(target, prop, value) {
      const next = String(value ?? '');
      if (target[prop] === next) return true;
      target[prop] = next;
      track('style', `spacer.${String(prop)}`, next);
      return true;
    },
  }) };
  const stage = {
    querySelector(selector) {
      if (selector === '.ctox-chat-stage-inner') return stageInner;
      return null;
    },
  };

  const nodesBySelector = new Map([
    ['[data-chat-date-picker]', datePicker],
    ['[data-chat-busy-panel]', null],
    ['[data-chat-date-workload-panel]', null],
    ['[data-chat-dock]', dock],
    ['.ctox-chat-fab b', fabBadge],
    ['[data-chat-strip]', strip],
    ['[data-chat-stage]', stage],
    ['.ctox-chat-stage-inner', stageInner],
    ['.ctox-chat-messages', messages],
  ]);

  const root = {
    className: 'ctox-chat-root',
    classList: classListFor(['ctox-chat-root']),
    isConnected: true,
    __ctoxChatSync: null,
    __ctoxChatOnTrackingStateChanged: null,
    __ctoxChatLayoutFrame: 0,
    querySelector(selector) {
      if (nodesBySelector.has(selector)) return nodesBySelector.get(selector);
      if (selector === '.ctox-chat-window') return win;
      if (selector.startsWith('[data-chat-focus=')) return chip;
      return null;
    },
    querySelectorAll(selector) {
      if (selector === '.ctox-chat-window') return [win];
      if (selector === '.ctox-chat-chip') return [chip];
      if (selector === '[data-chat-focus]') return [chip];
      if (selector === '[data-chat-id]') return [win];
      if (selector === '[data-chat-id]:not(.is-minimized)') return [win];
      if (selector === '[data-countdown-timer]') return [];
      if (selector === '.ctox-chat-window.no-left-transition') return [];
      return [];
    },
    getBoundingClientRect() {
      return { left: 0, right: 800, width: 800, top: 80, bottom: 700, height: 620 };
    },
    addEventListener() {},
    set innerHTML(value) {
      track('innerHTML', 'root', String(value || '').slice(0, 80));
    },
  };

  return root;
}

test('ein gespeichertes Verlaufsdatum ueberdauert das Oeffnen nicht', () => {
  // Am 11.08.2026 stand die Chat-Leiste eines Nutzers beim Oeffnen noch auf dem
  // 6. August und zeigte die 24 Chats jenes Tages — darunter Fehlversuche aus
  // einer Fassung, die es nicht mehr gibt. Die vier erfolgreichen Laeufe des
  // laufenden Tages waren dahinter unsichtbar. Die Leiste startet ab jetzt
  // immer heute; vergangene Tage bleiben ueber die Datumsauswahl erreichbar.
  const { readChatState, getLocalDateString } = __businessChatTestInternals;
  const heute = getLocalDateString(Date.now());
  const gespeichert = { selectedDate: '2026-08-06', chats: [], activeChatId: '' };

  const vorherigerSpeicher = globalThis.localStorage;
  globalThis.localStorage = {
    getItem: (key) => (key === 'ctox.businessOs.chat.v1' ? JSON.stringify(gespeichert) : null),
    setItem() {},
  };
  try {
    const state = readChatState({ user: { id: 'nutzer-1' } });
    assert.equal(state.selectedDate, heute);
    assert.notEqual(state.selectedDate, '2026-08-06');
  } finally {
    if (vorherigerSpeicher === undefined) delete globalThis.localStorage;
    else globalThis.localStorage = vorherigerSpeicher;
  }
});

test('die Leiste springt beim Oeffnen nicht auf einen alten Tag', () => {
  // Am 11.08.2026 sprang die Leiste beim Oeffnen von Outbound auf den 28. Juli
  // und zeigte 26 alte Fehlversuche. Ursache war der Rueckfall auf den neuesten
  // Chat aus IRGENDEINEM Tag, sobald heute nichts Offenes vorlag.
  const { preferredChatForDockOpen, getLocalDateString } = __businessChatTestInternals;
  const heute = getLocalDateString(Date.now());
  const alt = {
    id: 'chat-alt', createdAt: Date.parse('2026-07-28T10:00:00Z'), open: true,
    messages: [{ role: 'user', text: 'alter Fehlversuch' }],
  };
  assert.equal(preferredChatForDockOpen({ chats: [alt] }), null);

  const neu = {
    id: 'chat-heute', createdAt: Date.now(), open: true,
    messages: [{ role: 'user', text: 'heutiger Lauf' }],
  };
  const gewaehlt = preferredChatForDockOpen({ chats: [alt, neu] });
  assert.equal(gewaehlt?.id, 'chat-heute');
  assert.equal(getLocalDateString(gewaehlt.createdAt), heute);
});

test('ein Lead-Chat von gestern wird nicht als heutiger wiederverwendet', () => {
  // Ein Lead behaelt seinen Kennschluessel ueber Wochen. Am 11.08.2026 lieferte
  // resolveChatForOpenDetail deshalb beim Wechsel in eine Kampagne den Chat des
  // Juli-Laufs zurueck; focusChatForUser zog die Leiste auf den 26. Juli.
  const { resolveChatForOpenDetail, getLocalDateString } = __businessChatTestInternals;
  const heute = getLocalDateString(Date.now());
  const detail = { chat_id: 'chat-juli' };
  const alt = { id: 'chat-juli', createdAt: Date.parse('2026-07-26T09:00:00Z'), open: true, messages: [] };
  const state = { ownerUserId: 'nutzer-1', selectedDate: heute, chats: [alt] };

  const gewaehlt = resolveChatForOpenDetail(state, { user: { id: 'nutzer-1' } }, detail);
  assert.notEqual(gewaehlt.id, 'chat-juli');
  assert.equal(getLocalDateString(gewaehlt.createdAt), heute);
});

test('focusChatForUser zieht die Leiste nicht ungefragt in die Vergangenheit', () => {
  // Die Wurzel aller fuenf Spruenge: focusChatForUser setzte selectedDate
  // bedingungslos auf das Datum des Chats und wird aus sechs Richtungen
  // gerufen, meist aus dem Hintergrund.
  const { focusChatForUser, getLocalDateString } = __businessChatTestInternals;
  const heute = getLocalDateString(Date.now());
  const alt = { id: 'chat-juli', createdAt: Date.parse('2026-07-26T09:00:00Z') };

  const hintergrund = { selectedDate: heute, chats: [alt] };
  focusChatForUser(hintergrund, alt);
  assert.equal(hintergrund.selectedDate, heute, 'Hintergrundvorgang darf den Tag nicht wechseln');

  const nutzer = { selectedDate: heute, chats: [alt] };
  focusChatForUser(nutzer, alt, { allowDateChange: true });
  assert.equal(nutzer.selectedDate, '2026-07-26', 'ausdrueckliche Navigation muss den Tag wechseln');
});

test('ein spaeter Oeffner verliert nach lokalem Minimieren sein Besitz-Ticket', () => {
  const {
    claimChatOpenOwnership,
    ownsChatOpenOwnership,
    markChatExpandedByUser,
    markChatMinimizedByUser,
  } = __businessChatTestInternals;
  const state = {};
  const chat = { id: 'chat-ticket', open: true, minimized: false };
  const openTicket = claimChatOpenOwnership(state);

  assert.equal(ownsChatOpenOwnership(state, openTicket), true);
  markChatMinimizedByUser(state, chat);
  assert.equal(ownsChatOpenOwnership(state, openTicket), false);
  assert.equal(markChatExpandedByUser(state, chat, openTicket), false);
  assert.equal(chat.minimized, true);
  assert.equal(chat.userMinimized, true);
});

test('lokal neueres Minimieren gewinnt gegen ein spaeteres business_chats Echo', async () => {
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
  const now = Date.now();
  const localChat = {
    id: 'chat-local-minimized',
    owner_user_id: 'user-1',
    title: 'Laufender Research',
    createdAt: now - 1000,
    updated_at_ms: now,
    presentation_updated_at_ms: now + 2000,
    open: true,
    minimized: true,
    userMinimized: true,
    messages: [{
      id: 'status-local-minimized',
      role: 'ctox',
      commandId: 'cmd-local-minimized',
      taskId: 'task-local-minimized',
      status: 'running',
      createdAt: now - 500,
    }],
  };
  const remoteChat = {
    ...localChat,
    updated_at_ms: now + 3000,
    presentation_updated_at_ms: now + 1000,
    minimized: false,
    userMinimized: false,
    messages: [
      ...localChat.messages,
      {
        id: 'reply-local-minimized',
        role: 'ctox',
        text: 'Fertig, ohne die Leiste erneut zu oeffnen.',
        replyFor: 'task-local-minimized',
        commandId: 'cmd-local-minimized',
        taskId: 'task-local-minimized',
        status: 'completed',
        createdAt: now + 3000,
      },
    ],
  };
  const state = {
    ownerUserId: 'user-1',
    selectedDate: __businessChatTestInternals.getLocalDateString(now),
    activeChatId: localChat.id,
    dockCollapsed: false,
    remoteHydrationComplete: true,
    deletedChatIds: {},
    chats: [localChat],
  };

  try {
    const changed = await __businessChatTestInternals.hydrateChatsFromRxDb({
      state,
      session: { user: { id: 'user-1' } },
      db: { raw: { business_chats: makeFindCollection([remoteChat]) } },
    });

    const merged = state.chats.find((chat) => chat.id === localChat.id);
    assert.equal(changed, true);
    assert.equal(merged.minimized, true);
    assert.equal(merged.userMinimized, true);
    assert.equal(merged.presentation_updated_at_ms, localChat.presentation_updated_at_ms);
    assert.equal(merged.messages.at(-1).status, 'completed');
  } finally {
    if (previousLocalStorage === undefined) delete globalThis.localStorage;
    else globalThis.localStorage = previousLocalStorage;
  }
});

test('lokales Minimieren ueberlebt Persistenz und Reload', async () => {
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
  const state = {
    ownerUserId: 'user-1',
    selectedDate: __businessChatTestInternals.getLocalDateString(Date.now()),
    activeChatId: 'chat-reload-minimized',
    dockCollapsed: false,
    chats: [{
      id: 'chat-reload-minimized',
      owner_user_id: 'user-1',
      title: 'Persistenter Chat',
      createdAt: Date.now(),
      updated_at_ms: Date.now(),
      open: true,
      minimized: false,
      messages: [],
    }],
  };

  try {
    __businessChatTestInternals.markChatMinimizedByUser(state, state.chats[0]);
    const minimizedAt = state.chats[0].presentation_updated_at_ms;
    await __businessChatTestInternals.persistChatState({ state, db: null, remote: false });
    const restored = __businessChatTestInternals.readChatState({ user: { id: 'user-1' } });
    assert.equal(restored.chats[0].minimized, true);
    assert.equal(restored.chats[0].userMinimized, true);
    assert.equal(restored.chats[0].presentation_updated_at_ms, minimizedAt);
  } finally {
    if (previousLocalStorage === undefined) delete globalThis.localStorage;
    else globalThis.localStorage = previousLocalStorage;
  }
});

test('eine Antwort auf einen alten Chat holt sich die Ansicht nicht', () => {
  // remoteReplyChatToFocus meldet jeden Chat mit neuer Serverantwort. Der
  // Abgleich schreibt laufend in alte Vorgaenge nach — am 11.08.2026 riss
  // deshalb jede Nachmeldung die Leiste auf den 26. Juli. Nachgewiesen mit
  // einer Aufrufspur aus der laufenden Seite:
  //   set selectedDate 2026-08-11 -> 2026-07-26
  //     at focusChatForUser (business-chat.js:1644)
  //     at hydrateChatsFromRxDb (business-chat.js:3673)
  const { focusChatForUser, getLocalDateString } = __businessChatTestInternals;
  const heute = getLocalDateString(Date.now());
  const alterChat = { id: 'chat-juli', createdAt: Date.parse('2026-07-26T09:00:00Z') };

  // So ruft der Antwort-Pfad jetzt: erst pruefen, dann fokussieren.
  const state = { selectedDate: heute, chats: [alterChat] };
  if (getLocalDateString(alterChat.createdAt) === heute) {
    focusChatForUser(state, alterChat, { allowDateChange: true });
  }
  assert.equal(state.selectedDate, heute);
  assert.notEqual(state.selectedDate, '2026-07-26');
});

test('die Chat-Leiste faengt nur dort Klicks, wo sie etwas anzeigt', () => {
  // Am 11.08.2026 lag die Empfaengerauswahl von Outbound Lead Generation im
  // durchsichtigen Zwischenraum des Docks. Der Detailbereich war bis zum
  // Anschlag gescrollt, das Haekchen sichtbar — und jeder Klick landete im
  // Dock. elementsFromPoint zeigte SECTION.ctox-chat-dock zuoberst. Ohne
  // Empfaenger keine Sellify-Uebergabe, kein Serienbrief, keine Serien-E-Mail:
  // die gesamte Kette endete an einem unsichtbaren Rechteck.
  const css = businessChatSource;
  const dockRegel = css.slice(css.indexOf('.ctox-chat-dock {'), css.indexOf('.ctox-chat-dock {') + 400);
  assert.match(dockRegel, /pointer-events:\s*none/,
    'Der Dock-Container darf keine Klicks der App darunter abfangen');

  // Und die Bedienelemente muessen sie zurueckbekommen, sonst ist die Leiste tot.
  assert.match(css, /\.ctox-chat-dock > \*,[\s\S]{0,200}pointer-events:\s*auto/,
    'Die sichtbaren Kinder des Docks brauchen pointer-events: auto');
});

// --- Vorgegebene Befehls-ID: einmal gueltig, danach frisch ------------------
// Web Research backt die Command-ID in den Prompttext und in den Writeback-
// Vertrag. Wuerde der Chat beim Senden eine eigene ID erfinden, zeigte der
// Research-Run auf einen Befehl, den es nie gab - genau das Muster der
// verwaisten "Failed"-Laeufe.
test('chatContextMetaFromDetail traegt eine vorgegebene Befehls-ID weiter', () => {
  const meta = __businessChatTestInternals.chatContextMetaFromDetail({
    module: 'research',
    command_type: 'research.systematic.run',
    command_id: 'cmd_vorgegeben',
    record_id: 'research_task_1',
  });
  assert.equal(meta.command_id, 'cmd_vorgegeben');
  assert.equal(meta.command_type, 'research.systematic.run');
});

test('ohne vorgegebene Befehls-ID bleibt das Feld leer statt erfunden', () => {
  const meta = __businessChatTestInternals.chatContextMetaFromDetail({
    module: 'outbound',
    command_type: 'business_os.chat.task',
  });
  assert.equal('command_id' in meta, false);
});

test('die vorgegebene Befehls-ID wird beim Senden verbraucht, nicht wiederverwendet', () => {
  const source = businessChatSource;
  const readIndex = source.indexOf('const commandId = meta.command_id || meta.commandId ||');
  const deleteIndex = source.indexOf('delete chat.contextMeta.command_id;', readIndex);
  const reassignIndex = source.indexOf('chat.contextMeta = {', readIndex);
  assert.ok(readIndex >= 0, 'commandId wird aus meta gelesen');
  assert.ok(deleteIndex > readIndex, 'die ID wird erst nach dem Lesen entfernt');
  assert.ok(reassignIndex > deleteIndex, 'das Entfernen passiert vor dem Neuaufbau von contextMeta');
});

// --- Web Research geht ueber den Chat, nicht am Chat vorbei -----------------
test('Web Research oeffnet den Chat und dispatcht nicht direkt', () => {
  const researchSource = readFileSync(
    new URL('../modules/research/index.js', import.meta.url),
    'utf8',
  );
  const runStart = researchSource.indexOf('async function runSelectedResearch()');
  assert.ok(runStart >= 0);
  const runBody = researchSource.slice(runStart, runStart + 12000);
  assert.ok(
    runBody.includes('state.ctx?.openBusinessChat'),
    'der Lauf muss den Business Chat oeffnen',
  );
  assert.ok(
    runBody.includes('draft: instruction'),
    'der vollstaendige systematic-research Prompt muss im Eingabefeld stehen',
  );
  assert.ok(
    runBody.includes("command_type: 'research.systematic.run'"),
    'der Chat muss denselben Befehlstyp dispatchen wie zuvor',
  );
  // Der Direktversand bleibt nur als Rueckfallebene ohne Chat-Oberflaeche.
  const dispatchIndex = runBody.indexOf('commandBus.dispatch');
  const fallbackIndex = runBody.indexOf('} else {');
  assert.ok(dispatchIndex > fallbackIndex && fallbackIndex >= 0,
    'commandBus.dispatch darf nur im Rueckfallzweig stehen');
});

test('crew app presence maps active queue tasks to their app by member', () => {
  const members = [
    { id: 'm1', name: 'Pico', shape: 'round', color: '#e0a458', state: 'on_duty' },
    { id: 'm2', name: 'Nia', shape: 'tall', color: '#5aa9e6', state: 'home' },
  ];
  const tasks = [
    { id: 't1', status: 'running', module: 'documents', crew_member_id: 'm1' },
    { id: 't2', status: 'leased', source_module: 'documents', crew_member_id: 'm1' }, // same member twice → once
    { id: 't3', status: 'review', module: 'tickets', crew_member_id: 'm2' },
    { id: 't4', status: 'succeeded', module: 'tickets', crew_member_id: 'm1' }, // finished → no presence
    { id: 't5', status: 'running', module: 'tickets', crew_member_id: 'ghost' }, // unknown member → skipped
    { id: 't6', status: 'running', crew_member_id: 'm2' }, // no module → skipped
  ];
  const presence = crewAppPresenceFromTasks(tasks, members);
  assert.deepEqual([...presence.keys()].sort(), ['documents', 'tickets']);
  assert.deepEqual(presence.get('documents').map((entry) => entry.member.id), ['m1']);
  assert.deepEqual(presence.get('tickets').map((entry) => entry.member.id), ['m2']);
  assert.equal(crewAppPresenceFromTasks([], members).size, 0);
  assert.equal(crewAppPresenceFromTasks(tasks, []).size, 0);
});
