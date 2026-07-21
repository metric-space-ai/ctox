import { loadModuleMessages } from '../../shared/i18n.js';

// pi-coding workbench: pick a Business OS app, describe a task, delegate one
// bounded coding turn to the built-in pi agent (`ctox.coding.turn`), then show
// the result and the recent turns from the command log. The old vendor-CLI
// provider/session/diagnostics machinery is gone; the standard shell frame
// (.ctox-workspace--two-pane with data-resize-frame) stays.

const ROOT_CLASSES = 'ctox-workspace coding-agents-module';
const CODING_TURN_COMMAND = 'ctox.coding.turn';
const COMMAND_LOG_COLLECTION = 'business_commands';
const SESSIONS_COLLECTION = 'coding_agent_sessions';
const EVENTS_COLLECTION = 'coding_agent_events';
const RECENT_TURNS_LIMIT = 8;

// Optional model override. DEFAULT = no `model` in the payload, so the turn
// runs on the SAME model/provider as CTOX (the gateway). Only an explicit
// non-default pick sends a pi-ai provider model.
const MODEL_PRESETS = [
  // Honest list: only what the pi sidecar actually runs today. Anything else
  // must come from real model discovery (provider registry / llm gateway),
  // never from invented labels.
  { id: 'ctox', label: 'CTOX (Standard)', model: null },
];
const DEFAULT_MODEL_PRESET = MODEL_PRESETS[0].id;

const state = {
  ctx: null,
  modules: [],
  activeModuleId: '',
  listState: 'loading',
  running: false,
  recentTurns: [],
  activeSession: null,
  projectionTimers: {},
  // Canonical column grammar state: search + collapsed tray per column,
  // counted view band and cards/list toggle in the main view.
  appSearch: '',
  appSort: 'title',
  appSortDir: 'asc',
  chatSearch: '',
  chatRoleFilter: 'all',
  chatViewMode: 'cards',
  centerTab: 'chat',
};

const labels = {
  de: {
    leftKicker: 'pi Coding Agent',
    leftTitle: 'Apps',
    refresh: 'Aktualisieren',
    workbenchKicker: 'pi Coding Agent',
    workbenchEmptyTitle: 'Workbench',
    formTitle: 'Neuer Coding-Turn',
    taskLabel: 'Aufgabe',
    taskPlaceholder: 'Beschreibe, was der pi-Agent an dieser App ändern soll…',
    modelLabel: 'Modell',
    runLabel: 'Delegieren',
    resultTitle: 'Letzter Turn',
    recentTitle: 'Letzte Turns',
    loadingApps: 'Apps werden geladen…',
    emptyApps: 'Keine Apps verfügbar.',
    emptyAppsHint: 'Installiere eine Business-OS-App, um Coding-Turns zu delegieren.',
    selectAppHint: 'Wähle links eine App aus, um einen Turn zu delegieren.',
    emptyTask: 'Bitte beschreibe die gewünschte Änderung.',
    taskTooShort: 'Die Aufgabenbeschreibung ist zu kurz.',
    working: 'pi-Agent arbeitet… das kann einen Moment dauern.',
    turnDone: 'Fertig',
    turnFailed: 'Fehlgeschlagen',
    filesChanged: 'Dateien geändert',
    fileChanged: 'Datei geändert',
    messages: 'Nachrichten',
    recentEmpty: 'Noch keine Turns für diese App.',
    statusCompleted: 'abgeschlossen',
    statusFailed: 'fehlgeschlagen',
    statusRunning: 'läuft',
    sessionLabel: 'Session',
    sessionActive: 'aktiv',
    sessionTurns: 'Turns',
    sessionNone: 'Noch keine Session — der erste Turn startet sie.',
  },
  en: {
    leftKicker: 'pi Coding Agent',
    leftTitle: 'Apps',
    refresh: 'Refresh',
    workbenchKicker: 'pi Coding Agent',
    workbenchEmptyTitle: 'Workbench',
    formTitle: 'New coding turn',
    taskLabel: 'Task',
    taskPlaceholder: 'Describe what the pi agent should change in this app…',
    modelLabel: 'Model',
    runLabel: 'Delegate',
    resultTitle: 'Last turn',
    recentTitle: 'Recent turns',
    loadingApps: 'Loading apps…',
    emptyApps: 'No apps available.',
    emptyAppsHint: 'Install a Business OS app to delegate coding turns.',
    selectAppHint: 'Select an app on the left to delegate a turn.',
    emptyTask: 'Please describe the change you want.',
    taskTooShort: 'The task description is too short.',
    working: 'pi agent is working… this can take a moment.',
    turnDone: 'Done',
    turnFailed: 'Failed',
    filesChanged: 'files changed',
    fileChanged: 'file changed',
    messages: 'messages',
    recentEmpty: 'No turns for this app yet.',
    statusCompleted: 'completed',
    statusFailed: 'failed',
    statusRunning: 'running',
    sessionLabel: 'Session',
    sessionActive: 'active',
    sessionTurns: 'turns',
    sessionNone: 'No session yet — the first turn starts it.',
  },
};

let els = {};
let t = (k, f) => f || k;
// Fluid vendored chat core (../../vendor/chat-ui). Loaded lazily so a missing
// vendor file degrades to the inline transcript renderer instead of breaking
// the whole module.
let chatView = null;

