import { loadModuleMessages } from '../../shared/i18n.js';
import {
  THREAD_COLLECTIONS,
  buildApprovalRequestPayload,
  buildNotePayload,
  buildThreadsCommand,
  splitUserIds,
} from './commands.js';

const REFRESH_DEBOUNCE_MS = 80;

const labels = {
  de: {
    refresh: 'Aktualisieren',
    search: 'Threads suchen',
    noThreads: 'Keine relevanten Threads vorhanden.',
    noSelection: 'Kein Thread ausgewählt.',
    commandFailed: 'Aktion konnte nicht abgeschlossen werden.',
  },
  en: {
    refresh: 'Refresh',
    search: 'Search threads',
    noThreads: 'No relevant threads.',
    noSelection: 'No thread selected.',
    commandFailed: 'Action could not be completed.',
  },
};

const state = {
  ctx: null,
  t: (key, fallback) => fallback || key,
  filter: 'inbox',
  search: '',
  selectedId: '',
  data: emptyData(),
  cleanup: [],
  refreshTimer: null,
  busy: false,
  status: '',
};

let els = {};

export async function mount(ctx) {
  state.ctx = ctx;
  state.filter = 'inbox';
  state.search = '';
  state.selectedId = '';
  state.data = emptyData();
  state.status = '';

  const messages = await loadModuleMessages(import.meta.url, ctx.locale || 'de', labels);
  state.t = (key, fallback) => messages[key] ?? fallback ?? key;

  await ensureStyles();
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  ctx.host.innerHTML = html;
  ctx.left.replaceChildren();
  ctx.right.replaceChildren();

  bindElements(ctx.host);
  applyLabels();
  wireUi();
  await startSync();
  state.cleanup.push(wireRealtime());
  await refresh();

  return () => {
    state.cleanup.forEach((fn) => {
      try { fn?.(); } catch {}
    });
    state.cleanup = [];
    if (state.refreshTimer) window.clearTimeout(state.refreshTimer);
  };
}

function emptyData() {
  return {
    threads: [],
    messages: [],
    links: [],
    notifications: [],
    approvals: [],
    commands: [],
    queue: [],
  };
}

async function ensureStyles() {
  if (document.querySelector('link[data-threads-style]')) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = new URL('./index.css', import.meta.url).href;
  link.dataset.threadsStyle = 'true';
  document.head.append(link);
}

function bindElements(root) {
  els.root = root.querySelector('[data-threads-root]');
  els.refresh = root.querySelector('[data-refresh]');
  els.search = root.querySelector('[data-thread-search]');
  els.filters = [...root.querySelectorAll('[data-filter]')];
  els.list = root.querySelector('[data-thread-list]');
  els.title = root.querySelector('[data-thread-title]');
  els.source = root.querySelector('[data-thread-source]');
  els.status = root.querySelector('[data-thread-status]');
  els.timeline = root.querySelector('[data-thread-timeline]');
  els.context = root.querySelector('[data-thread-context]');
  els.messageForm = root.querySelector('[data-message-form]');
  els.messageBody = root.querySelector('[data-message-body]');
  els.noteForm = root.querySelector('[data-note-form]');
  els.noteTarget = root.querySelector('[data-note-target]');
  els.noteBody = root.querySelector('[data-note-body]');
  els.approvalForm = root.querySelector('[data-approval-form]');
  els.approvalReviewer = root.querySelector('[data-approval-reviewer]');
  els.approvalPrompt = root.querySelector('[data-approval-prompt]');
  els.watch = root.querySelector('[data-thread-watch]');
  els.snooze = root.querySelector('[data-thread-snooze]');
  els.archive = root.querySelector('[data-thread-archive]');
}

function applyLabels() {
  if (els.refresh) {
    els.refresh.title = state.t('refresh', 'Aktualisieren');
    els.refresh.setAttribute('aria-label', state.t('refresh', 'Aktualisieren'));
  }
  if (els.search) {
    els.search.placeholder = state.t('search', 'Threads suchen');
    els.search.setAttribute('aria-label', state.t('search', 'Threads suchen'));
  }
}

