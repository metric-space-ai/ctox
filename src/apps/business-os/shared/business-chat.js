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
    const datePickerTrigger = event.target.closest?.('.ctox-date-picker-trigger');
    if (datePickerTrigger && root.contains(datePickerTrigger)) {
      if (event.target.tagName !== 'INPUT') {
        const picker = datePickerTrigger.querySelector('[data-chat-date-picker]');
        if (picker) {
          event.preventDefault();
          event.stopPropagation();
          try {
            picker.showPicker();
          } catch (error) {
            picker.click();
          }
        }
      }
      return;
    }
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
    const createNewChat = shouldCreateChatForExternalSubmit(detail);
    const chat = createNewChat ? createChat(state.ownerUserId, state.selectedDate) : ensureChat(state, session);
    if (createNewChat) state.chats.push(chat);
    if (detail.title) chat.title = String(detail.title).trim() || chat.title;
    chat.contextMeta = chatContextMetaFromDetail(detail);
    expandChatOnly(state, chat);
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
      : createChat(state.ownerUserId, state.selectedDate);
    if (detail.reuseActive !== true) state.chats.push(chat);
    chat.title = String(detail.title || chat.title || 'CTOX').trim() || 'CTOX';
    expandChatOnly(state, chat);
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

  let scrollTimeout = null;
  const handleScroll = (event) => {
    const strip = root.querySelector('[data-chat-strip]');
    const stageInner = root.querySelector('.ctox-chat-stage-inner');
    if (strip && stageInner && event.target.closest('[data-chat-strip]')) {
      root.classList.add('is-scrolling');
      alignChatWindows(root);
      
      if (scrollTimeout) clearTimeout(scrollTimeout);
      scrollTimeout = setTimeout(() => {
        root.classList.remove('is-scrolling');
      }, 150);
    }
  };

  const handleWheel = (event) => {
    const strip = event.target.closest('[data-chat-strip]');
    const dock = event.target.closest('[data-chat-dock]');
    const scrollableMessages = event.target.closest('.ctox-chat-messages');

    if ((strip || dock) && !scrollableMessages) {
      const targetStrip = strip || dock.querySelector('[data-chat-strip]');
      if (!targetStrip) return;

      // Redirect vertical scrolls (deltaY) to horizontal scrolls if vertical scroll is dominant.
      // Leave horizontal swipes (deltaX) to native touchpad physics.
      if (Math.abs(event.deltaY) > Math.abs(event.deltaX) && event.deltaY !== 0) {
        event.preventDefault();
        root.classList.add('is-scrolling');
        targetStrip.scrollLeft += event.deltaY;
        alignChatWindows(root);

        if (scrollTimeout) clearTimeout(scrollTimeout);
        scrollTimeout = setTimeout(() => {
          root.classList.remove('is-scrolling');
        }, 150);
      }
    }
  };

  let isDragging = false;
  let startX = 0;
  let scrollLeft = 0;
  let dragMoved = false;
  let dragStrip = null;

  const handleMouseDown = (e) => {
    // Avoid starting drag-scroll when interacting with buttons, inputs, date navigators, or chips!
    if (e.target.closest('button, input, textarea, select, a, svg, path')) return;
    const strip = e.target.closest('[data-chat-strip]');
    const dock = e.target.closest('[data-chat-dock]');
    const targetStrip = strip || (dock ? dock.querySelector('[data-chat-strip]') : null);
    if (!targetStrip) return;

    isDragging = true;
    dragMoved = false;
    dragStrip = targetStrip;
    startX = e.pageX;
    scrollLeft = targetStrip.scrollLeft;
    root.classList.add('is-scrolling');
  };

  const handleMouseMove = (e) => {
    if (!isDragging || !dragStrip) return;
    const walk = (e.pageX - startX) * 1.5;
    if (Math.abs(walk) > 4) {
      dragMoved = true;
      e.preventDefault();
      root.classList.add('is-scrolling');
      dragStrip.scrollLeft = scrollLeft - walk;
      alignChatWindows(root);
    }
  };

  const handleMouseUp = () => {
    if (isDragging) {
      isDragging = false;
      dragStrip = null;
      root.classList.remove('is-scrolling');
    }
  };

  const handleCaptureClick = (e) => {
    if (dragMoved && e.target.closest('[data-chat-strip]')) {
      e.preventDefault();
      e.stopPropagation();
      dragMoved = false;
    }
  };

  const handleResize = () => {
    alignChatWindows(root);
  };

  root.addEventListener('click', handleRootClick, true);
  window.addEventListener('ctox-business-os-chat-submit', handleExternalSubmit);
  window.addEventListener(CHAT_OPEN_EVENT, handleExternalOpen);
  root.addEventListener('scroll', handleScroll, true);
  window.addEventListener('resize', handleResize);
  root.addEventListener('wheel', handleWheel, { passive: false });
  root.addEventListener('mousedown', handleMouseDown);
  root.addEventListener('mousemove', handleMouseMove);
  window.addEventListener('mouseup', handleMouseUp);
  root.addEventListener('click', handleCaptureClick, true);

  const businessChatsSub = db?.raw?.[CHAT_COLLECTION]?.$?.subscribe?.(syncChats) || null;
  const businessCommandsSub = db?.raw?.business_commands?.$?.subscribe?.(sync) || null;
  const queueTasksSub = db?.raw?.ctox_queue_tasks?.$?.subscribe?.(sync) || null;
  const timer = window.setInterval(sync, 4000);

  root.__ctoxChatCleanup = () => {
    root.removeEventListener('click', handleRootClick, true);
    window.removeEventListener('ctox-business-os-chat-submit', handleExternalSubmit);
    window.removeEventListener(CHAT_OPEN_EVENT, handleExternalOpen);
    root.removeEventListener('scroll', handleScroll, true);
    window.removeEventListener('resize', handleResize);
    root.removeEventListener('wheel', handleWheel, { passive: false });
    root.removeEventListener('mousedown', handleMouseDown);
    root.removeEventListener('mousemove', handleMouseMove);
    window.removeEventListener('mouseup', handleMouseUp);
    root.removeEventListener('click', handleCaptureClick, true);
    businessChatsSub?.unsubscribe?.();
    businessCommandsSub?.unsubscribe?.();
    queueTasksSub?.unsubscribe?.();
    window.clearInterval(timer);
  };
}

function shouldCreateChatForExternalSubmit(detail = {}) {
  if (detail.reuseActive === true) return false;
  if (detail.reuseActive === false) return true;
  const action = detail.action || detail.client_context?.action || detail.clientContext?.action || '';
  return action === 'context-chat';
}

function alignChatWindows(root) {
  if (!root) return;
  const strip = root.querySelector('[data-chat-strip]');
  const stage = root.querySelector('[data-chat-stage]');
  const stageInner = root.querySelector('.ctox-chat-stage-inner');
  if (!strip || !stage || !stageInner) return;

  const windows = stageInner.querySelectorAll('.ctox-chat-window');
  const isNarrow = window.innerWidth <= 780;

  if (isNarrow) {
    windows.forEach((win) => {
      win.style.position = '';
      win.style.left = '';
    });
    return;
  }

  const scrollLeft = strip.scrollLeft || 0;
  const rootRect = stageInner.getBoundingClientRect();

  windows.forEach((win) => {
    const chatId = win.dataset.chatId;
    const chip = strip.querySelector(`[data-chat-focus="${chatId}"]`);
    if (chip) {
      const winWidth = win.classList.contains('is-maximized') ? 390 : 264;
      const chipCenter = chip.offsetLeft + chip.offsetWidth / 2;
      let targetLeft = chipCenter - winWidth / 2 - scrollLeft;
      
      // Clamp targetLeft so the window stays strictly within stageInner column bounds with 8px safe margins
      const minLeft = 8;
      const maxLeft = rootRect.width - 8 - winWidth;
      targetLeft = Math.max(minLeft, Math.min(maxLeft, targetLeft));
      
      win.style.position = 'absolute';
      win.style.left = `${targetLeft}px`;
    }
  });

  const spacer = stageInner.querySelector('.ctox-chat-stage-spacer');
  if (spacer) {
    spacer.style.position = 'absolute';
    spacer.style.width = '1px';
  }
}

