import { showBusinessConfirm } from './dialogs.js';

const CHAT_STYLE_ID = 'ctox-business-chat-style';
const CHAT_STATE_KEY = 'ctox.businessOs.chat.v1';
const CHAT_CHANNEL = 'business_os.llm.chat';
const CHAT_COLLECTION = 'business_chats';
const CHAT_OPEN_EVENT = 'ctox-business-os-chat-open';

export function initBusinessChat({
  session,
  commandBus,
  db,
  getActiveModule,
}) {
  if (!session?.authenticated || document.querySelector('[data-ctox-chat-root]')) return;
  installChatStyles();
  const state = readChatState(session);
  const root = document.createElement('div');
  root.className = 'ctox-chat-root';
  root.dataset.ctoxChatRoot = 'true';
  document.body.append(root);
  const handleRootClick = (event) => {
    const minimizeButton = event.target.closest?.('[data-chat-minimize]');
    if (minimizeButton && root.contains(minimizeButton)) {
      event.preventDefault();
      event.stopPropagation();
      collapseChatWindow({ root, state, commandBus, db, getActiveModule, target: minimizeButton }).catch((error) => {
        console.warn('[business-chat] chat minimize failed', error);
      });
      return;
    }
    const sendButton = event.target.closest?.('[data-chat-send]');
    if (sendButton && root.contains(sendButton)) {
      event.preventDefault();
      event.stopPropagation();
      const node = sendButton.closest('[data-chat-id]');
      const chat = state.chats.find((item) => item.id === node?.dataset.chatId);
      if (!node || !chat) return;
      submitChatForm({ root, state, chat, node, commandBus, db, getActiveModule }).catch((error) => {
        console.warn('[business-chat] chat send failed', error);
      });
      return;
    }
    const chatOpenButton = event.target.closest?.('[data-chat-open]');
    if (!chatOpenButton || !root.contains(chatOpenButton)) return;
    event.preventDefault();
    event.stopPropagation();
    toggleChatDock({ root, state, commandBus, db, getActiveModule }).catch((error) => {
      console.warn('[business-chat] chat dock toggle failed', error);
    });
  };

  const sync = () => {
    captureDrafts(root, state);
    syncTrackedMessages({ state, db }).then((changed) => {
      if (changed) persistChatState({ state, db });
      if (changed) renderChatRoot({ root, state, commandBus, db, getActiveModule });
    }).catch(() => {});
  };
  const syncChats = () => {
    captureDrafts(root, state);
    hydrateChatsFromRxDb({ state, db, session }).then((changed) => {
      if (changed) renderChatRoot({ root, state, commandBus, db, getActiveModule });
    }).catch(() => {});
  };
  const handleExternalSubmit = async (event) => {
    const detail = event.detail || {};
    const text = String(detail.text || detail.message || '').trim();
    if (!text) return;
    const chat = ensureChat(state, session);
    chat.open = true;
    chat.minimized = false;
    state.dockCollapsed = false;
    chat.draft = '';
    await submitChatMessage({
      state,
      chat,
      text,
      commandBus,
      getActiveModule,
      meta: detail,
      onPending: async () => {
        await persistChatState({ state, db });
        renderChatRoot({ root, state, commandBus, db, getActiveModule });
      },
    });
    await persistChatState({ state, db });
    renderChatRoot({ root, state, commandBus, db, getActiveModule });
    syncTrackedMessages({ state, db }).then((changed) => {
      if (changed) persistChatState({ state, db });
      if (changed) renderChatRoot({ root, state, commandBus, db, getActiveModule });
    }).catch(() => {});
  };
  const handleExternalOpen = async (event) => {
    const detail = event.detail || {};
    const chat = detail.reuseActive === true
      ? ensureChat(state, session)
      : createChat(state.ownerUserId);
    if (detail.reuseActive !== true) state.chats.push(chat);
    chat.title = String(detail.title || chat.title || 'CTOX').trim() || 'CTOX';
    chat.open = true;
    chat.minimized = false;
    chat.maximized = Boolean(detail.maximized);
    chat.draft = String(detail.draft || detail.message || '');
    chat.contextMeta = chatContextMetaFromDetail(detail);
    const contextText = String(detail.context_text || detail.contextText || '').trim();
    if (contextText && !chat.messages.some((message) => message.contextFor === chat.id)) {
      chat.messages.push({
        id: `context_${crypto.randomUUID()}`,
        role: 'ctox',
        text: contextText,
        contextFor: chat.id,
        detail: detail.context_label || detail.contextLabel || 'Kontext',
        createdAt: Date.now(),
      });
    }
    state.activeChatId = chat.id;
    state.dockCollapsed = false;
    state.preCollapseExpandedChatIds = [];
    touchChats(state, [chat]);
    await persistChatState({ state, db });
    renderChatRoot({ root, state, commandBus, db, getActiveModule });
  };

  hydrateChatsFromRxDb({ state, db, session })
    .then(() => renderChatRoot({ root, state, commandBus, db, getActiveModule }))
    .catch(() => renderChatRoot({ root, state, commandBus, db, getActiveModule }));
  root.addEventListener('click', handleRootClick, true);
  window.addEventListener('ctox-business-os-chat-submit', handleExternalSubmit);
  window.addEventListener(CHAT_OPEN_EVENT, handleExternalOpen);
  const businessChatsSub = db?.raw?.[CHAT_COLLECTION]?.$?.subscribe?.(syncChats) || null;
  const businessCommandsSub = db?.raw?.business_commands?.$?.subscribe?.(sync) || null;
  const queueTasksSub = db?.raw?.ctox_queue_tasks?.$?.subscribe?.(sync) || null;
  const timer = window.setInterval(sync, 4000);
  root.__ctoxChatCleanup = () => {
    root.removeEventListener('click', handleRootClick, true);
    window.removeEventListener('ctox-business-os-chat-submit', handleExternalSubmit);
    window.removeEventListener(CHAT_OPEN_EVENT, handleExternalOpen);
    businessChatsSub?.unsubscribe?.();
    businessCommandsSub?.unsubscribe?.();
    queueTasksSub?.unsubscribe?.();
    window.clearInterval(timer);
  };
}