function wireUi() {
  els.refresh?.addEventListener('click', () => refresh({ restartSync: true }));
  els.search?.addEventListener('input', (event) => {
    state.search = event.target.value || '';
    syncSelection();
    render();
  });
  els.filters.forEach((button) => {
    button.addEventListener('click', () => {
      state.filter = button.dataset.filter || 'inbox';
      els.filters.forEach((item) => item.classList.toggle('is-active', item === button));
      syncSelection();
      render();
    });
  });
  els.list?.addEventListener('click', (event) => {
    const target = event.target instanceof Element ? event.target : null;
    const row = target?.closest?.('[data-thread-id]');
    if (!row) return;
    state.selectedId = row.getAttribute('data-thread-id') || '';
    render();
  });
  els.context?.addEventListener('click', (event) => {
    const target = event.target instanceof Element ? event.target : null;
    const deepLink = target?.closest?.('[data-thread-deep-link]');
    const mark = target?.closest?.('[data-mark-notification]');
    const dismiss = target?.closest?.('[data-dismiss-notification]');
    if (deepLink) {
      event.preventDefault();
      navigateDeepLink(deepLink.getAttribute('data-thread-deep-link') || deepLink.getAttribute('href') || '');
    } else if (mark) {
      updateNotification(mark.getAttribute('data-mark-notification') || '', 'mark_read').catch(showError);
    } else if (dismiss) {
      updateNotification(dismiss.getAttribute('data-dismiss-notification') || '', 'dismiss').catch(showError);
    }
  });
  els.timeline?.addEventListener('click', (event) => {
    const target = event.target instanceof Element ? event.target : null;
    const approve = target?.closest?.('[data-approve-approval]');
    const reject = target?.closest?.('[data-reject-approval]');
    const edit = target?.closest?.('[data-edit-approval]');
    if (approve) {
      decideApproval(approve.getAttribute('data-approve-approval') || '', 'approve').catch(showError);
    } else if (reject) {
      decideApproval(reject.getAttribute('data-reject-approval') || '', 'reject').catch(showError);
    } else if (edit) {
      editApproval(edit.getAttribute('data-edit-approval') || '').catch(showError);
    }
  });
  els.watch?.addEventListener('click', () => toggleWatch().catch(showError));
  els.snooze?.addEventListener('click', () => snoozeSelectedThread().catch(showError));
  els.archive?.addEventListener('click', () => archiveSelectedThread().catch(showError));
  els.messageForm?.addEventListener('submit', (event) => {
    event.preventDefault();
    submitMessage().catch(showError);
  });
  els.noteForm?.addEventListener('submit', (event) => {
    event.preventDefault();
    submitNote().catch(showError);
  });
  els.approvalForm?.addEventListener('submit', (event) => {
    event.preventDefault();
    submitApprovalRequest().catch(showError);
  });
}

async function startSync() {
  await Promise.all(THREAD_COLLECTIONS.map((name) => state.ctx?.sync?.startCollection?.(name).catch(() => null)));
}

function wireRealtime() {
  const subscriptions = THREAD_COLLECTIONS
    .map((name) => collectionFor(name)?.$?.subscribe?.(() => scheduleRefresh()))
    .filter(Boolean);
  return () => subscriptions.forEach((subscription) => {
    try { subscription.unsubscribe?.(); } catch {}
  });
}

function scheduleRefresh() {
  if (state.refreshTimer) return;
  state.refreshTimer = window.setTimeout(() => {
    state.refreshTimer = null;
    refresh().catch((error) => console.warn('[threads] refresh failed', error));
  }, REFRESH_DEBOUNCE_MS);
}

async function refresh(options = {}) {
  if (options.restartSync) await startSync();
  const baseEntries = await Promise.all([
    ['threads', loadCollection('user_threads')],
    ['messages', loadCollection('user_thread_messages')],
    ['links', loadCollection('user_thread_links')],
    ['notifications', loadCollection('user_notifications')],
    ['approvals', loadCollection('ctox_task_approval_requests')],
  ].map(async ([key, promise]) => [key, await promise]));
  const base = Object.fromEntries(baseEntries);
  const commandIds = linkedCommandIds(base);
  const commands = await loadRecordsByIds('business_commands', commandIds);
  const taskIds = linkedTaskIds(base, commands);
  const queue = await loadRecordsByIds('ctox_queue_tasks', taskIds);
  state.data = {
    ...base,
    commands,
    queue,
  };
  syncSelection();
  render();
}

