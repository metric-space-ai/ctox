import { createBusinessDb } from './shared/db.js';
import { createSyncRuntime } from './shared/sync.js?v=20260518-localfirst2';
import { createCommandBus } from './shared/command-bus.js';
import { openReactSettings } from './shared/react-settings.js?v=20260517-auth3';
import { initBusinessReporter } from './shared/business-reporter.js?v=20260517-roles1';

const SESSION_TOKEN_KEY = 'ctox.businessOs.sessionToken';
const AUTH_HEADER_KEY = 'ctox.businessOs.authHeader';
const LOGGED_OUT_KEY = 'ctox.businessOs.loggedOut';
const ACCOUNT_PREFS_KEY = 'ctox.businessOs.accountPreferences';
const MODULE_LAYOUT_KEY = 'ctox.businessOs.moduleLayout';
const SHELL_COLUMN_LAYOUT_KEY_PREFIX = 'ctox.businessOs.shellColumnLayout.';
const APP_BUILD = '20260518-ctox-communications1';
const FETCH_TIMEOUT_MS = 1500;
let moduleLayoutSaveTimer = null;
let shellColumnResizeSync = null;

const SHELL_COL_MIN = {
  left: 210,
  center: 420,
  right: 260,
};

const SHELL_COL_SIDE_MAX = 620;

const state = {
  modules: [],
  activeModule: null,
  activeUnmount: null,
  db: null,
  sync: null,
  commandBus: null,
  session: null,
  governance: null,
  moduleLayout: null,
  schemaRegistrations: new Map(),
  schemaRegistrationQueue: Promise.resolve(),
  syncStartedModules: new Set(),
  backgroundModuleWorkScheduled: false,
};

const moduleAliases = {};

const shellMessages = {
  de: {
    context: 'Kontext',
    topics: 'Themen',
    loadingWorkspace: 'Workspace wird geladen',
    loadingModule: 'Modul-Workspace wird geladen.',
    localWorkspace: 'Lokaler Workspace',
    loginRequired: 'Login erforderlich',
    startupChecking: 'CTOX-Sitzung prüfen',
    syncConnecting: 'Sync-Verbindungen starten',
    collection: 'Collection',
    activity: 'Aktivität',
    agentContext: 'Agent-Kontext',
    webrtcSync: 'WebRTC-Sync',
    moduleTitles: {
      ctox: 'CTOX',
      documents: 'Dokumente',
      knowledge: 'Knowledge',
      'matching': 'Matching',
    },
  },
  en: {
    context: 'Context',
    topics: 'Topics',
    loadingWorkspace: 'Loading workspace',
    loadingModule: 'Loading module workspace.',
    localWorkspace: 'Local workspace',
    loginRequired: 'Login required',
    startupChecking: 'Checking CTOX session',
    syncConnecting: 'Connecting sync peers',
    collection: 'Collection',
    activity: 'Activity',
    agentContext: 'Agent context',
    webrtcSync: 'WebRTC sync',
    moduleTitles: {
      ctox: 'CTOX',
      documents: 'Documents',
      knowledge: 'Knowledge',
      'matching': 'Matching',
    },
  },
};

const els = {
  status: document.querySelector('[data-status-text]'),
  tabs: document.querySelector('[data-module-tabs]'),
  host: document.querySelector('[data-module-host]'),
  leftContent: document.querySelector('[data-left-content]'),
  rightContent: document.querySelector('[data-right-content]'),
  backdrop: document.querySelector('[data-backdrop]'),
  leftDrawer: document.querySelector('[data-drawer-left]'),
  rightDrawer: document.querySelector('[data-drawer-right]'),
  bottomDrawer: document.querySelector('[data-drawer-bottom]'),
  accountButton: document.querySelector('[data-open-account]'),
  languageSelect: document.querySelector('[data-language-select]'),
  themeSelect: document.querySelector('[data-theme-select]'),
};

bootstrap().catch((error) => {
  console.error(error);
  setStatus(`Startup failed: ${error.message || error}`);
});

async function bootstrap() {
  const prefs = readAccountPrefs();
  applyShellTheme(prefs.theme || 'dark', { persist: false });
  applyShellLanguage(prefs.language || 'de', { persist: false });
  syncHeaderControls();
  wireShellActions();
  setStatus(shellText('startupChecking'));
  const session = await loadSession();
  state.session = session;
  renderAccountButton(session);
  if (!session.authenticated) {
    renderLoginGate(session);
    setStatus(shellText('loginRequired'));
    return;
  }
  state.db = await createBusinessDb({ name: 'ctox_business_os_v3' });
  setStatus(shellText('syncConnecting'));
  const [modules, syncConfig] = await Promise.all([
    loadModules(),
    loadSyncConfig(),
  ]);
  state.sync = createSyncRuntime({
    db: state.db,
    baseUrl: '/api/business-os',
    config: syncConfig,
  });
  state.commandBus = createCommandBus({
    baseUrl: '/api/business-os',
    db: state.db,
    config: syncConfig,
  });
  state.modules = modules.modules || [];
  state.governance = modules.governance || null;
  state.moduleLayout = normalizeModuleLayout(await loadModuleLayout(), state.modules);
  persistModuleLayout();
  renderTabs();
  setStatus(shellText('localWorkspace'));
  initBusinessReporter({
    session: state.session,
    getActiveModule: () => state.activeModule,
    authHeaders: businessOsAuthHeaders,
    endpoint: '/api/business-os/reports',
  });
  await openModule(currentHashModuleId() || state.modules[0]?.id || 'ctox');
  scheduleBackgroundModuleWork();
  if (state.sync?.config?.http_bridge_available !== false) {
    refreshRemoteShellStateInBackground();
    loadStatus().catch(() => null);
  }
}

function wireShellActions() {
  window.addEventListener('unhandledrejection', (event) => {
    if (!isVolatileSyncTransportError(event.reason)) return;
    console.warn('[business-os] ignored volatile local sync transport error', event.reason);
    event.preventDefault();
  });
  window.addEventListener('error', (event) => {
    if (!isVolatileSyncTransportError(event.error || event.message)) return;
    console.warn('[business-os] ignored volatile local sync transport error', event.error || event.message);
    event.preventDefault();
  });
  window.addEventListener('message', (event) => {
    if (event.data?.type === 'ctox-business-os-command') {
      handleModuleCommand(event);
    }
  });
  window.addEventListener('hashchange', () => {
    const id = currentHashModuleId();
    if (id) openModule(id);
  });
  document.querySelector('[data-open-settings]')?.addEventListener('click', () => {
    els.rightDrawer.classList.remove('account-popover');
    openReactSettings({
      mount: els.rightDrawer,
      modules: state.modules,
      session: state.session,
      governance: state.governance,
      syncConfig: state.sync?.config,
      commandBus: state.commandBus,
      onAccount: openAccountDrawer,
      onClose: closeDrawers,
      onModulesChanged: refreshModules,
    });
    showBackdrop();
  });
  els.accountButton?.addEventListener('click', openAccountDrawer);
  els.languageSelect?.addEventListener('change', () => {
    applyShellLanguage(els.languageSelect.value);
    syncHeaderControls();
    postCurrentPreferencesToModule();
  });
  els.themeSelect?.addEventListener('change', () => {
    applyShellTheme(els.themeSelect.value);
    syncHeaderControls();
    postCurrentPreferencesToModule();
  });
  els.backdrop?.addEventListener('click', closeDrawers);
  els.tabs.addEventListener('dragover', (event) => {
    if (!draggedModuleId(event)) return;
    event.preventDefault();
    els.tabs.classList.add('is-drop-end');
  });
  els.tabs.addEventListener('dragleave', (event) => {
    if (!els.tabs.contains(event.relatedTarget)) els.tabs.classList.remove('is-drop-end');
  });
  els.tabs.addEventListener('drop', (event) => {
    const moduleId = draggedModuleId(event);
    if (!moduleId || moduleId === 'ctox') return;
    event.preventDefault();
    els.tabs.classList.remove('is-drop-end');
    moveModuleToUngrouped(moduleId);
  });
  window.addEventListener('beforeunload', () => {
    state.db?.close?.();
  });
  shellColumnResizeSync = setupShellColumnResizing();
}