export async function mount(ctx) {
  state.ctx = ctx;
  const messages = await loadModuleMessages(import.meta.url, ctx.locale, labels);
  t = (key, fallback) => messages[key] ?? fallback ?? key;

  // Inject stylesheet dynamically; CSS must inherit the JS cache-buster so a
  // deploy never leaves fresh JS running against a stale cached stylesheet.
  const cssVersion = String(import.meta.url).split('?v=')[1] || '';
  const cssHref = new URL('./index.css', import.meta.url).pathname + (cssVersion ? `?v=${cssVersion}` : '');
  let styleLink = document.getElementById('coding-agents-module-styles');
  if (!styleLink) {
    styleLink = document.createElement('link');
    styleLink.rel = 'stylesheet';
    styleLink.id = 'coding-agents-module-styles';
    document.head.appendChild(styleLink);
  }
  if (styleLink.getAttribute('href') !== cssHref) styleLink.href = cssHref;

  ctx.host.innerHTML = await loadModuleMarkup();
  ctx.left.replaceChildren();
  ctx.right.replaceChildren();

  bindElements(ctx.host);
  applyStaticTexts();
  renderModelSelect();
  wireEvents();
  await initChatView();
  const projectionSubscriptions = subscribeProjectionUpdates();

  // Column resizing is owned by the shell (setupModuleResizers in app.js),
  // wired declaratively from the `.ctox-column-resizer[data-resizer-var]`
  // handle inside the `[data-resize-frame]` root — no module JS needed here.

  loadApps();

  return () => {
    clearProjectionTimers();
    projectionSubscriptions.forEach((subscription) => {
      try { subscription?.unsubscribe?.(); } catch (err) { console.warn('[coding-agents] projection unsubscribe failed', err); }
    });
    try { chatView?.destroy?.(); } catch (err) { console.warn('[coding-agents] chat-ui destroy failed', err); }
    chatView = null;
    railChip?.remove();
    railChip = null;
    styleLink.remove();
  };
}

// Bring up the vendored chat core over #ca-recent-list. A load or construction
// failure leaves chatView null and renderRecentTurns() falls back to the inline
// transcript renderer — the module keeps working either way.
async function initChatView() {
  if (!els.recentList) return;
  try {
    const mod = await import('../../vendor/chat-ui/chat-ui.mjs');
    if (typeof mod.createChatView === 'function') {
      chatView = mod.createChatView(els.recentList, {});
    }
  } catch (err) {
    console.warn('[coding-agents] chat-ui unavailable, using inline transcript', err);
    chatView = null;
  }
}

async function loadModuleMarkup() {
  // Markup inherits the JS cache-buster — like the stylesheet, a deploy must
  // never leave fresh JS binding against stale cached markup.
  const version = String(import.meta.url).split('?v=')[1] || '';
  const markupHref = new URL('./index.html', import.meta.url).pathname + (version ? `?v=${version}` : '');
  const html = await fetch(markupHref).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}

function bindElements(root) {
  els.root = root.querySelector('[data-coding-agents-root]');
  els.appList = root.querySelector('#ca-app-list');
  els.refreshApps = root.querySelector('#ca-refresh-apps');
  els.activeAppKicker = root.querySelector('#ca-active-app-kicker');
  els.activeAppTitle = root.querySelector('#ca-active-app-title');
  els.modelBadge = root.querySelector('#ca-model-badge');
  els.openEditor = root.querySelector('#ca-open-editor');
  els.turnForm = root.querySelector('#ca-turn-form');
  els.taskInput = root.querySelector('#ca-task-input');
  els.modelSelect = root.querySelector('#ca-model-select');
  els.runButton = root.querySelector('#ca-run');
  els.hint = root.querySelector('#ca-hint');
  els.result = root.querySelector('#ca-result');
  els.resultBadge = root.querySelector('#ca-result-badge');
  els.resultBody = root.querySelector('#ca-result-body');
  els.recentList = root.querySelector('#ca-recent-list');
  els.turnsList = root.querySelector('#ca-turns-list');
  els.appSearch = root.querySelector('[data-app-search]');
  els.appFilterToggle = root.querySelector('[data-toggle-app-filters]');
  els.appFilterTray = root.querySelector('[data-app-filter-advanced]');
  els.appSort = root.querySelector('[data-app-sort]');
  els.appSortDir = root.querySelector('[data-app-sort-dir]');
  els.appFilterReset = root.querySelector('[data-reset-app-filters]');
  els.countApps = root.querySelector('[data-count-apps]');
  els.chatSearch = root.querySelector('[data-chat-search]');
  els.chatFilterToggle = root.querySelector('[data-toggle-chat-filters]');
  els.chatFilterTray = root.querySelector('[data-chat-filter-advanced]');
  els.chatRoleFilter = root.querySelector('[data-chat-role-filter]');
  els.chatFilterReset = root.querySelector('[data-reset-chat-filters]');
  els.chatViewButtons = [...root.querySelectorAll('[data-chat-view]')];
  els.centerTabs = [...root.querySelectorAll('[data-center-tab]')];
  els.countEvents = root.querySelector('[data-count-events]');
  els.countTurns = root.querySelector('[data-count-turns]');
  els.centerFooter = root.querySelector('#ca-center-footer');
  els.artifactFooter = root.querySelector('#ca-artifact-footer');
  if (els.root) els.root.className = ROOT_CLASSES;
}

function applyStaticTexts() {
  const root = els.root;
  if (!root) return;
  const setText = (selector, key) => {
    const node = root.querySelector(selector);
    if (node) node.textContent = t(key);
  };
  setText('[data-ca-left-kicker]', 'leftKicker');
  setText('[data-ca-left-title]', 'leftTitle');
  setText('[data-ca-form-title]', 'formTitle');
  setText('[data-ca-task-label]', 'taskLabel');
  setText('[data-ca-model-label]', 'modelLabel');
  setText('[data-ca-run-label]', 'runLabel');
  setText('[data-ca-result-title]', 'resultTitle');
  setText('[data-ca-recent-title]', 'recentTitle');
  const refreshLabel = t('refresh');
  if (els.refreshApps) {
    els.refreshApps.title = refreshLabel;
    els.refreshApps.setAttribute('aria-label', refreshLabel);
  }
  if (els.taskInput) els.taskInput.placeholder = t('taskPlaceholder');
}