async function loadCollection(name) {
  const collection = collectionFor(name);
  if (!collection?.find) return [];
  const docs = await collection.find().exec();
  return docs
    .map((doc) => doc?.toJSON?.() || doc)
    .filter((doc) => doc && doc._deleted !== true && doc.is_deleted !== true);
}

function collectionFor(name) {
  return state.ctx?.db?.collection?.(name) || null;
}

async function loadRecordsByIds(name, ids) {
  const collection = collectionFor(name);
  const uniqueIds = [...new Set((ids || []).map((id) => String(id || '').trim()).filter(Boolean))];
  if (!collection?.findOne || !uniqueIds.length) return [];
  const docs = await Promise.all(uniqueIds.map((id) => collection.findOne(id).exec().catch(() => null)));
  return docs
    .map((doc) => doc?.toJSON?.() || doc)
    .filter((doc) => doc && doc._deleted !== true && doc.is_deleted !== true);
}

function render() {
  const threads = visibleThreads();
  renderList(threads);
  renderDetail(threads);
}

function visibleThreads() {
  const me = currentUserId();
  const isAdmin = currentUserRole() === 'chef' || currentUserRole() === 'admin';
  const search = state.search.trim().toLowerCase();
  return state.data.threads
    .filter((thread) => {
      if (!isAdmin && me && !threadRelevantToUser(thread, me)) return false;
      if (state.filter === 'archived') return thread.status === 'archived';
      if (thread.status === 'archived') return false;
      if (state.filter === 'snoozed') return isSnoozed(thread);
      if (isSnoozed(thread)) return false;
      if (state.filter === 'approvals') {
        return approvalsForThread(thread.id).some((item) => item.status === 'pending' && (!me || item.reviewer_user_id === me || isAdmin));
      }
      if (state.filter === 'mentions') return threadMentionsUser(thread.id, me);
      if (state.filter === 'waiting') return threadWaitingOnUser(thread.id, me, isAdmin);
      if (state.filter === 'delegated') return threadDelegatedByUser(thread, me);
      if (state.filter === 'running') return threadHasRunningCtox(thread.id);
      if (state.filter === 'failed') return threadHasFailedCtox(thread.id) || thread.status === 'blocked';
      if (state.filter === 'watching') return arrayField(thread.watcher_user_ids).includes(me);
      if (state.filter === 'inbox') {
        return !me
          || unreadNotificationsForThread(thread.id, me).length > 0
          || arrayField(thread.participant_ids).includes(me)
          || approvalsForThread(thread.id).some((item) => item.reviewer_user_id === me && item.status === 'pending');
      }
      return true;
    })
    .filter((thread) => {
      if (!search) return true;
      const haystack = [
        thread.title,
        thread.source_module,
        thread.source_label,
        thread.source_record_id,
        ...messagesForThread(thread.id).map((item) => item.body),
      ].join(' ').toLowerCase();
      return haystack.includes(search);
    })
    .sort((left, right) => Number(right.last_message_at_ms || right.updated_at_ms || 0) - Number(left.last_message_at_ms || left.updated_at_ms || 0));
}

function syncSelection() {
  const visible = visibleThreads();
  if (!visible.some((thread) => thread.id === state.selectedId)) {
    state.selectedId = visible[0]?.id || '';
  }
}

