// Desktop – App-API 4.0 (Shell v4, Strang UC4). Port-Protokoll: V4-PORT.md.
//
// Die App ist der Arbeitsflächen-Hintergrund: Desktop-Icons (desktop_icons), Raster (desktop_layout), Uhr/Datum,
// Kontextmenüs. Launcher, Taskleiste/Anheften, Live-Aktivitätsmeldungen und Sync-Anzeige gehören in v4 dem Rahmen
// (v4/frame) bzw. der Zustandszone – siehe V4-PORT.md „Rahmen übernimmt“.
import { getSvgIcon, registerSvgIcon } from 'ctox:shared/icons.js';
import { sendContextChat } from 'ctox:shared/context-chat.js';
import { applyWorkjetCategory, normalizeWorkjetCategory, workjetCategoryForModule } from 'ctox:shared/workjet-theme.js';
import { createCtoxLauncher } from './ctoxLauncher.js';
import { addMissingDesktopIcons, arrangeDesktopIcons, replaceDesktopIcons, runDesktopActionOnce } from './desktopMenuActions.js';
import { makeIconDraggable } from './iconDrag.js';
import { isRuntimeInstalledApp, moduleRenamePayload } from './appCommands.js';

const APP_ID = 'desktop';
const LAYOUT_DOC_ID = 'layout';
const DESKTOP_ICON_PERSISTED_FIELDS = new Set([
  'id', 'target_type', 'target_module', 'target_record_id', 'label', 'glyph', 'x', 'y', 'pinned', 'hidden', 'sort_index', 'updated_at_ms',
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
    openInModule: 'Öffnen',
    chatWithCtox: 'Mit CTOX chatten',
    askCtox: 'Frage stellen',
    workWithData: 'Daten ändern',
    modifyApp: 'App ändern',
    createApp: 'App Creator öffnen',
    renameApp: 'App umbenennen',
    chatContextLabel: 'Desktop-Kontext',
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
    restoreConfirm: 'Anordnung von vor dem letzten Zurücksetzen wiederherstellen?',
    noBackup: 'Keine vorherige Desktop-Anordnung gespeichert.',
    resetConfirm: 'Desktop-Icons auf Standard zurücksetzen? Eigene Verknüpfungen gehen verloren.',
    menuActionFailed: 'Desktop-Aktion fehlgeschlagen',
  },
  en: {
    moduleTitle: 'Desktop',
    openInModule: 'Open',
    chatWithCtox: 'Chat with CTOX',
    askCtox: 'Ask question',
    workWithData: 'Change data',
    modifyApp: 'Modify app',
    createApp: 'Open App Creator',
    renameApp: 'Rename app',
    chatContextLabel: 'Desktop context',
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
    restoreConfirm: 'Restore the arrangement from before the last reset?',
    noBackup: 'No previous desktop arrangement is saved.',
    resetConfirm: 'Reset desktop icons to default? Custom shortcuts will be removed.',
    menuActionFailed: 'Desktop action failed',
  },
};