function wireEvents() {
  if (els.refreshApps) {
    els.refreshApps.addEventListener('click', () => loadApps());
  }
  if (els.modelSelect) {
    els.modelSelect.addEventListener('change', () => {
      renderModelBadge();
    });
  }
  if (els.turnForm) {
    els.turnForm.addEventListener('submit', (event) => {
      event.preventDefault();
      delegateTurn();
    });
  }
  if (els.openEditor) {
    els.openEditor.addEventListener('click', () => {
      const mod = selectedModule();
      if (!mod) return;
      // Cross-link into the app's source editor (the per-app IDE + agent thread).
      state.ctx?.openDesktopApp?.('code-editor', { args: { moduleId: mod.id, moduleTitle: mod.title } });
    });
  }

  // Left column: search + collapsed tray (sort select, direction, reset).
  els.appSearch?.addEventListener('input', () => {
    state.appSearch = els.appSearch.value.trim().toLowerCase();
    syncFilterIndicators();
    renderAppList();
  });
  els.appFilterToggle?.addEventListener('click', () => toggleTray(els.appFilterToggle, els.appFilterTray));
  els.appSort?.addEventListener('change', () => { state.appSort = els.appSort.value; syncFilterIndicators(); renderAppList(); });
  els.appSortDir?.addEventListener('click', () => {
    state.appSortDir = state.appSortDir === 'asc' ? 'desc' : 'asc';
    els.appSortDir.dataset.dir = state.appSortDir;
    els.appSortDir.title = state.appSortDir === 'asc' ? 'Aufsteigend' : 'Absteigend';
    els.appSortDir.style.transform = state.appSortDir === 'asc' ? '' : 'scaleY(-1)';
    syncFilterIndicators();
    renderAppList();
  });
  els.appFilterReset?.addEventListener('click', () => {
    state.appSearch = ''; state.appSort = 'title'; state.appSortDir = 'asc';
    if (els.appSearch) els.appSearch.value = '';
    if (els.appSort) els.appSort.value = 'title';
    if (els.appSortDir) { els.appSortDir.dataset.dir = 'asc'; els.appSortDir.style.transform = ''; }
    syncFilterIndicators();
    renderAppList();
  });

  // Main view: transcript search + role tray + cards/list toggle + view band.
  els.chatSearch?.addEventListener('input', () => {
    state.chatSearch = els.chatSearch.value.trim().toLowerCase();
    syncFilterIndicators();
    renderRecentTurns();
  });
  els.chatFilterToggle?.addEventListener('click', () => toggleTray(els.chatFilterToggle, els.chatFilterTray));
  els.chatRoleFilter?.addEventListener('change', () => {
    state.chatRoleFilter = els.chatRoleFilter.value;
    syncFilterIndicators();
    renderRecentTurns();
  });
  els.chatFilterReset?.addEventListener('click', () => {
    state.chatSearch = ''; state.chatRoleFilter = 'all';
    if (els.chatSearch) els.chatSearch.value = '';
    if (els.chatRoleFilter) els.chatRoleFilter.value = 'all';
    syncFilterIndicators();
    renderRecentTurns();
  });
  els.chatViewButtons.forEach((button) => button.addEventListener('click', () => {
    state.chatViewMode = button.dataset.chatView;
    els.chatViewButtons.forEach((other) => other.setAttribute('aria-pressed', String(other === button)));
    renderRecentTurns();
  }));
  els.centerTabs.forEach((tab) => tab.addEventListener('click', () => {
    state.centerTab = tab.dataset.centerTab;
    els.centerTabs.forEach((other) => other.setAttribute('aria-selected', String(other === tab)));
    renderRecentTurns();
  }));
}

function toggleTray(toggle, tray) {
  if (!toggle || !tray) return;
  const open = tray.hidden;
  tray.hidden = !open;
  toggle.setAttribute('aria-expanded', String(open));
}

// Active-filter dot on the tray toggles: visible whenever a non-default
// filter is set, so a collapsed tray never hides active filtering.
function syncFilterIndicators() {
  const appActive = Boolean(state.appSearch) || state.appSort !== 'title' || state.appSortDir !== 'asc';
  els.appFilterToggle?.classList.toggle('has-active-filters', appActive);
  const chatActive = Boolean(state.chatSearch) || state.chatRoleFilter !== 'all';
  els.chatFilterToggle?.classList.toggle('has-active-filters', chatActive);
}

/* App/Module picker */

async function loadApps() {
  state.listState = 'loading';
  renderAppList();
  try {
    const raw = Array.isArray(state.ctx?.modules) && state.ctx.modules.length
      ? state.ctx.modules
      : typeof state.ctx?.getModules === 'function'
        ? state.ctx.getModules()
        : [];
    state.modules = normalizeCatalogModules(raw);
    state.listState = 'ready';
  } catch (err) {
    console.warn('[coding-agents] module catalog unavailable:', err);
    state.modules = [];
    state.listState = 'error';
  }
  renderAppList();
  // A cross-link from an app's source editor can launch this app focused on that
  // app (ctx.args.moduleId); otherwise fall back to the first app.
  const requested = String(state.ctx?.args?.moduleId || '').trim();
  if (state.modules.length && !state.modules.some((mod) => mod.id === state.activeModuleId)) {
    const target = requested && state.modules.some((mod) => mod.id === requested)
      ? requested
      : state.modules[0].id;
    selectApp(target);
  } else {
    updateWorkbenchHeader();
    updateFormState();
    renderRecentTurns();
  }
}