function renderList(threads) {
  if (!els.list) return;
  if (!threads.length) {
    els.list.innerHTML = `<div class="threads-empty">${escapeHtml(state.t('noThreads', 'Keine relevanten Threads vorhanden.'))}</div>`;
    return;
  }
  els.list.innerHTML = threads.map((thread) => {
    const messages = messagesForThread(thread.id);
    const last = messages[messages.length - 1];
    const pending = approvalsForThread(thread.id).filter((item) => item.status === 'pending').length;
    return `
      <button type="button" class="threads-list-item ${thread.id === state.selectedId ? 'is-active' : ''}"
        data-thread-id="${escapeAttr(thread.id)}"
        data-record-id="${escapeAttr(thread.source_record_id || thread.id)}"
        data-record-type="thread"
        data-title="${escapeAttr(thread.title || '')}">
        <span class="threads-list-title">${escapeHtml(thread.title || thread.id)}</span>
        <span class="threads-list-meta">${escapeHtml(thread.source_module || 'threads')} · ${escapeHtml(formatTime(thread.last_message_at_ms || thread.updated_at_ms))}${pending ? ` · ${pending} offen` : ''}</span>
        <span class="threads-list-preview">${escapeHtml(last?.body || thread.source_label || '')}</span>
      </button>
    `;
  }).join('');
}

function renderDetail(threads) {
  const thread = threads.find((item) => item.id === state.selectedId) || null;
  if (!thread) {
    if (els.title) els.title.textContent = state.t('noSelection', 'Kein Thread ausgewählt.');
    if (els.source) els.source.textContent = 'Threads';
    if (els.status) els.status.textContent = state.status || 'bereit';
    if (els.timeline) els.timeline.innerHTML = `<div class="threads-empty">${escapeHtml(state.t('noSelection', 'Kein Thread ausgewählt.'))}</div>`;
    if (els.context) els.context.innerHTML = '';
    if (els.messageBody) els.messageBody.disabled = true;
    setThreadActionState(null);
    return;
  }
  if (els.messageBody) els.messageBody.disabled = false;
  if (els.title) els.title.textContent = thread.title || thread.id;
  if (els.source) els.source.textContent = contextLabel(thread);
  if (els.status) els.status.textContent = state.status || thread.status || 'open';
  setThreadActionState(thread);
  renderTimeline(thread);
  renderContext(thread);
}

function renderTimeline(thread) {
  const messages = messagesForThread(thread.id);
  const approvals = approvalsForThread(thread.id);
  const timeline = [
    ...messages.map((item) => ({ type: 'message', at: Number(item.created_at_ms || item.updated_at_ms || 0), item })),
    ...approvals.map((item) => ({ type: 'approval', at: Number(item.requested_at_ms || item.created_at_ms || 0), item })),
  ].sort((left, right) => left.at - right.at);
  if (!timeline.length) {
    els.timeline.innerHTML = '<div class="threads-empty">Noch keine Nachrichten.</div>';
    return;
  }
  const me = currentUserId();
  els.timeline.innerHTML = timeline.map((entry) => {
    if (entry.type === 'approval') return renderApproval(entry.item);
    const message = entry.item;
    const mine = message.author_user_id && message.author_user_id === me;
    return `
      <article class="threads-message ${mine ? 'is-mine' : ''}" data-message-id="${escapeAttr(message.id)}">
        <div class="threads-message-meta">${escapeHtml(message.author_display_name || message.author_user_id || 'system')} · ${escapeHtml(formatTime(message.created_at_ms || message.updated_at_ms))} · ${escapeHtml(message.kind || 'note')}</div>
        <div class="threads-message-body">${escapeHtml(message.body || '')}</div>
      </article>
    `;
  }).join('');
}

function renderApproval(approval) {
  const command = approval.approved_command_id ? commandById(approval.approved_command_id) : null;
  const task = approval.approved_task_id ? taskById(approval.approved_task_id) : null;
  const me = currentUserId();
  const canDecide = approval.status === 'pending'
    && (approval.reviewer_user_id === me || currentUserRole() === 'admin' || currentUserRole() === 'chef');
  return `
    <article class="threads-approval-card" data-approval-id="${escapeAttr(approval.id)}">
      <div class="threads-message-meta">CTOX Freigabe · ${escapeHtml(approval.status || 'pending')} · ${escapeHtml(formatTime(approval.requested_at_ms || approval.created_at_ms))}</div>
      <div class="threads-message-body">${escapeHtml(approval.prompt || '')}</div>
      <div class="threads-message-meta">Requester: ${escapeHtml(approval.requester_display_name || approval.requester_user_id || '')} · Reviewer: ${escapeHtml(approval.reviewer_display_name || approval.reviewer_user_id || '')}</div>
      ${command ? `<div class="threads-message-meta">Command: ${escapeHtml(command.command_type || '')} · ${escapeHtml(command.status || '')}</div>` : ''}
      ${task ? `<div class="threads-message-meta">Task: ${escapeHtml(task.title || task.id)} · ${escapeHtml(task.status || '')}</div>` : ''}
      ${canDecide ? `
        <div class="threads-approval-actions">
          <button type="button" data-edit-approval="${escapeAttr(approval.id)}">Bearbeiten</button>
          <button type="button" data-approve-approval="${escapeAttr(approval.id)}">Freigeben</button>
          <button type="button" data-reject-approval="${escapeAttr(approval.id)}">Ablehnen</button>
        </div>
      ` : ''}
    </article>
  `;
}