function renderChatRoot({ root, state, commandBus, db, getActiveModule }) {
  initSchedulerLoop({ root, state, commandBus, db, getActiveModule });

  const selectedDate = state.selectedDate || getLocalDateString(Date.now());
  const chatsOfSelectedDate = state.chats.filter((chat) => getLocalDateString(chat.createdAt) === selectedDate);
  const openChats = chatsOfSelectedDate.filter((chat) => chat.open !== false);
  const hasMaximized = openChats.some(chat => chat.maximized && !chat.minimized);
  const activeChat = activeChatFor(state, openChats);
  const dockCollapsed = Boolean(state.dockCollapsed);
  const wasCollapsed = root.classList.contains('is-collapsed');
  root.classList.toggle('is-collapsed', dockCollapsed);

  // --- SMART IN-PLACE DOM UPDATE FAST-PATH ---
  const datePickerEl = root.querySelector('[data-chat-date-picker]');
  const matchesCurrentDate = datePickerEl && datePickerEl.value === selectedDate;
  const existingWindows = Array.from(root.querySelectorAll('.ctox-chat-window'));
  const currentWindowIds = existingWindows.map(w => w.dataset.chatId);
  const openChatIds = openChats.map(c => c.id);
  const canUpdateInPlace = existingWindows.length === openChats.length &&
                           currentWindowIds.every((id, idx) => id === openChatIds[idx]) &&
                           root.querySelector('[data-chat-dock]') &&
                           wasCollapsed === dockCollapsed &&
                           matchesCurrentDate;

  if (canUpdateInPlace) {
    // 1. Update dock state / collapse class
    const dockEl = root.querySelector('[data-chat-dock]');
    if (dockEl) {
      dockEl.className = `ctox-chat-dock ${dockCollapsed ? 'is-collapsed' : ''}`;
    }
    
    // Update Chat count badge in FAB
    const fabBadge = root.querySelector('.ctox-chat-fab b');
    if (fabBadge) {
      fabBadge.textContent = openChats.length || '';
    }

    // 2. Update active states and details on chips in the dock
    const chips = root.querySelectorAll('.ctox-chat-chip');
    chips.forEach(chip => {
      const chatId = chip.dataset.chatFocus;
      const chat = openChats.find(c => c.id === chatId);
      if (chat) {
        const taskState = getTaskState(chat);
        const count = Array.isArray(chat.messages) ? chat.messages.length : 0;
        const status = chat.lastTrackingId ? (taskState.toUpperCase()) : count ? `${count} Msg` : 'Leer';

        chip.className = `ctox-chat-chip ${chat.id === activeChat?.id && !chat.minimized ? 'is-active' : ''} ${chat.minimized ? 'is-minimized' : ''} ${!chat.minimized ? 'is-expanded' : ''}`;
        
        const smallEl = chip.querySelector('.ctox-chat-chip-copy small');
        if (smallEl) smallEl.textContent = status;
        
        const strongEl = chip.querySelector('.ctox-chat-chip-copy strong');
        if (strongEl) strongEl.textContent = chat.title || 'CTOX';
      }
    });

    // 3. Update active states, 3D relation tags, maximized and minimized classes on windows
    const activeIndex = openChats.findIndex((c) => c.id === activeChat?.id);
    existingWindows.forEach((win, idx) => {
      const chat = openChats[idx];
      const relation = idx < activeIndex ? 'left' : idx > activeIndex ? 'right' : 'center';
      const taskState = getTaskState(chat);

      win.className = `ctox-chat-window ${chat.maximized ? 'is-maximized' : ''} ${chat.id === activeChat?.id ? 'is-active' : ''} ${chat.minimized ? 'is-minimized' : ''} is-task-${taskState}`;
      win.dataset.chatRel = relation;

      // Update title text in header
      const titleStrong = win.querySelector('.ctox-chat-title strong');
      if (titleStrong) titleStrong.textContent = chat.title || 'CTOX';

      // Update maximize icon in window header
      const maxBtn = win.querySelector('[data-chat-maximize]');
      if (maxBtn) {
        maxBtn.innerHTML = chat.maximized 
          ? `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 14 10 14 10 20"></polyline><polyline points="20 10 14 10 14 4"></polyline><line x1="14" y1="10" x2="21" y2="3"></line><line x1="10" y1="14" x2="3" y2="21"></line></svg>` 
          : `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 3 21 3 21 9"></polyline><polyline points="9 21 3 21 3 15"></polyline><line x1="21" y1="3" x2="14" y2="10"></line><line x1="3" y1="21" x2="10" y2="14"></line></svg>`;
      }

      // Update messages container content if it changed
      const messagesContainer = win.querySelector('.ctox-chat-messages');
      if (messagesContainer) {
        const expectedHtml = (chat.messages.length ? chat.messages.map(messageMarkup).join('') : '<div class="ctox-chat-empty">CTOX Aufgabe eingeben.</div>').trim();
        if (messagesContainer.innerHTML.trim() !== expectedHtml) {
          messagesContainer.innerHTML = expectedHtml;
          messagesContainer.scrollTop = messagesContainer.scrollHeight;
        }
      }

      // Update textarea content or placeholder if needed
      const textarea = win.querySelector('[name="message"]');
      if (textarea && textarea.value !== (chat.draft || '')) {
        textarea.value = chat.draft || '';
      }
    });

    // 4. Align position and scroll
    alignChatWindows(root);
    scrollActiveChatIntoView(root, state);
    return; // Exit early without recreating DOM nodes!
  }
  // --- END OF IN-PLACE DOM UPDATE FAST-PATH ---

  const maxDateVal = getLocalDateString(Date.now() + 10 * 365 * 24 * 60 * 60 * 1000);

  root.innerHTML = `
    <section class="ctox-chat-dock ${dockCollapsed ? 'is-collapsed' : ''}" data-chat-dock>
      <button class="ctox-chat-fab" type="button" data-chat-open aria-label="Chat öffnen">
        <span>Chat</span><b>${openChats.length || ''}</b>
      </button>

      <div class="ctox-chat-date-pill">
        <button class="ctox-date-nav-btn" type="button" data-chat-date-prev aria-label="Vorheriger Tag">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6"></polyline></svg>
        </button>
        <div class="ctox-date-picker-trigger">
          <span class="ctox-date-label">${formatGermanDateLabel(selectedDate)}</span>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="18" rx="2" ry="2"></rect><line x1="16" y1="2" x2="16" y2="6"></line><line x1="8" y1="2" x2="8" y2="6"></line><line x1="3" y1="10" x2="21" y2="10"></line></svg>
          <input type="date" class="ctox-date-native-picker" data-chat-date-picker value="${selectedDate}" max="${maxDateVal}" />
        </div>
        <button class="ctox-date-nav-btn" type="button" data-chat-date-next aria-label="Nächster Tag">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"></polyline></svg>
        </button>
      </div>

      ${dockCollapsed ? '' : `
        <button class="ctox-chat-nav" type="button" data-chat-prev aria-label="Vorheriger Chat">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6"></polyline></svg>
        </button>
        <div class="ctox-chat-strip" data-chat-strip aria-label="Offene Chats">
          ${openChats.map((chat) => chatDockItem(chat, activeChat?.id)).join('')}
        </div>
        <button class="ctox-chat-nav" type="button" data-chat-next aria-label="Nächster Chat">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"></polyline></svg>
        </button>
        <button class="ctox-chat-new" type="button" data-chat-new aria-label="Neuer Chat">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="5" x2="12" y2="19"></line><line x1="5" y1="12" x2="19" y2="12"></line></svg>
        </button>
      `}
    </section>
    <div class="ctox-chat-stage" data-chat-stage>
      <div class="ctox-chat-stage-inner ${hasMaximized ? 'has-maximized' : ''}">
        ${dockCollapsed ? '' : (() => {
          const activeIndex = openChats.findIndex((c) => c.id === activeChat?.id);
          return openChats.map((chat, idx) => {
            const relation = idx < activeIndex ? 'left' : idx > activeIndex ? 'right' : 'center';
            return chatWindow(chat, activeChat?.id, relation);
          }).join('');
        })()}
        <div class="ctox-chat-stage-spacer" style="position: relative; width: 1px; height: 1px; pointer-events: none; margin-top: -1px;"></div>
      </div>
    </div>
  `;

  root.querySelector('[data-chat-date-prev]')?.addEventListener('click', async () => {
    shiftSelectedDate(state, -1);
    const chat = ensureChat(state);
    chat.minimized = false;
    await persistChatState({ state, db });
    renderChatRoot({ root, state, commandBus, db, getActiveModule });
  });

  root.querySelector('[data-chat-date-next]')?.addEventListener('click', async () => {
    shiftSelectedDate(state, 1);
    const chat = ensureChat(state);
    chat.minimized = false;
    await persistChatState({ state, db });
    renderChatRoot({ root, state, commandBus, db, getActiveModule });
  });

  root.querySelector('[data-chat-date-picker]')?.addEventListener('change', async (event) => {
    const val = event.currentTarget.value;
    if (val) {
      state.selectedDate = val;
      const chat = ensureChat(state);
      chat.minimized = false;
      await persistChatState({ state, db });
      renderChatRoot({ root, state, commandBus, db, getActiveModule });
    }
  });

  root.querySelector('[data-chat-new]')?.addEventListener('click', async () => {
    const next = createChat(state.ownerUserId, state.selectedDate);
    state.chats.push(next);
    expandChatOnly(state, next);
    state.dockCollapsed = false;
    touchChats(state, [next]);
    await persistChatState({ state, db });
    renderChatRoot({ root, state, commandBus, db, getActiveModule });
  });

  root.querySelector('[data-chat-prev]')?.addEventListener('click', (e) => {
    e.preventDefault();
    e.stopPropagation();
    const strip = root.querySelector('[data-chat-strip]');
    if (strip) {
      strip.scrollBy({ left: -200, behavior: 'smooth' });
    }
  });

  root.querySelector('[data-chat-next]')?.addEventListener('click', (e) => {
    e.preventDefault();
    e.stopPropagation();
    const strip = root.querySelector('[data-chat-strip]');
    if (strip) {
      strip.scrollBy({ left: 200, behavior: 'smooth' });
    }
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

    node.addEventListener('click', async (e) => {
      if (node.classList.contains('is-active')) return;
      if (e.target.closest('button, a, input, textarea, form, svg, path')) return;
      state.activeChatId = chat.id;
      chat.minimized = false;
      touchChats(state, [chat]);
      await persistChatState({ state, db });
      renderChatRoot({ root, state, commandBus, db, getActiveModule });
    });

    node.querySelectorAll('[data-chat-minimize]').forEach((button) => button.addEventListener('click', async () => {
      chat.minimized = true;
      touchChats(state, [chat]);
      await persistChatState({ state, db });
      renderChatRoot({ root, state, commandBus, db, getActiveModule });
    }));

    node.querySelectorAll('[data-chat-title]').forEach((titleBtn) => {
      titleBtn.addEventListener('click', async (e) => {
        chat.maximized = !chat.maximized;
        chat.minimized = false;
        state.activeChatId = chat.id;
        touchChats(state, [chat]);
        await persistChatState({ state, db });
        renderChatRoot({ root, state, commandBus, db, getActiveModule });
      });
    });

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
      const next = createChat(state.ownerUserId, state.selectedDate);
      state.chats.push(next);
      expandChatOnly(state, next);
      state.dockCollapsed = false;
      touchChats(state, [next]);
      await persistChatState({ state, db });
      renderChatRoot({ root, state, commandBus, db, getActiveModule });
    });

    node.querySelectorAll('[data-track-task]').forEach((button) => {
      button.addEventListener('click', () => {
        openCtoxTask(button.dataset.taskId || '', button.dataset.commandId || '', button.dataset.taskStatus || '').catch((error) => {
          console.warn('[business-chat] failed to open CTOX task', error);
        });
      });
    });

    node.querySelector('[data-chat-cancel-schedule]')?.addEventListener('click', async () => {
      await cancelScheduledChat(state, chat, db, root, commandBus, getActiveModule);
    });

    node.querySelector('[data-chat-time-input]')?.addEventListener('change', async (event) => {
      const timeVal = event.currentTarget.value;
      if (timeVal) {
        const [hours, minutes] = timeVal.split(':').map(Number);
        const d = new Date(chat.createdAt);
        d.setHours(hours, minutes, 0, 0);
        chat.createdAt = d.getTime();
        chat.updated_at_ms = Date.now();
        await persistChatState({ state, db });
        renderChatRoot({ root, state, commandBus, db, getActiveModule });
      }
    });

    node.querySelectorAll('[data-chat-clip]').forEach((clipBtn) => {
      clipBtn.addEventListener('click', () => {
        const fileInput = node.querySelector(`[data-chat-file-input="${chat.id}"]`);
        fileInput?.click();
      });
    });

    const fileInput = node.querySelector(`[data-chat-file-input="${chat.id}"]`);
    fileInput?.addEventListener('change', async (e) => {
      const files = e.currentTarget.files;
      if (files?.length) {
        for (const file of Array.from(files)) {
          await addAttachmentToChatState(chat, file);
        }
        await persistChatState({ state, db });
        renderChatRoot({ root, state, commandBus, db, getActiveModule });
      }
    });

    node.querySelectorAll('[data-remove-attachment]').forEach((btn) => {
      btn.addEventListener('click', async (e) => {
        e.stopPropagation();
        e.preventDefault();
        const parts = btn.dataset.removeAttachment.split(':');
        const attIdx = parseInt(parts[parts.length - 1], 10);
        if (chat.attachments && chat.attachments[attIdx]) {
          chat.attachments.splice(attIdx, 1);
          await persistChatState({ state, db });
          renderChatRoot({ root, state, commandBus, db, getActiveModule });
        }
      });
    });

    node.addEventListener('dragover', (e) => {
      e.preventDefault();
      node.classList.add('drag-active');
    });
    node.addEventListener('dragleave', (e) => {
      if (e.relatedTarget && node.contains(e.relatedTarget)) return;
      node.classList.remove('drag-active');
    });
    node.addEventListener('drop', async (e) => {
      e.preventDefault();
      node.classList.remove('drag-active');
      const files = e.dataTransfer?.files;
      if (files?.length) {
        let added = false;
        for (const file of Array.from(files)) {
          if (file.type.startsWith('image/') || file.type === 'application/pdf') {
            await addAttachmentToChatState(chat, file);
            added = true;
          }
        }
        if (added) {
          await persistChatState({ state, db });
          renderChatRoot({ root, state, commandBus, db, getActiveModule });
        }
      }
    });

    const textarea = node.querySelector('[name="message"]');
    if (textarea) {
      const adjustHeight = () => {
        textarea.style.height = 'auto';
        textarea.style.height = `${textarea.scrollHeight}px`;
      };
      textarea.addEventListener('input', (event) => {
        chat.draft = event.currentTarget.value;
        adjustHeight();
      });
      textarea.addEventListener('paste', async (e) => {
        const items = e.clipboardData?.items;
        if (!items) return;
        let fileAdded = false;
        for (const item of items) {
          if (item.type.startsWith('image/') || item.type === 'application/pdf') {
            const file = item.getAsFile();
            if (file) {
              e.preventDefault();
              await addAttachmentToChatState(chat, file);
              fileAdded = true;
            }
          }
        }
        if (fileAdded) {
          await persistChatState({ state, db });
          renderChatRoot({ root, state, commandBus, db, getActiveModule });
        }
      });
      window.requestAnimationFrame(adjustHeight);
    }

    const form = node.querySelector('[data-chat-form]');
    const submitFromForm = async (event) => {
      event.preventDefault();
      event.stopPropagation();
      await submitChatForm({ root, state, chat, node, commandBus, db, getActiveModule });
    };
    form?.addEventListener('submit', submitFromForm);
    form?.querySelector('button[type="submit"]')?.addEventListener('click', submitFromForm);
  });

  root.querySelectorAll('[data-chat-followup-trigger]').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const chatId = btn.dataset.chatFollowupTrigger;
      const chat = state.chats.find((item) => item.id === chatId);
      if (chat) {
        chat.showFollowUp = true;
        await persistChatState({ state, db });
        renderChatRoot({ root, state, commandBus, db, getActiveModule });
      }
    });
  });

  alignChatWindows(root);
  scrollActiveChatIntoView(root, state);
  window.requestAnimationFrame(() => {
    root.querySelectorAll('.ctox-chat-window.no-left-transition').forEach((win) => {
      win.classList.remove('no-left-transition');
    });
  });
}