function normalizeCatalogModules(modules) {
  const rows = Array.isArray(modules) ? modules : [];
  const seen = new Set();
  return rows
    .filter((mod) => mod && typeof mod.id === 'string' && mod.id.trim())
    .filter((mod) => mod.hidden !== true && mod.editable !== false)
    .map((mod) => ({
      id: mod.id.trim(),
      title: String(mod.title || mod.name || mod.id).trim(),
    }))
    .filter((mod) => {
      if (seen.has(mod.id)) return false;
      seen.add(mod.id);
      return true;
    })
    .sort((left, right) => left.title.localeCompare(right.title, undefined, { sensitivity: 'base' }));
}

function renderAppList() {
  const box = els.appList;
  if (!box) return;
  box.innerHTML = '';

  if (state.listState === 'loading') {
    box.innerHTML = `<div class="ctox-empty"><strong>${escapeHtml(t('loadingApps'))}</strong></div>`;
    return;
  }
  if (state.listState === 'error' || state.modules.length === 0) {
    if (els.countApps) els.countApps.textContent = ' (0)';
    box.innerHTML = `<div class="ctox-empty"><strong>${escapeHtml(t('emptyApps'))}</strong><span>${escapeHtml(t('emptyAppsHint'))}</span></div>`;
    return;
  }

  const query = state.appSearch;
  const direction = state.appSortDir === 'desc' ? -1 : 1;
  const key = state.appSort === 'id' ? 'id' : 'title';
  const visible = state.modules
    .filter((mod) => !query || mod.title.toLowerCase().includes(query) || mod.id.toLowerCase().includes(query))
    .sort((left, right) => direction * left[key].localeCompare(right[key], undefined, { sensitivity: 'base' }));
  if (els.countApps) els.countApps.textContent = ` (${visible.length})`;
  if (!visible.length) {
    box.innerHTML = `<div class="ctox-empty"><strong>Keine Treffer.</strong></div>`;
    return;
  }

  visible.forEach((mod) => {
    const item = document.createElement('button');
    item.type = 'button';
    // Kit list row; `is-selected` drives the kit selection styling. The rail
    // shows the app icon only; the title lives in the hover chip (and inline
    // once the operator drags the rail wide enough for labels).
    item.className = `ctox-list-item coding-agents-app-item ${state.activeModuleId === mod.id ? 'is-selected' : ''}`;
    item.dataset.moduleId = mod.id;
    item.setAttribute('aria-label', mod.title);
    const initial = (mod.title.trim().charAt(0) || '?').toUpperCase();
    item.innerHTML = `
      <span class="coding-agents-app-icon"><img src="${escapeHtml(moduleIconUrl(mod.id))}" alt="" loading="lazy"><span class="coding-agents-app-monogram" hidden>${escapeHtml(initial)}</span></span>
      <span class="coding-agents-app-row">
        <span class="coding-agents-app-title">${escapeHtml(mod.title)}</span>
        <span class="coding-agents-app-id">${escapeHtml(mod.id)}</span>
      </span>
    `;
    const img = item.querySelector('img');
    img.addEventListener('error', () => {
      img.remove();
      item.querySelector('.coding-agents-app-monogram').hidden = false;
    });
    item.addEventListener('click', () => selectApp(mod.id));
    item.addEventListener('mouseenter', () => showRailChip(item, mod.title));
    item.addEventListener('mouseleave', hideRailChip);
    item.addEventListener('focus', () => showRailChip(item, mod.title));
    item.addEventListener('blur', hideRailChip);
    box.appendChild(item);
  });
}

function moduleIconUrl(moduleId) {
  return new URL(`../${moduleId}/icon.svg`, import.meta.url).pathname;
}

// One shared floating name chip for the narrow rail. position:fixed so the
// pane's overflow clipping cannot swallow it; skipped once inline labels show.
let railChip = null;
function showRailChip(item, title) {
  const rail = item.closest('.coding-agents-left');
  if (!rail || rail.getBoundingClientRect().width >= 150) return;
  if (!railChip) {
    railChip = document.createElement('div');
    railChip.className = 'coding-agents-rail-chip';
    document.body.appendChild(railChip);
  }
  const rect = item.getBoundingClientRect();
  railChip.textContent = title;
  railChip.style.left = `${Math.round(rect.right + 8)}px`;
  railChip.style.top = `${Math.round(rect.top + rect.height / 2)}px`;
  railChip.hidden = false;
}
function hideRailChip() {
  if (railChip) railChip.hidden = true;
}

function selectApp(moduleId) {
  state.activeModuleId = moduleId;
  renderAppList();
  updateWorkbenchHeader();
  updateFormState();
  setHint('');
  loadActiveSession();
  loadRecentTurns();
}

function selectedModule() {
  return state.modules.find((mod) => mod.id === state.activeModuleId) || null;
}

function updateWorkbenchHeader() {
  const mod = selectedModule();
  if (els.activeAppKicker) els.activeAppKicker.textContent = mod ? mod.id : t('workbenchKicker');
  if (els.activeAppTitle) els.activeAppTitle.textContent = mod ? mod.title : t('workbenchEmptyTitle');
  if (els.openEditor) els.openEditor.disabled = !mod;
}