function renderContext(thread) {
  const links = linksForThread(thread.id);
  const approvals = approvalsForThread(thread.id);
  const unread = unreadNotificationsForThread(thread.id, currentUserId());
  const rows = [
    { label: 'Quelle', value: contextLabel(thread) },
    { label: 'Record', value: [thread.source_record_type, thread.source_record_id].filter(Boolean).join(' / ') },
    { label: 'Deep Link', value: thread.source_deep_link, deepLink: thread.source_deep_link },
    { label: 'Teilnehmer', value: arrayField(thread.participant_ids).join(', ') },
    { label: 'Watcher', value: arrayField(thread.watcher_user_ids).join(', ') },
    { label: 'Ungelesen', value: String(unread.length) },
    { label: 'Snoozed bis', value: thread.snoozed_until_ms ? formatTime(thread.snoozed_until_ms) : '' },
    { label: 'Offene Freigaben', value: String(approvals.filter((item) => item.status === 'pending').length) },
    ...links.map((link) => ({
      label: link.link_type || link.link_role || 'Link',
      value: [link.app_collection, link.source_module, link.source_record_type, link.source_record_id, link.source_label].filter(Boolean).join(' / '),
      deepLink: link.deep_link,
    })),
  ].filter((row) => String(row.value || row.deepLink || '').trim());
  const rowHtml = rows.length
    ? rows.map(renderContextRow).join('')
    : '<div class="threads-empty">Kein verknüpfter App-Kontext.</div>';
  const notificationHtml = unread.length
    ? `<div class="threads-notification-list">${unread.map((item) => `
        <div class="threads-notification-item">
          <span>${escapeHtml(item.title || item.notification_type || 'Notification')}</span>
          <div>
            <button type="button" data-mark-notification="${escapeAttr(item.id)}">Gelesen</button>
            <button type="button" data-dismiss-notification="${escapeAttr(item.id)}">Ausblenden</button>
          </div>
        </div>
      `).join('')}</div>`
    : '';
  els.context.innerHTML = rowHtml + notificationHtml;
}

async function submitMessage() {
  const thread = state.data.threads.find((item) => item.id === state.selectedId);
  const body = String(els.messageBody?.value || '').trim();
  if (!thread || !body) return;
  await dispatchThreadsCommand('threads.message.create', {
    thread_id: thread.id,
    body,
  }, {
    recordId: thread.id,
    sourceModule: thread.source_module || 'threads',
  });
  els.messageBody.value = '';
  await refresh();
}

async function submitNote() {
  const body = String(els.noteBody?.value || '').trim();
  const targetUserIds = splitUserIds(els.noteTarget?.value || '');
  if (!body) return;
  const payload = buildNotePayload({
    body,
    targetUserIds,
    threadId: state.selectedId,
    sourceContext: selectedSourceContext(),
  });
  await dispatchThreadsCommand('threads.note.create', payload, {
    recordId: payload.thread_id,
    sourceModule: payload.source_context?.module || 'threads',
  });
  if (els.noteBody) els.noteBody.value = '';
  await refresh();
}

async function submitApprovalRequest() {
  const prompt = String(els.approvalPrompt?.value || '').trim();
  const reviewerUserId = String(els.approvalReviewer?.value || '').trim();
  if (!prompt || !reviewerUserId) return;
  const payload = buildApprovalRequestPayload({
    prompt,
    reviewerUserId,
    threadId: state.selectedId,
    sourceContext: selectedSourceContext(),
  });
  await dispatchThreadsCommand('threads.ctox_approval.request', payload, {
    recordId: payload.thread_id,
    sourceModule: payload.source_context?.module || 'threads',
  });
  if (els.approvalPrompt) els.approvalPrompt.value = '';
  await refresh();
}