async function submitChatForm({ root, state, chat, node, commandBus, db, getActiveModule }) {
  if (chat.__submitting) return;
  captureDrafts(root, state);
  const input = node.querySelector('[name="message"]');
  const text = String(input?.value || chat.draft || '').trim();
  if (!text) return;

  const isFuture = chat.createdAt > Date.now();
  if (isFuture) {
    chat.__submitting = true;
    chat.draft = '';
    chat.showFollowUp = false;
    chat.attachments = [];
    if (input) input.value = '';
    try {
      const now = Date.now();
      const messageId = `chatmsg_${crypto.randomUUID()}`;
      const commandId = `cmd_${crypto.randomUUID()}`;
      
      chat.messages.push({
        id: messageId,
        role: 'user',
        text,
        createdAt: now,
      });
      
      chat.messages.push({
        id: `status_${commandId}`,
        role: 'ctox',
        text: 'Ausführung verzögert/geplant.',
        commandId,
        taskId: '',
        status: 'scheduled',
        createdAt: now,
      });
      
      chat.lastTrackingId = commandId;
      touchChats(state, [chat]);
      
      await persistChatState({ state, db });
      renderChatRoot({ root, state, commandBus, db, getActiveModule });
    } finally {
      delete chat.__submitting;
    }
    return;
  }

  chat.__submitting = true;
  chat.draft = '';
  chat.showFollowUp = false; // Reset follow-up container state
  chat.attachments = [];
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
  if (!chat.minimized) {
    chat.minimized = true;
  } else {
    chat.open = true;
    chat.minimized = false;
    state.activeChatId = chat.id;
  }
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

function getTaskState(chat) {
  const isFuture = chat.createdAt > Date.now();
  const hasScheduledMessage = Array.isArray(chat.messages) && chat.messages.some(m => m.status === 'scheduled');
  if (isFuture && hasScheduledMessage) return 'scheduled';

  if (!chat.lastTrackingId) return 'idle';
  const trackingMsg = [...chat.messages].reverse().find(m => 
    (m.commandId && m.commandId === chat.lastTrackingId) || 
    (m.taskId && m.taskId === chat.lastTrackingId)
  );
  if (!trackingMsg) return 'idle';
  const status = String(trackingMsg.status || '').toLowerCase();
  if (status === 'scheduled') return 'scheduled';
  if (!status) return 'idle';
  if (status === 'success' || status === 'completed' || status === 'done' || status === 'erledigt') return 'success';
  if (['failed', 'blocked', 'stale_missing_native', 'error'].includes(status)) return 'failed';
  if (['queued', 'pending', 'pending_sync', 'waiting'].includes(status)) return 'queued';
  if (['running', 'processing', 'executing', 'active'].includes(status)) return 'running';
  return 'idle';
}

function expandChatOnly(state, activeChat) {
  state.activeChatId = activeChat.id;
  activeChat.open = true;
  activeChat.minimized = false;
}


function chatWindow(chat, activeId, relation = 'center') {
  const moduleName = chat.contextMeta?.module || 'ctox';
  const taskState = getTaskState(chat);
  const isFuture = chat.createdAt > Date.now();

  const stagedAttachments = chat.attachments || [];
  const attachmentsHtml = stagedAttachments.length ? `
    <div class="ctox-chat-attachments-preview">
      ${stagedAttachments.map((att, idx) => `
        <div class="ctox-attachment-item" data-att-idx="${idx}">
          ${att.mimeType.startsWith('image/') 
            ? `<img class="ctox-attachment-thumbnail" src="${escapeAttr(att.base64Data)}" alt="${escapeAttr(att.name)}" />`
            : `<span class="ctox-attachment-icon">📄</span>`
          }
          <span class="ctox-attachment-name" title="${escapeAttr(att.name)}">${escapeHtml(att.name)}</span>
          <button type="button" class="ctox-attachment-remove" data-remove-attachment="${escapeAttr(chat.id)}:${idx}" title="Entfernen">×</button>
        </div>
      `).join('')}
    </div>
  ` : '';

  let statusBadgeHtml = '';
  if (taskState === 'running') {
    statusBadgeHtml = `
      <span class="ctox-chat-status-badge is-running" title="CTOX läuft...">
        <span class="ctox-status-spinner"></span>
        <span>Aktiv</span>
      </span>
    `;
  } else if (taskState === 'queued') {
    statusBadgeHtml = `
      <span class="ctox-chat-status-badge is-queued" title="In Warteschlange...">
        <span class="ctox-status-dot"></span>
        <span>Queue</span>
      </span>
    `;
  } else if (taskState === 'success') {
    statusBadgeHtml = `
      <span class="ctox-chat-status-badge is-success" title="Erledigt!">
        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"></polyline></svg>
        <span>Erledigt</span>
      </span>
    `;
  } else if (taskState === 'failed') {
    statusBadgeHtml = `
      <span class="ctox-chat-status-badge is-failed" title="Blocked/Fehlgeschlagen">
        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="9" x2="12" y2="13"></line><line x1="12" y1="17" x2="12.01" y2="17"></line><path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"></path></svg>
        <span>Blocked</span>
      </span>
    `;
  } else if (taskState === 'scheduled') {
    statusBadgeHtml = `
      <span class="ctox-chat-status-badge is-scheduled" title="Verzögerte Ausführung geplant">
        <svg class="ctox-clock-pulse" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"></circle><polyline points="12 6 12 12 16 14"></polyline></svg>
        <span>Geplant</span>
      </span>
    `;
  }

  // Determine what to show at the bottom
  let bottomHtml = '';
  if (taskState === 'scheduled') {
    const timeText = getFormattedDateTime(chat.createdAt);
    bottomHtml = `
      <div class="ctox-chat-scheduler-card">
        <div class="ctox-scheduler-glow"></div>
        <div class="ctox-scheduler-header">
          <svg class="ctox-clock-spinner" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"></circle><polyline points="12 6 12 12 16 14"></polyline></svg>
          <div class="ctox-scheduler-info">
            <strong>Verzögerte Ausführung geplant</strong>
            <span>Wird ausgeführt am: ${timeText}</span>
          </div>
        </div>
        <div class="ctox-scheduler-timer-container">
          <span class="ctox-scheduler-timer-badge">Timer:</span>
          <strong class="ctox-scheduler-timer" data-countdown-timer="${chat.id}">${getCountdownText(chat.createdAt)}</strong>
        </div>
        <button class="ctox-scheduler-cancel-btn" type="button" data-chat-cancel-schedule="${chat.id}">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
          <span>Planung abbrechen</span>
        </button>
      </div>
    `;
  } else if (taskState === 'queued' || taskState === 'running') {
    // Hide input, show active progress card
    const trackingMsg = [...chat.messages].reverse().find(m => 
      (m.commandId && m.commandId === chat.lastTrackingId) || 
      (m.taskId && m.taskId === chat.lastTrackingId)
    );
    const taskId = trackingMsg?.taskId || '';
    const commandId = trackingMsg?.commandId || chat.lastTrackingId || '';
    const taskStatus = trackingMsg?.status || 'queued';
    
    bottomHtml = `
      <div class="ctox-chat-delegation-card">
        <div class="ctox-delegation-glow"></div>
        <div class="ctox-delegation-header">
          <span class="ctox-delegation-spinner"></span>
          <div class="ctox-delegation-info">
            <strong>Aufgabe delegiert &amp; aktiv</strong>
            <span>CTOX verarbeitet deine Anfrage...</span>
          </div>
        </div>
        <button class="ctox-delegation-watch-btn" type="button" data-track-task data-task-id="${escapeAttr(taskId)}" data-command-id="${escapeAttr(commandId)}" data-task-status="${escapeAttr(taskStatus)}">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"></path><circle cx="12" cy="12" r="3"></circle></svg>
          <span>Live-Harness beobachten</span>
        </button>
      </div>
    `;
  } else if (taskState === 'success' || taskState === 'failed') {
    if (chat.showFollowUp) {
      bottomHtml = `
        ${attachmentsHtml}
        <form class="ctox-chat-form" data-chat-form>
          <input type="file" multiple accept="image/*,application/pdf" style="display: none;" data-chat-file-input="${chat.id}" />
          <button type="button" class="ctox-chat-clip-btn" data-chat-clip="${chat.id}" title="Datei hinzufügen">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48"></path></svg>
          </button>
          <textarea name="message" placeholder="Folgeaufgabe eingeben..." required>${escapeHtml(chat.draft || '')}</textarea>
          <button type="submit" data-chat-send aria-label="Senden">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="19" x2="12" y2="5"></line><polyline points="5 12 12 5 19 12"></polyline></svg>
          </button>
        </form>
      `;
    } else {
      bottomHtml = `
        <div class="ctox-followup-container">
          <button class="ctox-followup-btn" type="button" data-chat-followup-trigger="${escapeAttr(chat.id)}">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="5" x2="12" y2="19"></line><line x1="5" y1="12" x2="19" y2="12"></line></svg>
            <span>Folgeaufgabe eingeben</span>
          </button>
        </div>
      `;
    }
  } else {
    // idle state
    bottomHtml = `
      ${attachmentsHtml}
      <form class="ctox-chat-form" data-chat-form>
        <input type="file" multiple accept="image/*,application/pdf" style="display: none;" data-chat-file-input="${chat.id}" />
        <button type="button" class="ctox-chat-clip-btn" data-chat-clip="${chat.id}" title="Datei hinzufügen">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48"></path></svg>
        </button>
        <textarea name="message" placeholder="Aufgabe an CTOX..." required>${escapeHtml(chat.draft || '')}</textarea>
        <button type="submit" data-chat-send aria-label="Senden">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="19" x2="12" y2="5"></line><polyline points="5 12 12 5 19 12"></polyline></svg>
        </button>
      </form>
    `;
  }

  const isMinimizedClass = chat.minimized ? 'is-minimized' : '';
  const taskStateClass = `is-task-${taskState}`;

  let schedulerBarHtml = '';
  if (isFuture) {
    schedulerBarHtml = `
      <div class="ctox-chat-scheduler-bar">
        <div style="display: flex; align-items: center; gap: 6px;">
          <svg class="ctox-clock-pulse" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"></circle><polyline points="12 6 12 12 16 14"></polyline></svg>
          <span>Planung:</span>
          <strong>${formatGermanDateLabel(getLocalDateString(chat.createdAt))}</strong>
          <span>um</span>
          <input type="time" class="ctox-chat-time-input" data-chat-time-input="${chat.id}" value="${getFormattedTime(chat.createdAt)}" />
        </div>
        <div>
          <span class="ctox-chat-countdown" data-countdown-timer="${chat.id}">${getCountdownText(chat.createdAt)}</span>
        </div>
      </div>
    `;
  }

  return `
    <section class="ctox-chat-window no-left-transition ${chat.maximized ? 'is-maximized' : ''} ${chat.id === activeId ? 'is-active' : ''} ${isMinimizedClass} ${taskStateClass}" data-chat-id="${escapeAttr(chat.id)}" data-chat-module="${escapeAttr(moduleName)}" data-chat-rel="${escapeAttr(relation)}">
      <header>
        <button class="ctox-chat-title" type="button" data-chat-title="${escapeAttr(chat.id)}">
          <div style="display: flex; align-items: center; gap: 8px; width: 100%; min-width: 0;">
            <strong>${escapeHtml(chat.title || 'CTOX')}</strong>
            ${statusBadgeHtml}
          </div>
          ${chat.lastTrackingId ? `<span>${escapeHtml(chat.lastTrackingId)}</span>` : '<span>Business OS</span>'}
        </button>
        <div class="ctox-chat-header-actions">
          <button type="button" data-chat-new aria-label="Neuer Chat">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="5" x2="12" y2="19"></line><line x1="5" y1="12" x2="19" y2="12"></line></svg>
          </button>
          <button type="button" data-chat-maximize aria-label="Chat maximieren">
            ${chat.maximized 
              ? `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 14 10 14 10 20"></polyline><polyline points="20 10 14 10 14 4"></polyline><line x1="14" y1="10" x2="21" y2="3"></line><line x1="10" y1="14" x2="3" y2="21"></line></svg>` 
              : `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 3 21 3 21 9"></polyline><polyline points="9 21 3 21 3 15"></polyline><line x1="21" y1="3" x2="14" y2="10"></line><line x1="3" y1="21" x2="10" y2="14"></line></svg>`}
          </button>
          <button type="button" data-chat-minimize aria-label="Chat einklappen">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="5" y1="12" x2="19" y2="12"></line></svg>
          </button>
          <button type="button" data-chat-delete aria-label="Chat löschen" class="is-delete">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path><line x1="10" y1="11" x2="10" y2="17"></line><line x1="14" y1="11" x2="14" y2="17"></line></svg>
          </button>
        </div>
      </header>
      <div class="ctox-chat-drag-overlay">
        <svg viewBox="0 0 24 24" width="28" height="28" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path>
          <polyline points="17 8 12 3 7 8"></polyline>
          <line x1="12" y1="3" x2="12" y2="15"></line>
        </svg>
        <strong>Datei hier ablegen</strong>
      </div>
      ${schedulerBarHtml}
      <div class="ctox-chat-messages">
        ${chat.messages.length ? chat.messages.map(messageMarkup).join('') : '<div class="ctox-chat-empty">CTOX Aufgabe eingeben.</div>'}
      </div>
      ${bottomHtml}
    </section>
  `;
}

function chatDockItem(chat, activeId) {
  const taskState = getTaskState(chat);
  const count = Array.isArray(chat.messages) ? chat.messages.length : 0;
  const status = chat.lastTrackingId ? (taskState.toUpperCase()) : count ? `${count} Msg` : 'Leer';
  const moduleName = chat.contextMeta?.module || 'ctox';
  
  let markHtml = '';
  if (taskState === 'running') {
    markHtml = `<span class="ctox-chat-chip-mark is-running" aria-hidden="true"><span class="ctox-chip-spinner"></span></span>`;
  } else if (taskState === 'queued') {
    markHtml = `<span class="ctox-chat-chip-mark is-queued" aria-hidden="true"></span>`;
  } else if (taskState === 'success') {
    markHtml = `
      <span class="ctox-chat-chip-mark is-success" aria-hidden="true">
        <svg width="8" height="8" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="4.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"></polyline></svg>
      </span>
    `;
  } else if (taskState === 'failed') {
    markHtml = `
      <span class="ctox-chat-chip-mark is-failed" aria-hidden="true">
        <svg width="8" height="8" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="4.5" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="9" x2="12" y2="13"></line><line x1="12" y1="17" x2="12.01" y2="17"></line></svg>
      </span>
    `;
  } else {
    markHtml = `<span class="ctox-chat-chip-mark" aria-hidden="true"></span>`;
  }

  return `
    <button class="ctox-chat-chip ${chat.id === activeId && !chat.minimized ? 'is-active' : ''} ${chat.minimized ? 'is-minimized' : ''} ${!chat.minimized ? 'is-expanded' : ''}" type="button" data-chat-focus="${escapeAttr(chat.id)}" data-chat-module="${escapeAttr(moduleName)}">
      ${markHtml}
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
  const activeChip = Array.from(root.querySelectorAll('[data-chat-focus]'))
    .find((node) => node.dataset.chatFocus === state.activeChatId);
  activeChip?.scrollIntoView?.({ inline: 'center', block: 'nearest', behavior: 'smooth' });
  
  // Auto-scroll messages list to bottom for all open/expanded windows
  root.querySelectorAll('[data-chat-id]:not(.is-minimized)').forEach((node) => {
    const messagesContainer = node.querySelector('.ctox-chat-messages');
    if (messagesContainer) {
      messagesContainer.scrollTop = messagesContainer.scrollHeight;
    }
  });
}