function isVolatileSyncTransportError(error) {
  const text = String(error?.message || error || '');
  return /cannot send after peer is destroyed|ERR_DATA_CHANNEL|Failure to send data|User-Initiated Abort/i.test(text);
}

function setupShellColumnResizing() {
  const frame = document.querySelector('.workspace-frame');
  if (!frame) return null;

  const leftHandle = document.createElement('div');
  leftHandle.className = 'workspace-col-resizer workspace-col-resizer-left';
  leftHandle.setAttribute('role', 'separator');
  leftHandle.setAttribute('aria-orientation', 'vertical');
  leftHandle.setAttribute('aria-label', 'Linke und mittlere Spalte anpassen');

  const rightHandle = document.createElement('div');
  rightHandle.className = 'workspace-col-resizer workspace-col-resizer-right';
  rightHandle.setAttribute('role', 'separator');
  rightHandle.setAttribute('aria-orientation', 'vertical');
  rightHandle.setAttribute('aria-label', 'Mittlere und rechte Spalte anpassen');

  frame.append(leftHandle, rightHandle);

  let activeWidths = null;
  let persistedRatios = null;
  let dragState = null;
  let resizeRaf = 0;
  let currentLayoutKey = '';

  function layoutKey() {
    return state.activeModule?.id
      ? `${SHELL_COLUMN_LAYOUT_KEY_PREFIX}${state.activeModule.id}`
      : '';
  }

  function readPersistedRatios() {
    const key = layoutKey();
    if (!key || key === currentLayoutKey) return persistedRatios;
    currentLayoutKey = key;
    try {
      persistedRatios = sanitizeColumnLayoutRatios(JSON.parse(localStorage.getItem(key) || 'null'));
    } catch {
      persistedRatios = null;
    }
    return persistedRatios;
  }

  function isResizableLayout() {
    if (!state.activeModule || moduleUsesFullWorkspace(state.activeModule)) return false;
    if (document.body.dataset.moduleLoading) return false;
    if (document.body.dataset.authState === 'locked') return false;
    return Boolean(readGridTrackPixels(frame));
  }

  function hideHandles() {
    leftHandle.hidden = true;
    rightHandle.hidden = true;
    leftHandle.classList.remove('is-active');
    rightHandle.classList.remove('is-active');
  }

  function showHandles() {
    leftHandle.hidden = false;
    rightHandle.hidden = false;
  }

  function applyWidths(widths) {
    if (!widths) return;
    frame.style.gridTemplateColumns = `${widths.left}px ${widths.center}px ${widths.right}px`;
  }

  function placeHandles(metrics, widths) {
    if (!metrics || !widths) return;
    leftHandle.style.left = `${Math.round(widths.left + (metrics.gap / 2))}px`;
    rightHandle.style.left = `${Math.round(widths.left + metrics.gap + widths.center + (metrics.gap / 2))}px`;
  }

  function persistCurrentLayout() {
    const key = layoutKey();
    const ratios = columnPixelsToRatios(activeWidths);
    if (!key || !ratios) return;
    persistedRatios = ratios;
    currentLayoutKey = key;
    localStorage.setItem(key, JSON.stringify(ratios));
  }

  function syncLayout() {
    if (!isResizableLayout()) {
      frame.style.removeProperty('grid-template-columns');
      hideHandles();
      activeWidths = null;
      return;
    }

    const metrics = getGridMetrics(frame);
    if (!metrics || metrics.trackTotal <= 0) return;

    let nextWidths = readPersistedRatios()
      ? columnRatiosToPixels(persistedRatios, metrics.trackTotal)
      : null;

    if (!nextWidths) {
      nextWidths = clampShellColumns(readGridTrackPixels(frame), metrics.trackTotal);
    }

    if (!nextWidths) return;
    activeWidths = nextWidths;
    applyWidths(activeWidths);
    placeHandles(metrics, activeWidths);
    showHandles();
  }

  function stopDrag() {
    if (!dragState) return;
    dragState = null;
    leftHandle.classList.remove('is-active');
    rightHandle.classList.remove('is-active');
    document.body.classList.remove('is-workspace-col-resizing');
    persistCurrentLayout();
  }

  function startDrag(which, event) {
    if (!isResizableLayout()) return;
    const metrics = getGridMetrics(frame);
    if (!metrics || metrics.trackTotal <= 0) return;
    const initial = activeWidths || clampShellColumns(readGridTrackPixels(frame), metrics.trackTotal);
    if (!initial) return;

    activeWidths = initial;
    dragState = {
      which,
      frameRect: frame.getBoundingClientRect(),
      metrics,
      widths: { ...initial },
    };

    if (which === 'left') leftHandle.classList.add('is-active');
    if (which === 'right') rightHandle.classList.add('is-active');
    document.body.classList.add('is-workspace-col-resizing');
    event.preventDefault();
  }

  function handleDragMove(event) {
    if (!dragState) return;
    const { which, frameRect, metrics, widths } = dragState;
    const pointerX = event.clientX - frameRect.left - metrics.padLeft;
    const boundedX = clampNumber(pointerX, 0, metrics.contentWidth);

    if (which === 'left') {
      const right = widths.right;
      const maxLeft = Math.max(
        SHELL_COL_MIN.left,
        Math.min(SHELL_COL_SIDE_MAX, metrics.trackTotal - right - SHELL_COL_MIN.center)
      );
      const left = clampNumber(boundedX - (metrics.gap / 2), SHELL_COL_MIN.left, maxLeft);
      activeWidths = clampShellColumns({ left, center: metrics.trackTotal - left - right, right }, metrics.trackTotal);
    } else {
      const left = widths.left;
      const maxRight = Math.max(
        SHELL_COL_MIN.right,
        Math.min(SHELL_COL_SIDE_MAX, metrics.trackTotal - left - SHELL_COL_MIN.center)
      );
      const right = clampNumber(metrics.contentWidth - boundedX - (metrics.gap / 2), SHELL_COL_MIN.right, maxRight);
      activeWidths = clampShellColumns({ left, center: metrics.trackTotal - left - right, right }, metrics.trackTotal);
    }

    if (!activeWidths) return;
    applyWidths(activeWidths);
    placeHandles(metrics, activeWidths);
  }

  function handleResize() {
    if (resizeRaf) cancelAnimationFrame(resizeRaf);
    resizeRaf = requestAnimationFrame(() => {
      resizeRaf = 0;
      syncLayout();
    });
  }

  leftHandle.addEventListener('pointerdown', (event) => startDrag('left', event));
  rightHandle.addEventListener('pointerdown', (event) => startDrag('right', event));
  window.addEventListener('pointermove', handleDragMove);
  window.addEventListener('pointerup', stopDrag);
  window.addEventListener('pointercancel', stopDrag);
  window.addEventListener('blur', stopDrag);
  window.addEventListener('resize', handleResize);

  syncLayout();
  return syncLayout;
}

function readGridTrackPixels(gridEl) {
  if (!gridEl) return null;
  const tracks = String(getComputedStyle(gridEl).gridTemplateColumns || '')
    .split(/\s+/)
    .map((part) => Number.parseFloat(part))
    .filter((value) => Number.isFinite(value) && value > 0);
  if (tracks.length < 3) return null;
  return {
    left: tracks[0],
    center: tracks[1],
    right: tracks[2],
  };
}

function getGridMetrics(gridEl) {
  if (!gridEl) return null;
  const styles = getComputedStyle(gridEl);
  const gap = Number.parseFloat(styles.columnGap || styles.gap || '0') || 0;
  const padLeft = Number.parseFloat(styles.paddingLeft || '0') || 0;
  const padRight = Number.parseFloat(styles.paddingRight || '0') || 0;
  const contentWidth = Math.max(0, gridEl.clientWidth - padLeft - padRight);
  return {
    gap,
    padLeft,
    contentWidth,
    trackTotal: Math.max(0, contentWidth - (gap * 2)),
  };
}

