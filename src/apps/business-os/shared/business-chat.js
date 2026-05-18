const CHAT_STYLE_ID = 'ctox-business-chat-style';
const CHAT_STATE_KEY = 'ctox.businessOs.chat.v1';
const CHAT_CHANNEL = 'business_os.llm.chat';

export function initBusinessChat({
  session,
  commandBus,
  db,
  getActiveModule,
}) {
  if (!session?.authenticated || document.querySelector('[data-ctox-chat-root]')) return;
  installChatStyles();
  const state = readChatState();
  const root = document.createElement('div');
  root.className = 'ctox-chat-root';
  root.dataset.ctoxChatRoot = 'true';
  document.body.append(root);

  const sync = () => {
    captureDrafts(root, state);
    syncTrackedMessages({ state, db }).then((changed) => {
      if (changed) writeChatState(state);
      if (changed) renderChatRoot({ root, state, commandBus, db, getActiveModule });
    }).catch(() => {});
  };
  const handleExternalSubmit = async (event) => {
    const detail = event.detail || {};
    const text = String(detail.text || detail.message || '').trim();
    if (!text) return;
    const chat = ensureChat(state);
    chat.open = true;
    chat.minimized = false;
    chat.draft = '';
    await submitChatMessage({ state, chat, text, commandBus, getActiveModule, meta: detail });
    writeChatState(state);
    renderChatRoot({ root, state, commandBus, db, getActiveModule });
    syncTrackedMessages({ state, db }).then((changed) => {
      if (changed) writeChatState(state);
      if (changed) renderChatRoot({ root, state, commandBus, db, getActiveModule });
    }).catch(() => {});
  };

  renderChatRoot({ root, state, commandBus, db, getActiveModule });
  window.addEventListener('ctox-business-os-chat-submit', handleExternalSubmit);
  const businessCommandsSub = db?.raw?.business_commands?.$?.subscribe?.(sync) || null;
  const queueTasksSub = db?.raw?.ctox_queue_tasks?.$?.subscribe?.(sync) || null;
  const timer = window.setInterval(sync, 4000);
  root.__ctoxChatCleanup = () => {
    window.removeEventListener('ctox-business-os-chat-submit', handleExternalSubmit);
    businessCommandsSub?.unsubscribe?.();
    queueTasksSub?.unsubscribe?.();
    window.clearInterval(timer);
  };
}

function renderChatRoot({ root, state, commandBus, db, getActiveModule }) {
  root.innerHTML = `
    <button class="ctox-chat-fab" type="button" data-chat-open>Chat</button>
    <div class="ctox-chat-tray" data-chat-tray>
      ${state.chats.filter((chat) => chat.open !== false).map((chat) => chatWindow(chat)).join('')}
    </div>
  `;
  root.querySelector('[data-chat-open]')?.addEventListener('click', () => {
    const chat = ensureChat(state);
    chat.open = true;
    chat.minimized = false;
    writeChatState(state);
    renderChatRoot({ root, state, commandBus, db, getActiveModule });
  });
  root.querySelectorAll('[data-chat-id]').forEach((node) => {
    const chat = state.chats.find((item) => item.id === node.dataset.chatId);
    if (!chat) return;
    node.querySelector('[data-chat-minimize]')?.addEventListener('click', () => {
      chat.minimized = !chat.minimized;
      writeChatState(state);
      renderChatRoot({ root, state, commandBus, db, getActiveModule });
    });
    node.querySelector('[data-chat-close]')?.addEventListener('click', () => {
      chat.open = false;
      writeChatState(state);
      renderChatRoot({ root, state, commandBus, db, getActiveModule });
    });
    node.querySelector('[data-chat-new]')?.addEventListener('click', () => {
      const next = createChat();
      state.chats.push(next);
      writeChatState(state);
      renderChatRoot({ root, state, commandBus, db, getActiveModule });
    });
    node.querySelectorAll('[data-track-task]').forEach((button) => {
      button.addEventListener('click', () => {
        openCtoxTask(button.dataset.taskId || '', button.dataset.commandId || '', button.dataset.taskStatus || '');
      });
    });
    node.querySelector('[name="message"]')?.addEventListener('input', (event) => {
      chat.draft = event.currentTarget.value;
    });
    node.querySelector('[data-chat-form]')?.addEventListener('submit', async (event) => {
      event.preventDefault();
      const form = event.currentTarget;
      captureDrafts(root, state);
      const input = form.querySelector('[name="message"]');
      const text = String(input?.value || '').trim();
      if (!text) return;
      chat.draft = '';
      input.value = '';
      await submitChatMessage({ state, chat, text, commandBus, getActiveModule });
      writeChatState(state);
      renderChatRoot({ root, state, commandBus, db, getActiveModule });
      syncTrackedMessages({ state, db }).then((changed) => {
        if (changed) writeChatState(state);
        if (changed) renderChatRoot({ root, state, commandBus, db, getActiveModule });
      }).catch(() => {});
    });
  });
}

