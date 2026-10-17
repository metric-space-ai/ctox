// App-API 4.0 (Shell v4): mount(root, api, {restored}) -> {unmount, snapshot}. Port-Protokoll: V4-PORT.md.
// Daten nur über api.query/api.get (Live-Abos, lokal und vollständig); Befehle über api.command ohne Warten;
// Zustände nur in die Zustandszone (api.status). Fachlogik liegt in handlers/ (V4-HANDLERS.md).
import { renderHtmlIfChanged } from 'ctox:shared/stable-dom.js';
import { autoWirePaneGrammar } from 'ctox:shared/pane-grammar.js';
import { normalizeInternalDeepLink, sourceDeepLinkFor as buildSourceDeepLink, sourceFocusSupported } from './links.js';
import { THREAD_COLLECTIONS, buildApprovalRequestPayload, buildNotePayload, splitUserIds } from './commands.js';
import { isIdentifier, namingLookupIds, threadName } from './naming.js';

const APP_ID = 'threads';

// Eingebettete Texte (Rückfall, solange der Schlüsselraum von api.shell.t offen ist, app-api.d.ts OFFEN-B).
// Keine Zustandstexte (R11): Laden/Synchronisieren/Leer-Hinweise gibt es nicht mehr.
const labels = {
  de: {
    search: 'Threads suchen',
    commandFailed: 'Aktion konnte nicht abgeschlossen werden.',
    viewShowList: 'Als Liste anzeigen',
    viewShowCards: 'Als Karten anzeigen',
    statusOpen: 'Offen',
    statusBlocked: 'Blockiert',
    statusEscalated: 'Eskaliert',
    statusSnoozed: 'Später',
    statusArchived: 'Erledigt',
    statusRunning: 'AI arbeitet',
    statusCompleted: 'Abgeschlossen',
    workFailed: 'Fehlgeschlagen',
    workRunning: 'In Arbeit',
    workQueued: 'Eingereiht',
    workDone: 'Abgeschlossen',
    workBlocked: 'Blockiert',
    metaDue: 'Frist',
    filterApprovals: 'Freigaben',
    filterDelegated: 'Delegiert',
    approvalPanelTitle: 'CTOX Freigabe',
    approvalReviewerLabel: 'Reviewer',
    approvalReviewerPlaceholder: 'erfahrener-user-id',
    approvalPromptLabel: 'Prompt',
    approvalPromptPlaceholder: 'Was CTOX nach Freigabe tun soll',
    approvalRequestSubmit: 'Freigabe anfragen',
    approvalEdit: 'Bearbeiten',
    approvalApprove: 'Freigeben',
    approvalReject: 'Ablehnen',
    approvalRequester: 'Anfragende:r',
    approvalReviewerShort: 'Reviewer',
    approvalCommand: 'Befehl',
    approvalTask: 'Aufgabe',
    approvalHeadingPrefix: 'CTOX Freigabe',
    nameSupervisor: 'Supervisor',
    nameCtoxWork: 'CTOX Arbeit',
    detailId: 'Kennung',
  },
  en: {
    search: 'Search threads',
    commandFailed: 'Action could not be completed.',
    viewShowList: 'Show as list',
    viewShowCards: 'Show as cards',
    statusOpen: 'Open',
    statusBlocked: 'Blocked',
    statusEscalated: 'Escalated',
    statusSnoozed: 'Snoozed',
    statusArchived: 'Done',
    statusRunning: 'AI working',
    statusCompleted: 'Completed',
    workFailed: 'Failed',
    workRunning: 'Running',
    workQueued: 'Queued',
    workDone: 'Completed',
    workBlocked: 'Blocked',
    metaDue: 'Due',
    filterApprovals: 'Approvals',
    filterDelegated: 'Delegated',
    approvalPanelTitle: 'CTOX approval',
    approvalReviewerLabel: 'Reviewer',
    approvalReviewerPlaceholder: 'experienced-user-id',
    approvalPromptLabel: 'Prompt',
    approvalPromptPlaceholder: 'What CTOX should do after approval',
    approvalRequestSubmit: 'Request approval',
    approvalEdit: 'Edit',
    approvalApprove: 'Approve',
    approvalReject: 'Reject',
    approvalRequester: 'Requester',
    approvalReviewerShort: 'Reviewer',
    approvalCommand: 'Command',
    approvalTask: 'Task',
    approvalHeadingPrefix: 'CTOX approval',
    nameSupervisor: 'Supervisor',
    nameCtoxWork: 'CTOX work',
    detailId: 'Identifier',
  },
};

const PRIMARY_FILTERS = ['inbox', 'waiting', 'running', 'archived'];
const SECONDARY_FILTERS = ['delegated', 'snoozed', 'team', 'mentions', 'approvals', 'failed', 'all', 'system'];
const THREAD_FILTERS = [...PRIMARY_FILTERS, ...SECONDARY_FILTERS];

// Modulzustand: je mount() neu gesetzt; die Abos gehören der Montage (lokal festgehalten, PORTING §4 Falle 1).
const state = {
  api: null,
  root: null,
  lang: 'de',
  filter: 'inbox',
  search: '',
  listView: false,
  centerListView: false,
  selectedId: '',
  mobileView: 'list',
  requestedThreadId: '',
  data: emptyData(),
  roster: [],
  apps: new Map(),
  projects: new Map(),
  commandsById: new Map(),
  tasksById: new Map(),
  personalStateByThread: new Map(),
  pendingRestore: null,
  renderTimer: null,
};
let els = {};
let mountSequence = 0;

function emptyData() {
  return { threads: [], messages: [], links: [], notifications: [], approvals: [], commands: [], queue: [], states: [] };
}

export async function mount(root, api, { restored } = {}) {
  const sequence = ++mountSequence;
  const metricPrefix = `ctox.threads.${sequence}`;
  performance.mark(`${metricPrefix}.mount`);
  const disposers = [];
  const detail = { threadId: null, stops: [] };
  const lookups = { commands: { key: null, stop: null }, tasks: { key: null, stop: null } };
  Object.assign(state, {
    api, root, filter: 'inbox', search: '', listView: false, centerListView: false, selectedId: '', mobileView: 'list',
    requestedThreadId: '', data: emptyData(), roster: [], apps: new Map(), projects: new Map(), commandsById: new Map(),
    tasksById: new Map(), personalStateByThread: new Map(), renderTimer: null, metrics: { prefix: metricPrefix, recorded: new Set() },
    lang: langFrom(api.shell.locale.get()), detail, lookups, alive: () => mountSequence === sequence,
    pendingRestore: isPlainObject(restored) ? restored : null,
  });
  if (!root.dataset.app) root.dataset.app = APP_ID;
  root.innerHTML = await loadModuleMarkup();
  bindElements(root);
  autoWirePaneGrammar(root);
  api.shell.setTitle('Threads');
  applyRestoredView(state.pendingRestore);
  applyLabels();
  wireUi(disposers);
  try { api.setVisible(THREAD_COLLECTIONS); } catch {}

  const me = currentUserId();
  const kicker = els.root?.querySelector('[data-label="hubKicker"]');
  if (kicker && api.actor.display_name) kicker.textContent = `Inbox · ${api.actor.display_name}`;

  const live = (collection, query, onRows) => {
    try {
      disposers.push(api.query(collection, query).subscribe((rows) => {
        if (!state.alive()) return;
        onRows(rows.filter((row) => row && row.is_deleted !== true));
        afterData();
      }));
    } catch (error) {
      // Fremd-Collection ohne Freigabe im Manifest (forbidden) o. ä.: Name fällt auf die nächste Stufe zurück.
      if (error?.code !== 'forbidden') reportError(`threads.query.${collection}`, error);
    }
  };
  live('user_threads', { sort: [{ updated_at_ms: 'desc' }] }, (rows) => { state.data.threads = rows; });
  live('user_thread_states', {
    selector: { user_id: me, $or: [{ attention_score: { $gt: 0 } }, { thread_id: '__preferences__' }] },
    sort: [{ updated_at_ms: 'desc' }],
  }, (rows) => {
    state.data.states = rows;
    state.personalStateByThread = new Map(rows.map((item) => [item.thread_id, item]));
  });
  live('ctox_task_approval_requests', { sort: [{ updated_at_ms: 'desc' }] }, (rows) => { state.data.approvals = rows; });
  live('user_notifications', { selector: { user_id: me }, sort: [{ updated_at_ms: 'desc' }] }, (rows) => { state.data.notifications = rows; });
  live('business_users', {}, (rows) => {
    state.roster = rows.filter((user) => user.id || user.user_id)
      .map((user) => ({ id: user.id || user.user_id, name: user.display_name || user.name || user.id || user.user_id }));
    if (me && !state.roster.some((user) => user.id === me)) state.roster.push({ id: me, name: api.actor.display_name || me });
    renderRosterSelects();
  });
  live('business_apps', {}, (rows) => { state.apps = new Map(rows.map((app) => [app.id, app])); });
  // Projektnamen (workjet#594): Fremd-Collection der ctox-App; ohne Freigabe nur Rückfallstufen (V4-PORT.md OFFEN-2).
  live('workjet_projects', {}, (rows) => { state.projects = new Map(rows.map((project) => [project.id, project])); });

  disposers.push(api.shell.locale.subscribe((locale) => {
    const lang = langFrom(locale);
    if (lang === state.lang) return;
    state.lang = lang;
    applyLabels();
    scheduleRender();
  }));
  disposers.push(api.shell.onRoute((event) => {
    const params = event?.params || {};
    const threadId = String(params.thread_id || params.thread || '').trim();
    if (!threadId) return;
    state.requestedThreadId = threadId;
    state.mobileView = 'detail';
    syncSelection();
    scheduleRender();
  }));

  syncSelection();
  render();

  return {
    unmount() {
      if (state.alive() && state.renderTimer) clearTimeout(state.renderTimer);
      for (const stop of disposers) { try { stop?.(); } catch {} }
      disposers.length = 0;
      for (const stop of detail.stops) { try { stop?.(); } catch {} }
      detail.stops = [];
      for (const lookup of Object.values(lookups)) { try { lookup.stop?.(); } catch {} lookup.stop = null; }
      if (state.alive()) mountSequence += 1;
      root.replaceChildren();
    },
    snapshot,
  };
}

function langFrom(locale) {
  return String(locale || '').toLowerCase().startsWith('en') ? 'en' : 'de';
}

function t(key, fallback) {
  const full = `${APP_ID}.${key}`;
  try {
    const value = state.api?.shell.t(full);
    if (value && value !== full) return value;
  } catch {}
  return labels[state.lang]?.[key] ?? labels.de[key] ?? fallback ?? key;
}