async function decideApproval(approvalId, decision) {
  if (!approvalId) return;
  const note = decision === 'reject' ? window.prompt('Begründung oder Änderungswunsch:') || '' : '';
  await dispatchThreadsCommand(
    decision === 'approve' ? 'threads.ctox_approval.approve' : 'threads.ctox_approval.reject',
    {
      approval_request_id: approvalId,
      decision_note: note,
    },
    {
      recordId: approvalId,
      sourceModule: 'threads',
    },
  );
  await refresh();
}

async function editApproval(approvalId) {
  if (!approvalId) return;
  const approval = state.data.approvals.find((item) => item.id === approvalId);
  const prompt = window.prompt('Finaler CTOX Prompt:', approval?.prompt || '');
  if (!prompt || !prompt.trim()) return;
  await dispatchThreadsCommand('threads.ctox_approval.edit', {
    approval_request_id: approvalId,
    prompt: prompt.trim(),
    instruction: prompt.trim(),
  }, {
    recordId: approvalId,
    sourceModule: 'threads',
  });
  await refresh();
}

async function toggleWatch() {
  const thread = selectedThread();
  if (!thread) return;
  const watching = arrayField(thread.watcher_user_ids).includes(currentUserId());
  await dispatchThreadsCommand(watching ? 'threads.thread.unwatch' : 'threads.thread.watch', {
    thread_id: thread.id,
  }, {
    recordId: thread.id,
    sourceModule: 'threads',
  });
  await refresh();
}

async function snoozeSelectedThread() {
  const thread = selectedThread();
  if (!thread) return;
  const hours = Number(window.prompt('Snooze in Stunden:', '24') || 0);
  if (!Number.isFinite(hours) || hours <= 0) return;
  await dispatchThreadsCommand('threads.thread.snooze', {
    thread_id: thread.id,
    snoozed_until_ms: Date.now() + Math.round(hours * 60 * 60 * 1000),
  }, {
    recordId: thread.id,
    sourceModule: 'threads',
  });
  await refresh();
}

async function archiveSelectedThread() {
  const thread = selectedThread();
  if (!thread) return;
  await dispatchThreadsCommand('threads.thread.archive', {
    thread_id: thread.id,
  }, {
    recordId: thread.id,
    sourceModule: 'threads',
  });
  await refresh();
}

async function updateNotification(notificationId, action) {
  if (!notificationId) return;
  await dispatchThreadsCommand(
    action === 'dismiss' ? 'threads.notification.dismiss' : 'threads.notification.mark_read',
    { notification_id: notificationId },
    {
      recordId: notificationId,
      sourceModule: 'threads',
    },
  );
  await refresh();
}

async function dispatchThreadsCommand(commandType, payload, { recordId = '', sourceModule = 'threads' } = {}) {
  if (!state.ctx?.commandBus?.dispatch) throw new Error('Command bus unavailable.');
  setBusy(true);
  try {
    const command = buildThreadsCommand({
      commandType,
      payload,
      recordId,
      sourceModule,
      actor: actorContext(),
      clientContext: {
        visible_scope: {
          app: { module_id: 'threads', app_id: 'threads' },
          source: payload?.source_context || selectedSourceContext(),
        },
      },
    });
    const outcome = await state.ctx.commandBus.dispatch(command);
    state.status = outcome?.status || 'completed';
    return outcome;
  } finally {
    setBusy(false);
  }
}

function setBusy(busy) {
  state.busy = busy;
  [els.noteForm, els.approvalForm, els.messageForm].forEach((form) => {
    form?.querySelectorAll?.('button, input, textarea').forEach((control) => {
      if (control === els.messageBody && !state.selectedId) {
        control.disabled = true;
      } else {
        control.disabled = busy;
      }
    });
  });
  setThreadActionState(selectedThread());
  if (els.status) els.status.textContent = busy ? 'sendet' : (state.status || 'bereit');
}