function trackButtonLabel(message) {
  const de = (document.documentElement.lang || 'de').toLowerCase().startsWith('de');
  const status = String(message.status || '').toLowerCase();
  if (isFailureStatus(status)) return de ? 'Details ansehen' : 'View details';
  if (['completed', 'passed', 'done', 'handled'].includes(status)) {
    return de ? 'Ergebnis ansehen' : 'View result';
  }
  return de ? 'Fortschritt ansehen' : 'View progress';
}

function formatChatBodyHtml(rawText) {
  const text = String(rawText || '');
  return text
    .split(/(```[\s\S]*?```)/g)
    .map((part) => {
      if (part.length >= 6 && part.startsWith('```') && part.endsWith('```')) {
        const body = part.slice(3, -3);
        const nl = body.indexOf('\n');
        const firstLine = nl >= 0 ? body.slice(0, nl).trim() : '';
        const code = nl >= 0 && /^[a-zA-Z0-9_+#.-]*$/.test(firstLine) ? body.slice(nl + 1) : body;
        return `<pre class="ctox-chat-code"><code>${escapeHtml(code.replace(/\n$/, ''))}</code></pre>`;
      }
      if (!part) return '';
      // escapeHtml first, then layer minimal, safe inline Markdown onto escaped text.
      let html = escapeHtml(part);
      html = html.replace(/`([^`]+)`/g, (_m, code) => `<code>${code}</code>`);
      html = html.replace(/\*\*([^*\n]+)\*\*/g, '<strong>$1</strong>');
      // Links: the URL comes from already-escaped text, so quotes/&/< are neutralised
      // and cannot break out of the attribute.
      html = html.replace(
        /\[([^\]\n]+)\]\((https?:\/\/[^\s)]+)\)/g,
        (_m, label, url) => `<a href="${url}" target="_blank" rel="noopener noreferrer">${label}</a>`,
      );
      return `<span class="ctox-chat-text">${html}</span>`;
    })
    .join('');
}

function messageMarkup(message) {
  const trackId = message.taskId || message.commandId;
  const tracking = message.trackable === false ? '' : (message.commandId || message.taskId)
    ? `<button class="ctox-chat-track" type="button" data-track-task data-task-id="${escapeAttr(message.taskId || '')}" data-command-id="${escapeAttr(message.commandId || '')}" data-task-status="${escapeAttr(message.status || '')}" title="${escapeAttr(trackId)}">${escapeHtml(trackButtonLabel(message))}</button>`
    : '';
  const meta = [message.status, message.detail].filter(Boolean).join(' · ');
  return `
    <article class="ctox-chat-message is-${escapeAttr(message.role || 'ctox')}">
      <div class="ctox-chat-body">${formatChatBodyHtml(message.text || '')}</div>
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
      prompt: meta.prompt || extraPayload.prompt || text,
      chat_id: chat.id,
      message_id: messageId,
      conversation: compactConversation(chat.messages),
      attachments: chat.attachments || [],
      inbound_channel: meta.inbound_channel || CHAT_CHANNEL,
      outbound_channel: 'business_os_chat',
      response_channel: 'business_os_chat',
      reply_to: chat.id,
      thread_key: meta.thread_key || extraPayload.thread_key || `business-os/chat/${chat.id}`,
      priority: meta.priority || extraPayload.priority || 'normal',
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
    pendingMessage.text = `Command konnte nicht an CTOX übergeben werden: ${error?.message || String(error)}`;
    pendingMessage.commandId = failedCommandId;
    pendingMessage.taskId = '';
    pendingMessage.status = error?.status || 'failed';
    pendingMessage.trackable = false;
    pendingMessage.detail = 'nicht übergeben';
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
      const orphanedTracking = !commandDoc && !taskDoc && isActiveTrackingStatus(message.status) && trackingMessageAgeMs(message) > 10 * 60 * 1000;
      const nextStatus = orphanedTracking ? 'failed' : (taskDoc?.status || commandDoc?.task_status || commandDoc?.status || message.status || '');
      if (orphanedTracking && message.trackable !== false) {
        message.trackable = false;
        changed = true;
      }
      if (nextTaskId && nextTaskId !== message.taskId) {
        message.taskId = nextTaskId;
        chat.lastTrackingId = nextTaskId;
        changed = true;
      }
      if (nextStatus && nextStatus !== message.status) {
        message.status = nextStatus;
        if (orphanedTracking) {
          message.text = 'CTOX kann diese ältere Aufgabe nicht mehr verfolgen: kein passender Command oder Queue-Task ist vorhanden.';
        }
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
  try {
    const doc = await collection.findOne(id).exec();
    return doc?.toJSON?.() || null;
  } catch {
    return null;
  }
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

function isActiveTrackingStatus(status) {
  return ['accepted', 'queued', 'pending', 'pending_sync', 'waiting', 'running', 'processing', 'executing', 'active'].includes(String(status || '').toLowerCase());
}

function trackingMessageAgeMs(message) {
  const createdAt = Number(message?.createdAt || 0);
  return Number.isFinite(createdAt) && createdAt > 0 ? Math.max(0, Date.now() - createdAt) : 0;
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

async function openCtoxTask(taskId, commandId, taskStatus) {
  const focus = { taskId, commandId, taskStatus, sourceModule: 'business-os-chat', openDrawer: true };
  try {
    sessionStorage.setItem('ctox.businessOs.focusTask', JSON.stringify(focus));
  } catch {}
  const params = new URLSearchParams();
  if (taskId) params.set('task_id', taskId);
  if (commandId) params.set('command_id', commandId);
  if (taskStatus) params.set('task_status', taskStatus);
  params.set('source', 'business-os-chat');
  params.set('drawer', '1');
  location.hash = `#ctox?${params.toString()}`;
  const app = window.CTOX_BUSINESS_OS_APP;
  if (typeof app?.openModule === 'function' && app.activeModule?.id !== 'ctox') {
    await app.openModule('ctox');
  }
  window.dispatchEvent(new CustomEvent('ctox-business-os-focus-task', { detail: focus }));
}

// Date and Temporal Utilities for Calendar-Scoped Chats
function getLocalDateString(timestampOrDate = Date.now()) {
  const d = new Date(timestampOrDate);
  const yyyy = d.getFullYear();
  const mm = String(d.getMonth() + 1).padStart(2, '0');
  const dd = String(d.getDate()).padStart(2, '0');
  return `${yyyy}-${mm}-${dd}`;
}

function formatGermanDateLabel(dateStr) {
  const todayStr = getLocalDateString(Date.now());
  
  const yesterday = new Date();
  yesterday.setDate(yesterday.getDate() - 1);
  const yesterdayStr = getLocalDateString(yesterday);
  
  const tomorrow = new Date();
  tomorrow.setDate(tomorrow.getDate() + 1);
  const tomorrowStr = getLocalDateString(tomorrow);
  
  if (dateStr === todayStr) return 'Heute';
  if (dateStr === yesterdayStr) return 'Gestern';
  if (dateStr === tomorrowStr) return 'Morgen';
  
  const [y, m, d] = dateStr.split('-').map(Number);
  const shortMonths = [
    'Jan', 'Feb', 'Mär', 'Apr', 'Mai', 'Jun',
    'Jul', 'Aug', 'Sep', 'Okt', 'Nov', 'Dez'
  ];
  return `${d}. ${shortMonths[m - 1]} '${String(y).slice(-2)}`;
}

function shiftSelectedDate(state, days) {
  const selectedDate = state.selectedDate || getLocalDateString(Date.now());
  const [y, m, d] = selectedDate.split('-').map(Number);
  const date = new Date(y, m - 1, d);
  date.setDate(date.getDate() + days);
  state.selectedDate = getLocalDateString(date);
}

function createTimestampForDateString(dateStr) {
  const todayStr = getLocalDateString(Date.now());
  if (dateStr === todayStr) {
    return Date.now();
  }
  const now = new Date();
  const [y, m, d] = dateStr.split('-').map(Number);
  const targetDate = new Date(y, m - 1, d, now.getHours(), now.getMinutes(), now.getSeconds(), now.getMilliseconds());
  return targetDate.getTime();
}

function ensureChat(state, session = null) {
  const dateStr = state.selectedDate || getLocalDateString(Date.now());
  const chatsOfDate = state.chats.filter((c) => getLocalDateString(c.createdAt) === dateStr);
  let chat = chatsOfDate.find((item) => item.id === state.activeChatId)
    || chatsOfDate.find((item) => item.open !== false)
    || chatsOfDate[0];
  if (!chat) {
    chat = createChat(ownerUserId(session) || state.ownerUserId, dateStr);
    state.chats.push(chat);
  }
  chat.open = true;
  state.activeChatId = chat.id;
  return chat;
}

function createChat(owner = '', dateStr = '') {
  const targetDateStr = dateStr || getLocalDateString(Date.now());
  const timestamp = createTimestampForDateString(targetDateStr);
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
    createdAt: timestamp,
    updated_at_ms: timestamp,
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
      selectedDate: parsed.selectedDate || getLocalDateString(Date.now()),
      activeChatId: parsed.activeChatId || '',
      dockCollapsed: 'dockCollapsed' in parsed ? Boolean(parsed.dockCollapsed) : true,
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
          showFollowUp: Boolean(chat.showFollowUp),
          attachments: Array.isArray(chat.attachments) ? chat.attachments : [],
        })),
    };
  } catch {
    return { ownerUserId: owner, selectedDate: getLocalDateString(Date.now()), dockCollapsed: true, preCollapseExpandedChatIds: [], chats: [] };
  }
}