function clampShellColumns(widths, trackTotal) {
  if (!widths || !Number.isFinite(trackTotal) || trackTotal <= 0) return null;
  const maxLeft = Math.max(
    SHELL_COL_MIN.left,
    Math.min(SHELL_COL_SIDE_MAX, trackTotal - SHELL_COL_MIN.center - SHELL_COL_MIN.right)
  );
  const maxRight = Math.max(
    SHELL_COL_MIN.right,
    Math.min(SHELL_COL_SIDE_MAX, trackTotal - SHELL_COL_MIN.center - SHELL_COL_MIN.left)
  );

  let left = clampNumber(Number(widths.left) || SHELL_COL_MIN.left, SHELL_COL_MIN.left, maxLeft);
  let right = clampNumber(Number(widths.right) || SHELL_COL_MIN.right, SHELL_COL_MIN.right, maxRight);
  let center = trackTotal - left - right;

  if (center < SHELL_COL_MIN.center) {
    let shortage = SHELL_COL_MIN.center - center;
    const reduceLeft = Math.min(shortage, left - SHELL_COL_MIN.left);
    left -= reduceLeft;
    shortage -= reduceLeft;
    if (shortage > 0) {
      const reduceRight = Math.min(shortage, right - SHELL_COL_MIN.right);
      right -= reduceRight;
    }
    center = trackTotal - left - right;
  }

  if (center < SHELL_COL_MIN.center) return null;
  return {
    left: Math.round(left),
    center: Math.round(center),
    right: Math.round(right),
  };
}

function sanitizeColumnLayoutRatios(raw) {
  if (!raw || typeof raw !== 'object') return null;
  const left = Number(raw.left);
  const center = Number(raw.center);
  const right = Number(raw.right);
  if (![left, center, right].every(Number.isFinite)) return null;
  if (left <= 0 || center <= 0 || right <= 0) return null;
  const sum = left + center + right;
  if (sum <= 0) return null;
  return {
    left: left / sum,
    center: center / sum,
    right: right / sum,
  };
}

function columnPixelsToRatios(widths) {
  if (!widths) return null;
  const left = Number(widths.left) || 0;
  const center = Number(widths.center) || 0;
  const right = Number(widths.right) || 0;
  const sum = left + center + right;
  if (sum <= 0) return null;
  return {
    left: Number((left / sum).toFixed(6)),
    center: Number((center / sum).toFixed(6)),
    right: Number((right / sum).toFixed(6)),
  };
}

function columnRatiosToPixels(ratios, trackTotal) {
  const safe = sanitizeColumnLayoutRatios(ratios);
  if (!safe) return null;
  return clampShellColumns({
    left: safe.left * trackTotal,
    center: safe.center * trackTotal,
    right: safe.right * trackTotal,
  }, trackTotal);
}