export async function mount(root, api, { restored } = {}) {
  if (!root.dataset.app) root.dataset.app = APP_ID;
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  root.innerHTML = html;
  const host = root.querySelector('[data-desktop-root]');
  const refs = {
    root: host,
    surface: host.querySelector('[data-desktop-surface]'),
    icons: host.querySelector('[data-desktop-icons]'),
    time: host.querySelector('[data-widget-time]'),
    date: host.querySelector('[data-widget-date]'),
  };
  refs.surface.tabIndex = -1;
  applyWorkjetCategory(refs.root, workjetCategoryForModule({ id: APP_ID }));
  api.shell.setTitle('Desktop');

  const lang = () => (String(api.shell.locale.get() || 'de').startsWith('en') ? 'en' : 'de');
  const t = (key, fallback) => {
    const full = `${APP_ID}.${key}`;
    const v = api.shell.t(full);
    if (v && v !== full) return v;
    return FALLBACK_LABELS[lang()]?.[key] ?? FALLBACK_LABELS.de[key] ?? fallback ?? key;
  };
  const report = (code, text, level = 'error', ref) => api.status.report({ level, code: `${APP_ID}.${code}`, text, ...(ref ? { ref } : {}) });
  const clear = (code) => api.status.clear(`${APP_ID}.${code}`);
  const watch = (handle, code = 'write') => { handle?.local?.catch?.((error) => report(code, `${t('menuActionFailed')}: ${error?.code || error?.message || error}`)); return handle; };

  const snap = restored && typeof restored === 'object' ? restored : {};
  const cleanups = [];
  const pendingActions = new Set();
  let disposed = false;
  let modules = [];
  let launcher = createLauncher([]);
  let iconDocs = [];
  let iconsSeen = false;
  let layout = null;
  let layoutSeen = false;
  let seeded = false;
  let surfaceWait = null; // ResizeObserver, bis die Fläche ihre echte Größe hat
  let normalized = false;
  let selectedIconId = typeof snap.selectedIconId === 'string' ? snap.selectedIconId : '';
  // Sicherung vor „Zurücksetzen“: früher localStorage, jetzt Montagezustand + snapshot (OFFEN-3).
  let resetBackup = Array.isArray(snap.resetBackup) ? snap.resetBackup : null;
  let renderSignature = '';

  // Uhr/Datum: reine UI-Anzeige im Sekundentakt.
  const updateClock = () => {
    const locale = lang();
    try {
      setTextIfChanged(refs.time, new Date().toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit' }));
      setTextIfChanged(refs.date, new Date().toLocaleDateString(locale, { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' }));
    } catch { /* ungültige Locale: Anzeige bleibt */ }
  };
  let clockTimer = 0;
  const tick = () => { updateClock(); clockTimer = setTimeout(tick, 1000 - (Date.now() % 1000) + 5); };
  tick();
  cleanups.push(() => clearTimeout(clockTimer));

  cleanups.push(api.query('business_apps', {}).subscribe((rows) => {
    modules = rows.filter((row) => row?.id && row.id !== 'shell');
    // App icons are assets of each app (layout.icon_svg); shared/icons.js only supplies generic ones.
    for (const row of modules) if (row.layout?.icon_svg) registerSvgIcon(row.id, row.layout.icon_svg);
    launcher = createLauncher(modules);
    renderSignature = '';
    maybeSeed();
    renderIcons();
  }));
  cleanups.push(api.get('desktop_layout', LAYOUT_DOC_ID).subscribe((doc) => {
    layout = doc;
    if (!layoutSeen) {
      layoutSeen = true;
      if (!doc) watch(api.put('desktop_layout', { id: LAYOUT_DOC_ID, ...defaultLayout(), updated_at_ms: Date.now() }), 'write.layout');
    }
    renderSignature = '';
    renderIcons();
  }));
  cleanups.push(api.query('desktop_icons', { sort: [{ sort_index: 'asc' }] }).subscribe((rows) => {
    iconDocs = rows;
    iconsSeen = true;
    maybeSeed();
    renderIcons();
  }));
  cleanups.push(api.shell.locale.subscribe(() => { updateClock(); renderSignature = ''; renderIcons(); }));

  refs.surface.addEventListener('contextmenu', onSurfaceContextMenu);
  cleanups.push(() => refs.surface.removeEventListener('contextmenu', onSurfaceContextMenu));

  return {
    unmount() {
      disposed = true;
      surfaceWait?.disconnect(); surfaceWait = null;
      for (const dispose of cleanups.splice(0)) {
        try { dispose?.(); } catch (error) { console.error('[desktop] cleanup failed:', error); }
      }
      root.replaceChildren();
    },
    snapshot() {
      return { v: 1, selectedIconId, ...(resetBackup ? { resetBackup } : {}) };
    },
  };

  // ---------- helpers ----------

  function createLauncher(list) {
    return createCtoxLauncher({ modules: list, currentModuleId: APP_ID, navigate: (id, params) => api.shell.navigate(id, params) });
  }

  function categoryForLauncherEntry(entry) {
    return entry?.kind === 'module' ? workjetCategoryForModule(entry) : normalizeWorkjetCategory(entry?.category);
  }

  /** Standard-Icons für bekannte Apps anlegen/angleichen – einmal je Montage, sobald Katalog und Icons da sind. */
  function maybeSeed() {
    if (seeded || disposed || !iconsSeen || !modules.length) return;
    // Erst anlegen/angleichen, wenn die Fläche ihre echte Größe hat: während das Fenster aus der Launcher-Kachel
    // aufgeht, ist sie wenige Pixel groß – Raster (1 Spalte) und Klemmung legten dann alle Icons dauerhaft auf 12/12
    // (Rundgang RUND1 @1280). Warten per ResizeObserver statt Takt.
    const size = surfaceSize();
    if (refs.surface && (size.width < DEFAULT_GRID.cellW * 2 || size.height < DEFAULT_GRID.cellH * 2) && typeof ResizeObserver === 'function') {
      if (!surfaceWait) {
        surfaceWait = new ResizeObserver(() => {
          const now = surfaceSize();
          if (now.width >= DEFAULT_GRID.cellW * 2 && now.height >= DEFAULT_GRID.cellH * 2) { surfaceWait?.disconnect(); surfaceWait = null; maybeSeed(); }
        });
        surfaceWait.observe(refs.surface);
      }
      return;
    }
    seeded = true;
    const grid = currentGrid();
    const existingById = new Map(iconDocs.map((doc) => [doc.id, doc]));
    launcher.entries().forEach((entry, index) => {
      const seed = iconSeedForEntry(entry, index, grid);
      const existing = existingById.get(seed.id);
      if (!existing) { watch(api.put('desktop_icons', seed), 'write.seed'); return; }
      const patch = normalizeIconPatch(existing, seed, grid);
      if (Object.keys(patch).length) watch(api.patch('desktop_icons', seed.id, patch), 'write.seed');
    });
    normalizeIconLayoutIfNeeded();
  }

  function renderIcons() {
    if (disposed) return;
    const docs = iconDocs.length || iconsSeen ? iconDocs : [];
    const signature = JSON.stringify(docs.map((value) => [
      value.id, value.label, value.glyph, value.target_module, value.target_type, value.x, value.y, value.pinned, value.hidden, value.sort_index,
      launcher.knows(value.target_module),
    ]));
    if (signature === renderSignature && refs.icons.childElementCount) return;
    renderSignature = signature;
    refs.icons.replaceChildren();
    for (const doc of [...docs].sort((a, b) => (a.sort_index ?? 0) - (b.sort_index ?? 0))) {
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
    if (doc.id === selectedIconId) el.classList.add('selected');
    const position = clampIconPosition(doc, doc, currentGrid());
    el.style.left = `${position.x}px`;
    el.style.top = `${position.y}px`;
    el.innerHTML = '<div class="desktop-icon-glyph" aria-hidden="true"></div><div class="desktop-icon-label"></div>';
    const glyphEl = el.querySelector('.desktop-icon-glyph');
    const targetModule = doc.target_module || '';
    applyWorkjetCategory(el, categoryForLauncherEntry(launcher.entries().find((entry) => entry.id === targetModule)));
    const svgIcon = getSvgIcon(targetModule, 28);
    if (svgIcon) glyphEl.innerHTML = svgIcon;
    else glyphEl.textContent = doc.glyph || launcher.glyphFor(targetModule);
    const label = desktopIconLabel(doc);
    const labelEl = el.querySelector('.desktop-icon-label');
    labelEl.textContent = label;
    // FIX11 (G4): Symbolbeschriftung zwei Zeilen wie im Finder; ein unteilbarer Bezeichner wird gekuerzt, der volle
    // Name steht im Tooltip (el.title) und im aria-label.
    labelEl.setAttribute('data-v4-allow-ellipsis', '');
    el.title = label;
    el.setAttribute('role', 'button');
    el.setAttribute('aria-label', label);
    el.tabIndex = 0;
    el.addEventListener('keydown', (event) => {
      if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); openDesktopTarget(doc); }
    });
    el.addEventListener('contextmenu', (event) => onIconContextMenu(event, doc));
    makeIconDraggable(el, {
      surface: refs.surface,
      iconId: doc.id,
      grid: { ...currentGrid(), flow: document.documentElement.dataset.workjetMobileHost === 'true' || currentGrid().compact },
      onSelect: () => {
        for (const node of refs.icons.querySelectorAll('.desktop-icon.selected')) node.classList.remove('selected');
        el.classList.add('selected');
        selectedIconId = doc.id;
      },
      onOpen: () => openDesktopTarget(doc),
      onMoved: (iconId, pos) => { watch(api.patch('desktop_icons', iconId, { ...pos, updated_at_ms: Date.now() }), 'write.move'); },
      onDragToTopbar: () => {},
      onReorder: reorderIcons,
    });
    return el;
  }

  async function onSurfaceContextMenu(event) {
    if (event.target.closest('.desktop-icon')) return;
    event.preventDefault();
    for (const node of refs.icons.querySelectorAll('.desktop-icon.selected')) node.classList.remove('selected');
    selectedIconId = '';
    const items = [
      { id: 'creator', label: t('createApp'), icon: '+', disabled: !launcher.knows('creator') },
      { id: 'chat', label: t('chatWithCtox'), icon: '◆' },
      { type: 'separator' },
      { id: 'explorer', label: t('openExplorer'), icon: '⌘', disabled: !launcher.knows('explorer') },
      { id: 'notes', label: t('openNotes'), icon: '✎', disabled: !launcher.knows('notes') },
      { type: 'separator' },
      { id: 'arrange', label: t('arrangeIcons'), icon: '▦' },
      { id: 'sort', label: t('sortIcons'), icon: '⇅' },
      { id: 'add-missing', label: t('addMissingIcons'), icon: '+' },
      { type: 'separator' },
      { id: 'maintenance', label: t('maintenance'), icon: '⋯' },
    ];
    const id = menuId(await api.shell.menu({ x: event.clientX, y: event.clientY, items }));
    const actions = {
      creator: () => openLauncherTarget('creator'),
      explorer: () => openLauncherTarget('explorer'),
      notes: () => openLauncherTarget('notes'),
      chat: () => chatWithCtoxAboutDesktop(),
      arrange: () => arrangeIcons('current'),
      sort: () => arrangeIcons('name'),
      'add-missing': () => addMissingDefaultIcons(),
      maintenance: () => showDesktopMaintenance(event),
    };
    if (actions[id]) safeAction(actions[id], id);
  }

  async function onIconContextMenu(event, doc) {
    event.preventDefault();
    event.stopPropagation();
    // Android WebView: Long-Press gehört dem Umordnen (siehe iconDrag.js).
    if (document.documentElement.dataset.workjetMobileHost === 'true') return;
    if (event.currentTarget?.classList?.contains('touch-reordering')) return;
    const app = moduleForDesktopTarget(doc.target_module);
    const canRenameApp = isRuntimeInstalledApp(app);
    const items = [
      { id: 'open', label: t('openInModule'), icon: '↗' },
      { id: 'ask', label: t('askCtox'), icon: '?' },
      { id: 'data', label: t('workWithData'), icon: '◇' },
      { id: 'app', label: t('modifyApp'), icon: '✦' },
      ...(canRenameApp ? [{ id: 'rename-app', label: t('renameApp'), icon: '✎' }] : []),
      { id: 'rename-icon', label: t('renameIcon'), icon: '✎' },
      { type: 'separator' },
      { id: 'delete-icon', label: t('deleteIcon'), icon: '−' },
    ];
    const id = menuId(await api.shell.menu({ x: event.clientX, y: event.clientY, items }));
    const actions = {
      open: () => openDesktopTarget(doc),
      ask: () => chatWithCtoxAboutIcon(doc, 'ask'),
      data: () => chatWithCtoxAboutIcon(doc, 'data'),
      app: () => chatWithCtoxAboutIcon(doc, 'app'),
      'rename-app': () => renameApp(doc, app),
      'rename-icon': () => renameIcon(doc),
      'delete-icon': () => deleteIcon(doc.id),
    };
    if (actions[id]) safeAction(actions[id], `${id}:${doc.id}`);
  }

  async function showDesktopMaintenance(event) {
    const items = [
      { id: 'reset', label: t('iconRestoreDefaults'), icon: '⟳' },
      { id: 'restore', label: t('restoreArrangement'), icon: '↶', disabled: !resetBackup },
    ];
    const id = menuId(await api.shell.menu({ x: event.clientX, y: event.clientY, items }));
    if (id === 'reset') await restoreDefaultIcons();
    if (id === 'restore') await restorePreviousArrangement();
  }

  /** Öffnen über api.shell.navigate; unbekannte Ziele meldet die Shell selbst in die Zone (§10.2a Nr. 2). */
  function openDesktopTarget(doc) {
    const targetModule = doc?.target_module || '';
    return Boolean(launcher.open(targetModule) || api.shell.navigate(targetModule, {}));
  }

  function openLauncherTarget(targetId) {
    return openDesktopTarget({ target_module: targetId });
  }

  function safeAction(action, key = '') {
    runDesktopActionOnce(pendingActions, key || Symbol('desktop-action'), () => Promise.resolve().then(action))
      .then(() => clear('action'))
      .catch((error) => report('action', `${t('menuActionFailed')}: ${String(error?.message || error)}`));
  }

  async function renameApp(doc, app) {
    const current = String(app?.title || titleForModule(doc.target_module) || '').trim();
    const answer = await api.shell.dialog({ kind: 'prompt', title: t('renameApp'), value: current, default: null });
    const title = String(typeof answer === 'string' ? answer : answer?.value || '').trim();
    if (!title || title === current || disposed) return;
    const payload = moduleRenamePayload(app, title);
    const h = api.command('ctox.module.save', {
      ...payload,
      record_id: app.id,
      client_context: { source: 'desktop-app-context-menu', action: 'app.rename', module_id: app.id, app_id: app.id },
    });
    const code = 'command.ctox.module.save';
    h.local.catch((error) => report(code, `${t('menuActionFailed')}: ${error?.message || error}`, 'error', h.id));
    const off = h.status.subscribe((s) => {
      if (s?.status === 'failed') report(code, `${t('renameApp')}: ${s.error || 'failed'}`, 'error', h.id);
      if (s?.status === 'done') clear(code);
      if (s?.status === 'done' || s?.status === 'failed') queueMicrotask(() => off());
    });
    cleanups.push(off);
    // Icon-Beschriftung folgt lokal sofort (vorher erst nach `terminal`).
    watch(api.patch('desktop_icons', doc.id, { label: title, updated_at_ms: Date.now() }), 'write.label');
  }

  function moduleForDesktopTarget(moduleId) {
    return modules.find((module) => module?.id === moduleId) || null;
  }

  async function renameIcon(doc) {
    if (!doc?.id) return;
    const current = doc.label || titleForModule(doc.target_module);
    const answer = await api.shell.dialog({ kind: 'prompt', title: t('renameIcon'), value: current, default: null });
    const label = String(typeof answer === 'string' ? answer : answer?.value || '').trim();
    if (!label || label === current || disposed) return;
    watch(api.patch('desktop_icons', doc.id, { label, updated_at_ms: Date.now() }), 'write.label');
  }

  function deleteIcon(iconId) {
    watch(api.patch('desktop_icons', iconId, { hidden: true, updated_at_ms: Date.now() }), 'write.hide');
  }

  // Kontext-Chat über den Baustein ctox:shared/context-chat.js (Strang SKL-D, Kern §16.40 commands_out): Text im
  // App-Fenster (api.shell.dialog), dann Standardbefehl ctox.chat.task bzw. ctox.app.modify.request an die Chat-App
  // ctox und Sprung in den Chat. Ersetzt den Sprung mit Chat-Entwurf (OFFEN-4: die ctox-App las {chat} nie).
  async function chatWithCtoxAboutIcon(doc, mode = 'ask') {
    const label = doc.label || titleForModule(doc.target_module) || doc.id || 'Desktop object';
    const targetKind = doc.target_type || launcher.kindOf(doc.target_module) || 'module';
    const targetModule = doc.target_module || '';
    const recordId = doc.target_record_id || '';
    const agentMode = desktopAgentModeConfig(mode, label);
    const iconContext = { id: doc.id || '', label, target_type: targetKind, target_module: targetModule, target_record_id: recordId, glyph: doc.glyph || launcher.glyphFor(targetModule) };
    const text = await promptContextText(agentMode.title, agentMode.draft);
    if (!text || disposed) return null;
    return sendDesktopContextChat({
      // App ändern zielt auf die App hinter dem Symbol; Fragen/Daten tragen sie als Bezug.
      source_app: agentMode.mode === 'app' && targetModule ? targetModule : 'desktop',
      record_ref: recordId || doc.id || targetModule,
      text,
      mode: agentMode.mode,
      canModifyApp: agentMode.mode === 'app',
      title: agentMode.commandTitle,
      thread_key: targetModule ? `business-os/${targetModule}` : `business-os/desktop/${doc.id || 'surface'}`,
      context: { source: 'desktop-icon-context-menu', desktop_icon: iconContext },
    });
  }

  function desktopAgentModeConfig(mode, label) {
    if (mode === 'app') return { mode: 'app', title: `${t('modifyApp')} · ${label}`, draft: `Ändere die App "${label}": `, commandTitle: `${label} App ändern` };
    if (mode === 'data') return { mode: 'data', title: `${t('workWithData')} · ${label}`, draft: `Ändere Daten in "${label}": `, commandTitle: `${label} Daten ändern` };
    return { mode: 'ask', title: `${t('askCtox')} · ${label}`, draft: `Frage zu "${label}": `, commandTitle: `${label} Frage` };
  }

  async function chatWithCtoxAboutDesktop() {
    const iconCount = refs.icons?.querySelectorAll('.desktop-icon')?.length || 0;
    const text = await promptContextText('CTOX · Desktop', 'Bitte hilf mir mit dem Desktop. ');
    if (!text || disposed) return null;
    return sendDesktopContextChat({
      source_app: 'desktop',
      record_ref: 'desktop',
      text,
      mode: 'data',
      title: 'Desktop',
      context: { source: 'desktop-surface-context-menu', desktop_surface: { visible_icon_count: iconCount } },
    });
  }

  async function promptContextText(title, draft) {
    const answer = await api.shell.dialog({ kind: 'prompt', title, value: draft, default: null });
    const text = String(typeof answer === 'string' ? answer : answer?.value || '').trim();
    return text && text !== draft.trim() ? text : '';
  }

  function sendDesktopContextChat(spec) {
    try {
      const handle = sendContextChat(api, spec);
      if (handle) watch(handle, 'command.context_chat');
      return handle;
    } catch (error) {
      report('command.context_chat', error?.message || String(error), 'error');
      return null;
    }
  }

  async function restoreDefaultIcons() {
    const confirmed = await api.shell.dialog({ kind: 'confirm', title: t('iconRestoreDefaults'), text: t('resetConfirm'), confirm_label: t('iconRestoreDefaults'), default: false });
    if (confirmed !== true || disposed) return;
    resetBackup = iconDocs.map((doc) => Object.fromEntries(Object.entries(doc).filter(([key]) => DESKTOP_ICON_PERSISTED_FIELDS.has(key))));
    const grid = currentGrid();
    const defaults = launcher.entries().map((entry, index) => iconSeedForEntry(entry, index, grid));
    replaceDesktopIcons({ api, docs: iconDocs, snapshot: defaults }).forEach((h) => watch(h, 'write.reset'));
  }

  async function restorePreviousArrangement() {
    if (!resetBackup) { report('restore.no_backup', t('noBackup'), 'info'); return; }
    const confirmed = await api.shell.dialog({ kind: 'confirm', title: t('restoreArrangement'), text: t('restoreConfirm'), default: false });
    if (confirmed !== true || disposed) return;
    replaceDesktopIcons({ api, docs: iconDocs, snapshot: resetBackup.map((icon) => ({ ...icon, updated_at_ms: Date.now() })) }).forEach((h) => watch(h, 'write.reset'));
    resetBackup = null;
  }

  function addMissingDefaultIcons() {
    const { handles } = addMissingDesktopIcons({ api, docs: iconDocs, entries: launcher.entries(), gridPosition, glyphFor: launcher.glyphFor });
    handles.forEach((h) => watch(h, 'write.icons'));
  }

  function arrangeIcons(order = 'current') {
    arrangeDesktopIcons({ api, docs: iconDocs, knows: launcher.knows, labelFor: desktopIconLabel, gridPosition, order, locale: lang() })
      .forEach((h) => watch(h, 'write.icons'));
  }

  function reorderIcons(orderedIconIds) {
    if (!Array.isArray(orderedIconIds) || !orderedIconIds.length) return;
    const docsById = new Map(iconDocs.map((doc) => [doc.id, doc]));
    const visibleIds = orderedIconIds.filter((id) => {
      const doc = docsById.get(id);
      return doc && !doc.hidden && launcher.knows(doc.target_module);
    });
    const remainingIds = iconDocs
      .filter((doc) => !doc.hidden && launcher.knows(doc.target_module) && !visibleIds.includes(doc.id))
      .sort((a, b) => (a.sort_index ?? 0) - (b.sort_index ?? 0))
      .map((doc) => doc.id);
    const nextIds = [...visibleIds, ...remainingIds];
    if (nextIds.length < 2) return;
    const updatedAt = Date.now();
    nextIds.forEach((id, sortIndex) => watch(api.patch('desktop_icons', id, { sort_index: sortIndex, updated_at_ms: updatedAt + sortIndex }), 'write.icons'));
  }

  function titleForModule(moduleId) {
    const entry = launcher.entries().find((mod) => mod.id === moduleId);
    return entry?.title || moduleId || '';
  }

  function desktopIconLabel(doc) {
    const targetModule = String(doc?.target_module || '').trim();
    const persisted = String(doc?.label || '').trim();
    if (targetModule === 'ctox' && (!persisted || persisted === 'CTOX')) return 'CTOX Backend';
    return persisted || titleForModule(targetModule);
  }

  function defaultLayout() {
    return {
      wallpaper_url: '',
      wallpaper_mode: 'cover',
      taskbar_pins: ['ctox', 'documents', 'explorer', 'knowledge', 'research'].filter((id) => launcher.knows(id)),
      grid_cell_w: DEFAULT_GRID.cellW,
      grid_cell_h: DEFAULT_GRID.cellH,
      grid_offset: DEFAULT_GRID.offset,
    };
  }

  function currentGrid() {
    const surfaceWidth = surfaceSize().width || globalThis.innerWidth || 0;
    // Gleicher Kompakt-Umbruch wie index.css (Container ≤ 767 px).
    if (surfaceWidth > 0 && surfaceWidth <= 767) return { ...COMPACT_GRID, compact: true };
    return {
      cellW: Math.max(DEFAULT_GRID.cellW, layout?.grid_cell_w || DEFAULT_GRID.cellW),
      cellH: Math.max(DEFAULT_GRID.cellH, layout?.grid_cell_h || DEFAULT_GRID.cellH),
      offset: layout?.grid_offset || DEFAULT_GRID.offset,
      compact: false,
    };
  }

  // Layoutmaße der Fläche (clientWidth/-Height), NICHT getBoundingClientRect: während der Öffnen-Animation des
  // Fensters (Morph aus der Launcher-Kachel per transform) ist das Rechteck winzig. Damit geklemmt und gespeichert lagen
  // alle Icons auf 12/12 übereinander (Rundgang RUND1, Desktop über den Launcher geöffnet @1280).
  function surfaceSize() {
    const el = refs.surface;
    return { width: el?.clientWidth || 0, height: el?.clientHeight || 0 };
  }

  function gridPosition(index, grid = currentGrid()) {
    const surfaceRect = surfaceSize();
    const usableWidth = Math.max(grid.cellW, (surfaceRect?.width || 1024) - grid.offset * 2);
    const columns = Math.max(1, Math.floor(usableWidth / grid.cellW));
    return { x: grid.offset + (index % columns) * grid.cellW, y: grid.offset + Math.floor(index / columns) * grid.cellH };
  }

  function iconSeedForEntry(entry, index, grid) {
    const position = gridPosition(index, grid);
    return {
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
      sort_index: index,
      updated_at_ms: Date.now(),
    };
  }

  function normalizeIconPatch(doc, seed, grid) {
    const patch = {};
    const position = clampIconPosition(doc, seed, grid);
    if (position.x !== doc.x) patch.x = position.x;
    if (position.y !== doc.y) patch.y = position.y;
    if (!doc.target_type) patch.target_type = seed.target_type;
    if (!doc.target_module) patch.target_module = seed.target_module;
    if (!doc.label || (doc.id === 'desk_icon_research' && doc.target_module === 'research' && doc.label === 'Research')) patch.label = seed.label;
    if (!isSafePersistedGlyph(doc.glyph)) patch.glyph = seed.glyph;
    if (!Number.isFinite(doc.sort_index)) patch.sort_index = seed.sort_index;
    if (Object.keys(patch).length) patch.updated_at_ms = Date.now();
    return patch;
  }

  function clampIconPosition(doc, fallback, grid) {
    const surfaceRect = surfaceSize();
    const iconWidth = grid.compact ? ICON_METRICS.compactWidth : ICON_METRICS.width;
    const iconHeight = grid.compact ? ICON_METRICS.compactHeight : ICON_METRICS.height;
    const maxX = Math.max(grid.offset, (surfaceRect?.width || 1024) - iconWidth - 8);
    const x = Number.isFinite(doc.x) ? doc.x : fallback.x;
    const y = Number.isFinite(doc.y) ? doc.y : fallback.y;
    // Nur seitlich klemmen: die Icon-Fläche scrollt senkrecht (index.css .desktop-icons overflow-y: auto). Eine
    // Klemmung auf die Flächenhöhe stapelte alle Zeilen unter dem Falz auf die letzte Zeile (Rundgang RUND1).
    void iconHeight;
    return { x: Math.max(grid.offset, Math.min(x, maxX)), y: Math.max(grid.offset, y) };
  }

  /** Überlappende Icons (gleiche Rasterzelle) einmal je Montage zeilenweise neu verteilen. */
  function normalizeIconLayoutIfNeeded() {
    if (normalized) return;
    normalized = true;
    const docs = iconDocs.filter((doc) => !doc.hidden && launcher.knows(doc.target_module)).sort((a, b) => (a.sort_index ?? 0) - (b.sort_index ?? 0));
    const seen = new Set();
    let hasCollision = false;
    for (const doc of docs) {
      const key = `${Math.round(doc.x || 0)}:${Math.round(doc.y || 0)}`;
      if (seen.has(key)) { hasCollision = true; break; }
      seen.add(key);
    }
    if (!hasCollision) return;
    const grid = currentGrid();
    docs.forEach((doc, index) => watch(api.patch('desktop_icons', doc.id, { ...gridPosition(index, grid), sort_index: index, updated_at_ms: Date.now() }), 'write.icons'));
  }
}

function menuId(choice) {
  return typeof choice === 'string' ? choice : (choice?.id || '');
}

function isSafePersistedGlyph(value) {
  const glyph = String(value || '').trim();
  if (!glyph || glyph.length > 16) return false;
  return !/^(?:data:|https?:)/i.test(glyph) && !/[<>{}]/.test(glyph);
}

// textContent nur bei echter Änderung schreiben (Uhr tickt sekündlich; sonst Layout-Pass je Tick).
function setTextIfChanged(element, value) {
  const next = String(value ?? '');
  if (!element || element.textContent === next) return;
  element.textContent = next;
}
