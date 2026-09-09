import { loadModuleMessages } from '../../shared/i18n.js';
import { showBusinessPrompt } from '../../shared/dialogs.js?v=20260816-browser-sync-guards-v141';
import { createCtoxLauncher } from './ctoxLauncher.js';
import { makeIconDraggable } from './iconDrag.js?v=20260816-browser-sync-guards-v141';
import { getSvgIcon as getFallbackSvgIcon } from '../../shared/icons.js?v=20260816-browser-sync-guards-v141';
import {
  buildQuickAppCreateCommand,
  isRuntimeInstalledApp,
  moduleRenamePayload,
  nextQuickAppIdentity,
} from './appCommands.js';
import {
  applyWorkjetCategory,
  normalizeWorkjetCategory,
  workjetCategoryForModule,
} from '../../shared/workjet-theme.js?v=20260903-entertainment-import-v336';

const STYLE_BUILD = '20260826-workjet-ui-contract-v1';
const LAYOUT_DOC_ID = 'layout';
const ICON_POSITION_CACHE_KEY = 'ctox.businessOs.desktopIconPositions';
const ICON_POSITION_CACHE_VERSION = 2;
const ROW_MAJOR_LAYOUT_MIGRATION = 'row-major-v2';
const DESKTOP_SYNC_COLLECTIONS = Object.freeze([
  'desktop_icons',
  'desktop_layout',
  'desktop_notifications',
]);
const DESKTOP_ICON_PERSISTED_FIELDS = new Set([
  'id',
  'target_type',
  'target_module',
  'target_record_id',
  'label',
  'glyph',
  'x',
  'y',
  'pinned',
  'hidden',
  'sort_index',
  'updated_at_ms',
]);
const DEFAULT_GRID = { cellW: 120, cellH: 140, offset: 24 };
const COMPACT_GRID = { cellW: 88, cellH: 132, offset: 12 };
const ICON_METRICS = {
  width: 112,
  height: 136,
  compactWidth: 88,
  compactHeight: 128,
};

const FALLBACK_LABELS = {
  de: {
    moduleTitle: 'Desktop',
    emptyDesktop: 'Keine Icons auf dem Desktop.',
    syncingData: 'Daten werden synchronisiert.',
    openInModule: 'Öffnen',
    openUnavailable: 'Diese App ist auf dieser Instanz nicht verfügbar.',
    openUnavailableDetail: '„{label}" verweist auf {target} — dieses Modul steht im Katalog dieser Instanz nicht (mehr) zur Verfügung.',
    chatWithCtox: 'Mit CTOX chatten',
    askCtox: 'Frage stellen',
    workWithData: 'Daten ändern',
    modifyApp: 'App ändern',
    createApp: 'Neue App erstellen',
    createAppStarted: 'App wurde erstellt',
    createAppStartedDetail: '{title} ist bereit und erscheint gleich auf dem Desktop.',
    createAppFailed: 'App konnte nicht erstellt werden',
    renameApp: 'App umbenennen',
    renameAppDone: 'App umbenannt',
    renameAppFailed: 'App konnte nicht umbenannt werden',
    chatContextLabel: 'Desktop-Kontext',
    pinToTaskbar: 'An Bar anheften',
    unpinFromTaskbar: 'Von Bar lösen',
    renameIcon: 'Icon umbenennen',
    deleteIcon: 'Icon entfernen',
    arrangeIcons: 'Icons ausrichten',
    addMissingIcons: 'Fehlende Standard-Icons hinzufügen',
    openExplorer: 'Explorer öffnen',
    openNotes: 'Notiz öffnen',
    iconRestoreDefaults: 'Standard-Icons wiederherstellen',
    refresh: 'Aktualisieren',
    platformActive: 'CTOX Plattform aktiv',
    syncReady: 'Sync aktuell',
    syncStarting: 'Sync startet',
    syncRunning: 'Sync läuft',
    syncIssue: 'Sync prüfen',
    syncNoDiagnostics: 'Warte auf Verbindungsstatus',
    syncReadyDetail: 'Icons, Layout und Hinweise sind lokal bereit.',
    syncWaitingOn: 'Wartet auf {collections}',
    ctoxLiveActivity: 'CTOX live',
  },
  en: {
    moduleTitle: 'Desktop',
    emptyDesktop: 'No icons on the desktop.',
    syncingData: 'Syncing data.',
    openInModule: 'Open',
    openUnavailable: 'This app is not available on this instance.',
    openUnavailableDetail: '"{label}" points at {target}, which is not (or no longer) in this instance catalog.',
    chatWithCtox: 'Chat with CTOX',
    askCtox: 'Ask question',
    workWithData: 'Change data',
    modifyApp: 'Modify app',
    createApp: 'Create new app',
    createAppStarted: 'App created',
    createAppStartedDetail: '{title} is ready and will appear on the desktop shortly.',
    createAppFailed: 'App could not be created',
    renameApp: 'Rename app',
    renameAppDone: 'App renamed',
    renameAppFailed: 'App could not be renamed',
    chatContextLabel: 'Desktop context',
    pinToTaskbar: 'Pin to bar',
    unpinFromTaskbar: 'Unpin from bar',
    renameIcon: 'Rename icon',
    deleteIcon: 'Remove icon',
    arrangeIcons: 'Arrange icons',
    addMissingIcons: 'Add missing default icons',
    openExplorer: 'Open Explorer',
    openNotes: 'Open note',
    iconRestoreDefaults: 'Restore default icons',
    refresh: 'Refresh',
    platformActive: 'CTOX platform active',
    syncReady: 'Sync current',
    syncStarting: 'Starting sync',
    syncRunning: 'Sync running',
    syncIssue: 'Check sync',
    syncNoDiagnostics: 'Waiting for connection status',
    syncReadyDetail: 'Icons, layout and notices are locally ready.',
    syncWaitingOn: 'Waiting for {collections}',
    ctoxLiveActivity: 'CTOX live',
  },
};

const SYNC_COLLECTION_LABELS = {
  de: {
    desktop_icons: 'Icons',
    desktop_layout: 'Layout',
    desktop_notifications: 'Benachrichtigungen',
  },
  en: {
    desktop_icons: 'Icons',
    desktop_layout: 'Layout',
    desktop_notifications: 'Notifications',
  },
};