function clampNumber(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

function shellLang() {
  return document.documentElement.lang === 'en' ? 'en' : 'de';
}

function shellText(key) {
  return shellMessages[shellLang()]?.[key] || shellMessages.en[key] || key;
}

function applyShellTheme(theme, options = {}) {
  const value = theme === 'light' ? 'light' : 'dark';
  document.documentElement.dataset.theme = value;
  if (options.persist !== false) {
    writeAccountPrefs({ theme: value });
  }
}

function syncHeaderControls() {
  if (els.languageSelect) {
    els.languageSelect.value = shellLang();
  }
  if (els.themeSelect) {
    els.themeSelect.value = document.documentElement.dataset.theme === 'light' ? 'light' : 'dark';
  }
}

function localizeShellChrome() {
  document.querySelector('[data-left-pane] > .pane-title')?.replaceChildren(document.createTextNode(shellText('context')));
  document.querySelector('[data-right-pane] > .pane-title')?.replaceChildren(document.createTextNode(shellText('topics')));
}

async function handleModuleCommand(event) {
  const requestId = event.data?.requestId || `cmdreq_${crypto.randomUUID()}`;
  try {
    if (!state.commandBus) throw new Error('Command bus is not ready');
    const command = event.data.command || {};
    const result = await state.commandBus.dispatch({
      ...command,
      client_context: {
        ...(command.client_context || {}),
        module_surface: event.data.surface || 'module-frame',
        shell_module: state.activeModule?.id || '',
      },
    });
    event.source?.postMessage({
      type: 'ctox-business-os-command-result',
      requestId,
      ok: true,
      result,
    }, '*');
  } catch (error) {
    event.source?.postMessage({
      type: 'ctox-business-os-command-result',
      requestId,
      ok: false,
      error: String(error?.message || error),
    }, '*');
  }
}

function applyShellLanguage(lang, options = {}) {
  const value = lang === 'en' ? 'en' : 'de';
  document.documentElement.lang = value;
  if (options.persist !== false) {
    writeAccountPrefs({ language: value });
  }
}

function postCurrentPreferencesToModule() {
  const detail = {
    theme: document.documentElement.dataset.theme === 'light' ? 'light' : 'dark',
    language: document.documentElement.lang === 'en' ? 'en' : 'de',
  };
  window.dispatchEvent(new CustomEvent('ctox-business-os-preferences', { detail }));
  window.postMessage({ type: 'ctox-business-os-language', lang: detail.language }, '*');
  for (const frame of els.host?.querySelectorAll?.('iframe') || []) {
    frame.contentWindow?.postMessage({ type: 'ctox-business-os-preferences', ...detail }, '*');
  }
}

function renderTabs() {
  els.tabs.replaceChildren();
  state.moduleLayout = normalizeModuleLayout(state.moduleLayout || readModuleLayout(), state.modules);
  const modulesById = new Map(state.modules.map((mod) => [mod.id, mod]));
  const ctox = modulesById.get('ctox');
  if (ctox) {
    els.tabs.append(renderModuleTab(ctox, { locked: true }));
  }

  for (const moduleId of state.moduleLayout.ungrouped) {
    const mod = modulesById.get(moduleId);
    if (mod) els.tabs.append(renderModuleTab(mod));
  }
  for (const group of state.moduleLayout.groups) {
    const visibleItems = group.items.filter((moduleId) => modulesById.has(moduleId));
    if (visibleItems.length) {
      els.tabs.append(renderModuleGroup({ ...group, items: visibleItems }, modulesById));
    }
  }
}

function renderModuleTab(mod, options = {}) {
  const button = document.createElement('button');
  button.className = 'module-tab';
  button.type = 'button';
  button.textContent = moduleDisplayTitle(mod);
  button.dataset.module = mod.id;
  if (options.locked) {
    button.dataset.locked = 'true';
    button.draggable = false;
  } else {
    button.draggable = true;
    button.addEventListener('dragstart', (event) => {
      event.dataTransfer.effectAllowed = 'move';
      event.dataTransfer.setData('application/x-ctox-module', mod.id);
      event.dataTransfer.setData('text/plain', mod.id);
      button.classList.add('is-dragging');
    });
    button.addEventListener('dragend', () => {
      button.classList.remove('is-dragging');
    });
    button.addEventListener('dragover', (event) => {
      if (!draggedModuleId(event)) return;
      event.preventDefault();
      button.classList.add('is-drop-before');
    });
    button.addEventListener('dragleave', () => button.classList.remove('is-drop-before'));
    button.addEventListener('drop', (event) => {
      const moduleId = draggedModuleId(event);
      if (!moduleId || moduleId === 'ctox') return;
      event.preventDefault();
      event.stopPropagation();
      button.classList.remove('is-drop-before');
      moveModuleBefore(moduleId, mod.id);
    });
    button.addEventListener('contextmenu', (event) => {
      event.preventDefault();
      if (canModifyModule(mod)) openModuleEditDrawer(mod);
    });
    button.addEventListener('dblclick', () => {
      if (canModifyModule(mod)) openModuleEditDrawer(mod);
    });
  }
  button.addEventListener('click', () => {
    location.hash = mod.id;
    openModule(mod.id);
  });
  return button;
}

function renderModuleGroup(group, modulesById) {
  const wrap = document.createElement('details');
  wrap.className = 'module-group';
  wrap.dataset.groupId = group.id;
  const activeInside = group.items.includes(state.activeModule?.id);
  if (activeInside) wrap.dataset.active = 'true';
  wrap.addEventListener('dragover', (event) => {
    if (!draggedModuleId(event)) return;
    event.preventDefault();
    wrap.classList.add('is-drop-target');
  });
  wrap.addEventListener('dragleave', () => wrap.classList.remove('is-drop-target'));
  wrap.addEventListener('drop', (event) => {
    const moduleId = draggedModuleId(event);
    if (!moduleId || moduleId === 'ctox') return;
    event.preventDefault();
    event.stopPropagation();
    wrap.classList.remove('is-drop-target');
    moveModuleToGroup(moduleId, group.id);
  });

  const summary = document.createElement('summary');
  summary.className = 'module-group-summary';
  summary.innerHTML = `<span>${escapeHtml(group.title || 'Gruppe')}</span><small>${group.items.length}</small>`;
  summary.addEventListener('contextmenu', (event) => {
    event.preventDefault();
    openGroupEditDrawer(group.id);
  });
  summary.addEventListener('dblclick', (event) => {
    event.preventDefault();
    openGroupEditDrawer(group.id);
  });
  wrap.append(summary);

  const menu = document.createElement('div');
  menu.className = 'module-group-menu';
  for (const moduleId of group.items) {
    const mod = modulesById.get(moduleId);
    if (mod) menu.append(renderModuleTab(mod));
  }
  if (!menu.childElementCount) {
    const empty = document.createElement('span');
    empty.className = 'module-group-empty';
    empty.textContent = 'Leere Gruppe';
    menu.append(empty);
  }
  wrap.append(menu);
  return wrap;
}

async function openModule(moduleId, options = {}) {
  moduleId = String(moduleId || '').split('?')[0];
  const requestedId = moduleAliases[moduleId] || moduleId;
  if (requestedId !== moduleId && currentHashModuleId() === moduleId) {
    history.replaceState(null, '', `#${requestedId}`);
  }
  const mod = state.modules.find((item) => item.id === requestedId) || state.modules[0];
  if (!mod) return;
  if (state.activeModule?.id === mod.id && !options.force) return;
  if (typeof state.activeUnmount === 'function') {
    await state.activeUnmount();
  }
  state.activeModule = mod;
  state.activeUnmount = null;
  document.body.dataset.activeModule = mod.id;
  document.body.dataset.moduleShell = moduleUsesFullWorkspace(mod) ? 'full' : 'pane';
  document.body.dataset.moduleLoading = mod.id;
  shellColumnResizeSync?.();
  for (const button of els.tabs.querySelectorAll('[data-module]')) {
    button.setAttribute('aria-current', button.dataset.module === mod.id ? 'page' : 'false');
  }
  els.host.replaceChildren(renderModuleFrame(mod));
  els.leftContent.replaceChildren(renderLeftContext(mod));
  els.rightContent.replaceChildren(renderRightContext(mod));
  try {
    await registerModuleSchemas(mod);
  } catch (error) {
    console.error(`[business-os] Schema registration failed for ${mod.id}`, error);
    setStatus(`Schema warning: ${error.message || error}`);
  }
  try {
    const moduleScript = await import(`./${moduleBasePath(mod)}/index.js?v=${APP_BUILD}`);
    if (typeof moduleScript.mount === 'function') {
      state.activeUnmount = await moduleScript.mount(createModuleContext(mod));
    }
  } finally {
    delete document.body.dataset.moduleLoading;
    shellColumnResizeSync?.();
  }
  postCurrentPreferencesToModule();
  startModuleSync(mod);
}

function moduleUsesFullWorkspace(mod) {
  return mod.id === 'ctox'
    || mod.id === 'matching'
    || mod.layout?.shell === 'full-workspace'
    || mod.layout?.full_workspace === true
    || mod.layout?.fullFrame === true;
}

function currentHashModuleId() {
  return location.hash.replace(/^#/, '').split('?')[0];
}

async function registerModuleSchemas(mod) {
  if (!mod?.id || !state.db) return;
  if (state.schemaRegistrations.has(mod.id)) {
    return state.schemaRegistrations.get(mod.id);
  }
  const registration = (async () => {
    const schemaModule = await import(`./${moduleBasePath(mod)}/schema.js?v=${APP_BUILD}`);
    if (schemaModule.collections) {
      const nextRegistration = state.schemaRegistrationQueue
        .catch(() => {})
        .then(() => state.db.addCollections(schemaModule.collections));
      state.schemaRegistrationQueue = nextRegistration.catch(() => {});
      await nextRegistration;
    }
  })().catch((error) => {
    state.schemaRegistrations.delete(mod.id);
    throw error;
  });
  state.schemaRegistrations.set(mod.id, registration);
  return registration;
}

function startAllModuleSync() {
  const modules = state.modules.filter((mod) => mod.id !== state.activeModule?.id);
  modules.forEach((mod, index) => {
    window.setTimeout(() => startModuleSync(mod), index * 350);
  });
}

function startModuleSync(mod) {
  if (!mod?.id || !state.sync || state.syncStartedModules.has(mod.id)) return;
  state.syncStartedModules.add(mod.id);
  registerModuleSchemas(mod)
    .then(() => state.sync.startModule(mod))
    .catch((error) => {
      state.syncStartedModules.delete(mod.id);
      console.error(`[business-os] Sync startup failed for ${mod.id}`, error);
      setStatus(`Sync failed: ${error.message || error}`);
    });
}

function preloadModuleScripts() {
  const modules = state.modules.filter((mod) => mod.id !== state.activeModule?.id);
  for (const [index, mod] of modules.entries()) {
    const href = `./${moduleBasePath(mod)}/index.js?v=${APP_BUILD}`;
    if (document.head.querySelector(`link[rel="modulepreload"][href="${href}"]`)) continue;
    window.setTimeout(() => {
      if (document.head.querySelector(`link[rel="modulepreload"][href="${href}"]`)) return;
      const link = document.createElement('link');
      link.rel = 'modulepreload';
      link.href = href;
      document.head.append(link);
    }, index * 250);
  }
}

function scheduleBackgroundModuleWork() {
  if (state.backgroundModuleWorkScheduled) return;
  state.backgroundModuleWorkScheduled = true;
  const run = () => {
    preloadModuleScripts();
    startAllModuleSync();
  };
  if ('requestIdleCallback' in window) {
    window.requestIdleCallback(run, { timeout: 3000 });
  } else {
    window.setTimeout(run, 1200);
  }
}

function moduleBasePath(mod) {
  const entry = String(mod.entry || `modules/${mod.id}/index.html`)
    .replace(/^\.?\//, '')
    .split('?')[0]
    .split('#')[0];
  const slash = entry.lastIndexOf('/');
  return slash >= 0 ? entry.slice(0, slash) : `modules/${mod.id}`;
}

function createModuleContext(mod) {
  return {
    module: mod,
    locale: document.documentElement.lang === 'en' ? 'en' : 'de',
    host: els.host.querySelector('[data-module-root]'),
    left: els.leftContent,
    right: els.rightContent,
    db: state.db,
    sync: state.sync,
    commandBus: state.commandBus,
    syncConfig: state.sync.config,
    session: state.session,
    governance: state.governance,
    canModifyModule: () => canModifyModule(mod),
    reportIssue: (details = {}) => reportCurrentModule({ module: mod, ...details }),
    openLeftDrawer: (content) => openDrawer('left', content),
    openRightDrawer: (content) => openDrawer('right', content),
    openBottomDrawer: (content) => openDrawer('bottom', content),
    closeDrawers,
  };
}

function renderModuleFrame(mod) {
  const root = document.createElement('div');
  root.className = 'module-root';
  root.dataset.moduleRoot = mod.id;
  if (mod.id === 'documents') {
    root.innerHTML = renderDocumentsLoadingShell(moduleDisplayTitle(mod), shellText('loadingModule'));
    return root;
  }
  root.innerHTML = `
    <div class="empty-state module-loading-state" aria-busy="true">
      <strong>${escapeHtml(moduleDisplayTitle(mod))}</strong>
      <span>${escapeHtml(shellText('loadingModule'))}</span>
    </div>
  `;
  return root;
}

function renderDocumentsLoadingShell(title, subtitle) {
  const safeTitle = escapeHtml(title || 'Documents');
  const safeSubtitle = escapeHtml(subtitle || shellText('loadingModule'));
  return `
    <div class="module-loading-shell module-loading-shell-documents" aria-busy="true">
      <section class="module-loading-panel" aria-hidden="true">
        <div class="module-loading-panel-head"><span></span><i></i></div>
        <div class="module-loading-tools"><b></b><b></b></div>
        <div class="module-loading-list"><b></b><b></b><b></b></div>
      </section>
      <section class="module-loading-panel module-loading-panel-main">
        <div class="module-loading-panel-head"><span></span><i></i></div>
        <div class="module-loading-copy">
          <strong>${safeTitle}</strong>
          <span>${safeSubtitle}</span>
        </div>
        <div class="module-loading-document" aria-hidden="true"><b></b><b></b><b></b><b></b></div>
      </section>
      <section class="module-loading-panel" aria-hidden="true">
        <div class="module-loading-panel-head"><span></span><i></i></div>
        <div class="module-loading-list is-compact"><b></b><b></b><b></b></div>
      </section>
    </div>
  `;
}

function readModuleLayout() {
  try {
    return JSON.parse(localStorage.getItem(MODULE_LAYOUT_KEY) || '{}') || {};
  } catch {
    return {};
  }
}

function persistModuleLayout() {
  const layout = state.moduleLayout || {
    version: 1,
    labels: {},
    ungrouped: [],
    groups: [],
  };
  localStorage.setItem(MODULE_LAYOUT_KEY, JSON.stringify(layout));
  clearTimeout(moduleLayoutSaveTimer);
  moduleLayoutSaveTimer = null;
}

function normalizeModuleLayout(layout, modules) {
  const movableIds = modules
    .map((mod) => mod.id)
    .filter((id) => id && id !== 'ctox');
  const movable = new Set(movableIds);
  const next = {
    version: 1,
    labels: Object.fromEntries(
      Object.entries(layout?.labels || {}).filter(([id]) => movable.has(id))
    ),
    ungrouped: [],
    groups: [],
  };
  const seen = new Set();

  for (const rawGroup of layout?.groups || []) {
    const id = sanitizeClientId(rawGroup.id || `group-${crypto.randomUUID()}`);
    const items = [];
    for (const moduleId of rawGroup.items || []) {
      if (!movable.has(moduleId) || seen.has(moduleId)) continue;
      seen.add(moduleId);
      items.push(moduleId);
    }
    next.groups.push({
      id,
      title: String(rawGroup.title || 'Gruppe').trim() || 'Gruppe',
      open: rawGroup.open !== false,
      items,
    });
  }
  for (const moduleId of layout?.ungrouped || []) {
    if (!movable.has(moduleId) || seen.has(moduleId)) continue;
    seen.add(moduleId);
    next.ungrouped.push(moduleId);
  }
  for (const moduleId of movableIds) {
    if (!seen.has(moduleId)) {
      seen.add(moduleId);
      next.ungrouped.push(moduleId);
    }
  }
  return next;
}

function moduleDisplayTitle(mod) {
  return state.moduleLayout?.labels?.[mod.id] || mod.title || mod.id;
}

function draggedModuleId(event) {
  const moduleId = event.dataTransfer?.getData('application/x-ctox-module')
    || event.dataTransfer?.getData('text/plain');
  return moduleId && moduleId !== 'ctox' ? moduleId : '';
}

function removeModuleFromLayout(moduleId) {
  state.moduleLayout.ungrouped = state.moduleLayout.ungrouped.filter((id) => id !== moduleId);
  for (const group of state.moduleLayout.groups) {
    group.items = group.items.filter((id) => id !== moduleId);
  }
}

function moveModuleBefore(moduleId, beforeModuleId) {
  if (moduleId === beforeModuleId || moduleId === 'ctox') return;
  removeModuleFromLayout(moduleId);
  for (const group of state.moduleLayout.groups) {
    const index = group.items.indexOf(beforeModuleId);
    if (index >= 0) {
      group.items.splice(index, 0, moduleId);
      persistModuleLayout();
      renderTabs();
      return;
    }
  }
  const index = state.moduleLayout.ungrouped.indexOf(beforeModuleId);
  if (index >= 0) {
    state.moduleLayout.ungrouped.splice(index, 0, moduleId);
  } else {
    state.moduleLayout.ungrouped.push(moduleId);
  }
  persistModuleLayout();
  renderTabs();
}

function moveModuleToGroup(moduleId, groupId) {
  if (moduleId === 'ctox') return;
  removeModuleFromLayout(moduleId);
  const group = state.moduleLayout.groups.find((item) => item.id === groupId);
  if (!group) return;
  group.items.push(moduleId);
  group.open = true;
  persistModuleLayout();
  renderTabs();
}

function moveModuleToUngrouped(moduleId) {
  if (moduleId === 'ctox') return;
  removeModuleFromLayout(moduleId);
  state.moduleLayout.ungrouped.push(moduleId);
  persistModuleLayout();
  renderTabs();
}

function createModuleGroup(title = 'Neue Gruppe') {
  const group = {
    id: `group-${crypto.randomUUID()}`,
    title,
    open: true,
    items: [],
  };
  state.moduleLayout.groups.push(group);
  persistModuleLayout();
  renderTabs();
  return group;
}

function renameModule(moduleId, label) {
  if (moduleId === 'ctox') return;
  const mod = state.modules.find((item) => item.id === moduleId);
  const fallback = mod?.title || mod?.id || '';
  const trimmed = label.trim();
  if (!trimmed || trimmed === fallback) {
    delete state.moduleLayout.labels[moduleId];
  } else {
    state.moduleLayout.labels[moduleId] = trimmed;
  }
  persistModuleLayout();
  renderTabs();
  if (state.activeModule?.id === moduleId) {
    els.host.querySelector('.empty-state strong')?.replaceChildren(document.createTextNode(trimmed || fallback));
  }
}

function openModuleEditDrawer(mod) {
  if (!canModifyModule(mod)) return;
  const body = document.createElement('div');
  body.className = 'drawer-body module-organizer-drawer';
  const groups = state.moduleLayout.groups;
  const currentGroup = groups.find((group) => group.items.includes(mod.id))?.id || '';
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <h2>Modul organisieren</h2>
        <p>${escapeHtml(mod.id)}</p>
      </div>
      <button class="icon-button" type="button" data-close-organizer aria-label="Schließen">×</button>
    </header>
    <form class="module-organizer-form" data-module-organizer-form>
      <label>
        <span>Anzeigename</span>
        <input name="label" value="${escapeHtml(moduleDisplayTitle(mod))}" />
      </label>
      <label>
        <span>Gruppe</span>
        <select name="group">
          <option value="">Oberste Modulebene</option>
          ${groups.map((group) => `<option value="${escapeHtml(group.id)}"${group.id === currentGroup ? ' selected' : ''}>${escapeHtml(group.title)}</option>`).join('')}
          <option value="__new__">Neue Gruppe...</option>
        </select>
      </label>
      <label data-new-group-row hidden>
        <span>Neue Gruppe</span>
        <input name="newGroupTitle" value="Neue Gruppe" />
      </label>
      <div class="module-organizer-actions">
        <button class="text-button account-primary" type="submit">Speichern</button>
        <button class="text-button" type="button" data-ungroup-module>Aus Gruppe lösen</button>
      </div>
      <small>Drag and Drop in der oberen Navigation ändert Reihenfolge und Gruppenzuordnung direkt. CTOX bleibt fix.</small>
    </form>
  `;
  body.querySelector('[data-close-organizer]')?.addEventListener('click', closeDrawers);
  const groupSelect = body.querySelector('select[name="group"]');
  const newGroupRow = body.querySelector('[data-new-group-row]');
  groupSelect.addEventListener('change', () => {
    newGroupRow.hidden = groupSelect.value !== '__new__';
  });
  body.querySelector('[data-ungroup-module]')?.addEventListener('click', () => {
    moveModuleToUngrouped(mod.id);
    closeDrawers();
  });
  body.querySelector('[data-module-organizer-form]')?.addEventListener('submit', (event) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    renameModule(mod.id, form.get('label')?.toString() || '');
    const groupValue = form.get('group')?.toString() || '';
    if (groupValue === '__new__') {
      const group = createModuleGroup(form.get('newGroupTitle')?.toString().trim() || 'Neue Gruppe');
      moveModuleToGroup(mod.id, group.id);
    } else if (groupValue) {
      moveModuleToGroup(mod.id, groupValue);
    } else {
      moveModuleToUngrouped(mod.id);
    }
    closeDrawers();
  });
  openDrawer('right', body);
}

function openGroupEditDrawer(groupId) {
  const group = state.moduleLayout.groups.find((item) => item.id === groupId);
  if (!group) return;
  const body = document.createElement('div');
  body.className = 'drawer-body module-organizer-drawer';
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <h2>Gruppe organisieren</h2>
        <p>${escapeHtml(group.items.length)} Module</p>
      </div>
      <button class="icon-button" type="button" data-close-organizer aria-label="Schließen">×</button>
    </header>
    <form class="module-organizer-form" data-group-organizer-form>
      <label>
        <span>Gruppenname</span>
        <input name="title" value="${escapeHtml(group.title)}" />
      </label>
      <div class="module-organizer-actions">
        <button class="text-button account-primary" type="submit">Speichern</button>
        <button class="text-button" type="button" data-dissolve-group>Gruppe auflösen</button>
      </div>
    </form>
  `;
  body.querySelector('[data-close-organizer]')?.addEventListener('click', closeDrawers);
  body.querySelector('[data-group-organizer-form]')?.addEventListener('submit', (event) => {
    event.preventDefault();
    const title = new FormData(event.currentTarget).get('title')?.toString().trim();
    group.title = title || 'Gruppe';
    persistModuleLayout();
    renderTabs();
    closeDrawers();
  });
  body.querySelector('[data-dissolve-group]')?.addEventListener('click', () => {
    state.moduleLayout.ungrouped.push(...group.items);
    state.moduleLayout.groups = state.moduleLayout.groups.filter((item) => item.id !== group.id);
    persistModuleLayout();
    renderTabs();
    closeDrawers();
  });
  openDrawer('right', body);
}

function sanitizeClientId(value) {
  return String(value || '')
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, '-')
    .replace(/^-+|-+$/g, '') || `group-${crypto.randomUUID()}`;
}

function renderLoginGate(session) {
  document.body.dataset.authState = 'locked';
  delete document.body.dataset.moduleShell;
  state.modules = [];
  els.tabs.replaceChildren();
  els.leftContent.replaceChildren();
  els.rightContent.replaceChildren();
  els.host.replaceChildren();
}

function renderAccountButton(session = state.session) {
  if (!els.accountButton) return;
  const labelNode = els.accountButton.querySelector('[data-account-label]');
  const user = session?.user || {};
  if (session?.authenticated) {
    const prefs = readAccountPrefs();
    const label = prefs.displayName || user.display_name || user.id || 'Account';
    const role = roleDisplayName(user.role || (user.is_admin ? 'admin' : 'user'));
    if (labelNode) labelNode.textContent = `${label} · ${role}`;
    els.accountButton.setAttribute('aria-label', `Account: ${label}, Rolle: ${role}`);
    els.accountButton.title = `Account: ${label} · Rolle: ${role}`;
    els.accountButton.dataset.authenticated = 'true';
  } else {
    if (labelNode) labelNode.textContent = 'Login';
    els.accountButton.setAttribute('aria-label', 'Login öffnen');
    els.accountButton.title = 'Login öffnen';
    els.accountButton.dataset.authenticated = 'false';
  }
}

function openAccountDrawer() {
  const content = state.session?.authenticated
    ? renderProfileDrawer()
    : renderLoginDrawer(state.session || {});
  els.rightDrawer.classList.add('account-popover');
  openDrawer('right', content);
}

function renderLoginDrawer(session) {
  const body = document.createElement('div');
  body.className = 'drawer-body account-drawer';
  const savedUser = readAccountPrefs().loginUser || 'admin';
  const loginUrl = session.login_url || '';
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <h2>Login</h2>
        <p>Bei Desktop-Start wird die CTOX-Instanz automatisch übernommen.</p>
      </div>
      <button class="icon-button" type="button" data-close-account aria-label="Schließen">×</button>
    </header>
    <form class="account-form" data-login-form>
      <label>
        <span>Benutzer</span>
        <input name="user" autocomplete="username" value="${escapeHtml(savedUser)}" placeholder="admin" />
      </label>
      <label>
        <span>Passwort</span>
        <input type="password" name="password" autocomplete="current-password" placeholder="Passwort" />
      </label>
      <button class="text-button account-primary" type="submit">Einloggen</button>
      ${loginUrl ? `<a class="text-button" href="${escapeHtml(loginUrl)}">Extern einloggen</a>` : ''}
      <small>Business OS zeigt die aktive Rolle danach im Account-Menü.</small>
    </form>
  `;
  body.querySelector('[data-close-account]')?.addEventListener('click', closeDrawers);
  body.querySelector('[data-login-form]')?.addEventListener('submit', (event) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const user = form.get('user')?.toString().trim() || '';
    const password = form.get('password')?.toString() || '';
    if (user && password) {
      localStorage.setItem(AUTH_HEADER_KEY, `Basic ${encodeBasicAuth(user, password)}`);
      localStorage.removeItem(SESSION_TOKEN_KEY);
      localStorage.removeItem(LOGGED_OUT_KEY);
      writeAccountPrefs({ loginUser: user });
    } else {
      return;
    }
    location.reload();
  });
  return body;
}

function renderProfileDrawer() {
  const body = document.createElement('div');
  body.className = 'drawer-body account-drawer';
  const user = state.session?.user || {};
  const prefs = readAccountPrefs();
  const role = user.role || (user.is_admin ? 'admin' : 'user');
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <h2>Account</h2>
        <p>${escapeHtml(user.display_name || user.id || 'CTOX User')} · ${escapeHtml(roleDisplayName(role))}</p>
      </div>
      <button class="icon-button" type="button" data-close-account aria-label="Schließen">×</button>
    </header>
    <section class="account-role-card">
      <span>Rolle</span>
      <strong>${escapeHtml(roleDisplayName(role))}</strong>
      <small>${escapeHtml(roleDescription(role))}</small>
    </section>
    <form class="account-form" data-profile-form>
      <label>
        <span>Anzeigename</span>
        <input name="displayName" value="${escapeHtml(prefs.displayName || user.display_name || '')}" placeholder="Name" />
      </label>
      <label>
        <span>Standard-Sprache</span>
        <select name="language">
          <option value="de"${(prefs.language || document.documentElement.lang) === 'de' ? ' selected' : ''}>Deutsch</option>
          <option value="en"${(prefs.language || document.documentElement.lang) === 'en' ? ' selected' : ''}>English</option>
        </select>
      </label>
      <div class="account-actions">
        <button class="text-button account-primary" type="submit">Speichern</button>
        <button class="text-button" type="button" data-logout>Logout</button>
      </div>
      <small>Persönliche Einstellungen bleiben lokal und werden beim Laden der Module angewendet.</small>
    </form>
  `;
  body.querySelector('[data-close-account]')?.addEventListener('click', closeDrawers);
  body.querySelector('[data-profile-form]')?.addEventListener('submit', (event) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const prefs = {
      ...readAccountPrefs(),
      displayName: form.get('displayName')?.toString().trim() || '',
      language: form.get('language')?.toString() || 'de',
    };
    writeAccountPrefs(prefs);
    applyShellLanguage(prefs.language);
    syncHeaderControls();
    postCurrentPreferencesToModule();
    renderAccountButton({
      ...state.session,
      user: {
        ...(state.session?.user || {}),
        display_name: prefs.displayName || state.session?.user?.display_name || 'CTOX User',
      },
    });
    closeDrawers();
  });
  body.querySelector('[data-logout]')?.addEventListener('click', () => {
    localStorage.removeItem(SESSION_TOKEN_KEY);
    localStorage.removeItem(AUTH_HEADER_KEY);
    localStorage.setItem(LOGGED_OUT_KEY, '1');
    location.reload();
  });
  return body;
}

function readAccountPrefs() {
  try {
    return JSON.parse(localStorage.getItem(ACCOUNT_PREFS_KEY) || '{}') || {};
  } catch {
    return {};
  }
}

function writeAccountPrefs(nextPrefs) {
  const prefs = { ...readAccountPrefs(), ...(nextPrefs || {}) };
  localStorage.setItem(ACCOUNT_PREFS_KEY, JSON.stringify(prefs));
  return prefs;
}

function encodeBasicAuth(user, password) {
  return btoa(unescape(encodeURIComponent(`${user}:${password}`)));
}

function decodeBasicAuthUser(authHeader) {
  const encoded = String(authHeader || '').replace(/^Basic\s+/i, '');
  try {
    return decodeURIComponent(escape(atob(encoded))).split(':')[0] || '';
  } catch {
    return '';
  }
}

function roleDisplayName(role) {
  const value = normalizeRole(role);
  return { chef: 'Chef', admin: 'Admin', founder: 'Founder', user: 'User' }[value] || value;
}

function roleDescription(role) {
  const value = normalizeRole(role);
  return {
    chef: 'Voller Zugriff auf Instanz, Nutzer, Policies und CTOX Steuerung.',
    admin: 'Verwaltet Module, Nutzer, Runtime und operative Einstellungen.',
    founder: 'Founder-Sicht mit Zugriff auf Review, Entscheidungen und Business-Kontext.',
    user: 'Normale Nutzung der freigegebenen Business-OS Module.',
  }[value] || 'Rolle dieser Business-OS Sitzung.';
}

function normalizeRole(role) {
  const value = String(role || '').trim().toLowerCase().replace(/^business_os_/, '');
  if (value === 'owner') return 'chef';
  if (['chef', 'admin', 'founder', 'user'].includes(value)) return value;
  return 'user';
}

function inferLocalRoleFromUser(user) {
  const value = String(user || '').trim().toLowerCase();
  if (value === 'chef' || value === 'owner') return 'chef';
  if (value === 'founder') return 'founder';
  if (value === 'admin') return 'admin';
  return 'user';
}

function roleCanAdmin(role) {
  return ['chef', 'admin'].includes(normalizeRole(role));
}

function canModifyModule(mod) {
  if (!mod?.id) return false;
  const role = normalizeRole(state.session?.user?.role || (state.session?.user?.is_admin ? 'admin' : 'user'));
  if (['chef', 'admin'].includes(role)) return true;
  if (role !== 'founder') return false;
  const userId = state.session?.user?.id || '';
  const assignments = state.governance?.founders?.[mod.id] || [];
  return assignments.some((item) => item.user_id === userId && item.active !== false);
}

async function reportCurrentModule(details = {}) {
  const mod = details.module || state.activeModule;
  return fetchJson('/api/business-os/reports', {
    method: 'POST',
    headers: businessOsAuthHeaders({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({
      module_id: mod?.id || 'ctox',
      kind: details.kind || 'bug',
      severity: details.severity || 'medium',
      title: details.title || 'Business OS report',
      summary: details.summary || '',
      expected: details.expected || '',
      client_context: {
        url: location.href,
        module_id: mod?.id || '',
        viewport: { width: innerWidth, height: innerHeight },
        user_agent: navigator.userAgent,
        source: 'business-os-shell',
      },
    }),
  });
}

function renderLeftContext(mod) {
  const wrap = document.createElement('div');
  wrap.className = 'list';
  for (const collection of mod.collections || []) {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = collection;
    button.addEventListener('click', () => {
      openDrawer('left', drawerContent('Collection', collection));
    });
    wrap.append(button);
  }
  return wrap;
}

function renderRightContext(mod) {
  const wrap = document.createElement('div');
  wrap.className = 'list';
  for (const topic of ['Activity', 'Agent context', 'WebRTC sync']) {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = topic;
    button.addEventListener('click', () => {
      const text = topic === 'WebRTC sync'
        ? `room=${state.sync.config?.sync_room || 'unknown'} · role=${state.sync.config?.peer_role || 'unknown'}`
        : `${mod.title || mod.id} topic context`;
      openDrawer('right', drawerContent(topic, text));
    });
    wrap.append(button);
  }
  return wrap;
}

async function openTemplateStoreDrawer() {
  const body = document.createElement('div');
  body.className = 'drawer-body template-store-drawer';
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <h2>Template Store</h2>
        <p>Füge eine kopierte Modulinstanz hinzu. CTOX kann diese Kopie danach frei verändern.</p>
      </div>
      <button class="icon-button" type="button" data-close-template-store aria-label="Schließen">×</button>
    </header>
    <div class="template-store-list" data-template-store-list>
      <div class="empty-state"><strong>Templates werden geladen</strong></div>
    </div>
  `;
  body.querySelector('[data-close-template-store]')?.addEventListener('click', closeDrawers);
  openDrawer('right', body);
  const list = body.querySelector('[data-template-store-list]');
  try {
    const payload = await loadTemplates();
    const templates = payload.templates || [];
    list.replaceChildren(...templates.map((template) => renderTemplateStoreItem(template)));
    if (!templates.length) {
      list.innerHTML = '<div class="empty-state"><strong>Keine Templates gefunden</strong><span>Lege Templates unter business-os/template-store an.</span></div>';
    }
  } catch (error) {
    list.innerHTML = `
      <div class="auth-gate-panel">
        <strong>Template Store nicht verfügbar</strong>
        <span>${escapeHtml(error.message || error)}</span>
      </div>
    `;
  }
}

function renderTemplateStoreItem(template) {
  const card = document.createElement('article');
  card.className = 'template-store-card';
  const installedCount = state.modules.filter((mod) => mod.id === template.id || mod.id.startsWith(`${template.id}-`)).length;
  const defaultTitle = template.default_title || template.title || template.id;
  const suggestedTitle = installedCount ? `${defaultTitle} ${installedCount + 1}` : defaultTitle;
  card.innerHTML = `
    <div>
      <strong>${escapeHtml(template.title || template.id)}</strong>
      <span>${escapeHtml(template.category || 'Template')}</span>
    </div>
    <p>${escapeHtml(template.description || '')}</p>
    <label>
      <span>Name der Kopie</span>
      <input value="${escapeHtml(suggestedTitle)}" data-template-title />
    </label>
    <div class="template-store-tags">
      ${(template.tags || []).map((tag) => `<span>${escapeHtml(tag)}</span>`).join('')}
    </div>
    <button class="text-button account-primary" type="button" data-install-template>Hinzufügen</button>
  `;
  card.querySelector('[data-install-template]')?.addEventListener('click', async (event) => {
    const button = event.currentTarget;
    button.disabled = true;
    button.textContent = 'Kopiere...';
    try {
      const title = card.querySelector('[data-template-title]')?.value?.trim() || suggestedTitle;
      const result = await installTemplate({ templateId: template.id, title });
      await refreshModules();
      closeDrawers();
      location.hash = result.module?.id || '';
      await openModule(result.module?.id || state.modules[0]?.id || 'ctox');
    } catch (error) {
      button.disabled = false;
      button.textContent = 'Fehlgeschlagen';
      const message = document.createElement('small');
      message.className = 'template-store-error';
      message.textContent = error.message || String(error);
      card.append(message);
    }
  });
  return card;
}

async function refreshModules() {
  const modules = await loadModules();
  state.modules = modules.modules || [];
  state.governance = modules.governance || state.governance;
  state.moduleLayout = normalizeModuleLayout(state.moduleLayout || readModuleLayout(), state.modules);
  persistModuleLayout();
  renderTabs();
  state.backgroundModuleWorkScheduled = false;
  scheduleBackgroundModuleWork();
  if (state.sync?.config?.http_bridge_available !== false) {
    refreshRemoteShellStateInBackground();
  }
}

function renderNavigationDrawer() {
  const body = document.createElement('div');
  body.className = 'drawer-body';
  body.innerHTML = '<h2>Modules</h2>';
  const list = document.createElement('div');
  list.className = 'list';
  for (const mod of state.modules) {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = mod.title || mod.id;
    button.addEventListener('click', () => {
      closeDrawers();
      location.hash = mod.id;
      openModule(mod.id);
    });
    list.append(button);
  }
  body.append(list);
  return body;
}

function drawerContent(title, text) {
  const body = document.createElement('div');
  body.className = 'drawer-body';
  body.innerHTML = `<h2>${escapeHtml(title)}</h2><p>${escapeHtml(text)}</p>`;
  return body;
}

function openDrawer(side, content) {
  const target = side === 'left' ? els.leftDrawer : side === 'right' ? els.rightDrawer : els.bottomDrawer;
  if (side === 'right' && !target.classList.contains('account-popover')) {
    target.classList.remove('account-popover');
  }
  target.replaceChildren(content);
  target.hidden = false;
  showBackdrop();
}

function showBackdrop() {
  els.backdrop.hidden = false;
}

function closeDrawers() {
  els.backdrop.hidden = true;
  els.leftDrawer.hidden = true;
  els.rightDrawer.hidden = true;
  els.bottomDrawer.hidden = true;
  els.rightDrawer.classList.remove('account-popover');
  els.leftDrawer.replaceChildren();
  els.rightDrawer.replaceChildren();
  els.bottomDrawer.replaceChildren();
}

async function fetchJson(url, options) {
  const timeoutMs = options?.timeoutMs ?? FETCH_TIMEOUT_MS;
  const controller = new AbortController();
  const timer = timeoutMs > 0 ? window.setTimeout(() => controller.abort(), timeoutMs) : null;
  const fetchOptions = { ...(options || {}) };
  delete fetchOptions.timeoutMs;
  try {
    const res = await fetch(url, {
      cache: 'no-store',
      signal: controller.signal,
      ...fetchOptions,
    });
    if (!res.ok) throw new Error(`${url} returned ${res.status}`);
    return res.json();
  } finally {
    if (timer) window.clearTimeout(timer);
  }
}

async function loadStatus() {
  try {
    return await fetchJson('/api/business-os/status');
  } catch {
    return { ok: true, runtime: 'electron-static', now_ms: Date.now() };
  }
}

async function loadSession() {
  const explicitLogout = localStorage.getItem(LOGGED_OUT_KEY) === '1';
  if (explicitLogout) {
    return {
      ok: true,
      authenticated: false,
      auth_required: true,
      reason: 'logged_out',
    };
  }
  const injected = readInjectedDesktopSession();
  if (injected) return injected;
  const token = localStorage.getItem(SESSION_TOKEN_KEY)?.trim();
  const authHeader = localStorage.getItem(AUTH_HEADER_KEY)?.trim();
  const headers = token ? { 'X-CTOX-Business-OS-Session': token } : authHeader ? { Authorization: authHeader } : undefined;
  try {
    const session = await fetchJson('/api/business-os/session', headers ? { headers, timeoutMs: 600 } : { timeoutMs: 600 });
    return session;
  } catch (error) {
    if (authHeader) return localBasicAuthSession(authHeader);
    if (token) return localTokenSession();
    return {
      ok: false,
      authenticated: false,
      reason: `session_endpoint_unavailable: ${error.message || error}`,
    };
  }
}

function localBasicAuthSession(authHeader) {
  const user = decodeBasicAuthUser(authHeader) || 'local-user';
  const role = inferLocalRoleFromUser(user);
  return {
    ok: true,
    authenticated: true,
    auth_required: true,
    source: 'stored-basic-auth',
    user: {
      id: user,
      display_name: user,
      role,
      is_admin: roleCanAdmin(role),
    },
    reason: null,
  };
}

function localTokenSession() {
  return {
    ok: true,
    authenticated: true,
    auth_required: true,
    source: 'stored-session-token',
    user: {
      id: 'ctox-user',
      display_name: 'CTOX User',
      role: 'user',
      is_admin: false,
    },
    reason: null,
  };
}

function readInjectedDesktopSession() {
  const candidates = [
    globalThis.CTOX_BUSINESS_OS_SESSION,
    globalThis.ctoxBusinessOsSession,
    globalThis.ctoxBusinessOsLaunch?.session,
    globalThis.CTOX_DESKTOP_SESSION,
    globalThis.ctoxDesktop?.session,
  ];
  const session = candidates.find((item) => item && typeof item === 'object');
  if (!session) return null;
  const user = session.user || {};
  const role = normalizeRole(user.role || (user.is_admin ? 'admin' : 'user'));
  return {
    ok: true,
    authenticated: session.authenticated !== false,
    auth_required: false,
    ...session,
    user: {
      id: user.id || 'ctox-desktop',
      display_name: user.display_name || user.name || user.id || 'CTOX Desktop',
      role,
      is_admin: roleCanAdmin(role),
      ...user,
    },
    reason: null,
  };
}

function isLocalBusinessOsSurface() {
  return ['127.0.0.1', 'localhost', '::1'].includes(location.hostname);
}

function localDesktopSession() {
  return {
    ok: true,
    authenticated: false,
    auth_required: true,
    login_url: null,
    user: {
      id: '',
      display_name: '',
      role: 'user',
      is_admin: false,
    },
    reason: 'login_required',
  };
}

async function loadModules() {
  try {
    return await fetchJson('modules/registry.json', { timeoutMs: 600 });
  } catch {
    return fetchJson('/api/business-os/modules', { headers: businessOsAuthHeaders(), timeoutMs: 800 });
  }
}

async function loadModuleLayout() {
  return readModuleLayout();
}

async function saveModuleLayout(layout) {
  return fetchJson('/api/business-os/module-layout', {
    method: 'POST',
    headers: businessOsAuthHeaders({ 'Content-Type': 'application/json' }),
    body: JSON.stringify(layout),
  });
}

async function loadTemplates() {
  return fetchJson('/api/business-os/templates', { headers: businessOsAuthHeaders() });
}

async function installTemplate({ templateId, title }) {
  return fetchJson('/api/business-os/modules/install-template', {
    method: 'POST',
    headers: businessOsAuthHeaders({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({
      template_id: templateId,
      title,
    }),
  });
}

function businessOsAuthHeaders(extra = {}) {
  const token = localStorage.getItem(SESSION_TOKEN_KEY)?.trim();
  const authHeader = localStorage.getItem(AUTH_HEADER_KEY)?.trim();
  return {
    ...(token ? { 'X-CTOX-Business-OS-Session': token } : authHeader ? { Authorization: authHeader } : {}),
    ...extra,
  };
}

async function loadSyncConfig() {
  const injected = globalThis.CTOX_BUSINESS_OS_CONFIG || globalThis.ctoxBusinessOsLaunch;
  if (injected && typeof injected === 'object') return injected;
  try {
    return await fetchJson('config.default.json', { timeoutMs: 600 });
  } catch {
    return fetchJson('/api/business-os/sync/config', { timeoutMs: 800 });
  }
}

function refreshRemoteShellStateInBackground() {
  if (!state.session?.authenticated) return;
  if (state.sync?.config?.http_bridge_available === false) return;
  window.setTimeout(() => {
    fetchJson('/api/business-os/modules', { headers: businessOsAuthHeaders(), timeoutMs: 800 })
      .then((modules) => {
        if (!Array.isArray(modules?.modules) || !modules.modules.length) return;
        const currentIds = state.modules.map((mod) => mod.id).join('\n');
        const nextIds = modules.modules.map((mod) => mod.id).join('\n');
        if (currentIds === nextIds) return;
        state.modules = modules.modules;
        state.governance = modules.governance || state.governance;
        state.moduleLayout = normalizeModuleLayout(state.moduleLayout || readModuleLayout(), state.modules);
        persistModuleLayout();
        renderTabs();
      })
      .catch(() => {});
  }, 2000);
}

function setStatus(text) {
  if (els.status) els.status.textContent = text;
}

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}