function captureDrafts(root, state) {
  root.querySelectorAll('[data-chat-id]').forEach((node) => {
    const chat = state.chats.find((item) => item.id === node.dataset.chatId);
    const input = node.querySelector('[name="message"]');
    if (chat && input) chat.draft = input.value;
  });
}

function chatWindow(chat) {
  const minimized = chat.minimized;
  return `
    <section class="ctox-chat-window ${minimized ? 'is-minimized' : ''}" data-chat-id="${escapeAttr(chat.id)}">
      <header>
        <button class="ctox-chat-title" type="button" data-chat-minimize>
          <strong>${escapeHtml(chat.title || 'CTOX')}</strong>
          ${chat.lastTrackingId ? `<span>${escapeHtml(chat.lastTrackingId)}</span>` : '<span>Business OS</span>'}
        </button>
        <div>
          <button type="button" data-chat-new aria-label="Neuer Chat">+</button>
          <button type="button" data-chat-close aria-label="Schliessen">x</button>
        </div>
      </header>
      ${minimized ? '' : `
        <div class="ctox-chat-messages">
          ${chat.messages.length ? chat.messages.map(messageMarkup).join('') : '<div class="ctox-chat-empty">CTOX Aufgabe eingeben.</div>'}
        </div>
        <form class="ctox-chat-form" data-chat-form>
          <textarea name="message" rows="2" placeholder="Aufgabe an CTOX..." required>${escapeHtml(chat.draft || '')}</textarea>
          <button type="submit">Senden</button>
        </form>
      `}
    </section>
  `;
}

function messageMarkup(message) {
  const tracking = message.commandId || message.taskId
    ? `<button class="ctox-chat-track" type="button" data-track-task data-task-id="${escapeAttr(message.taskId || '')}" data-command-id="${escapeAttr(message.commandId || '')}" data-task-status="${escapeAttr(message.status || '')}">${escapeHtml(message.taskId || message.commandId)}</button>`
    : '';
  const meta = [message.status, message.detail].filter(Boolean).join(' · ');
  return `
    <article class="ctox-chat-message is-${escapeAttr(message.role || 'ctox')}">
      <p>${escapeHtml(message.text || '')}</p>
      ${tracking || meta ? `<footer>${tracking}${meta ? `<span>${escapeHtml(meta)}</span>` : ''}</footer>` : ''}
    </article>
  `;
}

async function submitChatMessage({ state, chat, text, commandBus, getActiveModule, meta = {} }) {
  const activeModule = getActiveModule?.() || { id: 'ctox', title: 'CTOX' };
  const sourceModule = meta.module || meta.source_module || activeModule.id || 'ctox';
  const sourceTitle = meta.source_title || activeModule.title || sourceModule || 'CTOX';
  const commandType = meta.command_type || meta.commandType || 'business_os.chat.task';
  const extraPayload = meta.payload && typeof meta.payload === 'object' ? meta.payload : {};
  const extraClientContext = meta.client_context && typeof meta.client_context === 'object' ? meta.client_context : {};
  const now = Date.now();
  const messageId = `chatmsg_${crypto.randomUUID()}`;
  chat.messages.push({
    id: messageId,
    role: 'user',
    text,
    createdAt: now,
  });
  chat.title = chat.title === 'CTOX' ? titleFromText(text) : chat.title;
  const command = {
    module: sourceModule,
    type: commandType,
    record_id: meta.record_id || chat.id,
    inbound_channel: meta.inbound_channel || CHAT_CHANNEL,
    payload: {
      ...extraPayload,
      title: meta.title || extraPayload.title || titleFromText(text),
      instruction: meta.instruction || extraPayload.instruction || text,
      prompt: text,
      chat_id: chat.id,
      message_id: messageId,
      conversation: compactConversation(chat.messages),
      inbound_channel: meta.inbound_channel || CHAT_CHANNEL,
      outbound_channel: 'business_os_chat',
      response_channel: 'business_os_chat',
      reply_to: chat.id,
      thread_key: `business-os/chat/${chat.id}`,
      priority: 'normal',
      source_module: sourceModule,
    },
    client_context: {
      ...extraClientContext,
      source: 'business-os-chat',
      module: sourceModule,
      source_module: sourceModule,
      source_title: sourceTitle,
      inbound_channel: meta.inbound_channel || CHAT_CHANNEL,
      outbound_channel: 'business_os_chat',
      chat_id: chat.id,
      message_id: messageId,
      url: location.href,
      language: document.documentElement.lang || 'de',
      created_at: new Date(now).toISOString(),
    },
  };
  try {
    const result = await commandBus.dispatch(command);
    const taskId = result.task_id || '';
    const commandId = result.command_id || command.id || '';
    chat.lastTrackingId = taskId || commandId;
    chat.messages.push({
      id: `status_${crypto.randomUUID()}`,
      role: 'ctox',
      text: taskId ? 'Task angelegt und in der CTOX Queue. Antwort erscheint hier, sobald der CTOX Service ihn verarbeitet.' : 'Command angelegt. Keine CTOX Queue-ID erhalten.',
      commandId,
      taskId,
      status: result.task_status || result.status || 'queued',
      createdAt: Date.now(),
    });
  } catch (error) {
    const commandId = error?.command_id || error?.commandId || '';
    chat.messages.push({
      id: `error_${crypto.randomUUID()}`,
      role: 'ctox',
      text: error?.message || String(error),
      commandId,
      status: error?.status || 'failed',
      createdAt: Date.now(),
    });
    if (commandId) chat.lastTrackingId = commandId;
  }
}