function updateFormState() {
  const ready = Boolean(state.activeModuleId) && !state.running;
  if (els.taskInput) els.taskInput.disabled = !ready;
  if (els.modelSelect) els.modelSelect.disabled = !ready;
  if (els.runButton) {
    els.runButton.disabled = !ready;
    els.runButton.classList.toggle('is-running', state.running);
    els.runButton.setAttribute('aria-busy', state.running ? 'true' : 'false');
  }
  if (!state.activeModuleId && !state.running) setHint(t('selectAppHint'));
}

/* Model preset selector */

function modelPresetById(id) {
  return MODEL_PRESETS.find((preset) => preset.id === id) || MODEL_PRESETS[0];
}

function renderModelSelect() {
  const select = els.modelSelect;
  if (!select) return;
  select.innerHTML = '';
  MODEL_PRESETS.forEach((preset) => {
    const option = document.createElement('option');
    option.value = preset.id;
    option.textContent = preset.label;
    select.appendChild(option);
  });
  select.value = DEFAULT_MODEL_PRESET;
  renderModelBadge();
}

function renderModelBadge() {
  if (!els.modelBadge || !els.modelSelect) return;
  els.modelBadge.textContent = modelPresetById(els.modelSelect.value).label;
}

/* Delegation — the exact ctox.coding.turn dispatch pattern from the
   code-editor's delegateAgentTurn. */

function validateTaskPrompt(input) {
  const prompt = String(input || '').trim();
  if (!prompt) return { valid: false, prompt: '', error: 'empty' };
  if (prompt.length < 8) return { valid: false, prompt, error: 'too_short' };
  return { valid: true, prompt, error: '' };
}

function buildTurnPayload({ moduleId, prompt, presetId }) {
  const payload = {
    module_id: String(moduleId || '').trim(),
    prompt: String(prompt || '').trim(),
  };
  // Only an explicit non-default provider pick sends a model override;
  // omitted = the SAME model/provider as CTOX (default).
  const preset = modelPresetById(presetId);
  if (preset.model) payload.model = { ...preset.model };
  return payload;
}

async function delegateTurn() {
  if (!state.activeModuleId || state.running) return;
  const validation = validateTaskPrompt(els.taskInput?.value || '');
  if (!validation.valid) {
    setHint(t(validation.error === 'too_short' ? 'taskTooShort' : 'emptyTask'), true);
    els.taskInput?.focus();
    return;
  }

  state.running = true;
  updateFormState();
  setHint(t('working'));

  try {
    const accepted = await dispatchCodingTurn(buildTurnPayload({
      moduleId: state.activeModuleId,
      prompt: validation.prompt,
      presetId: els.modelSelect?.value || DEFAULT_MODEL_PRESET,
    }));
    const commandError = turnErrorFromProjection(accepted);
    if (commandError) {
      renderResult({ ok: false, error: commandError, module_id: state.activeModuleId });
      setHint(commandError, true);
      return;
    }
    const result = accepted?.result && typeof accepted.result === 'object' ? accepted.result : {};
    renderResult(result);
    if (result.ok === false) {
      setHint(result.error || t('turnFailed'), true);
    } else {
      const applied = Array.isArray(result.applied_files) ? result.applied_files.length : 0;
      setHint(`${t('turnDone')} — ${applied} ${applied === 1 ? t('fileChanged') : t('filesChanged')}.`);
      if (els.taskInput) els.taskInput.value = '';
    }
  } catch (error) {
    console.error('[coding-agents] coding turn failed:', error);
    const message = `${t('turnFailed')}: ${error?.message || error}`;
    renderResult({ ok: false, error: error?.message || String(error), module_id: state.activeModuleId });
    setHint(message, true);
  } finally {
    state.running = false;
    updateFormState();
    loadActiveSession();
    loadRecentTurns();
  }
}

async function dispatchCodingTurn(payload) {
  if (!state.ctx?.commandBus?.dispatch) {
    throw new Error('business_commands collection is required for coding turns');
  }
  const commandId = `cmd_coding_turn_${newId()}`;
  return state.ctx.commandBus.dispatch({
    id: commandId,
    module: 'ctox',
    type: CODING_TURN_COMMAND,
    record_id: `${payload.module_id}:coding`,
    inbound_channel: payload.module_id,
    payload,
    client_context: {
      source: 'business-os-coding-agents',
      module: 'coding-agents',
      module_id: payload.module_id,
      actor: actorContext(state.ctx.session),
    },
  }, { until: 'accepted' });
}

// Policy denials / failed commands become a readable message; ok:false
// results (e.g. "nothing to commit") are handled by the caller.
function turnErrorFromProjection(projection) {
  const status = String(projection?.status || '');
  const result = projection?.result && typeof projection.result === 'object' ? projection.result : {};
  const decision = result.policy_decision && typeof result.policy_decision === 'object'
    ? result.policy_decision
    : null;
  if (decision?.allowed === false) {
    return String(decision.display_reason || decision.reason_code || result.error || 'Turn was denied');
  }
  if (status === 'failed') {
    return String(result.error || projection?.error || 'Turn command failed');
  }
  return '';
}

/* Last-turn result card */