function writeChatState(state) {
  localStorage.setItem(CHAT_STATE_KEY, JSON.stringify({
    selectedDate: state.selectedDate || getLocalDateString(Date.now()),
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
      showFollowUp: Boolean(chat.showFollowUp),
      attachments: Array.isArray(chat.attachments) ? chat.attachments : [],
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
      showFollowUp: Boolean(chat.showFollowUp),
      attachments: Array.isArray(chat.attachments) ? chat.attachments : [],
    };
    try {
      const existing = await collection.findOne(chat.id).exec();
      if (existing) await existing.incrementalPatch(doc);
      else await collection.insert(doc);
    } catch (error) {
      if (isVolatileChatPersistenceError(error)) return;
      throw error;
    }
  }
}

function isVolatileChatPersistenceError(error) {
  const text = String(error?.message || error || '');
  return /QUERY_CANCELLED|replication-cancel|WebRTC replication cancelled|IDBDatabase.*closing|database connection is closing|collection is closed|closed collection|RxDB Error-Code: COL21/i.test(text);
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
    showFollowUp: Boolean(chat.showFollowUp),
    attachments: Array.isArray(chat.attachments) ? chat.attachments : [],
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
  return clean.length > 42 ? `${clean.slice(0, 39)}...` : clean || 'CTOX';
}

function installChatStyles() {
  if (document.getElementById(CHAT_STYLE_ID)) return;
  const style = document.createElement('style');
  style.id = CHAT_STYLE_ID;
  style.textContent = `
    @keyframes ctoxChatSlideIn {
      from {
        opacity: 0;
        transform: translateY(40px) scale(0.95);
      }
      to {
        opacity: 1;
        transform: translateY(0) scale(1);
      }
    }
    @keyframes ctoxChipSlideIn {
      from {
        opacity: 0;
        transform: scale(0.85) translateX(-10px);
      }
      to {
        opacity: 1;
        transform: scale(1) translateX(0);
      }
    }
    @keyframes ctoxChipActivePulse {
      0% {
        transform: translateY(0) scale(1);
        box-shadow: 0 0 0 0 color-mix(in srgb, var(--accent) 30%, transparent);
      }
      100% {
        transform: translateY(-1px) scale(1.02);
        box-shadow: 0 4px 12px color-mix(in srgb, var(--accent) 30%, transparent), 0 0 0 1px var(--accent) inset;
      }
    }
    .ctox-chat-root {
      --spring-bounce: cubic-bezier(0.34, 1.56, 0.64, 1);
      --spring-ease: cubic-bezier(0.25, 1, 0.5, 1);
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
    .ctox-chat-root.is-scrolling .ctox-chat-window {
      transition: none !important;
    }
    .ctox-chat-root button,
    .ctox-chat-root textarea {
      font: inherit;
    }
    .ctox-chat-dock {
      pointer-events: auto;
      grid-row: 2;
      justify-self: start;
      display: grid;
      grid-template-columns: 88px 115px 28px minmax(0, max-content) 28px 34px;
      align-items: center;
      gap: 8px;
      min-width: 0;
      width: auto;
      max-width: 100%;
      padding: 6px;
      border: 1px solid color-mix(in srgb, var(--line) 35%, transparent);
      border-radius: 14px;
      background: color-mix(in srgb, var(--surface) 35%, transparent);
      backdrop-filter: blur(20px) saturate(180%);
      -webkit-backdrop-filter: blur(20px) saturate(180%);
      box-shadow: 0 16px 40px rgba(0, 0, 0, 0.12), 0 1px 0 rgba(255, 255, 255, 0.08) inset;
      transition: border-color 0.3s var(--spring-bounce), box-shadow 0.3s var(--spring-bounce);
    }
    .ctox-chat-dock:hover {
      border-color: color-mix(in srgb, var(--line) 55%, transparent);
    }
    .ctox-chat-date-pill {
      display: inline-flex;
      align-items: center;
      justify-content: space-between;
      height: 30px;
      width: 115px;
      min-width: 115px;
      border: 1px solid color-mix(in srgb, var(--line) 20%, transparent);
      border-radius: 15px;
      background: color-mix(in srgb, var(--surface) 15%, transparent);
      padding: 0 2px;
      box-sizing: border-box;
      gap: 2px;
      transition: border-color 0.25s ease, background-color 0.25s ease;
    }
    .ctox-chat-date-pill:hover {
      border-color: color-mix(in srgb, var(--line) 55%, transparent);
      background: color-mix(in srgb, var(--surface) 35%, transparent);
    }
    .ctox-date-nav-btn {
      display: flex;
      align-items: center;
      justify-content: center;
      width: 22px;
      height: 22px;
      border: none;
      border-radius: 50%;
      background: transparent;
      color: var(--muted);
      cursor: pointer;
      transition: transform 0.2s var(--spring-bounce), background-color 0.2s ease, color 0.2s ease;
      padding: 0;
    }
    .ctox-date-nav-btn:hover {
      background: color-mix(in srgb, var(--surface-2) 60%, transparent);
      color: var(--text);
      transform: scale(1.05);
    }
    .ctox-date-nav-btn:active {
      transform: scale(0.95);
    }
    .ctox-date-picker-trigger {
      position: relative;
      display: flex;
      align-items: center;
      justify-content: center;
      gap: 4px;
      flex: 1;
      height: 22px;
      border-radius: 11px;
      color: var(--text);
      cursor: pointer;
      min-width: 0;
      padding: 0 2px;
      transition: background-color 0.2s ease;
    }
    .ctox-date-picker-trigger:hover {
      background: color-mix(in srgb, var(--surface-2) 40%, transparent);
    }
    .ctox-date-label {
      font-size: 10px;
      font-weight: 600;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
      color: var(--text);
    }
    .ctox-date-picker-trigger svg {
      flex-shrink: 0;
      color: var(--muted);
      transition: color 0.2s ease;
    }
    .ctox-date-picker-trigger:hover svg {
      color: var(--text);
    }
    .ctox-date-native-picker {
      position: absolute;
      bottom: 38px;
      left: 50%;
      transform: translateX(-50%);
      width: 115px;
      height: 1px;
      opacity: 0;
      pointer-events: none;
      -webkit-appearance: none;
      appearance: none;
      z-index: 10;
    }
    .ctox-chat-root.is-collapsed {
      right: auto;
      width: auto;
      max-width: none;
    }
    .ctox-chat-dock.is-collapsed {
      grid-template-columns: 88px 115px;
      width: auto;
    }
    .ctox-chat-dock.is-collapsed .ctox-chat-nav,
    .ctox-chat-dock.is-collapsed .ctox-chat-strip,
    .ctox-chat-dock.is-collapsed .ctox-chat-new {
      display: none !important;
    }
    .ctox-chat-root.is-collapsed .ctox-chat-stage {
      display: none;
    }
    .ctox-chat-root.is-collapsed .ctox-chat-window {
      display: none !important;
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
      cursor: pointer;
      transition: transform 0.3s var(--spring-bounce), background-color 0.2s ease, border-color 0.2s ease;
    }
    .ctox-chat-fab:hover {
      transform: translateY(-1px) scale(1.02);
      background: color-mix(in srgb, var(--accent) 15%, var(--surface));
    }
    .ctox-chat-fab:active {
      transform: scale(0.98);
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
      display: flex;
      align-items: center;
      justify-content: center;
      height: 30px;
      border: 1px solid color-mix(in srgb, var(--line) 30%, transparent);
      border-radius: 50%;
      background: color-mix(in srgb, var(--surface) 25%, transparent);
      color: var(--muted);
      cursor: pointer;
      transition: transform 0.3s var(--spring-bounce), background-color 0.25s ease, color 0.25s ease, border-color 0.25s ease;
    }
    .ctox-chat-nav {
      width: 28px;
    }
    .ctox-chat-new {
      width: 30px;
      border-color: color-mix(in srgb, var(--accent) 30%, transparent);
      background: color-mix(in srgb, var(--accent) 12%, transparent);
      color: var(--accent);
    }
    .ctox-chat-nav:hover,
    .ctox-chat-new:hover {
      transform: scale(1.1) translateY(-1px);
      background: color-mix(in srgb, var(--surface-2) 60%, transparent);
      color: var(--text);
    }
    .ctox-chat-new:hover {
      background: color-mix(in srgb, var(--accent) 20%, transparent);
    }
    .ctox-chat-nav:active,
    .ctox-chat-new:active {
      transform: scale(0.95);
    }
    .ctox-chat-strip {
      display: flex;
      align-items: center;
      gap: 6px;
      min-width: 0;
      overflow-x: auto;
      overscroll-behavior-x: contain;
      scrollbar-width: none;
      position: relative;
    }
    .ctox-chat-strip::-webkit-scrollbar {
      display: none;
    }
    .ctox-chat-chip {
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
      cursor: pointer;
      animation: ctoxChipSlideIn 0.3s cubic-bezier(0.34, 1.56, 0.64, 1) both;
      transition: transform 0.3s var(--spring-bounce), background-color 0.2s ease, border-color 0.2s ease, color 0.2s ease, box-shadow 0.3s var(--spring-bounce);
      --accent: var(--theme-accent, #10b981);
      --accent-soft: var(--theme-accent-soft, rgba(16, 185, 129, 0.12));
    }
    .ctox-chat-chip[data-chat-module="ctox"] {
      --accent: #10b981 !important;
      --accent-soft: rgba(16, 185, 129, 0.12) !important;
    }
    .ctox-chat-chip[data-chat-module="documents"] {
      --accent: #3b82f6 !important;
      --accent-soft: rgba(59, 130, 246, 0.12) !important;
    }
    .ctox-chat-chip[data-chat-module="knowledge"] {
      --accent: #a855f7 !important;
      --accent-soft: rgba(168, 85, 247, 0.12) !important;
    }
    .ctox-chat-chip[data-chat-module="research"] {
      --accent: #06b6d4 !important;
      --accent-soft: rgba(6, 182, 212, 0.12) !important;
    }
    .ctox-chat-chip[data-chat-module="matching"] {
      --accent: #f59e0b !important;
      --accent-soft: rgba(245, 158, 11, 0.12) !important;
    }
    .ctox-chat-chip[data-chat-module="reports"] {
      --accent: #ef4444 !important;
      --accent-soft: rgba(239, 68, 68, 0.12) !important;
    }
    .ctox-chat-chip[data-chat-module="conversations"] {
      --accent: #6366f1 !important;
      --accent-soft: rgba(99, 102, 241, 0.12) !important;
    }
    .ctox-chat-chip[data-chat-module="outbound"] {
      --accent: #f43f5e !important;
      --accent-soft: rgba(244, 63, 94, 0.12) !important;
    }
    .ctox-chat-chip:hover {
      transform: translateY(-1.5px);
      background: color-mix(in srgb, var(--surface) 35%, transparent);
      color: var(--text);
    }
    .ctox-chat-chip.is-minimized {
      border-color: color-mix(in srgb, var(--line) 30%, transparent) !important;
      background: color-mix(in srgb, var(--surface) 30%, transparent) !important;
      color: var(--muted) !important;
      box-shadow: none !important;
      opacity: 0.75 !important;
      transform: none !important;
    }
    .ctox-chat-chip.is-minimized:hover {
      border-color: color-mix(in srgb, var(--line) 45%, transparent) !important;
      background: color-mix(in srgb, var(--surface-2) 40%, transparent) !important;
      color: var(--text) !important;
      opacity: 0.98 !important;
      transform: translateY(-1px) !important;
    }
    .ctox-chat-chip.is-expanded:not(.is-active) {
      border-color: color-mix(in srgb, var(--accent) 60%, transparent);
      background: color-mix(in srgb, var(--accent) 26%, var(--surface-2));
      color: color-mix(in srgb, var(--text) 95%, var(--accent));
      opacity: 0.96;
    }
    .ctox-chat-chip.is-active {
      border-color: var(--accent);
      background: color-mix(in srgb, var(--accent) 26%, var(--surface-2));
      color: var(--text);
      box-shadow: 0 4px 12px color-mix(in srgb, var(--accent) 30%, transparent), 0 0 0 1px var(--accent) inset;
      opacity: 1 !important;
      transform: translateY(-1px) scale(1.02);
      animation: ctoxChipActivePulse 0.4s var(--spring-ease) both;
    }
    .ctox-chat-chip-mark {
      display: flex;
      align-items: center;
      justify-content: center;
      width: 12px;
      height: 12px;
      border-radius: 50%;
      background: var(--accent) !important;
      box-shadow: 0 0 6px var(--accent);
      transition: background-color 0.25s ease, transform 0.25s var(--spring-bounce), box-shadow 0.25s ease;
      color: #fff;
      flex-shrink: 0;
    }
    .ctox-chat-chip-mark svg {
      display: block;
      width: 8px;
      height: 8px;
    }
    .ctox-chat-chip.is-active .ctox-chat-chip-mark {
      transform: scale(1.1);
    }
    .ctox-chat-chip.is-minimized .ctox-chat-chip-mark {
      transform: scale(0.9) !important;
      background: color-mix(in srgb, var(--muted) 50%, transparent) !important;
      box-shadow: none !important;
      animation: none !important;
    }
    .ctox-chat-chip.is-minimized .ctox-chip-spinner {
      display: none !important;
    }

    /* State-colored marks */
    .ctox-chat-chip-mark.is-running {
      background: var(--accent) !important;
      position: relative;
    }
    @keyframes ctoxChipSpin {
      100% { transform: rotate(360deg); }
    }
    .ctox-chip-spinner {
      display: block;
      width: 8px;
      height: 8px;
      border: 1.5px solid rgba(255, 255, 255, 0.3);
      border-top-color: #fff;
      border-radius: 50%;
      animation: ctoxChipSpin 1s linear infinite;
    }
    .ctox-chat-chip-mark.is-queued {
      background: #f59e0b !important;
      box-shadow: 0 0 6px #f59e0b;
      animation: ctoxPulseQueuedDot 1.5s infinite ease-in-out;
    }
    @keyframes ctoxPulseQueuedDot {
      0% { transform: scale(1); opacity: 0.7; }
      50% { transform: scale(1.25); opacity: 1; }
      100% { transform: scale(1); opacity: 0.7; }
    }
    .ctox-chat-chip-mark.is-success {
      background: #10b981 !important;
      box-shadow: 0 0 6px #10b981;
    }
    .ctox-chat-chip-mark.is-failed {
      background: #ef4444 !important;
      box-shadow: 0 0 6px #ef4444;
      animation: ctoxPulseFailedDot 1.5s infinite ease-in-out;
    }
    @keyframes ctoxPulseFailedDot {
      0% { transform: scale(1); }
      50% { transform: scale(1.2); }
      100% { transform: scale(1); }
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
      grid-template-columns: 88px 115px 28px minmax(0, 1fr) 28px 34px;
      align-items: end;
      gap: 8px;
      box-sizing: border-box;
      min-width: 0;
      overflow: hidden;
      padding: 0 6px;
    }
    .ctox-chat-stage-inner {
      grid-column: 4;
      position: relative;
      overflow: visible;
      height: min(340px, calc(100vh - 132px));
      transition: height 0.3s var(--spring-bounce);
      min-width: 0;
      pointer-events: none;
      padding: 24px 0 10px 0;
      box-sizing: border-box;
      perspective: 1200px;
      transform-style: preserve-3d;
    }
    .ctox-chat-stage-inner.has-maximized {
      height: min(480px, calc(100vh - 132px));
    }
    .ctox-chat-stage::-webkit-scrollbar {
      display: none;
    }
    .ctox-chat-stage-inner::-webkit-scrollbar {
      display: none;
    }
    .ctox-chat-window {
      position: absolute;
      bottom: 10px;
      z-index: 61;
      pointer-events: auto;
      display: grid;
      grid-template-rows: 38px minmax(0, 1fr) auto;
      width: 264px;
      height: min(320px, calc(100vh - 132px));
      min-width: min(264px, calc(100vw - 24px));
      overflow: hidden;
      box-sizing: border-box;
      max-width: min(390px, calc(100vw - 24px));
      border: 1px solid color-mix(in srgb, var(--line) 25%, transparent);
      border-radius: 16px;
      background: color-mix(in srgb, var(--surface) 60%, transparent);
      backdrop-filter: blur(24px) saturate(180%);
      -webkit-backdrop-filter: blur(24px) saturate(180%);
      color: var(--text);
      box-shadow: 0 20px 48px rgba(0, 0, 0, 0.12), 0 1px 0 rgba(255, 255, 255, 0.08) inset;
      font-family: var(--font-family, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif);
      font-size: 12px;
      line-height: 1.4;
      animation: ctoxChatSlideIn 0.35s cubic-bezier(0.34, 1.56, 0.64, 1) both;
      flex-shrink: 0;
      transition: 
        left 0.28s var(--spring-ease),
        width 0.3s var(--spring-bounce),
        height 0.3s var(--spring-bounce),
        opacity 0.25s ease,
        transform 0.35s var(--spring-bounce),
        border-color 0.25s ease,
        box-shadow 0.25s ease,
        filter 0.25s ease;
      --accent: var(--theme-accent, #10b981);
      --accent-soft: var(--theme-accent-soft, rgba(16, 185, 129, 0.12));
      transform-style: preserve-3d;
      backface-visibility: hidden;
    }
    .ctox-chat-window[data-chat-module="ctox"] {
      --accent: #10b981 !important;
      --accent-soft: rgba(16, 185, 129, 0.12) !important;
    }
    .ctox-chat-window[data-chat-module="documents"] {
      --accent: #3b82f6 !important;
      --accent-soft: rgba(59, 130, 246, 0.12) !important;
    }
    .ctox-chat-window[data-chat-module="knowledge"] {
      --accent: #a855f7 !important;
      --accent-soft: rgba(168, 85, 247, 0.12) !important;
    }
    .ctox-chat-window[data-chat-module="research"] {
      --accent: #06b6d4 !important;
      --accent-soft: rgba(6, 182, 212, 0.12) !important;
    }
    .ctox-chat-window[data-chat-module="matching"] {
      --accent: #f59e0b !important;
      --accent-soft: rgba(245, 158, 11, 0.12) !important;
    }
    .ctox-chat-window[data-chat-module="reports"] {
      --accent: #ef4444 !important;
      --accent-soft: rgba(239, 68, 68, 0.12) !important;
    }
    .ctox-chat-window[data-chat-module="conversations"] {
      --accent: #6366f1 !important;
      --accent-soft: rgba(99, 102, 241, 0.12) !important;
    }
    .ctox-chat-window[data-chat-module="outbound"] {
      --accent: #f43f5e !important;
      --accent-soft: rgba(244, 63, 94, 0.12) !important;
    }
    .ctox-chat-window:not(.is-active) {
      opacity: 0.6;
    }
    .ctox-chat-window:not(.is-active)[data-chat-rel="left"] {
      transform: rotateY(32deg) scale(0.8) translateZ(-160px) translateY(18px);
    }
    .ctox-chat-window:not(.is-active)[data-chat-rel="right"] {
      transform: rotateY(-32deg) scale(0.8) translateZ(-160px) translateY(18px);
    }
    .ctox-chat-window:not(.is-active)[data-chat-rel="center"] {
      transform: scale(0.8) translateZ(-160px) translateY(18px);
    }
    .ctox-chat-window:not(.is-active) * {
      pointer-events: none !important;
    }
    .ctox-chat-window:not(.is-active):hover {
      opacity: 0.85;
      filter: none;
      z-index: 64;
    }
    .ctox-chat-window:not(.is-active)[data-chat-rel="left"]:hover {
      transform: rotateY(12deg) scale(0.9) translateZ(-40px) translateY(6px);
    }
    .ctox-chat-window:not(.is-active)[data-chat-rel="right"]:hover {
      transform: rotateY(-12deg) scale(0.9) translateZ(-40px) translateY(6px);
    }
    .ctox-chat-window:not(.is-active)[data-chat-rel="center"]:hover {
      transform: scale(0.9) translateY(6px);
    }
    
    @keyframes ctoxActiveFocusSpotlight {
      0% {
        transform: scale(0.99) translateY(1px);
        box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1), 0 0 0 1px var(--accent) inset;
      }
      100% {
        transform: scale(1) translateY(0);
        box-shadow: 0 16px 36px rgba(0, 0, 0, 0.15), 0 0 0 1px var(--accent) inset, 0 0 12px color-mix(in srgb, var(--accent) 20%, transparent);
      }
    }
    .ctox-chat-window.is-active {
      border-color: var(--accent);
      box-shadow: 0 16px 36px rgba(0, 0, 0, 0.15), 0 0 0 1px var(--accent) inset, 0 0 12px color-mix(in srgb, var(--accent) 20%, transparent);
      z-index: 65;
      opacity: 1;
      filter: none;
      transform: scale(1) translateZ(0px) translateY(0);
      animation: ctoxActiveFocusSpotlight 0.4s var(--spring-ease) both;
    }
    .ctox-chat-window.is-active.is-task-running {
      animation: ctoxActiveFocusSpotlight 0.4s var(--spring-ease) both, ctoxPulseRunning 2s infinite ease-in-out 0.4s;
    }
    .ctox-chat-window.is-active.is-task-queued {
      animation: ctoxActiveFocusSpotlight 0.4s var(--spring-ease) both, ctoxPulseQueued 2s infinite ease-in-out 0.4s;
    }
    .ctox-chat-window.is-active.is-task-failed {
      animation: ctoxActiveFocusSpotlight 0.4s var(--spring-ease) both, ctoxPulseFailed 2.5s infinite ease-in-out 0.4s;
    }
    .ctox-chat-window.is-maximized {
      width: 390px !important;
      height: min(460px, calc(100vh - 132px)) !important;
    }
    .ctox-chat-window.is-minimized {
      opacity: 0 !important;
      pointer-events: none !important;
      transform: translateY(30px) scale(0.9) !important;
    }
    .ctox-chat-window.no-left-transition {
      transition: 
        width 0.3s var(--spring-bounce),
        height 0.3s var(--spring-bounce),
        opacity 0.25s ease,
        transform 0.35s var(--spring-bounce),
        border-color 0.25s ease,
        box-shadow 0.25s ease !important;
    }

    /* State-based animations and glows */
    @keyframes ctoxPulseRunning {
      0% {
        border-color: color-mix(in srgb, var(--accent) 50%, var(--line));
        box-shadow: 0 20px 48px rgba(0, 0, 0, 0.18), 0 0 12px color-mix(in srgb, var(--accent) 20%, transparent);
      }
      50% {
        border-color: var(--accent);
        box-shadow: 0 20px 48px rgba(0, 0, 0, 0.22), 0 0 24px color-mix(in srgb, var(--accent) 45%, transparent);
      }
      100% {
        border-color: color-mix(in srgb, var(--accent) 50%, var(--line));
        box-shadow: 0 20px 48px rgba(0, 0, 0, 0.18), 0 0 12px color-mix(in srgb, var(--accent) 20%, transparent);
      }
    }
    @keyframes ctoxPulseQueued {
      0% {
        border-color: rgba(245, 158, 11, 0.4);
        box-shadow: 0 20px 48px rgba(0, 0, 0, 0.18), 0 0 10px rgba(245, 158, 11, 0.15);
      }
      50% {
        border-color: rgba(245, 158, 11, 0.95);
        box-shadow: 0 20px 48px rgba(0, 0, 0, 0.22), 0 0 20px rgba(245, 158, 11, 0.45);
      }
      100% {
        border-color: rgba(245, 158, 11, 0.4);
        box-shadow: 0 20px 48px rgba(0, 0, 0, 0.18), 0 0 10px rgba(245, 158, 11, 0.15);
      }
    }
    @keyframes ctoxPulseFailed {
      0% {
        border-color: rgba(239, 68, 68, 0.4);
        box-shadow: 0 20px 48px rgba(0, 0, 0, 0.18), 0 0 10px rgba(239, 68, 68, 0.15);
      }
      50% {
        border-color: rgba(239, 68, 68, 0.95);
        box-shadow: 0 20px 48px rgba(0, 0, 0, 0.22), 0 0 20px rgba(239, 68, 68, 0.45);
      }
      100% {
        border-color: rgba(239, 68, 68, 0.4);
        box-shadow: 0 20px 48px rgba(0, 0, 0, 0.18), 0 0 10px rgba(239, 68, 68, 0.15);
      }
    }

    .ctox-chat-window.is-task-running {
      animation: ctoxPulseRunning 2s infinite ease-in-out;
    }
    .ctox-chat-window.is-task-queued {
      animation: ctoxPulseQueued 2s infinite ease-in-out;
    }
    .ctox-chat-window.is-task-success {
      border-color: #10b981 !important;
      box-shadow: 0 20px 48px rgba(0, 0, 0, 0.2), 0 0 20px rgba(16, 185, 129, 0.35) !important;
    }
    .ctox-chat-window.is-task-failed {
      animation: ctoxPulseFailed 2.5s infinite ease-in-out;
    }

    .ctox-chat-window header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 8px;
      border-bottom: 1px solid color-mix(in srgb, var(--line) 30%, transparent);
      background: color-mix(in srgb, var(--surface) 20%, transparent);
      padding: 0 6px 0 10px;
      height: 38px;
    }
    .ctox-chat-header-actions {
      display: flex;
      align-items: center;
      gap: 4px;
      flex-shrink: 0;
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
      transition: transform 0.2s var(--spring-bounce), background-color 0.15s ease, color 0.15s ease, border-color 0.15s ease;
    }
    .ctox-chat-window header button:not(.ctox-chat-title):hover {
      transform: translateY(-1px) scale(1.05);
      background: color-mix(in srgb, var(--surface-2) 50%, transparent);
      border-color: color-mix(in srgb, var(--line) 40%, transparent);
      color: var(--text);
    }
    .ctox-chat-window header button:not(.ctox-chat-title):active {
      transform: scale(0.95);
    }
    .ctox-chat-window header button.is-delete:hover {
      background: rgba(239, 68, 68, 0.12) !important;
      border-color: rgba(239, 68, 68, 0.25) !important;
      color: #ef4444 !important;
    }
    .ctox-chat-title {
      display: flex !important;
      flex-direction: column !important;
      justify-content: center !important;
      align-items: flex-start !important;
      min-width: 0 !important;
      flex: 1 1 auto !important;
      max-width: calc(100% - 136px) !important;
      text-align: left !important;
      padding: 0 !important;
      width: auto !important;
      height: 100% !important;
      min-height: 0 !important;
      background: transparent !important;
      border: none !important;
      cursor: pointer !important;
      color: inherit !important;
      flex-shrink: 1 !important;
    }
    .ctox-chat-title:hover {
      border-color: transparent !important;
    }
    .ctox-chat-title strong,
    .ctox-chat-title span {
      display: block;
      width: 100%;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      max-width: 100%;
    }
    .ctox-chat-title strong {
      color: var(--text);
      font-size: 12px;
      font-weight: 760;
      flex: 1;
      min-width: 0;
    }
    .ctox-chat-title span {
      color: var(--muted);
      font-size: 10px;
    }
    .ctox-chat-messages {
      display: flex;
      flex-direction: column;
      gap: 8px;
      overflow: auto;
      padding: 12px;
      background: transparent;
      scrollbar-width: thin;
      min-width: 0;
      max-width: 100%;
      box-sizing: border-box;
    }
    .ctox-chat-messages::-webkit-scrollbar {
      width: 4px;
    }
    .ctox-chat-messages::-webkit-scrollbar-track {
      background: transparent;
    }
    .ctox-chat-messages::-webkit-scrollbar-thumb {
      background: color-mix(in srgb, var(--line) 40%, transparent);
      border-radius: 99px;
    }
    .ctox-chat-messages::-webkit-scrollbar-thumb:hover {
      background: color-mix(in srgb, var(--line) 60%, transparent);
    }
    .ctox-chat-empty {
      margin: auto;
      color: var(--muted);
      font-weight: 550;
      opacity: 0.6;
      font-size: 11px;
      letter-spacing: 0.3px;
    }
    .ctox-chat-message {
      max-width: 88%;
      word-break: break-word;
      overflow-wrap: anywhere;
      min-width: 0;
      display: block;
      box-sizing: border-box;
    }
    .ctox-chat-message.is-user {
      align-self: flex-end;
      background: color-mix(in srgb, var(--accent) 15%, var(--surface-2)) !important;
      border: none !important;
      box-shadow: 0 4px 12px rgba(0, 0, 0, 0.03) !important;
      border-radius: 14px 14px 4px 14px !important;
      padding: 8px 12px !important;
      max-width: 88%;
    }
    .ctox-chat-message.is-ctox {
      align-self: flex-start;
      background: transparent !important;
      box-shadow: none !important;
      border: none !important;
      border-left: 2px solid var(--accent) !important;
      border-radius: 0 !important;
      padding: 4px 0 4px 12px !important;
      margin-left: 4px;
      margin-right: 12px;
      max-width: 88%;
    }
    .ctox-chat-message p {
      margin: 0;
      white-space: pre-wrap;
      word-break: break-word;
      overflow-wrap: anywhere;
      max-width: 100%;
    }
    .ctox-chat-body {
      margin: 0;
      max-width: 100%;
      min-width: 0;
      word-break: break-word;
      overflow-wrap: anywhere;
      white-space: normal;
    }
    .ctox-chat-body .ctox-chat-text {
      display: block;
      max-width: 100%;
      min-width: 0;
      white-space: pre-wrap;
      word-break: break-word;
      overflow-wrap: anywhere;
    }
    .ctox-chat-body code {
      font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
      font-size: 0.92em;
      background: color-mix(in srgb, var(--accent) 12%, var(--surface));
      border-radius: 5px;
      padding: 1px 5px;
      white-space: normal;
      word-break: break-word;
      overflow-wrap: anywhere;
    }
    .ctox-chat-body pre.ctox-chat-code {
      margin: 6px 0;
      padding: 8px 10px;
      border-radius: 8px;
      background: color-mix(in srgb, var(--line) 22%, var(--surface));
      border: 1px solid color-mix(in srgb, var(--line) 40%, transparent);
      overflow-x: auto;
      max-width: 100%;
    }
    .ctox-chat-body pre.ctox-chat-code code {
      background: none;
      padding: 0;
      white-space: pre-wrap;
      font-size: 0.88em;
      line-height: 1.45;
      word-break: break-word;
      overflow-wrap: anywhere;
    }
    .ctox-chat-body a {
      color: var(--accent);
      text-decoration: underline;
      word-break: break-word;
      overflow-wrap: anywhere;
    }
    .ctox-chat-message footer {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: 6px;
      margin-top: 6px;
      color: var(--muted);
      font-size: 11px;
      max-width: 100%;
      min-width: 0;
    }
    .ctox-chat-message footer span {
      max-width: 100%;
      overflow-wrap: anywhere;
      word-break: break-word;
      white-space: normal;
      min-width: 0;
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
      max-width: 100%;
      min-width: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      display: inline-block;
      vertical-align: middle;
      box-sizing: border-box;
    }
    .ctox-chat-form {
      display: flex;
      align-items: center;
      min-width: 0;
      border: none !important;
      border-top: 1px solid color-mix(in srgb, var(--line) 20%, transparent) !important;
      border-radius: 0 !important;
      background: color-mix(in srgb, var(--surface) 25%, transparent) !important;
      margin: 0 !important;
      padding: 8px 12px !important;
      transition: background-color 0.25s ease;
      box-sizing: border-box;
    }
    .ctox-chat-form:focus-within {
      background: color-mix(in srgb, var(--surface-2) 40%, transparent) !important;
    }
    .ctox-chat-form textarea {
      flex: 1;
      min-width: 0;
      resize: none;
      border: none !important;
      background: transparent !important;
      color: var(--text);
      min-height: 20px;
      max-height: 120px;
      padding: 4px 0;
      outline: none !important;
      box-shadow: none !important;
      font-size: 12px;
      line-height: 1.4;
      overflow-y: auto;
    }
    .ctox-chat-form textarea::placeholder {
      color: var(--muted);
      opacity: 0.55;
    }
    .ctox-chat-form button {
      display: flex;
      align-items: center;
      justify-content: center;
      border: none;
      border-radius: 50%;
      background: var(--accent);
      color: var(--bg);
      cursor: pointer;
      width: 26px;
      height: 26px;
      min-width: 26px;
      min-height: 26px;
      padding: 0;
      transition: transform 0.2s var(--spring-bounce), filter 0.15s ease;
      align-self: flex-end;
    }
    .ctox-chat-form button:hover {
      transform: scale(1.08) translateY(-0.5px);
      filter: brightness(1.1);
    }
    .ctox-chat-form button:active {
      transform: scale(0.95);
    }

    /* Active Delegation Card styling */
    .ctox-chat-delegation-card {
      position: relative;
      margin: 0 !important;
      padding: 10px 12px;
      border: none !important;
      border-top: 1px solid color-mix(in srgb, var(--accent) 20%, transparent) !important;
      border-radius: 0 !important;
      background: color-mix(in srgb, var(--accent) 5%, var(--surface)) !important;
      display: flex;
      flex-direction: column;
      gap: 10px;
      overflow: hidden;
      box-shadow: none !important;
      min-width: 0;
      max-width: 100%;
      box-sizing: border-box;
    }
    .ctox-delegation-glow {
      position: absolute;
      top: -50%;
      left: -50%;
      width: 200%;
      height: 200%;
      background: radial-gradient(circle, color-mix(in srgb, var(--accent) 8%, transparent) 0%, transparent 60%);
      pointer-events: none;
      animation: ctoxGlowRotate 6s linear infinite;
    }
    @keyframes ctoxGlowRotate {
      100% { transform: rotate(360deg); }
    }
    .ctox-delegation-header {
      display: flex;
      align-items: center;
      gap: 10px;
      z-index: 1;
      min-width: 0;
    }
    @keyframes ctoxSpin {
      100% { transform: rotate(360deg); }
    }
    .ctox-delegation-spinner {
      display: block;
      width: 14px;
      height: 14px;
      border: 2px solid color-mix(in srgb, var(--accent) 25%, transparent);
      border-top-color: var(--accent);
      border-radius: 50%;
      animation: ctoxSpin 0.8s linear infinite;
    }
    .ctox-delegation-info {
      display: flex;
      flex-direction: column;
      gap: 1px;
      min-width: 0;
    }
    .ctox-delegation-info strong {
      font-size: 11px;
      font-weight: 760;
      color: var(--text);
      overflow-wrap: anywhere;
      word-break: break-word;
    }
    .ctox-delegation-info span {
      font-size: 10px;
      color: var(--muted);
      overflow-wrap: anywhere;
      word-break: break-word;
    }
    .ctox-delegation-watch-btn {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: 6px;
      width: 100%;
      min-width: 0;
      height: 28px;
      border: 1px solid color-mix(in srgb, var(--accent) 35%, var(--line));
      border-radius: 8px;
      background: color-mix(in srgb, var(--accent) 12%, var(--surface));
      color: var(--accent);
      font-size: 11px;
      font-weight: 760;
      cursor: pointer;
      z-index: 1;
      transition: transform 0.2s var(--spring-bounce), background-color 0.2s ease, border-color 0.2s ease;
    }
    .ctox-delegation-watch-btn span {
      min-width: 0;
      overflow-wrap: anywhere;
      word-break: break-word;
    }
    .ctox-delegation-watch-btn:hover {
      transform: translateY(-1px);
      background: color-mix(in srgb, var(--accent) 18%, var(--surface));
      border-color: var(--accent);
    }
    .ctox-delegation-watch-btn:active {
      transform: scale(0.97);
    }
    
    /* Follow Up Button styling */
    .ctox-followup-container {
      margin: 0 !important;
      padding: 8px 12px !important;
      border-top: 1px solid color-mix(in srgb, var(--accent) 20%, transparent) !important;
      background: color-mix(in srgb, var(--accent) 3%, transparent) !important;
    }
    .ctox-followup-btn {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: 8px;
      width: 100%;
      height: 32px;
      border: none !important;
      border-radius: 8px !important;
      background: color-mix(in srgb, var(--accent) 12%, var(--surface-2)) !important;
      color: var(--accent) !important;
      font-size: 11px !important;
      font-weight: 700 !important;
      cursor: pointer;
      transition: transform 0.3s var(--spring-bounce), background-color 0.2s ease, box-shadow 0.2s ease;
    }
    .ctox-followup-btn:hover {
      transform: translateY(-1px);
      background: color-mix(in srgb, var(--accent) 18%, var(--surface-2)) !important;
      box-shadow: 0 4px 12px color-mix(in srgb, var(--accent) 20%, transparent);
    }
    .ctox-followup-btn:active {
      transform: scale(0.97);
    }
    
    /* Status Badge in Header styling */
    .ctox-chat-status-badge {
      display: inline-flex;
      align-items: center;
      gap: 4px;
      padding: 2px 6px;
      border-radius: 6px;
      font-size: 9px;
      font-weight: 760;
      text-transform: uppercase;
      letter-spacing: 0.3px;
      backdrop-filter: blur(6px);
      -webkit-backdrop-filter: blur(6px);
    }
    .ctox-chat-status-badge.is-running {
      border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
      background: color-mix(in srgb, var(--accent) 10%, transparent);
      color: var(--accent);
    }
    .ctox-chat-status-badge.is-running .ctox-status-spinner {
      display: block;
      width: 7px;
      height: 7px;
      border: 1.5px solid color-mix(in srgb, var(--accent) 25%, transparent);
      border-top-color: var(--accent);
      border-radius: 50%;
      animation: ctoxSpin 0.8s linear infinite;
    }
    .ctox-chat-status-badge.is-queued {
      border: 1px solid rgba(245, 158, 11, 0.3);
      background: rgba(245, 158, 11, 0.1);
      color: #f59e0b;
    }
    .ctox-chat-status-badge.is-queued .ctox-status-dot {
      display: block;
      width: 6px;
      height: 6px;
      border-radius: 50%;
      background: #f59e0b;
      animation: ctoxPulseQueuedDot 1.5s infinite ease-in-out;
    }
    .ctox-chat-status-badge.is-success {
      border: 1px solid rgba(16, 185, 129, 0.3);
      background: rgba(16, 185, 129, 0.1);
      color: #10b981;
    }
    .ctox-chat-status-badge.is-failed {
      border: 1px solid rgba(239, 68, 68, 0.3);
      background: rgba(239, 68, 68, 0.1);
      color: #ef4444;
    }

    @media (max-width: 780px) {
      .ctox-chat-root {
        right: 18px;
        width: auto;
        max-width: calc(100vw - 36px);
      }
      .ctox-chat-dock {
        display: flex !important;
        align-items: center !important;
        justify-content: flex-start !important;
        gap: 6px !important;
        overflow-x: auto !important;
        width: 100% !important;
        scrollbar-width: none !important;
      }
      .ctox-chat-dock::-webkit-scrollbar {
        display: none !important;
      }
      .ctox-chat-strip {
        flex: 1 1 auto !important;
        min-width: 0 !important;
      }
      .ctox-chat-stage {
        display: block !important;
        width: 100% !important;
        padding: 0 !important;
      }
      .ctox-chat-stage-inner {
        grid-column: auto !important;
        display: flex !important;
        flex-direction: row !important;
        overflow-x: auto !important;
        scroll-snap-type: x mandatory !important;
        gap: 12px !important;
        width: 100% !important;
        padding: 8px 0 !important;
      }
      .ctox-chat-window {
        position: relative !important;
        flex: 0 0 100% !important;
        width: 100% !important;
        min-width: 100% !important;
        scroll-snap-align: center !important;
        left: auto !important;
        bottom: 0 !important;
      }
    }

    /* Scheduled Task and Timer Styles */
    .ctox-chat-status-badge.is-scheduled {
      border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
      background: color-mix(in srgb, var(--accent) 8%, transparent);
      color: var(--accent);
    }
    
    @keyframes ctoxClockRotate {
      0% { transform: rotate(0deg); }
      100% { transform: rotate(360deg); }
    }
    
    .ctox-clock-pulse {
      animation: ctoxPulseQueuedDot 2s infinite ease-in-out;
    }
    
    .ctox-chat-scheduler-bar {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 6px 12px;
      background: color-mix(in srgb, var(--surface) 25%, transparent);
      border-bottom: 1px solid color-mix(in srgb, var(--line) 20%, transparent);
      font-size: 10.5px;
      color: var(--muted);
      gap: 4px;
    }
    
    .ctox-chat-time-input {
      border: 1px solid color-mix(in srgb, var(--line) 40%, transparent);
      border-radius: 4px;
      background: var(--surface-2);
      color: var(--text);
      font-size: 10px;
      padding: 1px 4px;
      outline: none;
      width: 54px;
      transition: border-color 0.2s ease;
    }
    .ctox-chat-time-input:focus {
      border-color: var(--accent);
    }
    
    .ctox-chat-scheduler-card {
      position: relative;
      overflow: hidden;
      display: flex;
      flex-direction: column;
      gap: 8px;
      margin: 8px 12px;
      padding: 10px 12px;
      border: 1px dashed color-mix(in srgb, var(--accent) 40%, var(--line));
      border-radius: 10px;
      background: color-mix(in srgb, var(--accent) 4%, transparent);
      box-shadow: 0 4px 12px rgba(0,0,0,0.02);
    }
    
    .ctox-scheduler-glow {
      position: absolute;
      top: -30px;
      right: -30px;
      width: 80px;
      height: 80px;
      background: radial-gradient(circle, color-mix(in srgb, var(--accent) 25%, transparent) 0%, transparent 70%);
      pointer-events: none;
    }
    
    .ctox-scheduler-header {
      display: flex;
      align-items: center;
      gap: 8px;
    }
    
    .ctox-clock-spinner {
      color: var(--accent);
      animation: ctoxClockRotate 8s linear infinite;
    }
    
    .ctox-scheduler-info {
      display: flex;
      flex-direction: column;
      min-width: 0;
    }
    
    .ctox-scheduler-info strong {
      font-size: 11px;
      font-weight: 760;
      color: var(--text);
    }
    
    .ctox-scheduler-info span {
      font-size: 9.5px;
      color: var(--muted);
    }
    
    .ctox-scheduler-timer-container {
      display: flex;
      align-items: center;
      gap: 6px;
      background: color-mix(in srgb, var(--accent) 8%, transparent);
      padding: 5px 8px;
      border-radius: 6px;
      width: fit-content;
    }
    
    .ctox-scheduler-timer-badge {
      font-size: 9px;
      text-transform: uppercase;
      font-weight: 600;
      color: var(--muted);
    }
    
    .ctox-scheduler-timer {
      font-size: 13px;
      font-family: monospace;
      color: var(--accent);
      font-weight: 700;
      letter-spacing: 0.5px;
    }
    
    .ctox-scheduler-cancel-btn {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: 6px;
      height: 26px;
      border: 1px solid color-mix(in srgb, var(--line) 40%, transparent);
      border-radius: 6px;
      background: var(--surface-2);
      color: var(--muted);
      font-size: 10.5px;
      font-weight: 600;
      cursor: pointer;
      width: fit-content;
      padding: 0 8px;
      transition: all 0.2s ease;
    }
    
    .ctox-scheduler-cancel-btn:hover {
      background: color-mix(in srgb, var(--accent) 10%, var(--surface-2));
      color: var(--accent);
      border-color: color-mix(in srgb, var(--accent) 30%, transparent);
    }
    
    /* Attachment styles */
    .ctox-chat-attachments-preview {
      display: flex;
      flex-wrap: wrap;
      gap: 6px;
      padding: 8px 10px;
      background: var(--surface-2);
      border-top: 1px solid var(--line);
      border-bottom: 1px solid var(--line);
      max-height: 120px;
      overflow-y: auto;
    }
    
    .ctox-attachment-item {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      background: var(--surface);
      border: 1px solid var(--line);
      border-radius: 6px;
      padding: 4px 6px;
      font-size: 11px;
      max-width: 140px;
      position: relative;
    }
    
    .ctox-attachment-thumbnail {
      width: 18px;
      height: 18px;
      object-fit: cover;
      border-radius: 3px;
    }
    
    .ctox-attachment-icon {
      font-size: 12px;
    }
    
    .ctox-attachment-name {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      flex: 1;
      color: var(--text);
    }
    
    .ctox-attachment-remove {
      background: none;
      border: none;
      color: var(--muted);
      font-size: 14px;
      line-height: 1;
      cursor: pointer;
      padding: 0 2px;
      transition: color 0.15s ease;
    }
    
    .ctox-attachment-remove:hover {
      color: var(--accent);
    }
    
    .ctox-chat-clip-btn {
      background: none;
      border: none;
      color: var(--muted);
      cursor: pointer;
      display: flex;
      align-items: center;
      justify-content: center;
      width: 32px;
      height: 32px;
      border-radius: 6px;
      transition: all 0.2s ease;
      flex-shrink: 0;
      padding: 0;
    }
    
    .ctox-chat-clip-btn:hover {
      background: var(--surface-2);
      color: var(--accent);
    }
    
    /* Drag & Drop overlay */
    .ctox-chat-drag-overlay {
      display: none;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 12px;
      background: color-mix(in srgb, var(--accent) 92%, black);
      color: white;
      z-index: 100;
      position: absolute;
      inset: 0;
      pointer-events: none;
      border-radius: 12px;
      opacity: 0.95;
    }
    
    .ctox-chat-window.drag-active .ctox-chat-drag-overlay {
      display: flex;
    }
    
    .ctox-chat-drag-overlay svg {
      animation: ctoxClockPulse 2s infinite ease-in-out;
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

function fileToBase64(file) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.readAsDataURL(file);
    reader.onload = () => resolve(reader.result);
    reader.onerror = (error) => reject(error);
  });
}

async function addAttachmentToChatState(chat, file) {
  if (file.size > 8 * 1024 * 1024) {
    alert("Datei ist zu groß. Maximale Dateigröße beträgt 8MB.");
    return;
  }
  if (!chat.attachments) {
    chat.attachments = [];
  }
  if (chat.attachments.some((a) => a.name === file.name && a.size === file.size)) {
    return;
  }
  try {
    const base64Data = await fileToBase64(file);
    chat.attachments.push({
      name: file.name,
      mimeType: file.type || 'application/octet-stream',
      size: file.size,
      base64Data,
    });
  } catch (err) {
    console.error("Fehler beim Konvertieren der Datei zu Base64", err);
  }
}

// ----------------------------------------------------
// Future Chats & Countdown Timer Scheduler Helpers
// ----------------------------------------------------

function getFormattedTime(timestamp) {
  const d = new Date(timestamp);
  const hh = String(d.getHours()).padStart(2, '0');
  const mm = String(d.getMinutes()).padStart(2, '0');
  return `${hh}:${mm}`;
}

function getFormattedDateTime(timestamp) {
  const dateStr = getLocalDateString(timestamp);
  const dateLabel = formatGermanDateLabel(dateStr);
  const timeStr = getFormattedTime(timestamp);
  return `${dateLabel} um ${timeStr}`;
}

function getCountdownText(timestamp) {
  const diff = timestamp - Date.now();
  if (diff <= 0) return '00:00:00';
  const hours = Math.floor(diff / (1000 * 60 * 60));
  const minutes = Math.floor((diff % (1000 * 60 * 60)) / (1000 * 60));
  const seconds = Math.floor((diff % (1000 * 60)) / 1000);
  
  const hh = String(hours).padStart(2, '0');
  const mm = String(minutes).padStart(2, '0');
  const ss = String(seconds).padStart(2, '0');
  return `${hh}:${mm}:${ss}`;
}

function initSchedulerLoop({ root, state, commandBus, db, getActiveModule }) {
  if (window._ctoxChatSchedulerInterval) return;
  
  window._ctoxChatSchedulerInterval = setInterval(async () => {
    // 1. Update countdown displays in DOM
    const timerEls = root.querySelectorAll('[data-countdown-timer]');
    timerEls.forEach(el => {
      const chatId = el.dataset.countdownTimer;
      const chat = state.chats.find(c => c.id === chatId);
      if (chat) {
        el.textContent = getCountdownText(chat.createdAt);
      }
    });
    
    // 2. Check for scheduled chats whose time has arrived
    const nowMs = Date.now();
    let stateChanged = false;
    
    for (const chat of state.chats) {
      const scheduledMsgIdx = Array.isArray(chat.messages) 
        ? chat.messages.findIndex(m => m.status === 'scheduled') 
        : -1;
        
      if (scheduledMsgIdx >= 0 && chat.createdAt <= nowMs) {
        const scheduledMsg = chat.messages[scheduledMsgIdx];
        console.log(`[business-chat] Executing scheduled chat task for chat ${chat.id}`);
        
        scheduledMsg.status = 'pending_sync';
        const commandId = scheduledMsg.commandId || `cmd_${crypto.randomUUID()}`;
        chat.lastTrackingId = commandId;
        scheduledMsg.commandId = commandId;
        
        stateChanged = true;
        
        const text = scheduledMsg.text || '';
        const now = Date.now();
        const command = {
          id: commandId,
          module: chat.contextMeta?.module || 'ctox',
          type: chat.contextMeta?.command_type || 'business_os.chat.task',
          record_id: chat.id,
          inbound_channel: CHAT_CHANNEL,
          payload: {
            title: titleFromText(text),
            instruction: text,
            prompt: text,
            chat_id: chat.id,
            message_id: scheduledMsg.id,
            conversation: compactConversation(chat.messages),
            inbound_channel: CHAT_CHANNEL,
            outbound_channel: 'business_os_chat',
            response_channel: 'business_os_chat',
            reply_to: chat.id,
            thread_key: `business-os/chat/${chat.id}`,
            priority: 'normal',
            source_module: chat.contextMeta?.module || 'ctox',
          },
          client_context: {
            source: 'business-os-chat',
            module: chat.contextMeta?.module || 'ctox',
            source_module: chat.contextMeta?.module || 'ctox',
            source_title: chat.contextMeta?.source_title || 'CTOX',
            inbound_channel: CHAT_CHANNEL,
            outbound_channel: 'business_os_chat',
            chat_id: chat.id,
            message_id: scheduledMsg.id,
            url: location.href,
            language: document.documentElement.lang || 'de',
            created_at: new Date(now).toISOString(),
          },
        };
        
        (async () => {
          try {
            const result = await commandBus.dispatch(command);
            const taskId = result.task_id || '';
            const acceptedCommandId = result.command_id || commandId;
            chat.lastTrackingId = taskId || acceptedCommandId;
            
            const statusMsg = chat.messages.find(m => m.id === `status_${commandId}`);
            if (statusMsg) {
              statusMsg.text = taskId
                ? 'Task angelegt und in der CTOX Queue. Antwort erscheint hier, sobald der CTOX Service ihn verarbeitet.'
                : 'Command angelegt. Keine CTOX Queue-ID erhalten.';
              statusMsg.commandId = acceptedCommandId;
              statusMsg.taskId = taskId;
              statusMsg.status = result.task_status || result.status || 'queued';
            }
          } catch (error) {
            const failedCommandId = error?.command_id || error?.commandId || commandId;
            const statusMsg = chat.messages.find(m => m.id === `status_${commandId}`);
            if (statusMsg) {
              statusMsg.text = error?.message || String(error);
              statusMsg.commandId = failedCommandId;
              statusMsg.status = error?.status || 'failed';
            }
          }
          
          await persistChatState({ state, db });
          renderChatRoot({ root, state, commandBus, db, getActiveModule });
          
          syncTrackedMessages({ state, db }).then((changed) => {
            if (changed) persistChatState({ state, db });
            if (changed) renderChatRoot({ root, state, commandBus, db, getActiveModule });
          }).catch(() => {});
        })();
      }
    }
    
    if (stateChanged) {
      await persistChatState({ state, db });
      renderChatRoot({ root, state, commandBus, db, getActiveModule });
    }
  }, 1000);
}

async function cancelScheduledChat(state, chat, db, root, commandBus, getActiveModule) {
  const messages = chat.messages || [];
  const statusMsgIdx = [...messages].reverse().findIndex(m => m.role === 'ctox' && m.status === 'scheduled');
  if (statusMsgIdx >= 0) {
    const actualStatusIdx = messages.length - 1 - statusMsgIdx;
    const statusMsg = messages[actualStatusIdx];
    const userMsgIdx = messages.findIndex(m => m.role === 'user' && m.id === statusMsg.id.replace('status_', 'chatmsg_'));
    const actualUserIdx = userMsgIdx >= 0 ? userMsgIdx : actualStatusIdx - 1;
    
    if (actualUserIdx >= 0 && messages[actualUserIdx].role === 'user') {
      chat.draft = messages[actualUserIdx].text || '';
    }
    
    chat.messages = messages.filter((_, idx) => idx !== actualStatusIdx && idx !== actualUserIdx);
  }
  
  chat.lastTrackingId = '';
  touchChats(state, [chat]);
  await persistChatState({ state, db });
  renderChatRoot({ root, state, commandBus, db, getActiveModule });
}