async function syncTrackedMessages({ state, db }) {
  let changed = false;
  const commands = db?.raw?.business_commands;
  const queue = db?.raw?.ctox_queue_tasks;
  if (!commands && !queue) return false;
  for (const chat of state.chats) {
    for (const message of chat.messages) {
      if (!message.commandId && !message.taskId) continue;
      const commandDoc = message.commandId && commands ? await findDoc(commands, message.commandId) : null;
      const taskDoc = (message.taskId || commandDoc?.task_id) && queue ? await findDoc(queue, message.taskId || commandDoc.task_id) : null;
      const nextTaskId = message.taskId || commandDoc?.task_id || taskDoc?.id || '';
      const nextStatus = taskDoc?.status || commandDoc?.task_status || commandDoc?.status || message.status || '';
      if (nextTaskId && nextTaskId !== message.taskId) {
        message.taskId = nextTaskId;
        chat.lastTrackingId = nextTaskId;
        changed = true;
      }
      if (nextStatus && nextStatus !== message.status) {
        message.status = nextStatus;
        changed = true;
      }
      const outbound = extractOutboundText(commandDoc) || extractOutboundText(taskDoc);
      if (outbound && !chat.messages.some((item) => item.replyFor === (message.taskId || message.commandId))) {
        chat.messages.push({
          id: `reply_${crypto.randomUUID()}`,
          role: 'ctox',
          text: outbound,
          replyFor: message.taskId || message.commandId,
          commandId: message.commandId || '',
          taskId: message.taskId || '',
          status: nextStatus || '',
          createdAt: Date.now(),
        });
        changed = true;
      }
      if (isFailureStatus(nextStatus) && !chat.messages.some((item) => item.failureFor === (message.taskId || message.commandId))) {
        chat.messages.push({
          id: `failure_${crypto.randomUUID()}`,
          role: 'ctox',
          text: failureText(commandDoc, taskDoc),
          failureFor: message.taskId || message.commandId,
          commandId: message.commandId || '',
          taskId: message.taskId || '',
          status: nextStatus || 'failed',
          createdAt: Date.now(),
        });
        changed = true;
      }
    }
  }
  return changed;
}

async function findDoc(collection, id) {
  if (!id) return null;
  const doc = await collection.findOne(id).exec();
  return doc?.toJSON?.() || null;
}

function extractOutboundText(doc) {
  if (!doc || typeof doc !== 'object') return '';
  const candidates = [
    doc.outbound_text,
    doc.response,
    doc.answer,
    doc.summary,
    doc.result_summary,
    doc.result?.outbound_text,
    doc.result?.response,
    doc.result?.answer,
    doc.result?.message,
    doc.result?.summary,
    doc.payload?.outbound_text,
    doc.payload?.response,
    doc.payload?.answer,
  ];
  return String(candidates.find((value) => String(value || '').trim()) || '').trim();
}