function renderResult(result) {
  if (!els.result || !els.resultBody || !els.resultBadge) return;
  els.result.hidden = false;
  els.resultBody.innerHTML = '';

  const ok = result && result.ok !== false;
  els.resultBadge.textContent = ok ? t('turnDone') : t('turnFailed');
  els.resultBadge.className = `ctox-badge ${ok ? 'is-success' : 'is-danger'}`;

  if (!ok) {
    const callout = document.createElement('div');
    callout.className = 'ctox-callout is-danger';
    callout.textContent = result?.error || t('turnFailed');
    els.resultBody.appendChild(callout);
    return;
  }

  const appliedFiles = Array.isArray(result.applied_files) ? result.applied_files : [];
  const meta = document.createElement('p');
  meta.className = 'coding-agents-turn-row-meta';
  const messageCount = Number(result.message_count || 0);
  meta.textContent = `${appliedFiles} ${appliedFiles === 1 ? t('fileChanged') : t('filesChanged')} · ${messageCount} ${t('messages')}`;
  els.resultBody.appendChild(meta);

  if (appliedFiles.length) {
    const list = document.createElement('ul');
    list.className = 'coding-agents-files';
    appliedFiles.forEach((path) => {
      const item = document.createElement('li');
      item.className = 'coding-agents-file';
      item.textContent = path;
      list.appendChild(item);
    });
    els.resultBody.appendChild(list);
  }
}

/* Recent turns from the business_commands log */

function subscribeProjectionUpdates() {
  const subscriptions = [];
  const commandLog = state.ctx?.db?.collection?.(COMMAND_LOG_COLLECTION);
  const commandSub = commandLog?.$?.subscribe?.(() => {
    scheduleProjectionRefresh(COMMAND_LOG_COLLECTION, () => loadRecentTurns());
  });
  if (commandSub) subscriptions.push(commandSub);
  const sessions = state.ctx?.db?.collection?.(SESSIONS_COLLECTION);
  const sessionSub = sessions?.$?.subscribe?.(() => {
    scheduleProjectionRefresh(SESSIONS_COLLECTION, () => loadActiveSession());
  });
  if (sessionSub) subscriptions.push(sessionSub);
  const events = state.ctx?.db?.collection?.(EVENTS_COLLECTION);
  const eventsSub = events?.$?.subscribe?.(() => {
    scheduleProjectionRefresh(EVENTS_COLLECTION, () => loadSessionEvents());
  });
  if (eventsSub) subscriptions.push(eventsSub);
  return subscriptions;
}

function scheduleProjectionRefresh(key, fn) {
  if (state.projectionTimers[key]) clearTimeout(state.projectionTimers[key]);
  state.projectionTimers[key] = setTimeout(async () => {
    delete state.projectionTimers[key];
    try {
      await fn();
    } catch (err) {
      console.warn(`[coding-agents] projection refresh failed for ${key}`, err);
    }
  }, 150);
}

function clearProjectionTimers() {
  Object.values(state.projectionTimers || {}).forEach((timer) => clearTimeout(timer));
  state.projectionTimers = {};
}

// One coding session per app (native `coding_agent_sessions`, id `pi:<module>`).
async function loadActiveSession() {
  if (!state.activeModuleId) {
    state.activeSession = null;
    renderRecentTurns();
    return;
  }
  const docs = await readCollectionDocs(SESSIONS_COLLECTION);
  const session = (Array.isArray(docs) ? docs : []).find(
    (doc) =>
      doc &&
      doc.is_deleted !== true &&
      doc._deleted !== true &&
      String(doc.workspace_root || '') === state.activeModuleId,
  );
  state.activeSession = session || null;
  await loadSessionEvents();
  renderRecentTurns();
}

// The conversation: native coding_agent_events (role/text/status/seq) of the
// active session — this IS the chat transcript the pi sidecar writes.
async function loadSessionEvents() {
  const sessionId = state.activeSession?.session_id
    || (state.activeModuleId ? `pi:${state.activeModuleId}` : '');
  if (!sessionId) { state.sessionEvents = []; return; }
  const docs = await readCollectionDocs(EVENTS_COLLECTION);
  state.sessionEvents = (Array.isArray(docs) ? docs : [])
    .filter((doc) => doc && doc.is_deleted !== true && String(doc.session_id || '') === sessionId)
    .sort((left, right) => Number(left.seq || 0) - Number(right.seq || 0));
}

async function loadRecentTurns() {
  const docs = await readCollectionDocs(COMMAND_LOG_COLLECTION);
  state.recentTurns = (Array.isArray(docs) ? docs : [])
    .filter((doc) => String(doc.command_type || doc.type || '') === CODING_TURN_COMMAND)
    .map(turnFromCommand)
    .filter((turn) => turn && (!state.activeModuleId || turn.moduleId === state.activeModuleId))
    .sort((left, right) => right.timeMs - left.timeMs)
    .slice(0, RECENT_TURNS_LIMIT);
  renderRecentTurns();
}

function turnFromCommand(doc) {
  if (!doc || doc.is_deleted === true || doc._deleted === true) return null;
  const payload = doc.payload && typeof doc.payload === 'object' ? doc.payload : {};
  const result = doc.result && typeof doc.result === 'object' ? doc.result : {};
  const appliedFiles = Array.isArray(result.applied_files) ? result.applied_files : [];
  return {
    id: String(doc.id || ''),
    moduleId: String(payload.module_id || result.module_id || ''),
    prompt: String(payload.prompt || ''),
    status: String(doc.status || ''),
    ok: result.ok !== false,
    error: result.ok === false ? String(result.error || '') : '',
    appliedCount: appliedFiles.length,
    timeMs: Number(doc.created_at_ms || doc.updated_at_ms || 0),
  };
}

