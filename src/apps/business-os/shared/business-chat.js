import { normalizeCrewAppearance, renderCrewCreature, CREW_CREATURE_BASE_CSS } from './crew-renderer.js';
import { showBusinessConfirm } from './dialogs.js?v=20260831-ctox-desktopapp-ports-v328';
import {
  FILE_CHUNK_HASH_SCHEME,
  FILE_CONTENT_HASH_SCHEME,
  base64ToBytes,
  sha256Hex,
} from './file-integrity.js?v=20260831-ctox-desktopapp-ports-v328';
import { renderGlobalCtoxAgentScopeHtml } from './shell-permissions-ui.js?v=20260831-ctox-desktopapp-ports-v328';
import {
  normalizeWorkjetCategory,
  workjetCategoryStyle,
} from './workjet-theme.js?v=20260903-entertainment-import-v336';

const CHAT_STYLE_ID = 'ctox-business-chat-style';
const CHAT_STATE_KEY = 'ctox.businessOs.chat.v1';
const CHAT_CHANNEL = 'business_os.llm.chat';
const CHAT_COLLECTION = 'business_chats';
const CHAT_OPEN_EVENT = 'ctox-business-os-chat-open';
const CHAT_LAYOUT_EVENT = 'ctox-business-os-chat-layout';
const MANY_CHAT_THRESHOLD = 12;
const MAX_RENDERED_CHAT_TABS = 12;
const MAX_BUSY_LIST_ITEMS = 80;
const MAX_BUSY_GROUPS = 24;
const CHAT_ATTACHMENT_MAX_BYTES = 8 * 1024 * 1024;
const CHAT_ATTACHMENT_CHUNK_SIZE = 16 * 1024;
const CHAT_ATTACHMENT_ROOT_ID = 'fs_business_os_chat_attachments';
const CHAT_ATTACHMENT_ROOT_PATH = '/Business OS Chat';
const CHAT_DELETE_TOMBSTONE_RETENTION_MS = 30 * 24 * 60 * 60 * 1000;
const ACTIVE_TRACKING_SYNC_INTERVAL_MS = 4000;
const CHAT_REMOTE_PERSIST_TIMEOUT_MS = 1500;
const CHAT_REMOTE_PERSIST_DEFER_MS = 0;
const CREW_NAMES = Object.freeze(['Milo', 'Nori', 'Lumi', 'Pico', 'Tavi', 'Momo', 'Koda', 'Fino']);
const CREW_COLORS = Object.freeze(['#1685ee', '#00aa9a', '#7d7f84', '#7c6df2', '#e97255', '#34a26f']);
const CHAT_LIVE_SYNC_COLLECTIONS = Object.freeze([
  CHAT_COLLECTION,
  'business_commands',
  'ctox_queue_tasks',
  'ctox_crew_members',
]);

// --- The crew pool in the bar: members, draggable onto any app -----------------
// A member is picked up from the bar, wriggles on the hook while it is carried,
// and is dropped on a record; the shell then opens the CTOX context menu right
// there with the member standing by (pre-lease assignment travels with the task).
async function loadCrewMembers({ state, db }) {
  const collection = db?.raw?.ctox_crew_members;
  if (!collection || typeof collection.find !== 'function') return false;
  try {
    const docs = await collection.find({ selector: { archived: false }, limit: 64 }).exec();
    const members = (Array.isArray(docs) ? docs : [])
      .map((doc) => doc?.toJSON?.() || doc)
      .filter((doc) => doc && doc.id && doc.name)
      .map((doc) => ({
        id: String(doc.id),
        name: String(doc.name),
        shape: String(doc.shape || 'round'),
        color: String(doc.color || '#7d7f84'),
        state: String(doc.state || 'home'),
        domain: Array.isArray(doc.domain) ? doc.domain.filter(Boolean) : [],
        last_memory_read_at_ms: Number(doc.last_memory_read_at_ms) || 0,
        last_learning_at_ms: Number(doc.last_learning_at_ms) || 0,
      }))
      .sort((a, b) => a.name.localeCompare(b.name));
    const before = JSON.stringify(state.crewMembers || []);
    state.crewMembers = members;
    return before !== JSON.stringify(members);
  } catch (error) {
    console.warn?.('[business-chat] crew pool load failed', error);
    return false;
  }
}

// How long a member visibly reads its memory (attempt start) or learns
// (after the learning tick). Both are stamps from the projection, not guesses.
export const CREW_READING_WINDOW_MS = 20 * 1000;
export const CREW_LEARNING_WINDOW_MS = 90 * 1000;

// The one expression a member wears right now: reading (on duty, memory just
// read), learning (at home, just learned), running, failed or idle.
export function crewMemberExpression(member, nowMs = Date.now()) {
  const readAt = Number(member?.last_memory_read_at_ms) || 0;
  const learnedAt = Number(member?.last_learning_at_ms) || 0;
  if (member?.state === 'on_duty') {
    return readAt && nowMs - readAt >= 0 && nowMs - readAt < CREW_READING_WINDOW_MS ? 'reading' : 'running';
  }
  if (member?.state === 'resting_after_failure') return 'failed';
  return learnedAt && nowMs - learnedAt >= 0 && nowMs - learnedAt < CREW_LEARNING_WINDOW_MS ? 'learning' : 'idle';
}

// When the current expression ends (ms from now), or 0 when it does not decay.
export function crewMemberExpressionTtlMs(member, nowMs = Date.now()) {
  const expression = crewMemberExpression(member, nowMs);
  if (expression === 'reading') return Math.max(1, Number(member.last_memory_read_at_ms) + CREW_READING_WINDOW_MS - nowMs);
  if (expression === 'learning') return Math.max(1, Number(member.last_learning_at_ms) + CREW_LEARNING_WINDOW_MS - nowMs);
  return 0;
}

// What the rendered pool depends on: who is in it and how each one looks.
function crewPoolSignature(state) {
  const now = Date.now();
  return (state?.crewMembers || []).map((member) => `${member.id}:${crewMemberExpression(member, now)}`).join('|');
}

// ---- Crew presence on the apps -------------------------------------------
// A member working an app task is visible on that app: in the corner of the
// window icon and on the desktop icon. Source is the queue (task module +
// member), never a guess from the UI.
export const CREW_APP_PRESENCE_STATUSES = new Set(['running', 'leased', 'review', 'drafting']);
export const CREW_APP_PRESENCE_LIMIT = 2;
const CREW_APP_PRESENCE_TASK_LIMIT = 200;

export function crewAppPresenceFromTasks(tasks, members) {
  const byId = new Map((members || []).map((member) => [member.id, member]));
  const presence = new Map();
  for (const task of Array.isArray(tasks) ? tasks : []) {
    const status = String(task?.status || '').trim().toLowerCase();
    if (!CREW_APP_PRESENCE_STATUSES.has(status)) continue;
    const memberId = String(task?.crew_member_id || '').trim();
    const member = memberId ? byId.get(memberId) : null;
    if (!member) continue;
    const moduleId = String(task?.module || task?.source_module || '').trim();
    if (!moduleId) continue;
    const entries = presence.get(moduleId) || [];
    if (entries.some((entry) => entry.member.id === member.id)) continue;
    entries.push({ member, task });
    presence.set(moduleId, entries);
  }
  return presence;
}

function crewAppPresenceSignature(entries, nowMs = Date.now()) {
  return entries.map((entry) => `${entry.member.id}:${crewMemberExpression(entry.member, nowMs)}`).join('|');
}

function crewAppPresenceHtml(entries) {
  const german = chatUiIsGerman();
  const shown = entries.slice(0, CREW_APP_PRESENCE_LIMIT);
  const names = entries.map((entry) => entry.member.name).join(', ');
  const title = german
    ? `${names} ${entries.length === 1 ? 'arbeitet' : 'arbeiten'} hier`
    : `${names} ${entries.length === 1 ? 'is' : 'are'} working here`;
  const more = entries.length > shown.length ? `<b>+${entries.length - shown.length}</b>` : '';
  return `<span class="ctox-crew-app-presence" data-crew-presence data-crew-presence-signature="${escapeAttr(crewAppPresenceSignature(entries))}" title="${escapeAttr(title)}" aria-label="${escapeAttr(title)}">${shown.map((entry) => crewMemberCreatureHtml(entry.member, 'badge')).join('')}${more}</span>`;
}

function crewAppPresenceHosts() {
  const hosts = [];
  const appIdOf = (value) => String(value || '').trim().replace(/^(desktop-app|module):/, '');
  document.querySelectorAll('.shell-window[data-owner-id] .shell-window-v2-icon').forEach((icon) => {
    const appId = appIdOf(icon.closest('.shell-window')?.dataset.ownerId);
    if (appId) hosts.push({ host: icon, appId });
  });
  document.querySelectorAll('.desktop-icon[data-target] .desktop-icon-glyph').forEach((glyph) => {
    const appId = appIdOf(glyph.closest('.desktop-icon')?.dataset.target);
    if (appId) hosts.push({ host: glyph, appId });
  });
  return hosts;
}

export function applyCrewAppPresence(presence) {
  for (const { host, appId } of crewAppPresenceHosts()) {
    const entries = presence.get(appId) || [];
    const existing = host.querySelector(':scope > [data-crew-presence]');
    if (!entries.length) {
      existing?.remove();
      host.classList.remove('has-crew-presence');
      continue;
    }
    const signature = crewAppPresenceSignature(entries);
    if (existing && existing.dataset.crewPresenceSignature === signature) continue;
    existing?.remove();
    host.insertAdjacentHTML('beforeend', crewAppPresenceHtml(entries));
    host.classList.add('has-crew-presence');
  }
}

async function loadCrewAppTasks(db) {
  const collection = db?.raw?.ctox_queue_tasks;
  if (!collection || typeof collection.find !== 'function') return [];
  try {
    const docs = await collection.find({
      selector: { updated_at_ms: { $gt: 0 } },
      sort: [{ updated_at_ms: 'desc' }],
      limit: CREW_APP_PRESENCE_TASK_LIMIT,
    }).exec();
    return (Array.isArray(docs) ? docs : []).map((doc) => doc?.toJSON?.() || doc).filter(Boolean);
  } catch (error) {
    console.warn?.('[business-chat] crew app presence load failed', error);
    return [];
  }
}

// Presence follows the queue, the crew, opened windows and the desktop grid.
function wireCrewAppPresence({ state, db, syncFacade }) {
  let disposed = false;
  let tasks = [];
  let reloadTimer = null;
  let expressionTimer = null;
  let busWired = false;
  let desktopObserver = null;
  let desktopObserverTarget = null;
  const subscriptions = [];
  const readinessCleanups = [];
  let windowBusCleanup = null;
  const apply = () => {
    if (disposed) return;
    const members = state.crewMembers || [];
    applyCrewAppPresence(crewAppPresenceFromTasks(tasks, members));
    // Expressions decay (reading -> running, learning -> idle); re-draw when
    // the earliest one ends so the badge does not freeze mid-expression.
    if (expressionTimer) window.clearTimeout(expressionTimer);
    const ttl = members.reduce((min, member) => {
      const next = crewMemberExpressionTtlMs(member);
      return next > 0 ? Math.min(min, next) : min;
    }, Infinity);
    expressionTimer = Number.isFinite(ttl) ? window.setTimeout(() => { expressionTimer = null; apply(); }, ttl + 50) : null;
    wireLate();
  };
  const reload = () => {
    if (disposed) return Promise.resolve();
    return loadCrewAppTasks(db).then((next) => {
      if (disposed) return;
      tasks = next;
      apply();
    }).catch(() => {});
  };
  const scheduleReload = () => {
    if (disposed || reloadTimer) return;
    reloadTimer = window.setTimeout(() => { reloadTimer = null; reload(); }, 400);
  };
  const wireLate = () => {
    if (disposed) return;
    if (!busWired) {
      const bus = window.CTOX_BUSINESS_OS_APP?.eventBus;
      if (bus && typeof bus.on === 'function' && typeof bus.off === 'function') {
        busWired = true;
        const token = bus.on('window:opened', apply);
        windowBusCleanup = () => bus.off('window:opened', token);
      }
    }
    const container = document.querySelector('[data-desktop-icons]');
    if (container && desktopObserverTarget !== container && typeof MutationObserver === 'function') {
      desktopObserver?.disconnect?.();
      desktopObserverTarget = container;
      desktopObserver = new MutationObserver(() => { queueMicrotask(apply); });
      desktopObserver.observe(container, { childList: true });
    }
  };
  try { subscriptions.push(db?.raw?.ctox_queue_tasks?.$?.subscribe?.(scheduleReload) || null); } catch {}
  try {
    subscriptions.push(db?.raw?.ctox_crew_members?.$?.subscribe?.(() => {
      if (disposed) return;
      loadCrewMembers({ state, db }).then(apply).catch(() => {});
    }) || null);
  } catch {}
  try {
    readinessCleanups.push(syncFacade?.subscribeCollectionReadiness?.('ctox_queue_tasks', reload));
    readinessCleanups.push(syncFacade?.subscribeCollectionReadiness?.('ctox_crew_members', reload));
  } catch {}
  reload();
  return () => {
    if (disposed) return;
    disposed = true;
    subscriptions.forEach((subscription) => subscription?.unsubscribe?.());
    readinessCleanups.forEach((cleanup) => cleanup?.());
    windowBusCleanup?.();
    if (reloadTimer) window.clearTimeout(reloadTimer);
    if (expressionTimer) window.clearTimeout(expressionTimer);
    desktopObserver?.disconnect?.();
  };
}

function crewMemberCreatureHtml(member, placement = 'fab') {
  return crewCreatureHtml({ crewKey: member.id, crewIdentity: { name: member.name, shape: member.shape, color: member.color } }, crewMemberExpression(member), placement);
}

function crewPoolSlotHtml(member, placement = 'fab') {
  const german = chatUiIsGerman();
  const expression = crewMemberExpression(member);
  const stateText = expression === 'reading' ? (german ? 'liest sein Gedächtnis' : 'reading its memory')
    : expression === 'learning' ? (german ? 'lernt aus dem Einsatz' : 'learning from the assignment')
      : member.state === 'on_duty' ? (german ? 'im Einsatz' : 'on duty')
        : member.state === 'resting_after_failure' ? (german ? 'erholt sich' : 'recovering')
          : (german ? 'zu Hause' : 'at home');
  const domain = member.domain.length ? ` · ${member.domain.join(', ')}` : '';
  const focusable = placement === 'fab' ? '' : ' role="button" tabindex="0"';
  return `<span class="ctox-chat-crew-slot"${focusable} data-crew-drag="${escapeAttr(member.id)}" title="${escapeAttr(`${member.name} · ${stateText}${domain} · ${german ? 'auf eine App ziehen' : 'drag onto an app'}`)}" aria-label="${escapeAttr(member.name)}">${crewMemberCreatureHtml(member, placement)}</span>`;
}

const CREW_DRAG_THRESHOLD_PX = 6;

function wireCrewDrag(root, state) {
  if (root.dataset.crewDragWired === '1') return;
  root.dataset.crewDragWired = '1';
  let drag = null;
  const cleanup = () => {
    if (!drag) return;
    drag.ghost?.remove();
    document.body.classList.remove('ctox-crew-dragging');
    window.removeEventListener('pointermove', onMove);
    window.removeEventListener('pointerup', onUp);
    window.removeEventListener('pointercancel', onCancel);
    window.removeEventListener('keydown', onKey, true);
    drag = null;
  };
  const moveGhost = (x, y) => {
    if (!drag?.ghost) return;
    drag.ghost.style.transform = `translate(${Math.round(x - 24)}px, ${Math.round(y + 6)}px)`;
  };
  const onMove = (event) => {
    if (!drag) return;
    const dx = event.clientX - drag.startX;
    const dy = event.clientY - drag.startY;
    if (!drag.active) {
      if (Math.hypot(dx, dy) < CREW_DRAG_THRESHOLD_PX) return;
      drag.active = true;
      document.body.classList.add('ctox-crew-dragging');
      const ghost = document.createElement('div');
      ghost.className = 'ctox-crew-drag-ghost';
      ghost.setAttribute('aria-hidden', 'true');
      ghost.innerHTML = `<span class="ctox-crew-drag-hook"></span><span class="ctox-crew-drag-body">${crewMemberCreatureHtml(drag.member, 'map')}</span><span class="ctox-crew-drag-name">${escapeHtml(drag.member.name)}</span>`;
      document.body.appendChild(ghost);
      drag.ghost = ghost;
    }
    moveGhost(event.clientX, event.clientY);
  };
  const onUp = (event) => {
    if (!drag) return;
    const wasActive = drag.active;
    const member = drag.member;
    const x = event.clientX;
    const y = event.clientY;
    cleanup();
    if (!wasActive) return;
    state.crewDragEndedAt = Date.now();
    const crew = {
      id: member.id,
      name: member.name,
      shape: member.shape,
      color: member.color,
      creatureHtml: crewMemberCreatureHtml(member, 'map'),
    };
    const opened = window.CTOX_BUSINESS_OS_APP?.openCrewContextMenu?.({ clientX: x, clientY: y, crew });
    if (!opened) {
      // Dropped outside an app surface: nothing to hand over here.
      window.dispatchEvent(new CustomEvent('ctox-business-os-crew-drop-missed', { detail: { member_id: member.id } }));
    }
  };
  const onCancel = () => cleanup();
  const onKey = (event) => {
    if (event.key === 'Escape') {
      event.stopPropagation();
      cleanup();
    }
  };
  // Keyboard path (Review-Befund B8): Enter or Space on a focused slot hands
  // the member to the focused app window, the same way a drop does.
  root.addEventListener('keydown', (event) => {
    if (event.key !== 'Enter' && event.key !== ' ') return;
    const slot = event.target?.closest?.('[data-crew-drag][tabindex]');
    if (!slot || !root.contains(slot)) return;
    const member = (state.crewMembers || []).find((item) => item.id === slot.dataset.crewDrag);
    if (!member) return;
    event.preventDefault();
    event.stopPropagation();
    const target = document.querySelector('.shell-window.is-focused, [data-shell-window].is-focused, .shell-window');
    const rect = target?.getBoundingClientRect?.();
    const crew = { id: member.id, name: member.name, shape: member.shape, color: member.color, creatureHtml: crewMemberCreatureHtml(member, 'map') };
    const opened = rect && rect.width > 0
      ? window.CTOX_BUSINESS_OS_APP?.openCrewContextMenu?.({ clientX: rect.left + rect.width / 2, clientY: rect.top + rect.height / 2, crew })
      : false;
    if (!opened) window.dispatchEvent(new CustomEvent('ctox-business-os-crew-drop-missed', { detail: { member_id: member.id } }));
  });
  root.addEventListener('pointerdown', (event) => {
    const slot = event.target?.closest?.('[data-crew-drag]');
    if (!slot || !root.contains(slot) || event.button !== 0) return;
    const member = (state.crewMembers || []).find((item) => item.id === slot.dataset.crewDrag);
    if (!member) return;
    drag = { member, startX: event.clientX, startY: event.clientY, active: false, ghost: null };
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
    window.addEventListener('pointercancel', onCancel);
    window.addEventListener('keydown', onKey, true);
  });
}

export function initBusinessChat({
  session,
  commandBus,
  db,
  sync: syncFacade,
  getActiveModule,
}) {
  if (!session?.authenticated || document.querySelector('[data-ctox-chat-root]')) return;
  installChatStyles();
  const state = readChatState(session);
  const root = document.createElement('div');
  root.className = 'ctox-chat-root';
  root.dataset.ctoxChatRoot = 'true';
  root.__ctoxChatSync = syncFacade || null;
  // Diagnostic handle: the bar's state (crew pool, chats) for a browser probe.
  root.__ctoxChatState = state;
  root.__ctoxChatOnTrackingStateChanged = null;
  document.body.append(root);

  const handleRootClick = (event) => {
    const datePickerTrigger = event.target.closest?.('.ctox-date-picker-trigger');
    if (datePickerTrigger && root.contains(datePickerTrigger)) {
      if (event.target.tagName !== 'INPUT') {
        event.preventDefault();
        event.stopPropagation();
        state.dateWorkloadOpen = !state.dateWorkloadOpen;
        state.chatListOpen = false;
        renderChatRoot({ root, state, commandBus, db, getActiveModule });
      }
      return;
    }
    const maximizeButton = event.target.closest?.('[data-chat-maximize]');
    if (maximizeButton && root.contains(maximizeButton)) {
      event.preventDefault();
      event.stopImmediatePropagation();
      const node = maximizeButton.closest('[data-chat-id]');
      const chat = state.chats.find((item) => item.id === node?.dataset.chatId);
      if (!chat) return;
      captureDrafts(root, state);
      const ticket = claimChatOpenOwnership(state);
      chat.maximized = !chat.maximized;
      markChatExpandedByUser(state, chat, ticket);
      state.dockCollapsed = false;
      state.activeChatId = chat.id;
      touchChats(state, [chat]);
      renderAndPersistChatState({ root, state, commandBus, db, getActiveModule });
      return;
    }
    const minimizeButton = event.target.closest?.('[data-chat-minimize]');
    if (minimizeButton && root.contains(minimizeButton)) {
      event.preventDefault();
      event.stopPropagation();
      event.stopImmediatePropagation?.();
      collapseChatWindow({ root, state, commandBus, db, getActiveModule, target: minimizeButton }).catch((error) => {
        console.warn('[business-chat] chat minimize failed', error);
      });
      return;
    }
    const deleteButton = event.target.closest?.('[data-chat-delete]');
    if (deleteButton && root.contains(deleteButton)) {
      event.preventDefault();
      event.stopPropagation();
      event.stopImmediatePropagation?.();
      deleteChatFromTarget({ root, state, commandBus, db, getActiveModule, target: deleteButton }).catch((error) => {
        console.warn('[business-chat] chat delete failed', error);
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
      submitChatForm({ root, state, chat, node, commandBus, db, sync: syncFacade, getActiveModule }).catch((error) => {
        console.warn('[business-chat] chat send failed', error);
      });
      return;
    }
    const chatOpenButton = event.target.closest?.('[data-chat-open]');
    if (!chatOpenButton || !root.contains(chatOpenButton)) return;
    event.preventDefault();
    event.stopPropagation();
    if (Date.now() - Number(state.crewDragEndedAt || 0) < 500) return;
    toggleChatDock({ root, state, commandBus, db, getActiveModule }).catch((error) => {
      console.warn('[business-chat] chat dock toggle failed', error);
    });
  };

  let trackingSyncTimer = null;
  let trackingSyncRunning = false;
  let trackingSyncRerun = false;
  let trackingWatch = null;

  const runTrackedMessageSync = async () => {
    if (trackingSyncRunning) {
      trackingSyncRerun = true;
      return;
    }
    const presentationTicket = currentChatOpenOwnership(state);
    trackingSyncRunning = true;
    try {
      captureDrafts(root, state);
      const membersChanged = await loadCrewMembers({ state, db }).catch(() => false);
      const changed = (await syncTrackedMessages({ state, db, sync: syncFacade })) || membersChanged;
      if (changed) persistChatState({ state, db });
      if (changed && ownsChatOpenOwnership(state, presentationTicket)) {
        renderChatRoot({ root, state, commandBus, db, getActiveModule });
      }
    } finally {
      trackingSyncRunning = false;
      trackingWatch?.refresh?.();
      if (trackingSyncRerun && hasTrackedMessagesNeedingSync(state)) {
        trackingSyncRerun = false;
        scheduleTrackedMessageSync(75);
      } else {
        trackingSyncRerun = false;
      }
    }
  };

  const scheduleTrackedMessageSync = (delayMs = 75) => {
    if (trackingSyncTimer) return;
    trackingSyncTimer = window.setTimeout(() => {
      trackingSyncTimer = null;
      runTrackedMessageSync().catch(() => {});
    }, Math.max(0, delayMs));
  };

  const sync = () => {
    scheduleTrackedMessageSync();
  };

  const syncAfterSubmit = () => {
    if (trackingWatch?.refresh?.()) scheduleTrackedMessageSync(0);
  };

  let chatHydrationRetryTimer = null;
  let chatLayoutObserver = null;
  const scheduleChatHydrationRetry = (delayMs = 750) => {
    if (chatHydrationRetryTimer) return;
    chatHydrationRetryTimer = window.setTimeout(() => {
      chatHydrationRetryTimer = null;
      syncChats();
    }, Math.max(0, delayMs));
  };

  const syncChats = () => {
    if (shouldDeferRemoteChatHydration(root, state)) {
      scheduleChatHydrationRetry();
      return;
    }
    const presentationTicket = currentChatOpenOwnership(state);
    captureDrafts(root, state);
    hydrateChatsFromRxDb({ state, db, session }).then((changed) => {
      if (changed && ownsChatOpenOwnership(state, presentationTicket)) {
        renderChatRoot({ root, state, commandBus, db, getActiveModule });
      }
    }).catch(() => {});
  };

  startChatLiveCollections({
    sync: syncFacade,
    db,
    onReady: () => {
      syncChats();
      scheduleTrackedMessageSync(0);
      trackingWatch?.refresh?.({ schedule: true });
    },
  });
  // The pool follows the crew collection, however late it becomes ready: the
  // first load after readiness fills the bar, later changes re-render it.
  let crewChangeSubscription = null;
  let crewPoolDisposed = false;
  let crewPoolRetryTimer = null;
  let crewReadinessCleanup = null;
  const refreshCrewPool = () => {
    if (crewPoolDisposed) return Promise.resolve();
    return loadCrewMembers({ state, db }).then((changed) => {
      if (crewPoolDisposed) return;
      if (!crewChangeSubscription) {
        try {
          crewChangeSubscription = db?.raw?.ctox_crew_members?.$?.subscribe?.(() => {
            if (crewPoolDisposed) return;
            loadCrewMembers({ state, db }).then((next) => {
              if (next && !crewPoolDisposed) renderChatRoot({ root, state, commandBus, db, getActiveModule });
            }).catch(() => {});
          }) || null;
        } catch {}
      }
      if (changed) renderChatRoot({ root, state, commandBus, db, getActiveModule });
    }).catch(() => {});
  };
  try {
    crewReadinessCleanup = syncFacade?.subscribeCollectionReadiness?.('ctox_crew_members', refreshCrewPool);
  } catch {}
  // Bounded start-up retries: the collection registers and fills after the
  // bar exists; a handful of widening attempts, then readiness/changes only.
  const crewPoolRetryDelays = [2000, 5000, 10000, 20000, 40000];
  const retryCrewPool = (attempt = 0) => {
    if (crewPoolDisposed || (state.crewMembers || []).length || attempt >= crewPoolRetryDelays.length) return;
    crewPoolRetryTimer = window.setTimeout(() => {
      crewPoolRetryTimer = null;
      refreshCrewPool().then(() => retryCrewPool(attempt + 1));
    }, crewPoolRetryDelays[attempt]);
  };
  refreshCrewPool().then(() => retryCrewPool(0));
  const cleanupCrewAppPresence = wireCrewAppPresence({ state, db, syncFacade });
  root.__ctoxCrewAppPresenceCleanup = cleanupCrewAppPresence;

  const handleExternalSubmit = async (event) => {
    const detail = event.detail || {};
    const text = String(detail.text || detail.message || '').trim();
    if (!text) return;
    const presentationTicket = claimChatOpenOwnership(state);
    state.selectedDate = getLocalDateString(Date.now());
    const createNewChat = shouldCreateChatForExternalSubmit(detail);
    const chat = createNewChat ? createChat(state.ownerUserId, state.selectedDate) : ensureChat(state, session);
    if (createNewChat) state.chats.push(chat);
    if (detail.title) chat.title = String(detail.title).trim() || chat.title;
    chat.contextMeta = chatContextMetaFromDetail(detail);
    const handedMember = String(detail.crew_member_id || detail.crewMemberId || '').trim();
    if (handedMember) {
      chat.crew_member_id = handedMember;
      const identity = detail.crew_identity || detail.crewIdentity || (state.crewMembers || []).find((member) => member.id === handedMember);
      if (identity?.name) chat.crewIdentity = { name: String(identity.name), shape: String(identity.shape || ''), color: String(identity.color || '') };
    }
    markChatExpandedByUser(state, chat, presentationTicket);
    focusChatForUser(state, chat);
    chat.draft = '';
    try {
      const submission = await submitChatMessage({
        state,
        chat,
        text,
        commandBus,
        db,
        sync: syncFacade,
        getActiveModule,
        meta: detail,
        onPending: () => {
          persistChatState({ state, db, remote: false }).catch(() => {});
          renderChatRoot({ root, state, commandBus, db, getActiveModule });
          detail.onPresented?.({ command_id: chat.lastTrackingId });
        },
      });
      if (!submission) {
        throw new Error('Die Aufgabe konnte nicht an die Crew übergeben werden.');
      }

      // Queue acceptance is the app-facing contract. Remote chat persistence
      // must not keep the originating workflow in a false pending state.
      detail.resolveSubmission?.(submission);
      await persistChatState({ state, db });
      if (ownsChatOpenOwnership(state, presentationTicket)) {
        renderChatRoot({ root, state, commandBus, db, getActiveModule });
      }
      syncAfterSubmit();
    } catch (error) {
      detail.rejectSubmission?.(
        error instanceof Error ? error : new Error(String(error || 'Aufgabe konnte nicht übergeben werden.')),
      );
      if (ownsChatOpenOwnership(state, presentationTicket)) {
        renderChatRoot({ root, state, commandBus, db, getActiveModule });
      }
    }
  };

  const handleExternalOpen = async (event) => {
    const detail = event.detail || {};
    const presentationTicket = claimChatOpenOwnership(state);
    // A fresh draft and an already loaded conversation are local presentation
    // operations. Only an unresolved tracking identity needs a lookup before
    // choosing a chat; creating one first could duplicate the remote history.
    if (chatOpenNeedsHydration(state, detail)) {
      await hydrateChatsFromRxDb({ state, db, session }).catch(() => false);
    }
    // Crew readiness and updates already belong to refreshCrewPool above.
    if (!ownsChatOpenOwnership(state, presentationTicket)) return;
    state.selectedDate = getLocalDateString(Date.now());
    const chat = resolveChatForOpenDetail(state, session, detail);
    chat.title = String(detail.title || chat.title || 'Crew').trim() || 'Crew';
    const handedMember = String(detail.crew_member_id || detail.crewMemberId || '').trim();
    if (handedMember) {
      chat.crew_member_id = handedMember;
      const identity = detail.crew_identity || detail.crewIdentity || (state.crewMembers || []).find((member) => member.id === handedMember) || null;
      if (identity?.name) chat.crewIdentity = { name: String(identity.name), shape: String(identity.shape || ''), color: String(identity.color || '') };
    }
    markChatExpandedByUser(state, chat, presentationTicket);
    focusChatForUser(state, chat);
    chat.maximized = Boolean(detail.maximized);
    if ('draft' in detail || 'message' in detail) {
      chat.draft = String(detail.draft || detail.message || '');
    }
    chat.contextMeta = {
      ...(chat.contextMeta && typeof chat.contextMeta === 'object' ? chat.contextMeta : {}),
      ...chatContextMetaFromDetail(detail),
    };
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
    state.preCollapseExpandedChatIds = [];
    touchChats(state, [chat]);
    renderChatRoot({ root, state, commandBus, db, getActiveModule });
    await persistChatState({ state, db });
    if (!ownsChatOpenOwnership(state, presentationTicket)) return;
    renderChatRoot({ root, state, commandBus, db, getActiveModule });
  };

  const initialPresentationTicket = currentChatOpenOwnership(state);
  hydrateChatsFromRxDb({ state, db, session })
    .then(() => {
      if (!ownsChatOpenOwnership(state, initialPresentationTicket)) return;
      renderChatRoot({ root, state, commandBus, db, getActiveModule });
      trackingWatch?.refresh?.({ schedule: true });
    })
    .catch(() => {
      if (!ownsChatOpenOwnership(state, initialPresentationTicket)) return;
      renderChatRoot({ root, state, commandBus, db, getActiveModule });
      trackingWatch?.refresh?.({ schedule: true });
    });

  let scrollTimeout = null;
  const handleScroll = (event) => {
    const strip = root.querySelector('[data-chat-strip]');
    const stageInner = root.querySelector('.ctox-chat-stage-inner');
    if (strip && stageInner && event.target.closest('[data-chat-strip]')) {
      root.classList.add('is-scrolling');
      alignChatWindows(root);
      updateChatStripOverflowState(root);
      
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
        updateChatStripOverflowState(root);

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
      updateChatStripOverflowState(root);
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
    updateChatStripOverflowState(root);
    publishChatLayout(root, state);
  };

  root.addEventListener('click', handleRootClick, true);
  window.addEventListener('ctox-business-os-chat-submit', handleExternalSubmit);
  window.addEventListener(CHAT_OPEN_EVENT, handleExternalOpen);
  const handleOpenForCommand = (event) => {
    const detail = event?.data?.type === 'ctox-business-os-open-chat' ? event.data : (event?.detail || null);
    if (!detail) return;
    if (event?.data && event.origin && event.origin !== window.location.origin) return;
    openChatForCommand(state, db, session, detail).catch(() => {});
  };
  window.addEventListener('ctox-business-os-open-chat', handleOpenForCommand);
  window.addEventListener('message', handleOpenForCommand);
  root.addEventListener('scroll', handleScroll, true);
  window.addEventListener('resize', handleResize);
  root.addEventListener('wheel', handleWheel, { passive: false });
  root.addEventListener('mousedown', handleMouseDown);
  root.addEventListener('mousemove', handleMouseMove);
  window.addEventListener('mouseup', handleMouseUp);
  root.addEventListener('click', handleCaptureClick, true);

  if (typeof ResizeObserver === 'function') {
    chatLayoutObserver = new ResizeObserver(() => publishChatLayout(root, state));
    chatLayoutObserver.observe(root);
  }

  const businessChatsSub = db?.raw?.[CHAT_COLLECTION]?.$?.subscribe?.(syncChats) || null;
  trackingWatch = createTrackedMessageWatch({
    state,
    db,
    scheduleSync: sync,
  });
  root.__ctoxChatOnTrackingStateChanged = () => trackingWatch?.refresh?.({ schedule: true });
  trackingWatch.refresh({ schedule: true });

  root.__ctoxChatCleanup = () => {
    crewPoolDisposed = true;
    if (crewPoolRetryTimer) window.clearTimeout(crewPoolRetryTimer);
    crewChangeSubscription?.unsubscribe?.();
    crewReadinessCleanup?.();
    root.__ctoxCrewAppPresenceCleanup?.();
    root.removeEventListener('click', handleRootClick, true);
    window.removeEventListener('ctox-business-os-chat-submit', handleExternalSubmit);
    window.removeEventListener(CHAT_OPEN_EVENT, handleExternalOpen);
    window.removeEventListener('ctox-business-os-open-chat', handleOpenForCommand);
    window.removeEventListener('message', handleOpenForCommand);
    root.removeEventListener('scroll', handleScroll, true);
    window.removeEventListener('resize', handleResize);
    root.removeEventListener('wheel', handleWheel, { passive: false });
    root.removeEventListener('mousedown', handleMouseDown);
    root.removeEventListener('mousemove', handleMouseMove);
    window.removeEventListener('mouseup', handleMouseUp);
    root.removeEventListener('click', handleCaptureClick, true);
    businessChatsSub?.unsubscribe?.();
    trackingWatch?.stop?.();
    root.__ctoxChatOnTrackingStateChanged = null;
    if (trackingSyncTimer) window.clearTimeout(trackingSyncTimer);
    if (chatHydrationRetryTimer) window.clearTimeout(chatHydrationRetryTimer);
    chatLayoutObserver?.disconnect?.();
    if (root.__ctoxChatLayoutFrame) window.cancelAnimationFrame(root.__ctoxChatLayoutFrame);
    stopCrewProceduralMotion(root);
    window.dispatchEvent(new CustomEvent(CHAT_LAYOUT_EVENT, {
      detail: { version: 1, present: false, expanded: false },
    }));
    clearSchedulerLoop(root);
  };

  // Mount the local presentation immediately. Hydration may be delayed or
  // unavailable; it must not gate the dock or its prompt/open event handlers.
  renderChatRoot({ root, state, commandBus, db, getActiveModule });
}

function publishChatLayout(root, state) {
  if (!root?.isConnected) return;
  if (root.__ctoxChatLayoutFrame) window.cancelAnimationFrame(root.__ctoxChatLayoutFrame);
  root.__ctoxChatLayoutFrame = window.requestAnimationFrame(() => {
    root.__ctoxChatLayoutFrame = 0;
    if (!root.isConnected) return;
    const rect = root.getBoundingClientRect();
    const dockRect = root.querySelector('[data-chat-dock]')?.getBoundingClientRect?.();
    const expanded = !Boolean(state?.dockCollapsed);
    window.dispatchEvent(new CustomEvent(CHAT_LAYOUT_EVENT, {
      detail: {
        version: 1,
        present: true,
        expanded,
        collapsed: !expanded,
        top: rect.top,
        left: rect.left,
        right: rect.right,
        bottom: rect.bottom,
        width: rect.width,
        height: rect.height,
        dock_top: dockRect?.top,
        dock_right: dockRect?.right,
        dock_bottom: dockRect?.bottom,
        dock_left: dockRect?.left,
        dock_width: dockRect?.width,
        dock_height: dockRect?.height,
      },
    }));
  });
}

function createTrackedMessageWatch({
  state,
  db,
  scheduleSync,
  timerWindow = typeof window !== 'undefined' ? window : globalThis,
} = {}) {
  let businessCommandsSub = null;
  let queueTasksSub = null;
  let timer = null;
  const notify = () => {
    if (hasTrackedMessagesNeedingSync(state)) {
      scheduleSync?.();
    } else {
      stopActiveWatch();
    }
  };
  const startActiveWatch = () => {
    if (businessCommandsSub || queueTasksSub || timer) return;
    businessCommandsSub = db?.raw?.business_commands?.$?.subscribe?.(notify) || null;
    queueTasksSub = db?.raw?.ctox_queue_tasks?.$?.subscribe?.(notify) || null;
    if (typeof timerWindow?.setInterval === 'function') {
      timer = timerWindow.setInterval(notify, ACTIVE_TRACKING_SYNC_INTERVAL_MS);
    }
  };
  const stopActiveWatch = () => {
    businessCommandsSub?.unsubscribe?.();
    queueTasksSub?.unsubscribe?.();
    businessCommandsSub = null;
    queueTasksSub = null;
    if (timer && typeof timerWindow?.clearInterval === 'function') {
      timerWindow.clearInterval(timer);
    }
    timer = null;
  };
  const refresh = ({ schedule = false } = {}) => {
    if (!hasTrackedMessagesNeedingSync(state)) {
      stopActiveWatch();
      return false;
    }
    startActiveWatch();
    if (schedule) scheduleSync?.();
    return true;
  };
  return {
    refresh,
    stop: stopActiveWatch,
    isWatching: () => Boolean(businessCommandsSub || queueTasksSub || timer),
  };
}

function shouldCreateChatForExternalSubmit(detail = {}) {
  if (detail.reuseActive === true) return false;
  if (detail.reuseActive === false) return true;
  const action = detail.action || detail.client_context?.action || detail.clientContext?.action || '';
  return action === 'context-chat';
}

function chatAllowsAutoFocus(chat) {
  const meta = chat?.contextMeta && typeof chat.contextMeta === 'object'
    ? chat.contextMeta
    : {};
  const clientContext = meta.client_context && typeof meta.client_context === 'object'
    ? meta.client_context
    : {};
  return meta.business_chat_auto_focus !== false
    && clientContext.business_chat_auto_focus !== false
    && clientContext.auto_focus !== false;
}

function alignChatWindows(root) {
  if (!root) return;
  const strip = root.querySelector('[data-chat-strip]');
  const stage = root.querySelector('[data-chat-stage]');
  const stageInner = root.querySelector('.ctox-chat-stage-inner');
  if (!strip || !stage || !stageInner) return;

  const windows = Array.from(stageInner.querySelectorAll('.ctox-chat-window'));
  const isNarrow = window.innerWidth <= 780;

  if (isNarrow) {
    if (stageInner.classList.contains('is-side-by-side')) {
      stageInner.classList.remove('is-side-by-side');
    }
    windows.forEach((win) => {
      setStyleIfChanged(win, 'position', '');
      setStyleIfChanged(win, 'left', '');
    });
    return;
  }

  const hasMaximized = windows.some((win) => win.classList.contains('is-maximized'));
  if (stageInner.classList.contains('has-maximized') !== hasMaximized) {
    stageInner.classList.toggle('has-maximized', hasMaximized);
  }

  const rootRect = stageInner.getBoundingClientRect();
  const gap = 12;
  const positions = [];

  windows.forEach((win) => {
    const chatId = win.dataset.chatId;
    const chip = strip.querySelector(`[data-chat-focus="${chatId}"]`);
    // Layout width, not the transformed rect: inactive windows sit at
    // scale(0.8) in the carousel, and the first side-by-side pass runs before
    // that transform is dropped. Measuring the scaled rect let three windows
    // overlap by ~50px while the stage still claimed side-by-side.
    const winWidth = win.offsetWidth
      || win.getBoundingClientRect().width
      || (win.classList.contains('is-maximized') ? 560 : 460);
    let preferredLeft = 8;

    if (chip) {
      const chipRect = chip.getBoundingClientRect();
      const chipCenter = chipRect.left + chipRect.width / 2;
      preferredLeft = chipCenter - rootRect.left - winWidth / 2;
    }

    positions.push({
      win,
      width: winWidth,
      left: preferredLeft,
      preferredLeft,
    });
  });

  const widestWindow = positions.reduce((max, item) => Math.max(max, item.width), 0);
  const layoutFrame = chatWindowStageFrame(root, stageInner, widestWindow);
  positions.forEach((item) => {
    item.left = clampChatWindowLeft(item.left, item.width, layoutFrame);
  });

  const totalWidthNeeded = positions.reduce((sum, item) => sum + item.width, 0)
    + Math.max(0, positions.length - 1) * gap;
  const fitsSideBySide = totalWidthNeeded <= layoutFrame.width;
  if (stageInner.classList.contains('is-side-by-side') !== fitsSideBySide) {
    stageInner.classList.toggle('is-side-by-side', fitsSideBySide);
  }

  if (fitsSideBySide && positions.length > 0) {
    for (let iteration = 0; iteration < 10; iteration += 1) {
      for (let index = 0; index < positions.length; index += 1) {
        if (index === 0) {
          positions[index].left = Math.max(layoutFrame.left, positions[index].left);
        } else {
          const previous = positions[index - 1];
          positions[index].left = Math.max(previous.left + previous.width + gap, positions[index].left);
        }
      }
      for (let index = positions.length - 1; index >= 0; index -= 1) {
        if (index === positions.length - 1) {
          positions[index].left = Math.min(layoutFrame.right - positions[index].width, positions[index].left);
        } else {
          const next = positions[index + 1];
          positions[index].left = Math.min(next.left - gap - positions[index].width, positions[index].left);
        }
      }
    }
  } else if (positions.length > 0) {
    const availableSpan = Math.max(0, layoutFrame.width - widestWindow);
    const naturalStep = positions.length > 1 ? availableSpan / (positions.length - 1) : 0;
    const carouselStep = positions.length > 1
      ? Math.max(56, Math.min(142, naturalStep))
      : 0;
    const activePositionIndex = positions.findIndex(({ win }) => win.classList.contains('is-active'));
    const activeIndex = activePositionIndex >= 0
      ? activePositionIndex
      : Math.floor(positions.length / 2);
    const active = positions[activeIndex];
    const activeLeft = clampChatWindowLeft(active.preferredLeft, active.width, layoutFrame);

    // The active window stays under its chip; the neighbours fan out with the
    // regular step only as far as the stage allows on their side, otherwise
    // they stack more densely. With the active chip at the far left the left
    // neighbours used to hang off the stage (welsch 08.09.: left -154px).
    const leftRoom = Math.max(0, activeLeft - layoutFrame.left);
    const rightRoom = Math.max(0, layoutFrame.right - (activeLeft + active.width));
    const leftCount = activeIndex;
    const rightCount = positions.length - 1 - activeIndex;
    const leftStep = leftCount > 0 ? Math.min(carouselStep, leftRoom / leftCount) : 0;
    const rightStep = rightCount > 0 ? Math.min(carouselStep, rightRoom / rightCount) : 0;
    positions.forEach((item, index) => {
      const distance = index - activeIndex;
      item.left = distance < 0
        ? activeLeft + distance * leftStep
        : activeLeft + distance * rightStep;
    });
  }

  positions.forEach(({ win, left, width }) => {
    setStyleIfChanged(win, 'position', 'absolute');
    const nextLeft = fitsSideBySide || win.classList.contains('is-active')
      ? clampChatWindowLeft(left, width, layoutFrame)
      : left;
    setStyleIfChanged(win, 'left', `${nextLeft}px`);
  });

  const spacer = stageInner.querySelector('.ctox-chat-stage-spacer');
  if (spacer) {
    setStyleIfChanged(spacer, 'position', 'absolute');
    setStyleIfChanged(spacer, 'width', '1px');
  }
}

function chatWindowStageFrame(root, stageInner, minContentWidth = 0) {
  const stageRect = stageInner.getBoundingClientRect();
  const rootRect = root?.getBoundingClientRect?.();
  const availableWidth = Math.max(stageRect.width, rootRect?.width || 0, minContentWidth);
  const left = 8;
  const right = Math.max(left, availableWidth - 8);
  return {
    left,
    right,
    width: right - left,
  };
}

function clampChatWindowLeft(left, width, frame) {
  if (!frame || frame.width <= 0) return Math.max(8, left);
  if (frame.width <= width) return frame.left;
  return Math.max(frame.left, Math.min(frame.right - width, left));
}

function renderAndPersistChatState({ root, state, commandBus, db, getActiveModule }) {
  renderChatRoot({ root, state, commandBus, db, getActiveModule });
  persistChatState({ state, db }).catch((error) => {
    console.warn('[business-chat] chat persistence failed', error);
  });
}

async function deleteChatFromTarget({ root, state, commandBus, db, getActiveModule, target }) {
  const node = target.closest('[data-chat-id]');
  const chat = state.chats.find((item) => item.id === node?.dataset.chatId);
  if (!chat) return;
  captureDrafts(root, state);
  if (!isChatEmptyForDeletion(chat)) {
    const confirmed = await showBusinessConfirm('Dieses Wesen wirklich aus der Crew entfernen?', {
      title: 'Wesen entfernen',
      confirmLabel: 'Entfernen',
    });
    if (!confirmed) return;
  }
  const deletion = deleteChat({ state, chat, db });
  renderChatRoot({ root, state, commandBus, db, getActiveModule });
  await deletion;
}

function isChatEmptyForDeletion(chat) {
  if (!chat) return true;
  if (Array.isArray(chat.messages) && chat.messages.length > 0) return false;
  if (String(chat.draft || '').trim()) return false;
  if (String(chat.lastTrackingId || '').trim()) return false;
  if (Array.isArray(chat.attachments) && chat.attachments.length > 0) return false;
  return !hasScheduledChatAttachments(chat.scheduledAttachmentsByCommand);
}

function hasScheduledChatAttachments(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  return Object.values(value).some((attachments) => Array.isArray(attachments) && attachments.length > 0);
}

function setWindowInteractiveState(win, isActive) {
  win.querySelectorAll('button, input, textarea, select, a, summary').forEach((node) => {
    const isAlwaysInteractiveHeaderControl = Boolean(node.closest('.ctox-chat-header-actions, .ctox-chat-delegation-card'));
    if (isActive || isAlwaysInteractiveHeaderControl) {
      if (node.dataset.chatInactiveTabManaged === 'true') {
        node.removeAttribute('tabindex');
        delete node.dataset.chatInactiveTabManaged;
      }
      if (node.hasAttribute('aria-hidden')) node.removeAttribute('aria-hidden');
      return;
    }
    if (!node.hasAttribute('tabindex')) {
      node.dataset.chatInactiveTabManaged = 'true';
      node.setAttribute('tabindex', '-1');
    } else if (node.dataset.chatInactiveTabManaged === 'true' && node.getAttribute('tabindex') !== '-1') {
      node.setAttribute('tabindex', '-1');
    }
    if (node.getAttribute('aria-hidden') !== 'true') {
      node.setAttribute('aria-hidden', 'true');
    }
  });
}

// Identical textContent / className / attribute writes force a layout pass even
// when nothing visible moved. Status ticks used to hit these paths every few
// seconds and the chat bar looked like it was rebuilding itself.
function setTextIfChanged(element, value) {
  const next = String(value ?? '');
  if (!element || element.textContent === next) return false;
  element.textContent = next;
  return true;
}

function setClassNameIfChanged(element, value) {
  const next = String(value ?? '');
  if (!element || element.className === next) return false;
  element.className = next;
  return true;
}

function setAttrIfChanged(element, name, value) {
  if (!element) return false;
  const next = String(value ?? '');
  if (element.getAttribute(name) === next) return false;
  element.setAttribute(name, next);
  return true;
}

function setDatasetIfChanged(element, key, value) {
  if (!element) return false;
  const next = String(value ?? '');
  if (element.dataset[key] === next) return false;
  element.dataset[key] = next;
  return true;
}

function setStyleIfChanged(element, prop, value) {
  if (!element?.style) return false;
  const next = String(value ?? '');
  if (element.style[prop] === next) return false;
  element.style[prop] = next;
  return true;
}

function setInlineStyleIfChanged(element, value) {
  if (!element?.style) return false;
  const next = String(value ?? '');
  if (element.style.cssText === next) return false;
  element.style.cssText = next;
  return true;
}

function stageWindowChats(expandedChats, activeExpandedChat) {
  if (!Array.isArray(expandedChats) || expandedChats.length === 0) return [];
  return selectVisibleChats(expandedChats, activeExpandedChat);
}

function ensureTaskTrackingDelegation(root) {
  if (!root || root.__ctoxTaskTrackingDelegated) return;
  root.__ctoxTaskTrackingDelegated = true;
  root.addEventListener('click', (event) => {
    const button = event.target?.closest?.('[data-track-task]');
    if (!button || !root.contains(button) || !button.dataset.taskId) return;
    event.preventDefault();
    event.stopPropagation();
    root.__ctoxBeforeTaskNavigation?.();
    openCtoxTask(
      button.dataset.taskId || '',
      button.dataset.commandId || '',
      button.dataset.taskStatus || '',
    ).catch((error) => {
      console.warn('[business-chat] failed to open CTOX task', error);
    });
  });
}

function renderChatRoot({ root, state, commandBus, db, getActiveModule }) {
  root.__ctoxBeforeTaskNavigation = () => {
    captureDrafts(root, state);
    const expanded = state.chats.filter((chat) => chat.open !== false && !chat.minimized);
    for (const chat of expanded) markChatMinimizedByUser(state, chat);
    state.dockCollapsed = true;
    state.preCollapseExpandedChatIds = expanded.map((chat) => chat.id);
    touchChats(state, expanded);
    renderAndPersistChatState({ root, state, commandBus, db, getActiveModule });
  };
  ensureTaskTrackingDelegation(root);
  const syncFacade = root.__ctoxChatSync || null;
  initSchedulerLoop({
    root,
    state,
    commandBus,
    db,
    sync: syncFacade,
    getActiveModule,
    onTrackingStateChanged: root.__ctoxChatOnTrackingStateChanged || null,
  });

  const selectedDate = state.selectedDate || getLocalDateString(Date.now());
  const chatsOfSelectedDate = state.chats.filter((chat) => getLocalDateString(chat.createdAt) === selectedDate);
  const openChats = chatsOfSelectedDate.filter((chat) => chat.open !== false);
  const expandedChats = openChats.filter((chat) => !chat.minimized);
  const hasMaximized = expandedChats.some((chat) => chat.maximized);
  const activeChat = activeChatFor(state, openChats);
  if (!activeChat && state.activeChatId) state.activeChatId = '';
  const visibleChats = selectVisibleChats(openChats, activeChat);
  const activeExpandedChat = activeChat && !activeChat.minimized
    ? activeChat
    : expandedChats.find((chat) => chat.id === state.activeChatId) || expandedChats[0] || null;
  const dockCollapsed = Boolean(state.dockCollapsed);
  // The stage is a workspace, not a tab switcher. Expanded crew members stay
  // visible together: they sit side by side while they fit and fold into the
  // existing 3D gallery once the available width is exhausted.
  const visibleWindowChats = stageWindowChats(expandedChats, activeExpandedChat);
  const stagedWindowCount = dockCollapsed ? 0 : visibleWindowChats.length;
  const hiddenChatCount = Math.max(0, openChats.length - visibleChats.length);
  const hasVisibleChats = openChats.length > 0;
  const showChatStrip = !Boolean(state.dockCollapsed) && hasVisibleChats;
  const showChatNav = showChatStrip && openChats.length > 1;
  const workload = chatWorkloadForDate(openChats);
  const dockStateClass = [
    Boolean(state.dockCollapsed) ? 'is-collapsed' : '',
    hasVisibleChats ? 'has-visible-chats' : 'has-no-chats',
    openChats.length === 1 ? 'has-one-chat' : '',
    openChats.length > 1 && openChats.length < MANY_CHAT_THRESHOLD ? 'has-few-chats' : '',
    openChats.length >= MANY_CHAT_THRESHOLD ? 'has-many-chats' : '',
    hiddenChatCount > 0 ? 'has-overflow-chats' : '',
    showChatNav ? 'has-nav' : 'has-no-nav',
  ].filter(Boolean).join(' ');
  const wasCollapsed = root.classList.contains('is-collapsed');
  root.classList.toggle('is-collapsed', dockCollapsed);

  // --- SMART IN-PLACE DOM UPDATE FAST-PATH ---
  const datePickerEl = root.querySelector('[data-chat-date-picker]');
  const matchesCurrentDate = datePickerEl && datePickerEl.value === selectedDate;
  const existingWindows = Array.from(root.querySelectorAll('.ctox-chat-window'));
  for (const win of existingWindows) {
    const chat = visibleWindowChats.find(item => item.id === win.dataset.chatId);
    if (chat) chat.inspectionOpen = Boolean(win.querySelector('.ctox-chat-inspection')?.open);
  }
  const hasBusyPanel = Boolean(root.querySelector('[data-chat-busy-panel]'));
  const hasDatePanel = Boolean(root.querySelector('[data-chat-date-workload-panel]'));
  const currentWindowIds = existingWindows.map(w => w.dataset.chatId);
  const visibleWindowChatIds = visibleWindowChats.map(c => c.id);
  const windowShapeUnchanged = existingWindows.length === visibleWindowChats.length
    && currentWindowIds.every((id, idx) => id === visibleWindowChatIds[idx]);
  const attachmentsUnchanged = windowShapeUnchanged && existingWindows.every((win, idx) => (
    win.dataset.chatAttachmentSignature === attachmentSignature(visibleWindowChats[idx])
  ));
  const composerShapeUnchanged = windowShapeUnchanged && existingWindows.every((win, idx) => (
    win.dataset.chatComposerSignature === chatComposerSignature(visibleWindowChats[idx])
  ));
  const taskStateUnchanged = windowShapeUnchanged && existingWindows.every((win, idx) => (
    windowTaskStateMatches(win, visibleWindowChats[idx])
  ));
  // taskStateUnchanged is deliberately NOT required: a status change is exactly
  // when the bar must update WITHOUT a full rebuild. The in-place path below
  // refreshes every task-state-dependent part (window class, chip class, status
  // text, and the chip mark icon), so a rebuild would only cause the jump.
  // The crew pool lives in the FAB and the bar: a new member, a member that
  // starts reading or learning, or the first pool load after boot needs the
  // full render — the in-place path does not touch the creatures.
  const crewPoolUnchanged = (root.dataset?.crewPoolSignature || '') === crewPoolSignature(state);
  const canUpdateInPlace = windowShapeUnchanged &&
                           attachmentsUnchanged &&
                           composerShapeUnchanged &&
                           crewPoolUnchanged &&
                           root.querySelector('[data-chat-dock]') &&
                           wasCollapsed === dockCollapsed &&
                           matchesCurrentDate &&
                           !state.chatListOpen &&
                           !hasBusyPanel &&
                           !state.dateWorkloadOpen &&
                           !hasDatePanel;

  if (canUpdateInPlace) {
    let inPlaceDomChanged = false;
    let layoutDomChanged = false;
    let activeChatChanged = false;

    // 1. Update dock state / collapse class
    const dockEl = root.querySelector('[data-chat-dock]');
    if (dockEl) {
      if (setClassNameIfChanged(dockEl, `ctox-chat-dock ${dockStateClass}`)) {
        inPlaceDomChanged = true;
        layoutDomChanged = true;
      }
    }

    // Update Chat count badge in FAB
    const fabBadge = root.querySelector('.ctox-chat-fab b');
    if (fabBadge) {
      if (setTextIfChanged(fabBadge, openChats.length || '')) inPlaceDomChanged = true;
    }

    // 2. Update active states and details on chips in the dock
    const chips = root.querySelectorAll('.ctox-chat-chip');
    chips.forEach(chip => {
      const chatId = chip.dataset.chatFocus;
      const chat = openChats.find(c => c.id === chatId);
      if (chat) {
        const category = chatWorkjetCategory(chat);
        const categoryChanged = setDatasetIfChanged(chip, 'workjetCategory', category);
        const chipCreature = chip.querySelector('.ctox-crew-creature');
        const crewChanged = Boolean(chipCreature && chipCreature.dataset?.crewIdentity !== JSON.stringify(crewIdentity(chat)));
        if (categoryChanged || crewChanged) {
          if (typeof chip.getAttribute === 'function') {
            if (setAttrIfChanged(chip, 'style', chatWorkjetCategoryStyleText(category, chat))) inPlaceDomChanged = true;
          } else if (setInlineStyleIfChanged(chip, chatWorkjetCategoryStyleText(category, chat))) {
            inPlaceDomChanged = true;
          }
        }
        const taskState = getTaskState(chat);
        const status = chatDockStatusText(chat, taskState);
        const aria = chatDockAriaLabel(chat, status);

        if (setClassNameIfChanged(chip, chatDockClassName(chat, activeChat?.id, taskState))) inPlaceDomChanged = true;
        if (setAttrIfChanged(chip, 'aria-label', aria)) inPlaceDomChanged = true;
        if (setAttrIfChanged(chip, 'title', aria)) inPlaceDomChanged = true;

        const smallEl = chip.querySelector('.ctox-chat-chip-copy small');
        if (smallEl && setTextIfChanged(smallEl, status)) inPlaceDomChanged = true;

        const strongEl = chip.querySelector('.ctox-chat-chip-copy strong');
        if (strongEl && setTextIfChanged(strongEl, crewIdentity(chat).name)) inPlaceDomChanged = true;

        // Refresh the status mark icon in place (spinner → check → warning …).
        // Only replace when the state class actually changed, so a stable chip
        // does not churn its DOM every tick.
        const markEl = chip.querySelector('.ctox-chat-chip-mark');
        if (markEl) {
          const markStateClass = ['running', 'queued', 'success', 'blocked', 'failed', 'scheduled'].includes(taskState)
            ? `is-${taskState}`
            : '';
          const markHasCorrectState = markStateClass
            ? markEl.classList.contains(markStateClass)
            : markEl.className.trim() === 'ctox-chat-chip-mark';
          const markCreature = markEl.querySelector?.('.ctox-crew-creature');
          const markHasCorrectMode = !markCreature
            || markCreature.dataset?.crewMode === crewCreatureMode(chat, taskState);
          if (!markHasCorrectState || !markHasCorrectMode || crewChanged) {
            markEl.outerHTML = chatChipMarkHtml(chat, taskState);
            inPlaceDomChanged = true;
          } else if (markCreature && syncCrewTelemetryNode(markCreature, chat)) {
            inPlaceDomChanged = true;
          }
        }
      }
    });
    root.querySelectorAll('.ctox-chat-fab-creatures:not(.is-members) .ctox-crew-creature').forEach((creature, index) => {
      const chat = (openChats.length ? openChats : [{ id: 'ctox-crew', title: 'Crew' }])[index];
      if (chat && syncCrewTelemetryNode(creature, chat)) inPlaceDomChanged = true;
    });

    // 3. Update active states, 3D relation tags, maximized and minimized classes on windows
    const activeIndex = visibleWindowChats.findIndex((c) => c.id === activeExpandedChat?.id);
    let messagesDomChanged = false;
    existingWindows.forEach((win, idx) => {
      const chat = visibleWindowChats[idx];
      const category = chatWorkjetCategory(chat);
      const relation = idx < activeIndex ? 'left' : idx > activeIndex ? 'right' : 'center';
      const taskState = getTaskState(chat);
      const creatureMode = crewCreatureMode(chat, taskState);
      const activityClass = executionActivityClass(chat);
      const activityTurns = executionProgressForChat(chat)?.activity_turns?.total || 0;

      const isActiveWindow = chat.id === activeExpandedChat?.id;
      const wasActiveWindow = win.classList.contains('is-active');
      const wasMaximized = win.classList.contains('is-maximized');
      const nextWindowClass = [
        'ctox-chat-window',
        chat.maximized ? 'is-maximized' : '',
        isActiveWindow ? 'is-active' : '',
        `is-task-${taskState}`,
        creatureMode === 'review' ? 'is-task-review' : '',
        activityClass,
      ].filter(Boolean).join(' ');
      if (setClassNameIfChanged(win, nextWindowClass)) inPlaceDomChanged = true;
      if (wasActiveWindow !== isActiveWindow) {
        activeChatChanged = true;
        layoutDomChanged = true;
      }
      if (wasMaximized !== Boolean(chat.maximized)) layoutDomChanged = true;
      const activityChanged = setDatasetIfChanged(win, 'activityTurns', activityTurns);
      if (activityChanged && activityClass) {
        win.classList.remove(activityClass);
        void win.offsetWidth;
        win.classList.add(activityClass);
        inPlaceDomChanged = true;
      }
      const categoryChanged = setDatasetIfChanged(win, 'workjetCategory', category);
      const windowCreature = win.querySelector('.ctox-crew-creature');
      const crewChanged = Boolean(windowCreature && windowCreature.dataset?.crewIdentity !== JSON.stringify(crewIdentity(chat)));
      if (categoryChanged || crewChanged) {
        if (typeof win.getAttribute === 'function') {
            if (setAttrIfChanged(win, 'style', chatWorkjetCategoryStyleText(category, chat))) inPlaceDomChanged = true;
        } else if (setInlineStyleIfChanged(win, chatWorkjetCategoryStyleText(category, chat))) {
          inPlaceDomChanged = true;
        }
      }
      if (setDatasetIfChanged(win, 'chatRel', relation)) inPlaceDomChanged = true;
      // Interactive tabindex/aria-hidden churn is only needed when the active
      // window actually changes. Re-applying it every tick was pure DOM noise.
      if (wasActiveWindow !== isActiveWindow) {
        setWindowInteractiveState(win, isActiveWindow);
      }

      // Update title text in header
      const titleButton = win.querySelector('[data-chat-title]');
      if (titleButton) {
        const title = chatWindowTitle(chat);
        if (setAttrIfChanged(titleButton, 'aria-label', title)) inPlaceDomChanged = true;
        if (setAttrIfChanged(titleButton, 'title', title)) inPlaceDomChanged = true;
      }
      const composerInput = win.querySelector('textarea[name="message"]');
      if (composerInput && setAttrIfChanged(composerInput, 'placeholder', chatUiIsGerman()
        ? `Aufgabe für ${crewIdentity(chat).name}...` : `Task for ${crewIdentity(chat).name}...`)) inPlaceDomChanged = true;
      const titleStrong = win.querySelector('.ctox-chat-title strong');
      if (titleStrong && setTextIfChanged(titleStrong, crewIdentity(chat).name)) inPlaceDomChanged = true;
      const titleTask = win.querySelector('.ctox-chat-title-task');
      if (titleTask && setTextIfChanged(titleTask, chat.title || (chatUiIsGerman() ? 'Neue Aufgabe' : 'New task'))) inPlaceDomChanged = true;
      const titleCopy = win.querySelector('.ctox-chat-title-copy');
      const progressHead = win.querySelector('.ctox-chat-progress-head');
      const expectedProgressHead = executionProgressHeaderHtml(chat);
      if (progressHead && !expectedProgressHead) {
        progressHead.remove();
        inPlaceDomChanged = true;
      } else if (!progressHead && expectedProgressHead && titleCopy) {
        titleCopy.insertAdjacentHTML('beforeend', expectedProgressHead);
        inPlaceDomChanged = true;
      } else if (progressHead && expectedProgressHead) {
        const progress = executionProgressForChat(chat);
        const turns = progress?.activity_turns?.total || 0;
        if (setTextIfChanged(progressHead, `${progress?.percent || 0}% · ${turns}T`)) inPlaceDomChanged = true;
        if (setAttrIfChanged(progressHead, 'title', `${turns} Aktivitäten`)) inPlaceDomChanged = true;
      }

      const creature = win.querySelector('.ctox-crew-creature');
      if (creature && (creature.dataset?.crewMode !== creatureMode || crewChanged)) {
        creature.outerHTML = crewCreatureHtml(chat, taskState, 'window');
        inPlaceDomChanged = true;
      } else if (creature && syncCrewTelemetryNode(creature, chat)) {
        inPlaceDomChanged = true;
      }

      // Existing progress remains visible after completion; update its final
      // revision too, so the ring cannot contradict the completed title.
      if (win.querySelector('.ctox-chat-delegation-card')
        || taskState === 'queued' || taskState === 'running' || taskState === 'blocked') {
        const trackingMessage = latestTrackingMessage(chat);
        const taskId = trackingMessage?.taskId || '';
        const commandId = trackingMessage?.commandId || chat.lastTrackingId || '';
        const taskStatus = trackingMessage?.status || 'queued';
        const progressCard = win.querySelector('.ctox-chat-delegation-card');
        const nextSignature = executionProgressSignature(chat);
        if (progressCard?.dataset.progressSignature !== nextSignature || progressCard?.querySelector('[data-track-task]')?.dataset.taskId !== taskId) {
          const expectedCard = delegationProgressCardHtml(chat, { taskId, commandId, taskStatus });
          let cardUpdated = false;
          if (progressCard) {
            progressCard.outerHTML = expectedCard;
            cardUpdated = true;
          } else if (typeof win.querySelector('header')?.insertAdjacentHTML === 'function') {
            win.querySelector('header').insertAdjacentHTML('beforeend', expectedCard);
            cardUpdated = true;
          }
          if (cardUpdated) {
            inPlaceDomChanged = true;
          }
        }
      }

      // Update maximize icon in window header only when maximized state flipped.
      const maxBtn = win.querySelector('[data-chat-maximize]');
      if (maxBtn) {
        const maxLabel = chat.maximized ? 'Arbeitsfenster wiederherstellen' : 'Arbeitsfenster maximieren';
        if (setAttrIfChanged(maxBtn, 'aria-label', maxLabel)) {
          maxBtn.innerHTML = chat.maximized
            ? `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 14 10 14 10 20"></polyline><polyline points="20 10 14 10 14 4"></polyline><line x1="14" y1="10" x2="21" y2="3"></line><line x1="10" y1="14" x2="3" y2="21"></line></svg>`
            : `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 3 21 3 21 9"></polyline><polyline points="9 21 3 21 3 15"></polyline><line x1="21" y1="3" x2="14" y2="10"></line><line x1="3" y1="21" x2="10" y2="14"></line></svg>`;
          inPlaceDomChanged = true;
        }
      }

      const inspection = win.querySelector('.ctox-chat-inspection');
      if (inspection) {
        const content = chatInspectionContent(chat);
        const summary = inspection.querySelector('summary');
        const body = inspection.querySelector('.ctox-chat-inspection-body');
        if (summary.textContent !== content.title) summary.textContent = content.title;
        if (body.innerHTML !== content.body) {
          const scrollTop = body.scrollTop;
          body.innerHTML = content.body;
          body.scrollTop = scrollTop;
          setWindowInteractiveState(win, win.classList.contains('is-active'));
          inPlaceDomChanged = true;
        }
      }
      // Update messages container content if it changed
      const messagesContainer = win.querySelector('.ctox-chat-messages');
      if (messagesContainer) {
        const expectedHtml = (renderChatAgentScopeHtml(chat.contextMeta)
          + (chat.messages.length
            ? chatMessagesMarkup(chat.messages)
            : `<div class="ctox-chat-empty">${escapeHtml(chatUiIsGerman() ? `Gib ${crewIdentity(chat).name} eine Aufgabe.` : `Give ${crewIdentity(chat).name} a task.`)}</div>`)).trim();
        if (messagesContainer.innerHTML.trim() !== expectedHtml) {
          // Only follow new messages when the reader was already at the bottom.
          // Scrolling up is an explicit intent to read history; a running task
          // that keeps appending messages must not yank the view back down.
          const wasPinnedToBottom = isScrolledToBottom(messagesContainer);
          const previousScrollTop = messagesContainer.scrollTop;
          messagesContainer.innerHTML = expectedHtml;
          if (wasPinnedToBottom) messagesContainer.scrollTop = messagesContainer.scrollHeight;
          else messagesContainer.scrollTop = previousScrollTop;
          inPlaceDomChanged = true;
          messagesDomChanged = true;
        }
      }

      // Owner-Befund 04.09.2026: "wenn ich einen chat eintippe, wird der einfach
      // weggeloescht, was ich getippt habe und random wieder eingesetzt."
      //
      // Der Neuaufbau schrieb `chat.draft` bedingungslos in das Feld zurueck.
      // Laeuft dazwischen ein Sync-Takt, traegt das Chat-Objekt einen aelteren
      // Entwurf - und der ueberschreibt, was der Mensch gerade tippt.
      //
      // Wer tippt, hat recht: ein fokussiertes Feld wird nie ueberschrieben,
      // sein Inhalt wird stattdessen in den Zustand uebernommen.
      const textarea = win.querySelector('[name="message"]');
      if (textarea) {
        const wirdGetippt = (win.ownerDocument || document).activeElement === textarea;
        if (wirdGetippt) {
          chat.draft = textarea.value;
        } else if (textarea.value !== (chat.draft || '')) {
          textarea.value = chat.draft || '';
          inPlaceDomChanged = true;
        }
      }
    });

    if (root.dataset) root.dataset.activeChatId = activeExpandedChat?.id || '';
    if (!inPlaceDomChanged) return;
    syncCrewProceduralMotion(root);

    // Geometry is independent from content/status. A new message, progress
    // turn or creature-state change must never recompute window positions,
    // smooth-scroll the dock, or republish the host layout. Those operations
    // used to make the entire bar jump whenever an unrelated task ticked.
    if (layoutDomChanged || activeChatChanged) {
      alignChatWindows(root);
      scrollActiveChatIntoView(root, state, {
        forceDock: activeChatChanged,
        forceMessages: messagesDomChanged,
      });
      updateChatStripOverflowState(root);
      publishChatLayout(root, state);
    } else if (messagesDomChanged) {
      scrollActiveChatIntoView(root, state, { forceDock: false, forceMessages: true });
    }
    return; // Exit early without recreating DOM nodes!
  }
  // --- END OF IN-PLACE DOM UPDATE FAST-PATH ---

  const maxDateVal = getLocalDateString(Date.now() + 10 * 365 * 24 * 60 * 60 * 1000);
  const previousStrip = root.querySelector('[data-chat-strip]');
  const previousStripScrollLeft = previousStrip?.scrollLeft || 0;
  const previousActiveChatId = root.dataset?.activeChatId || '';
  const hadRenderedDock = Boolean(root.querySelector('[data-chat-dock]'));

  if (root.dataset) root.dataset.crewPoolSignature = crewPoolSignature(state);
  root.innerHTML = `
    <section class="ctox-chat-dock ${dockStateClass}" data-chat-dock>
      <button class="ctox-chat-fab" type="button" data-chat-open aria-label="${dockCollapsed ? (chatUiIsGerman() ? 'Crew öffnen' : 'Open crew') : (chatUiIsGerman() ? 'Crew einklappen' : 'Collapse crew')}">
        <span class="ctox-chat-fab-label">Crew</span>
        <span class="ctox-chat-fab-creatures ${(state.crewMembers || []).length ? 'is-members' : ''}" ${(state.crewMembers || []).length ? '' : 'aria-hidden="true"'}>
          ${(state.crewMembers || []).length
            ? state.crewMembers.slice(0, 6).map((member) => crewPoolSlotHtml(member, 'fab')).join('')
            : (openChats.length ? openChats : [{ id: 'ctox-crew', title: 'Crew' }]).slice(0, 3).map((chat) => crewCreatureHtml(chat, getTaskState(chat), 'fab')).join('')}
        </span>
      </button>

      <div class="ctox-chat-date-pill">
        <button class="ctox-date-nav-btn" type="button" data-chat-date-prev aria-label="${chatUiIsGerman() ? 'Vorheriger Tag' : 'Previous day'}">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6"></polyline></svg>
        </button>
        <div class="ctox-date-picker-trigger" role="button" tabindex="0" aria-label="${escapeAttr(chatDateAriaLabel(selectedDate, workload.total))}" title="${escapeAttr(chatDateAriaLabel(selectedDate, workload.total))}">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="18" rx="2" ry="2"></rect><line x1="16" y1="2" x2="16" y2="6"></line><line x1="8" y1="2" x2="8" y2="6"></line><line x1="3" y1="10" x2="21" y2="10"></line></svg>
          <input type="date" class="ctox-date-native-picker" data-chat-date-picker value="${selectedDate}" max="${maxDateVal}" tabindex="-1" aria-hidden="true" />
        </div>
        <button class="ctox-date-nav-btn" type="button" data-chat-date-next aria-label="${chatUiIsGerman() ? 'Nächster Tag' : 'Next day'}">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"></polyline></svg>
        </button>
      </div>

      ${!dockCollapsed ? `
        ${showChatNav ? `<button class="ctox-chat-nav" type="button" data-chat-prev aria-label="${chatUiIsGerman() ? 'Vorheriges Wesen' : 'Previous crew member'}">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6"></polyline></svg>
        </button>` : ''}
        ${showChatStrip ? `<div class="ctox-chat-strip" data-chat-strip aria-label="${chatUiIsGerman() ? 'Aktive Crew' : 'Active crew'}">
          ${visibleChats.map((chat) => chatDockItem(chat, activeChat?.id)).join('')}
          ${hiddenChatCount > 0 ? chatOverflowItem(hiddenChatCount, Boolean(state.chatListOpen)) : ''}
        </div>` : ''}
        ${showChatNav ? `<button class="ctox-chat-nav" type="button" data-chat-next aria-label="${chatUiIsGerman() ? 'Nächstes Wesen' : 'Next crew member'}">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"></polyline></svg>
        </button>` : ''}
        <button class="ctox-chat-new" type="button" data-chat-new aria-label="${chatUiIsGerman() ? 'Neues Wesen zur Crew' : 'Add crew member'}">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="5" x2="12" y2="19"></line><line x1="5" y1="12" x2="19" y2="12"></line></svg>
        </button>
      ` : ''}
    </section>
    ${state.dateWorkloadOpen ? dateWorkloadPanel({ chats: state.chats, selectedDate }) : ''}
    ${state.chatListOpen && openChats.length > MAX_RENDERED_CHAT_TABS ? chatBusyPanel({ chats: openChats, selectedDate, state }) : ''}
    <div class="ctox-chat-stage" data-chat-stage>
      <div class="ctox-chat-stage-inner ${stagedWindowCount === 0 ? 'is-empty' : ''} ${hasMaximized ? 'has-maximized' : ''}">
        ${dockCollapsed ? '' : (() => {
          const activeIndex = visibleWindowChats.findIndex((c) => c.id === activeExpandedChat?.id);
          return visibleWindowChats.map((chat, idx) => {
            const relation = idx < activeIndex ? 'left' : idx > activeIndex ? 'right' : 'center';
            return chatWindow(chat, activeExpandedChat?.id, relation);
          }).join('');
        })()}
        <div class="ctox-chat-stage-spacer" style="position: relative; width: 1px; height: 1px; pointer-events: none; margin-top: -1px;"></div>
      </div>
    </div>
  `;

  root.querySelector('[data-chat-date-prev]')?.addEventListener('click', async () => {
    shiftSelectedDate(state, -1);
    renderAndPersistChatState({ root, state, commandBus, db, getActiveModule });
  });

  root.querySelector('[data-chat-date-next]')?.addEventListener('click', async () => {
    shiftSelectedDate(state, 1);
    renderAndPersistChatState({ root, state, commandBus, db, getActiveModule });
  });

  root.querySelector('[data-chat-date-picker]')?.addEventListener('change', async (event) => {
    const val = event.currentTarget.value;
    if (val) {
      state.selectedDate = val;
      state.dateWorkloadOpen = false;
      renderAndPersistChatState({ root, state, commandBus, db, getActiveModule });
    }
  });

  root.querySelector('[data-chat-date-picker-panel]')?.addEventListener('change', async (event) => {
    const val = event.currentTarget.value;
    if (!val) return;
    state.selectedDate = val;
    state.dateWorkloadOpen = false;
    renderAndPersistChatState({ root, state, commandBus, db, getActiveModule });
  });

  root.querySelectorAll('[data-chat-date-select]').forEach((button) => {
    button.addEventListener('click', async () => {
      const val = button.dataset.chatDateSelect;
      if (!val) return;
      state.selectedDate = val;
      state.dateWorkloadOpen = false;
      renderAndPersistChatState({ root, state, commandBus, db, getActiveModule });
    });
  });

  root.querySelector('[data-chat-date-workload-close]')?.addEventListener('click', (event) => {
    event.preventDefault();
    state.dateWorkloadOpen = false;
    renderChatRoot({ root, state, commandBus, db, getActiveModule });
  });

  root.querySelector('.ctox-date-picker-trigger')?.addEventListener('keydown', (event) => {
    if (event.key !== 'Enter' && event.key !== ' ') return;
    event.preventDefault();
    state.dateWorkloadOpen = !state.dateWorkloadOpen;
    state.chatListOpen = false;
    renderChatRoot({ root, state, commandBus, db, getActiveModule });
  });

  root.querySelector('[data-chat-overflow-open]')?.addEventListener('click', (event) => {
    event.preventDefault();
    event.stopPropagation();
    state.chatListOpen = !state.chatListOpen;
    renderChatRoot({ root, state, commandBus, db, getActiveModule });
  });

  root.querySelector('[data-chat-overflow-close]')?.addEventListener('click', (event) => {
    event.preventDefault();
    state.chatListOpen = false;
    renderChatRoot({ root, state, commandBus, db, getActiveModule });
  });

  root.querySelectorAll('[data-chat-list-filter]').forEach((control) => {
    const updateFilter = () => {
      const key = control.dataset.chatListFilter;
      state.chatListFilter = normalizeChatListFilter(state.chatListFilter);
      state.chatListFilter[key] = control.value;
      renderChatRoot({ root, state, commandBus, db, getActiveModule });
    };
    control.addEventListener(control.tagName === 'INPUT' ? 'input' : 'change', updateFilter);
  });

  root.querySelectorAll('[data-chat-list-focus]').forEach((button) => {
    button.addEventListener('click', async () => {
      const chat = state.chats.find((item) => item.id === button.dataset.chatListFocus);
      if (!chat) return;
      toggleChatFromDock(state, chat);
      state.chatListOpen = false;
      state.dockCollapsed = false;
      touchChats(state, [chat]);
      renderAndPersistChatState({ root, state, commandBus, db, getActiveModule });
    });
  });

  root.querySelector('[data-chat-new]')?.addEventListener('click', async () => {
    const next = createChat(state.ownerUserId, state.selectedDate);
    state.chats.push(next);
    expandChatOnly(state, next);
    state.dockCollapsed = false;
    touchChats(state, [next]);
    renderAndPersistChatState({ root, state, commandBus, db, getActiveModule });
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

  wireCrewDrag(root, state);
  root.querySelectorAll('[data-chat-focus]').forEach((button) => {
    button.addEventListener('click', async () => {
      const chat = state.chats.find((item) => item.id === button.dataset.chatFocus);
      if (!chat) return;
      toggleChatFromDock(state, chat);
      state.dockCollapsed = false;
      touchChats(state, [chat]);
      renderAndPersistChatState({ root, state, commandBus, db, getActiveModule });
    });
  });

  root.querySelectorAll('[data-chat-id]').forEach((node) => {
    const chat = state.chats.find((item) => item.id === node.dataset.chatId);
    if (!chat) return;
    setWindowInteractiveState(node, chat.id === activeChat?.id && !chat.minimized);

    node.addEventListener('click', async (e) => {
      if (node.classList.contains('is-active')) return;
      if (e.target.closest('button, a, input, textarea, form, svg, path')) return;
      const presentationTicket = claimChatOpenOwnership(state);
      state.activeChatId = chat.id;
      markChatExpandedByUser(state, chat, presentationTicket);
      touchChats(state, [chat]);
      renderAndPersistChatState({ root, state, commandBus, db, getActiveModule });
    });

    node.querySelectorAll('[data-chat-minimize]').forEach((button) => button.addEventListener('click', async () => {
      markChatMinimizedByUser(state, chat);
      touchChats(state, [chat]);
      renderAndPersistChatState({ root, state, commandBus, db, getActiveModule });
    }));

    node.querySelectorAll('[data-chat-title]').forEach((titleBtn) => {
      titleBtn.addEventListener('click', async (e) => {
        const presentationTicket = claimChatOpenOwnership(state);
        chat.maximized = !chat.maximized;
        markChatExpandedByUser(state, chat, presentationTicket);
        state.activeChatId = chat.id;
        touchChats(state, [chat]);
        renderAndPersistChatState({ root, state, commandBus, db, getActiveModule });
      });
    });

    node.querySelectorAll('[data-chat-maximize]').forEach((button) => button.addEventListener('click', async () => {
      const presentationTicket = claimChatOpenOwnership(state);
      chat.maximized = !chat.maximized;
      markChatExpandedByUser(state, chat, presentationTicket);
      state.dockCollapsed = false;
      state.activeChatId = chat.id;
      touchChats(state, [chat]);
      renderAndPersistChatState({ root, state, commandBus, db, getActiveModule });
    }));

    node.querySelector('[data-chat-delete]')?.addEventListener('click', async (event) => {
      event.preventDefault();
      event.stopPropagation();
      await deleteChatFromTarget({ root, state, commandBus, db, getActiveModule, target: event.currentTarget });
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
      await submitChatForm({ root, state, chat, node, commandBus, db, sync: syncFacade, getActiveModule });
    };
    form?.addEventListener('submit', submitFromForm);
    form?.querySelector('button[type="submit"]')?.addEventListener('click', submitFromForm);
  });

  root.querySelectorAll('[data-chat-followup-trigger]').forEach((btn) => {
    btn.addEventListener('click', () => {
      const chatId = btn.dataset.chatFollowupTrigger;
      const chat = state.chats.find((item) => item.id === chatId);
      if (chat) {
        chat.showFollowUp = true;
        touchChats(state, [chat]);
        state.lastUiMutationMs = Date.now();
        renderChatRoot({ root, state, commandBus, db, getActiveModule });
        root.querySelector('.ctox-chat-window.is-active textarea[name="message"]')?.focus();
        persistChatState({ state, db }).catch((error) => {
          console.warn('Unable to persist chat follow-up state', error);
        });
      }
    });
  });

  const nextStrip = root.querySelector('[data-chat-strip]');
  if (nextStrip && hadRenderedDock) nextStrip.scrollLeft = previousStripScrollLeft;
  if (root.dataset) root.dataset.activeChatId = activeExpandedChat?.id || '';
  syncCrewProceduralMotion(root);
  alignChatWindows(root);
  scrollActiveChatIntoView(root, state, {
    forceDock: !hadRenderedDock || previousActiveChatId !== (activeExpandedChat?.id || ''),
    forceMessages: true,
  });
  updateChatStripOverflowState(root);
  publishChatLayout(root, state);
  window.requestAnimationFrame(() => {
    root.querySelectorAll('.ctox-chat-window.no-left-transition').forEach((win) => {
      win.classList.remove('no-left-transition');
    });
    updateChatStripOverflowState(root);
  });
}

async function submitChatForm({ root, state, chat, node, commandBus, db, sync, getActiveModule }) {
  if (chat.__submitting) return;
  captureDrafts(root, state);
  const input = node.querySelector('[name="message"]');
  const text = String(input?.value || chat.draft || '').trim();
  if (!text) return;
  const attachments = Array.isArray(chat.attachments) ? chat.attachments.slice() : [];
  moveEmptyHistoricalChatToToday(state, chat);
  focusChatForUser(state, chat, { allowDateChange: true });

  const isFuture = chat.createdAt > Date.now();
  if (isFuture) {
    chat.__submitting = true;
    chat.draft = '';
    chat.showFollowUp = false;
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
        attachments: attachments.map(chatMessageAttachmentSummary),
      });
      
      chat.messages.push({
        id: `status_${commandId}`,
        role: 'ctox',
        text: 'Ausführung verzögert/geplant.',
        promptText: text,
        userMessageId: messageId,
        attachments: attachments.map(chatMessageAttachmentSummary),
        commandId,
        taskId: '',
        status: 'scheduled',
        createdAt: now,
      });
      
      chat.lastTrackingId = commandId;
      chat.scheduledAttachmentsByCommand = {
        ...(chat.scheduledAttachmentsByCommand && typeof chat.scheduledAttachmentsByCommand === 'object' ? chat.scheduledAttachmentsByCommand : {}),
        [commandId]: attachments,
      };
      chat.attachments = [];
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
  const isFollowUpSubmission = chat.showFollowUp === true || Boolean(chat.lastTrackingId);
  chat.showFollowUp = false; // Reset follow-up container state
  if (input) input.value = '';
  try {
    const delivered = await submitChatMessage({
      state,
      chat,
      text,
      commandBus,
      db,
      sync,
      getActiveModule,
      meta: chat.contextMeta || {},
      attachments,
      followUpSubmission: isFollowUpSubmission,
      onPending: () => {
        persistChatState({ state, db, remote: false }).catch(() => {});
        renderChatRoot({ root, state, commandBus, db, getActiveModule });
        root.__ctoxChatOnTrackingStateChanged?.();
      },
    });
    if (delivered) chat.attachments = [];
    await persistChatState({ state, db });
    renderChatRoot({ root, state, commandBus, db, getActiveModule });
    root.__ctoxChatOnTrackingStateChanged?.();
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
  const openingTicket = state.dockCollapsed ? claimChatOpenOwnership(state) : 0;
  let selectedDate = state.selectedDate || getLocalDateString(Date.now());
  if (state.dockCollapsed && !selectedDateHasSubstantiveOpenChat(state, selectedDate)) {
    const preferred = preferredChatForDockOpen(state);
    if (preferred) {
      selectedDate = getLocalDateString(preferred.createdAt);
      state.selectedDate = selectedDate;
      markChatExpandedByUser(state, preferred, openingTicket);
      state.activeChatId = preferred.id;
    } else {
      selectedDate = getLocalDateString(Date.now());
    }
  }
  state.selectedDate = selectedDate;
  const openChats = state.chats.filter((chat) => (
    chat.open !== false && getLocalDateString(chat.createdAt) === selectedDate
  ));
  if (!state.dockCollapsed) {
    invalidateChatOpenOwnership(state);
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
    const restoreSet = new Set(restoreIds);
    const hasRestorableChatForDate = openChats.some((chat) => restoreSet.has(chat.id));
    if (hasRestorableChatForDate) {
      for (const chat of openChats) {
        const nextMinimized = !restoreSet.has(chat.id);
        if (chat.minimized !== nextMinimized) {
          if (nextMinimized) {
            chat.minimized = true;
          } else {
            markChatExpandedByUser(state, chat, openingTicket);
          }
          changedChats.push(chat);
        }
      }
      state.activeChatId = restoreIds.find((id) => openChats.some((chat) => chat.id === id)) || state.activeChatId;
    } else if (!openChats.some((chat) => !chat.minimized)) {
      const chat = ensureChat(state);
      markChatExpandedByUser(state, chat, openingTicket);
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
  if (chat.id === state.activeChatId && !chat.minimized) {
    markChatMinimizedByUser(state, chat);
    return;
  }
  const presentationTicket = claimChatOpenOwnership(state);
  chat.open = true;
  markChatExpandedByUser(state, chat, presentationTicket);
  state.activeChatId = chat.id;
}

async function collapseChatWindow({ root, state, commandBus, db, getActiveModule, target }) {
  const node = target.closest('[data-chat-id]');
  const chat = state.chats.find((item) => item.id === node?.dataset.chatId);
  if (!chat) return;
  captureDrafts(root, state);
  markChatMinimizedByUser(state, chat);
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
  const status = canonicalTrackingStatus(trackingMsg.status);
  if (status === 'scheduled') return 'scheduled';
  if (!status) return 'idle';
  if (status === 'success' || status === 'completed' || status === 'handled' || status === 'done' || status === 'erledigt') return 'success';
  if (isBlockedTrackingStatus(status)) return 'blocked';
  if (['failed', 'error'].includes(status)) return 'failed';
  if (['queued', 'pending', 'pending_sync', 'waiting', 'terminal'].includes(status)) return 'queued';
  if (['running', 'processing', 'executing', 'active'].includes(status)) return 'running';
  return 'idle';
}

function chatTrackingSummary(chat) {
  const messages = Array.isArray(chat?.messages) ? chat.messages : [];
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index] || {};
    const commandId = String(message.commandId || message.command_id || '').trim();
    const taskId = String(message.taskId || message.task_id || '').trim();
    if (!commandId && !taskId) continue;
    const status = canonicalTrackingStatus(message.status || 'queued') || 'queued';
    return {
      tracking_active: message.trackable !== false && isActiveTrackingStatus(status),
      tracking_status: status,
      tracking_id: taskId || commandId,
      tracking_command_id: commandId,
      tracking_task_id: taskId,
      tracking_message_id: String(message.id || '').trim(),
    };
  }
  return {
    tracking_active: false,
    tracking_status: '',
    tracking_id: '',
    tracking_command_id: '',
    tracking_task_id: '',
    tracking_message_id: '',
  };
}

function applyChatTrackingSummary(chat) {
  if (!chat || typeof chat !== 'object') return chat;
  Object.assign(chat, chatTrackingSummary(chat));
  return chat;
}

function expandChatOnly(state, activeChat) {
  state.activeChatId = activeChat.id;
  activeChat.open = true;
  activeChat.minimized = false;
}

// Den angezeigten Tag wechselt nur, wer ausdruecklich dorthin navigiert.
// Vorher setzte diese Funktion selectedDate bedingungslos auf das Datum des
// Chats — und sie wird aus sechs Richtungen gerufen, die meisten davon
// Hintergrundvorgaenge: Wiederherstellung beim Laden, Statusabgleich eines
// alten Vorgangs, Oeffnen eines Lead-Chats. Am 11.08.2026 zog jeder dieser
// Wege die Leiste auf den 26. Juli und riss ein WITTENSTEIN-Fenster von damals
// auf. Ich habe das zuerst an den Aufrufstellen einzeln abgefangen; das war
// Symptombekaempfung, es blieb immer noch ein Weg uebrig. Die Regel gehoert
// hierher: rueckwaerts in die Vergangenheit nur auf ausdrueckliches Verlangen.
function focusChatForUser(state, chat, { openDock = true, allowDateChange = false } = {}) {
  if (!state || !chat) return null;
  const chatDate = getLocalDateString(chat.createdAt || Date.now());
  if (allowDateChange || chatDate === getLocalDateString(Date.now())) {
    state.selectedDate = chatDate;
  }
  if (chat.userMinimized && chat.minimized) {
    // Der Nutzer hat dieses Fenster zugeklappt. Es wird aktiv gefuehrt und der
    // Chip markiert den neuen Zustand — aufgerissen wird es nicht.
    state.activeChatId = chat.id;
    chat.open = true;
    return chat;
  }
  expandChatOnly(state, chat);
  if (openDock) state.dockCollapsed = false;
  return chat;
}

function findChatForOpenDetail(state, detail = {}) {
  const focus = detail.focus && typeof detail.focus === 'object' ? detail.focus : {};
  const taskId = String(detail.task_id || detail.taskId || focus.task_id || focus.taskId || '').trim();
  const commandId = String(detail.command_id || detail.commandId || focus.command_id || focus.commandId || '').trim();
  if (!taskId && !commandId) return null;
  const chats = Array.isArray(state?.chats) ? state.chats : [];
  return [...chats].reverse().find((chat) => {
    if (taskId && String(chat?.lastTrackingId || '').trim() === taskId) return true;
    if (commandId && String(chat?.lastTrackingId || '').trim() === commandId) return true;
    return (Array.isArray(chat?.messages) ? chat.messages : []).some((message) => (
      (taskId && [message?.taskId, message?.task_id, message?.replyFor].some((value) => String(value || '').trim() === taskId))
      || (commandId && [message?.commandId, message?.command_id, message?.replyFor].some((value) => String(value || '').trim() === commandId))
    ));
  }) || null;
}

function chatOpenNeedsHydration(state, detail = {}) {
  const focus = detail.focus && typeof detail.focus === 'object' ? detail.focus : {};
  const taskId = String(detail.task_id || detail.taskId || focus.task_id || focus.taskId || '').trim();
  const commandId = String(detail.command_id || detail.commandId || focus.command_id || focus.commandId || '').trim();
  if (!taskId && !commandId) return false;
  const trackedChat = findChatForOpenDetail(state, detail);
  return !trackedChat || getLocalDateString(trackedChat.createdAt) !== getLocalDateString(Date.now());
}

function resolveChatForOpenDetail(state, session, detail = {}) {
  const trackedChat = findChatForOpenDetail(state, detail);
  // Nur ein Chat von HEUTE wird wiederverwendet. Ein Lead behaelt seinen
  // Kennschluessel ueber Wochen; findChatForOpenDetail lieferte deshalb den
  // Chat des letzten Laufs zurueck, und focusChatForUser zog die Leiste auf
  // dessen Tag. Am 11.08.2026 sprang sie beim Wechsel in eine Kampagne so auf
  // den 26. Juli und riss ein WITTENSTEIN-Fenster von damals auf, waehrend die
  // Laeufe des Tages unsichtbar blieben. Ein heutiger Lauf gehoert zu heute;
  // der alte Verlauf bleibt ueber die Datumsauswahl vollstaendig erreichbar.
  if (trackedChat && getLocalDateString(trackedChat.createdAt) === getLocalDateString(Date.now())) {
    return trackedChat;
  }
  if (detail.reuseActive === true) return ensureChat(state, session);
  const chat = createChat(state.ownerUserId, state.selectedDate);
  state.chats.push(chat);
  return chat;
}

function chatActivityMs(chat) {
  const messages = Array.isArray(chat?.messages) ? chat.messages : [];
  const latestMessage = messages.reduce((latest, message) => {
    const createdAt = Number(message?.createdAt || message?.created_at_ms || 0);
    return Number.isFinite(createdAt) ? Math.max(latest, createdAt) : latest;
  }, 0);
  return Math.max(
    Number(chat?.updated_at_ms || 0) || 0,
    Number(chat?.createdAt || 0) || 0,
    latestMessage,
  );
}

function isSubstantiveChat(chat) {
  return !isChatEmptyForDeletion(chat);
}

function selectedDateHasSubstantiveOpenChat(state, dateStr) {
  return (Array.isArray(state?.chats) ? state.chats : []).some((chat) => (
    chat.open !== false
    && getLocalDateString(chat.createdAt) === dateStr
    && isSubstantiveChat(chat)
  ));
}

function preferredChatForDockOpen(state) {
  const chats = (Array.isArray(state?.chats) ? state.chats : [])
    .filter((chat) => chat.open !== false && isSubstantiveChat(chat))
    .sort((a, b) => chatActivityMs(b) - chatActivityMs(a));
  if (!chats.length) return null;
  const today = getLocalDateString(Date.now());
  // NUR heute. Der Rueckfall auf chats[0] holte den neuesten Chat aus IRGENDEINEM
  // Tag und zog die Leiste auf dessen Datum: am 11.08.2026 sprang sie beim
  // Oeffnen von Outbound auf den 28. Juli und zeigte 26 alte Fehlversuche, waehrend
  // die elf erfolgreichen Laeufe des Tages unsichtbar blieben. Findet sich fuer
  // heute nichts, bleibt die Leiste leer auf heute stehen — vergangene Tage
  // erreicht man ueber die Datumsauswahl, nicht durch einen Sprung hinter dem
  // Ruecken des Nutzers.
  return chats.find((chat) => getLocalDateString(chat.createdAt) === today) || null;
}

function moveEmptyHistoricalChatToToday(state, chat) {
  if (!state || !chat || !isChatEmptyForDeletion(chat)) return false;
  const today = getLocalDateString(Date.now());
  if (getLocalDateString(chat.createdAt) === today) return false;
  const now = Date.now();
  chat.createdAt = now;
  chat.updated_at_ms = now;
  state.selectedDate = today;
  return true;
}

function attachmentSignature(chat) {
  return (Array.isArray(chat?.attachments) ? chat.attachments : [])
    .map((att) => [
      att.attachmentId || att.fileId || att.name || '',
      att.contentHash || '',
      att.size || att.size_bytes || 0,
    ].join(':'))
    .join('|');
}

function chatComposerSignature(chat) {
  // Immediate conversations keep their input across every task transition.
  return getTaskState(chat) === 'scheduled' ? 'scheduled' : 'conversation';
}

function windowTaskStateMatches(win, chat) {
  return Boolean(win?.classList?.contains?.(`is-task-${getTaskState(chat)}`));
}

export function chatAgentScopeViewFromMeta(contextMeta = {}) {
  const clientContext = contextMeta?.client_context && typeof contextMeta.client_context === 'object'
    ? contextMeta.client_context
    : {};
  const nestedScope = clientContext.scope && typeof clientContext.scope === 'object'
    ? clientContext.scope
    : {};
  const view = clientContext.visible_scope && typeof clientContext.visible_scope === 'object'
    ? clientContext.visible_scope
    : nestedScope.visible_scope && typeof nestedScope.visible_scope === 'object'
      ? nestedScope.visible_scope
      : nestedScope.rows && Array.isArray(nestedScope.rows)
        ? nestedScope
        : null;

  if (!view || !Array.isArray(view.rows) || view.rows.length === 0) return null;
  const rows = view.rows
    .filter((row) => row && typeof row === 'object')
    .map((row) => ({
      key: row.key || '',
      label: row.label || '',
      value: row.value || '',
    }))
    .filter((row) => String(row.label || row.value || '').trim());
  if (!rows.length) return null;
  return { ...view, rows };
}

export function renderChatAgentScopeHtml(contextMeta = {}) {
  const view = chatAgentScopeViewFromMeta(contextMeta);
  if (!view) return '';
  return renderGlobalCtoxAgentScopeHtml({ view });
}

function crewIdentityKey(subject = {}) {
  const tracking = Array.isArray(subject.messages) ? latestTrackingMessage(subject) : null;
  return subject.crewKey
    || subject.commandId
    || subject.command_id
    || tracking?.commandId
    || tracking?.command_id
    || subject.taskId
    || subject.task_id
    || tracking?.taskId
    || tracking?.task_id
    || subject.lastTrackingId
    || subject.id
    || subject.createdAt
    || subject.title
    || 'ctox-crew';
}

// Identity comes from the crew, never from a hash: a member of
// `ctox_crew_members` brings its own name, colour and shape. Until the router
// has decided who takes a task, the owner talks to the crew as a whole — one
// neutral creature, the same in the chat bar and on the map.
function crewIdentity(chat = {}) {
  return normalizeCrewAppearance(chat?.crewIdentity);
}

// The member holding a chat's command, as the harness recorded it; the reason
// the router gave, as the owner reads it.
function chatCrewSnapshot(chat) {
  const memberId = String(chat?.crew_member_id || '').trim();
  if (!memberId || !String(chat?.crewIdentity?.name || '').trim()) return {};
  // Cache public presentation only. Queue/member projections still determine
  // assignment; no soul, permissions or execution context comes from this cache.
  return { crew_member_id: memberId, crewIdentity: crewIdentity(chat) };
}

async function findCrewMembersByIds(collection, ids) {
  return findDocsByIds(collection, ids);
}

function selectionSentenceFromEventTitle(title, name) {
  const text = String(title || '');
  const judged = /^(routed|selected):\s*(.*?)\s*\(([^)]*)\):\s*(.*)$/s.exec(text);
  if (judged) return judged[4];
  const pinned = /^(assigned|continuity):\s*(.*?):\s*(.*?)\s*\(([^)]*)\)$/s.exec(text);
  if (pinned) return pinned[2];
  return text.replace(new RegExp(`^${name}[:,]?\\s*`), '');
}

async function crewSelectionReason(events, taskId) {
  if (!events || !taskId || typeof events.find !== 'function') return '';
  try {
    const docs = await events.find({ selector: { task_id: taskId, kind: 'crew_selected' }, limit: 4 }).exec();
    const rows = (Array.isArray(docs) ? docs : []).map((doc) => doc?.toJSON?.() || doc);
    rows.sort((a, b) => (Number(b?.created_at_ms) || 0) - (Number(a?.created_at_ms) || 0));
    return String(rows[0]?.title || '');
  } catch {
    return '';
  }
}

function takeoverText(name, reasonTitle) {
  const reason = selectionSentenceFromEventTitle(reasonTitle, name).trim();
  if (chatUiIsGerman()) return reason ? `${name} übernimmt: ${reason}` : `${name} übernimmt.`;
  return reason ? `${name} takes over: ${reason}` : `${name} takes over.`;
}

function crewCreatureMode(chat, taskState = getTaskState(chat)) {
  if (taskState === 'failed') return 'failed';
  if (taskState === 'running') {
    const phase = String(
      executionProgressForChat(chat)?.phase
      || chat?.executionPhase
      || chat?.execution_phase
      || chat?.routeStatus
      || chat?.status
      || '',
    ).toLowerCase();
    return ['review', 'awaiting_review', 'awaiting-review', 'reviewing', 'validating'].includes(phase)
      ? 'review'
      : 'working';
  }
  if (taskState === 'idle'
      || taskState === 'queued'
      || taskState === 'scheduled'
      || taskState === 'success') return 'sleeping';
  // reading / learning are member expressions (crewMemberExpression).
  return taskState;
}

function stopCrewProceduralMotion(root, { reset = true } = {}) {
  const state = root?.__ctoxCrewProceduralMotion;
  if (state?.frame) window.cancelAnimationFrame(state.frame);
  if (reset) {
    root?.querySelectorAll?.('.ctox-crew-creature')?.forEach((node) => {
      node.style.transform = '';
      const body = node.querySelector('.ctox-crew-body');
      const eyes = node.querySelector('.ctox-crew-eyes');
      if (body) body.style.transform = '';
      if (eyes) eyes.style.transform = '';
    });
  }
  if (root) root.__ctoxCrewProceduralMotion = null;
}

function executionActivityTelemetry(chat) {
  const progress = executionProgressForChat(chat);
  return {
    total: Math.max(0, Number(progress?.activity_turns?.total) || 0),
    thinking: Math.max(0, Number(progress?.activity_turns?.thinking) || 0),
    tools: Math.max(0, Number(progress?.activity_turns?.tools) || 0),
    lastKind: ['thinking', 'tool'].includes(progress?.activity_turns?.last_kind)
      ? progress.activity_turns.last_kind
      : '',
    updatedAt: Math.max(0, Number(progress?.updated_at_ms) || 0),
  };
}

function syncCrewTelemetryNode(node, chat) {
  if (!node) return false;
  const telemetry = executionActivityTelemetry(chat);
  const progress = executionProgressForChat(chat);
  const progressAngle = Math.max(0, Math.min(360, Number(progress?.percent || 0) * 3.6));
  let changed = false;
  changed = setDatasetIfChanged(node, 'activityTurns', telemetry.total) || changed;
  changed = setDatasetIfChanged(node, 'activityKind', telemetry.lastKind) || changed;
  changed = setDatasetIfChanged(node, 'activityUpdatedAt', telemetry.updatedAt) || changed;
  changed = setStyleIfChanged(node, '--ctox-progress-angle', `${progressAngle}deg`) || changed;
  return changed;
}

export function syncCrewProceduralMotion(root) {
  if (!root || typeof window === 'undefined') return;
  if (window.matchMedia?.('(prefers-reduced-motion: reduce)')?.matches) {
    stopCrewProceduralMotion(root);
    return;
  }

  let state = root.__ctoxCrewProceduralMotion;
  if (!state) {
    state = { frame: 0, lastFrameAt: 0, profiles: [], seenTurns: new Map() };
    root.__ctoxCrewProceduralMotion = state;
  }

  const nowMs = Date.now();
  const nextProfiles = Array.from(root.querySelectorAll('.ctox-crew-creature.is-working, .ctox-crew-creature.is-review'))
    .filter((node) => node.closest?.('.ctox-flow-creature-slot') || !node.getClientRects || node.getClientRects().length > 0)
    .slice(0, 36)
    .flatMap((node) => {
      const total = Math.max(0, Number(node.dataset.activityTurns) || 0);
      const key = String(node.dataset.crewKey || node.dataset.crewSeed || 'ctox-crew');
      const previous = state.seenTurns.get(key);
      const previousTotal = typeof previous === 'object' ? previous.total : previous;
      const mode = node.dataset.crewMode || 'working';
      const modeChanged = Boolean(previous && typeof previous === 'object' && previous.mode !== mode);
      state.seenTurns.set(key, { total, mode });
      const updatedAt = Math.max(0, Number(node.dataset.activityUpdatedAt) || 0);
      const freshInitialEvent = previous === undefined && total > 0 && nowMs - updatedAt <= 8000;
      if (!(total > (previousTotal ?? total)) && !freshInitialEvent && !modeChanged) return [];
      const body = node.querySelector('.ctox-crew-body');
      const eyes = node.querySelector('.ctox-crew-eyes');
      if (!body || !eyes) return [];
      const seed = (Number(node.dataset.crewSeed || 0) ^ Math.imul(total || 1, 2654435761)) >>> 0;
      const unit = (offset) => ((seed >>> offset) & 1023) / 1023;
      const kind = node.dataset.activityKind || 'tool';
      const duration = mode === 'review' ? 2200 : kind === 'thinking' ? 1800 : 1400;
      return [{
        key,
        node,
        body,
        eyes,
        mode,
        kind,
        startAt: 0,
        duration,
        direction: unit(12) > .5 ? 1 : -1,
        amplitude: .88 + unit(2) * .28,
      }];
    });

  if (nextProfiles.length) {
    const restartedKeys = new Set(nextProfiles.map((profile) => profile.key));
    state.profiles = state.profiles.filter((profile) => !restartedKeys.has(profile.key));
    state.profiles.push(...nextProfiles);
  }
  if (state.frame || state.profiles.length === 0) return;

  const tick = (now) => {
    if (!root.isConnected || root.__ctoxCrewProceduralMotion !== state) return;
    if (document.visibilityState === 'hidden') {
      state.frame = window.requestAnimationFrame(tick);
      return;
    }
    if (now - state.lastFrameAt < 33) {
      state.frame = window.requestAnimationFrame(tick);
      return;
    }
    state.lastFrameAt = now;
    state.profiles = state.profiles.filter(({ node }) => node.isConnected);
    state.profiles.forEach((profile) => {
      if (!profile.startAt) profile.startAt = now;
      const elapsed = now - profile.startAt;
      const progress = Math.min(1, elapsed / profile.duration);
      const envelope = Math.sin(progress * Math.PI);
      const bounce = Math.sin(progress * Math.PI * 2);
      if (profile.mode === 'review') {
        const x = profile.direction * envelope * 7.5 * profile.amplitude;
        const y = -envelope * 2.5;
        const rotation = profile.direction * bounce * 8 * envelope;
        const flow = envelope * .09;
        profile.node.style.transform = `translate(${x.toFixed(3)}px, ${y.toFixed(3)}px) rotate(${rotation.toFixed(3)}deg)`;
        profile.body.style.transform = `scale(${(1 + flow).toFixed(4)}, ${(1 - flow * .72).toFixed(4)}) skewX(${(profile.direction * envelope * 5).toFixed(3)}deg)`;
        profile.eyes.style.transform = `translateX(${(profile.direction * envelope * 5.5).toFixed(3)}px) rotate(${(profile.direction * bounce * 4).toFixed(3)}deg)`;
      } else if (profile.kind === 'thinking') {
        const rotation = profile.direction * envelope * 10.5 * profile.amplitude;
        const y = -envelope * 3;
        const flow = envelope * .07;
        profile.node.style.transform = `translateY(${y.toFixed(3)}px) rotate(${rotation.toFixed(3)}deg)`;
        profile.body.style.transform = `scale(${(1 - flow * .45).toFixed(4)}, ${(1 + flow).toFixed(4)}) skewX(${(-profile.direction * envelope * 4).toFixed(3)}deg)`;
        profile.eyes.style.transform = `translateX(${(profile.direction * bounce * 5.5).toFixed(3)}px) rotate(${(-rotation * .34).toFixed(3)}deg)`;
      } else {
        const impact = Math.sin(progress * Math.PI);
        const recoil = Math.sin(progress * Math.PI * 3) * envelope;
        const x = profile.direction * recoil * 2.8;
        const y = -impact * 7.5 * profile.amplitude;
        const rotation = profile.direction * recoil * 5.5;
        const squash = impact * .14;
        profile.node.style.transform = `translate(${x.toFixed(3)}px, ${y.toFixed(3)}px) rotate(${rotation.toFixed(3)}deg)`;
        profile.body.style.transform = `scale(${(1 + squash).toFixed(4)}, ${(1 - squash * .92).toFixed(4)}) skewX(${(profile.direction * recoil * 4).toFixed(3)}deg)`;
        profile.eyes.style.transform = `translateY(${(-impact * 2.4).toFixed(3)}px) rotate(${(-rotation * .45).toFixed(3)}deg)`;
      }
    });
    state.profiles = state.profiles.filter((profile) => {
      if (now - profile.startAt < profile.duration) return true;
      profile.node.style.transform = '';
      profile.body.style.transform = '';
      profile.eyes.style.transform = '';
      return false;
    });
    if (state.profiles.length > 0) state.frame = window.requestAnimationFrame(tick);
    else state.frame = 0;
  };
  state.frame = window.requestAnimationFrame(tick);
}

export function crewCreatureHtml(chat, taskState = getTaskState(chat), placement = 'dock') {
  return renderCrewCreature({
    appearance: crewIdentity(chat),
    animationKey: crewIdentityKey(chat),
    taskState,
    mode: crewCreatureMode(chat, taskState),
    placement,
    progressPercent: executionProgressForChat(chat)?.percent || 0,
    activity: executionActivityTelemetry(chat),
  });
}

function normalizeExecutionProgress(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  const steps = Array.isArray(value.steps)
    ? value.steps.map((step, index) => ({
        position: Number(step?.position) || index + 1,
        label: String(step?.label || `Schritt ${index + 1}`),
        status: String(step?.status || 'pending').toLowerCase(),
        activity_turns: Math.max(0, Number(step?.activity_turns ?? step?.activityTurns) || 0),
      }))
    : [];
  if (!steps.length) return null;
  const rawActivity = value.activity_turns || value.activityTurns;
  const activity = rawActivity && typeof rawActivity === 'object'
    ? rawActivity
    : {};
  return {
    version: Number(value.version) || 1,
    revision: Math.max(1, Number(value.revision) || 1),
    phase: String(value.phase || 'work'),
    percent: Math.max(0, Math.min(100, Number(value.percent) || 0)),
    current_step: Number(value.current_step ?? value.currentStep) || null,
    completed_steps: Math.max(0, Number(value.completed_steps ?? value.completedSteps) || 0),
    total_steps: Math.max(steps.length, Number(value.total_steps ?? value.totalSteps) || 0),
    steps,
    review: value.review && typeof value.review === 'object'
      ? { status: String(value.review.status || 'pending').toLowerCase() }
      : { status: 'pending' },
    activity_turns: {
      total: Math.max(0, Number(activity.total) || 0),
      thinking: Math.max(0, Number(activity.thinking) || 0),
      tools: Math.max(0, Number(activity.tools) || 0),
      last_kind: String(activity.last_kind || activity.lastKind || value.lastActivityKind || '').toLowerCase(),
    },
    updated_at_ms: Math.max(0, Number(value.updated_at_ms ?? value.updatedAtMs) || 0),
  };
}

function latestTrackingMessage(chat) {
  const messages = Array.isArray(chat?.messages) ? chat.messages : [];
  return [...messages].reverse().find((message) => (
    message?.executionProgress
    || message?.taskId
    || message?.commandId
  )) || null;
}

function executionProgressForChat(chat) {
  const latest = latestTrackingMessage(chat);
  const taskId = trackingIdFromMessage(latest, 'task');
  const commandId = trackingIdFromMessage(latest, 'command');
  // Status and plan projections arrive independently. A new status without a
  // plan must not hide the last plan for this task, nor inherit another task's
  // plan when the conversation receives a follow-up command.
  const previousPlan = [...(chat?.messages || [])].reverse().find(message => (
    message.executionProgress && (taskId
      ? trackingIdFromMessage(message, 'task') === taskId
      : commandId && trackingIdFromMessage(message, 'command') === commandId)
  ));
  return normalizeExecutionProgress(
    chat?.executionProgress
    || chat?.execution_progress
    || latest?.executionProgress
    || previousPlan?.executionProgress,
  );
}


function executionProgressSignature(chat) {
  const progress = executionProgressForChat(chat);
  const status = canonicalTrackingStatus(latestTrackingMessage(chat)?.status || getTaskState(chat) || '');
  if (!progress) return `planning:${status}`;
  return [
    progress.revision,
    progress.percent,
    progress.phase,
    progress.review.status,
    progress.activity_turns.total,
    progress.activity_turns.last_kind,
    progress.updated_at_ms,
    status,
  ].join(':');
}

function executionActivityClass(chat) {
  const kind = executionProgressForChat(chat)?.activity_turns?.last_kind;
  if (kind === 'thinking' || kind === 'tool') return `has-activity-${kind}`;
  return '';
}

function executionProgressHeaderHtml(chat) {
  return '';
}

function executionStepStatusLabel(status) {
  if (status === 'completed') return 'Erledigt';
  if (status === 'in_progress') return 'Aktiv';
  return 'Offen';
}

function executionProgressTooltip(progress, taskStatus = '') {
  if (!progress) return chatUiIsGerman() ? 'Plan wird erstellt' : 'Creating plan';
  const isReviewPhase = progressShowsActiveReview(progress, taskStatus);
  const current = isReviewPhase
    ? null
    : progress.steps.find((step) => step.position === progress.current_step)
      || progress.steps.find((step) => step.status === 'in_progress')
      || progress.steps.at(-1);
  const attributedTurns = progress.steps.reduce((total, step) => total + step.activity_turns, 0);
  const stepTurns = isReviewPhase
    ? Math.max(0, progress.activity_turns.total - attributedTurns)
    : Math.max(0, Number(current?.activity_turns) || 0);
  const nextIndex = Math.max(0, progress.steps.findIndex((step) => step.position === current?.position));
  const next = isReviewPhase
    ? null
    : progress.steps.slice(nextIndex + 1).find((step) => step.status !== 'completed');
  const lines = [
    isReviewPhase ? 'Prüfung' : current?.label || 'Crew',
    `${progress.percent}% · ${stepTurns}/${progress.activity_turns.total} Aktivitäten · Planstand ${progress.revision}`,
    `Denkschritte ${progress.activity_turns.thinking} · Werkzeugeinsätze ${progress.activity_turns.tools}`,
    ...progress.steps.map((step) => `${step.status === 'completed' ? '✓' : step.status === 'in_progress' ? '●' : '○'} ${step.label}`),
  ];
  if (next) lines.push(`→ ${next.label}`);
  return lines.join('\n');
}

function progressShowsActiveReview(progress, taskStatus = '') {
  if (!progress) return false;
  const status = canonicalTrackingStatus(taskStatus);
  if (isFailureStatus(status) || isCancelledTrackingStatus(status)) return false;
  return progress.phase === 'review' || progress.phase === 'completed';
}

function delegationProgressCardHtml(chat, { taskId = '', commandId = '', taskStatus = 'queued' } = {}) {
  const progress = executionProgressForChat(chat);
  if (!progress) {
    const isPlanning = taskStatus === 'queued' || taskStatus === 'running' || taskStatus === 'blocked';
    const tooltip = isPlanning
      ? (chatUiIsGerman() ? 'Plan wird erstellt' : 'Creating plan')
      : (chatUiIsGerman() ? 'Noch kein Ausführungsplan' : 'No execution plan yet');
    return `
      <div class="ctox-chat-delegation-card ${isPlanning ? 'is-planning' : 'is-dormant'}" data-progress-signature="${escapeAttr(executionProgressSignature(chat))}">
        <button class="ctox-progress-visual ${isPlanning ? 'is-planning' : 'is-dormant'}" type="button" style="--ctox-progress-percent:0" data-track-task ${taskId ? '' : 'disabled'} data-task-id="${escapeAttr(taskId)}" data-command-id="${escapeAttr(commandId)}" data-task-status="${escapeAttr(taskStatus)}" aria-label="${escapeAttr(tooltip)}" title="${escapeAttr(tooltip)}">
          <span class="ctox-progress-activity" style="--ctox-turn-angle:0deg" aria-hidden="true"><i></i></span>
          <span class="ctox-progress-track">
            <span class="ctox-progress-work"><span class="ctox-progress-planning-line"></span></span>
            <span class="ctox-progress-review is-pending"></span>
          </span>
        </button>
      </div>
    `;
  }

  const isReviewPhase = progressShowsActiveReview(progress, taskStatus);
  const current = isReviewPhase
    ? null
    : progress.steps.find((step) => step.position === progress.current_step)
      || progress.steps.find((step) => step.status === 'in_progress')
      || progress.steps.at(-1);
  const lastKind = progress.activity_turns.last_kind;
  const activityClass = lastKind === 'thinking' || lastKind === 'tool' ? `is-${lastKind}` : '';
  const reviewStatus = progress.review.status;
  const attributedTurns = progress.steps.reduce((total, step) => total + step.activity_turns, 0);
  const stepTurns = isReviewPhase
    ? Math.max(0, progress.activity_turns.total - attributedTurns)
    : Math.max(0, Number(current?.activity_turns) || 0);
  const turnAngle = (stepTurns % 60) * 6;
  const workSegments = progress.steps.map((step) => `
    <span class="ctox-progress-segment is-${escapeAttr(step.status)}" title="${escapeAttr(`${step.position}. ${step.label}: ${executionStepStatusLabel(step.status)}`)}"></span>
  `).join('');
  const tooltip = executionProgressTooltip(progress, taskStatus);

  return `
    <div class="ctox-chat-delegation-card ${activityClass}" data-progress-signature="${escapeAttr(executionProgressSignature(chat))}">
      <button class="ctox-progress-visual ${isReviewPhase ? 'is-reviewing' : ''}" type="button" style="--ctox-progress-percent:${Math.max(0, Math.min(100, Number(progress.percent) || 0))}" data-track-task ${taskId ? '' : 'disabled'} data-task-id="${escapeAttr(taskId)}" data-command-id="${escapeAttr(commandId)}" data-task-status="${escapeAttr(taskStatus)}" aria-label="${escapeAttr(tooltip)}" title="${escapeAttr(tooltip)}">
        <span class="ctox-progress-activity" style="--ctox-turn-angle:${turnAngle}deg" aria-hidden="true"><i></i></span>
        <span class="ctox-progress-track" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow="${progress.percent}">
          <div class="ctox-progress-work">${workSegments}</div>
          <span class="ctox-progress-review is-${escapeAttr(reviewStatus)}" title="Prüfung: ${escapeAttr(reviewStatus)}"></span>
        </span>
      </button>
    </div>
  `;
}

function chatWorkjetCategory(chat) {
  return normalizeWorkjetCategory(
    chat?.contextMeta?.workjet_category || chat?.contextMeta?.workjetCategory,
  );
}

function chatWorkjetCategoryStyleText(category, chat = null) {
  const style = workjetCategoryStyle(category);
  const crewColor = chat ? crewIdentity(chat).color : style.accent;
  return [
    `--shell-category-accent:${style.accent}`,
    `--shell-category-foreground:${style.foreground}`,
    `--shell-category-soft:${style.soft}`,
    `--shell-category-border:${style.border}`,
    `--crew-color:${crewColor}`,
  ].join(';');
}

function chatWindowTitle(chat) {
  return [
    `${crewIdentity(chat).name} · ${chat.title || (chatUiIsGerman() ? 'Neue Aufgabe' : 'New task')}`,
    chatDockStatusText(chat, getTaskState(chat)),
    executionProgressTooltip(executionProgressForChat(chat), getTaskState(chat)),
  ].filter(Boolean).join('\n');
}

function chatWindow(chat, activeId, relation = 'center') {

  const moduleName = chat.contextMeta?.module || 'ctox';
  const category = chatWorkjetCategory(chat);
  const categoryStyleText = chatWorkjetCategoryStyleText(category, chat);
  const taskState = getTaskState(chat);
  const creatureMode = crewCreatureMode(chat, taskState);
  const crew = crewIdentity(chat);
  const isFuture = chat.createdAt > Date.now();
  const agentScopeHtml = renderChatAgentScopeHtml(chat.contextMeta);

  const stagedAttachments = chat.attachments || [];
  const attachmentsHtml = stagedAttachments.length ? `
    <div class="ctox-chat-attachments-preview">
      ${stagedAttachments.map((att, idx) => `
        <div class="ctox-attachment-item" data-att-idx="${idx}">
          ${String(att.mimeType || att.mime_type || '').startsWith('image/')
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
      <span class="ctox-chat-status-badge is-running" title="Die Crew arbeitet…">
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
  } else if (taskState === 'blocked') {
    statusBadgeHtml = `
      <span class="ctox-chat-status-badge is-blocked" title="Wartet — blockiert, läuft weiter sobald der Block fällt">
        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="11" width="18" height="11" rx="2"></rect><path d="M7 11V7a5 5 0 0 1 10 0v4"></path></svg>
        <span>Blockiert</span>
      </span>
    `;
  } else if (taskState === 'failed') {
    statusBadgeHtml = `
      <span class="ctox-chat-status-badge is-failed" title="Fehlgeschlagen">
        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="9" x2="12" y2="13"></line><line x1="12" y1="17" x2="12.01" y2="17"></line><path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"></path></svg>
        <span>Fehlgeschlagen</span>
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
  const tracking = chatTrackingSummary(chat);
  const headerProgressHtml = delegationProgressCardHtml(chat, { taskId: tracking.tracking_task_id, commandId: tracking.tracking_command_id, taskStatus: taskState });
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
  } else {
    // Keep conversation input available while work waits, runs, or finishes.
    // Follow-up submissions retain the chat thread and use the normal queue.
    bottomHtml = `
      ${attachmentsHtml}
      <form class="ctox-chat-form" data-chat-form>
        <input type="file" multiple accept="image/*,application/pdf" style="display: none;" data-chat-file-input="${chat.id}" />
        <button type="button" class="ctox-chat-clip-btn" data-chat-clip="${chat.id}" title="Datei hinzufügen">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48"></path></svg>
        </button>
        <textarea name="message" placeholder="${escapeAttr(chatUiIsGerman() ? `Aufgabe für ${crew.name}...` : `Task for ${crew.name}...`)}" required>${escapeHtml(chat.draft || '')}</textarea>
        <button type="submit" data-chat-send aria-label="Senden">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="19" x2="12" y2="5"></line><polyline points="5 12 12 5 19 12"></polyline></svg>
        </button>
      </form>
    `;
  }

  const isMinimizedClass = chat.minimized ? 'is-minimized' : '';
  const taskStateClass = `is-task-${taskState}`;
  const windowTitle = chatWindowTitle(chat);

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
    <section class="ctox-chat-window no-left-transition ${chat.maximized ? 'is-maximized' : ''} ${chat.id === activeId ? 'is-active' : ''} ${isMinimizedClass} ${taskStateClass} ${creatureMode === 'review' ? 'is-task-review' : ''} ${executionActivityClass(chat)}" data-chat-id="${escapeAttr(chat.id)}" data-chat-module="${escapeAttr(moduleName)}" data-workjet-category="${escapeAttr(category)}" style="${escapeAttr(categoryStyleText)}" data-chat-rel="${escapeAttr(relation)}" data-chat-attachment-signature="${escapeAttr(attachmentSignature(chat))}" data-chat-composer-signature="${escapeAttr(chatComposerSignature(chat))}" data-activity-turns="${escapeAttr(executionProgressForChat(chat)?.activity_turns?.total || 0)}">
      <header>
        <button class="ctox-chat-title" type="button" data-chat-title="${escapeAttr(chat.id)}" aria-label="${escapeAttr(windowTitle)}" title="${escapeAttr(windowTitle)}">
          ${crewCreatureHtml(chat, taskState, 'window')}
        </button>
        <div class="ctox-chat-header-actions">
          <button type="button" data-chat-maximize aria-label="${chat.maximized ? 'Arbeitsfenster wiederherstellen' : 'Arbeitsfenster maximieren'}" title="${chat.maximized ? 'Wiederherstellen' : 'Maximieren'}">
            ${chat.maximized 
              ? `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 14 10 14 10 20"></polyline><polyline points="20 10 14 10 14 4"></polyline><line x1="14" y1="10" x2="21" y2="3"></line><line x1="10" y1="14" x2="3" y2="21"></line></svg>` 
              : `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 3 21 3 21 9"></polyline><polyline points="9 21 3 21 3 15"></polyline><line x1="21" y1="3" x2="14" y2="10"></line><line x1="3" y1="21" x2="10" y2="14"></line></svg>`}
          </button>
          <button type="button" data-chat-minimize aria-label="Wesen einklappen" title="Einklappen">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="5" y1="12" x2="19" y2="12"></line></svg>
          </button>
          <button type="button" data-chat-delete aria-label="Wesen aus der Crew entfernen" title="Entfernen" class="is-delete">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path><line x1="10" y1="11" x2="10" y2="17"></line><line x1="14" y1="11" x2="14" y2="17"></line></svg>
          </button>
        </div>
        ${headerProgressHtml}
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
      ${chatInspectionMarkup(chat)}
      <div class="ctox-chat-messages">
        ${agentScopeHtml}
        ${chat.messages.length ? chatMessagesMarkup(chat.messages) : `<div class="ctox-chat-empty">${escapeHtml(chatUiIsGerman() ? `Gib ${crew.name} eine Aufgabe.` : `Give ${crew.name} a task.`)}</div>`}
      </div>
      ${bottomHtml}
    </section>
  `;
}

function selectVisibleChats(openChats, activeChat) {
  if (openChats.length <= MAX_RENDERED_CHAT_TABS) return openChats;
  const activeIndex = Math.max(0, openChats.findIndex((chat) => chat.id === activeChat?.id));
  const half = Math.floor(MAX_RENDERED_CHAT_TABS / 2);
  const start = Math.max(0, Math.min(activeIndex - half, openChats.length - MAX_RENDERED_CHAT_TABS));
  return openChats.slice(start, start + MAX_RENDERED_CHAT_TABS);
}

function chatWorkloadForDate(chats) {
  const byStatus = new Map();
  const byModule = new Map();
  const bySource = new Map();
  const byHour = new Map();
  for (const chat of chats) {
    const status = getTaskState(chat);
    const moduleName = chat.contextMeta?.module || 'ctox';
    const source = chatSource(chat);
    const hour = String(new Date(chat.createdAt || Date.now()).getHours()).padStart(2, '0');
    byStatus.set(status, (byStatus.get(status) || 0) + 1);
    byModule.set(moduleName, (byModule.get(moduleName) || 0) + 1);
    bySource.set(source, (bySource.get(source) || 0) + 1);
    byHour.set(hour, (byHour.get(hour) || 0) + 1);
  }
  return {
    total: chats.length,
    byStatus,
    byModule,
    bySource,
    byHour,
  };
}

function formatCompactCount(count) {
  const value = Number(count) || 0;
  if (value >= 1000) return `${Math.floor(value / 100) / 10}k`;
  return String(value);
}

function chatOverflowItem(hiddenCount, active) {
  return `
    <button class="ctox-chat-overflow-chip ${active ? 'is-active' : ''}" type="button" data-chat-overflow-open aria-label="${escapeAttr(hiddenCount)} weitere Crew-Mitglieder anzeigen" title="${escapeAttr(hiddenCount)} weitere Wesen">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><circle cx="5" cy="12" r="2"></circle><circle cx="12" cy="12" r="2"></circle><circle cx="19" cy="12" r="2"></circle></svg>
    </button>
  `;
}

function chatBusyPanel({ chats, selectedDate, state }) {
  const filters = normalizeChatListFilter(state.chatListFilter);
  const stats = chatWorkloadForDate(chats);
  const filtered = filterBusyChats(chats, filters);
  const statusOptions = ['all', ...Array.from(stats.byStatus.keys()).sort()];
  const moduleOptions = ['all', ...Array.from(stats.byModule.keys()).sort()];
  const sourceOptions = ['all', ...Array.from(stats.bySource.keys()).sort()];
  const hourOptions = ['all', ...Array.from(stats.byHour.keys()).sort((a, b) => Number(a) - Number(b))];
  const groupOptions = [
    ['auto', 'Gruppen: Auto'],
    ['thread', 'Nach Serie'],
    ['source', 'Nach Quelle'],
    ['hour', 'Nach Stunde'],
    ['status', 'Nach Status'],
    ['none', 'Keine Gruppen'],
  ];
  const list = busyListMarkup({ filtered, filters, activeId: state.activeChatId });
  return `
    <section class="ctox-chat-busy-panel" data-chat-busy-panel aria-label="Crew-Einsätze für ${escapeAttr(formatGermanDateLabel(selectedDate))}">
      <header>
        <div>
          <strong>${escapeHtml(formatGermanDateLabel(selectedDate))}</strong>
          <span>${formatCompactCount(stats.total)} Aufgaben, ${formatCompactCount(filtered.length)} sichtbar</span>
        </div>
        <button type="button" data-chat-overflow-close aria-label="Crew-Übersicht schließen">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
        </button>
      </header>
      <div class="ctox-chat-busy-stats">
        ${busyStatMarkup('total', stats.total)}
        ${Array.from(stats.byStatus.entries()).sort((a, b) => b[1] - a[1]).slice(0, 4).map(([status, count]) => busyStatMarkup(status, count)).join('')}
      </div>
      <div class="ctox-chat-busy-filters">
        <select data-chat-list-filter="group" aria-label="Gruppierung wählen">
          ${groupOptions.map(([value, label]) => `<option value="${escapeAttr(value)}" ${filters.group === value ? 'selected' : ''}>${escapeHtml(label)}</option>`).join('')}
        </select>
        <select data-chat-list-filter="status" aria-label="Status filtern">
          ${statusOptions.map((value) => `<option value="${escapeAttr(value)}" ${filters.status === value ? 'selected' : ''}>${escapeHtml(value === 'all' ? 'Alle Status' : value)}</option>`).join('')}
        </select>
        <select data-chat-list-filter="module" aria-label="Modul filtern">
          ${moduleOptions.map((value) => `<option value="${escapeAttr(value)}" ${filters.module === value ? 'selected' : ''}>${escapeHtml(value === 'all' ? 'Alle Module' : value)}</option>`).join('')}
        </select>
        <select data-chat-list-filter="source" aria-label="Quelle filtern">
          ${sourceOptions.map((value) => `<option value="${escapeAttr(value)}" ${filters.source === value ? 'selected' : ''}>${escapeHtml(value === 'all' ? 'Alle Quellen' : value)}</option>`).join('')}
        </select>
        <select data-chat-list-filter="hour" aria-label="Stunde filtern">
          ${hourOptions.map((value) => `<option value="${escapeAttr(value)}" ${filters.hour === value ? 'selected' : ''}>${escapeHtml(value === 'all' ? 'Alle Stunden' : `${value}:00`)}</option>`).join('')}
        </select>
        <input type="search" data-chat-list-filter="text" value="${escapeAttr(filters.text)}" placeholder="Suchen" aria-label="Crew-Einsätze suchen" />
      </div>
      <div class="ctox-chat-busy-list" data-chat-busy-list>
        ${list.html}
        ${list.remaining > 0 ? `<div class="ctox-chat-busy-more">${formatCompactCount(list.remaining)} weitere Treffer durch Filter eingrenzen</div>` : ''}
        ${list.groupRemaining > 0 ? `<div class="ctox-chat-busy-more">${formatCompactCount(list.groupRemaining)} weitere Gruppen durch Filter eingrenzen</div>` : ''}
      </div>
    </section>
  `;
}

function dateWorkloadPanel({ chats, selectedDate }) {
  const days = workloadDaysAround(chats, selectedDate, 28);
  const max = Math.max(1, ...days.map((day) => day.count));
  const selected = days.find((day) => day.date === selectedDate);
  return `
    <section class="ctox-date-workload-panel" data-chat-date-workload-panel aria-label="Aufgaben nach Datum nach Datum">
      <header>
        <div>
          <strong>${escapeHtml(formatGermanDateLabel(selectedDate))}</strong>
          <span>${formatCompactCount(selected?.count || 0)} Aufgaben am ausgewaehlten Tag</span>
        </div>
        <button type="button" data-chat-date-workload-close aria-label="Datumsauswahl schliessen">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
        </button>
      </header>
      <input type="date" data-chat-date-picker-panel value="${escapeAttr(selectedDate)}" aria-label="Datum wählen" />
      <div class="ctox-date-heatmap" role="list" aria-label="Aufgaben nach Datum der umliegenden Tage">
        ${days.map((day) => dateHeatmapDay(day, max, selectedDate)).join('')}
      </div>
    </section>
  `;
}

function workloadDaysAround(chats, selectedDate, count) {
  const byDate = new Map();
  for (const chat of chats) {
    const date = getLocalDateString(chat.createdAt || Date.now());
    byDate.set(date, (byDate.get(date) || 0) + 1);
  }
  const selected = dateFromLocalDateString(selectedDate);
  const before = Math.floor(count / 2);
  return Array.from({ length: count }, (_, index) => {
    const date = new Date(selected);
    date.setDate(selected.getDate() + index - before);
    const dateStr = getLocalDateString(date);
    return { date: dateStr, count: byDate.get(dateStr) || 0 };
  });
}

function dateHeatmapDay(day, max, selectedDate) {
  const intensity = day.count <= 0 ? 0 : Math.max(1, Math.ceil((day.count / max) * 4));
  const date = dateFromLocalDateString(day.date);
  const label = `${formatGermanDateLabel(day.date)}: ${day.count} Aufgaben`;
  return `
    <button class="ctox-date-heatmap-day ${day.date === selectedDate ? 'is-selected' : ''}" type="button" data-chat-date-select="${escapeAttr(day.date)}" data-intensity="${intensity}" aria-label="${escapeAttr(label)}">
      <span>${date.getDate()}</span>
      <b>${day.count ? formatCompactCount(day.count) : ''}</b>
    </button>
  `;
}

function normalizeChatListFilter(filter = {}) {
  return {
    group: filter.group || 'auto',
    status: filter.status || 'all',
    module: filter.module || 'all',
    source: filter.source || 'all',
    hour: filter.hour || 'all',
    text: String(filter.text || '').trim().toLowerCase(),
  };
}

function filterBusyChats(chats, filters) {
  return chats.filter((chat) => {
    const status = getTaskState(chat);
    const moduleName = chat.contextMeta?.module || 'ctox';
    const source = chatSource(chat);
    const sourceTitle = chat.contextMeta?.source_title || '';
    const threadKey = chatSeriesKey(chat) || '';
    const hour = String(new Date(chat.createdAt || Date.now()).getHours()).padStart(2, '0');
    const haystack = `${chat.title || ''} ${moduleName} ${source} ${sourceTitle} ${threadKey} ${status} ${(chat.messages || []).map((message) => message.text || '').join(' ')}`.toLowerCase();
    return (filters.status === 'all' || status === filters.status)
      && (filters.module === 'all' || moduleName === filters.module)
      && (filters.source === 'all' || source === filters.source)
      && (filters.hour === 'all' || hour === filters.hour)
      && (!filters.text || haystack.includes(filters.text));
  });
}

function chatSource(chat) {
  return chat.contextMeta?.source_module || chat.contextMeta?.source_title || chat.contextMeta?.module || 'ctox';
}

function busyListMarkup({ filtered, filters, activeId }) {
  if (filters.group === 'none' || filtered.length <= MAX_RENDERED_CHAT_TABS) {
    const rows = filtered.slice(0, MAX_BUSY_LIST_ITEMS);
    return {
      html: rows.map((chat) => busyChatRow(chat, activeId)).join(''),
      remaining: Math.max(0, filtered.length - rows.length),
      groupRemaining: 0,
    };
  }

  const groups = groupBusyChats(filtered, filters.group);
  const visibleGroups = visibleBusyGroups(groups, activeId);
  const rowMap = allocateBusyGroupRows(visibleGroups);
  const renderedRows = Array.from(rowMap.values()).reduce((sum, rows) => sum + rows.length, 0);
  const html = visibleGroups.map((group) => busyChatGroup(group, rowMap.get(group.key) || [], activeId)).join('');

  return {
    html,
    remaining: Math.max(0, filtered.length - renderedRows),
    groupRemaining: Math.max(0, groups.length - visibleGroups.length),
  };
}

function groupBusyChats(chats, mode = 'auto') {
  const groups = new Map();
  for (const chat of chats) {
    const descriptor = chatGroupDescriptor(chat, mode);
    const key = descriptor.key;
    if (!groups.has(key)) {
      groups.set(key, {
        key,
        label: descriptor.label,
        detail: descriptor.detail,
        chats: [],
        statusCounts: new Map(),
        earliestCreated: chat.createdAt || Date.now(),
        latestUpdated: chat.updated_at_ms || chat.createdAt || Date.now(),
      });
    }
    const group = groups.get(key);
    group.chats.push(chat);
    const status = getTaskState(chat);
    group.statusCounts.set(status, (group.statusCounts.get(status) || 0) + 1);
    group.earliestCreated = Math.min(group.earliestCreated, chat.createdAt || Date.now());
    group.latestUpdated = Math.max(group.latestUpdated, chat.updated_at_ms || chat.createdAt || Date.now());
  }
  return Array.from(groups.values())
    .map((group) => ({
      ...group,
      chats: group.chats.slice().sort((a, b) => (b.updated_at_ms || b.createdAt || 0) - (a.updated_at_ms || a.createdAt || 0)),
    }))
    .sort((a, b) => b.chats.length - a.chats.length || b.latestUpdated - a.latestUpdated || a.label.localeCompare(b.label));
}

function visibleBusyGroups(groups, activeId) {
  const visible = groups.slice(0, MAX_BUSY_GROUPS);
  const activeGroup = groups.find((group) => group.chats.some((chat) => chat.id === activeId));
  if (activeGroup && !visible.some((group) => group.key === activeGroup.key)) {
    visible[Math.max(0, visible.length - 1)] = activeGroup;
  }
  return visible;
}

function allocateBusyGroupRows(groups) {
  const rowsByGroup = new Map(groups.map((group) => [group.key, []]));
  let renderedRows = 0;
  let added = true;
  while (renderedRows < MAX_BUSY_LIST_ITEMS && added) {
    added = false;
    for (const group of groups) {
      if (renderedRows >= MAX_BUSY_LIST_ITEMS) break;
      const rows = rowsByGroup.get(group.key);
      if (!rows || rows.length >= group.chats.length) continue;
      rows.push(group.chats[rows.length]);
      renderedRows += 1;
      added = true;
    }
  }
  return rowsByGroup;
}

function chatGroupDescriptor(chat, mode) {
  const status = getTaskState(chat);
  const source = chatSource(chat);
  const moduleName = chat.contextMeta?.module || 'ctox';
  const hour = String(new Date(chat.createdAt || Date.now()).getHours()).padStart(2, '0');
  const seriesKey = chatSeriesKey(chat);
  const titleSignature = normalizedTaskSignature(chat.contextMeta?.source_title || chat.contextMeta?.title || chat.title || '');
  const titleLabel = chat.contextMeta?.source_title || chat.contextMeta?.title || chat.title || source || 'Aufgaben';

  if (mode === 'thread' && seriesKey) {
    return { key: `series:${seriesKey}`, label: titleLabel, detail: 'Serie' };
  }
  if (mode === 'source') {
    return { key: `source:${source}`, label: source, detail: moduleName };
  }
  if (mode === 'hour') {
    return { key: `hour:${hour}`, label: `${hour}:00`, detail: 'Stunde' };
  }
  if (mode === 'status') {
    return { key: `status:${status}`, label: status, detail: 'Status' };
  }
  if (seriesKey) {
    return { key: `series:${seriesKey}`, label: titleLabel, detail: `${source} · Serie` };
  }
  if (source && source !== 'ctox') {
    return { key: `source-title:${source}:${titleSignature || 'tasks'}`, label: titleLabel || source, detail: source };
  }
  if (titleSignature && titleSignature !== 'ctox') {
    return { key: `title:${titleSignature}`, label: titleLabel, detail: moduleName };
  }
  return { key: `hour:${hour}`, label: `${hour}:00`, detail: `${moduleName} · Stunde` };
}

function chatSeriesKey(chat) {
  const meta = chat.contextMeta && typeof chat.contextMeta === 'object' ? chat.contextMeta : {};
  const payload = meta.payload && typeof meta.payload === 'object' ? meta.payload : {};
  const clientContext = meta.client_context && typeof meta.client_context === 'object' ? meta.client_context : {};
  const candidates = [
    meta.thread_key,
    meta.threadKey,
    meta.group_key,
    meta.groupKey,
    payload.thread_key,
    payload.threadKey,
    payload.group_key,
    payload.groupKey,
    clientContext.thread_key,
    clientContext.threadKey,
    clientContext.group_key,
    clientContext.groupKey,
    meta.record_id,
    meta.recordId,
  ].map((value) => String(value || '').trim()).filter(Boolean);
  return candidates.find((value) => value !== chat.id && !value.endsWith(`/${chat.id}`)) || '';
}

function normalizedTaskSignature(value) {
  return String(value || '')
    .toLowerCase()
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/\b(cmd|task|chat|run)_[a-z0-9-]+\b/g, ' ')
    .replace(/[a-f0-9]{8,}(?:-[a-f0-9]{4,})+/g, ' ')
    .replace(/\d{2,}/g, ' ')
    .replace(/[^a-z0-9]+/g, ' ')
    .trim()
    .slice(0, 80);
}

function busyStatMarkup(label, count) {
  return `<span><b>${formatCompactCount(count)}</b><small>${escapeHtml(label)}</small></span>`;
}

function busyChatGroup(group, rows, activeId) {
  const first = group.chats[0];
  const statusSummary = Array.from(group.statusCounts.entries())
    .sort((a, b) => b[1] - a[1])
    .slice(0, 3)
    .map(([status, count]) => `${formatCompactCount(count)} ${status}`)
    .join(' · ');
  const remaining = Math.max(0, group.chats.length - rows.length);
  return `
    <section class="ctox-chat-busy-group" data-chat-busy-group="${escapeAttr(group.key)}">
      <button class="ctox-chat-busy-group-head ${group.chats.some((chat) => chat.id === activeId) ? 'is-active' : ''}" type="button" data-chat-list-focus="${escapeAttr(first?.id || '')}">
        <span class="ctox-chat-busy-group-copy">
          <strong>${escapeHtml(group.label || 'Aufgaben')}</strong>
          <small>${escapeHtml([formatCompactCount(group.chats.length) + ' Aufgaben', group.detail, statusSummary].filter(Boolean).join(' · '))}</small>
        </span>
      </button>
      <div class="ctox-chat-busy-group-rows">
        ${rows.map((chat) => busyChatRow(chat, activeId)).join('')}
        ${remaining > 0 ? `<div class="ctox-chat-busy-group-more">+${formatCompactCount(remaining)} in dieser Gruppe</div>` : ''}
      </div>
    </section>
  `;
}

function busyChatRow(chat, activeId) {
  const status = getTaskState(chat);
  const moduleName = chat.contextMeta?.module || 'ctox';
  const time = getFormattedTime(chat.createdAt || Date.now());
  return `
    <button class="ctox-chat-busy-row ${chat.id === activeId ? 'is-active' : ''}" type="button" data-chat-list-focus="${escapeAttr(chat.id)}">
      <span class="ctox-chat-busy-time">${escapeHtml(time)}</span>
      <span class="ctox-chat-busy-main">
        <strong>${escapeHtml(chat.title || 'Crew')}</strong>
        <small>${escapeHtml(moduleName)} · ${escapeHtml(status)}</small>
      </span>
    </button>
  `;
}

// The chip status mark (spinner / check / warning …) as standalone markup, so
// the in-place fast-path can refresh it on a status change without rebuilding
// the whole dock. Previously any task-state change forced a full innerHTML
// rebuild of the chat bar — which is why the strip visibly jumped on every
// research status refresh.
function chatChipMarkHtml(chat, taskState) {
  const creature = crewCreatureHtml(chat, taskState, 'dock');
  if (taskState === 'running') {
    return `<span class="ctox-chat-chip-mark is-running" aria-hidden="true">${creature}<span class="ctox-crew-state-dot"><span class="ctox-chip-spinner"></span></span></span>`;
  }
  if (taskState === 'queued') {
    return `<span class="ctox-chat-chip-mark is-queued" aria-hidden="true">${creature}<span class="ctox-crew-state-dot"></span></span>`;
  }
  if (taskState === 'success') {
    return `<span class="ctox-chat-chip-mark is-success" aria-hidden="true">${creature}<span class="ctox-crew-state-dot"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="4.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"></polyline></svg></span></span>`;
  }
  if (taskState === 'blocked') {
    return `<span class="ctox-chat-chip-mark is-blocked" aria-hidden="true">${creature}<span class="ctox-crew-state-dot"></span></span>`;
  }
  if (taskState === 'failed') {
    return `<span class="ctox-chat-chip-mark is-failed" aria-hidden="true">${creature}<span class="ctox-crew-state-dot"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="4.5" stroke-linecap="round"><line x1="12" y1="9" x2="12" y2="13"></line><line x1="12" y1="17" x2="12.01" y2="17"></line></svg></span></span>`;
  }
  if (taskState === 'scheduled') {
    return `<span class="ctox-chat-chip-mark is-scheduled" aria-hidden="true">${creature}<span class="ctox-crew-state-dot"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="4.2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="8"></circle><polyline points="12 7 12 12 15 14"></polyline></svg></span></span>`;
  }
  return `<span class="ctox-chat-chip-mark" aria-hidden="true">${creature}</span>`;
}

function chatDockItem(chat, activeId) {
  const taskState = getTaskState(chat);
  const status = chatDockStatusText(chat, taskState);
  const moduleName = chat.contextMeta?.module || 'ctox';
  const category = chatWorkjetCategory(chat);
  const categoryStyleText = chatWorkjetCategoryStyleText(category, chat);
  const markHtml = chatChipMarkHtml(chat, taskState);
  const crew = crewIdentity(chat);
  const taskTitle = String(chat.title || (chatUiIsGerman() ? 'Neue Aufgabe' : 'New task')).trim();

  return `
    <button class="${chatDockClassName(chat, activeId, taskState)}" type="button" data-chat-focus="${escapeAttr(chat.id)}" data-chat-module="${escapeAttr(moduleName)}" data-workjet-category="${escapeAttr(category)}" style="${escapeAttr(categoryStyleText)}" aria-label="${escapeAttr(chatDockAriaLabel(chat, status))}" title="${escapeAttr(`${crew.name} · ${taskTitle} · ${status}`)}">
      ${markHtml}
    </button>
  `;
}

function chatDockStatusText(chat, taskState = getTaskState(chat)) {
  const labels = {
    running: 'Aktiv',
    queued: 'Wartet',
    success: 'Erledigt',
    blocked: 'Blockiert',
    failed: 'Fehler',
    scheduled: 'Geplant',
  };
  if (labels[taskState]) return labels[taskState];
  const count = Array.isArray(chat.messages) ? chat.messages.length : 0;
  return count ? `${count} ${count === 1 ? 'Eintrag' : 'Einträge'}` : 'Bereit';
}

function chatDockClassName(chat, activeId, taskState = getTaskState(chat)) {
  return [
    'ctox-chat-chip',
    `is-task-${taskState}`,
    chat.id === activeId && !chat.minimized ? 'is-active' : '',
    chat.minimized ? 'is-minimized' : '',
    !chat.minimized ? 'is-expanded' : '',
  ].filter(Boolean).join(' ');
}

function chatDockAriaLabel(chat, status) {
  const crew = crewIdentity(chat);
  const title = String(chat?.title || (chatUiIsGerman() ? 'Neue Aufgabe' : 'New task')).trim();
  const visibility = chat?.minimized ? 'minimiert' : 'geöffnet';
  return `${crew.name}, ${title}: ${status}, ${visibility}`;
}

function activeChatFor(state, openChats = state.chats.filter((chat) => chat.open !== false)) {
  if (!openChats.length) return null;
  let active = openChats.find((chat) => chat.id === state.activeChatId);
  const expanded = openChats.filter((chat) => !chat.minimized);
  if (!active || (active.minimized && expanded.length)) {
    active = expanded[0] || openChats[openChats.length - 1];
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
  const presentationTicket = claimChatOpenOwnership(state);
  markChatExpandedByUser(state, next, presentationTicket);
  state.activeChatId = next.id;
  return next;
}

// Jede asynchrone Oeffnung besitzt genau ein Ticket. Minimieren oder eine
// spaetere Oeffnung entwertet es. Ein nach await zurueckkehrender Pfad darf
// weder erneut montieren noch den lokal neueren Darstellungszustand ersetzen.
function claimChatOpenOwnership(state) {
  if (!state) return 0;
  state.chatOpenOwnershipTicket = Number(state.chatOpenOwnershipTicket || 0) + 1;
  return state.chatOpenOwnershipTicket;
}

function currentChatOpenOwnership(state) {
  return Number(state?.chatOpenOwnershipTicket || 0);
}

function ownsChatOpenOwnership(state, ticket) {
  return Number(ticket) === currentChatOpenOwnership(state);
}

function invalidateChatOpenOwnership(state) {
  return claimChatOpenOwnership(state);
}

function markChatMinimizedByUser(state, chat) {
  if (!state || !chat) return 0;
  invalidateChatOpenOwnership(state);
  const now = Date.now();
  chat.open = true;
  chat.minimized = true;
  chat.userMinimized = true;
  chat.presentation_updated_at_ms = now;
  state.lastUiMutationMs = now;
  return now;
}

function markChatExpandedByUser(state, chat, ticket = claimChatOpenOwnership(state)) {
  if (!state || !chat || !ownsChatOpenOwnership(state, ticket)) return false;
  const now = Date.now();
  chat.open = true;
  chat.minimized = false;
  chat.userMinimized = false;
  chat.presentation_updated_at_ms = now;
  state.lastUiMutationMs = now;
  return true;
}

function touchChats(state, chats) {
  const now = Date.now();
  state.lastUiMutationMs = now;
  chats.forEach((chat) => {
    if (!chat) return;
    chat.owner_user_id = chat.owner_user_id || state.ownerUserId || '';
    chat.updated_at_ms = now;
    applyChatTrackingSummary(chat);
  });
}

function shouldDeferRemoteChatHydration(root, state) {
  const needsTrackingSync = hasTrackedMessagesNeedingSync(state);
  const active = typeof document !== 'undefined' ? document.activeElement : null;
  const hasFocusedChatControl = Boolean(active?.closest?.('[data-chat-id]'))
    && /^(TEXTAREA|INPUT|SELECT|BUTTON)$/.test(active?.tagName || '');
  if (hasFocusedChatControl && !needsTrackingSync) return true;
  const lastMutation = Number(state?.lastUiMutationMs || 0);
  return !needsTrackingSync && lastMutation > 0 && Date.now() - lastMutation < 2500;
}

// A reader who has scrolled up is reading; only a view already resting at the
// bottom should follow new messages. The threshold absorbs sub-pixel rounding
// and the last rendered line's descent, so "close enough to the bottom" still
// counts as following.
const CHAT_BOTTOM_PIN_THRESHOLD_PX = 24;

function isScrolledToBottom(container) {
  if (!container) return true;
  const distanceFromBottom = container.scrollHeight - container.scrollTop - container.clientHeight;
  return distanceFromBottom <= CHAT_BOTTOM_PIN_THRESHOLD_PX;
}

function scrollActiveChatIntoView(root, state, { forceDock = true, forceMessages = true } = {}) {
  const activeChip = Array.from(root.querySelectorAll('[data-chat-focus]'))
    .find((node) => node.dataset.chatFocus === state.activeChatId);
  // scrollIntoView on every no-op tick re-animates the dock strip and is the
  // visible "chat bar jumps by itself" symptom. Only call it when the active
  // chip is actually outside the strip's visible range.
  if (forceDock && activeChip && !isChipMostlyVisible(root, activeChip)) {
    activeChip.scrollIntoView?.({ inline: 'center', block: 'nearest', behavior: 'smooth' });
  }
  updateChatStripOverflowState(root);

  // Follow new messages only in windows the reader left at the bottom. This ran
  // unconditionally over EVERY open window before, so scrolling up in one chat
  // was undone on the next render — including renders caused by an unrelated
  // chat receiving a message. In-place ticks pass forceMessages only when the
  // message HTML actually changed; a full rebuild keeps the default true so a
  // freshly created window still lands at the latest message.
  if (!forceMessages) return;
  root.querySelectorAll('[data-chat-id]:not(.is-minimized)').forEach((node) => {
    const messagesContainer = node.querySelector('.ctox-chat-messages');
    if (!messagesContainer) return;
    // Fresh nodes after a full rebuild report distanceFromBottom === 0 even when
    // empty, so isScrolledToBottom is true and we pin to the end. A reader who
    // scrolled up has a larger distance and is left alone.
    if (isScrolledToBottom(messagesContainer)) {
      const nextTop = messagesContainer.scrollHeight;
      if (messagesContainer.scrollTop !== nextTop) {
        messagesContainer.scrollTop = nextTop;
      }
    }
  });
}

function isChipMostlyVisible(root, chip) {
  const strip = root?.querySelector?.('[data-chat-strip]');
  if (!strip || !chip) return true;
  const stripRect = strip.getBoundingClientRect?.();
  const chipRect = chip.getBoundingClientRect?.();
  if (!stripRect || !chipRect || stripRect.width <= 0) return true;
  // Fully inside or only slightly clipped still counts as visible — no scroll.
  return chipRect.left >= stripRect.left - 2 && chipRect.right <= stripRect.right + 2;
}

function updateChatStripOverflowState(root) {
  const strip = root?.querySelector?.('[data-chat-strip]');
  if (!strip) return;
  const maxScroll = Math.max(0, strip.scrollWidth - strip.clientWidth);
  const scrollable = maxScroll > 1;
  const atStart = !scrollable || strip.scrollLeft <= 1;
  const atEnd = !scrollable || strip.scrollLeft >= maxScroll - 1;
  if (strip.classList.contains('is-scrollable') !== scrollable) {
    strip.classList.toggle('is-scrollable', scrollable);
  }
  if (strip.classList.contains('is-at-start') !== atStart) {
    strip.classList.toggle('is-at-start', atStart);
  }
  if (strip.classList.contains('is-at-end') !== atEnd) {
    strip.classList.toggle('is-at-end', atEnd);
  }
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

function friendlyCrewMessage(text) {
  const value = String(text || '');
  const de = chatUiIsGerman();
  const selection = value.match(/^(.{1,80} übernimmt):/);
  if (selection) return selection[1];
  if (/^Der Versuch wird wiederholt: CTOX chat could not continue because the model API/.test(value)) return de ? 'Der Modelldienst antwortet nicht. Die Crew versucht es erneut.' : 'The model service is not responding. The crew will retry.';
  if (value === 'Aufgabe in der CTOX Queue angelegt. Fortschritt und Antwort erscheinen hier.') {
    return de ? 'Die Crew hat deine Aufgabe erhalten. Fortschritt und Antwort erscheinen hier.'
      : 'The crew has received your task. Progress and the reply will appear here.';
  }
  if (/^CTOX konnte die Aufgabe nicht ausführen: CTOX chat could not continue because the model API is (temporarily unavailable|rate-limited)\./.test(value)) {
    return /rate-limited/.test(value)
      ? (de ? 'Die Crew konnte die Aufgabe nicht abschließen: Der Modelldienst hat zu viele Anfragen erhalten.' : 'The crew could not complete the task: The model service received too many requests.')
      : (de ? 'Die Crew konnte die Aufgabe nicht abschließen: Der Modelldienst war nicht erreichbar.' : 'The crew could not complete the task: The model service was unavailable.');
  }
  if (/^Der Versuch wird wiederholt: Harness retry feedback injected after runtime failure: direct session (error|timeout)/.test(value)) {
    return de ? 'Die Crew versucht es erneut. Die Verbindung zum Modelldienst wurde unterbrochen.'
      : 'The crew is trying again. The connection to the model service was interrupted.';
  }
  if (/^Task angelegt und in der CTOX Queue\.(?: Antwort erscheint hier, sobald (?:CTOX ihn|der CTOX Service ihn) verarbeitet\.)?$/.test(value)) {
    return de
      ? 'Deine Aufgabe steht auf der Aufgabenliste. Die Antwort erscheint hier, sobald die Crew sie bearbeitet hat.'
      : 'Your task is on the task list. The reply will appear here when the crew has worked on it.';
  }
  const legacy = {
    'Command wird an CTOX übergeben.': ['Aufgabe wird an die Crew gesendet.', 'Sending your task to the crew.'],
    'CTOX hat die Automatisierung ausgeführt.': ['Die Crew hat den Auftrag ausgeführt.', 'The crew has completed the task.'],
    'CTOX führt die Automatisierung aus.': ['Die Crew führt den Auftrag aus.', 'The crew is working on the task.'],
    'Verbindung unterbrochen. Annahme durch CTOX noch nicht bestätigt.': ['Verbindung unterbrochen. Die Crew hat die Annahme noch nicht bestätigt.', 'Connection interrupted. The crew has not confirmed receipt yet.'],
    'CTOX konnte die Aufgabe nicht ausführen.': ['Die Crew konnte die Aufgabe nicht abschließen.', 'The crew could not complete the task.'],
  };
  if (legacy[value]) return legacy[value][de ? 0 : 1];
  return humanizeHarnessLine(value);
}

// Harness status lines arrive as "<Satz>: technical:worker-runtime-api-failure — 2026-09-08T04:37:18Z".
// The bar shows the sentence and a short reason in words plus the clock time.
function humanizeHarnessLine(text) {
  const match = String(text || '').match(/^(.*?):\s*([a-z_]+):([a-z0-9_-]+)\s*[—-]\s*(\d{4}-\d{2}-\d{2}T[\d:.]+(?:Z|[+-]\d{2}:\d{2})?)\s*$/i);
  if (!match) return text;
  const [, sentence, klass, code, iso] = match;
  const when = new Date(iso);
  const clock = Number.isFinite(when.getTime()) ? when.toLocaleTimeString(chatUiIsGerman() ? 'de-DE' : 'en-GB', { hour: '2-digit', minute: '2-digit' }) : '';
  const de = chatUiIsGerman();
  const detail = /worker-runtime-api-failure|model.*(?:unavailable|timeout)|api.*failure/i.test(code)
    ? (de ? 'Die Anfrage an den Modelldienst ist fehlgeschlagen.' : 'The request to the model service failed.')
    : klass === 'review'
      ? (de ? 'Die Prüfung wurde nicht bestanden.' : 'The review did not pass.')
      : klass === 'policy'
        ? (de ? 'Eine Zugriffsregel verhindert die Bearbeitung.' : 'An access rule prevents this work.')
        : (de ? 'Die Bearbeitung wurde durch einen Fehler unterbrochen.' : 'Work was interrupted by an error.');
  return `${sentence.replace(/\bCTOX\b/g, de ? 'Die Crew' : 'The crew')} · ${detail}${clock ? ` · ${clock}` : ''}`;
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

function messageMarkup(message, { conversation = false } = {}) {
  const trackId = message.taskId || message.commandId;
  const visibleTrackId = compactTrackingId(trackId);
  const tracking = message.trackable === false || !message.taskId ? '' : (message.commandId || message.taskId)
    ? `<button class="ctox-chat-track" type="button" data-track-task data-task-id="${escapeAttr(message.taskId || '')}" data-command-id="${escapeAttr(message.commandId || '')}" data-task-status="${escapeAttr(message.status || '')}" title="${escapeAttr(`${trackButtonLabel(message)} · ${trackId}`)}" aria-label="${escapeAttr(`${trackButtonLabel(message)} · ${trackId}`)}"><svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M14 3h7v7"></path><path d="M10 14 21 3"></path><path d="M21 14v5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5"></path></svg><code class="ctox-chat-track-id">${escapeHtml(visibleTrackId)}</code></button>`
    : '';
  const rawText = conversation || message.role === 'user' ? String(message.text || '') : friendlyCrewMessage(message.text);
  const promptIsLong = message.role === 'user'
    && (rawText.length > 180 || rawText.split('\n').length > 2);
  const compactText = rawText.replace(/\s+/g, ' ').trim();
  const promptBody = promptIsLong
    ? `<details class="ctox-chat-prompt">
        <summary>
          <span class="ctox-chat-prompt-preview">${escapeHtml(compactText.slice(0, 180))}${compactText.length > 180 ? '…' : ''}</span>
          <span class="ctox-chat-prompt-more">Mehr</span>
          <span class="ctox-chat-prompt-less">Weniger</span>
        </summary>
        <div class="ctox-chat-prompt-full">${formatChatBodyHtml(rawText)}</div>
      </details>`
    : formatChatBodyHtml(rawText);
  return `
    <article class="ctox-chat-message is-${escapeAttr(message.role || 'ctox')}">
      <div class="ctox-chat-body">${promptBody}</div>
      ${tracking ? `<footer>${tracking}</footer>` : ''}
    </article>
  `;
}

function compactTrackingId(value) {
  const id = String(value || '').trim();
  if (id.length <= 16) return id;
  const tail = id.split(/[:/]/).filter(Boolean).at(-1) || id;
  if (tail.length <= 14) return `…${tail}`;
  return `…${tail.slice(-12)}`;
}

function isChatInspectionMessage(message) {
  if (message.role === 'user' || message.role === 'assistant') return false;
  if (['reply', 'question', 'interim'].includes(message.kind)) return false;
  if (message.kind === 'status' || message.takeoverFor || message.blockedFor || message.failureFor || String(message.id || '').startsWith('status_')) return true;
  // Old clients copied queue receipts out of command.result into replyFor.
  // Keep those receipts in inspection, without rewriting the stored conversation.
  if (message.replyFor) return /^(?:Aufgabe (?:wird|in der)|Task angelegt|Deine Aufgabe steht|Die Crew hat deine Aufgabe erhalten|Die Bearbeitung hat begonnen|Der Versuch wird wiederholt|CTOX konnte die Aufgabe)/.test(String(message.text || ''));
  return false;
}

function chatMessagesMarkup(messages = []) {
  return messages.filter(message => !isChatInspectionMessage(message)).map(message => messageMarkup(message, { conversation: true })).join('');
}

function chatInspectionContent(chat) {
  const messages = (chat.messages || []).filter(isChatInspectionMessage);
  const current = messages.at(-1);
  const progress = executionProgressForChat(chat);
  const de = chatUiIsGerman();
  const state = getTaskState(chat);
  const title = state === 'success' ? (de ? 'Aufgabe abgeschlossen' : 'Task completed')
    : state === 'failed' ? (de ? 'Bearbeitung fehlgeschlagen' : 'Task failed')
      : current ? friendlyCrewMessage(current.text) : (de ? 'Bereit' : 'Ready');
  const states = de ? { pending: 'Offen', in_progress: 'In Arbeit', completed: 'Erledigt', failed: 'Fehlgeschlagen', blocked: 'Wartet' }
    : { pending: 'Pending', in_progress: 'Working', completed: 'Done', failed: 'Failed', blocked: 'Waiting' };
  const steps = (progress?.steps || []).map(step => `<li><span>${escapeHtml(step.label)}</span><small>${escapeHtml(states[step.status] || (de ? 'Offen' : 'Pending'))}</small></li>`).join('');
  const history = messages.map(message => `<li><time>${escapeHtml(Number(message.createdAt) > 0 ? new Date(Number(message.createdAt)).toLocaleTimeString(de ? 'de-DE' : 'en-GB', { hour: '2-digit', minute: '2-digit' }) : '')}</time><span>${escapeHtml(friendlyCrewMessage(message.text))}</span></li>`).join('');
  const tracked = [...(chat.messages || [])].reverse().find(message => message.taskId && message.trackable !== false);
  const link = tracked ? `<button type="button" class="ctox-button" title="${escapeAttr(tracked.taskId)}" data-track-task data-task-id="${escapeAttr(tracked.taskId)}" data-command-id="${escapeAttr(tracked.commandId || '')}" data-task-status="${escapeAttr(tracked.status || '')}">${de ? 'Aufgabe öffnen' : 'Open task'}</button>` : '';
  return { title, body: `${steps ? `<ol class="ctox-chat-inspection-steps">${steps}</ol>` : ''}<ol class="ctox-chat-inspection-history">${history}</ol>${link}` };
}

function chatInspectionMarkup(chat) {
  const content = chatInspectionContent(chat);
  return `<details class="ctox-chat-inspection" ${chat.inspectionOpen ? 'open' : ''}><summary>${escapeHtml(content.title)}</summary><div class="ctox-chat-inspection-body">${content.body}</div></details>`;
}

async function submitChatMessage({
  state,
  chat,
  text,
  commandBus,
  db,
  sync,
  getActiveModule,
  meta = {},
  attachments = [],
  followUpSubmission = false,
  onPending = null,
}) {
  const activeModule = getActiveModule?.() || { id: 'ctox', title: 'Crew' };
  const sourceModule = meta.module || meta.source_module || meta.sourceModule || activeModule.id || 'ctox';
  const sourceTitle = meta.source_title || meta.sourceTitle || activeModule.title || activeModule.name || sourceModule || 'Crew';
  const commandType = meta.command_type || meta.commandType || 'business_os.chat.task';
  const declaredControlCommand = meta.control_command === true || meta.controlCommand === true;
  const extraPayload = meta.payload && typeof meta.payload === 'object' ? meta.payload : {};
  const extraClientContext = meta.client_context && typeof meta.client_context === 'object' ? meta.client_context : {};
  const now = Date.now();
  const commandId = meta.command_id || meta.commandId || `cmd_${crypto.randomUUID()}`;
  // Consume a caller-supplied command id: it is valid for this one submission.
  if (chat.contextMeta && typeof chat.contextMeta === 'object') {
    delete chat.contextMeta.command_id;
    delete chat.contextMeta.commandId;
  }
  const messageId = `chatmsg_${crypto.randomUUID()}`;
  const threadKey = meta.thread_key || meta.threadKey || extraPayload.thread_key || extraPayload.threadKey || chat.contextMeta?.thread_key || `business-os/chat/${chat.id}`;
  const sourceTracking = followUpSubmission ? chatTrackingSummary(chat) : null;
  const displayTitle = String(
    meta.display_title
    || meta.displayTitle
    || extraPayload.display_title
    || extraPayload.displayTitle
    || meta.title
    || extraPayload.title
    || titleFromText(text),
  ).trim();
  const displayPrompt = String(followUpSubmission
    ? text
    : meta.display_prompt
      || meta.displayPrompt
      || extraPayload.display_prompt
      || extraPayload.displayPrompt
      || text).trim();
  chat.contextMeta = {
    ...(chat.contextMeta && typeof chat.contextMeta === 'object' ? chat.contextMeta : {}),
    module: sourceModule,
    source_module: sourceModule,
    source_title: sourceTitle,
    command_type: commandType,
    record_id: meta.record_id || meta.recordId || chat.contextMeta?.record_id || chat.id,
    thread_key: threadKey,
  };
  chat.messages.push({
    id: messageId,
    role: 'user',
    text: displayPrompt,
    createdAt: now,
    attachments: attachments.map(chatMessageAttachmentSummary),
  });
  chat.title = ['CTOX', 'Crew'].includes(chat.title) ? displayTitle : chat.title;
  const pendingMessage = {
    id: `status_${commandId}`,
    role: 'ctox',
    text: 'Aufgabe wird an die Crew gesendet.',
    commandId,
    taskId: '',
    status: 'pending_sync',
    createdAt: Date.now(),
  };
  chat.messages.push(pendingMessage);
  chat.lastTrackingId = commandId;
  touchChats(state, [chat]);
  if (typeof onPending === 'function') {
    try {
      onPending();
    } catch (error) {
      console.warn('[business-chat] pending render failed', error);
    }
  }
  let submission = null;
  try {
    const attachmentRefs = await stageChatAttachments({
      db,
      sync,
      chat,
      commandId,
      messageId,
      attachments,
    });
    const command = {
      id: commandId,
      module: sourceModule,
      type: commandType,
      record_id: meta.record_id || chat.id,
      inbound_channel: meta.inbound_channel || CHAT_CHANNEL,
      payload: {
        ...extraPayload,
        title: displayTitle,
        display_title: displayTitle,
        display_prompt: displayPrompt,
        instruction: followUpSubmission ? text : meta.instruction || extraPayload.instruction || text,
        prompt: followUpSubmission ? text : meta.prompt || extraPayload.prompt || text,
        ...(followUpSubmission ? {
          continuation: true,
          source_task_id: sourceTracking?.tracking_task_id || '',
          source_command_id: sourceTracking?.tracking_command_id || '',
        } : {}),
        chat_id: chat.id,
        message_id: messageId,
        conversation: compactConversation(chat.messages),
        attachments: attachmentRefs,
        attachment_refs: attachmentRefs,
        inbound_channel: meta.inbound_channel || CHAT_CHANNEL,
        outbound_channel: 'business_os_chat',
        response_channel: 'business_os_chat',
        reply_to: chat.id,
        thread_key: threadKey,
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
        attachment_count: attachmentRefs.length,
        attachment_storage: attachmentRefs.length ? 'desktop_files' : '',
        url: location.href,
        language: document.documentElement.lang || 'de',
        created_at: new Date(now).toISOString(),
        ...(followUpSubmission ? {
          continuation: true,
          source_task_id: sourceTracking?.tracking_task_id || '',
          source_command_id: sourceTracking?.tracking_command_id || '',
        } : {}),
      },
    };
    if (!declaredControlCommand) {
      await flushChatTrackingCollections({ sync, db });
    }
    const result = await commandBus.dispatch(command, declaredControlCommand ? { until: 'local' } : {});
    const acceptedCommandId = result.command_id || commandId;
    const controlCommand = declaredControlCommand
      || String(result.execution_mode || '').toLowerCase() === 'control';
    const terminalCommand = isTerminalTrackingStatus(result.terminal_status || result.task_status || result.status);
    const taskId = result.task_id
      || (controlCommand ? '' : await waitForSubmittedTaskId(db, acceptedCommandId));
    if (!taskId && !controlCommand && !terminalCommand) {
      throw new Error('Die Annahme der Aufgabe wurde noch nicht bestätigt.');
    }
    chat.lastTrackingId = taskId || acceptedCommandId;
    pendingMessage.text = taskId
      ? 'Deine Aufgabe steht auf der Aufgabenliste. Die Antwort erscheint hier, sobald die Crew sie bearbeitet hat.'
      : terminalCommand
        ? 'Die Crew hat den Auftrag ausgeführt.'
        : 'Die Crew führt den Auftrag aus.';
    pendingMessage.commandId = acceptedCommandId;
    pendingMessage.taskId = taskId;
    pendingMessage.status = result.task_status || result.status || 'queued';
    pendingMessage.createdAt = Date.now();
    submission = {
      status: pendingMessage.status,
      command_id: acceptedCommandId,
      task_id: taskId,
      queue_id: taskId,
    };
  } catch (error) {
    const failedCommandId = error?.command_id || error?.commandId || commandId;
    pendingMessage.text = `Die Aufgabe konnte nicht an die Crew übergeben werden: ${error?.message || String(error)}`;
    pendingMessage.commandId = failedCommandId;
    pendingMessage.taskId = '';
    pendingMessage.status = error?.status || 'failed';
    pendingMessage.trackable = false;
    pendingMessage.detail = 'nicht übergeben';
    pendingMessage.createdAt = Date.now();
    if (failedCommandId) chat.lastTrackingId = failedCommandId;
    if (isTransientCommandTrackingError(error)) {
      pendingMessage.text = 'Verbindung unterbrochen. Die Crew hat die Annahme noch nicht bestätigt.';
      pendingMessage.commandId = failedCommandId;
      pendingMessage.taskId = '';
      pendingMessage.status = 'pending_sync';
      pendingMessage.trackable = true;
      pendingMessage.detail = 'Bestätigung ausstehend';
      submission = {
        status: pendingMessage.status,
        command_id: failedCommandId,
        task_id: '',
        queue_id: '',
      };
    }
  }
  touchChats(state, [chat]);
  return submission;
}

async function waitForSubmittedTaskId(db, commandId, { timeoutMs = 12_000, intervalMs = 150 } = {}) {
  const normalizedCommandId = String(commandId || '').trim();
  if (!normalizedCommandId) return '';
  const commands = db?.raw?.business_commands;
  const queue = db?.raw?.ctox_queue_tasks;
  if (!commands && !queue) return '';
  const deadline = Date.now() + Math.max(0, Number(timeoutMs) || 0);
  do {
    const commandDocs = await findDocsByIds(commands, new Set([normalizedCommandId]));
    const commandDoc = commandDocs.get(normalizedCommandId) || null;
    const projectedTaskId = String(commandDoc?.task_id || commandDoc?.taskId || '').trim();
    if (projectedTaskId) return projectedTaskId;
    const queueDocs = await findQueueDocsByCommandIds(queue, new Set([normalizedCommandId]));
    const queueTaskId = String(queueDocs.get(normalizedCommandId)?.id || '').trim();
    if (queueTaskId) return queueTaskId;
    if (Date.now() >= deadline) break;
    await new Promise((resolve) => window.setTimeout(resolve, Math.max(25, Number(intervalMs) || 150)));
  } while (Date.now() < deadline);
  return '';
}

async function syncTrackedMessages({ state, db, sync = null }) {
  let changed = false;
  const commands = db?.raw?.business_commands;
  const queue = db?.raw?.ctox_queue_tasks;
  if (!commands && !queue) return false;

  const tracked = collectTrackedMessages(state);
  if (!tracked.length) return false;
  await flushChatTrackingCollections({ sync, db });

  const commandIds = new Set();
  const taskIds = new Set();
  for (const { message } of tracked) {
    const commandId = trackingIdFromMessage(message, 'command');
    const taskId = trackingIdFromMessage(message, 'task');
    if (commandId) commandIds.add(commandId);
    if (taskId) taskIds.add(taskId);
  }

  const commandDocs = await findDocsByIds(commands, commandIds);
  for (const commandDoc of commandDocs.values()) {
    const taskId = String(commandDoc?.task_id || commandDoc?.taskId || '').trim();
    if (taskId) taskIds.add(taskId);
  }
  const taskDocs = await findDocsByIds(queue, taskIds);
  const taskDocsByCommand = await findQueueDocsByCommandIds(queue, commandIds);
  // Members holding any tracked task, loaded once per sync.
  const memberIds = new Set();
  for (const doc of [...taskDocs.values(), ...taskDocsByCommand.values()]) {
    const memberId = String(doc?.crew_member_id || '').trim();
    if (memberId) memberIds.add(memberId);
  }
  const memberDocs = memberIds.size ? await findCrewMembersByIds(db?.raw?.ctox_crew_members, memberIds) : new Map();

  for (const chat of state.chats) {
    let chatChanged = false;
    let shouldFocusChat = false;
    for (const message of chat.messages) {
      if (!message.commandId && !message.taskId) continue;
      const commandId = trackingIdFromMessage(message, 'command');
      const taskId = trackingIdFromMessage(message, 'task');
      const commandDoc = commandId ? commandDocs.get(commandId) || null : null;
      const taskDocByCommand = commandId ? taskDocsByCommand.get(commandId) || null : null;
      const resolvedTaskId = taskId
        || String(commandDoc?.task_id || commandDoc?.taskId || '').trim()
        || String(taskDocByCommand?.id || '').trim();
      const taskDoc = (resolvedTaskId ? taskDocs.get(resolvedTaskId) || null : null) || taskDocByCommand;
      const nextTaskId = taskId || resolvedTaskId || taskDoc?.id || '';
      // The creature becomes the member the moment the harness attaches one;
      // it then stays that member for the whole conversation.
      const memberId = String(taskDoc?.crew_member_id || '').trim();
      const member = memberId ? memberDocs.get(memberId) || null : null;
      if (member && chat.crew_member_id !== memberId) {
        chat.crew_member_id = memberId;
        chat.crewIdentity = { name: String(member.name || ''), shape: String(member.shape || ''), color: String(member.color || '') };
        changed = true;
        chatChanged = true;
      }
      const takeoverKey = nextTaskId || commandId;
      if (member && takeoverKey && !hasTrackingMarker(chat, 'takeoverFor', { commandId, taskId: nextTaskId })) {
        const reasonTitle = await crewSelectionReason(db?.raw?.ctox_harness_events, String(taskDoc?.task_id || taskDoc?.id || nextTaskId).replace(/^queue-/, ''));
        chat.messages.push({
          id: `takeover_${crypto.randomUUID()}`,
          role: 'ctox',
          text: takeoverText(String(member.name || ''), reasonTitle),
          takeoverFor: takeoverKey,
          commandId: commandId || '',
          taskId: nextTaskId || '',
          crewMemberId: memberId,
          status: 'running',
          createdAt: Date.now(),
        });
        changed = true;
        chatChanged = true;
      }
      const orphanedTracking = !commandDoc && !taskDoc && isActiveTrackingStatus(message.status) && trackingMessageAgeMs(message) > 10 * 60 * 1000;
      const nextStatus = orphanedTracking ? 'failed' : preferredTrackingStatus(commandDoc, taskDoc, message.status);
      const nextProgress = taskDoc?.execution_progress
        || taskDoc?.executionProgress
        || commandDoc?.execution_progress
        || commandDoc?.executionProgress
        || null;
      const normalizedProgress = normalizeExecutionProgress(nextProgress);
      if (normalizedProgress
        && JSON.stringify(message.executionProgress || null) !== JSON.stringify(normalizedProgress)) {
        message.executionProgress = normalizedProgress;
        changed = true;
        chatChanged = true;
      }
      if (orphanedTracking && message.trackable !== false) {
        message.trackable = false;
        changed = true;
        chatChanged = true;
      }
      if (nextTaskId && nextTaskId !== message.taskId) {
        message.taskId = nextTaskId;
        chat.lastTrackingId = nextTaskId;
        changed = true;
        chatChanged = true;
      }
      if (nextStatus && nextStatus !== message.status) {
        message.status = nextStatus;
        if (orphanedTracking) {
          message.text = 'Zu dieser älteren Aufgabe ist kein gespeicherter Arbeitsstand mehr verfügbar.';
        }
        changed = true;
        chatChanged = true;
      }
      const outbound = shouldDeliverTrackedOutbound(nextStatus, commandDoc, taskDoc)
        ? (extractOutboundText(commandDoc) || extractOutboundText(taskDoc))
        : '';
      // A tracked message starts with only its command id and gains its task id
      // the moment the queue admits it. Matching the marker against the current
      // identity alone therefore missed a reply that had been filed under the
      // other one, and the same answer was appended a second time (measured on
      // welsch 09.09.2026). Both identities are the same conversation turn.
      if (outbound && !hasReplyAlready(chat, message, outbound)) {
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
        chatChanged = true;
        shouldFocusChat = true;
      }
      if (isBlockedTrackingStatus(nextStatus) && !hasTrackingMarker(chat, 'blockedFor', message)) {
        chat.messages.push({
          id: `blocked_${crypto.randomUUID()}`,
          role: 'ctox',
          text: blockedText(commandDoc, taskDoc),
          blockedFor: message.taskId || message.commandId,
          commandId: message.commandId || '',
          taskId: message.taskId || '',
          status: nextStatus,
          createdAt: Date.now(),
        });
        changed = true;
        chatChanged = true;
      }
      if (isFailureStatus(nextStatus) && !hasTrackingMarker(chat, 'failureFor', message)) {
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
        chatChanged = true;
        shouldFocusChat = true;
      }
    }
    if (chatChanged) {
      applyChatTrackingSummary(chat);
      // Eine Statusmeldung im Hintergrund darf die Ansicht nicht entfuehren.
      // Wenn der Abgleich einen Vorgang vom 26. Juli endlich als abgeschlossen
      // verbucht, wurde hier der zugehoerige Juli-Chat fokussiert und
      // focusChatForUser zog die Leiste auf dessen Tag — am 11.08.2026 riss so
      // ein WITTENSTEIN-Fenster von damals auf, waehrend der Nutzer in einer
      // heutigen Kampagne arbeitete. Alte Chats werden weiterhin aktualisiert,
      // nur nicht mehr in den Vordergrund gezogen.
      if (shouldFocusChat
        && chatAllowsAutoFocus(chat)
        && getLocalDateString(chat.createdAt) === getLocalDateString(Date.now())) {
        focusChatForUser(state, chat);
      }
    }
  }
  return changed;
}

function hasActiveTrackedMessages(state) {
  return collectTrackedMessages(state).some(({ message }) => isActiveTrackingStatus(message.status || 'queued'));
}

function hasTrackedMessagesNeedingSync(state) {
  return collectTrackedMessages(state).some(({ chat, message }) => trackedMessageNeedsSync(chat, message));
}

function trackedMessageNeedsSync(chat, message) {
  const status = message?.status || 'queued';
  if (isActiveTrackingStatus(status)) return true;
  if (!isTerminalTrackingStatus(status)) return false;
  if ((isFailureStatus(status) || isCancelledTrackingStatus(status)) && !trackingIdFromMessage(message, 'task')) {
    return true;
  }
  return !hasTerminalReplyForTracking(chat, message);
}

function hasTerminalReplyForTracking(chat, trackedMessage) {
  const refs = [
    trackingIdFromMessage(trackedMessage, 'task'),
    trackingIdFromMessage(trackedMessage, 'command'),
  ].filter(Boolean);
  if (!refs.length) return false;
  return (Array.isArray(chat?.messages) ? chat.messages : []).some((message) => {
    if (!message) return false;
    if (message === trackedMessage && !['reply', 'interim'].includes(message.kind)) return false;
    // Completion of the task and delivery of its answer are separate updates.
    // A receipt/plan promoted to completed must not stop answer hydration.
    if (canonicalTrackingStatus(trackedMessage?.status) === 'completed'
      && isChatInspectionMessage(message)) return false;
    if (String(message.role || '').toLowerCase() !== 'ctox') return false;
    if (!String(message.text || '').trim()) return false;
    if (!isTerminalTrackingStatus(message.status || 'completed')) return false;
    const messageRefs = [
      message.replyFor,
      trackingIdFromMessage(message, 'task'),
      trackingIdFromMessage(message, 'command'),
    ].map((value) => String(value || '').trim()).filter(Boolean);
    return messageRefs.some((ref) => refs.includes(ref));
  });
}

function collectTrackedMessages(state) {
  const tracked = [];
  for (const chat of Array.isArray(state?.chats) ? state.chats : []) {
    for (const message of Array.isArray(chat?.messages) ? chat.messages : []) {
      if (message?.replyFor || message?.failureFor || message?.trackable === false) continue;
      if (trackingIdFromMessage(message, 'command') || trackingIdFromMessage(message, 'task')) {
        tracked.push({ chat, message });
      }
    }
  }
  return tracked;
}

function trackingIdFromMessage(message, kind) {
  if (!message || typeof message !== 'object') return '';
  const value = kind === 'command'
    ? message.commandId || message.command_id
    : message.taskId || message.task_id;
  return String(value || '').trim();
}

async function findDocsByIds(collection, ids) {
  const uniqueIds = Array.from(ids || [])
    .map((id) => String(id || '').trim())
    .filter(Boolean);
  const unique = Array.from(new Set(uniqueIds));
  const byId = new Map();
  if (!collection || !unique.length) return byId;
  if (typeof collection.find === 'function') {
    try {
      const docs = await collection
        .find({ selector: { id: { $in: unique } }, limit: unique.length })
        .exec();
      for (const doc of Array.isArray(docs) ? docs : []) {
        const json = doc?.toJSON?.() || doc;
        const id = String(json?.id || '').trim();
        if (id) byId.set(id, json);
      }
      return byId;
    } catch {}
  }
  if (typeof collection.findOne !== 'function') return byId;
  await Promise.all(unique.map(async (id) => {
    const doc = await findDoc(collection, id);
    if (doc?.id) byId.set(String(doc.id), doc);
  }));
  return byId;
}

async function findQueueDocsByCommandIds(collection, commandIds) {
  const unique = Array.from(new Set(Array.from(commandIds || [])
    .map((id) => String(id || '').trim())
    .filter(Boolean)));
  const byCommandId = new Map();
  if (!collection || !unique.length || typeof collection.find !== 'function') return byCommandId;
  try {
    const docs = await collection
      .find({ selector: { command_id: { $in: unique } }, limit: unique.length })
      .exec();
    for (const doc of Array.isArray(docs) ? docs : []) {
      const json = doc?.toJSON?.() || doc;
      const commandId = String(json?.command_id || json?.commandId || '').trim();
      if (commandId && !byCommandId.has(commandId)) byCommandId.set(commandId, json);
    }
  } catch {}
  return byCommandId;
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

function preferredTrackingStatus(commandDoc, taskDoc, currentStatus = '') {
  const commandOutcome = firstStatusValue(commandDoc, ['terminal_status', 'task_status', 'status', 'route_status']);
  const taskOutcome = firstStatusValue(taskDoc, ['terminal_status', 'task_status', 'status', 'route_status']);
  const commandPhase = firstStatusValue(commandDoc, ['execution_phase']);
  const taskPhase = firstStatusValue(taskDoc, ['execution_phase']);
  const outcomes = [commandOutcome, taskOutcome].map(canonicalTrackingStatus).filter(Boolean);
  const terminalStatus = outcomes.find(isTerminalTrackingStatus);
  if (terminalStatus) return canonicalTrackingStatus(terminalStatus);
  const phases = [commandPhase, taskPhase, currentStatus].map(canonicalTrackingStatus);
  if (phases.some((status) => status === 'running')) return 'running';
  return canonicalTrackingStatus(commandOutcome || taskOutcome || commandPhase || taskPhase || currentStatus || '');
}

async function flushChatTrackingCollections({ sync, db } = {}) {
  if (!sync?.startCollection) return [];
  const collections = ['business_commands', 'ctox_queue_tasks']
    .filter((collection) => db?.raw?.[collection]);
  if (!collections.length) return [];
  return Promise.all(collections.map(async (collection) => {
    try {
      const bridge = await sync.startCollection(collection);
      await waitForSyncBridgeReady(bridge, 3500);
      return { collection, ok: true };
    } catch (error) {
      console.warn?.(`[business-chat] tracking sync flush failed for ${collection}`, error);
      return { collection, ok: false, error };
    }
  }));
}

function firstStatusValue(doc, fields) {
  if (!doc || typeof doc !== 'object') return '';
  for (const field of fields) {
    const value = String(doc[field] || '').trim();
    if (!value || value === 'none' || value === 'terminal') continue;
    return value;
  }
  return '';
}

function canonicalTrackingStatus(status) {
  const value = String(status || '').trim().toLowerCase();
  if (value === 'handled' || value === 'success' || value === 'done' || value === 'passed') return 'completed';
  if (value === 'leased' || value === 'processing' || value === 'executing' || value === 'active') return 'running';
  if (['waiting', 'pending_sync', 'pending', 'retry_wait', 'retry-wait', 'review_rework', 'review-rework'].includes(value)) return 'queued';
  return value;
}

// `blocked` and `stale_missing_native` are deliberately NOT terminal: a
// blocked command is waiting (approval, a missing native peer), and it
// resumes on its own once the block clears. Treating them as terminal made
// the chat stop tracking work that was still alive, so the last thing the
// user saw was a failure that never got corrected. Orphaned tracking is
// still closed out by the 10-minute rule in reconcileTrackedMessages.
function isTerminalTrackingStatus(status) {
  const value = canonicalTrackingStatus(status);
  return ['completed', 'failed', 'cancelled', 'canceled', 'error'].includes(value);
}

// The answer reaches the chat document from two writers: the native projection
// appends it without a marker, and this tracking pass appends it with
// `replyFor`. Text plus the turn's identity is what makes them the same answer;
// measured on welsch 09.09.2026, where every completed chat carried its reply
// twice.
export function hasReplyAlready(chat, message, outbound) {
  const text = String(outbound || '').trim();
  if (!text) return false;
  // Admission receipts from older clients also carry replyFor. They belong
  // to inspection and must not silence the worker's later actual answer.
  const replies = (chat?.messages || []).filter(item => item?.role === 'ctox'
    && (!isChatInspectionMessage(item) || String(item.text || '').trim() === text));
  if (hasTrackingMarker({ messages: replies }, 'replyFor', message)) return true;
  const keys = [message?.taskId, message?.commandId].map((value) => String(value || '').trim()).filter(Boolean);
  return replies.some((item) => {
    if (item?.role !== 'ctox' || String(item?.text || '').trim() !== text) return false;
    const itemKeys = [item?.taskId, item?.commandId].map((value) => String(value || '').trim()).filter(Boolean);
    return !itemKeys.length || !keys.length || itemKeys.some((key) => keys.includes(key));
  });
}

// One conversation turn is addressed by its command id before the queue admits
// it and by its task id afterwards. A marker filed under either identity means
// the chat already carries that message.
export function hasTrackingMarker(chat, field, message) {
  const keys = [message?.taskId, message?.commandId].map((value) => String(value || '').trim()).filter(Boolean);
  if (!keys.length) return false;
  return (chat?.messages || []).some((item) => {
    const marker = String(item?.[field] || '').trim();
    return marker && keys.includes(marker);
  });
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

function shouldDeliverTrackedOutbound(status, commandDoc, taskDoc) {
  const value = canonicalTrackingStatus(status);
  if (isFailureStatus(value) || isCancelledTrackingStatus(value) || value === 'terminal') return false;
  if (!isTerminalTrackingStatus(value)) {
    const rawPhase = String(commandDoc?.execution_phase || taskDoc?.execution_phase || '').trim().toLowerCase();
    if (rawPhase === 'terminal') return false;
  }
  return true;
}

function isFailureStatus(status) {
  return ['failed', 'error'].includes(String(status || '').toLowerCase());
}

function isCancelledTrackingStatus(status) {
  const value = canonicalTrackingStatus(status);
  return value === 'cancelled' || value === 'canceled';
}

// Every other Business OS surface (ctox, conversations, outbound) reports
// these as their own "blocked" state. The chat used to fold them into
// failure, which is how a waiting command came to read as a dead one.
function isBlockedTrackingStatus(status) {
  return ['blocked', 'stale_missing_native'].includes(String(status || '').toLowerCase());
}

function isActiveTrackingStatus(status) {
  const value = String(status || '').toLowerCase();
  if (isBlockedTrackingStatus(value)) return true;
  if (value === 'terminal') return true;
  return ['accepted', 'queued', 'pending', 'pending_sync', 'waiting', 'retry_wait', 'retry-wait', 'review_rework', 'review-rework', 'running', 'processing', 'executing', 'active'].includes(value);
}

function trackingMessageAgeMs(message) {
  const createdAt = Number(message?.createdAt || 0);
  return Number.isFinite(createdAt) && createdAt > 0 ? Math.max(0, Date.now() - createdAt) : 0;
}

function isTransientCommandTrackingError(error) {
  if (error?.code === 'peer_connect_timeout' || error?.code === 'peer_unstable_after_open') return true;
  const text = String(error?.message || error || '');
  return /WebRTC native peer did not open|opened and closed again within|Timed out waiting for WebRTC response|rxdb\.query\.fetch|masterWrite|masterChangesSince|IDBDatabase.*closing|database connection is closing|collection is closed|closed collection|RxDB Error-Code: COL21|wartet noch auf die Rueckmeldung|Die Rückmeldung steht noch aus/i.test(text);
}

function failureText(commandDoc, taskDoc) {
  const error = taskDoc?.status_note
    || taskDoc?.error
    || commandDoc?.error
    || commandDoc?.client_context?.dispatch_error
    || '';
  if (error) return `Die Crew konnte die Aufgabe nicht abschließen: ${error}`;
  return 'Die Crew konnte die Aufgabe nicht abschließen. Öffne die Details, um den Grund zu sehen.';
}

function blockedText(commandDoc, taskDoc) {
  const reason = taskDoc?.status_note
    || taskDoc?.blocked_reason
    || commandDoc?.blocked_reason
    || '';
  const status = String(taskDoc?.status || commandDoc?.status || '').toLowerCase();
  if (status === 'stale_missing_native') {
    return 'Die Crew ist gerade nicht erreichbar. Der gespeicherte Arbeitsstand bleibt erhalten.';
  }
  if (reason) return `Die Aufgabe wartet: ${reason}`;
  return 'Die Aufgabe wartet auf eine Freigabe oder Rückmeldung.';
}

async function openCtoxTask(taskId, commandId, taskStatus) {
  const focus = { taskId, commandId, taskStatus, sourceModule: 'business-os-chat', openDrawer: false };
  try {
    sessionStorage.setItem('ctox.businessOs.focusTask', JSON.stringify(focus));
  } catch {}
  const params = new URLSearchParams();
  if (taskId) params.set('task_id', taskId);
  if (commandId) params.set('command_id', commandId);
  if (taskStatus) params.set('task_status', taskStatus);
  params.set('source', 'business-os-chat');
  params.set('drawer', '0');
  location.hash = `#ctox?${params.toString()}`;
  const app = window.CTOX_BUSINESS_OS_APP;
  if (typeof app?.openModule === 'function' && app.activeModule?.id !== 'ctox') {
    await app.openModule('ctox');
  }
  window.dispatchEvent(new CustomEvent('ctox-business-os-focus-task', { detail: focus }));
  // The module runs in its own frame; the DOM event above never reaches it.
  for (const frame of document.querySelectorAll('iframe')) {
    try {
      if (new URL(frame.getAttribute('src') || frame.src || '', window.location.href).origin !== window.location.origin) continue;
      frame.contentWindow?.postMessage({ type: 'ctox-business-os-focus-task', focus }, window.location.origin);
    } catch {}
  }
}

// Task -> chat (slice 7): the CTOX app asks for the chat that carries a command.
async function openChatForCommand(state, db, session, detail = {}) {
  const commandId = String(detail.commandId || detail.command_id || '').trim();
  const taskId = String(detail.taskId || detail.task_id || '').trim();
  if (!commandId && !taskId) return;
  await hydrateChatsFromRxDb({ state, db, session }).catch(() => false);
  const chat = (state.chats || []).find((candidate) => Array.isArray(candidate.messages) && candidate.messages.some((message) =>
    (commandId && (message.commandId === commandId || message.command_id === commandId))
    || (taskId && (message.taskId === taskId || message.task_id === taskId))));
  if (!chat) return;
  markChatExpandedByUser(state, chat, claimChatOpenOwnership(state));
  focusChatForUser(state, chat);
  touchChats(state, [chat]);
}

// Date and Temporal Utilities for Calendar-Scoped Chats
function getLocalDateString(timestampOrDate = Date.now()) {
  const d = new Date(timestampOrDate);
  const yyyy = d.getFullYear();
  const mm = String(d.getMonth() + 1).padStart(2, '0');
  const dd = String(d.getDate()).padStart(2, '0');
  return `${yyyy}-${mm}-${dd}`;
}

function dateFromLocalDateString(dateStr) {
  const [y, m, d] = String(dateStr || getLocalDateString(Date.now())).split('-').map(Number);
  return new Date(y || new Date().getFullYear(), (m || 1) - 1, d || 1);
}

function formatGermanDateLabel(dateStr) {
  const todayStr = getLocalDateString(Date.now());
  
  const yesterday = new Date();
  yesterday.setDate(yesterday.getDate() - 1);
  const yesterdayStr = getLocalDateString(yesterday);
  
  const tomorrow = new Date();
  tomorrow.setDate(tomorrow.getDate() + 1);
  const tomorrowStr = getLocalDateString(tomorrow);
  
  if (dateStr === todayStr) return chatUiIsGerman() ? 'Heute' : 'Today';
  if (dateStr === yesterdayStr) return chatUiIsGerman() ? 'Gestern' : 'Yesterday';
  if (dateStr === tomorrowStr) return chatUiIsGerman() ? 'Morgen' : 'Tomorrow';
  
  const [y, m, d] = dateStr.split('-').map(Number);
  const shortMonths = chatUiIsGerman()
    ? ['Jan', 'Feb', 'Mär', 'Apr', 'Mai', 'Jun', 'Jul', 'Aug', 'Sep', 'Okt', 'Nov', 'Dez']
    : ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
  return `${d}. ${shortMonths[m - 1]} '${String(y).slice(-2)}`;
}


function chatUiIsGerman() {
  const lang = typeof document === 'undefined' ? 'de' : (document.documentElement?.lang || 'de');
  return String(lang).toLowerCase().startsWith('de');
}

function chatDateAriaLabel(dateStr, total = 0) {
  const label = formatGermanDateLabel(dateStr);
  const count = Number(total) || 0;
  const countLabel = count === 1 ? '1 Aufgabe' : `${formatCompactCount(count)} Aufgaben`;
  return `${chatUiIsGerman() ? 'Crew-Einsätze' : 'Crew missions'}: ${label}, ${countLabel}`;
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
  return applyChatTrackingSummary({
    id: `chat_${crypto.randomUUID()}`,
    title: 'Crew',
    open: true,
    minimized: false,
    maximized: false,
    owner_user_id: owner || '',
    messages: [],
    draft: '',
    contextMeta: {},
    createdAt: timestamp,
    updated_at_ms: timestamp,
  });
}

function chatContextMetaFromDetail(detail = {}) {
  const payload = detail.payload && typeof detail.payload === 'object' ? detail.payload : {};
  const clientContext = detail.client_context && typeof detail.client_context === 'object'
    ? detail.client_context
    : detail.clientContext && typeof detail.clientContext === 'object'
      ? detail.clientContext
    : {};
  const meta = {
    module: detail.module || detail.source_module || '',
    source_module: detail.source_module || detail.module || '',
    source_title: detail.source_title || detail.sourceTitle || '',
    record_id: detail.record_id || detail.recordId || '',
    thread_key: detail.thread_key || detail.threadKey || payload.thread_key || payload.threadKey || clientContext.thread_key || clientContext.threadKey || '',
    group_key: detail.group_key || detail.groupKey || payload.group_key || payload.groupKey || clientContext.group_key || clientContext.groupKey || '',
    title: detail.command_title || detail.commandTitle || detail.title || '',
    instruction: detail.instruction || '',
    inbound_channel: detail.inbound_channel || detail.inboundChannel || '',
    command_type: detail.command_type || detail.commandType || '',
    // A caller that already minted the command id and baked it into the prompt
    // text (Web Research does) must dispatch under exactly that id, otherwise
    // its own run record points at a command that never exists. The id is
    // consumed once - submitChat drops it again so follow-up messages in the
    // same chat mint a fresh one instead of colliding on the idempotency key.
    command_id: detail.command_id || detail.commandId || '',
    workjet_category: detail.workjet_category
      || detail.workjetCategory
      || clientContext.workjet_category
      || clientContext.workjetCategory
      || '',
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
    const state = {
      ownerUserId: owner,
      // Ein gespeichertes Datum darf beim Oeffnen nicht ueberdauern. Am 11.08.2026
      // stand die Leiste bei einem Nutzer noch auf dem 6. August und zeigte die
      // 24 Chats jenes Tages — darunter Fehlversuche aus einer Fassung, die es
      // laengst nicht mehr gibt. Wer die App aufmacht, sieht dann ein Truemmerfeld,
      // waehrend die heutigen Laeufe sauber durchlaufen. Zurueckliegende Tage bleiben
      // ueber die Datumsauswahl erreichbar; die Voreinstellung ist immer heute.
      selectedDate: getLocalDateString(Date.now()),
      activeChatId: parsed.activeChatId || '',
      dockCollapsed: 'dockCollapsed' in parsed ? Boolean(parsed.dockCollapsed) : true,
      preCollapseExpandedChatIds: Array.isArray(parsed.preCollapseExpandedChatIds)
        ? parsed.preCollapseExpandedChatIds.filter(Boolean)
        : [],
      deletedChatIds: normalizeChatDeletionMap(parsed.deletedChatIds),
      remoteHydrationComplete: chats.length > 0,
      chats: chats
        .filter((chat) => !chat.owner_user_id || chat.owner_user_id === owner)
        .map((chat) => ({
          ...chatCrewSnapshot(chat),
          id: chat.id || `chat_${crypto.randomUUID()}`,
          title: chat.title || 'Crew',
          open: chat.open !== false,
          minimized: Boolean(chat.minimized),
          userMinimized: Boolean(chat.userMinimized && chat.minimized),
          presentation_updated_at_ms: Number(chat.presentation_updated_at_ms || 0),
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
          scheduledAttachmentsByCommand: chat.scheduledAttachmentsByCommand && typeof chat.scheduledAttachmentsByCommand === 'object'
            ? chat.scheduledAttachmentsByCommand
            : {},
        })),
    };
    collapseRestoredTerminalChat(state);
    return state;
  } catch {
    return { ownerUserId: owner, selectedDate: getLocalDateString(Date.now()), dockCollapsed: true, preCollapseExpandedChatIds: [], deletedChatIds: {}, remoteHydrationComplete: false, chats: [] };
  }
}

function collapseRestoredTerminalChat(state) {
  const activeId = String(state?.activeChatId || '');
  if (!activeId || state.dockCollapsed) return state;
  const active = state.chats?.find((chat) => chat.id === activeId);
  if (!active) return state;
  if (!['success', 'failed'].includes(getTaskState(active))) return state;
  active.minimized = true;
  state.activeChatId = '';
  state.dockCollapsed = true;
  state.preCollapseExpandedChatIds = [];
  return state;
}

function writeChatState(state) {
  const deletedChatIds = pruneChatDeletionTombstones(state);
  localStorage.setItem(CHAT_STATE_KEY, JSON.stringify({
    selectedDate: state.selectedDate || getLocalDateString(Date.now()),
    activeChatId: state.activeChatId || '',
    dockCollapsed: Boolean(state.dockCollapsed),
    preCollapseExpandedChatIds: Array.isArray(state.preCollapseExpandedChatIds)
      ? state.preCollapseExpandedChatIds.filter(Boolean)
      : [],
    deletedChatIds,
    chats: state.chats.filter((chat) => isOwnedChat(chat, state.ownerUserId)).map((chat) => ({
      ...chat,
      messages: chat.messages.slice(-40),
      draft: chat.draft || '',
      contextMeta: chat.contextMeta && typeof chat.contextMeta === 'object' ? chat.contextMeta : {},
      owner_user_id: chat.owner_user_id || state.ownerUserId || '',
      updated_at_ms: chat.updated_at_ms || Date.now(),
      showFollowUp: Boolean(chat.showFollowUp),
      attachments: Array.isArray(chat.attachments) ? chat.attachments : [],
      scheduledAttachmentsByCommand: chat.scheduledAttachmentsByCommand && typeof chat.scheduledAttachmentsByCommand === 'object'
        ? chat.scheduledAttachmentsByCommand
        : {},
    })),
  }));
}

function normalizeChatDeletionMap(value) {
  const source = value && typeof value === 'object' && !Array.isArray(value) ? value : {};
  const normalized = {};
  Object.entries(source).forEach(([id, deletedAt]) => {
    const chatId = String(id || '').trim();
    const timestamp = Number(deletedAt) || 0;
    if (chatId && timestamp > 0) normalized[chatId] = timestamp;
  });
  return normalized;
}

function pruneChatDeletionTombstones(state) {
  const source = normalizeChatDeletionMap(state?.deletedChatIds);
  const cutoff = Date.now() - CHAT_DELETE_TOMBSTONE_RETENTION_MS;
  const pruned = {};
  Object.entries(source).forEach(([id, deletedAt]) => {
    if (deletedAt >= cutoff) pruned[id] = deletedAt;
  });
  if (state) state.deletedChatIds = pruned;
  return pruned;
}

function markChatDeleted(state, chat) {
  const id = String(chat?.id || '').trim();
  if (!id) return 0;
  const deletedAt = Date.now();
  state.deletedChatIds = pruneChatDeletionTombstones(state);
  state.deletedChatIds[id] = deletedAt;
  state.lastUiMutationMs = deletedAt;
  return deletedAt;
}

function isChatLocallyDeleted(state, chat) {
  const id = String(chat?.id || '').trim();
  if (!id) return false;
  const deletedAt = Number(state?.deletedChatIds?.[id] || 0);
  if (!deletedAt) return false;
  const remoteUpdatedAt = Number(chat?.updated_at_ms || chat?.updatedAt || chat?.updated_at || 0);
  return !remoteUpdatedAt || deletedAt >= remoteUpdatedAt;
}

async function persistChatState({ state, db, remote = true }) {
  const now = Date.now();
  const ownedChats = state.chats.filter((item) => isOwnedChat(item, state.ownerUserId));
  for (const chat of ownedChats) {
    chat.owner_user_id = chat.owner_user_id || state.ownerUserId || '';
    chat.updated_at_ms = now;
    applyChatTrackingSummary(chat);
  }
  writeChatState(state);
  const collection = db?.raw?.[CHAT_COLLECTION];
  if (!remote || !collection || !ownedChats.length) return;
  const docs = ownedChats.map((chat) => applyChatTrackingSummary({
    ...chat,
    messages: Array.isArray(chat.messages) ? mergeChatMessages(chat.messages, []) : [],
    draft: chat.draft || '',
    contextMeta: chat.contextMeta && typeof chat.contextMeta === 'object' ? chat.contextMeta : {},
    updated_at_ms: chat.updated_at_ms,
    showFollowUp: Boolean(chat.showFollowUp),
    attachments: Array.isArray(chat.attachments) ? chat.attachments : [],
    scheduledAttachmentsByCommand: chat.scheduledAttachmentsByCommand && typeof chat.scheduledAttachmentsByCommand === 'object'
      ? chat.scheduledAttachmentsByCommand
      : {},
  }));
  scheduleChatRemotePersistence(collection, docs);
}

function scheduleChatRemotePersistence(collection, docs) {
  const timerApi = typeof window !== 'undefined' ? window : globalThis;
  const run = () => {
    persistChatDocsRemote(collection, docs).catch((error) => {
      if (isVolatileChatPersistenceError(error)) return;
      console.warn?.('[business-chat] chat persistence failed', error);
    });
  };
  if (typeof timerApi.setTimeout === 'function') {
    timerApi.setTimeout(run, CHAT_REMOTE_PERSIST_DEFER_MS);
  } else {
    Promise.resolve().then(run);
  }
}

async function persistChatDocsRemote(collection, docs) {
  for (const doc of docs) {
    try {
      const existing = await withChatPersistenceTimeout(collection.findOne(doc.id).exec());
      if (existing) {
        const existingJson = existing.toJSON?.() || {};
        const owner = doc.owner_user_id || existingJson.owner_user_id || '';
        const merged = mergeChatPair(doc, existingJson, owner);
        await withChatPersistenceTimeout(existing.incrementalPatch(merged));
      } else {
        await withChatPersistenceTimeout(collection.insert(doc));
      }
    } catch (error) {
      if (isVolatileChatPersistenceError(error)) return;
      throw error;
    }
  }
}

function startChatLiveCollections({ sync, db, onReady } = {}) {
  if (!sync?.startCollection) return Promise.resolve([]);
  const starts = CHAT_LIVE_SYNC_COLLECTIONS
    .filter((collection) => db?.raw?.[collection])
    .map(async (collection) => {
      try {
        const bridge = await sync.startCollection(collection);
        await waitForSyncBridgeReady(bridge, 5000);
        return { collection, ok: true };
      } catch (error) {
        console.warn?.(`[business-chat] live sync start failed for ${collection}`, error);
        return { collection, ok: false, error };
      }
    });
  if (!starts.length) return Promise.resolve([]);
  return Promise.all(starts).then((results) => {
    onReady?.(results);
    return results;
  });
}

async function withChatPersistenceTimeout(operation, timeoutMs = CHAT_REMOTE_PERSIST_TIMEOUT_MS) {
  const timerApi = typeof window !== 'undefined' ? window : globalThis;
  let timer = null;
  try {
    return await Promise.race([
      Promise.resolve(operation),
      new Promise((_, reject) => {
        timer = timerApi.setTimeout?.(() => {
          const error = new Error('Business chat persistence timed out locally.');
          error.transient = true;
          reject(error);
        }, timeoutMs);
      }),
    ]);
  } finally {
    if (timer) timerApi.clearTimeout?.(timer);
  }
}

function isVolatileChatPersistenceError(error) {
  const text = String(error?.message || error || '');
  return Boolean(error?.transient)
    || /Business chat persistence timed out locally|QUERY_CANCELLED|replication-cancel|WebRTC replication cancelled|Timed out waiting for WebRTC response|rxdb\.query\.fetch|masterWrite|masterChangesSince|IDBDatabase.*closing|database connection is closing|collection is closed|closed collection|RxDB Error-Code: COL21/i.test(text);
}

async function hydrateChatsFromRxDb({ state, db, session }) {
  const collection = db?.raw?.[CHAT_COLLECTION];
  if (!collection) return false;
  const owner = ownerUserId(session) || state.ownerUserId || '';
  state.ownerUserId = owner;
  pruneChatDeletionTombstones(state);
  const docs = await collection.find().exec();
  const remoteChats = docs
    .map((doc) => doc.toJSON())
    .filter((chat) => !chat?._deleted)
    .filter((chat) => !isChatLocallyDeleted(state, chat))
    .filter((chat) => isOwnedChat(chat, owner))
    .map(normalizeChat)
    .sort((a, b) => (a.createdAt || 0) - (b.createdAt || 0));
  if (!remoteChats.length) {
    state.remoteHydrationComplete = true;
    if (state.chats.length) await persistChatState({ state, db });
    return false;
  }
  const freshRemoteBaseline = state.remoteHydrationComplete !== true;
  const localActiveChatId = String(state.activeChatId || '');
  const hasRecentUserFocus = localActiveChatId
    && Number(state.lastUiMutationMs || 0) > 0
    && Date.now() - Number(state.lastUiMutationMs) < 30_000;
  const focusChatId = freshRemoteBaseline || state.dockCollapsed || hasRecentUserFocus
    ? ''
    : remoteReplyChatToFocus(state.chats, remoteChats);
  const merged = mergeChats(state.chats, remoteChats, owner);
  const preserveActiveChat = freshRemoteBaseline
    && localActiveChatId
    && merged.some((chat) => chat.id === localActiveChatId)
    && hasRecentUserFocus;
  const changed = JSON.stringify(stripDraftsForCompare(state.chats)) !== JSON.stringify(stripDraftsForCompare(merged));
  state.chats = freshRemoteBaseline
    ? merged.map((chat) => ({
      ...chat,
      minimized: preserveActiveChat ? chat.id !== localActiveChatId : true,
      maximized: false,
    }))
    : merged;
  state.remoteHydrationComplete = true;
  if (freshRemoteBaseline) {
    const activeChat = preserveActiveChat
      ? state.chats.find((chat) => chat.id === localActiveChatId)
      : null;
    // Einen aktiven Chat wiederherstellen heisst nicht, die Leiste in seine
    // Vergangenheit zu ziehen. focusChatForUser setzt selectedDate auf das Datum
    // des Chats; stammte der gespeicherte aktive Chat aus dem Juli, sprang die
    // Leiste beim Oeffnen dorthin — am 11.08.2026 auf den 26. Juli, samt eines
    // aufgerissenen WITTENSTEIN-Fensters von damals. Wiederhergestellt wird nur,
    // was zum heutigen Tag gehoert; ein ausdruecklicher Klick des Nutzers
    // (focusChatId weiter unten) darf den Tag weiterhin wechseln.
    if (activeChat && getLocalDateString(activeChat.createdAt) === getLocalDateString(Date.now())) {
      focusChatForUser(state, activeChat);
    } else {
      state.activeChatId = '';
    }
  }
  if (focusChatId) {
    const focusChat = state.chats.find((chat) => chat.id === focusChatId);
    // Nur ein Chat von heute wird durch eine eintreffende Antwort nach vorn
    // geholt. remoteReplyChatToFocus meldet jeden Chat, der serverseitig eine
    // neue Antwort bekam — und der Abgleich schreibt laufend in ALTE Vorgaenge
    // nach. Am 11.08.2026 riss deshalb jede dieser Nachmeldungen die Leiste auf
    // den 26. Juli. Der Chip des alten Chats aktualisiert sich weiterhin; er
    // bleibt ueber die Datumsauswahl erreichbar, holt sich die Ansicht aber
    // nicht mehr selbst.
    if (focusChat
      && chatAllowsAutoFocus(focusChat)
      && getLocalDateString(focusChat.createdAt) === getLocalDateString(Date.now())) {
      focusChatForUser(state, focusChat, { allowDateChange: true });
    }
  }
  writeChatState(state);
  return changed || Boolean(focusChatId);
}

async function deleteChat({ state, chat, db }) {
  const deletedAt = markChatDeleted(state, chat);
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
      updated_at_ms: deletedAt || Date.now(),
    }).catch(() => {});
  }
}

function mergeChats(localChats, remoteChats, owner) {
  const remoteById = new Map();
  const localById = new Map();
  for (const chat of remoteChats) {
    const normalized = normalizeChat({ ...chat, owner_user_id: chat.owner_user_id || owner });
    if (isOwnedChat(normalized, owner)) remoteById.set(normalized.id, normalized);
  }
  for (const chat of localChats) {
    const normalized = normalizeChat({ ...chat, owner_user_id: chat.owner_user_id || owner });
    if (isOwnedChat(normalized, owner)) localById.set(normalized.id, normalized);
  }
  const ids = new Set([...remoteById.keys(), ...localById.keys()]);
  const merged = [];
  for (const id of ids) {
    merged.push(mergeChatPair(localById.get(id), remoteById.get(id), owner));
  }
  return merged.sort((a, b) => (a.createdAt || 0) - (b.createdAt || 0));
}

function mergeChatPair(localChat, remoteChat, owner) {
  if (!localChat) return normalizeChat({ ...remoteChat, owner_user_id: remoteChat.owner_user_id || owner });
  if (!remoteChat) return normalizeChat({ ...localChat, owner_user_id: localChat.owner_user_id || owner });
  const local = normalizeChat(localChat);
  const remote = normalizeChat(remoteChat);
  const localIsNewer = (local.updated_at_ms || 0) >= (remote.updated_at_ms || 0);
  const base = localIsNewer ? local : remote;
  const localPresentationAt = Number(local.presentation_updated_at_ms || 0);
  const remotePresentationAt = Number(remote.presentation_updated_at_ms || 0);
  const presentation = localPresentationAt >= remotePresentationAt ? local : remote;
  const messages = mergeChatMessages(local.messages, remote.messages);
  return applyChatTrackingSummary({
    ...base,
    ...chatCrewSnapshot(localIsNewer ? remote : local),
    ...chatCrewSnapshot(base),
    title: local.title || remote.title || base.title,
    open: local.open !== false || remote.open !== false,
    minimized: Boolean(presentation.minimized),
    userMinimized: Boolean(presentation.userMinimized && presentation.minimized),
    presentation_updated_at_ms: Math.max(localPresentationAt, remotePresentationAt),
    maximized: Boolean(presentation.maximized),
    owner_user_id: local.owner_user_id || remote.owner_user_id || owner,
    lastTrackingId: preferredChatTrackingId(local, remote, messages),
    messages,
    draft: local.draft || '',
    contextMeta: { ...(remote.contextMeta || {}), ...(local.contextMeta || {}) },
    attachments: Array.isArray(local.attachments) ? local.attachments : [],
    scheduledAttachmentsByCommand: {
      ...(remote.scheduledAttachmentsByCommand && typeof remote.scheduledAttachmentsByCommand === 'object' ? remote.scheduledAttachmentsByCommand : {}),
      ...(local.scheduledAttachmentsByCommand && typeof local.scheduledAttachmentsByCommand === 'object' ? local.scheduledAttachmentsByCommand : {}),
    },
    showFollowUp: Boolean(local.showFollowUp),
    updated_at_ms: Math.max(local.updated_at_ms || 0, remote.updated_at_ms || 0),
  });
}

function mergeChatMessages(localMessages = [], remoteMessages = []) {
  const byKey = new Map();
  for (const message of [...localMessages, ...remoteMessages]) {
    const key = messageIdentity(message);
    const previous = byKey.get(key);
    byKey.set(key, preferredMessage(previous, message));
  }
  return Array.from(byKey.values())
    .sort((a, b) => (Number(a.createdAt) || 0) - (Number(b.createdAt) || 0))
    .slice(-40);
}

function remoteReplyChatToFocus(localChats = [], remoteChats = []) {
  const localById = new Map((Array.isArray(localChats) ? localChats : [])
    .map((chat) => [String(chat?.id || ''), chat])
    .filter(([id]) => Boolean(id)));
  return (Array.isArray(remoteChats) ? remoteChats : [])
    .filter((chat) => hasTerminalCtoxReply(chat)
      && !chatHasSameTerminalCtoxReply(localById.get(String(chat.id || '')), chat))
    .sort((a, b) => chatActivityMs(b) - chatActivityMs(a))[0]?.id || '';
}

function hasTerminalCtoxReply(chat) {
  return (Array.isArray(chat?.messages) ? chat.messages : []).some((message) => isTerminalCtoxReply(message));
}

function chatHasSameTerminalCtoxReply(localChat, remoteChat) {
  if (!localChat || !remoteChat) return false;
  const localKeys = new Set((Array.isArray(localChat.messages) ? localChat.messages : [])
    .filter(isTerminalCtoxReply)
    .map(messageIdentity));
  return (Array.isArray(remoteChat.messages) ? remoteChat.messages : [])
    .filter(isTerminalCtoxReply)
    .some((message) => localKeys.has(messageIdentity(message)));
}

function isTerminalCtoxReply(message = {}) {
  if (String(message.role || '').toLowerCase() !== 'ctox') return false;
  if (!String(message.text || '').trim()) return false;
  const hasTrackingRef = Boolean(message.replyFor || message.commandId || message.command_id || message.taskId || message.task_id);
  return hasTrackingRef && isTerminalTrackingStatus(message.status || 'completed');
}

function messageIdentity(message = {}) {
  const role = String(message.role || 'ctox').trim().toLowerCase();
  const text = String(message.text || '').replace(/\s+/g, ' ').trim();
  const trackingRef = String(
    message.taskId
    || message.task_id
    || message.replyFor
    || message.blockedFor
    || message.failureFor
    || message.commandId
    || message.command_id
    || '',
  ).trim();
  // Local optimistic messages and their RxDB projection can carry different
  // document ids. A tracked event is the same event when task, role and copy
  // agree; its status is deliberately excluded because the projection upgrades
  // queued -> running -> completed in place.
  if (trackingRef && text) return `tracked:${role}:${trackingRef}:${text}`;
  return String(message.id || message.replyFor || `${role}:${trackingRef}:${message.createdAt || ''}`);
}

function preferredMessage(previous, next) {
  if (!previous) return next;
  const previousRank = messageStatusRank(previous);
  const nextRank = messageStatusRank(next);
  if (nextRank !== previousRank) return nextRank > previousRank ? next : previous;
  return (Number(next.createdAt) || 0) >= (Number(previous.createdAt) || 0) ? next : previous;
}

function messageStatusRank(message = {}) {
  const status = canonicalTrackingStatus(message.status);
  if (isTerminalTrackingStatus(status)) return 3;
  if (isActiveTrackingStatus(status)) return 2;
  return 1;
}

function preferredChatTrackingId(local, remote, messages) {
  const remoteTrackingId = remote.lastTrackingId || '';
  if (remoteTrackingId && messages.some((message) => (
    (message.commandId === remoteTrackingId || message.taskId === remoteTrackingId || message.replyFor === remoteTrackingId)
      && isTerminalTrackingStatus(message.status)
  ))) {
    return remoteTrackingId;
  }
  return local.lastTrackingId || remoteTrackingId || '';
}

function normalizeChat(chat) {
  return applyChatTrackingSummary({
    ...chatCrewSnapshot(chat),
    id: chat.id || `chat_${crypto.randomUUID()}`,
    title: chat.title || 'Crew',
    open: chat.open !== false,
    minimized: Boolean(chat.minimized),
    userMinimized: Boolean(chat.userMinimized && chat.minimized),
    presentation_updated_at_ms: Number(chat.presentation_updated_at_ms || 0),
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
    scheduledAttachmentsByCommand: chat.scheduledAttachmentsByCommand && typeof chat.scheduledAttachmentsByCommand === 'object'
      ? chat.scheduledAttachmentsByCommand
      : {},
  });
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
    attachments: Array.isArray(message.attachments) ? message.attachments.map(chatMessageAttachmentSummary) : [],
  }));
}

function titleFromText(text) {
  const clean = String(text || '').replace(/\s+/g, ' ').trim();
  return clean.length > 42 ? `${clean.slice(0, 39)}...` : clean || 'Crew';
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
      position: fixed;
      left: 18px;
      /* Die ausgeklappte Leiste laeuft ueber die gesamte Fensterbreite. Sie
         sparte hier 132px fuer den Kaefer-Melder aus; der waechst beim
         Ueberfahren aber auf bis zu 280px und lag ohnehin ueber der Leiste —
         die Luecke kostete nur sichtbare Laenge. Der Melder sitzt jetzt im
         rechten Ende der Leiste (Freiraum: --ctox-chat-reporter-slot). */
      right: 18px;
      bottom: 18px;
      z-index: 60;
      display: grid;
      grid-template-rows: auto auto;
      gap: var(--space-2);
      width: auto;
      max-width: calc(100dvw - 36px);
      box-sizing: border-box;
      pointer-events: none;
      min-width: 0;
    }
    .ctox-chat-root.is-scrolling .ctox-chat-window {
      transition: none !important;
    }
    .ctox-chat-root button,
    .ctox-chat-root textarea {
      font: inherit;
    }
    .ctox-chat-dock {
      --ctox-date-pill-width: 146px;
      /* Die Leiste faengt nur dort Klicks, wo sie etwas anzeigt.
         .ctox-chat-root steht bewusst auf pointer-events:none, damit die App
         darunter bedienbar bleibt — das Dock hob das fuer seine GESAMTE Flaeche
         wieder auf, einschliesslich der durchsichtigen Zwischenraeume zwischen
         Knopf, Datumspille und Streifen. Am 11.08.2026 lag die
         Empfaengerauswahl von Outbound Lead Generation genau in diesem toten Streifen:
         der Detailbereich war bis zum Anschlag gescrollt, das Haekchen sichtbar,
         und jeder Klick landete im Dock. Ohne Empfaenger keine Sellify-Uebergabe,
         kein Serienbrief, keine Serien-E-Mail — die gesamte Kette endete an
         einem unsichtbaren Rechteck.
         Die Kinder holen sich pointer-events unten einzeln zurueck. */
      pointer-events: none;
      grid-row: 2;
      display: grid;
      box-sizing: border-box;
      grid-template-columns: 88px var(--ctox-date-pill-width) 34px;
      align-items: center;
      gap: var(--space-2);
      min-width: 0;
      width: max-content;
      max-width: 100%;
      padding: 6px;
      border: 1px solid var(--line);
      border-radius: var(--control-radius);
      background: var(--surface-2);
      backdrop-filter: none;
      -webkit-backdrop-filter: none;
      box-shadow: var(--workjet-shadow-panel, var(--shadow-md));
      transition: border-color var(--motion-slow) var(--ease-spring), box-shadow var(--motion-slow) var(--ease-spring);
    }
    .ctox-chat-dock:hover {
      border-color: color-mix(in srgb, var(--line) 55%, transparent);
    }
    /* Der Kaefer-Melder steht fest unten rechts (position: fixed). Solange die
       Leiste ueber die volle Breite laeuft, endet sie unter ihm — deshalb haelt
       sie an ihrem rechten Ende genau seinen Platz frei, statt die Leiste
       vorzeitig abzuschneiden. */
    body:not([data-shell-chat-dock-side]) .ctox-chat-dock:not(.is-collapsed) {
      padding-right: var(--ctox-chat-reporter-slot, 58px);
    }
    .ctox-chat-dock.has-visible-chats {
      grid-template-columns: 88px var(--ctox-date-pill-width) minmax(136px, auto) 34px;
    }
    .ctox-chat-dock.has-nav {
      grid-template-columns: 88px var(--ctox-date-pill-width) 28px minmax(0, auto) 28px 34px;
    }
    .ctox-chat-dock.has-many-chats {
      grid-template-columns: 88px var(--ctox-date-pill-width) 28px minmax(0, min(420px, 40dvw)) 28px 34px;
      width: max-content;
      max-width: min(860px, calc(100dvw - 132px));
    }
    .ctox-chat-dock.has-one-chat .ctox-chat-strip {
      width: 148px;
    }
    .ctox-chat-dock.has-few-chats .ctox-chat-strip {
      width: auto;
      max-width: min(760px, calc(100vw - 456px));
    }
    .ctox-chat-dock.has-many-chats .ctox-chat-strip {
      width: auto;
    }
    /* Die sichtbaren Bedienelemente des Docks holen sich die Klicks zurueck,
       die der Container oben abgegeben hat. Alles, was hier NICHT steht, ist
       durchsichtiger Zwischenraum — und der gehoert der App darunter. */
    .ctox-chat-dock > *,
    .ctox-chat-fab,
    .ctox-chat-date-pill,
    .ctox-chat-nav,
    .ctox-chat-strip,
    .ctox-chat-new {
      pointer-events: auto;
    }

    .ctox-chat-date-pill {
      display: inline-flex;
      align-items: center;
      justify-content: space-between;
      height: 34px;
      width: var(--ctox-date-pill-width);
      min-width: var(--ctox-date-pill-width);
      border: 1px solid color-mix(in srgb, var(--line) 20%, transparent);
      border-radius: 10px;
      background: color-mix(in srgb, var(--surface) 15%, transparent);
      padding: 0 2px;
      box-sizing: border-box;
      gap: 2px;
      transition: border-color var(--motion-base) var(--ease-standard), background-color var(--motion-base) var(--ease-standard);
    }
    .ctox-chat-date-pill:hover {
      border-color: color-mix(in srgb, var(--line) 55%, transparent);
      background: color-mix(in srgb, var(--surface) 35%, transparent);
    }
    .ctox-date-nav-btn {
      display: flex;
      align-items: center;
      justify-content: center;
      width: 24px;
      height: 24px;
      border: none;
      border-radius: 50%;
      background: transparent;
      color: var(--muted);
      cursor: pointer;
      transition: transform var(--motion-fast) var(--ease-spring), background-color var(--motion-fast) var(--ease-standard), color var(--motion-fast) var(--ease-standard);
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
      justify-content: space-between;
      gap: 5px;
      flex: 1;
      height: 28px;
      border-radius: 8px;
      color: var(--text);
      cursor: pointer;
      min-width: 0;
      padding: 0 2px;
      transition: background-color var(--motion-fast) var(--ease-standard);
    }
    .ctox-date-picker-trigger:hover {
      background: color-mix(in srgb, var(--surface-2) 40%, transparent);
    }
    .ctox-date-copy {
      display: grid;
      gap: 1px;
      min-width: 0;
      line-height: 1;
    }
    .ctox-date-scope {
      color: var(--muted);
      font-size: var(--fs-meta);
      font-weight: 800;
      letter-spacing: 0.4px;
      line-height: 1;
      text-transform: uppercase;
    }
    .ctox-date-row {
      display: inline-flex;
      align-items: center;
      gap: var(--space-1);
      min-width: 0;
    }
    .ctox-date-label {
      font-size: var(--fs-meta);
      font-weight: 760;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
      color: var(--text);
    }
    .ctox-date-workload-badge {
      display: inline-grid;
      place-items: center;
      min-width: 16px;
      height: 16px;
      padding: 0 var(--space-1);
      border-radius: 999px;
      background: color-mix(in srgb, var(--accent) 18%, transparent);
      color: var(--accent);
      font-size: var(--fs-meta);
      font-weight: 800;
      line-height: 1;
    }
    .ctox-date-picker-trigger svg {
      flex-shrink: 0;
      color: var(--muted);
      transition: color var(--motion-fast) var(--ease-standard);
    }
    .ctox-date-picker-trigger:hover svg {
      color: var(--text);
    }
    .ctox-date-native-picker {
      position: absolute;
      bottom: 38px;
      left: 50%;
      transform: translateX(-50%);
      width: var(--ctox-date-pill-width);
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
      grid-template-columns: 88px var(--ctox-date-pill-width);
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
      gap: var(--space-2);
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
      transition: transform var(--motion-slow) var(--ease-spring), background-color var(--motion-fast) var(--ease-standard), border-color var(--motion-fast) var(--ease-standard);
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
      font-size: var(--fs-meta);
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
      transition: transform var(--motion-slow) var(--ease-spring), background-color var(--motion-base) var(--ease-standard), color var(--motion-base) var(--ease-standard), border-color var(--motion-base) var(--ease-standard);
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
      padding-bottom: 2px;
      transition: box-shadow var(--motion-fast) var(--ease-standard);
    }
    .ctox-chat-strip::-webkit-scrollbar {
      display: none;
    }
    .ctox-chat-strip.is-scrollable {
      scrollbar-width: thin;
      scrollbar-color: color-mix(in srgb, var(--accent) 45%, transparent) transparent;
    }
    .ctox-chat-strip.is-scrollable:not(.is-at-start):not(.is-at-end) {
      box-shadow:
        12px 0 14px -16px color-mix(in srgb, var(--accent) 82%, transparent) inset,
        -12px 0 14px -16px color-mix(in srgb, var(--accent) 82%, transparent) inset;
    }
    .ctox-chat-strip.is-scrollable:not(.is-at-start).is-at-end {
      box-shadow: 12px 0 14px -16px color-mix(in srgb, var(--accent) 82%, transparent) inset;
    }
    .ctox-chat-strip.is-scrollable.is-at-start:not(.is-at-end) {
      box-shadow: -12px 0 14px -16px color-mix(in srgb, var(--accent) 82%, transparent) inset;
    }
    .ctox-chat-strip.is-scrollable::-webkit-scrollbar {
      display: block;
      height: 4px;
    }
    .ctox-chat-strip.is-scrollable::-webkit-scrollbar-track {
      background: transparent;
    }
    .ctox-chat-strip.is-scrollable::-webkit-scrollbar-thumb {
      border-radius: 99px;
      background: color-mix(in srgb, var(--accent) 38%, transparent);
    }
    .ctox-chat-overflow-chip {
      flex: 0 0 78px;
      display: grid;
      grid-template-columns: 1fr;
      align-items: center;
      justify-items: center;
      gap: 0;
      height: 34px;
      border: 1px dashed color-mix(in srgb, var(--accent) 50%, transparent);
      border-radius: 10px;
      background: color-mix(in srgb, var(--accent) 10%, transparent);
      color: var(--accent);
      cursor: pointer;
      transition: transform var(--motion-base) var(--ease-spring), background-color var(--motion-fast) var(--ease-standard), border-color var(--motion-fast) var(--ease-standard);
    }
    .ctox-chat-overflow-chip span {
      font-size: var(--fs-meta);
      font-weight: 800;
      line-height: 1;
    }
    .ctox-chat-overflow-chip small {
      color: var(--muted);
      font-size: var(--fs-meta);
      font-weight: 740;
      line-height: 1;
    }
    .ctox-chat-overflow-chip:hover,
    .ctox-chat-overflow-chip.is-active {
      transform: translateY(-1px);
      background: color-mix(in srgb, var(--accent) 18%, var(--surface-2));
      border-color: var(--accent);
    }
    .ctox-chat-busy-panel {
      position: absolute;
      left: 0;
      bottom: 52px;
      width: min(520px, calc(100vw - 132px));
      max-height: min(520px, calc(100vh - 180px));
      pointer-events: auto;
      display: grid;
      grid-template-rows: auto auto auto minmax(0, 1fr);
      gap: 10px;
      padding: var(--space-3);
      border: 1px solid var(--line);
      border-radius: var(--panel-radius);
      background: var(--surface);
      backdrop-filter: none;
      -webkit-backdrop-filter: none;
      box-shadow: var(--workjet-shadow-overlay, var(--shadow-lg));
      color: var(--text);
      z-index: 70;
    }
    .ctox-date-workload-panel {
      position: absolute;
      left: 104px;
      bottom: 52px;
      width: min(360px, calc(100vw - 132px));
      pointer-events: auto;
      display: grid;
      gap: 10px;
      padding: var(--space-3);
      border: 1px solid var(--line);
      border-radius: var(--panel-radius);
      background: var(--surface);
      backdrop-filter: none;
      -webkit-backdrop-filter: none;
      box-shadow: var(--workjet-shadow-overlay, var(--shadow-lg));
      color: var(--text);
      z-index: 71;
    }
    .ctox-date-workload-panel header,
    .ctox-chat-busy-panel header {
      display: flex;
      align-items: flex-start;
      justify-content: space-between;
      gap: var(--space-3);
    }
    .ctox-date-workload-panel header div,
    .ctox-chat-busy-panel header div {
      display: grid;
      gap: 2px;
      min-width: 0;
    }
    .ctox-date-workload-panel header strong,
    .ctox-chat-busy-panel header strong {
      font-size: var(--fs-base);
      font-weight: 820;
    }
    .ctox-date-workload-panel header span,
    .ctox-chat-busy-panel header span {
      color: var(--muted);
      font-size: var(--fs-meta);
      font-weight: 680;
    }
    .ctox-date-workload-panel header button,
    .ctox-chat-busy-panel header button {
      display: grid;
      place-items: center;
      width: 28px;
      height: 28px;
      border: 1px solid color-mix(in srgb, var(--line) 36%, transparent);
      border-radius: 50%;
      background: color-mix(in srgb, var(--surface-2) 40%, transparent);
      color: var(--muted);
      cursor: pointer;
    }
    .ctox-date-workload-panel input {
      height: 32px;
      border: 1px solid color-mix(in srgb, var(--line) 34%, transparent);
      border-radius: 8px;
      background: color-mix(in srgb, var(--surface) 65%, transparent);
      color: var(--text);
      padding: 0 var(--space-2);
      font: inherit;
      font-size: var(--fs-meta);
      font-weight: 680;
      color-scheme: dark;
    }
    .ctox-date-heatmap {
      display: grid;
      grid-template-columns: repeat(7, minmax(0, 1fr));
      gap: 5px;
    }
    .ctox-date-heatmap-day {
      aspect-ratio: 1;
      min-width: 0;
      display: grid;
      place-items: center;
      gap: 1px;
      border: 1px solid color-mix(in srgb, var(--line) 26%, transparent);
      border-radius: 7px;
      background: color-mix(in srgb, var(--surface-2) 28%, transparent);
      color: var(--muted);
      cursor: pointer;
      padding: 2px;
    }
    .ctox-date-heatmap-day[data-intensity="1"] {
      background: color-mix(in srgb, var(--accent) 16%, var(--surface-2));
    }
    .ctox-date-heatmap-day[data-intensity="2"] {
      background: color-mix(in srgb, var(--accent) 26%, var(--surface-2));
    }
    .ctox-date-heatmap-day[data-intensity="3"] {
      background: color-mix(in srgb, var(--accent) 38%, var(--surface-2));
      color: var(--text);
    }
    .ctox-date-heatmap-day[data-intensity="4"] {
      background: color-mix(in srgb, var(--accent) 54%, var(--surface-2));
      color: var(--text);
      border-color: color-mix(in srgb, var(--accent) 72%, transparent);
    }
    .ctox-date-heatmap-day.is-selected {
      outline: 2px solid var(--accent);
      outline-offset: 1px;
      color: var(--text);
    }
    .ctox-date-heatmap-day span {
      font-size: var(--fs-meta);
      font-weight: 780;
      line-height: 1;
    }
    .ctox-date-heatmap-day b {
      min-height: 9px;
      color: currentColor;
      font-size: var(--fs-meta);
      font-weight: 800;
      line-height: 1;
    }
    .ctox-chat-busy-stats {
      display: flex;
      gap: 6px;
      overflow-x: auto;
      scrollbar-width: none;
    }
    .ctox-chat-busy-stats::-webkit-scrollbar {
      display: none;
    }
    .ctox-chat-busy-stats span {
      flex: 0 0 auto;
      display: grid;
      min-width: 58px;
      gap: 1px;
      padding: 6px var(--space-2);
      border: 1px solid color-mix(in srgb, var(--line) 28%, transparent);
      border-radius: 8px;
      background: color-mix(in srgb, var(--surface-2) 34%, transparent);
    }
    .ctox-chat-busy-stats b {
      font-size: var(--fs-base);
      line-height: 1;
    }
    .ctox-chat-busy-stats small {
      color: var(--muted);
      font-size: var(--fs-meta);
      font-weight: 740;
      line-height: 1;
      text-transform: uppercase;
    }
    .ctox-chat-busy-filters {
      display: grid;
      grid-template-columns: repeat(3, minmax(0, 1fr));
      gap: 6px;
    }
    .ctox-chat-busy-filters input {
      grid-column: 1 / -1;
    }
    .ctox-chat-busy-filters select,
    .ctox-chat-busy-filters input {
      min-width: 0;
      height: 30px;
      border: 1px solid color-mix(in srgb, var(--line) 34%, transparent);
      border-radius: 8px;
      background: color-mix(in srgb, var(--surface) 65%, transparent);
      color: var(--text);
      padding: 0 var(--space-2);
      font: inherit;
      font-size: var(--fs-meta);
      font-weight: 680;
      outline: none;
    }
    .ctox-chat-busy-list {
      min-height: 0;
      overflow-y: auto;
      display: grid;
      align-content: start;
      gap: var(--space-1);
      padding-right: 2px;
    }
    .ctox-chat-busy-group {
      display: grid;
      gap: 3px;
      padding: var(--space-1);
      border: 1px solid color-mix(in srgb, var(--line) 24%, transparent);
      border-radius: 10px;
      background: color-mix(in srgb, var(--surface-2) 18%, transparent);
    }
    .ctox-chat-busy-group-head {
      display: grid;
      min-height: 38px;
      border: 1px solid transparent;
      border-radius: 8px;
      background: color-mix(in srgb, var(--surface-2) 34%, transparent);
      color: var(--text);
      text-align: left;
      padding: 6px var(--space-2);
      cursor: pointer;
    }
    .ctox-chat-busy-group-head:hover,
    .ctox-chat-busy-group-head.is-active {
      border-color: color-mix(in srgb, var(--accent) 48%, transparent);
      background: color-mix(in srgb, var(--accent) 12%, transparent);
    }
    .ctox-chat-busy-group-copy {
      display: grid;
      gap: 1px;
      min-width: 0;
    }
    .ctox-chat-busy-group-copy strong,
    .ctox-chat-busy-group-copy small {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .ctox-chat-busy-group-copy strong {
      font-size: var(--fs-meta);
      font-weight: 840;
    }
    .ctox-chat-busy-group-copy small,
    .ctox-chat-busy-group-more {
      color: var(--muted);
      font-size: var(--fs-meta);
      font-weight: 700;
    }
    .ctox-chat-busy-group-rows {
      display: grid;
      gap: 2px;
    }
    .ctox-chat-busy-group-more {
      padding: var(--space-1) var(--space-2) 5px 52px;
    }
    .ctox-chat-busy-row {
      display: grid;
      grid-template-columns: 44px minmax(0, 1fr);
      align-items: center;
      gap: var(--space-2);
      min-height: 38px;
      border: 1px solid transparent;
      border-radius: 8px;
      background: transparent;
      color: var(--text);
      text-align: left;
      padding: 5px 7px;
      cursor: pointer;
    }
    .ctox-chat-busy-row:hover,
    .ctox-chat-busy-row.is-active {
      border-color: color-mix(in srgb, var(--accent) 48%, transparent);
      background: color-mix(in srgb, var(--accent) 12%, transparent);
    }
    .ctox-chat-busy-time {
      color: var(--muted);
      font-size: var(--fs-meta);
      font-weight: 760;
    }
    .ctox-chat-busy-main {
      display: grid;
      gap: 1px;
      min-width: 0;
    }
    .ctox-chat-busy-main strong,
    .ctox-chat-busy-main small {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .ctox-chat-busy-main strong {
      font-size: var(--fs-meta);
      font-weight: 800;
    }
    .ctox-chat-busy-main small,
    .ctox-chat-busy-more {
      color: var(--muted);
      font-size: var(--fs-meta);
      font-weight: 680;
    }
    .ctox-chat-busy-more {
      padding: var(--space-2);
      text-align: center;
    }
    .ctox-chat-chip {
      flex: 0 0 148px;
      display: grid;
      grid-template-columns: 32px minmax(0, 1fr);
      align-items: center;
      gap: 7px;
      height: 40px;
      min-width: 0;
      border: 1px solid transparent;
      border-radius: 10px;
      background: transparent;
      color: var(--muted);
      padding: 0 7px;
      text-align: left;
      cursor: pointer;
      transition: transform var(--motion-slow) var(--ease-spring), background-color var(--motion-fast) var(--ease-standard), border-color var(--motion-fast) var(--ease-standard), color var(--motion-fast) var(--ease-standard), box-shadow var(--motion-slow) var(--ease-spring);
      --accent: var(--shell-category-accent, var(--workjet-accent, #1b4ed8));
      --accent-soft: var(--shell-category-soft, var(--workjet-accent-soft, #e0e6f7));
    }
    .ctox-chat-chip.is-task-running {
      --status-color: var(--accent);
      --status-soft: color-mix(in srgb, var(--accent) 16%, transparent);
    }
    .ctox-chat-chip.is-task-queued {
      --status-color: #f59e0b;
      --status-soft: rgba(245, 158, 11, 0.14);
    }
    .ctox-chat-chip.is-task-success {
      --status-color: #10b981;
      --status-soft: rgba(16, 185, 129, 0.13);
    }
    .ctox-chat-chip.is-task-failed {
      --status-color: #ef4444;
      --status-soft: rgba(239, 68, 68, 0.14);
    }
    .ctox-chat-chip.is-task-scheduled {
      --status-color: #38bdf8;
      --status-soft: rgba(56, 189, 248, 0.13);
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
    .ctox-chat-chip.is-minimized:not(.is-task-idle) {
      border-color: color-mix(in srgb, var(--status-color) 46%, transparent) !important;
      background: color-mix(in srgb, var(--status-color) 12%, var(--surface)) !important;
      color: color-mix(in srgb, var(--text) 82%, var(--status-color)) !important;
      opacity: 0.94 !important;
    }
    .ctox-chat-chip.is-minimized:hover {
      border-color: color-mix(in srgb, var(--line) 45%, transparent) !important;
      background: color-mix(in srgb, var(--surface-2) 40%, transparent) !important;
      color: var(--text) !important;
      opacity: 0.98 !important;
      transform: translateY(-1px) !important;
    }
    .ctox-chat-chip.is-minimized:not(.is-task-idle):hover {
      border-color: color-mix(in srgb, var(--status-color) 64%, transparent) !important;
      background: color-mix(in srgb, var(--status-color) 18%, var(--surface-2)) !important;
    }
    .ctox-chat-chip.is-expanded:not(.is-active) {
      border-color: color-mix(in srgb, var(--line) 45%, transparent);
      background: var(--surface-2);
      color: var(--text);
      opacity: 0.96;
    }
    .ctox-chat-chip.is-active {
      border-color: var(--accent);
      background: color-mix(in srgb, var(--accent) 26%, var(--surface-2));
      color: var(--text);
      box-shadow: 0 4px 12px color-mix(in srgb, var(--accent) 30%, transparent), 0 0 0 1px var(--accent) inset;
      opacity: 1 !important;
      transform: translateY(-1px) scale(1.02);
    }
    .ctox-chat-chip-mark {
      position: relative;
      display: flex;
      align-items: center;
      justify-content: center;
      width: 32px;
      height: 32px;
      background: transparent;
      box-shadow: none;
      transition: transform var(--motion-base) var(--ease-spring);
      flex-shrink: 0;
    }
    .ctox-chat-chip.is-active .ctox-chat-chip-mark {
      transform: scale(1.06);
    }
    .ctox-chat-chip.is-minimized.is-task-idle .ctox-chat-chip-mark {
      transform: scale(0.9) !important;
      box-shadow: none !important;
      animation: none !important;
    }
    .ctox-chat-chip.is-minimized.is-task-idle .ctox-chip-spinner {
      display: none !important;
    }
    .ctox-chat-chip:not(.is-task-idle) .ctox-chat-chip-copy small {
      color: var(--status-color);
      font-weight: 820;
    }
${CREW_CREATURE_BASE_CSS}
    /* Kept for the planning indicator; creature bodies never use this loop. */
    @keyframes ctoxCrewBreathe {
      0%, 100% { transform: scale(1); }
      50% { transform: scale(1.045, 0.965) translateY(1px); }
    }
    .ctox-crew-state-dot {
      position: absolute;
      right: -1px;
      bottom: -1px;
      display: grid;
      place-items: center;
      width: 11px;
      height: 11px;
      border: 2px solid var(--surface-2);
      border-radius: 50%;
      background: var(--status-color, var(--accent));
      color: white;
      box-sizing: border-box;
      box-shadow: 0 1px 4px rgba(0, 0, 0, 0.28);
    }
    .ctox-crew-state-dot > svg {
      width: 7px;
      height: 7px;
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
    .ctox-chat-chip-mark.is-queued .ctox-crew-state-dot {
      background: #f59e0b;
      animation: ctoxPulseQueuedDot 1.5s infinite ease-in-out;
    }
    @keyframes ctoxPulseQueuedDot {
      0% { transform: scale(1); opacity: 0.7; }
      50% { transform: scale(1.25); opacity: 1; }
      100% { transform: scale(1); opacity: 0.7; }
    }
    .ctox-chat-chip-mark.is-success .ctox-crew-state-dot {
      background: #10b981 !important;
    }
    .ctox-chat-chip-mark.is-failed .ctox-crew-state-dot {
      background: #ef4444 !important;
      animation: ctoxPulseFailedDot 1.5s infinite ease-in-out;
    }
    .ctox-chat-chip-mark.is-blocked .ctox-crew-state-dot {
      background: #f59e0b !important;
    }
    .ctox-chat-chip-mark.is-scheduled .ctox-crew-state-dot {
      background: #38bdf8 !important;
      color: #061018;
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
      font-size: var(--fs-meta);
      font-weight: 760;
    }
    .ctox-chat-chip-copy small {
      color: var(--muted);
      font-size: var(--fs-meta);
      font-weight: 680;
    }
    .ctox-chat-stage {
      pointer-events: none;
      grid-row: 1;
      display: block;
      width: 100%;
      box-sizing: border-box;
      min-width: 0;
      overflow: hidden;
      padding: 0 6px;
    }
    .ctox-chat-stage-inner {
      position: relative;
      overflow: visible;
      width: 100%;
      height: min(340px, calc(100dvh - 132px));
      transition: height var(--motion-slow) var(--ease-spring);
      min-width: 0;
      pointer-events: none;
      padding: var(--space-5) 0 10px 0;
      box-sizing: border-box;
      perspective: 1200px;
      transform-style: preserve-3d;
    }
    .ctox-chat-stage-inner.is-empty {
      height: 0;
      padding-top: 0;
      padding-bottom: 0;
    }
    .ctox-chat-stage-inner.has-maximized {
      height: min(480px, calc(100dvh - 132px));
    }
    .ctox-chat-stage-inner.is-side-by-side .ctox-chat-window {
      transform: none !important;
      opacity: 1 !important;
      filter: none !important;
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
      grid-template-rows: 52px minmax(0, 1fr) auto;
      width: min(320px, calc(100dvw - 24px));
      height: min(320px, calc(100dvh - 132px));
      min-width: 0;
      min-inline-size: 0;
      overflow: hidden;
      box-sizing: border-box;
      max-width: min(440px, calc(100dvw - 24px));
      border: 1px solid var(--line);
      border-radius: var(--panel-radius);
      background: var(--surface);
      backdrop-filter: none;
      -webkit-backdrop-filter: none;
      color: var(--text);
      box-shadow: var(--workjet-shadow-overlay, var(--shadow-lg));
      font-family: var(--font-sans, var(--workjet-font-sans, -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif));
      font-size: var(--fs-sm);
      line-height: 1.4;
      flex-shrink: 0;
      transition: 
        left var(--motion-slow) var(--ease-standard),
        width var(--motion-slow) var(--ease-spring),
        height var(--motion-slow) var(--ease-spring),
        opacity var(--motion-slow) var(--ease-standard),
        transform var(--motion-slow) var(--ease-spring),
        border-color var(--motion-slow) var(--ease-standard),
        box-shadow var(--motion-slow) var(--ease-standard),
        filter var(--motion-slow) var(--ease-standard);
      --accent: var(--shell-category-accent, var(--workjet-accent, #1b4ed8));
      --accent-soft: var(--shell-category-soft, var(--workjet-accent-soft, #e0e6f7));
      transform-style: preserve-3d;
      backface-visibility: hidden;
    }
    .ctox-chat-window:not(.is-active) {
      opacity: 0.6;
      visibility: visible;
      pointer-events: auto;
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
    .ctox-chat-window:not(.is-active) .ctox-chat-header-actions,
    .ctox-chat-window:not(.is-active) .ctox-chat-form,
    .ctox-chat-window:not(.is-active) .ctox-followup-container,
    .ctox-chat-window:not(.is-active) .ctox-chat-scheduler-card,
    .ctox-chat-window:not(.is-active) .ctox-chat-delegation-card,
    .ctox-chat-window:not(.is-active) .ctox-chat-attachments-preview {
      opacity: 0;
      visibility: hidden;
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
    }
    .ctox-chat-window.is-maximized {
      width: min(440px, calc(100vw - 24px)) !important;
      height: min(460px, calc(100vh - 132px)) !important;
    }
    .ctox-chat-window.is-minimized {
      opacity: 0 !important;
      pointer-events: none !important;
      transform: translateY(30px) scale(0.9) !important;
    }
    .ctox-chat-stage-inner.is-side-by-side .ctox-chat-window.is-minimized {
      opacity: 0 !important;
      pointer-events: none !important;
    }
    .ctox-chat-window.no-left-transition {
      transition: 
        width var(--motion-slow) var(--ease-spring),
        height var(--motion-slow) var(--ease-spring),
        opacity var(--motion-slow) var(--ease-standard),
        transform var(--motion-slow) var(--ease-spring),
        border-color var(--motion-slow) var(--ease-standard),
        box-shadow var(--motion-slow) var(--ease-standard) !important;
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
      border-color: color-mix(in srgb, var(--accent) 68%, var(--line));
    }
    .ctox-chat-window.is-task-queued {
      border-color: color-mix(in srgb, #f59e0b 58%, var(--line));
    }
    .ctox-chat-window.is-task-success {
      border-color: var(--success, var(--accent)) !important;
      box-shadow: var(--workjet-shadow-overlay, var(--shadow-lg)), 0 0 20px color-mix(in srgb, var(--success, var(--accent)) 35%, transparent) !important;
    }
    .ctox-chat-window.is-task-failed {
      border-color: color-mix(in srgb, #ef4444 78%, var(--line));
      box-shadow: var(--workjet-shadow-overlay, var(--shadow-lg)), 0 0 14px rgba(239, 68, 68, 0.24);
    }

    .ctox-chat-window header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: var(--space-2);
      border-bottom: 1px solid var(--line);
      background: var(--surface-2);
      padding: 4px 6px 4px 8px;
      height: 52px;
      min-width: 0;
    }
    .ctox-chat-header-actions {
      display: flex;
      align-items: center;
      gap: var(--space-1);
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
      transition: transform var(--motion-fast) var(--ease-spring), background-color var(--motion-fast) var(--ease-standard), color var(--motion-fast) var(--ease-standard), border-color var(--motion-fast) var(--ease-standard);
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
      flex-direction: row !important;
      justify-content: center !important;
      align-items: center !important;
      gap: 7px !important;
      min-width: 0 !important;
      flex: 1 1 auto !important;
      max-width: calc(100% - 104px) !important;
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
    .ctox-chat-title-copy {
      display: grid;
      grid-template-columns: minmax(0, 1fr) auto;
      gap: 1px;
      min-width: 0;
      flex: 1 1 auto;
      width: auto !important;
    }
    .ctox-chat-title-copy strong,
    .ctox-chat-title-task {
      display: block;
      width: 100%;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      max-width: 100%;
    }
    .ctox-chat-title-copy strong {
      grid-column: 1;
      grid-row: 1;
      color: var(--text);
      font-size: var(--fs-sm);
      font-weight: 820;
      flex: 1;
      min-width: 0;
    }
    .ctox-chat-title-task {
      grid-column: 1 / -1;
      grid-row: 2;
      color: var(--muted);
      font-size: var(--fs-meta);
    }
    .ctox-chat-title-status {
      display: flex;
      align-items: center;
      gap: 5px;
      width: auto !important;
      flex: 0 0 auto;
      overflow: visible !important;
    }
    .ctox-chat-progress-head {
      grid-column: 2;
      grid-row: 1;
      color: var(--accent);
      font-size: var(--fs-meta);
      font-weight: 820;
      white-space: nowrap;
    }
    .ctox-chat-window:has(.ctox-chat-progress-head) .ctox-chat-status-badge > span:last-child {
      display: none;
    }
    .ctox-chat-window:has(.ctox-chat-progress-head) .ctox-chat-title-status {
      display: none;
    }
    .ctox-chat-messages {
      display: flex;
      flex-direction: column;
      gap: var(--space-2);
      overflow: auto;
      padding: var(--space-3);
      background: transparent;
      scrollbar-width: thin;
      min-width: 0;
      max-width: 100%;
      box-sizing: border-box;
      overflow-wrap: anywhere;
    }
    .ctox-chat-inspection { flex: 0 0 auto; border-bottom: 1px solid var(--line); }
    .ctox-chat-inspection > summary { cursor: pointer; padding: 6px 14px; color: var(--muted); font-size: var(--fs-meta); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
    .ctox-chat-inspection-body { max-height: 260px; overflow: auto; padding: 0 14px 10px; font-size: var(--fs-meta); }
    .ctox-chat-inspection-body ol { padding: 0; list-style: none; }
    .ctox-chat-inspection-body li { display: flex; gap: 10px; margin: 8px 0; }
    .ctox-chat-inspection-body time, .ctox-chat-inspection-body small { color: var(--muted); flex-shrink: 0; }
    .ctox-chat-messages .ctox-agent-scope {
      flex: 0 0 auto;
      display: grid;
      gap: 6px;
      min-width: 0;
      max-width: 100%;
      box-sizing: border-box;
      border: 1px solid color-mix(in srgb, var(--line) 52%, transparent);
      border-radius: 8px;
      background: color-mix(in srgb, var(--surface-2) 52%, transparent);
      padding: var(--space-2);
      box-shadow: 0 1px 0 rgba(255, 255, 255, 0.08) inset;
    }
    .ctox-chat-messages .ctox-agent-scope-title {
      color: var(--text);
      font-size: var(--fs-meta);
      font-weight: 760;
      line-height: 1.2;
    }
    .ctox-chat-messages .ctox-agent-scope dl {
      display: grid;
      gap: var(--space-1);
      margin: 0;
      min-width: 0;
    }
    .ctox-chat-messages .ctox-agent-scope dl > div {
      display: grid;
      grid-template-columns: minmax(74px, 0.34fr) minmax(0, 1fr);
      align-items: baseline;
      gap: var(--space-2);
      min-width: 0;
    }
    .ctox-chat-messages .ctox-agent-scope dt,
    .ctox-chat-messages .ctox-agent-scope dd {
      min-width: 0;
      margin: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .ctox-chat-messages .ctox-agent-scope dt {
      color: var(--muted);
      font-size: var(--fs-meta);
      font-weight: 700;
    }
    .ctox-chat-messages .ctox-agent-scope dd {
      color: var(--text);
      font-size: 10.5px;
      font-weight: 620;
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
      font-size: var(--fs-meta);
      letter-spacing: 0.3px;
    }
    .ctox-chat-message {
      flex: 0 0 auto;
      max-width: 88%;
      min-inline-size: 0;
      max-inline-size: 88%;
      word-break: break-word;
      overflow-wrap: anywhere;
      min-width: 0;
      display: block;
      box-sizing: border-box;
      overflow: hidden;
    }
    .ctox-chat-message.is-user {
      align-self: flex-end;
      background: color-mix(in srgb, var(--accent) 15%, var(--surface-2)) !important;
      border: none !important;
      box-shadow: 0 4px 12px rgba(0, 0, 0, 0.03) !important;
      border-radius: 14px 14px 4px 14px !important;
      padding: var(--space-2) var(--space-3) !important;
      max-width: 88%;
      min-width: 0;
    }
    .ctox-chat-message.is-ctox {
      align-self: flex-start;
      background: transparent !important;
      box-shadow: none !important;
      border: none !important;
      border-left: 2px solid var(--accent) !important;
      border-radius: 0 !important;
      padding: var(--space-1) 0 var(--space-1) var(--space-3) !important;
      margin-left: var(--space-1);
      margin-right: var(--space-3);
      max-width: 88%;
      min-width: 0;
    }
    .ctox-chat-message * {
      max-width: 100%;
      min-width: 0;
      overflow-wrap: anywhere;
      word-break: break-word;
      box-sizing: border-box;
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
      max-inline-size: 100%;
      min-width: 0;
      min-inline-size: 0;
      word-break: break-word;
      overflow-wrap: anywhere;
      white-space: normal;
    }
    .ctox-chat-body .ctox-chat-text {
      display: block;
      max-width: 100%;
      max-inline-size: 100%;
      min-width: 0;
      min-inline-size: 0;
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
      padding: var(--space-2) 10px;
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
    .ctox-chat-prompt {
      display: block;
      min-width: 0;
    }
    .ctox-chat-prompt summary {
      display: grid;
      grid-template-columns: minmax(0, 1fr) auto;
      gap: 6px;
      cursor: pointer;
      list-style: none;
      color: inherit;
    }
    .ctox-chat-prompt summary::-webkit-details-marker {
      display: none;
    }
    .ctox-chat-prompt-preview {
      display: -webkit-box;
      overflow: hidden;
      -webkit-box-orient: vertical;
      -webkit-line-clamp: 2;
      line-clamp: 2;
      line-height: 1.4;
    }
    .ctox-chat-prompt-more,
    .ctox-chat-prompt-less {
      align-self: end;
      color: var(--accent);
      font-size: var(--fs-meta);
      font-weight: 780;
      white-space: nowrap;
    }
    .ctox-chat-prompt-less,
    .ctox-chat-prompt[open] .ctox-chat-prompt-more,
    .ctox-chat-prompt[open] .ctox-chat-prompt-preview {
      display: none;
    }
    .ctox-chat-prompt[open] .ctox-chat-prompt-less {
      display: inline;
    }
    .ctox-chat-prompt-full {
      padding-top: 7px;
      border-top: 1px solid color-mix(in srgb, var(--accent) 18%, transparent);
      margin-top: 7px;
    }
    .ctox-chat-message footer {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: 6px;
      margin-top: 6px;
      color: var(--muted);
      font-size: var(--fs-meta);
      max-width: 100%;
      min-width: 0;
      overflow: hidden;
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
      font-size: var(--fs-meta);
      font-weight: 760;
      max-width: 100%;
      max-inline-size: 100%;
      min-width: 0;
      min-inline-size: 0;
      overflow-wrap: anywhere;
      word-break: break-word;
      white-space: normal;
      display: inline-flex;
      flex: 0 1 auto;
      align-items: center;
      justify-content: center;
      vertical-align: middle;
      box-sizing: border-box;
      text-align: center;
      line-height: 1.2;
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
      padding: var(--space-2) var(--space-3) !important;
      transition: background-color var(--motion-base) var(--ease-standard);
      box-sizing: border-box;
      gap: var(--space-2);
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
      padding: var(--space-1) 0;
      outline: none !important;
      box-shadow: none !important;
      font-size: var(--fs-sm);
      line-height: 1.4;
      overflow-y: auto;
      overflow-wrap: anywhere;
      word-break: break-word;
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
      transition: transform var(--motion-fast) var(--ease-spring), filter var(--motion-fast) var(--ease-standard);
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
      padding: 10px var(--space-3);
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
    .ctox-execution-progress {
      position: relative;
      z-index: 1;
      display: grid;
      gap: 8px;
      min-width: 0;
    }
    .ctox-progress-summary {
      display: flex;
      align-items: end;
      justify-content: space-between;
      gap: var(--space-2);
    }
    .ctox-progress-summary > div {
      display: flex;
      align-items: baseline;
      gap: 7px;
      min-width: 0;
    }
    .ctox-progress-summary small,
    .ctox-progress-summary span,
    .ctox-progress-current-copy small,
    .ctox-progress-current-copy span,
    .ctox-progress-next small,
    .ctox-progress-plan small {
      color: var(--muted);
      font-size: var(--fs-meta);
    }
    .ctox-progress-summary strong {
      color: var(--accent);
      font-size: var(--fs-sm);
      font-weight: 860;
    }
    .ctox-progress-summary > span {
      font-weight: 760;
      white-space: nowrap;
    }
    .ctox-progress-track {
      display: flex;
      gap: 3px;
      width: 100%;
      height: 6px;
    }
    .ctox-progress-work {
      display: flex;
      flex: 0 0 calc(90% - 2px);
      gap: 2px;
      min-width: 0;
    }
    .ctox-progress-segment,
    .ctox-progress-review {
      display: block;
      min-width: 0;
      border-radius: 999px;
      background: color-mix(in srgb, var(--line) 60%, transparent);
    }
    .ctox-progress-segment {
      flex: 1 1 0;
    }
    .ctox-progress-review {
      flex: 1 1 10%;
      border-radius: 2px 999px 999px 2px;
    }
    .ctox-progress-segment.is-completed,
    .ctox-progress-review.is-completed {
      background: var(--accent);
    }
    .ctox-progress-segment.is-in_progress,
    .ctox-progress-review:is(.is-running, .is-pending) {
      background: linear-gradient(90deg, color-mix(in srgb, var(--accent) 78%, transparent), color-mix(in srgb, var(--accent) 30%, var(--line)));
    }
    .ctox-progress-review.is-failed {
      background: #ef4444;
    }
    .ctox-progress-current {
      display: grid;
      grid-template-columns: 34px minmax(0, 1fr);
      align-items: center;
      gap: 9px;
      min-width: 0;
    }
    .ctox-turn-clock {
      position: relative;
      width: 32px;
      height: 32px;
      border: 1px solid color-mix(in srgb, var(--accent) 42%, var(--line));
      border-radius: 50%;
      background: color-mix(in srgb, var(--accent) 7%, var(--surface));
      box-shadow: 0 0 0 2px color-mix(in srgb, var(--surface) 62%, transparent) inset;
    }
    .ctox-turn-clock::before {
      content: '';
      position: absolute;
      left: 50%;
      top: 4px;
      width: 1px;
      height: 3px;
      background: color-mix(in srgb, var(--accent) 55%, var(--muted));
    }
    .ctox-turn-clock-hand {
      position: absolute;
      left: calc(50% - 1px);
      bottom: 50%;
      width: 2px;
      height: 10px;
      border-radius: 999px;
      background: var(--accent);
      transform: rotate(var(--ctox-turn-angle)) translateY(1px);
      transform-origin: 50% 100%;
      transition: transform var(--motion-slow) var(--ease-spring);
    }
    .ctox-turn-clock i {
      position: absolute;
      left: calc(50% - 2px);
      top: calc(50% - 2px);
      width: 4px;
      height: 4px;
      border-radius: 50%;
      background: var(--accent);
    }
    .ctox-progress-current-copy {
      display: grid;
      gap: 1px;
      min-width: 0;
    }
    .ctox-progress-current-copy strong,
    .ctox-progress-next span {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      color: var(--text);
      font-size: var(--fs-meta);
      font-weight: 780;
    }
    .ctox-progress-next {
      display: grid;
      grid-template-columns: auto minmax(0, 1fr);
      gap: 7px;
      padding-left: 43px;
    }
    .ctox-progress-plan {
      border-top: 1px solid color-mix(in srgb, var(--line) 38%, transparent);
      padding-top: 6px;
    }
    .ctox-progress-plan summary {
      display: flex;
      justify-content: space-between;
      gap: 8px;
      color: var(--muted);
      cursor: pointer;
      font-size: var(--fs-meta);
      font-weight: 760;
    }
    .ctox-progress-plan summary span {
      color: var(--accent);
      white-space: nowrap;
    }
    .ctox-progress-plan ol {
      display: grid;
      gap: 4px;
      margin: 7px 0 0;
      padding: 0;
      list-style: none;
    }
    .ctox-progress-plan li,
    .ctox-progress-review-row {
      display: grid;
      grid-template-columns: 8px minmax(0, 1fr) auto;
      align-items: center;
      gap: 7px;
      color: var(--muted);
      font-size: var(--fs-meta);
    }
    .ctox-progress-plan li.is-in_progress,
    .ctox-progress-plan li.is-completed {
      color: var(--text);
    }
    .ctox-plan-step-mark {
      width: 7px;
      height: 7px;
      border-radius: 50%;
      background: color-mix(in srgb, var(--line) 75%, transparent);
    }
    .ctox-progress-plan li.is-completed .ctox-plan-step-mark,
    .ctox-progress-plan li.is-in_progress .ctox-plan-step-mark {
      background: var(--accent);
    }
    .ctox-progress-review-row {
      grid-template-columns: minmax(0, 1fr) auto;
      padding: 6px 0 0 15px;
    }
    .ctox-progress-review-row.is-completed {
      color: var(--text);
    }
    @keyframes ctoxCrewThinkingTick {
      0% { transform: rotate(0deg) translateY(0); }
      45% { transform: rotate(-7deg) translateY(-1px); }
      100% { transform: rotate(0deg) translateY(0); }
    }
    @keyframes ctoxCrewToolTick {
      0% { transform: scale(1, 1); }
      42% { transform: scale(1.12, 0.88) translateY(2px); }
      72% { transform: scale(0.96, 1.06) translateY(-1px); }
      100% { transform: scale(1, 1); }
    }
    .ctox-chat-window.is-task-running:not(.is-task-review).has-activity-thinking .ctox-chat-title .ctox-crew-creature {
      animation: none;
    }
    .ctox-chat-window.is-task-running:not(.is-task-review).has-activity-thinking .ctox-chat-title .ctox-crew-eyes {
      transform: translateX(2px) rotate(-4deg);
    }
    .ctox-chat-window.is-task-running:not(.is-task-review).has-activity-tool .ctox-chat-title .ctox-crew-creature {
      animation: none;
    }
    @media (prefers-reduced-motion: reduce) {
      .ctox-crew-creature,
      .ctox-crew-creature *,
      .ctox-chat-window.is-task-running:not(.is-task-review).has-activity-thinking .ctox-chat-title .ctox-crew-creature,
      .ctox-chat-window.is-task-running:not(.is-task-review).has-activity-tool .ctox-chat-title .ctox-crew-creature,
      .ctox-delegation-spinner,
      .ctox-turn-clock-hand {
        animation: none !important;
        transition: none !important;
      }
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
      font-size: var(--fs-meta);
      font-weight: 760;
      color: var(--text);
      overflow-wrap: anywhere;
      word-break: break-word;
    }
    .ctox-delegation-info span {
      font-size: var(--fs-meta);
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
      min-height: 28px;
      height: auto;
      padding: 6px var(--space-2);
      border: 1px solid color-mix(in srgb, var(--accent) 35%, var(--line));
      border-radius: 8px;
      background: color-mix(in srgb, var(--accent) 12%, var(--surface));
      color: var(--accent);
      font-size: var(--fs-meta);
      font-weight: 760;
      cursor: pointer;
      z-index: 1;
      transition: transform var(--motion-fast) var(--ease-spring), background-color var(--motion-fast) var(--ease-standard), border-color var(--motion-fast) var(--ease-standard);
      line-height: 1.2;
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
      padding: var(--space-2) var(--space-3) !important;
      border-top: 1px solid color-mix(in srgb, var(--accent) 20%, transparent) !important;
      background: color-mix(in srgb, var(--accent) 3%, transparent) !important;
    }
    .ctox-followup-btn {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: var(--space-2);
      width: 100%;
      height: 32px;
      border: none !important;
      border-radius: 8px !important;
      background: color-mix(in srgb, var(--accent) 12%, var(--surface-2)) !important;
      color: var(--accent) !important;
      font-size: var(--fs-meta) !important;
      font-weight: 700 !important;
      cursor: pointer;
      transition: transform var(--motion-slow) var(--ease-spring), background-color var(--motion-fast) var(--ease-standard), box-shadow var(--motion-fast) var(--ease-standard);
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
      gap: var(--space-1);
      padding: 2px 6px;
      border-radius: 6px;
      font-size: var(--fs-meta);
      font-weight: 760;
      text-transform: uppercase;
      letter-spacing: 0.3px;
      backdrop-filter: blur(6px);
      -webkit-backdrop-filter: blur(6px);
      max-width: 100%;
      min-width: 0;
    }
    .ctox-chat-status-badge span {
      min-width: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
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
    /* Amber, not red: blocked work is waiting, not dead. */
    .ctox-chat-status-badge.is-blocked {
      border: 1px solid rgba(245, 158, 11, 0.3);
      background: rgba(245, 158, 11, 0.1);
      color: #f59e0b;
    }

    @media (max-height: 680px) {
      .ctox-chat-stage-inner:not(.is-empty) {
        height: min(240px, calc(100vh - 132px));
      }
      .ctox-chat-stage-inner.has-maximized {
        height: min(280px, calc(100vh - 132px));
      }
      .ctox-chat-window {
        height: min(220px, calc(100vh - 132px));
      }
      .ctox-chat-window.is-maximized {
        height: min(260px, calc(100vh - 132px)) !important;
      }
    }
    @media (max-height: 520px) {
      .ctox-chat-stage-inner,
      .ctox-chat-stage-inner.has-maximized {
        height: min(160px, calc(100vh - 112px));
      }
      .ctox-chat-window,
      .ctox-chat-window.is-maximized {
        height: min(150px, calc(100vh - 112px)) !important;
      }
    }
    @media (max-height: 479px) {
      .ctox-chat-stage {
        display: none !important;
      }
    }
    @media (max-width: 780px) {
      .ctox-chat-root {
        right: 18px;
        width: auto;
        max-width: calc(100vw - 36px);
        grid-template-columns: minmax(0, 1fr);
      }
      .ctox-chat-dock {
        display: flex !important;
        align-items: center !important;
        justify-content: flex-start !important;
        gap: 6px !important;
        overflow-x: auto !important;
        width: calc(100vw - 36px) !important;
        max-width: 100% !important;
        min-width: 0 !important;
        box-sizing: border-box !important;
        scrollbar-width: none !important;
      }
      .ctox-chat-dock.has-many-chats {
        width: calc(100vw - 36px) !important;
      }
      .ctox-chat-dock::-webkit-scrollbar {
        display: none !important;
      }
      .ctox-chat-strip {
        flex: 0 1 auto !important;
        min-width: 0 !important;
      }
      .ctox-chat-dock.has-many-chats .ctox-chat-strip {
        flex: 1 1 auto !important;
      }
      .ctox-chat-busy-panel {
        width: calc(100vw - 36px) !important;
      }
      .ctox-chat-busy-filters {
        grid-template-columns: repeat(2, minmax(0, 1fr)) !important;
      }
      .ctox-date-workload-panel {
        left: 0 !important;
        width: calc(100vw - 36px) !important;
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
        overflow: hidden !important;
        scroll-snap-type: none !important;
        gap: 0 !important;
        width: 100% !important;
        padding: var(--space-2) 0 !important;
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
      .ctox-chat-window:not(.is-active) {
        display: none !important;
        pointer-events: none !important;
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
      padding: 6px var(--space-3);
      background: color-mix(in srgb, var(--surface) 25%, transparent);
      border-bottom: 1px solid color-mix(in srgb, var(--line) 20%, transparent);
      font-size: 10.5px;
      color: var(--muted);
      gap: var(--space-1);
    }
    
    .ctox-chat-time-input {
      border: 1px solid color-mix(in srgb, var(--line) 40%, transparent);
      border-radius: 4px;
      background: var(--surface-2);
      color: var(--text);
      font-size: var(--fs-meta);
      padding: 1px var(--space-1);
      outline: none;
      width: 54px;
      transition: border-color var(--motion-fast) var(--ease-standard);
    }
    .ctox-chat-time-input:focus {
      border-color: var(--accent);
    }
    
    .ctox-chat-scheduler-card {
      position: relative;
      overflow: hidden;
      display: flex;
      flex-direction: column;
      gap: var(--space-2);
      margin: var(--space-2) var(--space-3);
      padding: 10px var(--space-3);
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
      gap: var(--space-2);
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
      font-size: var(--fs-meta);
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
      padding: 5px var(--space-2);
      border-radius: 6px;
      width: fit-content;
    }
    
    .ctox-scheduler-timer-badge {
      font-size: var(--fs-meta);
      text-transform: uppercase;
      font-weight: 600;
      color: var(--muted);
    }
    
    .ctox-scheduler-timer {
      font-size: var(--fs-base);
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
      padding: 0 var(--space-2);
      transition: all var(--motion-fast) var(--ease-standard);
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
      padding: var(--space-2) 10px;
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
      padding: var(--space-1) 6px;
      font-size: var(--fs-meta);
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
      font-size: var(--fs-sm);
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
      transition: color var(--motion-fast) var(--ease-standard);
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
      transition: all var(--motion-fast) var(--ease-standard);
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
      gap: var(--space-3);
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

    /* Quiet crew surface: chat copy is the only persistent copy. */
    .ctox-chat-window,
    .ctox-chat-window.is-active,
    .ctox-chat-window[class*="is-task-"] {
      grid-template-rows: 64px minmax(0, 1fr) 56px;
      width: min(460px, calc(100dvw - 24px));
      max-width: min(460px, calc(100dvw - 24px));
      height: min(580px, calc(100dvh - 132px)) !important;
      min-height: min(420px, calc(100dvh - 132px));
      max-height: min(580px, calc(100dvh - 132px));
      border-color: var(--line) !important;
      border-radius: 14px !important;
      background: var(--surface) !important;
      box-shadow: var(--shadow-lg) !important;
      animation: none !important;
    }
    .ctox-chat-window.is-active {
      border-color: color-mix(in srgb, var(--accent) 28%, var(--line)) !important;
      box-shadow: var(--shadow-lg) !important;
    }
    .ctox-chat-stage-inner,
    .ctox-chat-stage-inner.has-maximized {
      height: min(600px, calc(100dvh - 112px));
    }
    .ctox-chat-stage-inner.is-empty { height: 0; padding: 0; }
    .ctox-chat-stage-inner.has-maximized { height: calc(100dvh - 112px); }
    .ctox-chat-window.is-maximized {
      width: min(960px, calc(100dvw - 24px)) !important;
      max-width: min(960px, calc(100dvw - 24px));
      height: calc(100dvh - 132px) !important;
      min-height: 0;
      max-height: calc(100dvh - 132px);
    }
    .ctox-chat-window:not(:has(.ctox-chat-form)):not(:has(.ctox-followup-container)):not(:has(.ctox-chat-scheduler-card)) {
      grid-template-rows: 64px minmax(0, 1fr);
    }
    .ctox-chat-window header {
      position: relative;
      box-sizing: border-box;
      height: 64px;
      padding: 10px;
      border-bottom-color: var(--line);
      background: var(--surface);
    }
    .ctox-chat-title {
      flex: 0 0 40px !important;
      width: 40px !important;
      max-width: 40px !important;
      justify-content: flex-start !important;
      overflow: visible !important;
    }
    .ctox-chat-title-copy,
    .ctox-chat-title-status,
    .ctox-chat-progress-head,
    .ctox-progress-summary,
    .ctox-progress-current,
    .ctox-progress-next,
    .ctox-progress-plan,
    .ctox-delegation-watch-btn,
    .ctox-delegation-info {
      display: none !important;
    }
    .ctox-crew-creature.is-window {
      position: relative;
      width: 40px;
      height: 40px;
      flex-basis: 40px;
      isolation: isolate;
    }
    .ctox-crew-creature.is-window::before {
      content: '';
      position: absolute;
      inset: -2px;
      z-index: -1;
      border-radius: 50%;
      background: conic-gradient(from -90deg,
        color-mix(in srgb, var(--crew-color) 72%, transparent) 0 var(--ctox-progress-angle),
        color-mix(in srgb, var(--line) 34%, transparent) var(--ctox-progress-angle) 360deg);
      -webkit-mask: radial-gradient(circle, transparent 66%, #000 68%);
      mask: radial-gradient(circle, transparent 66%, #000 68%);
      opacity: 0.75;
    }
    .ctox-chat-header-actions {
      gap: 1px;
      opacity: 0.38;
      transition: opacity var(--motion-fast) var(--ease-standard);
    }
    .ctox-chat-window header:hover .ctox-chat-header-actions,
    .ctox-chat-header-actions:focus-within { opacity: 0.9; }
    .ctox-chat-window header button:not(.ctox-chat-title) {
      width: 24px;
      min-width: 24px;
      height: 24px;
      min-height: 24px;
    }
    .ctox-chat-window:not(.is-maximized) .ctox-chat-messages {
      max-height: none;
    }
    .ctox-chat-messages {
      gap: 10px;
      padding: 12px;
      background: var(--surface);
    }
    .ctox-chat-message.is-ctox {
      margin: 0 !important;
      padding: 2px 0 !important;
      border-left: 0 !important;
    }
    .ctox-chat-message.is-user {
      padding: 7px 9px !important;
      border-radius: 10px !important;
      background: color-mix(in srgb, var(--accent) 7%, var(--surface-2)) !important;
      box-shadow: none !important;
    }
    .ctox-chat-message footer { min-height: 0; margin-top: 3px; }
    .ctox-chat-track {
      display: grid !important;
      place-items: center;
      width: 24px;
      height: 20px;
      min-width: 24px;
      padding: 0 !important;
      border: 0 !important;
      background: transparent !important;
      color: color-mix(in srgb, var(--muted) 68%, transparent) !important;
    }
    .ctox-chat-track:hover,
    .ctox-chat-track:focus-visible { color: var(--accent) !important; }
    .ctox-chat-delegation-card {
      position: absolute;
      left: 60px;
      right: 90px;
      top: 50%;
      bottom: auto;
      transform: translateY(-50%);
      z-index: 2;
      min-height: 0;
      height: 28px;
      padding: 0 !important;
      gap: 0;
      border: 0 !important;
      background: transparent !important;
      overflow: visible;
    }
    .ctox-progress-visual {
      display: flex;
      align-items: center;
      width: 100%;
      min-width: 0;
      height: 28px;
      padding: 0;
      border: 0;
      outline: 0;
      background: transparent;
      cursor: help;
      gap: 9px;
    }
    .ctox-chat-window header button.ctox-progress-visual {
      width: 100%;
      min-width: 0;
      height: 28px;
      min-height: 28px;
    }
    .ctox-progress-activity {
      position: relative;
      display: block;
      flex: 0 0 28px;
      width: 28px;
      height: 28px;
      border: 1px solid color-mix(in srgb, var(--accent) 38%, var(--line));
      border-radius: 50%;
      opacity: 0.68;
      transition: opacity var(--motion-fast) var(--ease-standard), transform var(--motion-fast) var(--ease-spring);
    }
    .ctox-progress-activity::after {
      content: '';
      position: absolute;
      left: calc(50% - 0.5px);
      bottom: 50%;
      width: 1px;
      height: 7px;
      border-radius: 999px;
      background: var(--crew-color, var(--accent));
      transform: rotate(var(--ctox-turn-angle, 0deg));
      transform-origin: 50% 100%;
      transition: transform var(--motion-slow) var(--ease-spring);
    }
    .ctox-progress-activity i {
      position: absolute;
      left: calc(50% - 2px);
      top: calc(50% - 2px);
      width: 4px;
      height: 4px;
      border-radius: 50%;
      background: var(--crew-color, var(--accent));
    }
    .ctox-progress-visual:hover .ctox-progress-activity,
    .ctox-progress-visual:focus-visible .ctox-progress-activity {
      opacity: 1;
      transform: scale(1.08);
    }
    .ctox-progress-track {
      display: flex;
      gap: 3px;
      width: 100%;
      height: 6px;
      overflow: visible;
    }
    .ctox-progress-segment,
    .ctox-progress-review {
      background: color-mix(in srgb, var(--line) 38%, transparent);
      opacity: 0.72;
    }
    .ctox-progress-segment.is-completed,
    .ctox-progress-review.is-completed {
      background: color-mix(in srgb, var(--accent) 74%, var(--crew-color));
      opacity: 0.9;
    }
    .ctox-progress-segment.is-in_progress,
    .ctox-progress-review:is(.is-running, .is-pending) {
      background: color-mix(in srgb, var(--accent) 56%, var(--line));
      opacity: 1;
      animation: ctoxQuietProgress 1.8s ease-in-out infinite;
    }
    .ctox-progress-planning-line {
      display: block;
      width: 38%;
      height: 100%;
      border-radius: 999px;
      background: color-mix(in srgb, var(--accent) 60%, var(--line));
      animation: ctoxQuietPlanning 1.8s ease-in-out infinite alternate;
    }
    .ctox-progress-visual.is-planning .ctox-progress-activity {
      animation: ctoxCrewBreathe 1.8s ease-in-out infinite;
    }
    .ctox-progress-visual.is-dormant .ctox-progress-track,
    .ctox-progress-visual.is-dormant .ctox-progress-activity {
      opacity: 0.68;
    }
    .ctox-chat-form {
      display: grid !important;
      grid-template-columns: 36px minmax(0, 1fr) 36px;
      align-items: center !important;
      gap: 8px !important;
      height: 56px;
      padding: 7px 10px !important;
      border-top-color: var(--line) !important;
      background: var(--surface) !important;
    }
    .ctox-chat-form textarea {
      box-sizing: border-box;
      width: 100%;
      height: 40px;
      min-height: 40px;
      max-height: 40px;
      padding: 9px 11px !important;
      border: 1px solid var(--line) !important;
      border-radius: 10px !important;
      background: var(--surface-2) !important;
      line-height: 20px;
    }
    .ctox-chat-form button,
    .ctox-chat-clip-btn {
      align-self: center !important;
      width: 36px !important;
      min-width: 36px !important;
      height: 36px !important;
      min-height: 36px !important;
    }
    @keyframes ctoxQuietProgress {
      0%, 100% { opacity: 0.52; }
      50% { opacity: 1; }
    }
    @keyframes ctoxQuietPlanning {
      0% { width: 18%; opacity: 0.45; }
      100% { width: 58%; opacity: 0.9; }
    }
    .ctox-chat-dock {
      --ctox-date-pill-width: 42px;
      grid-template-columns: 108px var(--ctox-date-pill-width) 36px;
      gap: 6px;
      padding: 5px;
      border-color: color-mix(in srgb, var(--line) 48%, transparent);
      border-radius: 16px;
      background: var(--surface);
    }
    .ctox-chat-dock.has-visible-chats {
      grid-template-columns: 108px var(--ctox-date-pill-width) minmax(48px, auto) 36px;
    }
    .ctox-chat-dock.has-nav {
      grid-template-columns: 108px var(--ctox-date-pill-width) 26px minmax(0, auto) 26px 36px;
    }
    .ctox-chat-dock.has-many-chats {
      grid-template-columns: 108px var(--ctox-date-pill-width) 26px minmax(0, min(350px, 36dvw)) 26px 36px;
    }
    .ctox-chat-fab {
      display: grid;
      grid-template-columns: auto 1fr;
      align-items: center;
      gap: 6px;
      width: 108px;
      min-width: 108px;
      height: 42px;
      padding: 0 8px;
      border-color: transparent;
      background: transparent;
    }
    .ctox-chat-fab-creatures {
      display: flex;
      align-items: center;
      justify-content: flex-end;
      width: 100%;
      height: 34px;
      padding-left: 8px;
    }
    .ctox-chat-fab-label {
      color: var(--text-strong);
      font-size: var(--fs-sm);
      font-weight: 760;
    }
    .ctox-chat-fab-creatures .ctox-crew-creature {
      width: 28px;
      height: 28px;
      flex: 0 0 28px;
      margin-left: -8px;
      filter: saturate(0.9);
    }
    .ctox-chat-fab-creatures.is-members .ctox-chat-crew-slot {
      display: inline-grid;
      width: 28px;
      height: 28px;
      flex: 0 0 28px;
      margin-left: -8px;
    }
    .ctox-chat-fab-creatures.is-members .ctox-crew-creature {
      margin-left: 0;
    }
    /* A member at work on an app: small portrait in the corner of the window
       icon and on the desktop icon. Pure presence, no text. */
    .ctox-crew-app-presence {
      position: absolute;
      right: 1px;
      bottom: 1px;
      z-index: 3;
      display: inline-flex;
      align-items: center;
      pointer-events: none;
    }
    .ctox-crew-app-presence .ctox-crew-creature.is-badge {
      width: 18px;
      height: 18px;
      flex: 0 0 18px;
      margin-left: -7px;
      filter: drop-shadow(0 1px 2px rgba(0, 0, 0, 0.45));
    }
    .ctox-crew-app-presence .ctox-crew-creature.is-badge:first-child {
      margin-left: 0;
    }
    .ctox-crew-app-presence b {
      margin-left: 1px;
      font: 700 9px/1 var(--font-family, system-ui, sans-serif);
      color: var(--text);
      text-shadow: 0 1px 2px rgba(0, 0, 0, 0.6);
    }
    .desktop-icon-glyph.has-crew-presence {
      position: relative;
    }
    /* Crew pool: the members, ready to be picked up and dropped on an app. */
    .ctox-chat-crew-pool {
      display: flex;
      align-items: center;
      gap: 2px;
      height: 42px;
      padding: 0 6px;
      border-left: 1px solid var(--line);
    }
    .ctox-chat-crew-slot {
      display: inline-grid;
      place-items: center;
      width: 30px;
      height: 30px;
      border-radius: 50%;
      cursor: grab;
      touch-action: none;
      transition: transform 140ms ease;
    }
    .ctox-chat-crew-slot:hover {
      transform: translateY(-2px) scale(1.06);
    }
    .ctox-chat-crew-slot:active {
      cursor: grabbing;
    }
    .ctox-chat-crew-slot .ctox-crew-creature {
      width: 26px;
      height: 26px;
    }
    /* The carried member: hanging from a hook, wriggling until it is dropped. */
    .ctox-crew-drag-ghost {
      position: fixed;
      left: 0;
      top: 0;
      z-index: 4000;
      display: grid;
      justify-items: center;
      gap: 2px;
      width: 48px;
      pointer-events: none;
      transform-origin: 24px 0;
      animation: ctoxCrewHookSwing 1.1s ease-in-out infinite alternate;
      filter: drop-shadow(0 6px 10px rgba(0, 0, 0, 0.35));
    }
    .ctox-crew-drag-hook {
      display: block;
      width: 2px;
      height: 14px;
      border-radius: 1px;
      background: var(--text-strong, currentColor);
      opacity: 0.7;
    }
    .ctox-crew-drag-body {
      display: block;
      width: 44px;
      height: 44px;
      transform-origin: 50% 0;
      animation: ctoxCrewHookWriggle 0.42s ease-in-out infinite alternate;
    }
    .ctox-crew-drag-body .ctox-crew-creature {
      width: 100%;
      height: 100%;
      transform: none !important;
    }
    .ctox-crew-drag-name {
      color: var(--text-strong);
      font-size: 11px;
      font-weight: 700;
      text-shadow: 0 1px 2px rgba(0, 0, 0, 0.5);
      white-space: nowrap;
    }
    @keyframes ctoxCrewHookSwing {
      from { rotate: -9deg; }
      to { rotate: 9deg; }
    }
    @keyframes ctoxCrewHookWriggle {
      0% { transform: rotate(-7deg) scale(1, 0.96); }
      50% { transform: rotate(4deg) scale(0.97, 1.04); }
      100% { transform: rotate(8deg) scale(1.02, 0.95); }
    }
    @media (prefers-reduced-motion: reduce) {
      .ctox-crew-drag-ghost,
      .ctox-crew-drag-body {
        animation: none;
      }
    }
    .ctox-chat-date-pill {
      height: 42px;
      border-color: transparent;
      background: transparent;
    }
    .ctox-date-picker-trigger {
      justify-content: center;
      flex: 0 0 30px;
      padding: 0;
    }
    .ctox-date-copy,
    .ctox-date-workload-badge { display: none !important; }
    .ctox-chat-strip { gap: 5px; }
    .ctox-chat-dock.has-one-chat .ctox-chat-strip { width: 48px; }
    .ctox-chat-chip {
      flex: 0 0 46px;
      display: grid;
      grid-template-columns: 1fr;
      place-items: center;
      width: 46px;
      height: 42px;
      padding: 4px;
      border-color: transparent;
      border-radius: 13px;
    }
    .ctox-chat-chip-copy { display: none !important; }
    .ctox-chat-chip.is-expanded:not(.is-active) {
      border-color: transparent;
      background: transparent;
    }
    .ctox-chat-chip.is-active {
      border-color: color-mix(in srgb, var(--accent) 32%, var(--line));
      background: color-mix(in srgb, var(--accent) 10%, transparent);
      box-shadow: none;
      transform: translateY(-1px);
    }
    .ctox-chat-overflow-chip {
      width: 40px;
      min-width: 40px;
      padding: 0;
    }
    .ctox-chat-overflow-chip span,
    .ctox-chat-overflow-chip small { display: none !important; }
    /* The dock has two deliberately opposite geometries. Keep these final
       state rules after all compact/theme overrides so a later generic dock
       rule cannot stretch the collapsed controls or cap the expanded strip. */
    .ctox-chat-root.is-collapsed {
      right: auto;
      width: max-content;
      max-width: max-content;
    }
    .ctox-chat-dock.is-collapsed {
      justify-self: start;
      width: max-content !important;
      max-width: max-content !important;
    }
    .ctox-chat-dock:not(.is-collapsed) {
      justify-self: start;
      width: max-content;
      max-width: 100%;
    }
    .ctox-chat-dock.has-visible-chats:not(.is-collapsed) {
      grid-template-columns: 108px var(--ctox-date-pill-width) minmax(48px, auto) 36px;
    }
    .ctox-chat-dock.has-few-chats:not(.is-collapsed),
    .ctox-chat-dock.has-many-chats:not(.is-collapsed) {
      grid-template-columns: 108px var(--ctox-date-pill-width) 26px minmax(0, 1fr) 26px 36px;
      justify-self: stretch;
      width: 100%;
      max-width: 100%;
    }
    .ctox-chat-dock:is(.has-few-chats, .has-many-chats):not(.is-collapsed) .ctox-chat-strip {
      width: 100%;
      max-width: none;
    }
    /* Every visible crew window owns its controls. Focusing the window is not
       a prerequisite for close/minimize/maximize. */
    .ctox-chat-window:not(.is-active) .ctox-chat-header-actions {
      opacity: 0.38;
      visibility: visible;
    }
    .ctox-chat-window:not(.is-active) .ctox-chat-delegation-card {
      opacity: 1;
      visibility: visible;
    }
    .ctox-chat-window:not(.is-active) .ctox-chat-header-actions,
    .ctox-chat-window:not(.is-active) .ctox-chat-header-actions *,
    .ctox-chat-window:not(.is-active) .ctox-chat-delegation-card .ctox-progress-activity,
    .ctox-chat-window:not(.is-active) .ctox-chat-delegation-card .ctox-progress-track {
      pointer-events: auto !important;
    }
    .ctox-chat-window:not(.is-active) header:hover .ctox-chat-header-actions,
    .ctox-chat-window:not(.is-active) .ctox-chat-header-actions:focus-within {
      opacity: 0.9;
    }
    /* Progress is the lower header frame, not a detached bar. The clock is the
       compact task link; all explanatory copy stays in its hover/focus title. */
    .ctox-chat-delegation-card {
      inset: 0;
      width: auto;
      height: auto;
      min-height: 0;
      transform: none;
      pointer-events: none;
    }
    .ctox-progress-visual,
    .ctox-chat-window header button.ctox-progress-visual {
      position: absolute;
      inset: 0;
      width: 100%;
      height: 100%;
      min-height: 0;
      pointer-events: none;
    }
    .ctox-progress-activity {
      position: absolute;
      left: 62px;
      top: 50%;
      width: 32px;
      height: 32px;
      transform: translateY(-50%);
      pointer-events: auto;
      cursor: pointer;
      border: 2px solid color-mix(in srgb, var(--crew-color, var(--accent)) 74%, var(--line));
      background:
        radial-gradient(circle at center, var(--surface) 0 50%, transparent 52%),
        repeating-conic-gradient(from -1deg, color-mix(in srgb, var(--crew-color, var(--accent)) 72%, transparent) 0 1deg, transparent 1deg 30deg);
      box-shadow: 0 0 0 3px color-mix(in srgb, var(--crew-color, var(--accent)) 8%, transparent), 0 0 12px color-mix(in srgb, var(--crew-color, var(--accent)) 22%, transparent);
      opacity: 0.94;
    }
    .ctox-progress-visual:hover .ctox-progress-activity,
    .ctox-progress-visual:focus-visible .ctox-progress-activity {
      transform: translateY(-50%) scale(1.08);
    }
    .ctox-progress-track {
      position: absolute;
      left: 0;
      right: 0;
      bottom: -1px;
      width: 100%;
      height: 5px;
      gap: 0;
      overflow: hidden;
      border-radius: 0 0 13px 13px;
      background: color-mix(in srgb, var(--line) 42%, transparent);
      pointer-events: auto;
      cursor: pointer;
      box-shadow: 0 -1px 0 color-mix(in srgb, var(--line) 28%, transparent);
    }
    .ctox-progress-track::before {
      content: '';
      position: absolute;
      inset: 0 auto 0 0;
      width: calc(var(--ctox-progress-percent, 0) * 1%);
      background: color-mix(in srgb, var(--crew-color, var(--accent)) 90%, white 10%);
      box-shadow: 0 0 12px color-mix(in srgb, var(--crew-color, var(--accent)) 58%, transparent);
      transition: width var(--motion-slow) var(--ease-standard);
    }
    .ctox-progress-work {
      position: relative;
      z-index: 1;
      gap: 0;
    }
    .ctox-progress-segment,
    .ctox-progress-review {
      position: relative;
      z-index: 1;
      border-radius: 0;
      border-right: 1px solid color-mix(in srgb, var(--surface) 80%, transparent);
      background: transparent;
      opacity: 1;
    }
    .ctox-progress-review { border-left: 1px solid color-mix(in srgb, var(--surface) 80%, transparent); border-right: 0; }
    .ctox-progress-segment.is-completed,
    .ctox-progress-review.is-completed {
      background: transparent;
      opacity: 1;
    }
    .ctox-progress-segment.is-in_progress,
    .ctox-progress-visual.is-reviewing .ctox-progress-review:is(.is-running, .is-pending) {
      background: color-mix(in srgb, var(--crew-color, var(--accent)) 34%, transparent);
      opacity: 1;
      animation: none;
    }
    .ctox-progress-visual:not(.is-reviewing) .ctox-progress-review.is-pending {
      background: transparent;
      animation: none;
    }
    .ctox-progress-planning-line {
      width: 0;
      height: 100%;
      border-radius: 0;
      background: transparent;
      box-shadow: none;
      animation: none;
    }
    .ctox-progress-visual.is-planning .ctox-progress-activity,
    .ctox-progress-visual.is-dormant .ctox-progress-activity {
      opacity: 0.34;
      animation: none;
    }
    .ctox-progress-visual.is-planning .ctox-progress-activity::after,
    .ctox-progress-visual.is-dormant .ctox-progress-activity::after {
      display: none;
    }
    @keyframes ctoxTurnInstrumentPulse {
      0% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--accent) 58%, transparent), 0 0 12px color-mix(in srgb, var(--accent) 22%, transparent); }
      55% { box-shadow: 0 0 0 7px transparent, 0 0 18px color-mix(in srgb, var(--accent) 62%, transparent); }
      100% { box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 8%, transparent), 0 0 12px color-mix(in srgb, var(--accent) 22%, transparent); }
    }
    .ctox-chat-delegation-card:is(.is-thinking, .is-tool) .ctox-progress-activity {
      animation: ctoxTurnInstrumentPulse 620ms var(--ease-spring) 1;
    }
    .ctox-chat-track {
      width: auto;
      min-width: 28px;
      height: 24px;
      padding: 0 6px;
      gap: 4px;
      color: color-mix(in srgb, var(--accent) 72%, var(--muted)) !important;
      cursor: pointer;
    }
    .ctox-chat-track code {
      position: absolute;
      width: 1px;
      height: 1px;
      margin: -1px;
      padding: 0;
      overflow: hidden;
      clip: rect(0 0 0 0);
      white-space: nowrap;
      border: 0;
    }
    .ctox-chat-track:hover,
    .ctox-chat-track:focus-visible {
      color: var(--accent) !important;
      background: color-mix(in srgb, var(--accent) 9%, transparent) !important;
    }
    @media (prefers-reduced-motion: reduce) {
      .ctox-progress-segment.is-in_progress,
      .ctox-progress-review:is(.is-running, .is-pending),
      .ctox-progress-planning-line,
      .ctox-progress-visual.is-planning .ctox-progress-activity {
        animation: none !important;
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

function fileToBase64(file) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.readAsDataURL(file);
    reader.onload = () => resolve(reader.result);
    reader.onerror = (error) => reject(error);
  });
}

function dataUrlBase64(dataUrl) {
  return String(dataUrl || '').split(',')[1] || '';
}

function newClientId(prefix) {
  const random = globalThis.crypto?.randomUUID?.() || `${Date.now()}_${Math.random().toString(36).slice(2)}`;
  return `${prefix}_${String(random).replace(/[^a-zA-Z0-9_-]/g, '_')}`;
}

function extensionForAttachment(name) {
  const extension = String(name || '').split('.').pop()?.toLowerCase() || '';
  return extension === String(name || '').toLowerCase() ? '' : extension;
}

function safeAttachmentName(name) {
  const cleaned = String(name || 'attachment')
    .replace(/[\/\\:\0-\x1f]/g, '_')
    .replace(/\s+/g, ' ')
    .trim();
  return (cleaned || 'attachment').slice(0, 120);
}

function chatMessageAttachmentSummary(attachment = {}) {
  return {
    name: attachment.name || 'attachment',
    mime_type: attachment.mimeType || attachment.mime_type || 'application/octet-stream',
    size_bytes: Number(attachment.size || attachment.size_bytes || 0),
  };
}

async function prepareChatAttachment(file) {
  const now = Date.now();
  const name = safeAttachmentName(file.name || 'attachment');
  const base64Data = await fileToBase64(file);
  const base64 = dataUrlBase64(base64Data);
  const contentHash = await sha256Hex(base64ToBytes(base64));
  return {
    attachmentId: newClientId('chatatt'),
    fileId: newClientId('chatfile'),
    generationId: `gen_${now}_${contentHash.slice(0, 12)}`,
    name,
    mimeType: file.type || 'application/octet-stream',
    size: file.size || 0,
    extension: extensionForAttachment(name),
    base64Data,
    contentHash,
    contentHashScheme: FILE_CONTENT_HASH_SCHEME,
    createdAt: now,
  };
}

async function addAttachmentToChatState(chat, file) {
  if (file.size > CHAT_ATTACHMENT_MAX_BYTES) {
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
    chat.attachments.push(await prepareChatAttachment(file));
  } catch (err) {
    console.error("Fehler beim Konvertieren der Datei zu Base64", err);
  }
}

async function stageChatAttachments({ db, sync, chat, commandId, messageId, attachments }) {
  const staged = Array.isArray(attachments) ? attachments.filter(Boolean) : [];
  if (!staged.length) return [];
  const attachmentSync = await prepareAttachmentSync(sync);
  try {
    const files = db?.collection?.('desktop_files') || db?.raw?.desktop_files;
    const chunks = db?.collection?.('desktop_file_chunks') || db?.raw?.desktop_file_chunks;
    if (!files || !chunks) {
      throw new Error('Business-OS Dateiablage ist nicht verfügbar.');
    }
    await ensureChatAttachmentRoot(files);
    const refs = [];
    for (const attachment of staged) {
      refs.push(await stageChatAttachment({
        files,
        chunks,
        chat,
        commandId,
        messageId,
        attachment,
      }));
    }
    await flushAttachmentSync(sync, attachmentSync);
    return refs;
  } finally {
    await releaseAttachmentChunkSync(attachmentSync);
  }
}

async function prepareAttachmentSync(sync) {
  if (!sync?.startCollection && !sync?.leaseCollection) return { bridges: [], leases: [], sync: null };
  const leases = [];
  const fileBridge = await sync?.startCollection?.('desktop_files').catch(() => null);
  const chunkBridge = await startAttachmentChunkSync(sync, leases);
  const bridges = [fileBridge, chunkBridge].filter(Boolean);
  await Promise.all(bridges.map((bridge) => waitForSyncBridgeReady(bridge, 10000)));
  return { bridges, leases, sync };
}

async function flushAttachmentSync(sync, attachmentSync = null) {
  if (!sync?.startCollection && !attachmentSync?.bridges?.length) return;
  const fileBridge = attachmentSync?.bridges?.find((bridge) => syncHandleCollection(bridge) === 'desktop_files')
    || await sync?.startCollection?.('desktop_files').catch(() => null);
  const chunkBridge = attachmentSync?.bridges?.find((bridge) => syncHandleCollection(bridge) === 'desktop_file_chunks');
  const bridges = [chunkBridge, fileBridge].filter(Boolean);
  await Promise.all(bridges.map((bridge) => waitForSyncBridgeReady(bridge, 15000)));
}

async function startAttachmentChunkSync(sync, leases) {
  if (typeof sync?.leaseCollection === 'function') {
    const lease = await sync.leaseCollection('desktop_file_chunks', 'business-chat-attachment');
    leases.push(lease);
    return lease;
  }
  throw new Error('desktop_file_chunks requires sync.leaseCollection().');
}

async function releaseAttachmentChunkSync(attachmentSync) {
  const leases = attachmentSync?.leases || [];
  if (leases.length) {
    await Promise.all(leases.map((lease) => lease?.release?.().catch(() => null)));
    return;
  }
  const chunkBridge = attachmentSync?.bridges?.find((bridge) => syncHandleCollection(bridge) === 'desktop_file_chunks');
  if (chunkBridge?.stop) {
    await chunkBridge.stop().catch(() => null);
  }
}

async function waitForSyncBridgeReady(bridge, timeoutMs = 10000) {
  const state = syncBridgeFromHandle(bridge)?.state;
  if (!state) return;
  let timer = null;
  try {
    await Promise.race([
      Promise.resolve()
        .then(() => state.awaitInSync?.() || state.awaitInitialReplication?.())
        .catch(() => {}),
      new Promise((resolve) => {
        timer = setTimeout(resolve, timeoutMs);
        timer?.unref?.();
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}

function syncBridgeFromHandle(handle) {
  return handle?.bridge || handle;
}

function syncHandleCollection(handle) {
  return handle?.collection || handle?.bridge?.collection || '';
}

async function ensureChatAttachmentRoot(files) {
  const now = Date.now();
  await upsertRxDocument(files, {
    id: CHAT_ATTACHMENT_ROOT_ID,
    parent_id: '',
    path: CHAT_ATTACHMENT_ROOT_PATH,
    virtual_path: CHAT_ATTACHMENT_ROOT_PATH,
    name: 'Business OS Chat',
    kind: 'folder',
    mime_type: '',
    extension: '',
    size_bytes: 0,
    source: 'business-os-chat',
    content_state: 'directory',
    sort_index: 90,
    is_deleted: false,
    created_at_ms: now,
    updated_at_ms: now,
  });
}

async function stageChatAttachment({ files, chunks, chat, commandId, messageId, attachment }) {
  const now = Date.now();
  const prepared = attachment.contentHash && attachment.base64Data
    ? attachment
    : await prepareStoredChatAttachment(attachment);
  const base64 = dataUrlBase64(prepared.base64Data);
  const total = Math.max(1, Math.ceil(base64.length / CHAT_ATTACHMENT_CHUNK_SIZE));
  const name = safeAttachmentName(prepared.name);
  const fileId = prepared.fileId || newClientId('chatfile');
  const contentHash = prepared.contentHash || await sha256Hex(base64ToBytes(base64));
  const generationId = prepared.generationId || `gen_${now}_${contentHash.slice(0, 12)}`;
  for (let idx = 0; idx < total; idx += 1) {
    const data = base64.slice(idx * CHAT_ATTACHMENT_CHUNK_SIZE, (idx + 1) * CHAT_ATTACHMENT_CHUNK_SIZE);
    await upsertRxDocument(chunks, {
      id: `${fileId}_${generationId}_${idx}`,
      file_id: fileId,
      generation_id: generationId,
      content_hash: contentHash,
      content_hash_scheme: FILE_CONTENT_HASH_SCHEME,
      idx,
      total,
      encoding: 'base64',
      data,
      chunk_hash: await sha256Hex(data),
      chunk_hash_scheme: FILE_CHUNK_HASH_SCHEME,
      size_bytes: data.length,
      created_at_ms: now,
    });
  }
  const virtualPath = `${CHAT_ATTACHMENT_ROOT_PATH}/${chat.id}/${name}`.replace(/\/+/g, '/');
  await upsertRxDocument(files, {
    id: fileId,
    parent_id: CHAT_ATTACHMENT_ROOT_ID,
    path: virtualPath,
    local_path: '',
    virtual_path: virtualPath,
    name,
    kind: 'file',
    mime_type: prepared.mimeType || prepared.mime_type || 'application/octet-stream',
    extension: prepared.extension || extensionForAttachment(name),
    size_bytes: Number(prepared.size || prepared.size_bytes || 0),
    owner_id: chat.owner_user_id || '',
    source: 'business-os-chat',
    linked_collection: 'business_chats',
    linked_record_id: chat.id,
    content_ref: fileId,
    content_state: 'available',
    content_hash: contentHash,
    content_hash_scheme: FILE_CONTENT_HASH_SCHEME,
    content_generation_id: generationId,
    content_synced_at_ms: now,
    sort_index: now,
    is_deleted: false,
    created_at_ms: prepared.createdAt || now,
    updated_at_ms: now,
  });
  return {
    kind: 'desktop_file',
    storage_collection: 'desktop_files',
    chunk_collection: 'desktop_file_chunks',
    attachment_id: prepared.attachmentId || newClientId('chatatt'),
    file_id: fileId,
    generation_id: generationId,
    name,
    mime_type: prepared.mimeType || prepared.mime_type || 'application/octet-stream',
    size_bytes: Number(prepared.size || prepared.size_bytes || 0),
    content_hash: contentHash,
    content_hash_scheme: FILE_CONTENT_HASH_SCHEME,
    virtual_path: virtualPath,
    chat_id: chat.id,
    message_id: messageId,
    command_id: commandId,
    content_state: 'available',
  };
}

async function prepareStoredChatAttachment(attachment) {
  if (attachment?.base64Data) {
    const base64 = dataUrlBase64(attachment.base64Data);
    return {
      ...attachment,
      name: safeAttachmentName(attachment.name),
      mimeType: attachment.mimeType || attachment.mime_type || 'application/octet-stream',
      size: Number(attachment.size || attachment.size_bytes || base64ToBytes(base64).length),
      contentHash: attachment.contentHash || await sha256Hex(base64ToBytes(base64)),
      contentHashScheme: FILE_CONTENT_HASH_SCHEME,
    };
  }
  throw new Error(`Anhang ${attachment?.name || ''} hat keine lokalen Inhaltsdaten.`);
}

async function upsertRxDocument(collection, doc) {
  if (typeof collection.upsert === 'function') {
    try {
      await collection.upsert(doc);
      return;
    } catch (error) {
      if (!isRxDbConflictError(error)) throw error;
    }
  }
  const existing = await collection.findOne(doc.id).exec();
  if (existing) await existing.incrementalPatch(doc);
  else await collection.insert(doc);
}

function isRxDbConflictError(error) {
  const message = String(error?.message || error || '');
  return message.includes('RxDB Error-Code: CONFLICT')
    || message.includes('conflict')
    || message.includes('document already exists')
    || message.includes('Document update conflict');
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

function initSchedulerLoop({ root, state, commandBus, db, sync, getActiveModule, onTrackingStateChanged = null }) {
  clearLegacyChatSchedulerInterval();

  const scheduledEntries = collectScheduledChatEntries(state);
  if (!scheduledEntries.length) {
    clearSchedulerLoop(root);
    return;
  }

  const scheduler = root.__ctoxChatScheduler || {
    running: false,
    timer: null,
    rerun: false,
  };
  scheduler.args = { root, state, commandBus, db, sync, getActiveModule, onTrackingStateChanged };
  root.__ctoxChatScheduler = scheduler;
  scheduleSchedulerTick(root, schedulerDelayMs({ root, state, entries: scheduledEntries }));
}

function clearLegacyChatSchedulerInterval() {
  if (!window._ctoxChatSchedulerInterval) return;
  window.clearInterval(window._ctoxChatSchedulerInterval);
  delete window._ctoxChatSchedulerInterval;
}

function clearSchedulerLoop(root) {
  clearLegacyChatSchedulerInterval();
  const scheduler = root?.__ctoxChatScheduler;
  if (scheduler?.timer) window.clearTimeout(scheduler.timer);
  if (root) delete root.__ctoxChatScheduler;
}

function scheduleSchedulerTick(root, delayMs) {
  const scheduler = root?.__ctoxChatScheduler;
  if (!scheduler) return;
  if (scheduler.timer) window.clearTimeout(scheduler.timer);
  scheduler.timer = window.setTimeout(() => {
    scheduler.timer = null;
    runScheduledChatTick(root).catch((error) => {
      console.warn('[business-chat] scheduled chat tick failed', error);
      initSchedulerLoop(scheduler.args);
    });
  }, Math.max(0, delayMs));
}

async function runScheduledChatTick(root) {
  const scheduler = root?.__ctoxChatScheduler;
  if (!scheduler?.args) return;
  if (scheduler.running) {
    scheduler.rerun = true;
    return;
  }
  scheduler.running = true;
  try {
    await processScheduledChats(scheduler.args);
  } finally {
    const args = scheduler.args;
    const rerun = scheduler.rerun;
    scheduler.running = false;
    scheduler.rerun = false;
    if (rerun) {
      scheduleSchedulerTick(root, 0);
    } else {
      initSchedulerLoop(args);
    }
  }
}

function collectScheduledChatEntries(state) {
  const entries = [];
  for (const chat of state?.chats || []) {
    const messages = Array.isArray(chat.messages) ? chat.messages : [];
    messages.forEach((message, messageIndex) => {
      if (message?.status !== 'scheduled') return;
      entries.push({
        chat,
        message,
        messageIndex,
        dueAt: Number(chat.createdAt) || Date.now(),
      });
    });
  }
  return entries;
}

function schedulerDelayMs({ root, state, entries = collectScheduledChatEntries(state) }) {
  if (!entries.length) return null;
  const nowMs = Date.now();
  const nextDueMs = Math.min(...entries.map((entry) => entry.dueAt));
  if (nextDueMs <= nowMs) return 0;
  const hasVisibleCountdown = Boolean(root?.querySelectorAll?.('[data-countdown-timer]')?.length);
  if (hasVisibleCountdown) return Math.min(1000, Math.max(100, nextDueMs - nowMs));
  return Math.min(nextDueMs - nowMs, 2_147_483_647);
}

async function processScheduledChats({ root, state, commandBus, db, sync, getActiveModule, onTrackingStateChanged = null }) {
  const timerEls = root.querySelectorAll('[data-countdown-timer]');
  timerEls.forEach(el => {
    const chatId = el.dataset.countdownTimer;
    const chat = state.chats.find(c => c.id === chatId);
    if (chat) {
      // Countdown text changes every second when a timer is visible; still
      // guard so identical second values do not force a layout pass.
      setTextIfChanged(el, getCountdownText(chat.createdAt));
    }
  });

  const nowMs = Date.now();
  const dueEntries = collectScheduledChatEntries(state).filter((entry) => entry.dueAt <= nowMs);
  if (!dueEntries.length) return;

  for (const { chat, message: scheduledMsg } of dueEntries) {
    console.log(`[business-chat] Executing scheduled chat task for chat ${chat.id}`);

    scheduledMsg.status = 'waiting';
    const commandId = scheduledMsg.commandId || `cmd_${crypto.randomUUID()}`;
    chat.lastTrackingId = commandId;
    scheduledMsg.commandId = commandId;
    touchChats(state, [chat]);
  }

  await persistChatState({ state, db });
  renderChatRoot({ root, state, commandBus, db, getActiveModule });
  onTrackingStateChanged?.();

  for (const { chat, message: scheduledMsg } of dueEntries) {
    await dispatchScheduledChat({ chat, scheduledMsg, commandBus, db, sync });
  }

  await persistChatState({ state, db });
  renderChatRoot({ root, state, commandBus, db, getActiveModule });
  onTrackingStateChanged?.();

  syncTrackedMessages({ state, db }).then((changed) => {
    if (changed) persistChatState({ state, db });
    if (changed) renderChatRoot({ root, state, commandBus, db, getActiveModule });
    onTrackingStateChanged?.();
  }).catch(() => {});
}

async function dispatchScheduledChat({ chat, scheduledMsg, commandBus, db, sync }) {
  const commandId = scheduledMsg.commandId || `cmd_${crypto.randomUUID()}`;
  const originalUserMessage = scheduledMsg.userMessageId
    ? chat.messages.find((message) => message.id === scheduledMsg.userMessageId)
    : null;
  const text = scheduledMsg.promptText || scheduledMsg.prompt_text || originalUserMessage?.text || scheduledMsg.text || '';
  const userMessageId = scheduledMsg.userMessageId || originalUserMessage?.id || scheduledMsg.id;
  const now = Date.now();
  const scheduledAttachments = chat.scheduledAttachmentsByCommand?.[commandId] || [];
  const chatClientContext = chat.contextMeta?.client_context && typeof chat.contextMeta.client_context === 'object'
    ? chat.contextMeta.client_context
    : {};
  let attachmentRefs = [];
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
      message_id: userMessageId,
      conversation: compactConversation(chat.messages),
      attachments: attachmentRefs,
      attachment_refs: attachmentRefs,
      inbound_channel: CHAT_CHANNEL,
      outbound_channel: 'business_os_chat',
      response_channel: 'business_os_chat',
      reply_to: chat.id,
      thread_key: `business-os/chat/${chat.id}`,
      priority: 'normal',
      source_module: chat.contextMeta?.module || 'ctox',
    },
    client_context: {
      ...chatClientContext,
      source: 'business-os-chat',
      module: chat.contextMeta?.module || 'ctox',
      source_module: chat.contextMeta?.module || 'ctox',
      source_title: chat.contextMeta?.source_title || 'Crew',
      inbound_channel: CHAT_CHANNEL,
      outbound_channel: 'business_os_chat',
      chat_id: chat.id,
      message_id: userMessageId,
      attachment_count: attachmentRefs.length,
      attachment_storage: attachmentRefs.length ? 'desktop_files' : '',
      url: location.href,
      language: document.documentElement.lang || 'de',
      created_at: new Date(now).toISOString(),
    },
  };

  try {
    attachmentRefs = await stageChatAttachments({
      db,
      sync,
      chat,
      commandId,
      messageId: userMessageId,
      attachments: scheduledAttachments,
    });
    command.payload.attachments = attachmentRefs;
    command.payload.attachment_refs = attachmentRefs;
    command.client_context.attachment_count = attachmentRefs.length;
    command.client_context.attachment_storage = attachmentRefs.length ? 'desktop_files' : '';
    const result = await commandBus.dispatch(command);
    const taskId = result.task_id || '';
    const acceptedCommandId = result.command_id || commandId;
    if (!taskId) {
      throw new Error('Die Annahme der Aufgabe wurde noch nicht bestätigt.');
    }
    chat.lastTrackingId = taskId || acceptedCommandId;

    const statusMsg = chat.messages.find(m => m.id === `status_${commandId}`);
    if (statusMsg) {
      statusMsg.text = 'Deine Aufgabe steht auf der Aufgabenliste. Die Antwort erscheint hier, sobald die Crew sie bearbeitet hat.';
      statusMsg.commandId = acceptedCommandId;
      statusMsg.taskId = taskId;
      statusMsg.status = result.task_status || result.status || 'queued';
    }
    if (chat.scheduledAttachmentsByCommand) {
      delete chat.scheduledAttachmentsByCommand[commandId];
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

export const __businessChatTestInternals = Object.freeze({
  hasTrackingMarker,
  hasReplyAlready,
  crewMemberExpression,
  wireCrewAppPresence,
  crewPoolSlotHtml,
  clearSchedulerLoop,
  chatAllowsAutoFocus,
  chatContextMetaFromDetail,
  collectScheduledChatEntries,
  collectTrackedMessages,
  collapseRestoredTerminalChat,
  crewCreatureHtml,
  crewCreatureMode,
  crewIdentity,
  delegationProgressCardHtml,
  executionProgressForChat,
  executionProgressHeaderHtml,
  executionProgressSignature,
  messageMarkup,
  isChatInspectionMessage,
  chatInspectionContent,
  chatInspectionMarkup,
  chatMessagesMarkup,
  chatWindow,
  chatComposerSignature,
  mergeChatPair,
  mergeChatMessages,
  claimChatOpenOwnership,
  currentChatOpenOwnership,
  ownsChatOpenOwnership,
  invalidateChatOpenOwnership,
  markChatExpandedByUser,
  markChatMinimizedByUser,
  normalizeExecutionProgress,
  createTrackedMessageWatch,
  findDocsByIds,
  findChatForOpenDetail,
  chatOpenNeedsHydration,
  focusChatForUser,
  getLocalDateString,
  hasActiveTrackedMessages,
  hasTrackedMessagesNeedingSync,
  flushChatTrackingCollections,
  hydrateChatsFromRxDb,
  initSchedulerLoop,
  isChatEmptyForDeletion,
  isScrolledToBottom,
  preferredChatForDockOpen,
  readChatState,
  renderChatRoot,
  resolveChatForOpenDetail,
  stopCrewProceduralMotion,
  syncCrewProceduralMotion,
  persistChatDocsRemote,
  persistChatState,
  schedulerDelayMs,
  setAttrIfChanged,
  setClassNameIfChanged,
  setTextIfChanged,
  shouldDeferRemoteChatHydration,
  startChatLiveCollections,
  stageChatAttachments,
  stageWindowChats,
  submitChatMessage,
  waitForSubmittedTaskId,
  windowTaskStateMatches,
  syncTrackedMessages,
  isTransientCommandTrackingError,
  withChatPersistenceTimeout,
  isBlockedTrackingStatus,
  isCancelledTrackingStatus,
  isFailureStatus,
  isTerminalTrackingStatus,
  isActiveTrackingStatus,
  getTaskState,
  preferredTrackingStatus,
  progressShowsActiveReview,
});