function isFailureStatus(status) {
  return ['failed', 'blocked', 'blocked_no_ctox_api', 'stale_missing_native'].includes(String(status || '').toLowerCase());
}

function failureText(commandDoc, taskDoc) {
  const error = taskDoc?.status_note
    || taskDoc?.error
    || commandDoc?.error
    || commandDoc?.client_context?.dispatch_error
    || '';
  if (error) return `CTOX konnte die Aufgabe nicht ausführen: ${error}`;
  return 'CTOX konnte die Aufgabe nicht ausführen. Der Task ist in der CTOX Queue fehlgeschlagen.';
}

function openCtoxTask(taskId, commandId, taskStatus) {
  const focus = { taskId, commandId, taskStatus, sourceModule: 'business-os-chat' };
  try {
    sessionStorage.setItem('ctox.businessOs.focusTask', JSON.stringify(focus));
  } catch {}
  window.dispatchEvent(new CustomEvent('ctox-business-os-focus-task', { detail: focus }));
  const params = new URLSearchParams();
  if (taskId) params.set('task_id', taskId);
  if (commandId) params.set('command_id', commandId);
  if (taskStatus) params.set('task_status', taskStatus);
  params.set('source', 'business-os-chat');
  location.hash = `#ctox?${params.toString()}`;
}

function ensureChat(state) {
  let chat = state.chats.find((item) => item.open !== false) || state.chats[0];
  if (!chat) {
    chat = createChat();
    state.chats.push(chat);
  }
  return chat;
}

function createChat() {
  return {
    id: `chat_${crypto.randomUUID()}`,
    title: 'CTOX',
    open: true,
    minimized: false,
    messages: [],
    draft: '',
    createdAt: Date.now(),
  };
}

function readChatState() {
  try {
    const parsed = JSON.parse(localStorage.getItem(CHAT_STATE_KEY) || '{}') || {};
    const chats = Array.isArray(parsed.chats) ? parsed.chats : [];
    return {
      chats: chats.slice(-4).map((chat) => ({
        id: chat.id || `chat_${crypto.randomUUID()}`,
        title: chat.title || 'CTOX',
        open: chat.open !== false,
        minimized: Boolean(chat.minimized),
        lastTrackingId: chat.lastTrackingId || '',
        messages: Array.isArray(chat.messages) ? chat.messages.slice(-40) : [],
        draft: chat.draft || '',
        createdAt: chat.createdAt || Date.now(),
      })),
    };
  } catch {
    return { chats: [] };
  }
}

function writeChatState(state) {
  localStorage.setItem(CHAT_STATE_KEY, JSON.stringify({
    chats: state.chats.slice(-4).map((chat) => ({
      ...chat,
      messages: chat.messages.slice(-40),
      draft: chat.draft || '',
    })),
  }));
}

function compactConversation(messages) {
  return messages.slice(-10).map((message) => ({
    role: message.role === 'user' ? 'user' : 'ctox',
    text: message.text || '',
    command_id: message.commandId || '',
    task_id: message.taskId || '',
  }));
}

function titleFromText(text) {
  const clean = String(text || '').replace(/\s+/g, ' ').trim();
  return clean.length > 42 ? `${clean.slice(0, 39)}...` : clean || 'CTOX Aufgabe';
}

