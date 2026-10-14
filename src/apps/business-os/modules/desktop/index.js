import { loadModuleMessages } from '../../shared/i18n.js';
import { showBusinessConfirm, showBusinessPrompt } from '../../shared/dialogs.js?v=20260816-browser-sync-guards-v141';
import { createCtoxLauncher } from './ctoxLauncher.js';
import { addMissingDesktopIcons, arrangeDesktopIcons, desktopIconWriteAvailability, dispatchDesktopChatOpen, replaceDesktopIcons, runDesktopActionOnce } from './desktopMenuActions.js';
import { ensureDesktopLayoutWithAuthority, isDatabaseClosingError, readLocalDesktopIcons, readLocalDesktopLayout, withDesktopIconReconciliationRead } from './layout-authority.js?v=20260929-desktop-icon-cancel-v1';
import { makeIconDraggable } from './iconDrag.js?v=20260816-browser-sync-guards-v141';
import { getSvgIcon as getFallbackSvgIcon } from '../../shared/icons.js?v=20260816-browser-sync-guards-v141';
import {
  isRuntimeInstalledApp,
  moduleRenamePayload,
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
    openInModule: 'Öffnen',
    openUnavailable: 'App nicht verfügbar.',
    chatWithCtox: 'Mit CTOX chatten',
    chatNotSaved: 'Das hat nicht geklappt.',
    askCtox: 'Frage stellen',
    workWithData: 'Daten ändern',
    modifyApp: 'App ändern',
    createApp: 'App Creator öffnen',
    renameApp: 'App umbenennen',
    renameAppDone: 'App umbenannt',
    chatContextLabel: 'Desktop-Kontext',
    pinToTaskbar: 'An Bar anheften',
    unpinFromTaskbar: 'Von Bar lösen',
    renameIcon: 'Icon umbenennen',
    deleteIcon: 'Icon entfernen',
    arrangeIcons: 'Icons ausrichten',
    sortIcons: 'Icons nach Namen sortieren',
    addMissingIcons: 'Fehlende Standard-Icons hinzufügen',
    openExplorer: 'Explorer öffnen',
    openNotes: 'Notizen öffnen',
    iconRestoreDefaults: 'Desktop-Icons zurücksetzen',
    maintenance: 'Desktop-Wartung…',
    restoreArrangement: 'Vorherige Anordnung wiederherstellen',
    restoreConfirm: 'Aktuelle Desktop-Icons durch die vor dem letzten Zurücksetzen gespeicherte Anordnung ersetzen?',
    backupUnavailable: 'Die vorherige Anordnung konnte nicht gesichert werden. Zurücksetzen abgebrochen.',
    noBackup: 'Keine vorherige Desktop-Anordnung gespeichert.',
    resetConfirm: 'Alle Desktop-Verknüpfungen und eigenen Anordnungen entfernen und Standard-Icons wiederherstellen?',
    refresh: 'Desktop-Icons neu laden',
    iconsUnavailable: 'Das hat nicht geklappt.',
    menuActionFailed: 'Das hat nicht geklappt.',
    noMissingIcons: 'Alle verfügbaren Standard-Icons sind bereits vorhanden.',
    iconsAdded: 'Fehlende Standard-Icons wurden hinzugefügt.',
    iconsArranged: 'Desktop-Icons wurden ausgerichtet.',
    iconsSorted: 'Desktop-Icons wurden nach Namen sortiert.',
  },
  en: {
    moduleTitle: 'Desktop',
    emptyDesktop: 'No icons on the desktop.',
    openInModule: 'Open',
    openUnavailable: 'App unavailable.',
    chatWithCtox: 'Chat with CTOX',
    chatNotSaved: 'That didn’t work.',
    askCtox: 'Ask question',
    workWithData: 'Change data',
    modifyApp: 'Modify app',
    createApp: 'Open App Creator',
    renameApp: 'Rename app',
    renameAppDone: 'App renamed',
    chatContextLabel: 'Desktop context',
    pinToTaskbar: 'Pin to bar',
    unpinFromTaskbar: 'Unpin from bar',
    renameIcon: 'Rename icon',
    deleteIcon: 'Remove icon',
    arrangeIcons: 'Arrange icons',
    sortIcons: 'Sort icons by name',
    addMissingIcons: 'Add missing default icons',
    openExplorer: 'Open Explorer',
    openNotes: 'Open Notes',
    iconRestoreDefaults: 'Reset desktop icons',
    maintenance: 'Desktop maintenance…',
    restoreArrangement: 'Restore previous arrangement',
    restoreConfirm: 'Replace current desktop icons with the arrangement saved before the last reset?',
    backupUnavailable: 'Previous arrangement could not be saved. Reset cancelled.',
    noBackup: 'No previous desktop arrangement is saved.',
    resetConfirm: 'Remove all desktop shortcuts and custom positions, then restore the default icons?',
    refresh: 'Reload desktop icons',
    iconsUnavailable: 'That didn’t work.',
    menuActionFailed: 'That didn’t work.',
    noMissingIcons: 'All available default icons are already present.',
    iconsAdded: 'Missing default icons were added.',
    iconsArranged: 'Desktop icons were arranged.',
    iconsSorted: 'Desktop icons were sorted by name.',
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
  };
  refs.surface.tabIndex = -1;
  applyWorkjetCategory(refs.root, workjetCategoryForModule(ctx.module));

  const initialModules = Array.isArray(ctx.modules) ? ctx.modules : await loadModuleRegistry();
  let launcher = createLauncher(initialModules);

  const cleanups = [];
  const pendingActions = new Set();
  let disposed = false;

  // Wire up the live clock and date widget
  let startClockTimer = null;
  const timeEl = refs.root.querySelector('[data-widget-time]');
  const dateEl = refs.root.querySelector('[data-widget-date]');
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
    // Start only after the local first paint succeeds, so a failed mount cannot
    // leave a clock ticking behind the shell's loading state.
    startClockTimer = () => {
      const clockInterval = setInterval(updateClock, 1000);
      cleanups.push(() => clearInterval(clockInterval));
    };
  }
  const layoutCollection = ctx.db?.collection?.('desktop_layout');
  const iconsCollection = ctx.db?.collection?.('desktop_icons');

  let layout = await readLocalDesktopLayout({
    collection: layoutCollection,
    documentId: LAYOUT_DOC_ID,
    defaultLayout: () => defaultLayout(launcher),
    onDatabaseClosing: () => console.info('[desktop] local layout unavailable during database restart'),
  });
  let iconPositionCache = readIconPositionCache();
  let iconsReadiness = readIconsReadiness();
  await renderIcons();
  startClockTimer?.();

  cleanups.push(subscribeIcons());
  cleanups.push(subscribeIconsReadiness());
  cleanups.push(subscribeModuleCatalogChanges());

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

  // Locally saved icons are already visible. Native layout authority and icon
  // repair continue after mount instead of holding the shell loading skeleton.
  const reconciliationTimer = setTimeout(() => {
    if (disposed) return;
    void (async () => {
      const previousGrid = [layout?.grid_cell_w, layout?.grid_cell_h, layout?.grid_offset];
      const reconciledLayout = await ensureLayout(layoutCollection, launcher, () => layout);
      if (disposed) return;
      layout = reconciledLayout;
      const nextGrid = [layout?.grid_cell_w, layout?.grid_cell_h, layout?.grid_offset];
      if (previousGrid.some((value, index) => value !== nextGrid[index])) {
        renderIcons.lastSignature = null;
        await renderIcons();
      }
      if (!disposed) await ensureIcons(iconsCollection, launcher);
    })().catch((error) => {
      if (disposed || isDatabaseClosingError(error)) return;
      if (showManagedAuthorizationError(error)) return;
      console.error('[desktop] background layout/icon reconciliation failed:', error);
    });
  }, 0);
  cleanups.push(() => clearTimeout(reconciliationTimer));

  return () => {
    disposed = true;
    for (const dispose of cleanups) {
      try { dispose?.(); } catch (error) { console.error('[desktop] cleanup failed:', error); }
    }
  };

  // ---------- helpers (closures over the mount scope) ----------

  // The clock rewrites the same minute string many times a second. Skip the
  // text-node replacement unless the value actually moved.
  function setTextIfChanged(element, value) {
    const next = String(value ?? '');
    if (!element || element.textContent === next) return;
    element.textContent = next;
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
    const { docs, usingFallbackDocs } = await readLocalDesktopIcons({
      collection: iconsCollection,
      fallbackIcons: () => fallbackIconDocs(launcher),
    });
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
        try {
          if (!iconsCollection) throw new Error(t('iconsUnavailable'));
          const existing = await iconsCollection.findOne(iconId).exec();
          if (!existing) throw new Error(t('iconsUnavailable'));
          await existing.incrementalPatch({ ...position, updated_at_ms: updatedAt });
        } catch (error) {
          iconPositionCache.delete(iconId);
          writeIconPositionCache();
          refreshIcons().catch(() => {});
          console.warn('[desktop] icon move failed:', error);
          notify({ type: 'error', title: t('menuActionFailed', 'Das hat nicht geklappt.') });
        }
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
    for (const node of refs.icons.querySelectorAll('.desktop-icon.selected')) node.classList.remove('selected');
    ctx.contextMenu.show(event, [
      { label: t('createApp', 'App Creator öffnen'), icon: '+', disabled: !launcher.knows('creator'), disabledReason: t('openUnavailable'), onDisabled: () => notifyUnavailableTarget('creator'), action: () => openLauncherTarget('creator') },
      { label: t('chatWithCtox', 'Mit CTOX chatten'), icon: '◆', action: safeAction(chatWithCtoxAboutDesktop) },
      { type: 'separator' },
      { label: t('openExplorer', 'Explorer öffnen'), icon: '⌘', disabled: !launcher.knows('explorer'), disabledReason: t('openUnavailable'), onDisabled: () => notifyUnavailableTarget('explorer'), action: () => openLauncherTarget('explorer') },
      { label: t('openNotes', 'Notizen öffnen'), icon: '✎', disabled: !launcher.knows('notes'), disabledReason: t('openUnavailable'), onDisabled: () => notifyUnavailableTarget('notes'), action: () => openLauncherTarget('notes') },
      { type: 'separator' },
      { label: t('arrangeIcons', 'Icons ausrichten'), icon: '▦', ...iconWriteAvailability(), action: safeAction(arrangeIcons, 'icons') },
      { label: t('sortIcons', 'Icons nach Namen sortieren'), icon: '⇅', ...iconWriteAvailability(), action: safeAction(() => arrangeIcons('name'), 'icons') },
      { label: t('addMissingIcons', 'Fehlende Standard-Icons hinzufügen'), icon: '+', ...iconWriteAvailability(), action: safeAction(addMissingDefaultIcons, 'icons') },
      { type: 'separator' },
      { label: t('refresh', 'Desktop-Icons neu laden'), icon: '↻', action: safeAction(refreshIcons, 'refresh') },
      { label: t('maintenance', 'Desktop-Wartung…'), icon: '⋯', action: safeAction(() => showDesktopMaintenance(event)) },
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
        action: safeAction(() => togglePinnedTarget(doc.target_module, !pinned), `pin:${doc.target_module}`),
      },
      { label: t('askCtox', 'Frage stellen'), icon: '?', action: safeAction(() => chatWithCtoxAboutIcon(doc, 'ask')) },
      { label: t('workWithData', 'Daten ändern'), icon: '◇', action: safeAction(() => chatWithCtoxAboutIcon(doc, 'data')) },
      { label: t('modifyApp', 'App ändern'), icon: '✦', action: safeAction(() => chatWithCtoxAboutIcon(doc, 'app')) },
      ...(canRenameApp ? [
        { label: t('renameApp', 'App umbenennen'), icon: '✎', action: safeAction(() => renameApp(doc, app), `rename-app:${app.id}`) },
      ] : []),
      { label: t('renameIcon', 'Icon umbenennen'), icon: '✎', ...iconWriteAvailability(), action: safeAction(() => renameIcon(doc), `icon:${doc.id}`) },
      { type: 'separator' },
      { label: t('deleteIcon', 'Icon entfernen'), icon: '−', ...iconWriteAvailability(), action: safeAction(() => deleteIcon(doc.id), `icon:${doc.id}`) },
    ]);
  }

  // launcher.open() meldet ein fehlendes Ziel nur mit `false` zurueck. Alle drei
  // Oeffnen-Wege (Doppelklick, Enter, Kontextmenue) haben diesen Rueckgabewert
  // verworfen: ein Icon, dessen Zielmodul nicht (mehr) im Katalog dieser Instanz
  // steht, tat auf Klick GAR NICHTS — kein Fenster, keine Meldung. Genau so
  // sieht ein "kaputtes" Kontextmenue aus. Der Fehlschlag wird jetzt benannt.
  async function openDesktopTarget(doc) {
    const targetModule = doc?.target_module || '';
    try {
      const opened = await runDesktopActionOnce(pendingActions, `open:${targetModule}`, () => launcher.open(targetModule));
      if (opened === undefined) return false;
      if (opened) return true;
    } catch (error) {
      console.warn('[desktop] open failed:', error);
      notify({ type: 'error', title: t('menuActionFailed', 'Das hat nicht geklappt.') });
      return false;
    }
    notifyUnavailableTarget(targetModule, doc);
    return false;
  }

  function notifyUnavailableTarget(targetModule, doc = null) {
    console.warn('[desktop] app unavailable:', targetModule, doc?.id || '');
    notify({
      type: 'warning',
      title: t('openUnavailable', 'App nicht verfügbar.'),
    });
  }

  function safeAction(action, key = '') {
    return () => {
      runDesktopActionOnce(pendingActions, key || Symbol('desktop-action'), () => Promise.resolve().then(action))
        .catch((error) => {
          console.warn('[desktop] context menu action failed:', error);
          notify({ type: 'error', title: t('menuActionFailed', 'Das hat nicht geklappt.') });
        });
    };
  }

  function openLauncherTarget(targetId) {
    return openDesktopTarget({ target_module: targetId, label: titleForModule(targetId) });
  }

  function iconWriteAvailability() {
    return desktopIconWriteAvailability({
      collection: iconsCollection,
      readiness: iconsReadiness,
      reason: t('iconsUnavailable'),
      notify: (reason) => notify({ type: 'warning', title: reason }),
      retry: safeAction(refreshIcons, 'refresh'),
    });
  }

  function showDesktopMaintenance(event) {
    ctx.contextMenu.show(event, [
      { label: t('iconRestoreDefaults'), icon: '⟳', ...iconWriteAvailability(), action: safeAction(restoreDefaultIcons, 'icons') },
      { label: t('restoreArrangement'), icon: '↶', disabled: !hasResetBackup() || iconWriteAvailability().disabled,
        disabledReason: !hasResetBackup() ? t('noBackup') : t('iconsUnavailable'),
        onDisabled: () => notify({ type: 'warning', title: !hasResetBackup() ? t('noBackup') : t('iconsUnavailable') }),
        action: safeAction(restorePreviousArrangement, 'icons') },
    ]);
  }

  async function refreshIcons() {
    renderIcons.lastSignature = '';
    await renderIcons();
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
    if (!iconsCollection) throw new Error(t('iconsUnavailable'));
    const existing = await iconsCollection.findOne(iconId).exec();
    if (existing) await existing.incrementalPatch({ hidden: true, updated_at_ms: Date.now() });
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
    return openCtoxChat(detail);
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
    return openCtoxChat(detail);
  }

  function openCtoxChat(detail) {
    return dispatchDesktopChatOpen({
      detail,
      openBusinessChat: ctx.openBusinessChat,
      dispatchEvent: (chatDetail) => window.dispatchEvent(new CustomEvent('ctox-business-os-chat-open', { detail: chatDetail })),
      onPersistError: (error) => {
        console.warn('[desktop] chat persist failed:', error);
        notify({
          type: 'error',
          title: t('chatNotSaved', 'Das hat nicht geklappt.'),
        });
      },
    });
  }

  async function restoreDefaultIcons() {
    if (!iconsCollection) throw new Error(t('iconsUnavailable'));
    const confirmed = await showBusinessConfirm(t('resetConfirm'), {
      title: t('iconRestoreDefaults'),
      confirmLabel: t('iconRestoreDefaults'),
    });
    if (!confirmed) return;
    const all = await iconsCollection.find().exec();
    const snapshot = all.map((doc) => Object.fromEntries(
      Object.entries(plainIconDoc(doc)).filter(([key]) => DESKTOP_ICON_PERSISTED_FIELDS.has(key)),
    ));
    try {
      window.localStorage.setItem(resetBackupStorageKey(), JSON.stringify(snapshot));
    } catch {
      throw new Error(t('backupUnavailable'));
    }
    const grid = currentGrid();
    const defaults = launcher.entries().map((entry, index) => iconSeedForEntry(entry, index, grid, launcher));
    await replaceDesktopIcons({ collection: iconsCollection, snapshot: defaults, insertMissingSeed });
    iconPositionCache = new Map();
    writeIconPositionCache();
    await refreshIcons();
  }

  async function restorePreviousArrangement() {
    if (!iconsCollection) throw new Error(t('iconsUnavailable'));
    const snapshot = readResetBackup();
    if (!snapshot) throw new Error(t('backupUnavailable'));
    const confirmed = await showBusinessConfirm(t('restoreConfirm'), { title: t('restoreArrangement') });
    if (!confirmed) return;
    await replaceDesktopIcons({
      collection: iconsCollection,
      snapshot: snapshot.map((icon) => ({ ...icon, updated_at_ms: Date.now() })),
      insertMissingSeed,
    });
    iconPositionCache = new Map();
    writeIconPositionCache();
    window.localStorage.removeItem(resetBackupStorageKey());
    await refreshIcons();
  }

  function resetBackupStorageKey() {
    return `${desktopIconPositionCacheStorageKey()}.reset-backup`;
  }

  function readResetBackup() {
    try {
      const snapshot = JSON.parse(window.localStorage.getItem(resetBackupStorageKey()) || 'null');
      return Array.isArray(snapshot) && snapshot.every((icon) => icon?.id && icon?.target_type) ? snapshot : null;
    } catch { return null; }
  }

  function hasResetBackup() {
    return readResetBackup() !== null;
  }

  async function addMissingDefaultIcons() {
    if (!iconsCollection) throw new Error(t('iconsUnavailable'));
    const count = await addMissingDesktopIcons({
      collection: iconsCollection,
      entries: launcher.entries(),
      gridPosition,
      glyphFor: launcher.glyphFor,
      insertMissingSeed,
    });
    if (!count) {
      notify({ type: 'info', title: t('noMissingIcons') });
      return;
    }
    await refreshIcons();
    notify({ type: 'success', title: t('iconsAdded') });
  }

  async function arrangeIcons(order = 'current') {
    if (!iconsCollection) throw new Error(t('iconsUnavailable'));
    await arrangeDesktopIcons({
      collection: iconsCollection,
      knows: launcher.knows,
      labelFor: desktopIconLabel,
      gridPosition,
      rememberPosition: rememberIconPosition,
      order,
      locale: ctx.locale || 'de',
    });
    await refreshIcons();
    notify({ type: 'success', title: t(order === 'name' ? 'iconsSorted' : 'iconsArranged') });
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


  function showManagedAuthorizationError(error) {
    const message = String(error?.message || error || '');
    if (!/UNAUTHORIZED: peer is not authorized for this collection/i.test(message)) return false;
    globalThis.showStartupError?.(error);
    return true;
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

  async function ensureLayout(collection, launcherRef, unknownLayout) {
    return ensureDesktopLayoutWithAuthority({
      collection,
      documentId: LAYOUT_DOC_ID,
      defaultLayout: () => defaultLayout(launcherRef),
      unknownLayout,
      isCurrent: () => !disposed,
      readNativeDocument: ctx.readNativeCollectionDocument
        ? () => ctx.readNativeCollectionDocument('desktop_layout', LAYOUT_DOC_ID, { timeoutMs: 5000 })
        : null,
      insertMissingSeed,
      onDatabaseClosing: () => console.info('[desktop] layout read skipped during database restart; using default layout'),
    });
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
    if (disposed || !collection) return;
    try {
      await withDesktopIconReconciliationRead(collection, async (existing) => {
        if (disposed) return;
        const grid = currentGrid();
        const entries = launcherRef.entries();
        const existingById = new Map(existing.map((doc) => [doc.id, doc]));
        const shouldUnhideDefaults = force;
        const seeds = entries.map((entry, index) => ({
          ...iconSeedForEntry(entry, index, grid, launcherRef),
          hidden: shouldUnhideDefaults ? false : undefined,
        }));
        await Promise.all(seeds.map(async (seed) => {
          if (disposed) return;
          const existingDoc = existingById.get(seed.id);
          if (existingDoc && !force) {
            const patch = normalizeIconPatch(existingDoc, seed, grid, shouldUnhideDefaults);
            clearUnpersistedIconFields(existingDoc, patch);
            if (Object.keys(patch).length) await existingDoc.incrementalPatch(patch);
            return;
          }
          await insertMissingSeed(collection, seed.id, { ...seed, hidden: false });
        }));
        if (!disposed) await normalizeIconLayoutIfNeeded(collection, launcherRef);
      });
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
    if (disposed) return;
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
      if (disposed) return undefined;
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

  async function insertMissingSeed(collection, id, seed) {
    try {
      await collection.insert(seed);
    } catch (error) {
      if (!isConflictError(error)) throw error;
      return;
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