function renderChatRoot({ root, state, commandBus, db, getActiveModule }) {
  const openChats = state.chats.filter((chat) => chat.open !== false);
  const activeChat = activeChatFor(state, openChats);
  const expandedChats = openChats.filter((chat) => !chat.minimized);
  const dockCollapsed = Boolean(state.dockCollapsed);
  root.classList.toggle('is-collapsed', dockCollapsed);
  root.innerHTML = `
    <section class="ctox-chat-dock ${dockCollapsed ? 'is-collapsed' : ''}" data-chat-dock>
      <button class="ctox-chat-fab" type="button" data-chat-open aria-label="Chat öffnen">
        <span>Chat</span><b>${openChats.length || ''}</b>
      </button>
      ${dockCollapsed ? '' : `
        <button class="ctox-chat-nav" type="button" data-chat-prev aria-label="Vorheriger Chat">‹</button>
        <div class="ctox-chat-strip" data-chat-strip aria-label="Offene Chats">
          ${openChats.map((chat) => chatDockItem(chat, activeChat?.id)).join('')}
        </div>
        <button class="ctox-chat-nav" type="button" data-chat-next aria-label="Nächster Chat">›</button>
        <button class="ctox-chat-new" type="button" data-chat-new aria-label="Neuer Chat">+</button>
      `}
    </section>
    <div class="ctox-chat-stage" data-chat-stage>
      <div class="ctox-chat-stage-inner">
        ${dockCollapsed ? '' : expandedChats.map((chat) => chatWindow(chat, activeChat?.id)).join('')}
      </div>
    </div>
  `;
  root.querySelector('[data-chat-new]')?.addEventListener('click', async () => {
    const next = createChat(state.ownerUserId);
    state.chats.push(next);
    state.activeChatId = next.id;
    state.dockCollapsed = false;
    touchChats(state, [next]);
    await persistChatState({ state, db });
    renderChatRoot({ root, state, commandBus, db, getActiveModule });
  });
  root.querySelector('[data-chat-prev]')?.addEventListener('click', async () => {
    const chat = focusAdjacentChat(state, -1);
    state.dockCollapsed = false;
    if (chat) touchChats(state, [chat]);
    await persistChatState({ state, db });
    renderChatRoot({ root, state, commandBus, db, getActiveModule });
  });
  root.querySelector('[data-chat-next]')?.addEventListener('click', async () => {
    const chat = focusAdjacentChat(state, 1);
    state.dockCollapsed = false;
    if (chat) touchChats(state, [chat]);
    await persistChatState({ state, db });
    renderChatRoot({ root, state, commandBus, db, getActiveModule });
  });
  root.querySelectorAll('[data-chat-focus]').forEach((button) => {
    button.addEventListener('click', async () => {
      const chat = state.chats.find((item) => item.id === button.dataset.chatFocus);
      if (!chat) return;
      toggleChatFromDock(state, chat);
      state.dockCollapsed = false;
      touchChats(state, [chat]);
      await persistChatState({ state, db });
      renderChatRoot({ root, state, commandBus, db, getActiveModule });
    });
  });
  root.querySelectorAll('[data-chat-id]').forEach((node) => {
    const chat = state.chats.find((item) => item.id === node.dataset.chatId);
    if (!chat) return;
    node.querySelectorAll('[data-chat-minimize]').forEach((button) => button.addEventListener('click', async () => {
      chat.minimized = true;
      touchChats(state, [chat]);
      await persistChatState({ state, db });
      renderChatRoot({ root, state, commandBus, db, getActiveModule });
    }));
    node.querySelectorAll('[data-chat-maximize]').forEach((button) => button.addEventListener('click', async () => {
      chat.maximized = !chat.maximized;
      chat.minimized = false;
      state.dockCollapsed = false;
      state.activeChatId = chat.id;
      touchChats(state, [chat]);
      await persistChatState({ state, db });
      renderChatRoot({ root, state, commandBus, db, getActiveModule });
    }));
    node.querySelector('[data-chat-delete]')?.addEventListener('click', async () => {
      const confirmed = await showBusinessConfirm('Diesen Chat wirklich löschen?', {
        title: 'Chat löschen',
        confirmLabel: 'Löschen',
      });
      if (!confirmed) return;
      await deleteChat({ state, chat, db });
      renderChatRoot({ root, state, commandBus, db, getActiveModule });
    });
    node.querySelector('[data-chat-new]')?.addEventListener('click', async () => {
      const next = createChat(state.ownerUserId);
      state.chats.push(next);
      state.activeChatId = next.id;
      state.dockCollapsed = false;
      touchChats(state, [next]);
      await persistChatState({ state, db });
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
    const form = node.querySelector('[data-chat-form]');
    const submitFromForm = async (event) => {
      event.preventDefault();
      event.stopPropagation();
      await submitChatForm({ root, state, chat, node, commandBus, db, getActiveModule });
    };
    form?.addEventListener('submit', submitFromForm);
    form?.querySelector('button[type="submit"]')?.addEventListener('click', submitFromForm);
  });
  scrollActiveChatIntoView(root, state);
}

async function submitChatForm({ root, state, chat, node, commandBus, db, getActiveModule }) {
  if (chat.__submitting) return;
  captureDrafts(root, state);
  const input = node.querySelector('[name="message"]');
  const text = String(input?.value || chat.draft || '').trim();
  if (!text) return;
  chat.__submitting = true;
  chat.draft = '';
  if (input) input.value = '';
  try {
    await submitChatMessage({
      state,
      chat,
      text,
      commandBus,
      getActiveModule,
      meta: chat.contextMeta || {},
      onPending: async () => {
        await persistChatState({ state, db });
        renderChatRoot({ root, state, commandBus, db, getActiveModule });
      },
    });
    await persistChatState({ state, db });
    renderChatRoot({ root, state, commandBus, db, getActiveModule });
    syncTrackedMessages({ state, db }).then((changed) => {
      if (changed) persistChatState({ state, db });
      if (changed) renderChatRoot({ root, state, commandBus, db, getActiveModule });
    }).catch(() => {});
  } finally {
    delete chat.__submitting;
  }
}

function captureDrafts(root, state) {
  root.querySelectorAll('[data-chat-id]').forEach((node) => {
    const chat = state.chats.find((item) => item.id === node.dataset.chatId);
    const input = node.querySelector('[name="message"]');
    if (chat && input) chat.draft = input.value;
  });
}

async function toggleChatDock({ root, state, commandBus, db, getActiveModule }) {
  captureDrafts(root, state);
  const openChats = state.chats.filter((chat) => chat.open !== false);
  if (!state.dockCollapsed) {
    state.preCollapseExpandedChatIds = openChats
      .filter((chat) => !chat.minimized)
      .map((chat) => chat.id);
    state.dockCollapsed = true;
    touchChats(state, openChats);
  } else {
    const restoreIds = Array.isArray(state.preCollapseExpandedChatIds)
      ? state.preCollapseExpandedChatIds
      : [];
    const changedChats = [];
    if (restoreIds.length) {
      const restoreSet = new Set(restoreIds);
      for (const chat of openChats) {
        const nextMinimized = !restoreSet.has(chat.id);
        if (chat.minimized !== nextMinimized) {
          chat.minimized = nextMinimized;
          changedChats.push(chat);
        }
      }
      state.activeChatId = restoreIds.find((id) => openChats.some((chat) => chat.id === id)) || state.activeChatId;
    } else if (!openChats.some((chat) => !chat.minimized)) {
      const chat = ensureChat(state);
      chat.open = true;
      chat.minimized = false;
      state.activeChatId = chat.id;
      changedChats.push(chat);
    }
    state.dockCollapsed = false;
    state.preCollapseExpandedChatIds = [];
    touchChats(state, changedChats.length ? changedChats : openChats);
  }
  renderChatRoot({ root, state, commandBus, db, getActiveModule });
  await persistChatState({ state, db });
}

function toggleChatFromDock(state, chat) {
  chat.open = true;
  if (!chat.minimized) {
    chat.minimized = true;
    const nextActive = state.chats.find((item) => item.open !== false && !item.minimized && item.id !== chat.id);
    if (nextActive) state.activeChatId = nextActive.id;
    return;
  }
  chat.minimized = false;
  state.activeChatId = chat.id;
}

async function collapseChatWindow({ root, state, commandBus, db, getActiveModule, target }) {
  const node = target.closest('[data-chat-id]');
  const chat = state.chats.find((item) => item.id === node?.dataset.chatId);
  if (!chat) return;
  captureDrafts(root, state);
  chat.minimized = true;
  touchChats(state, [chat]);
  renderChatRoot({ root, state, commandBus, db, getActiveModule });
  await persistChatState({ state, db });
}

function chatWindow(chat, activeId) {
  return `
    <section class="ctox-chat-window ${chat.maximized ? 'is-maximized' : ''} ${chat.id === activeId ? 'is-active' : ''}" data-chat-id="${escapeAttr(chat.id)}">
      <header>
        <button class="ctox-chat-title" type="button" data-chat-maximize>
          <strong>${escapeHtml(chat.title || 'CTOX')}</strong>
          ${chat.lastTrackingId ? `<span>${escapeHtml(chat.lastTrackingId)}</span>` : '<span>Business OS</span>'}
        </button>
        <div>
          <button type="button" data-chat-new aria-label="Neuer Chat">+</button>
          <button type="button" data-chat-maximize aria-label="Chat maximieren">${chat.maximized ? '↙' : '↗'}</button>
          <button type="button" data-chat-minimize aria-label="Chat einklappen">–</button>
          <button type="button" data-chat-delete aria-label="Chat löschen">Löschen</button>
        </div>
      </header>
      <div class="ctox-chat-messages">
        ${chat.messages.length ? chat.messages.map(messageMarkup).join('') : '<div class="ctox-chat-empty">CTOX Aufgabe eingeben.</div>'}
      </div>
      <form class="ctox-chat-form" data-chat-form>
        <textarea name="message" rows="2" placeholder="Aufgabe an CTOX..." required>${escapeHtml(chat.draft || '')}</textarea>
        <button type="submit" data-chat-send>Senden</button>
      </form>
    </section>
  `;
}

function chatDockItem(chat, activeId) {
  const count = Array.isArray(chat.messages) ? chat.messages.length : 0;
  const status = chat.lastTrackingId ? 'Queue' : count ? `${count} Msg` : 'Leer';
  return `
    <button class="ctox-chat-chip ${chat.id === activeId ? 'is-active' : ''} ${chat.minimized ? 'is-minimized' : ''}" type="button" data-chat-focus="${escapeAttr(chat.id)}">
      <span class="ctox-chat-chip-mark" aria-hidden="true">${chat.minimized ? '–' : '●'}</span>
      <span class="ctox-chat-chip-copy">
        <strong>${escapeHtml(chat.title || 'CTOX')}</strong>
        <small>${escapeHtml(status)}</small>
      </span>
    </button>
  `;
}

function activeChatFor(state, openChats = state.chats.filter((chat) => chat.open !== false)) {
  if (!openChats.length) return null;
  let active = openChats.find((chat) => chat.id === state.activeChatId);
  if (!active) {
    active = openChats.find((chat) => !chat.minimized) || openChats[openChats.length - 1];
    state.activeChatId = active.id;
  }
  return active;
}

function nextOpenChatId(state, currentId) {
  const open = state.chats.filter((chat) => chat.open !== false && chat.id !== currentId);
  return open.at(-1)?.id || '';
}

function focusAdjacentChat(state, direction) {
  const open = state.chats.filter((chat) => chat.open !== false);
  if (!open.length) return null;
  const index = open.findIndex((chat) => chat.id === state.activeChatId);
  const current = index >= 0 ? index : 0;
  const next = open[(current + direction + open.length) % open.length];
  next.minimized = false;
  state.activeChatId = next.id;
  return next;
}

function touchChats(state, chats) {
  const now = Date.now();
  state.lastUiMutationMs = now;
  chats.forEach((chat) => {
    if (!chat) return;
    chat.owner_user_id = chat.owner_user_id || state.ownerUserId || '';
    chat.updated_at_ms = now;
  });
}

function scrollActiveChatIntoView(root, state) {
  window.requestAnimationFrame(() => {
    const activeChip = Array.from(root.querySelectorAll('[data-chat-focus]'))
      .find((node) => node.dataset.chatFocus === state.activeChatId);
    const activeWindow = Array.from(root.querySelectorAll('[data-chat-id]'))
      .find((node) => node.dataset.chatId === state.activeChatId);
    activeChip?.scrollIntoView?.({ inline: 'center', block: 'nearest' });
    activeWindow?.scrollIntoView?.({ inline: 'center', block: 'nearest' });
  });
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

async function submitChatMessage({ state, chat, text, commandBus, getActiveModule, meta = {}, onPending = null }) {
  const activeModule = getActiveModule?.() || { id: 'ctox', title: 'CTOX' };
  const sourceModule = meta.module || meta.source_module || activeModule.id || 'ctox';
  const sourceTitle = meta.source_title || activeModule.title || sourceModule || 'CTOX';
  const commandType = meta.command_type || meta.commandType || 'business_os.chat.task';
  const extraPayload = meta.payload && typeof meta.payload === 'object' ? meta.payload : {};
  const extraClientContext = meta.client_context && typeof meta.client_context === 'object' ? meta.client_context : {};
  const now = Date.now();
  const commandId = meta.command_id || meta.commandId || `cmd_${crypto.randomUUID()}`;
  const messageId = `chatmsg_${crypto.randomUUID()}`;
  chat.messages.push({
    id: messageId,
    role: 'user',
    text,
    createdAt: now,
  });
  chat.title = chat.title === 'CTOX' ? titleFromText(text) : chat.title;
  const pendingMessage = {
    id: `status_${commandId}`,
    role: 'ctox',
    text: 'Command wird an CTOX übergeben.',
    commandId,
    taskId: '',
    status: 'pending_sync',
    createdAt: Date.now(),
  };
  chat.messages.push(pendingMessage);
  chat.lastTrackingId = commandId;
  touchChats(state, [chat]);
  if (typeof onPending === 'function') {
    await onPending();
  }
  const command = {
    id: commandId,
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
    const acceptedCommandId = result.command_id || commandId;
    chat.lastTrackingId = taskId || acceptedCommandId;
    pendingMessage.text = taskId
      ? 'Task angelegt und in der CTOX Queue. Antwort erscheint hier, sobald der CTOX Service ihn verarbeitet.'
      : 'Command angelegt. Keine CTOX Queue-ID erhalten.';
    pendingMessage.commandId = acceptedCommandId;
    pendingMessage.taskId = taskId;
    pendingMessage.status = result.task_status || result.status || 'queued';
    pendingMessage.createdAt = Date.now();
  } catch (error) {
    const failedCommandId = error?.command_id || error?.commandId || commandId;
    pendingMessage.text = error?.message || String(error);
    pendingMessage.commandId = failedCommandId;
    pendingMessage.taskId = '';
    pendingMessage.status = error?.status || 'failed';
    pendingMessage.createdAt = Date.now();
    if (failedCommandId) chat.lastTrackingId = failedCommandId;
  }
  touchChats(state, [chat]);
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
  return ['failed', 'blocked', 'stale_missing_native'].includes(String(status || '').toLowerCase());
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

function ensureChat(state, session = null) {
  let chat = state.chats.find((item) => item.id === state.activeChatId)
    || state.chats.find((item) => item.open !== false)
    || state.chats[0];
  if (!chat) {
    chat = createChat(ownerUserId(session) || state.ownerUserId);
    state.chats.push(chat);
  }
  chat.open = true;
  state.activeChatId = chat.id;
  return chat;
}

function createChat(owner = '') {
  return {
    id: `chat_${crypto.randomUUID()}`,
    title: 'CTOX',
    open: true,
    minimized: false,
    maximized: false,
    owner_user_id: owner || '',
    messages: [],
    draft: '',
    contextMeta: {},
    createdAt: Date.now(),
    updated_at_ms: Date.now(),
  };
}

function chatContextMetaFromDetail(detail = {}) {
  const payload = detail.payload && typeof detail.payload === 'object' ? detail.payload : {};
  const clientContext = detail.client_context && typeof detail.client_context === 'object'
    ? detail.client_context
    : {};
  const meta = {
    module: detail.module || detail.source_module || '',
    source_module: detail.source_module || detail.module || '',
    source_title: detail.source_title || detail.sourceTitle || '',
    record_id: detail.record_id || detail.recordId || '',
    title: detail.command_title || detail.commandTitle || detail.title || '',
    instruction: detail.instruction || '',
    inbound_channel: detail.inbound_channel || detail.inboundChannel || '',
    command_type: detail.command_type || detail.commandType || '',
    payload,
    client_context: clientContext,
  };
  return Object.fromEntries(
    Object.entries(meta).filter(([, value]) => {
      if (value == null) return false;
      if (typeof value === 'string') return value.trim() !== '';
      if (typeof value === 'object') return Object.keys(value).length > 0;
      return true;
    })
  );
}

function readChatState(session) {
  const owner = ownerUserId(session);
  try {
    const parsed = JSON.parse(localStorage.getItem(CHAT_STATE_KEY) || '{}') || {};
    const chats = Array.isArray(parsed.chats) ? parsed.chats : [];
    return {
      ownerUserId: owner,
      activeChatId: parsed.activeChatId || '',
      dockCollapsed: Boolean(parsed.dockCollapsed),
      preCollapseExpandedChatIds: Array.isArray(parsed.preCollapseExpandedChatIds)
        ? parsed.preCollapseExpandedChatIds.filter(Boolean)
        : [],
      chats: chats
        .filter((chat) => !chat.owner_user_id || chat.owner_user_id === owner)
        .map((chat) => ({
        id: chat.id || `chat_${crypto.randomUUID()}`,
        title: chat.title || 'CTOX',
        open: chat.open !== false,
        minimized: Boolean(chat.minimized),
        maximized: Boolean(chat.maximized),
        owner_user_id: chat.owner_user_id || owner,
        lastTrackingId: chat.lastTrackingId || '',
        messages: Array.isArray(chat.messages) ? chat.messages.slice(-40) : [],
        draft: chat.draft || '',
        contextMeta: chat.contextMeta && typeof chat.contextMeta === 'object' ? chat.contextMeta : {},
        createdAt: chat.createdAt || Date.now(),
        updated_at_ms: chat.updated_at_ms || Date.now(),
      })),
    };
  } catch {
    return { ownerUserId: owner, chats: [] };
  }
}

function writeChatState(state) {
  localStorage.setItem(CHAT_STATE_KEY, JSON.stringify({
    activeChatId: state.activeChatId || '',
    dockCollapsed: Boolean(state.dockCollapsed),
    preCollapseExpandedChatIds: Array.isArray(state.preCollapseExpandedChatIds)
      ? state.preCollapseExpandedChatIds.filter(Boolean)
      : [],
    chats: state.chats.filter((chat) => isOwnedChat(chat, state.ownerUserId)).map((chat) => ({
      ...chat,
      messages: chat.messages.slice(-40),
      draft: chat.draft || '',
      contextMeta: chat.contextMeta && typeof chat.contextMeta === 'object' ? chat.contextMeta : {},
      owner_user_id: chat.owner_user_id || state.ownerUserId || '',
      updated_at_ms: chat.updated_at_ms || Date.now(),
    })),
  }));
}

async function persistChatState({ state, db }) {
  const now = Date.now();
  const ownedChats = state.chats.filter((item) => isOwnedChat(item, state.ownerUserId));
  for (const chat of ownedChats) {
    chat.owner_user_id = chat.owner_user_id || state.ownerUserId || '';
    chat.updated_at_ms = now;
  }
  writeChatState(state);
  const collection = db?.raw?.[CHAT_COLLECTION];
  if (!collection) return;
  for (const chat of ownedChats) {
    const doc = {
      ...chat,
      messages: Array.isArray(chat.messages) ? chat.messages.slice(-40) : [],
      draft: chat.draft || '',
      contextMeta: chat.contextMeta && typeof chat.contextMeta === 'object' ? chat.contextMeta : {},
      updated_at_ms: chat.updated_at_ms,
    };
    const existing = await collection.findOne(chat.id).exec();
    if (existing) await existing.incrementalPatch(doc);
    else await collection.insert(doc);
  }
}

async function hydrateChatsFromRxDb({ state, db, session }) {
  const collection = db?.raw?.[CHAT_COLLECTION];
  if (!collection) return false;
  const owner = ownerUserId(session) || state.ownerUserId || '';
  state.ownerUserId = owner;
  const docs = await collection.find().exec();
  const remoteChats = docs
    .map((doc) => doc.toJSON())
    .filter((chat) => isOwnedChat(chat, owner))
    .map(normalizeChat)
    .sort((a, b) => (a.createdAt || 0) - (b.createdAt || 0));
  if (!remoteChats.length) {
    if (state.chats.length) await persistChatState({ state, db });
    return false;
  }
  const merged = mergeChats(state.chats, remoteChats, owner);
  const changed = JSON.stringify(stripDraftsForCompare(state.chats)) !== JSON.stringify(stripDraftsForCompare(merged));
  state.chats = merged;
  writeChatState(state);
  return changed;
}

async function deleteChat({ state, chat, db }) {
  state.chats = state.chats.filter((item) => item.id !== chat.id);
  if (state.activeChatId === chat.id) state.activeChatId = nextOpenChatId(state, chat.id);
  writeChatState(state);
  const collection = db?.raw?.[CHAT_COLLECTION];
  if (!collection) return;
  const existing = await collection.findOne(chat.id).exec();
  if (existing) {
    await existing.remove();
  } else {
    await collection.insert({
      ...normalizeChat(chat),
      owner_user_id: chat.owner_user_id || state.ownerUserId || '',
      _deleted: true,
      updated_at_ms: Date.now(),
    }).catch(() => {});
  }
}

function mergeChats(localChats, remoteChats, owner) {
  const byId = new Map();
  for (const chat of [...remoteChats, ...localChats]) {
    const normalized = normalizeChat({ ...chat, owner_user_id: chat.owner_user_id || owner });
    if (!isOwnedChat(normalized, owner)) continue;
    const previous = byId.get(normalized.id);
    if (!previous || (normalized.updated_at_ms || 0) >= (previous.updated_at_ms || 0)) {
      byId.set(normalized.id, normalized);
    }
  }
  return Array.from(byId.values())
    .sort((a, b) => (a.createdAt || 0) - (b.createdAt || 0));
}

function normalizeChat(chat) {
  return {
    id: chat.id || `chat_${crypto.randomUUID()}`,
    title: chat.title || 'CTOX',
    open: chat.open !== false,
    minimized: Boolean(chat.minimized),
    maximized: Boolean(chat.maximized),
    owner_user_id: chat.owner_user_id || '',
    lastTrackingId: chat.lastTrackingId || '',
    messages: Array.isArray(chat.messages) ? chat.messages.slice(-40) : [],
    draft: chat.draft || '',
    contextMeta: chat.contextMeta && typeof chat.contextMeta === 'object' ? chat.contextMeta : {},
    createdAt: chat.createdAt || Date.now(),
    updated_at_ms: chat.updated_at_ms || Date.now(),
  };
}

function stripDraftsForCompare(chats) {
  return chats.map((chat) => ({ ...chat, draft: '' }));
}

function ownerUserId(session) {
  return String(session?.user?.id || 'local-dev').trim() || 'local-dev';
}

function isOwnedChat(chat, owner) {
  return !owner || !chat?.owner_user_id || chat.owner_user_id === owner;
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
	      right: 96px;
	      bottom: 18px;
	      z-index: 60;
	      display: grid;
	      grid-template-rows: auto auto;
	      gap: 8px;
	      width: auto;
	      max-width: calc(100vw - 132px);
	      pointer-events: none;
	    }
    .ctox-chat-root button,
    .ctox-chat-root textarea {
      font: inherit;
    }
    .ctox-chat-dock {
      pointer-events: auto;
      grid-row: 2;
      display: grid;
      grid-template-columns: 88px 28px minmax(0, 1fr) 28px 34px;
      align-items: center;
	      gap: 6px;
	      min-width: 0;
	      width: 100%;
	      padding: 5px;
	      border: 1px solid var(--hairline, var(--line));
	      border-radius: 12px;
	      background: color-mix(in srgb, var(--surface) 92%, var(--bg));
	      box-shadow: 0 14px 34px rgba(0, 0, 0, .26);
	    }
    .ctox-chat-root.is-collapsed {
      right: auto;
      width: auto;
      max-width: none;
    }
    .ctox-chat-dock.is-collapsed {
      grid-template-columns: 88px;
      width: auto;
    }
    .ctox-chat-root.is-collapsed .ctox-chat-stage {
      display: none;
    }
    .ctox-chat-fab {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      height: 34px;
      width: 88px;
      min-width: 82px;
      border: 1px solid color-mix(in srgb, var(--accent) 24%, var(--line));
      border-radius: 10px;
      background: color-mix(in srgb, var(--accent) 10%, var(--surface));
      color: var(--text);
      padding: 0 10px;
      font-weight: 760;
    }
    .ctox-chat-fab b {
      display: grid;
      place-items: center;
      min-width: 18px;
      height: 18px;
      border-radius: 999px;
      background: color-mix(in srgb, var(--accent) 18%, transparent);
      color: var(--accent);
      font-size: 10px;
    }
    .ctox-chat-nav,
    .ctox-chat-new {
      height: 30px;
      border: 1px solid var(--hairline, var(--line));
      border-radius: 9px;
      background: color-mix(in srgb, var(--surface) 78%, var(--surface-2));
      color: var(--muted);
      font-weight: 820;
    }
    .ctox-chat-new {
      width: 34px;
      color: var(--accent);
    }
    .ctox-chat-strip {
      display: flex;
      align-items: center;
      gap: 6px;
      min-width: 0;
      overflow-x: auto;
      overscroll-behavior-x: contain;
      scroll-snap-type: x proximity;
      scrollbar-width: none;
    }
    .ctox-chat-strip::-webkit-scrollbar {
      display: none;
    }
	    .ctox-chat-chip {
	      scroll-snap-align: start;
	      flex: 0 0 136px;
      display: grid;
      grid-template-columns: auto minmax(0, 1fr);
      align-items: center;
      gap: 8px;
      height: 34px;
      min-width: 0;
      border: 1px solid transparent;
      border-radius: 10px;
      background: transparent;
      color: var(--muted);
      padding: 0 9px;
      text-align: left;
    }
    .ctox-chat-chip.is-active {
      border-color: color-mix(in srgb, var(--accent) 30%, transparent);
      background: color-mix(in srgb, var(--accent) 12%, var(--surface-2));
      color: var(--text);
    }
    .ctox-chat-chip-mark {
      display: grid;
      place-items: center;
      width: 18px;
      height: 18px;
      border-radius: 999px;
      background: color-mix(in srgb, var(--surface-2) 82%, transparent);
      color: var(--accent);
      font-size: 9px;
    }
    .ctox-chat-chip.is-minimized .ctox-chat-chip-mark {
      color: var(--muted);
    }
    .ctox-chat-chip-copy {
      display: grid;
      gap: 1px;
      min-width: 0;
    }
    .ctox-chat-chip-copy strong,
    .ctox-chat-chip-copy small {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .ctox-chat-chip-copy strong {
      color: inherit;
      font-size: 11px;
      font-weight: 760;
    }
    .ctox-chat-chip-copy small {
      color: var(--muted);
      font-size: 10px;
      font-weight: 680;
    }
	    .ctox-chat-stage {
	      pointer-events: none;
	      grid-row: 1;
	      display: grid;
	      grid-template-columns: 88px 28px minmax(0, 1fr) 28px 34px;
	      align-items: end;
	      gap: 6px;
	      box-sizing: border-box;
	      min-width: 0;
	      overflow: visible;
	      padding: 0 5px;
	    }
	    .ctox-chat-stage-inner {
	      grid-column: 3;
	      display: flex;
	      align-items: flex-end;
	      gap: 8px;
	      min-width: 0;
	      overflow-x: auto;
	      overscroll-behavior-x: contain;
	      scroll-snap-type: x proximity;
	      scrollbar-width: none;
	    }
	    .ctox-chat-stage::-webkit-scrollbar {
	      display: none;
	    }
	    .ctox-chat-stage-inner::-webkit-scrollbar {
	      display: none;
	    }
	    .ctox-chat-window {
	      pointer-events: auto;
	      scroll-snap-align: end;
	      flex: 0 0 256px;
	      display: grid;
	      grid-template-rows: 34px minmax(0, 1fr) auto;
	      width: 256px;
	      height: min(286px, calc(100vh - 132px));
	      min-width: 256px;
	      overflow: hidden;
	      border: 1px solid var(--hairline, var(--line));
	      border-radius: 10px;
	      background: color-mix(in srgb, var(--surface) 96%, var(--bg));
	      color: var(--text);
	      box-shadow: 0 18px 42px rgba(0, 0, 0, .34);
	      font: 12px/1.32 ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
	    }
	    .ctox-chat-window.is-active {
	      border-color: color-mix(in srgb, var(--accent) 36%, var(--line));
	    }
	    .ctox-chat-window.is-maximized {
	      flex-basis: 380px;
	      width: 380px;
	      height: min(430px, calc(100vh - 132px));
	    }
    .ctox-chat-window header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 8px;
      border-bottom: 1px solid var(--hairline, var(--line));
      background: color-mix(in srgb, var(--surface) 88%, var(--surface-2));
	      padding: 0 6px 0 10px;
	    }
	    .ctox-chat-window header > div {
	      display: flex;
	      align-items: center;
	      gap: 3px;
	    }
	    .ctox-chat-window header button {
	      display: grid;
	      place-items: center;
	      border: 1px solid transparent;
	      border-radius: 6px;
	      background: transparent;
	      color: var(--muted);
	      cursor: pointer;
	      width: 28px;
	      min-width: 28px;
	      height: 28px;
	      min-height: 28px;
	      line-height: 1;
		    }
	    .ctox-chat-window header button[data-chat-delete] {
	      width: auto;
	      min-width: 56px;
	      padding: 0 7px;
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
	      font-size: 12px;
	      font-weight: 760;
	    }
	    .ctox-chat-title span {
	      color: var(--muted);
	      font-size: 10px;
	    }
	    .ctox-chat-messages {
	      display: flex;
	      flex-direction: column;
	      gap: 6px;
	      overflow: auto;
	      padding: 9px;
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
	      padding: 7px 8px;
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
	      gap: 6px;
	      border-top: 1px solid var(--hairline, var(--line));
	      padding: 6px;
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
	      min-height: 36px;
	      padding: 6px 7px;
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
	      min-height: 32px;
	      padding: 0 9px;
	      font-weight: 760;
	    }
	    @media (max-width: 780px) {
	      .ctox-chat-root {
	        right: 18px;
        width: auto;
        max-width: calc(100vw - 36px);
      }
      .ctox-chat-dock {
        grid-template-columns: 88px 28px minmax(120px, 1fr) 28px 34px;
      }
      .ctox-chat-stage {
        grid-template-columns: 88px 28px minmax(120px, 1fr) 28px 34px;
      }
	      .ctox-chat-window {
	        min-width: 0;
	        flex-basis: 78vw;
	        width: 78vw;
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