function installChatStyles() {
  if (document.getElementById(CHAT_STYLE_ID)) return;
  const style = document.createElement('style');
  style.id = CHAT_STYLE_ID;
  style.textContent = `
    .ctox-chat-root {
      position: fixed;
      left: 18px;
      bottom: 18px;
      z-index: 52;
      display: flex;
      align-items: flex-end;
      gap: 10px;
      max-width: calc(100vw - 132px);
      pointer-events: none;
    }
    .ctox-chat-root button,
    .ctox-chat-root textarea {
      font: inherit;
    }
    .ctox-chat-fab {
      pointer-events: auto;
      align-self: flex-end;
      min-width: 72px;
      min-height: 40px;
      border: 1px solid rgba(135, 153, 170, .34);
      border-radius: 7px;
      background: #20252b;
      color: #e5e9ee;
      padding: 9px 13px;
      box-shadow: 0 12px 32px rgba(0, 0, 0, .35);
      font: 600 12px/1.1 ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }
    .ctox-chat-tray {
      pointer-events: none;
      display: flex;
      align-items: flex-end;
      gap: 10px;
      min-width: 0;
      max-width: 100%;
      overflow: hidden;
    }
    .ctox-chat-window {
      pointer-events: auto;
      display: grid;
      grid-template-rows: 42px minmax(0, 1fr) auto;
      width: min(310px, calc(100vw - 38px));
      height: 380px;
      min-width: 280px;
      overflow: hidden;
      border: 1px solid var(--line);
      border-radius: 7px 7px 0 0;
      background: var(--surface);
      color: var(--text);
      box-shadow: var(--shadow);
      font: 13px/1.35 ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }
    .ctox-chat-window.is-minimized {
      grid-template-rows: 42px;
      height: 42px;
    }
    .ctox-chat-window header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 8px;
      border-bottom: 1px solid var(--line);
      background: color-mix(in srgb, var(--surface) 88%, var(--surface-2));
      padding: 0 8px 0 10px;
    }
    .ctox-chat-window header > div {
      display: flex;
      align-items: center;
      gap: 4px;
    }
    .ctox-chat-window header button {
      border: 1px solid transparent;
      border-radius: 6px;
      background: transparent;
      color: var(--muted);
      cursor: pointer;
      min-width: 28px;
      min-height: 28px;
    }
    .ctox-chat-window header button:hover {
      border-color: var(--line);
      color: var(--text);
    }
    .ctox-chat-title {
      display: grid;
      min-width: 0;
      flex: 1;
      text-align: left;
      padding: 0;
    }
    .ctox-chat-title strong,
    .ctox-chat-title span {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .ctox-chat-title strong {
      color: var(--text);
      font-size: 13px;
      font-weight: 760;
    }
    .ctox-chat-title span {
      color: var(--muted);
      font-size: 11px;
    }
    .ctox-chat-messages {
      display: flex;
      flex-direction: column;
      gap: 8px;
      overflow: auto;
      padding: 10px;
      background: color-mix(in srgb, var(--bg) 72%, var(--surface));
    }
    .ctox-chat-empty {
      margin: auto;
      color: var(--muted);
      font-weight: 650;
    }
    .ctox-chat-message {
      max-width: 86%;
      border: 1px solid var(--line);
      border-radius: 8px;
      background: var(--surface);
      padding: 8px 9px;
    }
    .ctox-chat-message.is-user {
      align-self: flex-end;
      background: color-mix(in srgb, var(--accent) 13%, var(--surface));
      border-color: color-mix(in srgb, var(--accent) 44%, var(--line));
    }
    .ctox-chat-message.is-ctox {
      align-self: flex-start;
    }
    .ctox-chat-message p {
      margin: 0;
      white-space: pre-wrap;
    }
    .ctox-chat-message footer {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: 6px;
      margin-top: 6px;
      color: var(--muted);
      font-size: 11px;
    }
    .ctox-chat-track {
      border: 1px solid color-mix(in srgb, var(--accent) 44%, var(--line));
      border-radius: 999px;
      background: color-mix(in srgb, var(--accent) 10%, var(--surface));
      color: var(--accent);
      cursor: pointer;
      padding: 3px 7px;
      font-size: 11px;
      font-weight: 760;
    }
    .ctox-chat-form {
      display: grid;
      grid-template-columns: minmax(0, 1fr) auto;
      gap: 8px;
      border-top: 1px solid var(--line);
      padding: 8px;
      background: var(--surface);
    }
    .ctox-chat-form textarea {
      width: 100%;
      min-width: 0;
      resize: none;
      border: 1px solid var(--line);
      border-radius: 7px;
      background: color-mix(in srgb, var(--surface) 80%, var(--surface-2));
      color: var(--text);
      padding: 8px;
    }
    .ctox-chat-form textarea::placeholder {
      color: var(--muted);
      opacity: 0.72;
    }
    .ctox-chat-form button {
      align-self: end;
      border: 1px solid color-mix(in srgb, var(--accent) 44%, var(--line));
      border-radius: 7px;
      background: color-mix(in srgb, var(--accent) 14%, var(--surface));
      color: var(--accent);
      cursor: pointer;
      min-height: 34px;
      padding: 0 10px;
      font-weight: 760;
    }
    @media (max-width: 780px) {
      .ctox-chat-root {
        right: 18px;
        max-width: calc(100vw - 36px);
      }
      .ctox-chat-tray {
        flex: 1;
      }
      .ctox-chat-window {
        min-width: 0;
        width: 100%;
      }
    }
  `;
  document.head.append(style);
}

function escapeHtml(value) {
  return String(value ?? '').replace(/[&<>"']/g, (char) => ({
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#39;',
  }[char]));
}

function escapeAttr(value) {
  return escapeHtml(value).replace(/`/g, '&#96;');
}