function showError(error) {
  console.warn('[threads] action failed', error);
  state.status = state.t('commandFailed', 'Aktion konnte nicht abgeschlossen werden.');
  if (els.status) els.status.textContent = state.status;
}

function selectedThread() {
  return state.data.threads.find((item) => item.id === state.selectedId) || null;
}

function setThreadActionState(thread) {
  const disabled = !thread || state.busy;
  [els.watch, els.snooze, els.archive].forEach((button) => {
    if (button) button.disabled = disabled;
  });
  if (els.watch && thread) {
    els.watch.textContent = arrayField(thread.watcher_user_ids).includes(currentUserId()) ? 'Unwatch' : 'Watch';
  }
}

function selectedSourceContext() {
  const thread = state.data.threads.find((item) => item.id === state.selectedId);
  if (!thread) return { module: 'threads', label: 'Threads' };
  const link = linksForThread(thread.id).find((item) => item.deep_link);
  return {
    module: thread.source_module || 'threads',
    record_type: thread.source_record_type || 'thread',
    record_id: thread.source_record_id || thread.id,
    label: thread.source_label || thread.title || thread.id,
    deep_link: thread.source_deep_link || link?.deep_link || '',
  };
}

function renderContextRow(row) {
  const label = row?.label || '';
  const value = row?.value || row?.deepLink || '';
  const deepLink = String(row?.deepLink || '').trim();
  const valueHtml = deepLink
    ? `<a href="${escapeAttr(deepLink)}" data-thread-deep-link="${escapeAttr(deepLink)}">${escapeHtml(value || deepLink)}</a>`
    : `<span>${escapeHtml(value)}</span>`;
  return `<div class="threads-context-row"><strong>${escapeHtml(label)}</strong>${valueHtml}</div>`;
}

function navigateDeepLink(value) {
  const link = String(value || '').trim();
  if (!link) return;
  if (link.startsWith('#')) {
    window.location.hash = link;
  } else if (link.startsWith('/') || link.startsWith('?')) {
    window.location.assign(link);
  }
}

function messagesForThread(threadId) {
  return state.data.messages
    .filter((item) => item.thread_id === threadId)
    .sort((left, right) => Number(left.created_at_ms || left.updated_at_ms || 0) - Number(right.created_at_ms || right.updated_at_ms || 0));
}

function linksForThread(threadId) {
  return state.data.links.filter((item) => item.thread_id === threadId);
}

function approvalsForThread(threadId) {
  return state.data.approvals.filter((item) => item.thread_id === threadId);
}

function notificationsForThread(threadId) {
  return state.data.notifications.filter((item) => item.thread_id === threadId);
}

function unreadNotificationsForThread(threadId, userId) {
  return notificationsForThread(threadId)
    .filter((item) => (!userId || item.user_id === userId) && item.status === 'unread');
}

function threadRelevantToUser(thread, userId) {
  if (!userId) return true;
  return arrayField(thread.participant_ids).includes(userId)
    || arrayField(thread.watcher_user_ids).includes(userId)
    || notificationsForThread(thread.id).some((item) => item.user_id === userId && item.status !== 'dismissed')
    || approvalsForThread(thread.id).some((item) => item.reviewer_user_id === userId || item.requester_user_id === userId)
    || messagesForThread(thread.id).some((item) => item.author_user_id === userId || arrayField(item.target_user_ids).includes(userId));
}

function threadMentionsUser(threadId, userId) {
  if (!userId) return false;
  return messagesForThread(threadId).some((item) => arrayField(item.target_user_ids).includes(userId) || item.kind === 'mention')
    || notificationsForThread(threadId).some((item) => item.user_id === userId && ['mention', 'mentioned'].includes(item.notification_type || item.reason));
}

function threadWaitingOnUser(threadId, userId, isAdmin) {
  return approvalsForThread(threadId).some((item) => item.status === 'pending' && (isAdmin || !userId || item.reviewer_user_id === userId))
    || notificationsForThread(threadId).some((item) => (!userId || item.user_id === userId) && ['waiting_on_user', 'approval_request', 'approval_requested'].includes(item.notification_type || item.reason));
}