function renderRecentTurns() {
  const box = els.recentList;
  if (!box) return;

  if (!state.activeModuleId) {
    // Directly owns the host here (no transcript); the chat view re-attaches
    // its thread on the next update() once an app is selected.
    box.innerHTML = `<div class="ctox-empty"><strong>Projekt links wählen</strong><br>Dann kannst du dem Agenten hier Aufträge geben.</div>`;
    renderArtifact();
    return;
  }

  // The chat: native session events (the sidecar transcript) when present,
  // otherwise the turn log as user-bubble + status-line pairs. Search + role
  // tray filter the transcript; the band counts BOTH views (zeros included).
  const allEvents = buildTranscriptEvents();
  const events = allEvents.filter((event) => {
    if (state.chatRoleFilter !== 'all') {
      const role = event.role === 'agent' ? 'assistant' : event.role;
      if (state.chatRoleFilter === 'system' ? (role === 'user' || role === 'assistant') : role !== state.chatRoleFilter) return false;
    }
    return !state.chatSearch || String(event.text || '').toLowerCase().includes(state.chatSearch);
  });
  const running = state.activeSession?.status === 'running';
  const filtered = events.length !== allEvents.length;
  const emptyText = filtered ? 'Keine Treffer im Verlauf.' : t('recentEmpty');

  if (els.countEvents) els.countEvents.textContent = ` (${events.length})`;
  if (els.countTurns) els.countTurns.textContent = ` (${state.recentTurns.length})`;

  const showTurns = state.centerTab === 'turns';
  if (els.turnsList) els.turnsList.hidden = !showTurns;
  box.hidden = showTurns;
  if (showTurns) {
    renderTurnsList();
  } else if (chatView && state.chatViewMode === 'cards') {
    chatView.update({ events, running: running && !filtered, emptyText });
  } else {
    renderTranscriptList(box, events, emptyText);
  }

  const footer = els.root?.querySelector('#ca-footer');
  if (footer) footer.textContent = `${state.modules.length} Apps`;
  if (els.centerFooter) {
    els.centerFooter.textContent = state.activeModuleId
      ? `${state.recentTurns.length} Delegationen · ${state.activeModuleId}${running ? ' · Agent arbeitet' : ''}`
      : '';
  }
  renderArtifact();
}

// Compact list rendering (the counterpart to the bubble cards): one protocol
// row per event — role, one-line preview, status.
function renderTranscriptList(box, events, emptyText) {
  box.innerHTML = '';
  if (!events.length) {
    box.innerHTML = `<div class="ctox-empty"><strong>${escapeHtml(emptyText)}</strong></div>`;
    return;
  }
  const roleLabel = { user: 'Auftrag', assistant: 'Agent', agent: 'Agent' };
  for (const event of events) {
    const label = roleLabel[event.role] || 'System';
    box.insertAdjacentHTML('beforeend', `
      <div class="coding-agents-proto-row" data-role="${escapeHtml(event.role)}">
        <span class="coding-agents-proto-role">${escapeHtml(label)}</span>
        <span class="coding-agents-proto-text">${escapeHtml(String(event.text || '').replace(/\s+/g, ' ').slice(0, 160))}</span>
        ${event.status ? `<span class="coding-agents-proto-status">${escapeHtml(event.status)}</span>` : ''}
      </div>
    `);
  }
}

// The "Delegationen" band view: the turn log for the active app as kit rows.
function renderTurnsList() {
  const box = els.turnsList;
  if (!box) return;
  box.innerHTML = '';
  if (!state.recentTurns.length) {
    box.innerHTML = `<div class="ctox-empty"><strong>Noch keine Delegationen.</strong></div>`;
    return;
  }
  for (const turn of state.recentTurns) {
    box.insertAdjacentHTML('beforeend', `
      <div class="ctox-list-item coding-agents-turn-row">
        <span class="coding-agents-app-row">
          <span class="coding-agents-app-title">${escapeHtml(String(turn.prompt || '').replace(/\s+/g, ' ').slice(0, 120) || '—')}</span>
          <span class="coding-agents-app-id">${escapeHtml(turn.status || '')}${turn.timeMs ? ` · ${escapeHtml(relativeTime(turn.timeMs))}` : ''}</span>
        </span>
      </div>
    `);
  }
}

function relativeTime(timeMs) {
  const delta = Date.now() - Number(timeMs || 0);
  if (!Number.isFinite(delta) || delta < 0) return '';
  const minutes = Math.round(delta / 60000);
  if (minutes < 1) return 'gerade eben';
  if (minutes < 60) return `vor ${minutes} min`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `vor ${hours} h`;
  return `vor ${Math.round(hours / 24)} d`;
}

// The canonical transcript for the active app: native sidecar events when we
// have them, otherwise synthesized user-prompt + status-line pairs from the
// command log — shaped as chat-ui event objects (role/text/status/key).
function buildTranscriptEvents() {
  const sessionEvents = Array.isArray(state.sessionEvents) ? state.sessionEvents : [];
  if (sessionEvents.length) {
    return sessionEvents.map((event, index) => ({
      key: event.seq != null ? `evt:${event.seq}` : `evt:${event.id || index}`,
      role: String(event.role || 'system'),
      text: String(event.text || ''),
      status: String(event.status || ''),
    }));
  }

  const events = [];
  [...state.recentTurns].sort((a, b) => a.timeMs - b.timeMs).forEach((turn) => {
    events.push({ key: `turn:${turn.id}:u`, role: 'user', text: turn.prompt || turn.moduleId });
    const statusLabel = turn.status === 'completed'
      ? t('statusCompleted')
      : turn.status === 'failed'
        ? t('statusFailed')
        : t('statusRunning');
    const detail = [
      statusLabel,
      formatRecordTime(turn.timeMs),
      turn.appliedCount ? `${turn.appliedCount} ${turn.appliedCount === 1 ? t('fileChanged') : t('filesChanged')}` : '',
      turn.error,
    ].filter(Boolean).join(' · ');
    events.push({
      key: `turn:${turn.id}:s`,
      role: 'system',
      text: detail,
      failed: turn.status === 'failed' || !turn.ok,
    });
  });
  return events;
}