export async function mount(ctx) {
  await ensureStyles();
  const [html, messages] = await Promise.all([
    fetch(new URL('./index.html', import.meta.url)).then((res) => res.text()),
    loadModuleMessages(import.meta.url, ctx.locale, FALLBACK_LABELS),
  ]);
  ctx.host.innerHTML = html;

  const root = ctx.host.querySelector('[data-desktop-root]');
  if (!root) throw new Error('desktop: root element missing after fragment mount');

  const t = (key, fallback) => messages[key] ?? fallback ?? key;

  const refs = {
    root,
    surface: root.querySelector('[data-desktop-surface]'),
    icons: root.querySelector('[data-desktop-icons]'),
    widgetStatus: root.querySelector('[data-widget-status]'),
    widgetSync: root.querySelector('[data-widget-sync]'),
    widgetSyncTitle: root.querySelector('[data-widget-sync-title]'),
    widgetSyncCount: root.querySelector('[data-widget-sync-count]'),
    widgetSyncDetail: root.querySelector('[data-widget-sync-detail]'),
    widgetSyncFill: root.querySelector('[data-widget-sync-fill]'),
  };
  applyWorkjetCategory(refs.root, workjetCategoryForModule(ctx.module));

  const initialModules = Array.isArray(ctx.modules) ? ctx.modules : await loadModuleRegistry();
  let launcher = createLauncher(initialModules);

  const cleanups = [];
  let disposed = false;
  const mountedAtMs = Date.now();

  // Wire up the live clock and date widget
  let startClockTimer = null;
  const timeEl = refs.root.querySelector('[data-widget-time]');
  const dateEl = refs.root.querySelector('[data-widget-date]');
  if (refs.widgetStatus) refs.widgetStatus.textContent = t('platformActive', 'CTOX Plattform aktiv');
  if (timeEl && dateEl) {
    const updateClock = () => {
      try {
        const now = new Date();
        let locale = 'de';
        if (ctx && typeof ctx.locale === 'string') {
          locale = ctx.locale;
        }
        setTextIfChanged(timeEl, now.toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit' }));
        setTextIfChanged(dateEl, now.toLocaleDateString(locale, { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' }));
      } catch (e) {
        console.error('[desktop] clock update failed with locale:', ctx?.locale, e);
        try {
          const now = new Date();
          setTextIfChanged(timeEl, now.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' }));
          setTextIfChanged(dateEl, now.toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' }));
        } catch (fallbackErr) {
          console.error('[desktop] clock absolute fallback failed:', fallbackErr);
        }
      }
    };
    updateClock();
    // The timer starts only once the maintenance-sensitive icon persistence has
    // succeeded: during maintenance `ensureIcons` can reject, and a desktop that
    // never finished mounting must not keep a second-tick running behind it.
    startClockTimer = () => {
      const clockInterval = setInterval(updateClock, 1000);
      cleanups.push(() => clearInterval(clockInterval));
    };
  }
  wireSyncStatusWidget();
  const layoutCollection = ctx.db?.collection?.('desktop_layout');
  const iconsCollection = ctx.db?.collection?.('desktop_icons');
  const commandsCollection = ctx.db?.collection?.('business_commands');

  const layout = await ensureLayout(layoutCollection, launcher);
  let iconPositionCache = readIconPositionCache();
  let iconsReadiness = readIconsReadiness();
  await ensureIcons(iconsCollection, launcher);
  startClockTimer?.();
  await renderIcons();

  cleanups.push(subscribeIcons());
  cleanups.push(subscribeIconsReadiness());
  cleanups.push(subscribeModuleCatalogChanges());
  if (commandsCollection) cleanups.push(subscribeCommandStream());

  const onDragOverSurface = (event) => {
    const isPin = event.dataTransfer?.types.includes('application/x-ctox-taskbar-pin');
    if (isPin) {
      event.preventDefault();
      event.dataTransfer.dropEffect = 'move';
    }
  };

  const onDropSurface = (event) => {
    const pinId = event.dataTransfer?.getData('application/x-ctox-taskbar-pin')
      || event.dataTransfer?.getData('text/plain')
      || '';
    if (pinId) {
      event.preventDefault();
      event.stopPropagation();
      togglePinnedTarget(pinId, false);
    }
  };

  refs.surface.addEventListener('contextmenu', onSurfaceContextMenu);
  cleanups.push(() => refs.surface.removeEventListener('contextmenu', onSurfaceContextMenu));

  refs.surface.addEventListener('dragover', onDragOverSurface);
  refs.surface.addEventListener('drop', onDropSurface);
  cleanups.push(() => refs.surface.removeEventListener('dragover', onDragOverSurface));
  cleanups.push(() => refs.surface.removeEventListener('drop', onDropSurface));

  return () => {
    disposed = true;
    for (const dispose of cleanups) {
      try { dispose?.(); } catch (error) { console.error('[desktop] cleanup failed:', error); }
    }
  };

  // ---------- helpers (closures over the mount scope) ----------

  function wireSyncStatusWidget() {
    if (!refs.widgetSync) return;
    const render = () => {
      if (disposed) return;
      renderSyncStatus(syncStatusView(window.ctoxBusinessOsSyncDiagnostics || ctx.sync?.diagnostics || null));
    };
    render();
    window.addEventListener('ctox-business-os-sync-diagnostics', render);
    const interval = setInterval(render, 2000);
    cleanups.push(() => {
      window.removeEventListener('ctox-business-os-sync-diagnostics', render);
      clearInterval(interval);
    });
  }

  function syncStatusView(snapshot) {
    const declaredCollections = Array.isArray(ctx.module?.collections)
      ? ctx.module.collections.filter(Boolean)
      : [];
    const declaredSet = new Set(declaredCollections);
    const collections = DESKTOP_SYNC_COLLECTIONS.filter((name) => !declaredSet.size || declaredSet.has(name));
    const total = collections.length;
    if (!snapshot || snapshot.mode !== 'webrtc' || !snapshot.collections) {
      return {
        hidden: true,
        state: 'idle',
        ready: 0,
        total,
        title: '',
        detail: '',
      };
    }

    const items = collections.map((name) => {
      const diagnostics = snapshot.collections?.[name] || null;
      const status = diagnostics?.connectionStatus || diagnostics?.status || 'pending';
      return {
        name,
        label: syncCollectionLabel(name),
        status,
        diagnostics,
        ready: isSyncCollectionReady(diagnostics),
        issue: ['failed', 'error', 'stopped'].includes(status),
      };
    });
    const ready = items.filter((item) => item.ready).length;
    const issueItems = items.filter((item) => item.issue);
    const pending = items.filter((item) => !item.ready);

    if (issueItems.length) {
      return {
        state: 'issue',
        ready,
        total,
        title: t('syncIssue', 'Sync prüfen'),
        detail: `${issueItems[0].label}: ${syncStatusLabel(issueItems[0])}`,
      };
    }
    if (ready >= total) {
      return {
        hidden: true,
        state: 'idle',
        ready,
        total,
        title: '',
        detail: '',
      };
    }
    const pendingStatuses = new Set(pending.map((item) => String(item.status || '').toLowerCase()));
    const hasActiveSyncWork = ['connecting', 'reconnecting', 'running'].some((status) => pendingStatuses.has(status));
    const withinStartupGrace = Date.now() - mountedAtMs < 8000;
    if (!hasActiveSyncWork && !withinStartupGrace) {
      return {
        hidden: true,
        state: 'idle',
        ready,
        total,
        title: '',
        detail: '',
      };
    }
    const waiting = pending.slice(0, 2).map((item) => item.label).join(', ');
    return {
      state: 'syncing',
      ready,
      total,
      title: t('syncRunning', 'Sync läuft'),
      detail: t('syncWaitingOn', 'Wartet auf {collections}').replace('{collections}', waiting || 'Daten'),
    };
  }

  function syncCollectionLabel(name) {
    const lang = ctx.locale === 'en' ? 'en' : 'de';
    return SYNC_COLLECTION_LABELS[lang]?.[name] || name.replace(/^desktop_/, '').replaceAll('_', ' ');
  }

  function isSyncCollectionReady(diagnostics) {
    if (!diagnostics) return false;
    const status = diagnostics.connectionStatus || diagnostics.status || '';
    const activePeerCount = Number(diagnostics.frameTransport?.activePeerCount || 0);
    if (diagnostics.frameTransport && activePeerCount === 0) return false;
    if (diagnostics.initialReplicationAt || diagnostics.connectedAt) return true;
    if (diagnostics.initialReplicationState === 'complete') return true;
    return ['connected', 'running', 'reused'].includes(status);
  }

  function syncStatusLabel(item) {
    const lastError = item?.diagnostics?.lastError?.message || item?.diagnostics?.lastError?.code || '';
    if (lastError) return shortSyncDetail(lastError);
    const status = item?.status || 'pending';
    const lang = ctx.locale === 'en' ? 'en' : 'de';
    const labels = {
      de: {
        connecting: 'verbindet',
        pending: 'wartet',
        reconnecting: 'verbindet neu',
        error: 'Fehler',
        failed: 'fehlgeschlagen',
        stopped: 'gestoppt',
      },
      en: {
        connecting: 'connecting',
        pending: 'waiting',
        reconnecting: 'reconnecting',
        error: 'error',
        failed: 'failed',
        stopped: 'stopped',
      },
    };
    return labels[lang]?.[status] || status;
  }

  // Writing textContent replaces the text node even when the string is
  // identical, which costs a layout pass on every status tick. The desktop
  // widgets tick about twice a second, so the unchanged writes were the whole
  // visible twitch. Only write when the value actually moved.
  function setTextIfChanged(element, value) {
    const next = String(value ?? '');
    if (!element || element.textContent === next) return;
    element.textContent = next;
  }

  function shortSyncDetail(value) {
    const text = String(value || '').replace(/\s+/g, ' ').trim();
    if (text.length <= 72) return text;
    return `${text.slice(0, 69)}...`;
  }

  function renderSyncStatus(view) {
    refs.widgetSync.hidden = Boolean(view.hidden);
    if (refs.widgetSync.dataset.state !== view.state) refs.widgetSync.dataset.state = view.state;
    setTextIfChanged(refs.widgetSyncTitle, view.title);
    setTextIfChanged(refs.widgetSyncCount, `${view.ready}/${view.total}`);
    setTextIfChanged(refs.widgetSyncDetail, view.detail);
    const percent = view.total > 0 ? Math.round((view.ready / view.total) * 100) : 0;
    const width = `${Math.min(100, Math.max(0, percent))}%`;
    if (refs.widgetSyncFill.style.width !== width) refs.widgetSyncFill.style.width = width;
  }

  function createLauncher(modules) {
    return createCtoxLauncher({
      modules: Array.isArray(modules) ? modules : [],
      apps: typeof ctx.getDesktopApps === 'function' ? ctx.getDesktopApps() : (ctx.desktopApps || []),
      currentModuleId: ctx.module.id,
      openApp: ctx.openDesktopApp,
    });
  }

  function categoryForLauncherEntry(entry) {
    return entry?.kind === 'module'
      ? workjetCategoryForModule(entry)
      : normalizeWorkjetCategory(entry?.category);
  }

  function subscribeModuleCatalogChanges() {
    if (!ctx.eventBus?.on) return () => {};
    const token = ctx.eventBus.on('modules:changed', (payload = {}) => {
      const nextModules = Array.isArray(payload.modules)
        ? payload.modules
        : (typeof ctx.getModules === 'function' ? ctx.getModules() : ctx.modules);
      launcher = createLauncher(nextModules);
      Promise.resolve()
        .then(() => ensureIcons(iconsCollection, launcher))
        .then(renderIcons)
        .catch((error) => {
          if (isDatabaseClosingError(error)) return;
          if (showManagedAuthorizationError(error)) return;
          console.error('[desktop] module catalog refresh failed:', error);
        });
    });
    return () => ctx.eventBus.off?.('modules:changed', token);
  }

  async function renderIcons() {
    if (disposed) return;
    let docs = [];
    let usingFallbackDocs = false;
    try {
      if (iconsCollection) {
        docs = await iconsCollection.find().exec();
      } else {
        docs = fallbackIconDocs(launcher);
        usingFallbackDocs = true;
      }
    } catch (error) {
      if (!isDatabaseClosingError(error)) throw error;
      console.info('[desktop] icon read skipped during database restart; rendering default launcher icons');
      docs = fallbackIconDocs(launcher);
      usingFallbackDocs = true;
    }
    if (disposed) return;
    if (!usingFallbackDocs) {
      syncIconPositionCacheFromDocs(docs);
    }
    // Jeder Sync-Tick baute bisher saemtliche Icons neu auf, auch wenn sich
    // kein einziges geaendert hatte — gemessen 35 DOM-Ersetzungen in 20
    // Sekunden. Die Icons sind eine reine Funktion der Dokumente, also darf
    // ein unveraenderter Satz den Neuaufbau ueberspringen.
    const iconSignature = JSON.stringify([
      usingFallbackDocs,
      docs.map((doc) => {
        const value = typeof doc.toJSON === 'function' ? doc.toJSON() : doc;
        return [
          value.id, value.label, value.glyph, value.target_module, value.target_type,
          value.x, value.y, value.pinned, value.hidden, value.sort_index,
          categoryForLauncherEntry(launcher.entries().find((entry) => entry.id === value.target_module)),
        ];
      }),
    ]);
    if (iconSignature === renderIcons.lastSignature && refs.icons.childElementCount) return;
    renderIcons.lastSignature = iconSignature;
    refs.icons.innerHTML = '';
    if (!docs.length) {
      if (!usingFallbackDocs && iconsReadiness?.ready === false) {
        const syncing = document.createElement('div');
        syncing.className = 'ctox-syncing';
        syncing.setAttribute('role', 'status');
        syncing.setAttribute('aria-live', 'polite');
        syncing.textContent = t('syncingData', 'Daten werden synchronisiert.');
        refs.icons.appendChild(syncing);
        return;
      }
      const empty = document.createElement('div');
      empty.className = 'desktop-icon-empty';
      empty.textContent = t('emptyDesktop', 'Keine Icons auf dem Desktop.');
      refs.icons.appendChild(empty);
      return;
    }
    const sorted = docs
      .map((doc) => applyCachedIconPosition(plainIconDoc(doc)))
      .sort((a, b) => (a.sort_index ?? 0) - (b.sort_index ?? 0));
    for (const doc of sorted) {
      if (doc.hidden) continue;
      if (!launcher.knows(doc.target_module)) continue;
      refs.icons.appendChild(buildIcon(doc));
    }
  }

  function buildIcon(doc) {
    const el = document.createElement('div');
    el.className = 'desktop-icon';
    el.dataset.iconId = doc.id;
    el.dataset.target = doc.target_module || '';
    const position = clampIconPosition(doc, doc, currentGrid());
    el.style.left = `${position.x}px`;
    el.style.top = `${position.y}px`;
    el.innerHTML = `
      <div class="desktop-icon-glyph" aria-hidden="true"></div>
      <div class="desktop-icon-label"></div>
    `;
    
    const glyphEl = el.querySelector('.desktop-icon-glyph');
    const targetModule = doc.target_module || '';
    applyWorkjetCategory(
      el,
      categoryForLauncherEntry(launcher.entries().find((entry) => entry.id === targetModule)),
    );
    const resolveSvgIcon = typeof ctx.getSvgIcon === 'function' ? ctx.getSvgIcon : getFallbackSvgIcon;
    const svgIcon = resolveSvgIcon(targetModule, 28);
    if (svgIcon) {
      glyphEl.innerHTML = svgIcon;
    } else {
      glyphEl.textContent = doc.glyph || launcher.glyphFor(targetModule);
    }
    
    const label = desktopIconLabel(doc);
    el.querySelector('.desktop-icon-label').textContent = label;
    el.title = label;
    el.setAttribute('role', 'button');
    el.setAttribute('aria-label', label);
    el.tabIndex = 0;

    el.addEventListener('keydown', (event) => {
      if (event.key === 'Enter' || event.key === ' ') {
        event.preventDefault();
        openDesktopTarget(doc);
      }
    });
    el.addEventListener('contextmenu', (event) => onIconContextMenu(event, doc));

    makeIconDraggable(el, {
      surface: refs.surface,
      iconId: doc.id,
      grid: {
        ...currentGrid(),
        flow: document.documentElement.dataset.workjetMobileHost === 'true' || currentGrid().compact,
      },
      onSelect: () => {
        for (const node of refs.icons.querySelectorAll('.desktop-icon.selected')) {
          node.classList.remove('selected');
        }
        el.classList.add('selected');
      },
      onActivate: () => openDesktopTarget(doc),
      onMoved: async (iconId, position) => {
        const updatedAt = Date.now();
        rememberIconPosition(iconId, position, updatedAt);
      },
      onDragToTopbar: (iconId) => {
        if (doc.target_module) {
          togglePinnedTarget(doc.target_module, true);
        }
      },
      onReorder: reorderIcons,
    });

    return el;
  }

  function onSurfaceContextMenu(event) {
    if (event.target.closest('.desktop-icon')) return;
    if (!ctx.contextMenu) return;
    ctx.contextMenu.show(event, [
      { label: t('createApp', 'Neue App erstellen'), icon: '+', action: safeAction(createQuickApp) },
      { label: t('chatWithCtox', 'Mit CTOX chatten'), icon: '◆', action: safeAction(chatWithCtoxAboutDesktop) },
      { type: 'separator' },
      { label: t('openExplorer', 'Explorer öffnen'), icon: '⌘', disabled: !launcher.knows('explorer'), action: () => launcher.open('explorer') },
      { label: t('openNotes', 'Notiz öffnen'), icon: '✎', disabled: !launcher.knows('notes'), action: () => launcher.open('notes') },
      { type: 'separator' },
      { label: t('arrangeIcons', 'Icons ausrichten'), icon: '▦', action: safeAction(arrangeIcons) },
      { label: t('addMissingIcons', 'Fehlende Standard-Icons hinzufügen'), icon: '+', action: safeAction(addMissingDefaultIcons) },
      { label: t('iconRestoreDefaults', 'Standard-Icons wiederherstellen'), icon: '⟳', action: safeAction(restoreDefaultIcons) },
      { type: 'separator' },
      { label: t('refresh', 'Aktualisieren'), icon: '↻', action: safeAction(renderIcons) },
    ]);
  }

  function onIconContextMenu(event, doc) {
    event.preventDefault();
    event.stopPropagation();
    // Android WebView emits a synthetic contextmenu during the same long-press
    // gesture that enters launcher edit mode. Mobile reserves long-press for
    // full-cell reordering; opening the desktop menu here steals the drop and
    // can even activate a menu action under the moving finger.
    if (document.documentElement.dataset.workjetMobileHost === 'true') return;
    if (event.currentTarget?.classList?.contains('touch-reordering')) return;
    if (!ctx.contextMenu) return;
    const pinned = isPinnedTarget(doc.target_module);
    const app = moduleForDesktopTarget(doc.target_module);
    const canRenameApp = isRuntimeInstalledApp(app);
    ctx.contextMenu.show(event, [
      { label: t('openInModule', 'Öffnen'), icon: '↗', action: () => openDesktopTarget(doc) },
      {
        label: pinned ? t('unpinFromTaskbar', 'Von Bar lösen') : t('pinToTaskbar', 'An Bar anheften'),
        icon: pinned ? '−' : '+',
        action: safeAction(() => togglePinnedTarget(doc.target_module, !pinned)),
      },
      { label: t('askCtox', 'Frage stellen'), icon: '?', action: safeAction(() => chatWithCtoxAboutIcon(doc, 'ask')) },
      { label: t('workWithData', 'Daten ändern'), icon: '◇', action: safeAction(() => chatWithCtoxAboutIcon(doc, 'data')) },
      { label: t('modifyApp', 'App ändern'), icon: '✦', action: safeAction(() => chatWithCtoxAboutIcon(doc, 'app')) },
      ...(canRenameApp ? [
        { label: t('renameApp', 'App umbenennen'), icon: '✎', action: safeAction(() => renameApp(doc, app)) },
      ] : []),
      { label: t('renameIcon', 'Icon umbenennen'), icon: '✎', action: safeAction(() => renameIcon(doc)) },
      { type: 'separator' },
      { label: t('deleteIcon', 'Icon entfernen'), icon: '−', action: safeAction(() => deleteIcon(doc.id)) },
    ]);
  }

  // launcher.open() meldet ein fehlendes Ziel nur mit `false` zurueck. Alle drei
  // Oeffnen-Wege (Doppelklick, Enter, Kontextmenue) haben diesen Rueckgabewert
  // verworfen: ein Icon, dessen Zielmodul nicht (mehr) im Katalog dieser Instanz
  // steht, tat auf Klick GAR NICHTS — kein Fenster, keine Meldung. Genau so
  // sieht ein "kaputtes" Kontextmenue aus. Der Fehlschlag wird jetzt benannt.
  function openDesktopTarget(doc) {
    const targetModule = doc?.target_module || '';
    if (launcher.open(targetModule)) return true;
    notify({
      type: 'warning',
      title: t('openUnavailable', 'Diese App ist auf dieser Instanz nicht verfügbar.'),
      message: formatMessage(
        t('openUnavailableDetail', '„{label}" verweist auf {target} — dieses Modul steht im Katalog dieser Instanz nicht (mehr) zur Verfügung.'),
        { label: desktopIconLabel(doc), target: targetModule || '—' },
      ),
    });
    return false;
  }

  function safeAction(action) {
    return () => {
      Promise.resolve()
        .then(action)
        .catch((error) => console.error('[desktop] context menu action failed:', error));
    };
  }

  async function createQuickApp() {
    if (!ctx.commandBus?.dispatch) throw new Error('Business OS command runtime is not ready.');
    const identity = nextQuickAppIdentity(currentModules(), ctx.locale, Date.now());
    const command = buildQuickAppCreateCommand({
      moduleId: identity.id,
      title: identity.title,
      actor: actorContext(),
    });
    try {
      await ctx.commandBus.dispatch(command, { until: 'terminal' });
      notify({
        type: 'success',
        title: t('createAppStarted', 'App wurde erstellt'),
        message: formatMessage(
          t('createAppStartedDetail', '{title} ist bereit und erscheint gleich auf dem Desktop.'),
          { title: identity.title },
        ),
      });
    } catch (error) {
      notify({
        type: 'error',
        title: t('createAppFailed', 'App konnte nicht erstellt werden'),
        message: String(error?.message || error),
      });
      throw error;
    }
  }

  async function renameApp(doc, app) {
    if (!ctx.commandBus?.dispatch) throw new Error('Business OS command runtime is not ready.');
    const current = String(app?.title || titleForModule(doc.target_module) || '').trim();
    const next = await showBusinessPrompt(t('renameApp', 'App umbenennen'), {
      title: t('renameApp', 'App umbenennen'),
      defaultValue: current,
    });
    const title = String(next || '').trim();
    if (!title || title === current) return;
    const payload = moduleRenamePayload(app, title);
    const commandId = `cmd_module_rename_${crypto.randomUUID?.() || Date.now()}`;
    try {
      await ctx.commandBus.dispatch({
        id: commandId,
        command_id: commandId,
        module: 'ctox',
        command_type: 'ctox.module.save',
        record_id: app.id,
        payload,
        client_context: {
          source: 'desktop-app-context-menu',
          action: 'app.rename',
          module_id: app.id,
          app_id: app.id,
          actor: actorContext(),
        },
      }, { until: 'terminal' });
      const existing = iconsCollection ? await iconsCollection.findOne(doc.id).exec() : null;
      if (existing) {
        await existing.incrementalPatch({ label: title, updated_at_ms: Date.now() });
      }
      notify({ type: 'success', title: t('renameAppDone', 'App umbenannt'), message: title });
    } catch (error) {
      notify({
        type: 'error',
        title: t('renameAppFailed', 'App konnte nicht umbenannt werden'),
        message: String(error?.message || error),
      });
      throw error;
    }
  }

  function currentModules() {
    const modules = typeof ctx.getModules === 'function' ? ctx.getModules() : ctx.modules;
    return Array.isArray(modules) ? modules : [];
  }

  function moduleForDesktopTarget(moduleId) {
    // Renaming must use the complete shell module manifest so collections and
    // layout survive the ctox.module.save round-trip. Launcher entries can be
    // intentionally reduced and are therefore not safe mutation inputs.
    return currentModules().find((module) => module?.id === moduleId) || null;
  }

  function actorContext() {
    const user = ctx.session?.user || {};
    return {
      id: user.id || user.user_id || '',
      display_name: user.display_name || user.name || user.id || '',
      role: user.role || 'user',
      is_admin: Boolean(user.is_admin),
    };
  }

  function notify(notification) {
    ctx.notifications?.show?.(notification);
  }

  function formatMessage(template, values) {
    return String(template || '').replace(/\{(\w+)\}/g, (_, key) => String(values?.[key] ?? ''));
  }

  async function renameIcon(doc) {
    if (!iconsCollection || !doc?.id) return;
    const current = doc.label || titleForModule(doc.target_module);
    const next = await showBusinessPrompt(t('renameIcon', 'Icon umbenennen'), {
      title: t('renameIcon', 'Icon umbenennen'),
      defaultValue: current,
    });
    const label = String(next || '').trim();
    if (!label || label === current) return;
    const existing = await iconsCollection.findOne(doc.id).exec();
    if (existing) {
      await existing.incrementalPatch({
        label,
        updated_at_ms: Date.now(),
      });
    }
  }

  async function deleteIcon(iconId) {
    if (!iconsCollection) return;
    const existing = await iconsCollection.findOne(iconId).exec();
    if (existing) await existing.remove();
  }

  function isPinnedTarget(targetId) {
    return !!targetId && typeof ctx.isTaskbarPinned === 'function' && ctx.isTaskbarPinned(targetId);
  }

  function togglePinnedTarget(targetId, shouldPin) {
    if (!targetId) return;
    if (typeof ctx.toggleTaskbarPin === 'function') {
      ctx.toggleTaskbarPin(targetId, shouldPin);
      return;
    }
    if (shouldPin) ctx.pinToTaskbar?.(targetId);
    else ctx.unpinFromTaskbar?.(targetId);
  }

  function chatWithCtoxAboutIcon(doc, mode = 'ask') {
    const label = doc.label || titleForModule(doc.target_module) || doc.id || 'Desktop object';
    const targetKind = doc.target_type || launcher.kindOf(doc.target_module) || 'module';
    const targetModule = doc.target_module || '';
    const recordId = doc.target_record_id || '';
    const agentMode = desktopAgentModeConfig(mode, label);
    const iconContext = {
      id: doc.id || '',
      label,
      target_type: targetKind,
      target_module: targetModule,
      target_record_id: recordId,
      glyph: doc.glyph || launcher.glyphFor(targetModule),
    };
    const contextText = [
      `Kontext: Desktop-Objekt "${label}".`,
      targetModule ? `Ziel: ${targetKind} "${targetModule}".` : '',
      recordId ? `Record: ${recordId}.` : '',
    ].filter(Boolean).join(' ');
    const detail = {
      title: agentMode.title,
      draft: agentMode.draft,
      context_text: contextText,
      context_label: t('chatContextLabel', 'Desktop-Kontext'),
      module: targetModule || 'desktop',
      source_module: 'desktop',
      source_title: t('moduleTitle', 'Desktop'),
      record_id: doc.id || recordId || targetModule,
      command_title: agentMode.commandTitle,
      command_type: agentMode.commandType,
      mode: agentMode.mode,
      target: agentMode.target,
      thread_key: targetModule ? `business-os/${targetModule}` : `business-os/desktop/${doc.id || 'surface'}`,
      payload: {
        mode: agentMode.mode,
        target: agentMode.target,
        title: agentMode.commandTitle,
        thread_key: targetModule ? `business-os/${targetModule}` : `business-os/desktop/${doc.id || 'surface'}`,
        desktop_icon: iconContext,
        file_context: iconContext,
      },
      client_context: {
        source: 'desktop-icon-context-menu',
        action: agentMode.action,
        mode: agentMode.mode,
        target: agentMode.target,
        desktop_icon_id: doc.id || '',
        target_type: targetKind,
        target_module: targetModule,
        module_id: targetModule,
        app_id: targetModule,
        target_record_id: recordId,
      },
    };
    openCtoxChat(detail);
  }

  function desktopAgentModeConfig(mode, label) {
    if (mode === 'app') {
      return {
        mode: 'app',
        target: 'app',
        action: 'app.modify',
        title: `${t('modifyApp', 'App ändern')} · ${label}`,
        draft: `Ändere die App "${label}": `,
        commandTitle: `${label} App ändern`,
        commandType: 'ctox.business_os.app.modify',
      };
    }
    if (mode === 'data') {
      return {
        mode: 'data',
        target: 'data',
        action: 'data.modify',
        title: `${t('workWithData', 'Daten ändern')} · ${label}`,
        draft: `Ändere Daten in "${label}": `,
        commandTitle: `${label} Daten ändern`,
        commandType: 'business_os.chat.task',
      };
    }
    return {
      mode: 'ask',
      target: 'question',
      action: 'question.ask',
      title: `${t('askCtox', 'Frage stellen')} · ${label}`,
      draft: `Frage zu "${label}": `,
      commandTitle: `${label} Frage`,
      commandType: 'business_os.chat.task',
    };
  }

  function chatWithCtoxAboutDesktop() {
    const iconCount = refs.icons?.querySelectorAll('.desktop-icon')?.length || 0;
    const detail = {
      title: 'CTOX · Desktop',
      draft: 'Bitte hilf mir mit dem Desktop. ',
      context_text: `Kontext: Desktop-Oberfläche mit ${iconCount} sichtbaren Icons.`,
      context_label: t('chatContextLabel', 'Desktop-Kontext'),
      module: 'desktop',
      source_module: 'desktop',
      source_title: t('moduleTitle', 'Desktop'),
      record_id: 'desktop',
      command_title: 'Desktop',
      payload: {
        desktop_surface: {
          module: 'desktop',
          visible_icon_count: iconCount,
        },
      },
      client_context: {
        source: 'desktop-surface-context-menu',
        target_type: 'desktop_surface',
      },
    };
    openCtoxChat(detail);
  }

  function openCtoxChat(detail) {
    if (typeof ctx.openBusinessChat === 'function') {
      ctx.openBusinessChat(detail);
      return;
    }
    window.dispatchEvent(new CustomEvent('ctox-business-os-chat-open', { detail }));
  }

  async function restoreDefaultIcons() {
    if (!iconsCollection) return;
    iconPositionCache = new Map();
    writeIconPositionCache();
    const all = await iconsCollection.find().exec();
    await Promise.all(all.map((doc) => doc.remove()));
    await ensureIcons(iconsCollection, launcher, { force: true });
    await renderIcons();
  }

  async function addMissingDefaultIcons() {
    if (!iconsCollection) return;
    const existing = await iconsCollection.find().exec();
    const existingTargets = new Set(existing.map((doc) => doc.target_module).filter(Boolean));
    const entries = launcher.entries().filter((entry) => !existingTargets.has(entry.id));
    if (!entries.length) {
      await renderIcons();
      return;
    }
    const grid = currentGrid();
    const startIndex = existing.length;
    for (const [offsetIndex, entry] of entries.entries()) {
      const position = gridPosition(startIndex + offsetIndex, grid);
      const seed = {
        id: `desk_icon_${entry.id}`,
        target_type: entry.kind || 'module',
        target_module: entry.id,
        target_record_id: '',
        label: entry.title || entry.id,
        glyph: launcher.glyphFor(entry.id),
        x: position.x,
        y: position.y,
        pinned: false,
        hidden: false,
        sort_index: startIndex + offsetIndex,
        updated_at_ms: Date.now(),
      };
      await upsertSeed(iconsCollection, seed.id, { ...seed, hidden: false });
    }
  }

  async function arrangeIcons() {
    if (!iconsCollection) return;
    const docs = (await iconsCollection.find().exec())
      .filter((doc) => !doc.hidden)
      .sort((a, b) => (a.sort_index ?? 0) - (b.sort_index ?? 0));
    const grid = currentGrid();
    docs.forEach((doc, index) => {
      const position = gridPosition(index, grid);
      rememberIconPosition(doc.id, position, Date.now() + index);
    });
    await renderIcons();
  }

  async function reorderIcons(orderedIconIds) {
    if (!iconsCollection || !Array.isArray(orderedIconIds) || !orderedIconIds.length) return;
    const docs = await iconsCollection.find().exec();
    const docsById = new Map(docs.map((doc) => [doc.id, doc]));
    const visibleIds = orderedIconIds.filter((id) => {
      const doc = docsById.get(id);
      return doc && !doc.hidden && launcher.knows(doc.target_module);
    });
    const remainingIds = docs
      .filter((doc) => !doc.hidden && launcher.knows(doc.target_module) && !visibleIds.includes(doc.id))
      .sort((a, b) => (a.sort_index ?? 0) - (b.sort_index ?? 0))
      .map((doc) => doc.id);
    const nextIds = [...visibleIds, ...remainingIds];
    if (nextIds.length < 2) return;
    const updatedAt = Date.now();
    const updates = nextIds.map((id, sortIndex) => {
      const plain = plainIconDoc(docsById.get(id));
      const persisted = Object.fromEntries(
        Object.entries(plain).filter(([key]) => DESKTOP_ICON_PERSISTED_FIELDS.has(key)),
      );
      return {
        ...persisted,
        sort_index: sortIndex,
        updated_at_ms: updatedAt + sortIndex,
      };
    });
    if (typeof iconsCollection.bulkUpsert === 'function') {
      await iconsCollection.bulkUpsert(updates);
    } else {
      await Promise.all(updates.map((update) => docsById.get(update.id)?.incrementalPatch({
        sort_index: update.sort_index,
        updated_at_ms: update.updated_at_ms,
      })));
    }
    renderIcons.lastSignature = '';
    await renderIcons();
  }

  function subscribeIcons() {
    if (!iconsCollection?.$) return () => {};
    const sub = iconsCollection.$.subscribe(() => {
      renderIcons().catch((error) => {
        if (isDatabaseClosingError(error)) return;
        if (showManagedAuthorizationError(error)) return;
        console.error('[desktop] icon render failed:', error);
      });
    });
    return () => sub.unsubscribe?.();
  }

  function readIconsReadiness() {
    const read = ctx.sync?.collectionReadiness;
    return typeof read === 'function' ? read.call(ctx.sync, 'desktop_icons') : null;
  }

  function subscribeIconsReadiness() {
    const subscribe = ctx.sync?.subscribeCollectionReadiness;
    if (typeof subscribe !== 'function') return () => {};
    const unsubscribe = subscribe.call(ctx.sync, 'desktop_icons', (snapshot) => {
      iconsReadiness = snapshot;
      renderIcons().catch((error) => {
        if (isDatabaseClosingError(error)) return;
        if (showManagedAuthorizationError(error)) return;
        console.error('[desktop] icon render failed:', error);
      });
    });
    return typeof unsubscribe === 'function' ? unsubscribe : () => {};
  }

  function isDatabaseClosingError(error) {
    const message = String(error?.message || error || '');
    return /IDBDatabase.*closing|database connection is closing/i.test(message);
  }

  function showManagedAuthorizationError(error) {
    const message = String(error?.message || error || '');
    if (!/UNAUTHORIZED: peer is not authorized for this collection/i.test(message)) return false;
    globalThis.showStartupError?.(error);
    return true;
  }

  function subscribeCommandStream() {
    if (!ctx.notifications) return () => {};
    let lastSeenAt = Date.now();
    const sub = commandsCollection.$.subscribe((change) => {
      const doc = change?.documentData || change?.doc?._data || change?.doc;
      if (!doc) return;
      if (!doc.updated_at_ms || doc.updated_at_ms <= lastSeenAt) return;
      lastSeenAt = doc.updated_at_ms;
      if (doc.module === ctx.module.id) return;
      ctx.notifications.show({
        type: 'info',
        title: t('ctoxLiveActivity', 'CTOX live'),
        message: composeCommandToast(doc),
        action: launcher.knows(doc.module) ? {
          label: t('openInModule', 'Öffnen'),
          callback: () => launcher.open(doc.module),
        } : undefined,
      });
    });
    return () => sub.unsubscribe?.();
  }

  function titleForModule(moduleId) {
    const entry = launcher.entries().find((mod) => mod.id === moduleId);
    return entry?.title || moduleId || '';
  }

  function desktopIconLabel(doc) {
    const targetModule = String(doc?.target_module || '').trim();
    const persisted = String(doc?.label || '').trim();
    // Preserve explicit user renames. Only migrate the historical technical
    // launcher label, which otherwise reads like a second product app.
    if (targetModule === 'ctox' && (!persisted || persisted === 'CTOX')) {
      return 'CTOX Backend';
    }
    return persisted || titleForModule(targetModule);
  }

  function composeCommandToast(doc) {
    const moduleTitle = titleForModule(doc.module);
    return `${moduleTitle ? `[${moduleTitle}] ` : ''}${doc.command_type || ''}`.trim() || doc.command_id || '';
  }

  async function ensureLayout(collection, launcherRef) {
    if (!collection) return defaultLayout(launcherRef);
    try {
      const existing = await collection.findOne(LAYOUT_DOC_ID).exec();
      if (existing) return existing.toJSON();
      const seed = {
        id: LAYOUT_DOC_ID,
        ...defaultLayout(launcherRef),
        updated_at_ms: Date.now(),
      };
      await upsertSeed(collection, seed.id, seed);
      return seed;
    } catch (error) {
      if (!isDatabaseClosingError(error)) throw error;
      console.info('[desktop] layout read skipped during database restart; using default layout');
      return defaultLayout(launcherRef);
    }
  }

  function defaultLayout(launcherRef) {
    return {
      wallpaper_url: '',
      wallpaper_mode: 'cover',
      taskbar_pins: ['ctox', 'documents', 'explorer', 'knowledge', 'research']
        .filter((id) => launcherRef.knows(id)),
      grid_cell_w: DEFAULT_GRID.cellW,
      grid_cell_h: DEFAULT_GRID.cellH,
      grid_offset: DEFAULT_GRID.offset,
    };
  }

  function currentGrid() {
    const surfaceWidth = refs.surface?.getBoundingClientRect?.().width || globalThis.innerWidth || 0;
    // Keep in sync with the compact (touch) grid breakpoint in index.css:
    // @media (max-width: 767px) switches icons to a scrollable flow grid.
    if (surfaceWidth > 0 && surfaceWidth <= 767) {
      return { ...COMPACT_GRID, compact: true };
    }
    return {
      cellW: Math.max(DEFAULT_GRID.cellW, layout?.grid_cell_w || DEFAULT_GRID.cellW),
      cellH: Math.max(DEFAULT_GRID.cellH, layout?.grid_cell_h || DEFAULT_GRID.cellH),
      offset: layout?.grid_offset || DEFAULT_GRID.offset,
      compact: false,
    };
  }

  function plainIconDoc(doc) {
    if (!doc) return {};
    try {
      return typeof doc.toJSON === 'function' ? doc.toJSON() : doc;
    } catch {
      return doc;
    }
  }

  function readIconPositionCache() {
    const positions = new Map();
    try {
      const raw = window.localStorage.getItem(desktopIconPositionCacheStorageKey());
      const parsed = JSON.parse(raw || 'null');
      if (parsed?.version !== ICON_POSITION_CACHE_VERSION) return positions;
      const entries = parsed?.positions && typeof parsed.positions === 'object' ? parsed.positions : {};
      for (const [id, value] of Object.entries(entries)) {
        const x = Number(value?.x);
        const y = Number(value?.y);
        if (!id || !Number.isFinite(x) || !Number.isFinite(y)) continue;
        positions.set(id, {
          x,
          y,
          updated_at_ms: Number(value?.updated_at_ms) || 0,
        });
      }
    } catch {}
    return positions;
  }

  function writeIconPositionCache() {
    const positions = {};
    for (const [id, value] of iconPositionCache) {
      if (!id || !Number.isFinite(value?.x) || !Number.isFinite(value?.y)) continue;
      positions[id] = {
        x: value.x,
        y: value.y,
        updated_at_ms: Number(value.updated_at_ms) || 0,
      };
    }
    try {
      window.localStorage.setItem(desktopIconPositionCacheStorageKey(), JSON.stringify({
        version: ICON_POSITION_CACHE_VERSION,
        positions,
      }));
    } catch {}
  }

  function rememberIconPosition(iconId, position, updatedAt = Date.now()) {
    const x = Number(position?.x);
    const y = Number(position?.y);
    if (!iconId || !Number.isFinite(x) || !Number.isFinite(y)) return;
    iconPositionCache.set(iconId, { x, y, updated_at_ms: updatedAt });
    writeIconPositionCache();
  }

  function syncIconPositionCacheFromDocs(docs) {
    let changed = false;
    for (const doc of docs || []) {
      const plain = plainIconDoc(doc);
      if (!plain?.id || !Number.isFinite(plain.x) || !Number.isFinite(plain.y)) continue;
      const cached = iconPositionCache.get(plain.id);
      const docUpdatedAt = Number(plain.updated_at_ms) || 0;
      if (!cached) {
        iconPositionCache.set(plain.id, {
          x: plain.x,
          y: plain.y,
          updated_at_ms: docUpdatedAt,
        });
        changed = true;
      }
    }
    if (changed) writeIconPositionCache();
  }

  function applyCachedIconPosition(doc) {
    if (!doc?.id) return doc || {};
    const cached = iconPositionCache.get(doc.id);
    if (!cached) return doc;
    const docUpdatedAt = Number(doc.updated_at_ms) || 0;
    const cachedUpdatedAt = Number(cached.updated_at_ms) || 0;
    if (cachedUpdatedAt < docUpdatedAt) return doc;
    if (!Number.isFinite(cached.x) || !Number.isFinite(cached.y)) return doc;
    return {
      ...doc,
      x: cached.x,
      y: cached.y,
      updated_at_ms: Math.max(docUpdatedAt, cachedUpdatedAt),
    };
  }

  function desktopIconPositionCacheStorageKey() {
    const user = ctx.session?.user || {};
    const workspace = window.CTOX_BUSINESS_OS_CONFIG?.instance_id
      || ctx.session?.workspace_id
      || ctx.session?.workspaceId
      || 'workspace';
    const actor = user.id || user.user_id || user.email || user.login || (ctx.session?.authenticated ? 'authenticated' : 'browser');
    return `${ICON_POSITION_CACHE_KEY}.${storageKeyPart(workspace)}.${storageKeyPart(actor)}`;
  }

  function storageKeyPart(value) {
    return String(value || '')
      .trim()
      .toLowerCase()
      .replace(/[^a-z0-9_.-]+/g, '_')
      .slice(0, 96) || 'default';
  }

  function gridPosition(index, grid = currentGrid()) {
    const surfaceRect = refs.surface?.getBoundingClientRect();
    const usableWidth = Math.max(grid.cellW, (surfaceRect?.width || 1024) - grid.offset * 2);
    const columns = Math.max(1, Math.floor(usableWidth / grid.cellW));
    return {
      x: grid.offset + (index % columns) * grid.cellW,
      y: grid.offset + Math.floor(index / columns) * grid.cellH,
    };
  }

  async function ensureIcons(collection, launcherRef, { force = false } = {}) {
    if (!collection) return;
    try {
      const existing = await collection.find().exec();
      const grid = currentGrid();
      const entries = launcherRef.entries();
      const existingById = new Map(existing.map((doc) => [doc.id, doc]));
      const visibleLauncherIcons = existing.filter((doc) => !doc.hidden && launcherRef.knows(doc.target_module));
      const shouldUnhideDefaults = force || !visibleLauncherIcons.length;
      const seeds = entries.map((entry, index) => ({
        ...iconSeedForEntry(entry, index, grid, launcherRef),
        hidden: shouldUnhideDefaults ? false : undefined,
      }));
      await Promise.all(seeds.map(async (seed) => {
        const existingDoc = existingById.get(seed.id);
        if (existingDoc && !force) {
          const patch = normalizeIconPatch(existingDoc, seed, grid, shouldUnhideDefaults);
          clearUnpersistedIconFields(existingDoc, patch);
          if (Object.keys(patch).length) await existingDoc.incrementalPatch(patch);
          return;
        }
        await insertMissingSeed(collection, seed.id, { ...seed, hidden: false });
      }));
      await normalizeIconLayoutIfNeeded(collection, launcherRef);
    } catch (error) {
      if (!isDatabaseClosingError(error)) throw error;
      console.info('[desktop] icon seed skipped during database restart; using transient launcher icons');
    }
  }

  function fallbackIconDocs(launcherRef) {
    const grid = currentGrid();
    return launcherRef.entries().map((entry, index) => iconSeedForEntry(entry, index, grid, launcherRef));
  }

  function iconSeedForEntry(entry, index, grid, launcherRef) {
    const position = gridPosition(index, grid);
    return {
      id: `desk_icon_${entry.id}`,
      target_type: entry.kind || 'module',
      target_module: entry.id,
      target_record_id: '',
      label: entry.title || entry.id,
      glyph: launcherRef.glyphFor(entry.id),
      x: position.x,
      y: position.y,
      pinned: false,
      hidden: false,
      sort_index: index,
      updated_at_ms: Date.now(),
    };
  }

  function normalizeIconPatch(doc, seed, grid, shouldUnhide) {
    const patch = {};
    const position = clampIconPosition(doc, seed, grid);
    if (position.x !== doc.x) patch.x = position.x;
    if (position.y !== doc.y) patch.y = position.y;
    if (!doc.target_type) patch.target_type = seed.target_type;
    if (!doc.target_module) patch.target_module = seed.target_module;
    if (!doc.label || (doc.id === 'desk_icon_research' && doc.target_module === 'research' && doc.label === 'Research')) patch.label = seed.label;
    if (!isSafePersistedGlyph(doc.glyph)) patch.glyph = seed.glyph;
    if (!Number.isFinite(doc.sort_index)) patch.sort_index = seed.sort_index;
    if (shouldUnhide && doc.hidden) patch.hidden = false;
    if (Object.keys(patch).length) patch.updated_at_ms = Date.now();
    return patch;
  }

  function clearUnpersistedIconFields(doc, patch) {
    const raw = typeof doc?.toJSON === 'function' ? doc.toJSON() : doc;
    for (const key of Object.keys(raw || {})) {
      if (key.startsWith('_') || DESKTOP_ICON_PERSISTED_FIELDS.has(key)) continue;
      if (raw[key] === undefined) continue;
      patch[key] = undefined;
    }
  }

  function isSafePersistedGlyph(value) {
    const glyph = String(value || '').trim();
    if (!glyph || glyph.length > 16) return false;
    return !/^(?:data:|https?:)/i.test(glyph) && !/[<>{}]/.test(glyph);
  }

  function clampIconPosition(doc, fallback, grid) {
    const surfaceRect = refs.surface?.getBoundingClientRect();
    const iconWidth = grid.compact ? ICON_METRICS.compactWidth : ICON_METRICS.width;
    const iconHeight = grid.compact ? ICON_METRICS.compactHeight : ICON_METRICS.height;
    const maxX = Math.max(grid.offset, (surfaceRect?.width || 1024) - iconWidth - 8);
    const maxY = Math.max(grid.offset, (surfaceRect?.height || 720) - iconHeight - 8);
    const x = Number.isFinite(doc.x) ? doc.x : fallback.x;
    const y = Number.isFinite(doc.y) ? doc.y : fallback.y;
    return {
      x: Math.max(grid.offset, Math.min(x, maxX)),
      y: Math.max(grid.offset, Math.min(y, maxY)),
    };
  }

  async function normalizeIconLayoutIfNeeded(collection, launcherRef) {
    const docs = (await collection.find().exec())
      .filter((doc) => !doc.hidden && launcherRef.knows(doc.target_module))
      .sort((a, b) => (a.sort_index ?? 0) - (b.sort_index ?? 0));
    const migrationKey = `${desktopIconPositionCacheStorageKey()}.${ROW_MAJOR_LAYOUT_MIGRATION}`;
    let needsRowMajorMigration = true;
    try {
      needsRowMajorMigration = window.localStorage.getItem(migrationKey) !== 'complete';
    } catch {}
    const seen = new Set();
    let hasCollision = false;
    for (const doc of docs) {
      const key = `${Math.round(doc.x || 0)}:${Math.round(doc.y || 0)}`;
      if (seen.has(key)) {
        hasCollision = true;
        break;
      }
      seen.add(key);
    }
    if (!hasCollision && !needsRowMajorMigration) return;
    const grid = currentGrid();
    await Promise.all(docs.map((doc, index) => {
      const position = gridPosition(index, grid);
      iconPositionCache.set(doc.id, {
        x: position.x,
        y: position.y,
        updated_at_ms: Date.now() + index,
      });
      return doc.incrementalPatch({
        x: position.x,
        y: position.y,
        sort_index: index,
        updated_at_ms: Date.now(),
      });
    }));
    writeIconPositionCache();
    try {
      window.localStorage.setItem(migrationKey, 'complete');
    } catch {}
  }

  async function upsertSeed(collection, id, seed) {
    const existing = await collection.findOne(id).exec();
    if (existing) {
      await existing.incrementalPatch(seed);
      return;
    }
    try {
      await collection.insert(seed);
    } catch (error) {
      if (!isConflictError(error)) throw error;
      const conflicted = await collection.findOne(id).exec();
      if (!conflicted) throw error;
      await conflicted.incrementalPatch(seed);
    }
  }

  async function insertMissingSeed(collection, id, seed) {
    try {
      await collection.insert(seed);
    } catch (error) {
      if (!isConflictError(error)) throw error;
      const conflicted = await collection.findOne(id).exec();
      if (!conflicted) throw error;
      await conflicted.incrementalPatch(seed);
    }
  }

  function isConflictError(error) {
    const status = error?.status || error?.parameters?.writeError?.status;
    if (status === 409) return true;
    const code = String(error?.code || error?.rxdb || '').toUpperCase();
    if (code === 'CONFLICT') return true;
    const message = String(error?.message || error || '').toLowerCase();
    return message.includes('conflict') || message.includes('already');
  }
}

async function loadModuleRegistry() {
  try {
    const response = await fetch(new URL('../registry.json', import.meta.url));
    if (!response.ok) return [];
    const data = await response.json();
    return Array.isArray(data?.modules)
      ? data.modules.filter((module) => ['core', 'internal'].includes(module?.install_scope))
      : [];
  } catch (error) {
    console.error('[desktop] registry load failed:', error);
    return [];
  }
}

async function ensureStyles() {
  const href = `${new URL('./index.css', import.meta.url).pathname}?v=${STYLE_BUILD}`;
  if (document.querySelector(`link[href="${href}"]`)) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  document.head.append(link);
}