function threadDelegatedByUser(thread, userId) {
  if (!userId) return false;
  return thread.created_by_id === userId
    || messagesForThread(thread.id).some((item) => item.author_user_id === userId)
    || approvalsForThread(thread.id).some((item) => item.requester_user_id === userId);
}

function threadHasRunningCtox(threadId) {
  return linkedCommandsForThread(threadId).some((item) => statusIn(item.status || item.task_status, ['pending_sync', 'queued', 'running', 'in_progress']))
    || linkedTasksForThread(threadId).some((item) => statusIn(item.status, ['queued', 'running', 'in_progress']));
}

function threadHasFailedCtox(threadId) {
  return linkedCommandsForThread(threadId).some((item) => statusIn(item.status || item.task_status, ['failed', 'blocked', 'error']))
    || linkedTasksForThread(threadId).some((item) => statusIn(item.status, ['failed', 'blocked', 'error']));
}

function linkedCommandsForThread(threadId) {
  const ids = new Set([
    ...messagesForThread(threadId).map((item) => item.command_id),
    ...linksForThread(threadId).map((item) => item.command_id),
    ...approvalsForThread(threadId).map((item) => item.approved_command_id),
  ].map((id) => String(id || '').trim()).filter(Boolean));
  return state.data.commands.filter((item) => ids.has(item.id) || ids.has(item.command_id));
}

function linkedTasksForThread(threadId) {
  const ids = new Set([
    ...linksForThread(threadId).map((item) => item.task_id),
    ...approvalsForThread(threadId).map((item) => item.approved_task_id),
    ...linkedCommandsForThread(threadId).map((item) => item.task_id),
  ].map((id) => String(id || '').trim()).filter(Boolean));
  return state.data.queue.filter((item) => ids.has(item.id) || ids.has(item.task_id));
}

function linkedCommandIds(data = state.data) {
  return [
    ...data.messages.map((item) => item.command_id),
    ...data.links.map((item) => item.command_id),
    ...data.approvals.map((item) => item.approved_command_id),
  ];
}

function linkedTaskIds(data = state.data, commands = state.data.commands) {
  return [
    ...data.links.map((item) => item.task_id),
    ...data.approvals.map((item) => item.approved_task_id),
    ...commands.map((item) => item.task_id),
  ];
}

function isSnoozed(thread) {
  const until = Number(thread.snoozed_until_ms || 0);
  return thread.status === 'snoozed' && until > Date.now();
}

function statusIn(status, values) {
  const clean = String(status || '').trim().toLowerCase();
  return values.includes(clean);
}

function commandById(commandId) {
  return state.data.commands.find((item) => item.id === commandId || item.command_id === commandId) || null;
}

function taskById(taskId) {
  return state.data.queue.find((item) => item.id === taskId || item.task_id === taskId) || null;
}

function contextLabel(thread) {
  return [thread.source_module, thread.source_label || thread.source_record_id].filter(Boolean).join(' · ') || 'Threads';
}

function currentUserId() {
  return String(state.ctx?.session?.user?.id || '').trim();
}

function currentUserRole() {
  return String(state.ctx?.session?.user?.role || (state.ctx?.session?.user?.is_admin ? 'admin' : 'user') || 'user').trim();
}

function actorContext() {
  const user = state.ctx?.session?.user || {};
  const id = String(user.id || '').trim();
  if (!id) return null;
  return {
    id,
    display_name: user.display_name || user.name || id,
    role: user.role || (user.is_admin ? 'admin' : 'user'),
    is_admin: Boolean(user.is_admin),
  };
}

function arrayField(value) {
  return Array.isArray(value) ? value.map((item) => String(item || '').trim()).filter(Boolean) : [];
}

function formatTime(value) {
  const ms = Number(value || 0);
  if (!Number.isFinite(ms) || ms <= 0) return '';
  try {
    return new Intl.DateTimeFormat(document.documentElement.lang === 'en' ? 'en' : 'de', {
      day: '2-digit',
      month: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
    }).format(new Date(ms));
  } catch {
    return new Date(ms).toLocaleString();
  }
}

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function escapeAttr(value) {
  return escapeHtml(value).replace(/`/g, '&#96;');
}