// Fallback renderer used only when the vendored chat-ui failed to load. Keeps
// the previous inline .ca-msg presentation so the module never goes blank.
function renderTranscriptInline(box, events, emptyText) {
  box.innerHTML = '';
  if (!events.length) {
    box.innerHTML = `<div class="ctox-empty"><strong>${escapeHtml(emptyText)}</strong></div>`;
    return;
  }
  for (const event of events) {
    const role = String(event.role || 'system');
    if (role === 'user') {
      box.insertAdjacentHTML('beforeend', `<div class="ca-msg is-user"><div class="ca-msg-body">${escapeHtml(event.text || '')}</div></div>`);
    } else if (role === 'assistant' || role === 'agent') {
      box.insertAdjacentHTML('beforeend', `<div class="ca-msg is-agent"><div class="ca-msg-meta">Agent${event.status ? ` · ${escapeHtml(event.status)}` : ''}</div><div class="ca-msg-body">${escapeHtml(event.text || '')}</div></div>`);
    } else {
      box.insertAdjacentHTML('beforeend', `<div class="ca-msg is-event ${event.failed ? 'is-failed' : ''}">${escapeHtml(event.text || '')}${event.status ? ` · ${escapeHtml(event.status)}` : ''}</div>`);
    }
  }
  box.scrollTop = box.scrollHeight;
}

// Column 3: the agent's free HTML artifact — a live page the agent maintains
// about its task (contract: session.metadata.artifact_html). Sandboxed.
function renderArtifact() {
  const frame = els.root?.querySelector('#ca-artifact');
  const empty = els.root?.querySelector('#ca-artifact-empty');
  if (!frame || !empty) return;
  let metadata = state.activeSession?.metadata;
  if (typeof metadata === 'string') { try { metadata = JSON.parse(metadata); } catch { metadata = null; } }
  const html = metadata && typeof metadata === 'object' ? String(metadata.artifact_html || '') : '';
  if (html.trim()) {
    frame.srcdoc = html;
    frame.hidden = false;
    empty.hidden = true;
  } else {
    frame.hidden = true;
    empty.hidden = false;
  }
  if (els.artifactFooter) {
    const updatedMs = Number(state.activeSession?.updated_at_ms || 0);
    els.artifactFooter.textContent = html.trim() && updatedMs ? `Aktualisiert ${relativeTime(updatedMs)}` : '';
  }
}

function renderSessionBanner() {
  const banner = document.createElement('div');
  banner.className = 'ctox-list-item coding-agents-session-banner';
  const session = state.activeSession;
  if (!session) {
    banner.innerHTML = `
      <div class="coding-agents-turn-row-main">
        <span class="coding-agents-turn-row-prompt">${escapeHtml(t('sessionLabel'))} · ${escapeHtml(state.activeModuleId)}</span>
      </div>
      <div class="coding-agents-turn-row-meta">${escapeHtml(t('sessionNone'))}</div>
    `;
    return banner;
  }
  const metaParts = [`${state.recentTurns.length} ${t('sessionTurns')}`];
  const updated = formatRecordTime(Number(session.updated_at_ms || 0));
  if (updated) metaParts.push(updated);
  banner.innerHTML = `
    <div class="coding-agents-turn-row-main">
      <span class="coding-agents-turn-row-prompt">${escapeHtml(t('sessionLabel'))} · ${escapeHtml(state.activeModuleId)}</span>
      <span class="ctox-badge is-success">${escapeHtml(t('sessionActive'))}</span>
    </div>
    <div class="coding-agents-turn-row-meta">${escapeHtml(metaParts.filter(Boolean).join(' · '))}</div>
  `;
  return banner;
}

/* Helpers */

async function readCollectionDocs(collectionName) {
  const collection = state.ctx?.db?.collection?.(collectionName);
  if (!collection) return null;
  if (typeof collection.find === 'function') {
    const query = collection.find();
    const docs = await query?.exec?.();
    if (Array.isArray(docs)) return docs.map(toPlainDoc);
  }
  if (typeof collection.toArray === 'function') {
    return (await collection.toArray()).map(toPlainDoc);
  }
  if (Array.isArray(collection.items)) return collection.items.map(toPlainDoc);
  return null;
}

function toPlainDoc(doc) {
  return typeof doc?.toJSON === 'function' ? doc.toJSON() : doc;
}

function actorContext(session) {
  const user = session?.user || {};
  return {
    id: user.id || '',
    display_name: user.display_name || user.name || user.id || '',
    role: user.role || 'user',
    is_admin: Boolean(user.is_admin),
  };
}

function newId() {
  return globalThis.crypto?.randomUUID?.() || `${Date.now()}_${Math.random().toString(36).slice(2)}`;
}

function formatRecordTime(value) {
  const ms = Number(value || 0);
  if (!Number.isFinite(ms) || ms <= 0) return '';
  return new Date(ms).toLocaleString();
}

function setHint(text, isError = false) {
  if (!els.hint) return;
  els.hint.textContent = text || '';
  els.hint.classList.toggle('is-error', Boolean(isError));
}

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

export const __codingAgentsTestHooks = {
  MODEL_PRESETS,
  DEFAULT_MODEL_PRESET,
  CODING_TURN_COMMAND,
  modelPresetById,
  normalizeCatalogModules,
  validateTaskPrompt,
  buildTurnPayload,
  turnFromCommand,
  turnErrorFromProjection,
  escapeHtml,
};