function isPlainObject(value) {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

// ---- snapshot / restored (PORTING §2) ------------------------------------------------------------------------
const SNAPSHOT_VIEW_KEYS = { filter: 'string', search: 'string', listView: 'boolean', centerListView: 'boolean', selectedId: 'string', mobileView: 'string' };
const SNAPSHOT_DRAFTS = ['messageBody', 'noteBody', 'noteTarget', 'approvalPrompt', 'approvalReviewer', 'handoffTarget', 'handoffBody', 'handoffDue', 'handoffReturnReason', 'aiGoal'];

function snapshot() {
  const snap = { v: 1 };
  for (const key of Object.keys(SNAPSHOT_VIEW_KEYS)) snap[key] = state[key];
  snap.drafts = {};
  for (const key of SNAPSHOT_DRAFTS) {
    const value = els[key]?.value;
    if (value) snap.drafts[key] = String(value);
  }
  snap.actionsHidden = els.root?.classList.contains('is-actions-hidden') ?? true;
  snap.contextOpen = els.root?.classList.contains('is-context-open') ?? false;
  snap.openPanel = els.root?.querySelector('[data-action-panel][open]')?.dataset.actionPanel || '';
  snap.listScrollTop = els.list?.scrollTop || 0;
  return snap;
}

function applyRestoredView(restored) {
  if (!restored) return;
  for (const [key, type] of Object.entries(SNAPSHOT_VIEW_KEYS)) {
    if (typeof restored[key] === type) state[key] = restored[key];
  }
  if (!THREAD_FILTERS.includes(state.filter) || (state.filter === 'team' && !isAdminRole())) state.filter = 'inbox';
  if (state.mobileView !== 'detail') state.mobileView = 'list';
  if (state.selectedId) state.requestedThreadId = state.selectedId;
  if (els.search) els.search.value = state.search;
  for (const button of [els.viewToggle, els.centerViewToggle]) {
    if (!button) continue;
    const view = (button === els.viewToggle ? state.listView : state.centerListView) ? 'list' : 'cards';
    button.dataset.pgView = view;
  }
  els.root?.classList.toggle('is-list-view', state.listView);
  els.root?.classList.toggle('is-center-list-view', state.centerListView);
  if (typeof restored.actionsHidden === 'boolean') els.root?.classList.toggle('is-actions-hidden', restored.actionsHidden);
  if (typeof restored.contextOpen === 'boolean') els.root?.classList.toggle('is-context-open', restored.contextOpen);
  if (typeof restored.openPanel === 'string' && restored.openPanel) {
    const panel = els.root?.querySelector(`[data-action-panel="${CSS.escape(restored.openPanel)}"]`);
    if (panel) panel.open = true;
  }
  // Entwürfe sofort zurückspielen; Auswahl-Selects (Roster) erst, wenn die Personen da sind (renderRosterSelects).
  const drafts = isPlainObject(restored.drafts) ? restored.drafts : {};
  for (const key of SNAPSHOT_DRAFTS) {
    if (typeof drafts[key] === 'string' && els[key] && els[key].tagName !== 'SELECT') els[key].value = drafts[key];
  }
  state.restoredSelects = Object.fromEntries(SNAPSHOT_DRAFTS.filter((key) => els[key]?.tagName === 'SELECT' && typeof drafts[key] === 'string').map((key) => [key, drafts[key]]));
  state.restoredScrollTop = Number(restored.listScrollTop) || 0;
}

// ---- Markup ---------------------------------------------------------------------------------------------------
async function loadModuleMarkup() {
  // Relativ und ohne Cache-Buster: der Service Worker liefert die Artefaktdatei versioniert (§16.15). CSS lädt die Shell.
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}

function bindElements(root) {
  els = {};
  els.root = root.querySelector('[data-threads-root]');
  els.leftPane = root.querySelector('.threads-left');
  els.centerPane = root.querySelector('.threads-center');
  els.viewToggle = root.querySelector('.threads-left [data-threads-view-toggle]');
  els.centerViewToggle = root.querySelector('.threads-center [data-threads-view-toggle]');
  els.search = root.querySelector('[data-pg-search]');
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
  els.delegate = root.querySelector('[data-thread-delegate]');
  els.toggleActions = root.querySelector('[data-toggle-actions]');
  els.mobileBack = root.querySelector('[data-mobile-back]');
  els.mobileReply = root.querySelector('[data-mobile-reply]');
  els.mobileSnooze = root.querySelector('[data-mobile-snooze]');
  els.mobileMore = root.querySelector('[data-mobile-more]');
  els.claimButtons = [...root.querySelectorAll('[data-thread-claim]')];
  els.handoffForm = root.querySelector('[data-handoff-form]');
  els.handoffTarget = root.querySelector('[data-handoff-target]');
  els.handoffBody = root.querySelector('[data-handoff-body]');
  els.handoffDue = root.querySelector('[data-handoff-due]');
  els.handoffReturnReason = root.querySelector('[data-handoff-return-reason]');
  els.aiForm = root.querySelector('[data-ai-form]');
  els.aiGoal = root.querySelector('[data-ai-goal]');
  els.notificationPreferencesForm = root.querySelector('[data-notification-preferences-form]');
  els.notificationThreshold = root.querySelector('[data-notification-threshold]');
  els.quietStart = root.querySelector('[data-quiet-start]');
  els.quietEnd = root.querySelector('[data-quiet-end]');
  els.notificationApprovals = root.querySelector('[data-notification-approvals]');
  els.notificationMentions = root.querySelector('[data-notification-mentions]');
  els.notificationEscalations = root.querySelector('[data-notification-escalations]');
}

function applyLabels() {
  if (els.search) {
    els.search.placeholder = t('search', 'Threads suchen');
    els.search.setAttribute('aria-label', t('search', 'Threads suchen'));
  }
  syncViewToggle(els.viewToggle);
  syncViewToggle(els.centerViewToggle);
  const setFilterText = (filter, key, fb) => {
    const btn = els.root?.querySelector(`[data-pg-band="${filter}"]`);
    if (btn) btn.textContent = t(key, fb);
  };
  setFilterText('approvals', 'filterApprovals', 'Freigaben');
  setFilterText('delegated', 'filterDelegated', 'Delegiert');
  const teamOption = els.root?.querySelector('[data-pg-filter][data-pg-name="view"] option[value="team"]');
  if (teamOption) {
    const allowed = isAdminRole();
    teamOption.hidden = !allowed;
    teamOption.disabled = !allowed;
  }
  if (els.approvalForm) {
    const heading = els.approvalForm.closest('.threads-panel')?.querySelector('h3');
    if (heading) heading.textContent = t('approvalPanelTitle', 'CTOX Freigabe');
    const labelSpans = els.approvalForm.querySelectorAll('label > span');
    if (labelSpans[0]) labelSpans[0].textContent = t('approvalReviewerLabel', 'Reviewer');
    if (labelSpans[1]) labelSpans[1].textContent = t('approvalPromptLabel', 'Prompt');
    if (els.approvalReviewer) els.approvalReviewer.placeholder = t('approvalReviewerPlaceholder', 'erfahrener-user-id');
    if (els.approvalPrompt) els.approvalPrompt.placeholder = t('approvalPromptPlaceholder', 'Was CTOX nach Freigabe tun soll');
    const submitBtn = els.approvalForm.querySelector('button[type="submit"]');
    if (submitBtn) submitBtn.textContent = t('approvalRequestSubmit', 'Freigabe anfragen');
  }
}

function on(target, type, handler, disposers) {
  if (!target) return;
  target.addEventListener(type, handler);
  disposers.push(() => target.removeEventListener(type, handler));
}

function wireUi(disposers) {
  // Pane-Grammatik (autoWirePaneGrammar) meldet Suche/Ansicht/Filter über das blubbernde Grammatik-Ereignis.
  on(els.leftPane, 'ctox-pane-grammar-change', onLeftGrammarChange, disposers);
  on(els.centerPane, 'ctox-pane-grammar-change', onCenterGrammarChange, disposers);
  // Ein Knopf, kein Zustandspaar (Betreiber-Vorgabe 31.08.): der Klick setzt die NEUE Ansicht, bevor die
  // Grammatik meldet (Listener früher registriert); ohne verdrahtete Grammatik wendet die App ihn selbst an.
  for (const button of [els.viewToggle, els.centerViewToggle]) {
    on(button, 'click', () => {
      const next = button.dataset.pgView === 'list' ? 'cards' : 'list';
      button.dataset.pgView = next;
      syncViewToggle(button);
      if (button.closest('.ctox-pane')?.dataset.pgWired === 'true') return;
      if (button === els.centerViewToggle) onCenterGrammarChange({ detail: { view: next } });
      else onLeftGrammarChange({ detail: { view: next, search: state.search, filters: {} } });
    }, disposers);
  }
  els.leftPane?.querySelectorAll('[data-pg-band]').forEach((tab) => {
    on(tab, 'click', () => {
      const select = els.leftPane?.querySelector('[data-pg-filter][data-pg-name="view"]');
      if (select && select.value !== '') select.value = '';
    }, disposers);
  });
  on(els.root?.querySelector('[data-action="create-note"]'), 'click', () => openActionPanel('note', els.noteBody), disposers);
  on(els.root?.querySelector('[data-action="export-threads"]'), 'click', exportVisibleThreads, disposers);
  on(els.list, 'click', (event) => {
    const row = event.target instanceof Element ? event.target.closest('[data-thread-id]') : null;
    if (!row) return;
    state.selectedId = row.getAttribute('data-thread-id') || '';
    state.mobileView = 'detail';
    // Auswahl ist ein Klassenwechsel an Ort und Stelle, nie ein Neuaufbau der Liste (Scrollposition bleibt).
    applyThreadSelection();
    renderMobileState();
    syncDetailSubscription();
    renderDetail();
  }, disposers);
  on(els.context, 'click', (event) => {
    const target = event.target instanceof Element ? event.target : null;
    const deepLink = target?.closest?.('[data-thread-deep-link]');
    const mark = target?.closest?.('[data-mark-notification]');
    const dismiss = target?.closest?.('[data-dismiss-notification]');
    if (deepLink) {
      event.preventDefault();
      navigateDeepLink(deepLink.getAttribute('data-thread-deep-link') || deepLink.getAttribute('href') || '');
    } else if (mark) {
      updateNotification(mark.getAttribute('data-mark-notification') || '', 'mark_read');
    } else if (dismiss) {
      updateNotification(dismiss.getAttribute('data-dismiss-notification') || '', 'dismiss');
    }
  }, disposers);
  on(els.timeline, 'click', (event) => {
    const target = event.target instanceof Element ? event.target : null;
    const deepLink = target?.closest?.('[data-thread-deep-link]');
    if (deepLink) {
      event.preventDefault();
      navigateDeepLink(deepLink.getAttribute('data-thread-deep-link') || '');
      return;
    }
    const rework = target?.closest?.('[data-rework-context]');
    if (rework) { requestRework(rework.getAttribute('data-rework-context') || '').catch(showError); return; }
    const question = target?.closest?.('[data-question-approval]');
    if (question) { askApprovalQuestion(question.getAttribute('data-question-approval') || '').catch(showError); return; }
    const approve = target?.closest?.('[data-approve-approval]');
    const reject = target?.closest?.('[data-reject-approval]');
    const edit = target?.closest?.('[data-edit-approval]');
    if (approve) decideApproval(approve.getAttribute('data-approve-approval') || '', 'approve').catch(showError);
    else if (reject) decideApproval(reject.getAttribute('data-reject-approval') || '', 'reject').catch(showError);
    else if (edit) editApproval(edit.getAttribute('data-edit-approval') || '').catch(showError);
  }, disposers);
  on(els.source, 'click', () => navigateDeepLink(els.source.dataset.threadDeepLink || ''), disposers);
  on(els.delegate, 'click', () => openActionPanel('handoff', els.handoffTarget), disposers);
  wireMentionPopup(disposers);
  on(els.watch, 'click', toggleWatch, disposers);
  on(els.snooze, 'click', () => snoozeSelectedThread().catch(showError), disposers);
  on(els.archive, 'click', archiveSelectedThread, disposers);
  on(els.toggleActions, 'click', () => {
    const hidden = els.root?.classList.toggle('is-actions-hidden');
    els.toggleActions.setAttribute('aria-pressed', hidden ? 'false' : 'true');
  }, disposers);
  on(els.mobileBack, 'click', () => {
    state.mobileView = 'list';
    render();
  }, disposers);
  on(els.mobileReply, 'click', () => els.messageBody?.focus(), disposers);
  on(els.mobileSnooze, 'click', () => snoozeSelectedThread().catch(showError), disposers);
  on(els.mobileMore, 'click', () => els.root?.classList.toggle('is-context-open'), disposers);
  els.claimButtons.forEach((button) => on(button, 'click', claimSelectedThread, disposers));
  const submit = (form, fn) => on(form, 'submit', (event) => { event.preventDefault(); fn(); }, disposers);
  submit(els.messageForm, submitMessage);
  submit(els.noteForm, submitNote);
  submit(els.approvalForm, submitApprovalRequest);
  submit(els.handoffForm, submitHandoff);
  submit(els.aiForm, submitAiRequest);
  submit(els.notificationPreferencesForm, submitNotificationPreferences);
}

function openActionPanel(name, focusTarget) {
  els.root?.classList.remove('is-actions-hidden');
  els.root?.classList.add('is-context-open');
  els.toggleActions?.setAttribute('aria-pressed', 'true');
  const panel = els.root?.querySelector(`[data-action-panel="${name}"]`);
  if (panel) panel.open = true;
  focusTarget?.focus();
  focusTarget?.scrollIntoView?.({ block: 'center', behavior: 'smooth' });
}

// ---- Live-Daten -----------------------------------------------------------------------------------------------
// Jede Emission eines Abos landet hier: Nachschlage-Abos (Namen) und Detail-Abos nachziehen, dann gebündelt rendern.
function afterData() {
  syncLookupSubscriptions();
  syncSelection();
  syncDetailSubscription();
  scheduleRender();
}

function syncLookup(kind, collection, ids, onRows) {
  const lookup = state.lookups[kind];
  const key = ids.join('\n');
  if (lookup.key === key) return;
  lookup.key = key;
  try { lookup.stop?.(); } catch {}
  lookup.stop = null;
  // subscribe ruft sofort zurück; ein dabei verschachtelt nachgezogenes Abo (neue IDs) löst dieses ab – die
  // Generation verhindert, dass die äußere Zuweisung das innere Abo verwaist.
  const generation = (lookup.generation || 0) + 1;
  lookup.generation = generation;
  if (!ids.length) { onRows([]); return; }
  try {
    const stop = state.api.query(collection, { selector: { id: { $in: ids } } }).subscribe((rows) => {
      if (!state.alive() || lookup.generation !== generation) return;
      onRows(rows);
      afterData();
    });
    if (lookup.generation === generation) lookup.stop = stop;
    else stop();
  } catch (error) {
    reportError(`threads.query.${collection}`, error);
  }
}

// Befehle und Aufgaben: die der Thread-Namen (workjet#594) plus die mit dem ausgewählten Thread verknüpften.
function syncLookupSubscriptions() {
  const naming = namingLookupIds(state.data.threads, [...state.commandsById.values()], [...state.tasksById.values()]);
  const commandIds = new Set(naming.commandIds);
  const taskIds = new Set(naming.taskIds);
  const selected = state.data.threads.find((item) => item.id === state.selectedId);
  if (selected) {
    const base = selectedBase(selected.id);
    linkedCommandIds(base).forEach((id) => id && commandIds.add(String(id).trim()));
    linkedTaskIds(base, [...state.commandsById.values()].filter((cmd) => linkedCommandIds(base).includes(cmd.id) || linkedCommandIds(base).includes(cmd.command_id)))
      .forEach((id) => id && taskIds.add(String(id).trim()));
  }
  syncLookup('commands', 'business_commands', [...commandIds].filter(Boolean).sort(), (rows) => {
    state.commandsById = new Map(rows.map((row) => [row.id, row]));
  });
  syncLookup('tasks', 'ctox_queue_tasks', [...taskIds].filter(Boolean).sort(), (rows) => {
    state.tasksById = new Map(rows.map((row) => [row.id, row]));
  });
  // Detaildaten (Befehle/Aufgaben des ausgewählten Threads) – Umfang wie vor dem Port: nur der ausgewählte Thread.
  if (selected) {
    const base = selectedBase(selected.id);
    const cmdIds = new Set(linkedCommandIds(base).map((id) => String(id || '').trim()).filter(Boolean));
    state.data.commands = [...state.commandsById.values()].filter((item) => cmdIds.has(item.id) || cmdIds.has(item.command_id));
    const tIds = new Set(linkedTaskIds(base, state.data.commands).map((id) => String(id || '').trim()).filter(Boolean));
    state.data.queue = [...state.tasksById.values()].filter((item) => tIds.has(item.id) || tIds.has(item.task_id));
  } else {
    state.data.commands = [];
    state.data.queue = [];
  }
}

function selectedBase(threadId) {
  return {
    messages: state.data.messages.filter((item) => item.thread_id === threadId),
    links: state.data.links.filter((item) => item.thread_id === threadId),
    approvals: state.data.approvals.filter((item) => item.thread_id === threadId),
  };
}

// Nachrichten und Verknüpfungen nur des ausgewählten Threads (wie vor dem Port). Auswahlwechsel löst die alten Abos,
// bevor die neuen gebunden werden: eine späte Antwort des alten Threads kann nichts mehr überschreiben.
function syncDetailSubscription() {
  const detail = state.detail;
  const threadId = state.data.threads.some((item) => item.id === state.selectedId) ? state.selectedId : '';
  if (detail.threadId === threadId) return;
  for (const stop of detail.stops) { try { stop?.(); } catch {} }
  detail.stops = [];
  detail.threadId = threadId;
  state.data.messages = [];
  state.data.links = [];
  if (!threadId) return;
  const sub = (collection, assign) => {
    try {
      detail.stops.push(state.api.query(collection, { selector: { thread_id: threadId }, sort: [{ updated_at_ms: 'desc' }] })
        .subscribe((rows) => {
          if (!state.alive() || detail.threadId !== threadId) return;
          assign(rows.filter((row) => row && row.is_deleted !== true));
          syncLookupSubscriptions();
          scheduleRender();
        }));
    } catch (error) {
      reportError(`threads.query.${collection}`, error);
    }
  };
  sub('user_thread_messages', (rows) => { state.data.messages = rows; });
  sub('user_thread_links', (rows) => { state.data.links = rows; });
}

function scheduleRender() {
  if (state.renderTimer) return;
  state.renderTimer = setTimeout(() => {
    state.renderTimer = null;
    if (!state.alive()) return;
    try {
      render();
      state.api.status.clear('threads.render');
    } catch (error) {
      reportError('threads.render', error);
    }
  }, 16);
}

// ---- Rendern --------------------------------------------------------------------------------------------------
function render(options = {}) {
  renderMobileState();
  const threads = visibleThreads();
  renderNotificationPreferences();
  syncGrammarSurfaces();
  renderList(threads, options);
  renderDetail();
  if (threads.length) recordFirstRenderMetric('first_usable_list');
  if (state.selectedId && state.detail.threadId === state.selectedId) recordFirstRenderMetric('selected_detail');
  if (state.restoredScrollTop && els.list && threads.length) {
    els.list.scrollTop = state.restoredScrollTop;
    state.restoredScrollTop = 0;
  }
}

function recordFirstRenderMetric(name) {
  const metrics = state.metrics;
  if (!metrics || metrics.recorded.has(name)) return;
  metrics.recorded.add(name);
  const endMark = `${metrics.prefix}.${name}`;
  performance.mark(endMark);
  performance.measure(`ctox.threads.${name}`, `${metrics.prefix}.mount`, endMark);
}

function renderMobileState() {
  els.root?.classList.toggle('is-mobile-detail', state.mobileView === 'detail' && Boolean(state.selectedId));
}

// Glyphe und Label nennen die Ansicht, zu der der Klick wechselt; kein aria-pressed (Aktion, kein Zustand).
function syncViewToggle(button) {
  if (!button) return;
  const next = button.dataset.pgView === 'list' ? 'cards' : 'list';
  const label = next === 'list' ? t('viewShowList', 'Als Liste anzeigen') : t('viewShowCards', 'Als Karten anzeigen');
  button.setAttribute('aria-label', label);
  button.title = label;
  button.removeAttribute('aria-pressed');
}

function onLeftGrammarChange(event) {
  const detail = event?.detail || {};
  state.search = String(detail.search ?? '');
  state.listView = detail.view === 'list';
  els.root?.classList.toggle('is-list-view', state.listView);
  syncViewToggle(els.viewToggle);
  const secondary = String(detail.filters?.view || '');
  if (secondary === 'team' && !isAdminRole()) state.filter = 'inbox';
  else if (secondary) state.filter = secondary;
  else state.filter = els.leftPane?.querySelector('[data-pg-band][aria-selected="true"]')?.dataset.pgBand || 'inbox';
  syncSelection();
  syncDetailSubscription();
  syncLookupSubscriptions();
  // Absichtlicher Sprung nach oben: Suche/Ansicht/Filter ändern die Menge.
  render({ resetScroll: true });
}

function onCenterGrammarChange(event) {
  state.centerListView = (event?.detail || {}).view === 'list';
  els.root?.classList.toggle('is-center-list-view', state.centerListView);
  syncViewToggle(els.centerViewToggle);
}

// Export: die sichtbare (gefilterte + gesuchte) Liste als JSON-Download (kein Import: es gibt keinen Schreibfluss).
function exportVisibleThreads() {
  const threads = visibleThreads();
  const exportedAt = new Date().toISOString();
  const payload = {
    format: 'ctox-threads-export',
    version: 1,
    exportedAt,
    module: 'threads',
    view: state.filter || 'inbox',
    count: threads.length,
    threads: threads.map((thread) => ({
      threadId: String(thread.id || ''),
      title: String(thread.title || ''),
      kind: String(thread.kind || ''),
      status: String(thread.status || ''),
      sourceModule: String(thread.source_module || ''),
      sourceRecordId: String(thread.source_record_id || ''),
      assignedUserId: String(thread.assigned_user_id || ''),
      updatedAtMs: Number(thread.updated_at_ms || 0),
    })),
  };
  const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = `ctox-threads-${exportedAt.slice(0, 19).replace(/[:T]/g, '-')}.json`;
  anchor.style.display = 'none';
  (state.root || document.documentElement).appendChild(anchor);
  anchor.click();
  anchor.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

// Zähler aller vier Hauptansichten (Nullen inklusive) über die verdrahtete Grammatik – Navigation, kein Zustandstext.
// Keine Fußzeile (Owner-Regel: keine Zähl-/Zustandszeilen in der App). Die lokale
// Abfrage ist vollständig (App-API 4.0): Zähler sind exakt, kein „≥“/„Mindestens“ mehr.
function syncGrammarSurfaces() {
  const pane = els.leftPane;
  if (!pane) return;
  const me = currentUserId();
  const isAdmin = isAdminRole();
  const counts = {};
  for (const filter of PRIMARY_FILTERS) {
    counts[filter] = state.data.threads.filter((thread) => threadMatchesFilter(thread, filter, me, isAdmin)).length;
  }
  const pg = pane.__ctoxPaneGrammar;
  if (pg?.setCounts) pg.setCounts(counts);
  else for (const [key, value] of Object.entries(counts)) {
    const node = pane.querySelector(`[data-pg-count="${key}"]`);
    if (node) node.textContent = ` (${value})`;
  }
  const secondaryActive = !PRIMARY_FILTERS.includes(state.filter);
  for (const tab of pane.querySelectorAll('[data-pg-band]')) {
    const active = !secondaryActive && tab.dataset.pgBand === state.filter;
    tab.setAttribute('aria-selected', String(active));
    tab.classList.toggle('is-active', active);
  }
  const select = pane.querySelector('[data-pg-filter][data-pg-name="view"]');
  if (select) select.value = secondaryActive ? state.filter : '';
  pg?.refreshDot?.();
}

function renderNotificationPreferences() {
  const prefs = personalPreferences();
  if (!prefs || els.notificationPreferencesForm?.contains(document.activeElement)) return;
  if (els.notificationThreshold) els.notificationThreshold.value = String(prefs.priority_threshold ?? 20);
  if (els.quietStart) els.quietStart.value = prefs.quiet_start || '';
  if (els.quietEnd) els.quietEnd.value = prefs.quiet_end || '';
  const types = new Set(arrayField(prefs.notification_types));
  if (els.notificationApprovals) els.notificationApprovals.checked = !types.size || types.has('approval');
  if (els.notificationMentions) els.notificationMentions.checked = !types.size || types.has('mention');
  if (els.notificationEscalations) els.notificationEscalations.checked = !types.size || types.has('escalation');
}

// Ein Prädikat für Filter UND Zähler: die Zahl eines Reiters folgt exakt seiner Regel.
function threadMatchesFilter(thread, filter, me, isAdmin) {
  if (filter === 'team' && !isAdmin) return false;
  const wideView = ['all', 'team', 'system'].includes(filter);
  if (!wideView && me && !threadRelevantToUser(thread, me)) return false;
  if (filter === 'system') return thread.kind === 'ctox_task';
  if (filter === 'archived') return thread.status === 'archived';
  if (thread.status === 'archived') return false;
  if (filter === 'snoozed') return isSnoozed(thread);
  if (isSnoozed(thread)) return false;
  if (filter === 'approvals') {
    return approvalsForThread(thread.id).some((item) => item.status === 'pending' && (!me || item.reviewer_user_id === me || isAdmin));
  }
  if (filter === 'mentions') return threadMentionsUser(thread.id, me);
  if (filter === 'team') return !thread.assigned_user_id || thread.status === 'escalated';
  if (filter === 'waiting') return threadWaitingOnUser(thread.id, me, isAdmin);
  if (filter === 'delegated') return threadDelegatedByUser(thread, me);
  if (filter === 'running') return threadHasRunningCtox(thread.id);
  if (filter === 'failed') return threadHasFailedCtox(thread.id) || thread.status === 'blocked';
  if (filter === 'watching') return arrayField(thread.watcher_user_ids).includes(me);
  if (filter === 'inbox') {
    if (!me) return false;
    if (approvalsForThread(thread.id).some((item) => item.status === 'pending' && item.reviewer_user_id === me)) return true;
    const reasons = new Set(arrayField(userStateForThread(thread.id)?.attention_reasons));
    if (['completed', 'closed', 'archived', 'cancelled', 'canceled'].includes(thread.status)) return false;
    if (thread.kind === 'ctox_task') return reasons.has('Blockiert') || reasons.has('Fehlgeschlagen');
    return reasons.size > 0;
  }
  return true;
}

function visibleThreads() {
  const me = currentUserId();
  const isAdmin = isAdminRole();
  const search = state.search.trim().toLowerCase();
  return state.data.threads
    .filter((thread) => threadMatchesFilter(thread, state.filter, me, isAdmin))
    .filter((thread) => {
      if (!search) return true;
      // Suche über Titel und Quellobjekt wie vor dem Port, dazu der angezeigte Name (Projekt · Aufgabe).
      const haystack = [thread.title, thread.source_module, thread.source_label, thread.source_record_id, displayName(thread).title]
        .join(' ').toLowerCase();
      return haystack.includes(search);
    })
    .sort((left, right) => attentionScore(right) - attentionScore(left)
      || Number(right.last_message_at_ms || right.updated_at_ms || 0) - Number(left.last_message_at_ms || left.updated_at_ms || 0));
}

// Unbekannte Route-Kennung: nie stiller Rückgriff auf den ersten Datensatz, sondern Zustandszone (R13).
function syncSelection() {
  if (state.requestedThreadId) {
    const requested = state.data.threads.find((thread) => thread.id === state.requestedThreadId);
    if (requested) {
      if (!visibleThreads().some((thread) => thread.id === requested.id)) {
        state.filter = requested.status === 'archived' ? 'archived' : isSnoozed(requested) ? 'snoozed' : 'all';
      }
      state.selectedId = requested.id;
      state.mobileView = 'detail';
      state.requestedThreadId = '';
      state.api?.status.clear('threads.route.unknown_thread');
      return;
    }
    if (state.data.threads.length) {
      state.api?.status.report({ level: 'warn', code: 'threads.route.unknown_thread', text: 'Thread nicht verfügbar.', ref: state.requestedThreadId });
    }
    state.selectedId = '';
    return;
  }
  const visible = visibleThreads();
  if (!visible.some((thread) => thread.id === state.selectedId)) state.selectedId = visible[0]?.id || '';
}

function relativeTime(ms) {
  const time = Number(ms || 0);
  if (!time) return '';
  const diff = Date.now() - time;
  if (diff < 60000) return 'jetzt';
  if (diff < 3600000) return `vor ${Math.round(diff / 60000)} min`;
  if (diff < 86400000) return `vor ${Math.round(diff / 3600000)} h`;
  if (diff < 7 * 86400000) return `vor ${Math.round(diff / 86400000)} d`;
  return formatTime(time);
}

// Warum mich das betrifft – serverseitige attention_reasons zuerst (Hub-Vertrag), lokale Ableitung nur als Rückfall.
function whyMeLine(thread, pendingApprovals) {
  const stored = arrayField(userStateForThread(thread.id)?.attention_reasons);
  if (stored.length) return stored.slice(0, 2).join(' · ');
  const parts = [];
  if (pendingApprovals > 0) parts.push('Freigabe nötig');
  else if (thread.status === 'blocked') parts.push('Blockiert');
  else if (threadHasFailedCtox(thread.id)) parts.push('Fehlgeschlagen');
  else if (threadHasRunningCtox(thread.id)) parts.push('AI arbeitet');
  else if (!thread.assigned_user_id) parts.push('Unzugeteilt');
  const due = Number(thread.due_at_ms || 0);
  if (due) {
    const days = Math.ceil((due - Date.now()) / 86400000);
    if (days < 0) parts.push('Frist überschritten');
    else if (days === 0) parts.push('Frist heute');
    else if (days <= 7) parts.push(`Frist ${new Date(due).toLocaleDateString('de-DE', { weekday: 'short' })}`);
  }
  return parts.join(' · ');
}

// Vorschau: beim ausgewählten Thread der jüngste fremde Beitrag; sonst der native nächste Schritt bzw. das
// Quelllabel – aber nie eine Kennung (workjet#594).
function foreignPreview(thread) {
  const me = currentUserId();
  const messages = messagesForThread(thread.id);
  const foreign = [...messages].reverse().find((item) => (item.author_user_id || item.actor_id) !== me);
  const last = foreign || messages[messages.length - 1];
  if (!last?.body) {
    const name = displayName(thread);
    const text = [thread.next_step, thread.source_label].find((value) => value && !isIdentifier(value, [thread.id, thread.source_record_id])
      && !name.title.toLowerCase().includes(String(value).trim().toLowerCase())) || '';
    return { sender: '', text };
  }
  const sender = last.author_display_name || last.author_user_id || (last.actor_type === 'ai' ? 'CTOX' : '');
  return { sender, text: last.body };
}

const STATUS_LABELS = {
  open: ['statusOpen', 'Offen'],
  blocked: ['statusBlocked', 'Blockiert'],
  escalated: ['statusEscalated', 'Eskaliert'],
  snoozed: ['statusSnoozed', 'Später'],
  archived: ['statusArchived', 'Erledigt'],
  running: ['statusRunning', 'AI arbeitet'],
  completed: ['statusCompleted', 'Abgeschlossen'],
};

function statusLabel(status) {
  const entry = STATUS_LABELS[String(status || 'open')];
  return entry ? t(entry[0], entry[1]) : String(status || '');
}

// Status des Fachvorgangs (Befehl/Aufgabe) ausgeschrieben statt Rohwert.
const WORK_STATUS_LABELS = {
  failed: ['workFailed', 'Fehlgeschlagen'], error: ['workFailed', 'Fehlgeschlagen'],
  running: ['workRunning', 'In Arbeit'], in_progress: ['workRunning', 'In Arbeit'], leased: ['workRunning', 'In Arbeit'],
  queued: ['workQueued', 'Eingereiht'], pending: ['workQueued', 'Eingereiht'], pending_sync: ['workQueued', 'Eingereiht'], accepted: ['workQueued', 'Eingereiht'],
  completed: ['workDone', 'Abgeschlossen'], done: ['workDone', 'Abgeschlossen'], handled: ['workDone', 'Abgeschlossen'], success: ['workDone', 'Abgeschlossen'],
  blocked: ['workBlocked', 'Blockiert'],
};

function workStatusLabel(status) {
  const entry = WORK_STATUS_LABELS[String(status || '').trim().toLowerCase()];
  return entry ? t(entry[0], entry[1]) : String(status || '');
}

function rosterName(userId) {
  const id = String(userId || '').trim();
  if (!id) return '';
  return (state.roster || []).find((user) => user.id === id)?.name || id;
}

// Anzeigename nach Projekt und Aufgabe (naming.js, workjet#594).
function displayName(thread) {
  return threadName(thread, {
    command: (id) => state.commandsById.get(id) || null,
    task: (id) => state.tasksById.get(id) || null,
    project: (id) => state.projects.get(id) || null,
    appTitle: (module) => state.apps.get(module)?.title || '',
  }, { supervisor: t('nameSupervisor', 'Supervisor'), ctoxWork: t('nameCtoxWork', 'CTOX Arbeit'), lang: state.lang });
}

function threadMetaLine(thread, why, name) {
  const parts = [];
  const status = statusLabel(thread.status);
  if (status) parts.push(status);
  const assignee = rosterName(thread.assigned_user_id);
  if (assignee) parts.push(assignee);
  // Herkunft nur, wenn sie lesbar ist und nicht schon im Namen steht (keine Kennung, keine Dopplung).
  const source = String(thread.source_label || '').trim();
  if (source && !isIdentifier(source, [thread.id, thread.source_record_id]) && !name.title.toLowerCase().includes(source.toLowerCase())) parts.push(source);
  const due = Number(thread.due_at_ms || 0);
  if (due && !/Frist/.test(String(why || ''))) {
    parts.push(`${t('metaDue', 'Frist')} ${new Date(due).toLocaleDateString('de-DE', { day: '2-digit', month: '2-digit' })}`);
  }
  return parts.join(' · ');
}

function threadSenderInitial(preview) {
  const name = String(preview.sender || '').trim();
  if (!name) return '·';
  if (name.toUpperCase() === 'CTOX') return '⌬';
  return name.slice(0, 1).toUpperCase();
}

function nameHtml(name) {
  return `${name.project && name.task ? `<span class="threads-name-project">${escapeHtml(name.project)}</span><span class="threads-name-sep" aria-hidden="true"> · </span>` : ''}<span class="threads-name-task">${escapeHtml(name.task || name.project || name.title)}</span>`;
}

function renderList(threads, { resetScroll = false } = {}) {
  if (!els.list) return;
  if (!threads.length) {
    // Leere Liste rendert nichts (R11) – kein „wird synchronisiert“, kein „keine Treffer“.
    renderHtmlIfChanged(els.list, '', { signature: `empty:${state.filter}:${Boolean(state.search)}`, preserveScroll: !resetScroll });
    if (resetScroll) els.list.scrollTop = 0;
    return;
  }
  const me = currentUserId();
  const rows = threads.map((thread) => {
    const pending = approvalsForThread(thread.id).filter((item) => item.status === 'pending').length;
    const unread = unreadNotificationsForThread(thread.id, me).length;
    const why = whyMeLine(thread, pending);
    const name = displayName(thread);
    const meta = threadMetaLine(thread, why, name);
    const preview = foreignPreview(thread);
    return { thread, pending, unread, why, name, meta, preview };
  });
  const html = rows.map(({ thread, unread, why, name, meta, preview }) => `
      <button type="button" class="ctox-list-item threads-list-item ${unread ? 'is-unread' : ''}"
        data-thread-id="${escapeAttr(thread.id)}"
        data-context-record-id="${escapeAttr(thread.source_record_id || thread.id)}"
        data-context-record-type="thread"
        data-context-label="${escapeAttr(name.title)}"
        aria-selected="false">
        <span class="threads-item-anchor" aria-hidden="true">
          <span class="threads-item-dot"></span>
          <span class="threads-item-avatar">${escapeHtml(threadSenderInitial(preview))}</span>
        </span>
        <span class="threads-item-main">
          <span class="threads-item-top">
            <span class="threads-list-title" title="${escapeAttr(name.title)}">${nameHtml(name)}</span>
            <time class="threads-item-time">${escapeHtml(relativeTime(thread.last_message_at_ms || thread.updated_at_ms))}</time>
          </span>
          <span class="threads-list-meta">${why ? `<b class="threads-attention">${escapeHtml(why)}</b>` : ''}${meta ? `<span class="threads-meta-facts">${escapeHtml(meta)}</span>` : ''}</span>
          ${preview.text ? `<span class="threads-list-preview" data-v4-allow-ellipsis>${preview.sender ? `<b>${escapeHtml(preview.sender)}:</b> ` : ''}${escapeHtml(preview.text)}</span>` : ''}
        </span>
      </button>
    `).join('');
  const signature = JSON.stringify(rows.map(({ thread, pending, unread, why, name, meta, preview }) => ([
    thread.id, name.title, why, meta, thread.last_message_at_ms || thread.updated_at_ms || 0, pending, unread, preview.text, preview.sender,
  ])));
  const focusedThreadId = els.list.contains(document.activeElement)
    ? document.activeElement?.closest?.('[data-thread-id]')?.getAttribute('data-thread-id')
    : null;
  const changed = renderHtmlIfChanged(els.list, html, { signature, preserveScroll: !resetScroll });
  if (changed && focusedThreadId) {
    [...els.list.querySelectorAll('[data-thread-id]')]
      .find((row) => row.getAttribute('data-thread-id') === focusedThreadId)
      ?.focus({ preventScroll: true });
  }
  if (resetScroll) els.list.scrollTop = 0;
  applyThreadSelection();
}

function applyThreadSelection() {
  els.list?.querySelectorAll('[data-thread-id]').forEach((row) => {
    const active = (row.getAttribute('data-thread-id') || '') === state.selectedId;
    row.classList.toggle('is-selected', active);
    row.setAttribute('aria-selected', String(active));
  });
}

function renderDetail() {
  const thread = selectedThread();
  if (!thread) {
    if (els.title) els.title.textContent = '';
    if (els.source) {
      els.source.textContent = '';
      els.source.disabled = true;
      els.source.dataset.threadDeepLink = '';
    }
    if (els.status) els.status.textContent = '';
    renderHtmlIfChanged(els.timeline, '');
    renderHtmlIfChanged(els.context, '');
    if (els.messageBody) els.messageBody.disabled = true;
    setThreadActionState(null);
    return;
  }
  const name = displayName(thread);
  if (els.messageBody) els.messageBody.disabled = false;
  // Kopf: Projekt als Kicker (Sprung zur Quelle), Aufgabe als Titel – die Kennung steht nur in den Details.
  if (els.title) {
    els.title.textContent = name.task || name.project || name.title;
    els.title.title = name.title;
  }
  if (els.source) {
    els.source.textContent = name.task && name.project ? name.project : (state.apps.get(thread.source_module)?.title || '');
    const sourceLink = sourceDeepLinkFor(thread);
    els.source.dataset.threadDeepLink = sourceLink;
    els.source.disabled = !sourceLink;
    els.source.classList.toggle('is-linked', Boolean(sourceLink));
    const sourceAction = sourceFocusSupported(thread) ? 'Objekt in der Quell-App öffnen' : 'Quell-App öffnen';
    els.source.title = sourceLink ? sourceAction : '';
    els.source.setAttribute('aria-label', sourceLink ? sourceAction : 'Kein verknüpfter Datensatz');
  }
  if (els.status) {
    const command = linkedCommandsForThread(thread.id).at(-1);
    const task = linkedTasksForThread(thread.id).at(-1);
    const workStatus = task?.status || command?.status;
    els.status.textContent = `Abstimmung: ${statusLabel(thread.status)}${workStatus ? ` · Fachvorgang: ${workStatusLabel(workStatus)}` : ''}`;
  }
  setThreadActionState(thread);
  renderTimeline(thread);
  renderContext(thread, name);
}

function renderTimeline(thread) {
  const messages = messagesForThread(thread.id);
  const approvals = approvalsForThread(thread.id);
  const timeline = [
    ...messages.map((item) => ({ type: 'message', at: Number(item.created_at_ms || item.updated_at_ms || 0), item })),
    ...approvals.map((item) => ({ type: 'approval', at: Number(item.requested_at_ms || item.created_at_ms || 0), item })),
  ].sort((left, right) => left.at - right.at);
  if (!timeline.length) {
    renderHtmlIfChanged(els.timeline, '');
    return;
  }
  const me = currentUserId();
  const html = timeline.map((entry) => {
    if (entry.type === 'approval') return renderApproval(entry.item);
    const message = entry.item;
    const mine = message.author_user_id && message.author_user_id === me;
    const kind = message.event_type || message.kind || 'note';
    const messageLabel = String(message.body || message.source_label || kind || message.id).replace(/\s+/g, ' ').slice(0, 160);
    const sourceLink = sourceDeepLinkFor(message);
    const sourceText = [message.source_label, message.source_module].find((value) => value && !isIdentifier(value, [message.source_record_id])) || 'Quelle';
    const linkHtml = sourceLink
      ? `<button type="button" class="threads-msg-link" data-thread-deep-link="${escapeAttr(sourceLink)}" title="${sourceFocusSupported(message) ? 'Objekt öffnen' : 'Quell-App öffnen'}">↗ ${escapeHtml(sourceText)}</button>`
      : '';
    const isEvent = ['ctox_status', 'approval_request', 'approval_approved', 'approval_rejected', 'handoff', 'status'].includes(kind)
      || (!message.author_user_id && message.actor_type !== 'ai');
    if (isEvent) {
      // Systemereignisse sind Protokoll, keine Unterhaltung: eine kompakte Zeile, Verweise als Links.
      const failed = /blockiert|fehlgeschlagen|failed/i.test(message.body || '');
      const head = String(message.body || '').split('\n')[0];
      return `
        <div class="threads-message is-event" data-message-id="${escapeAttr(message.id)}" data-context-record-id="${escapeAttr(message.id)}" data-context-record-type="thread_message" data-context-label="${escapeAttr(messageLabel)}">
          <span class="threads-event-text">${escapeHtml(head)}</span>
          <span class="threads-event-meta">${escapeHtml(relativeTime(message.created_at_ms || message.updated_at_ms))}</span>
          ${linkHtml}
          ${failed ? `<button type="button" class="threads-msg-link is-rework" data-rework-context="${escapeAttr(head)}" title="Neue CTOX-Analyse anfordern">↻ CTOX um Analyse bitten</button>` : ''}
        </div>
      `;
    }
    return `
      <article class="threads-message ${mine ? 'is-mine' : ''}" data-message-id="${escapeAttr(message.id)}" data-context-record-id="${escapeAttr(message.id)}" data-context-record-type="thread_message" data-context-label="${escapeAttr(messageLabel)}">
        <div class="threads-message-meta">${escapeHtml(message.actor_type === 'ai' ? 'CTOX' : (message.author_display_name || message.author_user_id || 'System'))} · ${escapeHtml(relativeTime(message.created_at_ms || message.updated_at_ms))}${linkHtml ? ' · ' : ''}${linkHtml}</div>
        <div class="threads-message-body">${escapeHtml(message.body || '')}</div>
      </article>
    `;
  }).join('');
  renderHtmlIfChanged(els.timeline, html);
}

function renderApproval(approval) {
  const command = approval.approved_command_id ? commandById(approval.approved_command_id) : null;
  const task = approval.approved_task_id ? taskById(approval.approved_task_id) : null;
  const me = currentUserId();
  const canDecide = approval.status === 'pending' && (approval.reviewer_user_id === me || isAdminRole());
  const risk = approvalRisk(approval);
  const impact = approvalImpact(approval);
  const evidence = approvalEvidence(approval);
  const approvalLabel = String(approval.prompt || approval.instruction || approval.source_label || approval.target_module || approval.id).replace(/\s+/g, ' ').slice(0, 160);
  const sourceLink = sourceDeepLinkFor(approval);
  const sourceTitle = sourceFocusSupported(approval) ? 'Objekt in der Quell-App öffnen' : 'Quell-App öffnen';
  return `
    <article class="threads-approval-card" data-approval-id="${escapeAttr(approval.id)}" data-context-record-id="${escapeAttr(approval.id)}" data-context-record-type="thread_approval" data-context-label="${escapeAttr(approvalLabel)}">
      <div class="threads-card-actions">
        ${sourceLink ? `<button type="button" class="ctox-pane-icon" data-thread-deep-link="${escapeAttr(sourceLink)}" aria-label="${sourceTitle}" title="${sourceTitle}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M14 5h5v5M19 5l-8 8M9 5H6a1 1 0 0 0-1 1v12a1 1 0 0 0 1-1v-3"/></svg></button>` : ''}
        ${(command?.status === 'failed' || task?.status === 'failed') ? `<button type="button" class="ctox-pane-icon" data-rework-context="${escapeAttr(approval.prompt || approval.instruction || '')}" aria-label="Neue CTOX-Analyse anfordern" title="Neue CTOX-Analyse anfordern"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 10a8 8 0 1 1 2 7"/><path d="M4 5v5h5"/></svg></button>` : ''}
      </div>
      ${canDecide ? `
        <div class="threads-card-actions is-decide">
          <button type="button" class="ctox-pane-icon is-confirm" data-approve-approval="${escapeAttr(approval.id)}" aria-label="${escapeHtml(t('approvalApprove', 'Freigeben'))}" title="${escapeHtml(t('approvalApprove', 'Freigeben'))}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12.5l5 5L19 7"/></svg></button>
          <button type="button" class="ctox-pane-icon is-danger" data-reject-approval="${escapeAttr(approval.id)}" aria-label="${escapeHtml(t('approvalReject', 'Ablehnen'))}" title="${escapeHtml(t('approvalReject', 'Ablehnen'))}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"/></svg></button>
          <button type="button" class="ctox-pane-icon" data-question-approval="${escapeAttr(approval.id)}" aria-label="Rückfrage an Requester" title="Rückfrage an Requester"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 9a3 3 0 1 1 4.2 2.75c-.9.4-1.2 1-1.2 2.05"/><path d="M12 17.3v.2"/></svg></button>
          <button type="button" class="ctox-pane-icon" data-edit-approval="${escapeAttr(approval.id)}" aria-label="${escapeHtml(t('approvalEdit', 'Bearbeiten'))}" title="${escapeHtml(t('approvalEdit', 'Bearbeiten'))}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 20.5l4.3-1 9.1-9.1a2.1 2.1 0 0 0-3-3L5.3 16.2 4 20.5Z"/><path d="M13.5 5.5l3 3"/></svg></button>
        </div>
      ` : ''}
      <div class="threads-message-meta">${escapeHtml(t('approvalHeadingPrefix', 'CTOX Freigabe'))} · ${escapeHtml(approval.status || 'pending')} · ${escapeHtml(formatTime(approval.requested_at_ms || approval.created_at_ms))}</div>
      <div class="threads-message-body">${escapeHtml(approval.prompt || '')}</div>
      <dl class="ctox-fields">
        <dt>Ziel</dt><dd>${escapeHtml(approval.target_module || approval.source_module || 'CTOX')}</dd>
        <dt>Risiko</dt><dd>${escapeHtml(risk)}</dd>
        <dt>Erwartete Auswirkung</dt><dd>${escapeHtml(impact)}</dd>
        <dt>Evidenz</dt><dd>${escapeHtml(evidence)}</dd>
      </dl>
      <div class="threads-message-meta">${escapeHtml(t('approvalRequester', 'Requester'))}: ${escapeHtml(approval.requester_display_name || approval.requester_user_id || '')} · ${escapeHtml(t('approvalReviewerShort', 'Reviewer'))}: ${escapeHtml(approval.reviewer_display_name || approval.reviewer_user_id || '')}</div>
      ${command ? `<div class="threads-message-meta"><button type="button" class="threads-msg-link" data-thread-deep-link="#ctox?command_id=${escapeAttr(command.command_id || command.id || '')}" title="Befehl in der CTOX-App öffnen">↗ ${escapeHtml(t('approvalCommand', 'Command'))}: ${escapeHtml(command.command_type || '')} · ${escapeHtml(workStatusLabel(command.status))}</button></div>` : ''}
      ${task ? `<div class="threads-message-meta"><button type="button" class="threads-msg-link" data-thread-deep-link="#ctox?task_id=${escapeAttr(task.id || '')}" title="Aufgabe in der CTOX-App öffnen">↗ ${escapeHtml(t('approvalTask', 'Task'))}: ${escapeHtml(task.title || task.id)} · ${escapeHtml(workStatusLabel(task.status))}</button></div>` : ''}
    </article>
  `;
}

function renderContext(thread, name) {
  const links = linksForThread(thread.id);
  const approvals = approvalsForThread(thread.id);
  const unread = unreadNotificationsForThread(thread.id, currentUserId());
  const recentMessages = messagesForThread(thread.id);
  const lastMessage = recentMessages[recentMessages.length - 1];
  const openApproval = approvals.find((item) => item.status === 'pending');
  const nextStep = openApproval
    ? `Freigabe durch ${openApproval.reviewer_display_name || openApproval.reviewer_user_id}`
    : (thread.next_step || (threadHasRunningCtox(thread.id) ? 'AI-Ergebnis abwarten' : 'Nächsten Schritt festlegen'));
  const rows = [
    { label: 'Was ist passiert?', value: lastMessage?.body || name.title },
    { label: 'Was braucht mich?', value: attentionReasons(thread).join(', ') || 'Aktuell keine direkte Aktion' },
    { label: 'Was passiert als Nächstes?', value: nextStep },
    { label: 'Quelle', value: contextLabel(thread) },
    { label: 'Record', value: [thread.source_record_type, thread.source_record_id].filter(Boolean).join(' / ') },
    { label: 'Deep Link', value: thread.source_deep_link, deepLink: thread.source_deep_link },
    { label: 'Teilnehmer', value: arrayField(thread.participant_ids).map(rosterName).join(', ') },
    { label: 'Watcher', value: arrayField(thread.watcher_user_ids).map(rosterName).join(', ') },
    { label: 'Ungelesen', value: String(unread.length) },
    { label: 'Snoozed bis', value: thread.snoozed_until_ms ? formatTime(thread.snoozed_until_ms) : '' },
    { label: 'Offene Freigaben', value: String(approvals.filter((item) => item.status === 'pending').length) },
    ...links.map((link) => ({
      label: link.link_type || link.link_role || 'Link',
      value: [link.app_collection, link.source_module, link.source_record_type, link.source_record_id, link.source_label].filter(Boolean).join(' / '),
      deepLink: link.deep_link,
    })),
    // Die Kennung des Threads steht nur hier, in den Details (workjet#594).
    { label: t('detailId', 'Kennung'), value: thread.id, mono: true },
  ].filter((row) => String(row.value || row.deepLink || '').trim());
  const rowHtml = `<dl class="ctox-fields ctox-fields--stacked">${rows.map(renderContextRow).join('')}</dl>`;
  const notificationHtml = unread.length
    ? `<div class="threads-notification-list">${unread.map((item) => `
        <div class="threads-notification-item" data-context-record-id="${escapeAttr(item.id)}" data-context-record-type="thread_notification" data-context-label="${escapeAttr(item.title || item.body_preview || item.notification_type || item.id)}">
          <span>${escapeHtml(item.title || item.notification_type || 'Notification')}</span>
          <div class="threads-notification-actions">
            <button type="button" class="ctox-pane-icon" data-mark-notification="${escapeAttr(item.id)}" aria-label="Gelesen" title="Gelesen"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12.5l5 5L19 7"/></svg></button>
            <button type="button" class="ctox-pane-icon" data-dismiss-notification="${escapeAttr(item.id)}" aria-label="Ausblenden" title="Ausblenden"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"/></svg></button>
          </div>
        </div>
      `).join('')}</div>`
    : '';
  renderHtmlIfChanged(els.context, rowHtml + notificationHtml);
}

// ---- Befehle (nie warten; Status in die Zustandszone) ---------------------------------------------------------
function dispatchThreadsCommand(commandType, payload, { recordId = '', sourceModule = 'threads' } = {}) {
  const api = state.api;
  const code = `threads.command.${commandType}`;
  const clientContext = {
    action: commandType,
    module: APP_ID,
    module_id: APP_ID,
    app_id: APP_ID,
    source_module: sourceModule || APP_ID,
    visible_scope: { app: { module_id: APP_ID, app_id: APP_ID }, source: payload?.source_context || selectedSourceContext() },
  };
  let handle;
  try {
    // record_id/client_context als Befehlsoptionen (§16.19 Nr. 8, V4-HANDLERS U-3/W-1b), nicht im payload.
    handle = api.command(commandType, payload, { record_id: recordId, client_context: clientContext });
  } catch (error) {
    reportError(code, error);
    return null;
  }
  const failText = (detail) => `${t('commandFailed', 'Aktion konnte nicht abgeschlossen werden.')}${detail ? ` ${detail}` : ''}`;
  handle.local?.catch?.((error) => api.status.report({ level: 'error', code, text: failText(error?.message || ''), ref: handle.id }));
  const stop = handle.status.subscribe((status) => {
    if (status.status === 'failed') {
      api.status.report({ level: 'error', code, text: failText(status.error || ''), ref: handle.id });
      queueMicrotask(() => stop?.());
    } else if (status.status === 'done') {
      api.status.clear(code);
      queueMicrotask(() => stop?.());
    }
  });
  return handle;
}

function submitMessage() {
  const thread = selectedThread();
  const body = String(els.messageBody?.value || '').trim();
  if (!thread || !body) return;
  const mentionTargets = mentionTargetsIn(body);
  dispatchThreadsCommand('threads.message.create', {
    thread_id: thread.id,
    body,
    ...(mentionTargets.length ? { target_user_ids: mentionTargets, kind: 'mention', message_kind: 'mention' } : {}),
  }, { recordId: thread.id, sourceModule: thread.source_module || 'threads' });
  els.messageBody.value = '';
}

function claimSelectedThread() {
  const thread = selectedThread();
  if (!thread) return;
  dispatchThreadsCommand('threads.thread.claim', {
    thread_id: thread.id,
    expected_updated_at_ms: Number(thread.updated_at_ms || 0),
  }, { recordId: thread.id });
}

function submitHandoff() {
  const thread = selectedThread();
  const target = String(els.handoffTarget?.value || '').trim();
  const expectation = String(els.handoffBody?.value || '').trim();
  const dueAt = els.handoffDue?.value ? new Date(els.handoffDue.value).getTime() : 0;
  const returnReason = String(els.handoffReturnReason?.value || '').trim();
  if (!thread || !target || !expectation) return;
  dispatchThreadsCommand('threads.handoff.create', {
    thread_id: thread.id,
    expected_updated_at_ms: Number(thread.updated_at_ms || 0),
    target_user_id: target,
    expectation,
    due_at_ms: Number.isFinite(dueAt) ? dueAt : 0,
    return_reason: returnReason,
  }, { recordId: thread.id });
  els.handoffBody.value = '';
  if (els.handoffDue) els.handoffDue.value = '';
  if (els.handoffReturnReason) els.handoffReturnReason.value = '';
}

function submitAiRequest() {
  const thread = selectedThread();
  const goal = String(els.aiGoal?.value || '').trim();
  if (!thread || !goal) return;
  dispatchThreadsCommand('threads.ai.request', { thread_id: thread.id, goal, risk_class: 'internal' }, { recordId: thread.id });
  els.aiGoal.value = '';
}

function submitNotificationPreferences() {
  const notificationTypes = [];
  if (els.notificationApprovals?.checked) notificationTypes.push('approval');
  if (els.notificationMentions?.checked) notificationTypes.push('mention');
  if (els.notificationEscalations?.checked) notificationTypes.push('escalation');
  dispatchThreadsCommand('threads.notification.preferences.update', {
    priority_threshold: Number(els.notificationThreshold?.value || 20),
    quiet_start: String(els.quietStart?.value || ''),
    quiet_end: String(els.quietEnd?.value || ''),
    notification_types: notificationTypes,
  }, { recordId: currentUserId() });
}

function submitNote() {
  const body = String(els.noteBody?.value || '').trim();
  const targetUserIds = splitUserIds(els.noteTarget?.value || '');
  if (!body) return;
  const payload = buildNotePayload({ body, targetUserIds, threadId: state.selectedId, sourceContext: selectedSourceContext() });
  dispatchThreadsCommand('threads.note.create', payload, {
    recordId: payload.thread_id,
    sourceModule: payload.source_context?.module || 'threads',
  });
  if (els.noteBody) els.noteBody.value = '';
}

function submitApprovalRequest() {
  const prompt = String(els.approvalPrompt?.value || '').trim();
  const reviewerUserId = String(els.approvalReviewer?.value || '').trim();
  if (!prompt || !reviewerUserId) return;
  const payload = buildApprovalRequestPayload({ prompt, reviewerUserId, threadId: state.selectedId, sourceContext: selectedSourceContext() });
  dispatchThreadsCommand('threads.ctox_approval.request', payload, {
    recordId: payload.thread_id,
    sourceModule: payload.source_context?.module || 'threads',
  });
  if (els.approvalPrompt) els.approvalPrompt.value = '';
}

// Dialoge nur im App-Fenster (api.shell.dialog, OFFEN-A); destruktive/riskante Bestätigungen mit default:false.
async function promptDialog(title, defaultValue = '') {
  const answer = await state.api.shell.dialog({ kind: 'prompt', title, text: '', default: defaultValue });
  return answer == null || answer === false ? null : String(answer);
}

async function decideApproval(approvalId, decision) {
  if (!approvalId) return;
  const approval = approvalById(approvalId);
  const expectedUpdatedAt = Number(approval?.updated_at_ms || 0);
  if (!expectedUpdatedAt) throw new Error('Approval version missing.');
  if (decision === 'approve' && approvalNeedsExplicitConfirmation(approval)) {
    const confirmed = await state.api.shell.dialog({
      kind: 'confirm', title: 'Riskante Aktion wirklich freigeben?', text: approvalImpact(approval),
      confirm_label: t('approvalApprove', 'Freigeben'), default: false,
    });
    if (confirmed !== true) return;
  }
  const note = decision === 'reject' ? (await promptDialog('Begründung oder Änderungswunsch:', '')) || '' : '';
  // Kein Nachladen in Schleife mehr: die Entscheidung erscheint über das Live-Abo von ctox_task_approval_requests.
  dispatchThreadsCommand(
    decision === 'approve' ? 'threads.ctox_approval.approve' : 'threads.ctox_approval.reject',
    { approval_request_id: approvalId, expected_updated_at_ms: expectedUpdatedAt, decision_note: note },
    { recordId: approvalId, sourceModule: 'threads' },
  );
}

function approvalRisk(approval) {
  return String(approval?.risk_class || approval?.target_payload?.risk_class || approval?.target_payload?.risk || 'mittel')
    .trim().toLowerCase() || 'mittel';
}

function approvalImpact(approval) {
  return String(
    approval?.expected_impact
      || approval?.target_payload?.expected_impact
      || approval?.target_payload?.impact
      || approval?.instruction
      || approval?.prompt
      || 'Ausführung des angeforderten Commands',
  ).trim();
}

function approvalEvidence(approval) {
  const refs = arrayField(approval?.evidence_refs || approval?.target_payload?.evidence_refs);
  if (refs.length) return refs.slice(0, 3).join(', ');
  return [approval?.source_module, approval?.source_record_type, approval?.source_record_id].filter(Boolean).join(' / ') || 'Thread-Verlauf';
}

function approvalNeedsExplicitConfirmation(approval) {
  return ['high', 'critical', 'hoch', 'kritisch', 'external', 'financial', 'personal_data', 'irreversible'].includes(approvalRisk(approval));
}

async function editApproval(approvalId) {
  if (!approvalId) return;
  const approval = approvalById(approvalId);
  const expectedUpdatedAt = Number(approval?.updated_at_ms || 0);
  if (!expectedUpdatedAt) throw new Error('Approval version missing.');
  const prompt = await promptDialog('Finaler CTOX Prompt:', approval?.prompt || '');
  if (!prompt || !prompt.trim()) return;
  dispatchThreadsCommand('threads.ctox_approval.edit', {
    approval_request_id: approvalId,
    expected_updated_at_ms: expectedUpdatedAt,
    prompt: prompt.trim(),
    instruction: prompt.trim(),
  }, { recordId: approvalId, sourceModule: 'threads' });
}

function toggleWatch() {
  const thread = selectedThread();
  if (!thread) return;
  const watching = arrayField(thread.watcher_user_ids).includes(currentUserId());
  dispatchThreadsCommand(watching ? 'threads.thread.unwatch' : 'threads.thread.watch', { thread_id: thread.id }, {
    recordId: thread.id, sourceModule: 'threads',
  });
}

async function snoozeSelectedThread() {
  const thread = selectedThread();
  if (!thread) return;
  const hours = Number((await promptDialog('Snooze in Stunden:', '24')) || 0);
  if (!Number.isFinite(hours) || hours <= 0) return;
  dispatchThreadsCommand('threads.thread.snooze', {
    thread_id: thread.id,
    snoozed_until_ms: Date.now() + Math.round(hours * 60 * 60 * 1000),
  }, { recordId: thread.id, sourceModule: 'threads' });
}

function archiveSelectedThread() {
  const thread = selectedThread();
  if (!thread) return;
  dispatchThreadsCommand('threads.thread.archive', { thread_id: thread.id }, { recordId: thread.id, sourceModule: 'threads' });
}

function updateNotification(notificationId, action) {
  if (!notificationId) return;
  dispatchThreadsCommand(
    action === 'dismiss' ? 'threads.notification.dismiss' : 'threads.notification.mark_read',
    { notification_id: notificationId },
    { recordId: notificationId, sourceModule: 'threads' },
  );
}

function reportError(code, error) {
  console.warn('[threads]', code, error);
  state.api?.status.report({ level: 'error', code, text: `${t('commandFailed', 'Aktion konnte nicht abgeschlossen werden.')} ${error?.message || ''}`.trim() });
}

function showError(error) {
  reportError('threads.action', error);
}

function selectedThread() {
  return state.data.threads.find((item) => item.id === state.selectedId) || null;
}

function approvalById(approvalId) {
  return state.data.approvals.find((item) => item.id === approvalId || item.approval_request_id === approvalId) || null;
}

function setThreadActionState(thread) {
  const disabled = !thread;
  [els.watch, els.snooze, els.mobileSnooze, els.archive, els.delegate, ...els.claimButtons].forEach((button) => {
    if (button) button.disabled = disabled;
  });
  const terminal = ['archived', 'completed', 'closed', 'cancelled', 'canceled'].includes(thread?.status);
  for (const button of [els.snooze, els.mobileSnooze, els.archive, els.delegate]) {
    if (button) button.disabled = disabled || terminal;
  }
  for (const button of els.claimButtons) {
    const canClaim = thread && (!thread.assigned_user_id || thread.assigned_user_id === currentUserId()) && !terminal;
    button.disabled = disabled || !canClaim;
    button.title = thread?.assigned_user_id === currentUserId() ? 'Bereits übernommen' : 'Abstimmung übernehmen';
  }
  if (els.watch && thread) {
    const watching = arrayField(thread.watcher_user_ids).includes(currentUserId());
    const label = watching ? 'Unwatch' : 'Watch';
    els.watch.title = label;
    els.watch.setAttribute('aria-label', label);
    els.watch.setAttribute('aria-pressed', watching ? 'true' : 'false');
  }
}

function selectedSourceContext() {
  const thread = selectedThread();
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
  const deepLink = normalizeInternalDeepLink(row?.deepLink, state.selectedId, registeredModuleIds());
  const valueHtml = deepLink
    ? `<a href="${escapeAttr(deepLink)}" data-thread-deep-link="${escapeAttr(deepLink)}">${escapeHtml(value || deepLink)}</a>`
    : `<span${row?.mono ? ' class="threads-detail-id"' : ''}>${escapeHtml(value)}</span>`;
  return `<dt>${escapeHtml(label)}</dt><dd>${valueHtml}</dd>`;
}

// Entscheidungsflächen verlinken das Objekt, über das sie entscheiden (Mensch in der Schleife).
function sourceDeepLinkFor(entry) {
  return normalizeInternalDeepLink(buildSourceDeepLink(entry, state.selectedId), state.selectedId, registeredModuleIds());
}

// '#app?a=1' → api.shell.navigate(app, {a:'1'}); unbekanntes Ziel meldet die Shell selbst in die Zone (R13).
function navigateDeepLink(value) {
  const link = normalizeInternalDeepLink(value, state.selectedId, registeredModuleIds());
  const match = /^#([a-z][a-z0-9-]*)(?:\?(.*))?$/i.exec(link || '');
  if (!match) return;
  performance.mark('ctox.threads.source_navigation_started');
  state.api.shell.navigate(match[1], Object.fromEntries(new URLSearchParams(match[2] || '')));
}

function registeredModuleIds() {
  return state.apps.size ? new Set([...state.apps.keys()].map((id) => String(id || '').trim())) : null;
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
  return notificationsForThread(threadId).filter((item) => (!userId || item.user_id === userId) && item.status === 'unread');
}

function threadRelevantToUser(thread, userId) {
  if (!userId) return true;
  return Boolean(userStateForThread(thread.id))
    || thread.assigned_user_id === userId
    || arrayField(thread.participant_ids).includes(userId)
    || arrayField(thread.watcher_user_ids).includes(userId)
    || thread.created_by_id === userId
    || notificationsForThread(thread.id).some((item) => item.user_id === userId && item.status !== 'dismissed')
    || approvalsForThread(thread.id).some((item) => item.reviewer_user_id === userId || item.requester_user_id === userId);
}

function threadMentionsUser(threadId, userId) {
  if (!userId) return false;
  return arrayField(userStateForThread(threadId)?.attention_reasons).includes('Erwähnung');
}

function threadWaitingOnUser(threadId, userId, isAdmin) {
  return approvalsForThread(threadId).some((item) => item.status === 'pending' && (isAdmin || !userId || item.reviewer_user_id === userId))
    || notificationsForThread(threadId).some((item) => (!userId || item.user_id === userId) && ['waiting_on_user', 'approval_request', 'approval_requested'].includes(item.notification_type || item.reason));
}

function threadDelegatedByUser(thread, userId) {
  if (!userId) return false;
  return thread.created_by_id === userId || approvalsForThread(thread.id).some((item) => item.requester_user_id === userId);
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

function linkedCommandIds(data) {
  return [
    ...data.messages.map((item) => item.command_id),
    ...data.links.map((item) => item.command_id),
    ...data.approvals.map((item) => item.approved_command_id),
  ].filter(Boolean);
}

function linkedTaskIds(data, commands) {
  return [
    ...data.links.map((item) => item.task_id),
    ...data.approvals.map((item) => item.approved_task_id),
    ...commands.map((item) => item.task_id),
  ].filter(Boolean);
}

function isSnoozed(thread) {
  const until = Number(thread.snoozed_until_ms || 0);
  return thread.status === 'snoozed' && until > Date.now();
}

function statusIn(status, values) {
  return values.includes(String(status || '').trim().toLowerCase());
}

function commandById(commandId) {
  return state.data.commands.find((item) => item.id === commandId || item.command_id === commandId) || state.commandsById.get(commandId) || null;
}

function taskById(taskId) {
  return state.data.queue.find((item) => item.id === taskId || item.task_id === taskId) || state.tasksById.get(taskId) || null;
}

function contextLabel(thread) {
  return [thread.source_module, thread.source_label || thread.source_record_id].filter(Boolean).join(' · ') || 'Threads';
}

function userStateForThread(threadId) {
  return state.personalStateByThread?.get(threadId) || null;
}

function attentionReasons(thread) {
  return arrayField(userStateForThread(thread.id)?.attention_reasons);
}

function attentionScore(thread) {
  return Number(userStateForThread(thread.id)?.attention_score || 0);
}

function personalPreferences() {
  return state.data.states?.find((item) => item.user_id === currentUserId() && item.thread_id === '__preferences__')
    ?.notification_preferences || null;
}

function currentUserId() {
  return String(state.api?.actor.id || '').trim();
}

function currentUserRole() {
  return String(state.api?.actor.role || 'user').trim();
}

function isAdminRole() {
  return ['chef', 'admin'].includes(currentUserRole());
}

function arrayField(value) {
  return Array.isArray(value) ? value.map((item) => String(item || '').trim()).filter(Boolean) : [];
}

function formatTime(value) {
  const ms = Number(value || 0);
  if (!Number.isFinite(ms) || ms <= 0) return '';
  try {
    return new Intl.DateTimeFormat(state.lang === 'en' ? 'en' : 'de', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' }).format(new Date(ms));
  } catch {
    return new Date(ms).toLocaleString();
  }
}

function escapeHtml(value) {
  return String(value ?? '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

function escapeAttr(value) {
  return escapeHtml(value).replace(/`/g, '&#96;');
}

// ---- Personen: Auswahllisten und @-Erwähnungen (Menschen wählen Personen, tippen keine IDs) --------------------
function renderRosterSelects() {
  const roster = state.roster || [];
  for (const select of els.root?.querySelectorAll('select[data-roster-select]') || []) {
    const current = select.value;
    select.innerHTML = ['<option value="">— Person wählen —</option>',
      ...roster.map((user) => `<option value="${escapeAttr(user.id)}">${escapeHtml(user.name)}</option>`)].join('');
    if (current) select.value = current;
  }
  for (const [key, value] of Object.entries(state.restoredSelects || {})) {
    if (els[key] && [...els[key].options].some((option) => option.value === value)) {
      els[key].value = value;
      delete state.restoredSelects[key];
    }
  }
}

function mentionTargetsIn(body) {
  const roster = state.roster || [];
  const targets = new Set();
  for (const match of String(body || '').matchAll(/@([\p{L}\p{N}_.-]+)/gu)) {
    const token = match[1].toLowerCase();
    for (const user of roster) {
      if (user.id.toLowerCase() === token || user.name.toLowerCase() === token || user.name.toLowerCase().split(/\s+/)[0] === token) {
        targets.add(user.id);
      }
    }
  }
  return [...targets];
}

function wireMentionPopup(disposers) {
  const textarea = els.messageBody;
  const popup = els.root?.querySelector('[data-mention-popup]');
  if (!textarea || !popup) return;
  const close = () => { popup.hidden = true; popup.innerHTML = ''; };
  const apply = (user) => {
    const value = textarea.value;
    const upto = value.slice(0, textarea.selectionStart).replace(/@([\p{L}\p{N}_.-]*)$/u, `@${user.name.split(/\s+/)[0]} `);
    textarea.value = upto + value.slice(textarea.selectionStart);
    textarea.focus();
    close();
  };
  on(textarea, 'input', () => {
    const upto = textarea.value.slice(0, textarea.selectionStart);
    const match = upto.match(/@([\p{L}\p{N}_.-]*)$/u);
    if (!match) { close(); return; }
    const roster = state.roster || [];
    const query = match[1].toLowerCase();
    const hits = roster.filter((user) => !query || user.name.toLowerCase().includes(query) || user.id.toLowerCase().includes(query)).slice(0, 6);
    if (!hits.length) { close(); return; }
    popup.innerHTML = hits.map((user) => `<button type="button" data-mention-id="${escapeAttr(user.id)}"><b>${escapeHtml(user.name)}</b><span>${escapeHtml(user.id)}</span></button>`).join('');
    popup.hidden = false;
    for (const button of popup.querySelectorAll('[data-mention-id]')) {
      button.addEventListener('click', () => {
        const user = roster.find((entry) => entry.id === button.dataset.mentionId);
        if (user) apply(user);
      });
    }
  }, disposers);
  on(textarea, 'keydown', (event) => {
    if (event.key === 'Escape') close();
    if (event.key === 'Enter' && !popup.hidden) {
      const first = popup.querySelector('[data-mention-id]');
      if (first) { event.preventDefault(); first.click(); }
    }
  }, disposers);
  on(textarea, 'blur', () => { setTimeout(close, 200); }, disposers);
}

// Dritte Entscheidung neben Freigeben/Ablehnen: Rückfrage an die anfragende Person; die Freigabe bleibt offen.
async function askApprovalQuestion(approvalId) {
  const approval = approvalById(approvalId);
  if (!approval) return;
  const requester = approval.requester_user_id || '';
  const question = await promptDialog(`Rückfrage an ${approval.requester_display_name || requester || 'Requester'}:`, '');
  if (!question) return;
  dispatchThreadsCommand('threads.message.create', {
    thread_id: approval.thread_id || state.selectedId,
    body: `Rückfrage zur Freigabe: ${question}`,
    ...(requester ? { target_user_ids: [requester], kind: 'mention', message_kind: 'mention' } : {}),
  }, { recordId: approval.thread_id || approvalId, sourceModule: approval.source_module || 'threads' });
}

// Ein fehlgeschlagener CTOX-Befehl kann eine eigene Analyse anstoßen; der Quellbefehl bleibt unberührt.
async function requestRework(context) {
  const thread = selectedThread();
  if (!thread) return;
  const goal = await promptDialog('Was soll CTOX analysieren?', `Analysiere den Fehler: ${context}`.trim());
  if (!goal) return;
  dispatchThreadsCommand('threads.ai.request', { thread_id: thread.id, goal, risk_class: 'internal' }, { recordId: thread.id });
}
