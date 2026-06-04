import { loadModuleMessages } from '../../shared/i18n.js';
import { showBusinessPrompt } from '../../shared/dialogs.js';
import { createCtoxLauncher } from './ctoxLauncher.js';
import { makeIconDraggable } from './iconDrag.js';
import { getSvgIcon } from '../../shared/icons.js?v=20260604-rxdb-shared-catchup1';

const STYLE_BUILD = '20260604-rxdb-shared-catchup1';
const LAYOUT_DOC_ID = 'layout';
const DEFAULT_GRID = { cellW: 104, cellH: 120, offset: 24 };
const COMPACT_GRID = { cellW: 88, cellH: 100, offset: 12 };
const ICON_METRICS = {
  width: 96,
  height: 104,
  compactWidth: 80,
  compactHeight: 96,
};

const FALLBACK_LABELS = {
  de: {
    moduleTitle: 'Desktop',
    emptyDesktop: 'Keine Icons auf dem Desktop.',
    openInModule: 'Öffnen',
    chatWithCtox: 'Mit CTOX chatten',
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
    ctoxLiveActivity: 'CTOX live',
  },
  en: {
    moduleTitle: 'Desktop',
    emptyDesktop: 'No icons on the desktop.',
    openInModule: 'Open',
    chatWithCtox: 'Chat with CTOX',
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
    ctoxLiveActivity: 'CTOX live',
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
  };

  const launcher = createCtoxLauncher({
    modules: await loadModuleRegistry(),
    apps: ctx.desktopApps || [],
    currentModuleId: ctx.module.id,
    openApp: ctx.openDesktopApp,
  });

  const cleanups = [];

  // Wire up the live clock and date widget
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
        timeEl.textContent = now.toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit' });
        dateEl.textContent = now.toLocaleDateString(locale, { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' });
      } catch (e) {
        console.error('[desktop] clock update failed with locale:', ctx?.locale, e);
        try {
          const now = new Date();
          timeEl.textContent = now.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
          dateEl.textContent = now.toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' });
        } catch (fallbackErr) {
          console.error('[desktop] clock absolute fallback failed:', fallbackErr);
        }
      }
    };
    updateClock();
    const clockInterval = setInterval(updateClock, 1000);
    cleanups.push(() => clearInterval(clockInterval));
  }
  const layoutCollection = ctx.db?.collection?.('desktop_layout');
  const iconsCollection = ctx.db?.collection?.('desktop_icons');
  const commandsCollection = ctx.db?.collection?.('business_commands');

  const layout = await ensureLayout(layoutCollection, launcher);
  await ensureIcons(iconsCollection, launcher);
  await renderIcons();

  cleanups.push(subscribeIcons());
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
    for (const dispose of cleanups) {
      try { dispose?.(); } catch (error) { console.error('[desktop] cleanup failed:', error); }
    }
  };

  // ---------- helpers (closures over the mount scope) ----------

  async function renderIcons() {
    const docs = iconsCollection ? await iconsCollection.find().exec() : [];
    refs.icons.innerHTML = '';
    if (!docs.length) {
      const empty = document.createElement('div');
      empty.className = 'desktop-icon-empty';
      empty.textContent = t('emptyDesktop', 'Keine Icons auf dem Desktop.');
      refs.icons.appendChild(empty);
      return;
    }
    const sorted = [...docs].sort((a, b) => (a.sort_index ?? 0) - (b.sort_index ?? 0));
    for (const doc of sorted) {
      if (doc.hidden) continue;
      refs.icons.appendChild(buildIcon(doc));
    }
  }

  function buildIcon(doc) {
    const el = document.createElement('div');
    el.className = 'desktop-icon';
    el.dataset.iconId = doc.id;
    el.dataset.target = doc.target_module || '';
    el.style.left = `${doc.x ?? DEFAULT_GRID.offset}px`;
    el.style.top = `${doc.y ?? DEFAULT_GRID.offset}px`;
    el.innerHTML = `
      <div class="desktop-icon-glyph" aria-hidden="true"></div>
      <div class="desktop-icon-label"></div>
    `;
    
    const glyphEl = el.querySelector('.desktop-icon-glyph');
    const targetModule = doc.target_module || '';
    const svgIcon = getSvgIcon(targetModule, 28);
    if (svgIcon) {
      glyphEl.innerHTML = svgIcon;
    } else {
      glyphEl.textContent = doc.glyph || launcher.glyphFor(targetModule);
    }
    
    el.querySelector('.desktop-icon-label').textContent = doc.label || titleForModule(doc.target_module);
    el.title = doc.label || titleForModule(doc.target_module);
    el.tabIndex = 0;

    el.addEventListener('dblclick', () => launcher.open(doc.target_module));
    el.addEventListener('keydown', (event) => {
      if (event.key === 'Enter' || event.key === ' ') {
        event.preventDefault();
        launcher.open(doc.target_module);
      }
    });
    el.addEventListener('contextmenu', (event) => onIconContextMenu(event, doc));

    makeIconDraggable(el, {
      surface: refs.surface,
      iconId: doc.id,
      grid: currentGrid(),
      onSelect: () => {
        for (const node of refs.icons.querySelectorAll('.desktop-icon.selected')) {
          node.classList.remove('selected');
        }
        el.classList.add('selected');
      },
      onMoved: async (iconId, position) => {
        const existing = await iconsCollection?.findOne(iconId).exec();
        if (existing) {
          await existing.incrementalPatch({
            x: position.x,
            y: position.y,
            updated_at_ms: Date.now(),
          });
        }
      },
      onDragToTopbar: (iconId) => {
        if (doc.target_module) {
          togglePinnedTarget(doc.target_module, true);
        }
      },
    });

    return el;
  }

  function onSurfaceContextMenu(event) {
    if (event.target.closest('.desktop-icon')) return;
    if (!ctx.contextMenu) return;
    ctx.contextMenu.show(event, [
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
    if (!ctx.contextMenu) return;
    const pinned = isPinnedTarget(doc.target_module);
    ctx.contextMenu.show(event, [
      { label: t('openInModule', 'Öffnen'), icon: '↗', action: () => launcher.open(doc.target_module) },
      {
        label: pinned ? t('unpinFromTaskbar', 'Von Bar lösen') : t('pinToTaskbar', 'An Bar anheften'),
        icon: pinned ? '−' : '+',
        action: safeAction(() => togglePinnedTarget(doc.target_module, !pinned)),
      },
      { label: t('chatWithCtox', 'Mit CTOX chatten'), icon: '◆', action: safeAction(() => chatWithCtoxAboutIcon(doc)) },
      { label: t('renameIcon', 'Icon umbenennen'), icon: '✎', action: safeAction(() => renameIcon(doc)) },
      { type: 'separator' },
      { label: t('deleteIcon', 'Icon entfernen'), icon: '−', action: safeAction(() => deleteIcon(doc.id)) },
    ]);
  }

  function safeAction(action) {
    return () => {
      Promise.resolve()
        .then(action)
        .catch((error) => console.error('[desktop] context menu action failed:', error));
    };
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

  function chatWithCtoxAboutIcon(doc) {
    const label = doc.label || titleForModule(doc.target_module) || doc.id || 'Desktop object';
    const targetKind = doc.target_type || launcher.kindOf(doc.target_module) || 'module';
    const targetModule = doc.target_module || '';
    const recordId = doc.target_record_id || '';
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
    const draft = `Bitte hilf mir mit "${label}". `;
    const detail = {
      title: `CTOX · ${label}`,
      draft,
      context_text: contextText,
      context_label: t('chatContextLabel', 'Desktop-Kontext'),
      module: 'desktop',
      source_module: 'desktop',
      source_title: t('moduleTitle', 'Desktop'),
      record_id: doc.id || recordId || targetModule,
      command_title: `Desktop: ${label}`,
      payload: {
        desktop_icon: iconContext,
        file_context: iconContext,
      },
      client_context: {
        source: 'desktop-icon-context-menu',
        desktop_icon_id: doc.id || '',
        target_type: targetKind,
        target_module: targetModule,
        target_record_id: recordId,
      },
    };
    openCtoxChat(detail);
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
    const all = await iconsCollection.find().exec();
    await Promise.all(all.map((doc) => doc.remove()));
    await ensureIcons(iconsCollection, launcher, { force: true });
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
    await Promise.all(docs.map((doc, index) => {
      const position = gridPosition(index, grid);
      return doc.incrementalPatch({
        x: position.x,
        y: position.y,
        sort_index: index,
        updated_at_ms: Date.now(),
      });
    }));
  }

  function subscribeIcons() {
    if (!iconsCollection?.$) return () => {};
    const sub = iconsCollection.$.subscribe(() => {
      renderIcons().catch((error) => console.error('[desktop] icon render failed:', error));
    });
    return () => sub.unsubscribe?.();
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

  function composeCommandToast(doc) {
    const moduleTitle = titleForModule(doc.module);
    return `${moduleTitle ? `[${moduleTitle}] ` : ''}${doc.command_type || ''}`.trim() || doc.command_id || '';
  }

  async function ensureLayout(collection, launcherRef) {
    if (!collection) return defaultLayout(launcherRef);
    const existing = await collection.findOne(LAYOUT_DOC_ID).exec();
    if (existing) return existing.toJSON();
    const seed = {
      id: LAYOUT_DOC_ID,
      ...defaultLayout(launcherRef),
      updated_at_ms: Date.now(),
    };
    await upsertSeed(collection, seed.id, seed);
    return seed;
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
    if (surfaceWidth > 0 && surfaceWidth <= 560) {
      return { ...COMPACT_GRID, compact: true };
    }
    return {
      cellW: Math.max(104, layout?.grid_cell_w || DEFAULT_GRID.cellW),
      cellH: Math.max(120, layout?.grid_cell_h || DEFAULT_GRID.cellH),
      offset: layout?.grid_offset || DEFAULT_GRID.offset,
      compact: false,
    };
  }

  function gridPosition(index, grid = currentGrid()) {
    const surfaceRect = refs.surface?.getBoundingClientRect();
    const usableHeight = Math.max(grid.cellH, (surfaceRect?.height || 720) - grid.offset * 2);
    const rows = Math.max(1, Math.floor(usableHeight / grid.cellH));
    return {
      x: grid.offset + Math.floor(index / rows) * grid.cellW,
      y: grid.offset + (index % rows) * grid.cellH,
    };
  }

  async function ensureIcons(collection, launcherRef, { force = false } = {}) {
    if (!collection) return;
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
    for (const seed of seeds) {
      const existingDoc = existingById.get(seed.id);
      if (existingDoc && !force) {
        const patch = normalizeIconPatch(existingDoc, seed, grid, shouldUnhideDefaults);
        if (Object.keys(patch).length) await existingDoc.incrementalPatch(patch);
        continue;
      }
      await upsertSeed(collection, seed.id, { ...seed, hidden: false });
    }
    await normalizeIconLayoutIfNeeded(collection, launcherRef);
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
    if (!doc.glyph) patch.glyph = seed.glyph;
    if (!Number.isFinite(doc.sort_index)) patch.sort_index = seed.sort_index;
    if (shouldUnhide && doc.hidden) patch.hidden = false;
    if (Object.keys(patch).length) patch.updated_at_ms = Date.now();
    return patch;
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
    if (!hasCollision) return;
    const grid = currentGrid();
    await Promise.all(docs.map((doc, index) => {
      const position = gridPosition(index, grid);
      return doc.incrementalPatch({
        x: position.x,
        y: position.y,
        sort_index: index,
        updated_at_ms: Date.now(),
      });
    }));
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
    return Array.isArray(data?.modules) ? data.modules : [];
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
