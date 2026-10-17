// app-store – App-API 4.0 (Shell v4, Strang UD6). Port-Protokoll: V4-PORT.md.
// Katalog = Plattform-Collections business_apps/business_app_versions/app_catalog/app_drafts (ctox-platform, fremd
// gelesen, §16.29); Installieren/Aktualisieren/Zurückrollen/Freigeben/Veröffentlichen/Entfernen = Plattform-Befehle
// ctox.app.* des Kerns mit Vergleich-und-Setzen auf base_app_ver (= vom Client gesehene aktive Version).
import {
  appLifecycleBadge,
  appReleaseProjection,
  businessDataAreaLabel,
  canSeeModuleForAppVersion as lifecycleCanSeeModuleForAppVersion,
} from 'ctox:shared/app-lifecycle.js';
import { BusinessOsPermissions } from 'ctox:shared/business-permissions.js';
import { roleCanManage } from 'ctox:shared/roles.js';
import { buildGlobalCtoxAgentScopeView } from 'ctox:shared/shell-permissions-ui.js';
import { autoWirePaneGrammar } from 'ctox:shared/pane-grammar.js';
import LOCALE_DE from './locales/de.json' with { type: 'json' };
import LOCALE_EN from './locales/en.json' with { type: 'json' };

const LOCALES = Object.freeze({ de: LOCALE_DE, en: LOCALE_EN });
const REDUCED_MOTION_QUERY = '(prefers-reduced-motion: reduce)';

function prefersReducedMotion() {
  return globalThis.matchMedia?.(REDUCED_MOTION_QUERY)?.matches === true;
}

function shouldUseShelf(cardsMode, shelfUnavailable, reducedMotion) {
  return cardsMode && !shelfUnavailable && !reducedMotion;
}

const state = {
  root: null,
  api: null,
  lang: 'de',
  actor: { id: '', role: '', permissions: [] },
  platformApps: [],
  platformVersions: [],
  platformCatalog: [],
  platformDrafts: [],
  zone: new Set(),
  renderTimer: 0,
  t: (key, fallback) => fallback ?? key,
  catalog: null,
  marketplace: [],
  marketplaceStatus: 'idle',
  marketplaceMessage: '',
  selectedId: '',
  scope: 'marketplace',
  query: '',
  busy: false,
  status: null,
  operations: {},
  // Two renderings, one toggle: 'cards' is the shard view (the WebGL retail
  // shelf, or DOM shard cards when the shelf is unavailable or motion is
  // reduced), 'list' is the dense one-row-per-app list. The shelf is a
  // continuously animated WebGL surface, so reduced motion starts on the list;
  // switching to cards under that preference still uses the DOM card grid.
  viewMode: prefersReducedMotion()
    ? 'list'
    : 'cards',
  drawerOpen: false,
  // Canonical grammar state + the retail-box shelf (vendor/store-shelf).
  centerBand: 'catalog',
  categoryFilter: 'all',
  sortKey: 'title',
  shelf: null,
  shelfUnavailable: false,
  shelfSignature: '',
};

// Apps with a real product capture shipped in assets/previews (same captures
// as the public site). Box fronts and the detail panel use them; every other
// app renders the motif/monogram box art.
const PREVIEW_IDS = new Set(['app-store', 'browser', 'buchhaltung', 'calendar', 'coding-agents',
  'conversations', 'creator', 'ctox', 'customers', 'documents', 'files', 'knowledge', 'matching',
  'notes', 'outbound', 'reports', 'research', 'shiftflow', 'spreadsheets', 'tickets']);
// Artefakt-Dateien der App, relativ zu index.js (der SW liefert sie versioniert, §16.15); kein ?v=, kein HTTP-Abruf.
function previewUrlFor(id) {
  return PREVIEW_IDS.has(id) ? new URL(`./assets/previews/${id}.png`, import.meta.url).href : '';
}

const els = {};

// Fensterzustand für Live-Remount (PORTING.md §2).
const SNAPSHOT_KEYS = Object.freeze(['selectedId', 'scope', 'query', 'viewMode', 'drawerOpen', 'centerBand', 'categoryFilter', 'sortKey']);

export async function mount(root, api, { restored } = {}) {
  const owner = { active: true };
  if (state.mountOwner) state.mountOwner.active = false;
  retireShelf();
  state.mountOwner = owner;
  state.root = root;
  state.api = api;
  state.zone = new Set();
  // Befehlsstatus-Abos einer abgelösten Montage enden mit ihr (R5: Abo bei unmount lösen).
  for (const off of state.commandOffs || []) {
    try { off(); } catch {}
  }
  state.commandOffs = new Set();
  state.operations = {};
  state.platformApps = [];
  state.platformVersions = [];
  state.platformCatalog = [];
  state.platformDrafts = [];
  state.catalog = { modules: [], templates: [], marketplace: [], version_states: {} };
  state.marketplace = [];
  state.actor = { id: api.actor?.id || '', role: api.actor?.role || '', permissions: [] };
  // Abos je Montage lokal (PORTING §4 Falle 1).
  const cleanup = [];
  const isCurrent = () => owner.active && state.mountOwner === owner;
  if (!root.dataset.app) root.dataset.app = 'app-store';
  applyLocale(readLocale(api));
  applyRestored(restored);
  root.innerHTML = await loadModuleMarkup();
  if (!isCurrent()) return { unmount() { root.replaceChildren(); }, snapshot() { return { v: 1 }; } };
  applyTranslations(root, state.t);
  bindElements(root);
  applyHeaderActionIcons();
  wireEvents();
  autoWirePaneGrammar(root);
  api.shell.setTitle(state.t('title', 'App Store'));

  const offLocale = api.shell.locale?.subscribe?.((value) => {
    if (!isCurrent()) return;
    const lang = String(value || '').startsWith('en') ? 'en' : 'de';
    if (lang === state.lang) return;
    applyLocale(lang);
    applyTranslations(root, state.t);
    api.shell.setTitle(state.t('title', 'App Store'));
    render();
  });
  if (typeof offLocale === 'function') cleanup.push(offLocale);
  const offPermissions = api.actor?.permissions?.subscribe?.((list) => {
    if (!isCurrent()) return;
    state.actor = { ...state.actor, permissions: Array.isArray(list) ? [...list] : [] };
    scheduleRender();
  });
  if (typeof offPermissions === 'function') cleanup.push(offPermissions);

  // Katalog = Plattform-Collections von ctox-platform (§9.2/§16.14/§16.17), live und lokal.
  let wiring = true;
  const live = (collection, key) => {
    try {
      cleanup.push(api.query(collection, { sort: [{ id: 'asc' }] }).subscribe((rows) => {
        if (!isCurrent()) return;
        state[key] = rows;
        if (!wiring) scheduleRender();
      }));
    } catch (error) {
      reportStatus('error', `app-store.data.${collection}`, error?.message || String(error));
    }
  };
  live('business_apps', 'platformApps');
  live('business_app_versions', 'platformVersions');
  // §16.29: Release-Katalog (main = CTOX-Release, instance = auf dieser Instanz veröffentlicht) und eigene Entwürfe.
  live('app_catalog', 'platformCatalog');
  live('app_drafts', 'platformDrafts');
  wiring = false;
  rebuildCatalog();
  render();
  restoreGrammarControls(root);
  if (Number(restored?.wellScrollTop) > 0 && els.well) els.well.scrollTop = Number(restored.wellScrollTop);

  return {
    unmount() {
      for (const off of cleanup.splice(0)) {
        try { off?.(); } catch {}
      }
      if (state.mountOwner === owner) {
        owner.active = false;
        retireShelf();
        clearTimeout(state.renderTimer);
        state.renderTimer = 0;
        for (const off of state.commandOffs || []) {
          try { off(); } catch {}
        }
        state.commandOffs = new Set();
        for (const code of state.zone) {
          try { api.status.clear(code); } catch {}
        }
        state.zone.clear();
        state.api = null;
        state.root = null;
      }
      owner.active = false;
      root.replaceChildren();
    },
    snapshot() {
      if (state.mountOwner !== owner) return { v: 1 };
      const snap = { v: 1 };
      for (const key of SNAPSHOT_KEYS) snap[key] = state[key];
      snap.wellScrollTop = els.well?.scrollTop || 0;
      return snap;
    },
  };
}

// Nach dem Remount stehen Suche, Band und Filter wieder sichtbar in den Bedienelementen, nicht nur im Zustand
// (sonst filtert die Liste nach einem unsichtbaren Suchbegriff).
function restoreGrammarControls(root) {
  const pane = root.querySelector('.store-center');
  if (!pane) return;
  const search = pane.querySelector('[data-pg-search]');
  if (search && state.query) search.value = state.query;
  for (const tab of pane.querySelectorAll('[data-pg-band]')) {
    const active = tab.dataset.pgBand === state.centerBand;
    tab.setAttribute('aria-selected', String(active));
    tab.classList.toggle('is-active', active);
  }
  for (const select of pane.querySelectorAll('[data-pg-filter]')) {
    const value = select.dataset.pgName === 'category' ? state.categoryFilter : select.dataset.pgName === 'sort' ? state.sortKey : '';
    if (value && [...select.options].some((option) => option.value === value)) select.value = value;
  }
  pane.__ctoxPaneGrammar?.refreshDot?.();
}

function applyRestored(restored) {
  if (!restored || typeof restored !== 'object' || restored.v !== 1) return;
  for (const key of SNAPSHOT_KEYS) {
    if (key in restored && typeof restored[key] === typeof state[key]) state[key] = restored[key];
  }
}

function readLocale(api) {
  try {
    return String(api?.shell?.locale?.get?.() || '').startsWith('en') ? 'en' : 'de';
  } catch {
    return 'de';
  }
}

// Sprache (R8): api.shell.t('app-store.<key>') übersteuert, sonst die eigenen locales (Artefakt, kein HTTP-Abruf),
// sonst der Rückfalltext im Markup bzw. Aufruf (OFFEN-B).
function applyLocale(lang) {
  state.lang = lang === 'en' ? 'en' : 'de';
  const messages = LOCALES[state.lang] || LOCALES.de;
  const api = state.api;
  state.t = (key, fallback) => {
    const shellKey = `app-store.${key}`;
    let fromShell = '';
    try { fromShell = api?.shell?.t ? api.shell.t(shellKey) : ''; } catch {}
    if (typeof fromShell === 'string' && fromShell && fromShell !== shellKey) return fromShell;
    return messages[key] ?? fallback ?? key;
  };
}

// Zustand nur in der Zustandszone (R11); stabile Codes, damit clear greift.
function reportStatus(level, code, text, ref, action) {
  const api = state.api;
  if (!api?.status) return;
  const item = { level, code, text: String(text || code) };
  if (ref) item.ref = ref;
  if (action && typeof action.run === 'function') item.action = action;
  api.status.report(item);
  state.zone.add(code);
}

function clearStatus(code) {
  const api = state.api;
  if (!api?.status || !state.zone.has(code)) return;
  api.status.clear(code);
  state.zone.delete(code);
}

function scheduleRender() {
  if (state.renderTimer) return;
  state.renderTimer = setTimeout(() => {
    state.renderTimer = 0;
    if (!state.api) return;
    rebuildCatalog();
    render();
  }, 16);
}

function retireShelf() {
  try { state.shelf?.destroy?.(); } catch {}
  state.shelf = null;
  state.shelfPromise = null;
  state.shelfSignature = '';
  state.shelfUnavailable = false;
  clearTimeout(state.shelfRetryTimer);
  state.shelfRetryTimer = null;
  state.shelfRetryPending = false;
}

async function loadModuleMarkup() {
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}

function bindElements(root) {
  els.root = root;
  els.leftPane = root.querySelector('.store-left');
  els.centerPane = root.querySelector('.store-center');
  els.well = root.querySelector('.store-well');
  els.scopes = root.querySelector('[data-scope-list]');
  els.title = root.querySelector('[data-visible-category-title]');
  els.grid = root.querySelector('[data-apps-grid]');
  els.detail = root.querySelector('[data-detail-drawer]');
  els.detailIcon = root.querySelector('[data-detail-icon]');
  els.detailTitle = root.querySelector('[data-detail-title]');
  els.detailVersion = root.querySelector('[data-detail-version]');
  els.detailCategory = root.querySelector('[data-detail-category]');
  els.detailDeveloper = root.querySelector('[data-detail-developer]');
  els.detailLicense = root.querySelector('[data-detail-license]');
  els.detailSource = root.querySelector('[data-detail-source]');
  els.detailStatus = root.querySelector('[data-detail-status]');
  els.readme = root.querySelector('[data-readme-content]');
  els.closeDrawer = root.querySelector('[data-close-drawer]');
  els.viewToggle = root.querySelector('[data-store-view-toggle]');
  els.viewGlyphs = [...root.querySelectorAll('[data-view-glyph]')];
  els.exportCatalog = root.querySelector('[data-export-catalog]');
  els.categoryFilter = root.querySelector('[data-pg-filter][data-pg-name="category"]');
  els.shelfStage = root.querySelector('[data-shelf-stage]');
  els.shelfCanvas = root.querySelector('[data-shelf-canvas]');
  els.shelfScroll = root.querySelector('[data-shelf-scroll]');
  els.shelfTrack = root.querySelector('[data-shelf-track]');
  els.shelfHint = root.querySelector('[data-shelf-hint]');
  els.detailActions = root.querySelector('[data-detail-actions]');
  els.detailCapture = root.querySelector('[data-detail-capture]');
  els.detailCaptureImg = root.querySelector('[data-detail-capture-img]');
}

// Header-/Modal-Icons sind app-lokal (App-API 4.0 hat keine Icon-API, OFFEN-D).
function actionIcon(name) {
  const paths = {
    close: 'M6 6l12 12M18 6 6 18',
    export: 'M12 3v11M12 3 8 7M12 3l4 4M5 12v7h14v-7',
  };
  const d = paths[name];
  if (!d) return '';
  return `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="${d}"></path></svg>`;
}

function applyHeaderActionIcons() {
  if (!els.exportCatalog) return;
  els.exportCatalog.innerHTML = actionIcon('export');
}

function wireEvents() {
  els.scopes?.addEventListener('click', (event) => {
    const button = event.target.closest('[data-scope]');
    if (!button) return;
    setScope(button.dataset.scope || 'marketplace');
  });

  els.grid?.addEventListener('click', (event) => {
    const actionBtn = event.target.closest('[data-card-action]');
    const card = event.target.closest('[data-app-id]');
    if (!card) return;

    const appId = card.dataset.appId || '';
    state.selectedId = appId;
    // In-place selection flip — a selection click never rebuilds the grid
    // (scroll + focus stay put); only the detail drawer re-renders.
    applyAppStoreSelection();

    if (actionBtn) {
      const actionType = actionBtn.dataset.cardAction;
      triggerCardAction(appId, actionType);
      return;
    }

    state.drawerOpen = true;
    renderDetails();
  });
  els.grid?.addEventListener('keydown', (event) => {
    if (!['Enter', ' '].includes(event.key)) return;
    const card = event.target.closest('[data-app-id]');
    if (!card) return;
    event.preventDefault();
    state.selectedId = card.dataset.appId || '';
    applyAppStoreSelection();
    state.drawerOpen = true;
    renderDetails();
  });

  els.closeDrawer?.addEventListener('click', () => {
    state.drawerOpen = false;
    renderDetails();
  });
  els.root.addEventListener('keydown', (event) => {
    if (event.key === 'Escape' && state.drawerOpen) {
      state.drawerOpen = false;
      renderDetails();
    }
  });
  els.detail?.addEventListener('click', (event) => {
    const actionBtn = event.target.closest('[data-card-action]');
    if (!actionBtn || !state.selectedId) return;
    triggerCardAction(state.selectedId, actionBtn.dataset.cardAction);
  });

  // Pane chrome is SHELL-owned canonical grammar (autoWirePaneGrammar wires
  // the data-pg-* markup once, debounced ~120ms after mount): search input,
  // shelf/list toggle, collapsed tray with reset + active-dot, counted band.
  // The module only keeps its state in sync through the bubbling grammar
  // event and re-renders — the same contract knowledge/threads use.
  els.centerPane?.addEventListener('ctox-pane-grammar-change', onCenterGrammarChange);

  // ONE view control (Betreiber-Direktive 31.08.2026): a single action button
  // that flips between the shard/shelf rendering and the dense list. The shell
  // grammar only wires the two-button [data-pg-view] radio pair, so this
  // toggle is module-owned and state.viewMode is module state.
  els.viewToggle?.addEventListener('click', () => {
    state.viewMode = state.viewMode === 'list' ? 'cards' : 'list';
    render({ resetScroll: true });
  });

  els.exportCatalog?.addEventListener('click', exportVisibleCatalog);


  els.root.querySelector('[data-action="create-scratch"]')?.addEventListener('click', () => {
    openCreatorFromStore({ mode: 'scratch' });
  });

  els.root.querySelector('[data-action="install-github"]')?.addEventListener('click', () => {
    installFromGithub();
  });

  els.root.querySelector('[data-action="install-zip"]')?.addEventListener('click', () => {
    installFromZip();
  });
}

// Grammar state application (center pane: search, shelf/list view, counted
// band, category/sort tray filters). Intentional reset: grammar changes move
// the content set, so the well scrolls back to the top (the shell scroll
// guard also clears its recorded offsets on this event).
function onCenterGrammarChange(event) {
  const detail = event?.detail || {};
  state.query = String(detail.search ?? '').trim().toLowerCase();
  // The view is NOT read back from the grammar event: the single toggle above
  // owns state.viewMode, and the grammar reports its own 'cards' fallback for
  // panes without a [data-pg-view] pair.
  if (detail.band) state.centerBand = detail.band;
  state.categoryFilter = String(detail.filters?.category ?? 'all') || 'all';
  state.sortKey = String(detail.filters?.sort ?? 'title') || 'title';
  render({ resetScroll: true });
}

// Selection is an in-place flip over the existing cards — never a grid
// rebuild for a selection click (an innerHTML/replaceChildren rebuild would
// clamp the well's scrollTop to 0).
function applyAppStoreSelection() {
  if (!els.grid) return;
  for (const card of els.grid.querySelectorAll('[data-app-id]')) {
    const selected = card.dataset.appId === state.selectedId;
    card.classList.toggle('active', selected);
    card.classList.toggle('is-selected', selected);
    card.setAttribute('aria-selected', selected ? 'true' : 'false');
  }
}

async function triggerCardAction(appId, actionType) {
  const item = currentCatalogItem(appId);
  if (!item || (state.busy && !['details', 'repository', 'versions'].includes(actionType))) return;

  if (actionType === 'install') {
    await installMarketplaceItem(item);
  } else if (actionType === 'update') {
    if (!canInstallAppStoreItem(state, item)) return;
    // Fachverhalten wie v3: ein lokal angepasstes App-Update überschreibt die Anpassung nur nach ausdrücklicher
    // Bestätigung (mode 'discard'). Automatisch (§9.6) wird es nie angewendet; der Hinweis steht in der Zone.
    const customized = item.modification_status === 'modified' || item.modification_status === 'customized';
    if (customized
      && !await askConfirm(
        `${item.title} aktualisieren?`,
        // v4-allow: text – Bestätigungsdialog (api.shell.dialog), kein Inhaltstext
        `${item.title} hat lokale Änderungen. Ein Update überschreibt sie. Vor dem Update wird automatisch eine Wiederherstellungs-Version angelegt – fortfahren?`,
        state.t('actionUpdate', 'Aktualisieren'),
      )) {
      return;
    }
    await updateInstalledItem(item, { mode: customized ? 'discard' : 'vanilla' });
  } else if (actionType === 'publish') {
    if (!canEditAppStoreItem(state, item)) return;
    publishDraft(item);
  } else if (actionType === 'check-updates') {
    await checkModuleUpdates(item);
  } else if (actionType === 'versions') {
    await openVersionsDialog(item);
  } else if (actionType === 'open') {
    if (item.id === 'create-scratch') {
      openCreatorFromStore({ mode: 'scratch' });
    } else if (item.kind === 'template') {
      openCreatorFromStore({ mode: 'template', template: item.id });
    } else if (item.launch_kind === 'desktop-app') {
      openModule(item.id);
    } else {
      openModule(item.id);
    }
  } else if (actionType === 'edit') {
    if (!canEditAppStoreItem(state, item)) return;
    openCreatorFromStore({ mode: 'upgrade', upgrade: item.id });
  } else if (actionType === 'release') {
    if (!canReleaseAppStoreItem(state, item)) return;
    await openReleaseDialog(item);
  } else if (actionType === 'uninstall') {
    if (!canUninstallAppStoreItem(state, item)) return;
    await uninstallInstalledItem(item);
  } else if (actionType === 'repository') {
    if (item.homepage) {
      window.open(item.homepage, '_blank', 'noopener,noreferrer');
    }
  } else if (actionType === 'details') {
    state.drawerOpen = true;
    renderDetails();
  }
}

// v4-Katalog aus den Plattform-Collections (§9.1/§9.2/§9.6):
//   business_apps[id]               = {id, title, active_ver, migrated_at_seq, …}
//   business_app_versions[id@ver]   = {id, app_id, app_ver, manifest{…, provenance{base_release, base_hash, locally_modified}}, migrated_at_seq}
//   app_catalog[*]                  = {app_id, app_ver, title, layout, source: 'main'|'instance', published_at} (§16.29)
// Jede App der Instanz ist im Instanz-Store installiert (§9.6). „Update verfügbar“ = app_catalog führt für die App eine
// höhere Version mit source 'main' als die aktive. Ist die App lokal angepasst (locally_modified), bleibt das Update
// sichtbar markiert („Update … · angepasst“), steht aber nicht im Band „Updates“ und geht nur über den Verwerfen-Dialog.
const NON_APP_IDS = new Set(['ctox-platform', 'shell']);

function rebuildCatalog() {
  const catalogByApp = catalogRowsByApp();
  const versionsByApp = new Map();
  for (const version of state.platformVersions || []) {
    const appId = String(version?.app_id || String(version?.id || '').split('@')[0] || '');
    if (!appId) continue;
    if (!versionsByApp.has(appId)) versionsByApp.set(appId, []);
    versionsByApp.get(appId).push(version);
  }
  const modules = [];
  const versionStates = {};
  for (const app of state.platformApps || []) {
    if (!app?.id) continue;
    // Plattform- und Shell-Zeilen (ctox-platform veröffentlicht der Kern selbst, E48; shell ist der Rahmen) sind keine Apps.
    if (NON_APP_IDS.has(app.id) || String(app.id).startsWith('lib:')) continue;
    const versions = (versionsByApp.get(app.id) || []).slice()
      .sort((left, right) => Number(right.migrated_at_seq || 0) - Number(left.migrated_at_seq || 0));
    const active = versions.find((version) => version.app_ver === app.active_ver) || null;
    const manifest = active?.manifest && typeof active.manifest === 'object' ? active.manifest : {};
    const provenance = manifest.provenance && typeof manifest.provenance === 'object' ? manifest.provenance : {};
    const release = newerMainRelease(catalogByApp.get(app.id), app.active_ver);
    const layout = manifest.layout && typeof manifest.layout === 'object' ? manifest.layout : (app.layout || {});
    const installedVersion = String(manifest.version || app.active_ver || '');
    modules.push({
      id: app.id,
      title: app.title || manifest.title || app.id,
      description: manifest.description || app.description || '',
      category: layout.category || manifest.category || app.category || '',
      version: installedVersion,
      layout,
      source: 'installed',
      install_scope: 'installed',
      installable: true,
      editable: true,
      deletable: true,
      collections: Object.keys(manifest.collections || {}),
      active_ver: app.active_ver ?? null,
      provenance,
      locally_modified: provenance.locally_modified === true,
      catalog_release: release,
      lifecycle: release ? {
        update_available: true,
        catalog_version: String(release.app_ver),
        installed_version: installedVersion,
      } : {},
    });
    versionStates[app.id] = {
      current_bundle_sha256: active?.id || '',
      baseline_bundle_sha256: provenance.base_hash || '',
      modified: provenance.locally_modified === true,
      version_count: versions.length,
      versions: versions.map((version) => ({
        version_id: version.id,
        app_ver: version.app_ver,
        seq: Number(version.migrated_at_seq || 0),
        origin: version.manifest?.provenance?.base_release ? 'install' : 'edit',
        label: String(version.manifest?.version || version.app_ver || version.id),
        created_at_ms: Date.parse(version.manifest?.created_at || '') || 0,
        sealed: true,
        file_count: Object.keys(version.manifest?.files || {}).length,
        active: version.app_ver === app.active_ver,
      })),
    };
  }
  const entries = latestCatalogEntries(catalogByApp);
  state.catalog = {
    modules,
    templates: entries.filter(isTemplateEntry).map(normalizeCatalogEntry),
    marketplace: entries.filter((row) => !isTemplateEntry(row)).map(normalizeCatalogEntry),
    version_states: versionStates,
  };
  state.marketplace = state.catalog.marketplace;
  state.marketplaceStatus = 'ready';
  state.marketplaceMessage = '';
  syncLocallyModifiedZone(modules);
}

// app_catalog nach App gruppiert, je App absteigend nach Version (gleiche Version: main vor instance, dann neuer).
function catalogRowsByApp() {
  const byApp = new Map();
  for (const row of state.platformCatalog || []) {
    if (!row || row._deleted) continue;
    const appId = String(row.app_id || String(row.id || '').split('@')[0] || '');
    const appVer = String(row.app_ver ?? '');
    if (!appId || !appVer) continue;
    if (!byApp.has(appId)) byApp.set(appId, []);
    byApp.get(appId).push({ ...row, app_id: appId, app_ver: appVer });
  }
  for (const rows of byApp.values()) {
    rows.sort((left, right) => compareVersions(right.app_ver, left.app_ver)
      || (left.source === 'main' ? 0 : 1) - (right.source === 'main' ? 0 : 1)
      || String(right.published_at || '').localeCompare(String(left.published_at || '')));
  }
  return byApp;
}

// Höchste main-Version über der aktiven, sonst null (instance-Veröffentlichungen sind kein Update).
function newerMainRelease(rows, activeVer) {
  const main = (rows || []).find((row) => row.source === 'main');
  if (!main) return null;
  if (activeVer != null && activeVer !== '' && compareVersions(main.app_ver, activeVer) <= 0) return null;
  return { app_ver: main.app_ver, source: 'main', published_at: main.published_at || '' };
}

function latestCatalogEntries(byApp) {
  return [...byApp.values()].map((rows) => rows[0]).filter(Boolean);
}

// Vorlage = Katalogeintrag mit layout.template === true oder Kategorie „Template(s)“/„Vorlage(n)“ (V4-PORT.md OFFEN-2).
function isTemplateEntry(row) {
  const layout = row?.layout && typeof row.layout === 'object' ? row.layout : {};
  return layout.template === true || /^(templates?|vorlagen?)$/i.test(String(layout.category || '').trim());
}

function normalizeCatalogEntry(row) {
  const layout = row.layout && typeof row.layout === 'object' ? row.layout : {};
  return {
    id: row.app_id,
    module_id: row.app_id,
    title: row.title || row.app_id,
    description: String(layout.description || ''),
    category: layout.category || '',
    version: row.app_ver,
    developer: 'CTOX',
    source: row.source === 'instance' ? 'instance' : 'main',
    layout,
    icon_svg: layout.icon_svg || '',
    catalog: { app_ver: row.app_ver, source: row.source, published_at: row.published_at || '' },
    installable: true,
  };
}

// §9.6 / Anf. 70/71: ein Release-Update gilt nur für unveränderte Apps. Ist eine App lokal angepasst und ein Update da,
// sieht der Admin das in der Zustandszone – nicht als Text in der Liste.
function syncLocallyModifiedZone(modules) {
  const wanted = new Set();
  if (canSeeLocallyModifiedHint()) {
    for (const module of modules) {
      if (module.lifecycle?.update_available === true && module.provenance?.locally_modified === true) {
        const code = `app-store.update.locally_modified.${module.id}`;
        wanted.add(code);
        if (!state.zone.has(code) && state.api) {
          state.api.status.report({ level: 'info', code, text: `${module.title}: ${state.t('status.updateLocallyModified', 'Update verfügbar, App lokal angepasst')}`, ref: module.id });
          state.zone.add(code);
        }
      }
    }
  }
  for (const code of [...state.zone]) {
    if (code.startsWith('app-store.update.locally_modified.') && !wanted.has(code)) clearStatus(code);
  }
}

function canSeeLocallyModifiedHint() {
  const role = String(state.actor?.role || '').toLowerCase();
  return role === 'admin' || role === 'owner' || role === 'chef' || actorHasPermission(state, 'apps.source.edit');
}

function catalogItems() {
  return uniqueCatalogItems(rawCatalogItems()).sort(sortItems);
}

function rawCatalogItems() {
  const modules = Array.isArray(state.catalog?.modules) ? state.catalog.modules : [];
  const templates = Array.isArray(state.catalog?.templates) ? state.catalog.templates : [];

  const scratchTemplate = {
    id: 'create-scratch',
    module_id: 'create-scratch',
    title: 'Neue App per KI-Prompt erstellen',
    description: 'Freie App aus einem Prompt erstellen.',
    category: 'Templates',
    version: 'v1',
    developer: 'KI Generator',
    license: 'AGPL-3.0-only',
    source: 'creator',
    default_title: 'App von Scratch erstellen',
    collections: [],
    installable: true,
  };

  const items = [
    normalizeItem(scratchTemplate, 'template'),
    ...state.marketplace
      .map((item) => normalizeItem(item, 'marketplace')),
    ...templates.map((item) => normalizeItem(item, 'template')),
    ...modules
      .filter(isLaunchableModule)
      .filter(canSeeModuleForAppVersion)
      .map((item) => normalizeItem(item, moduleKind(item))),
  ];
  return items.filter(Boolean);
}

function isLaunchableModule(item) {
  const scope = String(item?.install_scope || '').trim().toLowerCase();
  return item?.id
    && item.id !== 'desktop'
    && item.id !== 'notizen'
    && !['internal', 'store', 'starter'].includes(scope);
}

function canSeeModuleForAppVersion(item) {
  return canSeeAppStoreModuleForAppVersion(state, item);
}

function canSeeAppStoreModuleForAppVersion(permissionState, item) {
  return lifecycleCanSeeModuleForAppVersion(item, appStorePermissionOptions(permissionState));
}

function moduleKind(item) {
  if (item?.core) return 'system';
  if (item?.install_scope === 'internal') return 'system';
  if (item?.source === 'installed') return 'installed';
  return 'local';
}

const CATEGORY_LABELS = Object.freeze({
  workspace: 'Arbeitsplatz', collaboration: 'Zusammenarbeit', productivity: 'Produktivität',
  entertainment: 'Unterhaltung', development: 'Entwicklung', engineering: 'Engineering',
  knowledge: 'Wissen', research: 'Recherche', sales: 'Vertrieb', recruiting: 'Recruiting',
  finance: 'Finanzen', operations: 'Betrieb', governance: 'Governance', security: 'Sicherheit',
  analytics: 'Analytik', system: 'System', imported: 'Weitere Apps',
});

function displayCategory(value, fallback) {
  const category = String(value || fallback || '').trim();
  return CATEGORY_LABELS[category.toLowerCase()] || category;
}

function normalizeItem(item, kind) {
  const id = sanitizeId(item.module_id || item.id || item.source_module || item.default_title || '');
  const remote = marketplaceItemFor(id);
  const release = latestReleaseFor(id);
  const status = statusForItem(item, kind);
  const installedVersion = installedVersionLabel(item, release, kind);
  const availableVersion = availableVersionLabel(remote, item, kind);
  const installable = item.installable !== false && item.store?.installable !== false;
  const moduleClass = installable ? 'fork' : 'maintained';
  const update = updateStateFor(item, remote, kind, moduleClass);
  const modification = modificationStateFor(item, release, kind, id);
  const lifecycle = appLifecycleBadge(item, appStorePermissionOptions(state));
  const releaseProjection = appReleaseProjection(item);
  return {
    id,
    kind,
    status,
    launch_kind: item.launch_kind || 'module',
    title: item.title || item.default_title || id,
    description: item.description || '',
    category: displayCategory(item.category || item.source, item.core ? 'System' : 'Local'),
    version: item.version || item.release || 'v1',
    developer: item.developer || item.publisher || 'CTOX',
    license: item.license || 'AGPL-3.0-only',
    source: sourceLabel(item, kind),
    repo: item.repo || item.repository || remote?.repo || '',
    download_url: item.download_url || remote?.download_url || '',
    source_path: item.source_path || '',
    manifest_url: item.manifest_url || remote?.manifest_url || '',
    homepage: item.homepage || remote?.homepage || '',
    icon_svg: item.layout?.icon_svg || item.icon_svg || '',
    install_scope: item.install_scope || item.raw?.install_scope || '',
    permissions: item.permissions || item.collections || [],
    installable,
    module_class: moduleClass,
    editable: item.editable === true && kind !== 'system',
    deletable: item.deletable === true && kind === 'installed',
    manifest_sha256: item.manifest_sha256 || '',
    local_manifest_path: item.local_manifest_path || '',
    installed_version: installedVersion,
    available_version: availableVersion,
    update_available: update.available,
    update_reason: update.reason,
    modification_status: modification.status,
    modification_label: modification.label,
    lifecycle,
    release_projection: releaseProjection,
    version_state: versionStateFor(id),
    latest_release: release,
    app_source: (item.app_source && typeof item.app_source === 'object') ? item.app_source : null,
    instance_visible: item.instance_visible !== false,
    raw: item,
  };
}

function externalSourceBadgeHtml(item) {
  const src = item?.app_source;
  if (!src || typeof src !== 'object') return '';
  const kind = String(src.kind || '').trim();
  if (kind !== 'github' && kind !== 'url') return '';
  if (src.verified === true) return '';
  const where = kind === 'github' && src.repo ? ` · ${escapeHtml(String(src.repo))}` : '';
  return `<span class="ctox-badge is-warning" title="Aus externer Quelle installiert – noch nicht verifiziert. Externe Apps erhalten keine Datenrechte bis zum Data-Access-Review.">Externe Quelle · nicht verifiziert${where}</span>`;
}

function statusForItem(item, kind) {
  if (kind === 'marketplace') {
    if (installedIds().has(item.module_id || item.id)) return 'installed';
    return item.installable === false || item.store?.installable === false ? 'system' : 'available';
  }
  if (kind === 'template') return 'template';
  if (kind === 'system') return 'system';
  if (kind === 'installed') return 'installed';
  return 'local';
}

function sourceLabel(item, kind) {
  if (kind === 'marketplace' && item.catalog) return item.source === 'instance' ? 'Diese Instanz' : 'CTOX main';
  if (kind === 'marketplace') return item.repo || item.source || 'GitHub';
  if (kind === 'template') return item.source_module || 'template-store';
  return item.source || kind;
}

function syncCategoryOptions() {
  if (!els.categoryFilter) return;
  const categories = categoriesOf(scopedCatalogItems(state.scope));
  const wanted = ['all', ...categories];
  const current = [...els.categoryFilter.options].map((option) => option.value);
  if (wanted.join('|') !== current.join('|')) {
    els.categoryFilter.innerHTML = '<option value="all">Alle Kategorien</option>'
      + categories.map((category) => `<option value="${escapeAttr(category)}">${escapeHtml(category)}</option>`).join('');
  }
  const clamped = wanted.includes(state.categoryFilter) ? state.categoryFilter : 'all';
  els.categoryFilter.value = clamped;
  // Keep module state coherent with the clamped select (a stale category
  // would otherwise filter the list to zero rows while the select shows
  // "Alle Kategorien", e.g. after a scope switch).
  state.categoryFilter = clamped;
}

// Counts on the counted view band (zeros included) + both one-line pane
// footers go through the shell-wired grammar handle when present
// (null-guarded: the shell wires panes debounced ~120ms after mount, so early
// renders fall back to the direct data-pg-* targets). The left pane has no
// grammar wiring — its footer fills directly.
function syncGrammarSurfaces(items, searched) {
  const counts = { catalog: searched.length, updates: searched.filter(itemHasUpdate).length };
  const pg = els.centerPane?.__ctoxPaneGrammar;
  if (pg?.setCounts) pg.setCounts(counts);
  else for (const [key, value] of Object.entries(counts)) {
    const node = els.centerPane?.querySelector(`[data-pg-count="${key}"]`);
    if (node) node.textContent = ` (${value})`;
  }
  // Textdiät Regel 3/5: die Anzahl steht im Reiter „Katalog(N)“ – kein zweiter Zähler im Fuß.
  const footerText = '';
  if (pg?.setFooter) pg.setFooter(footerText);
  else {
    const node = els.centerPane?.querySelector('[data-pg-footer]');
    if (node) node.textContent = footerText;
  }
  const leftFooter = els.leftPane?.querySelector('[data-pg-footer]');
  if (leftFooter) leftFooter.textContent = '';
}

// Band „Updates“: nur installierte, NICHT lokal angepasste Apps, für die app_catalog eine höhere main-Version führt
// (update_available aus rebuildCatalog). Früher (v3, OFFEN-4) zählte jede abweichende Anzeige-Zeile „Installiert: x“ /
// „Katalog: lokal“ als Update – damit stand jede installierte App im Band.
function itemHasUpdate(item) {
  if (!isInstalledCatalogItem(item)) return false;
  return item.update_available === true && !itemIsCustomized(item);
}

function itemIsCustomized(item) {
  return item?.modification_status === 'modified'
    || item?.modification_status === 'customized'
    || item?.raw?.locally_modified === true;
}

// Angepasste App mit verfügbarem main-Release: eigener Fachbegriff am Eintrag, kein Banner.
function customizedUpdateBadgeHtml(item) {
  if (item?.update_available !== true || !itemIsCustomized(item)) return '';
  const ver = String(item.raw?.catalog_release?.app_ver || '');
  const text = state.t('badgeUpdateCustomized', 'Update {ver} · angepasst').replace('{ver}', ver);
  return `<span class="ctox-badge is-warning" data-update-customized="${escapeAttr(ver)}">${escapeHtml(text)}</span>`;
}

function categoriesOf(items) {
  return [...new Set(items.map((item) => String(item.category || '').trim()).filter(Boolean))]
    .sort((left, right) => left.localeCompare(right, undefined, { sensitivity: 'base' }));
}

function searchedItems() {
  return scopedCatalogItems(state.scope).filter((item) => {
    const haystack = `${item.title} ${item.description} ${item.category} ${item.repo} ${item.source}`.toLowerCase();
    if (state.query && !haystack.includes(state.query)) return false;
    return state.categoryFilter === 'all' || String(item.category || '').trim() === state.categoryFilter;
  });
}

function filteredItems() {
  let items = searchedItems();
  if (state.centerBand === 'updates') items = items.filter(itemHasUpdate);
  const key = state.sortKey === 'category' ? 'category' : 'title';
  return [...items].sort((left, right) => String(left[key] || '').localeCompare(String(right[key] || ''), undefined, { sensitivity: 'base' }));
}

// The header export follows the exact list contract used by both the shelf and
// compact list renderers: active scope, search, category, band and sort are all
// reflected, so "Installed" exports only the installed rows currently visible.
function exportVisibleCatalog() {
  const exportedAt = new Date().toISOString();
  const payload = buildAppStoreExport({
    items: filteredItems(),
    exportedAt,
    scope: state.scope,
    query: state.query,
    categoryFilter: state.categoryFilter,
    sortKey: state.sortKey,
    centerBand: state.centerBand,
  });
  downloadJsonFile(
    `ctox-app-store-${payload.scope}-${payload.view}-${exportedAt.slice(0, 19).replace(/[:T]/g, '-')}.json`,
    payload,
  );
}

function buildAppStoreExport({ items, exportedAt, scope, query, categoryFilter, sortKey, centerBand }) {
  const apps = (Array.isArray(items) ? items : []).map((item) => ({
    id: String(item?.id || item?.module_id || ''),
    title: String(item?.title || item?.name || item?.id || ''),
    description: String(item?.description || ''),
    category: String(item?.category || ''),
    kind: String(item?.kind || ''),
    status: String(item?.status || ''),
    version: String(item?.version || item?.installed_version || item?.available_version || ''),
    developer: String(item?.developer || ''),
    license: String(item?.license || ''),
    source: String(item?.source || ''),
    repo: String(item?.repo || ''),
    sourcePath: String(item?.source_path || ''),
    installScope: String(item?.install_scope || ''),
    launchKind: String(item?.launch_kind || ''),
    installed: isInstalledCatalogItem(item),
    updateAvailable: itemHasUpdate(item),
  }));
  return {
    format: 'ctox-app-store-export',
    version: 1,
    exportedAt: String(exportedAt || new Date().toISOString()),
    module: 'app-store',
    scope: String(scope || 'all'),
    view: centerBand === 'updates' ? 'updates' : 'catalog',
    filters: {
      search: String(query || ''),
      category: String(categoryFilter || 'all'),
      sort: sortKey === 'category' ? 'category' : 'title',
    },
    count: apps.length,
    apps,
  };
}

function downloadJsonFile(filename, payload) {
  const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  anchor.style.display = 'none';
  (state.root || document.documentElement).appendChild(anchor);
  anchor.click();
  anchor.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function scopedCatalogItems(scope) {
  const items = scope === 'marketplace' ? rawCatalogItems() : catalogItems();
  const scoped = scope === 'all'
    ? items
    : items.filter((item) => itemMatchesScope(item, scope));
  return uniqueCatalogItems(scoped);
}

function currentCatalogItem(appId) {
  if (!appId) return null;
  return scopedCatalogItems(state.scope).find((candidate) => candidate.id === appId)
    || catalogItems().find((candidate) => candidate.id === appId)
    || rawCatalogItems().find((candidate) => candidate.id === appId)
    || null;
}

function itemMatchesScope(item, scope) {
  if (scope === 'installed') return isInstalledCatalogItem(item);
  return scope === 'all' || item.kind === scope || item.status === scope;
}

function isInstalledCatalogItem(item) {
  if (!item) return false;
  if (item.id === 'create-scratch') return false;
  if (item.kind === 'marketplace') return item.status === 'installed';
  return ['installed', 'local', 'system'].includes(item.kind)
    || ['installed', 'local', 'system'].includes(item.status);
}

function uniqueCatalogItems(items) {
  const byId = new Map();
  for (const item of items) {
    const key = item.id || item.module_id || item.title;
    if (!key) continue;
    byId.set(key, chooseCanonicalCatalogItem(byId.get(key), item));
  }
  return [...byId.values()].sort(sortItems);
}

function chooseCanonicalCatalogItem(existing, candidate) {
  if (!existing) return candidate;
  const rank = {
    system: 0,
    local: 1,
    installed: 2,
    template: 3,
    marketplace: 4,
  };
  const existingRank = rank[existing.kind] ?? 9;
  const candidateRank = rank[candidate.kind] ?? 9;
  if (candidateRank < existingRank) return candidate;
  if (candidateRank > existingRank) return existing;
  if (candidate.status === 'installed' && existing.status !== 'installed') return candidate;
  return existing;
}

function render({ resetScroll = false } = {}) {
  const items = filteredItems();
  const searched = searchedItems();
  updateScopeButtons();
  renderMarketplaceState();
  renderMessage();
  if (els.title) els.title.textContent = scopeTitle(state.scope);

  // Counted view band (zeros included) + tray options + per-pane footers.
  syncGrammarSurfaces(items, searched);
  syncCategoryOptions();

  // 'cards' renders as the WebGL retail shelf only when motion is allowed;
  // reduced motion and shelf failure both use actionable DOM shard cards.
  const cardsMode = state.viewMode !== 'list';
  const shelfMode = shouldUseShelf(cardsMode, state.shelfUnavailable, prefersReducedMotion());
  // Data re-renders never move the operator: preserve the well's scroll
  // offset across the list rebuild (intentional resets — search/view/band/
  // filter/scope — pass resetScroll because the content set changed). The
  // shell scroll guard backs this up; the reset lands after its restore.
  const scrollTop = resetScroll ? 0 : (els.well?.scrollTop || 0);
  if (els.shelfStage) els.shelfStage.hidden = !shelfMode;
  if (els.grid) {
    els.grid.hidden = shelfMode;
    if (!shelfMode) {
      els.grid.className = cardsMode ? 'store-card-grid' : 'store-card-grid is-list-view';
      els.grid.replaceChildren(...renderCatalogBody(items, { compact: !cardsMode }));
    }
  }
  if (els.well && !shelfMode) {
    if (resetScroll) requestAnimationFrame(() => { els.well.scrollTop = 0; });
    else els.well.scrollTop = scrollTop;
  }
  if (shelfMode) syncShelf(items);

  syncViewToggle();

  if (state.selectedId && !items.some((item) => item.id === state.selectedId)) {
    state.selectedId = '';
    state.drawerOpen = false;
  }
  renderDetails();
}

// The single view control names the view it switches TO — glyph, title and
// aria-label all describe the action. No aria-pressed: it is an action, not a
// two-state control (Betreiber-Direktive 31.08.2026).
function syncViewToggle() {
  if (!els.viewToggle) return;
  const listNow = state.viewMode === 'list';
  const target = listNow ? 'cards' : 'list';
  const label = listNow
    ? state.t('viewShowCards', 'Als Karten anzeigen')
    : state.t('viewShowList', 'Als Liste anzeigen');
  els.viewToggle.dataset.viewMode = listNow ? 'list' : 'cards';
  els.viewToggle.setAttribute('aria-label', label);
  els.viewToggle.title = label;
  els.viewToggle.removeAttribute('aria-pressed');
  for (const glyph of els.viewGlyphs || []) {
    glyph.hidden = glyph.dataset.viewGlyph !== target;
  }
}

// ---------------------------------------------------------------------------
// Retail-box shelf (vendor/store-shelf): the "cards" rendering of this app.
// Load lazily; a WebGL/import failure falls back to DOM shard cards, so the
// cards view keeps a rendering and the toggle keeps two real views.
// ---------------------------------------------------------------------------
async function ensureShelf() {
  if (state.shelf || state.shelfUnavailable) return state.shelf;
  if (!els.shelfCanvas || !els.shelfStage || !els.shelfScroll || !els.shelfTrack) return null;
  // Single-flight: every render calls this; concurrent imports must never
  // create competing shelf instances on the same canvas (the empty last one
  // would own the WebGL context and the boxes would never show).
  if (state.shelfPromise) return state.shelfPromise;
  const owner = state.mountOwner;
  const hosts = {
    canvas: els.shelfCanvas, stage: els.shelfStage,
    scroll: els.shelfScroll, track: els.shelfTrack,
  };
  state.shelfPromise = buildShelf(owner, hosts);
  return state.shelfPromise;
}

async function buildShelf(owner, hosts) {
  try {
    // Vendor-Bibliothek der App (vendor/store-shelf + vendor/three); scheitert der Import, bleiben DOM-Karten.
    const mod = await import('./vendor/store-shelf/store-shelf.mjs');
    if (!owner?.active || state.mountOwner !== owner) return null;
    const shelf = mod.createStoreShelf(hosts.canvas, {
      apps: [],
      locale: state.lang === 'en' ? 'en' : 'de',
      scrollContainer: hosts.scroll,
      track: hosts.track,
      stage: hosts.stage,
      onSelect: (id) => {
        if (!owner.active || state.mountOwner !== owner) return;
        state.selectedId = id;
        state.drawerOpen = true;
        render();
      },
    });
    if (!owner.active || state.mountOwner !== owner) {
      shelf.destroy();
      return null;
    }
    state.shelf = shelf;
  } catch (err) {
    if (!owner?.active || state.mountOwner !== owner) return null;
    // The cards view stays the cards view: without the WebGL surface it
    // renders as DOM shard cards, so the toggle keeps two real renderings.
    console.warn('[app-store] shelf unavailable, falling back to DOM shard cards', err);
    state.shelfUnavailable = true;
  }
  return state.shelf;
}

function shelfAppFor(item) {
  const locallyServed = ['installed', 'system', 'local'].includes(item.kind) || isInstalledCatalogItem(item);
  const preview = previewUrlFor(item.id);
  return {
    id: item.id,
    title: item.title || item.id,
    category: item.category || 'App',
    description: item.description || '',
    // Fremde App-Icons nicht per Pfad aus dem eigenen Artefakt ableiten (`../<id>/icon.svg` war ein HTTP-Abruf
    // quer über Apps); die Shell löst Artefakt-URLs auf (api.shell.assetUrl, V4-PORT.md OFFEN-1).
    iconUrl: locallyServed ? appAssetUrl(item.id, item.raw?.layout?.icon_asset || 'icon.svg') : '',
    screenshots: preview ? [preview] : [],
  };
}

function appAssetUrl(appId, path) {
  const resolve = state.api?.shell?.assetUrl;
  if (typeof resolve !== 'function' || !appId || !path) return '';
  try {
    const url = resolve.call(state.api.shell, appId, path);
    return typeof url === 'string' ? url : '';
  } catch {
    return '';
  }
}

async function syncShelf(items) {
  const owner = state.mountOwner;
  const shelf = await ensureShelf();
  if (!owner?.active || state.mountOwner !== owner) return;
  if (!shelf) {
    if (state.shelfUnavailable) render();
    return;
  }
  const apps = items.filter((item) => item.id !== 'create-scratch').map(shelfAppFor);
  const signature = JSON.stringify(apps.map((app) => [app.id, app.title, app.category, app.screenshots.length]));
  if (signature !== state.shelfSignature) {
    try {
      shelf.setApps(apps);
      state.shelfSignature = signature;
    } catch (err) {
      // A poisoned catalog entry must not leave the shelf permanently empty:
      // log, reset the signature and retry once on the next tick.
      console.error('[app-store] shelf setApps failed, retrying once', err);
      state.shelfSignature = '';
      if (!state.shelfRetryPending) {
        state.shelfRetryPending = true;
        state.shelfRetryTimer = setTimeout(() => {
          if (!owner.active || state.mountOwner !== owner) return;
          state.shelfRetryPending = false;
          render();
        }, 600);
      }
      return;
    }
    if (els.shelfTrack && els.shelfScroll) {
      els.shelfTrack.style.height = `${Math.max(els.shelfScroll.clientHeight + 1, apps.length * 170 + els.shelfScroll.clientHeight * 0.6)}px`;
    }
  }
  if (state.drawerOpen && state.selectedId) shelf.select(state.selectedId);
  else shelf.deselect();
  if (els.shelfHint) els.shelfHint.hidden = state.drawerOpen || !apps.length;
}

function renderCatalogBody(items, { compact = false } = {}) {
  // R11: eine leere Liste rendert nichts.
  return items.map((item) => renderCard(item, { compact }));
}

function cardActionsHtml(item, operation, cardStatus, { includeDetails = true } = {}) {
  let actionsHtml = `<div class="app-card-actions">`;

  if (operation?.kind === 'running') {
    actionsHtml += progressButtonHtml(operation.text || item.title);
  } else if (item.id === 'create-scratch') {
    actionsHtml += `<button type="button" class="ctox-button ctox-button--sm is-primary" data-card-action="open" aria-label="${escapeHtml(item.title)} erstellen">${escapeHtml(state.t('actionCreate', 'Erstellen'))}</button>`;
  } else if (item.kind === 'marketplace') {
    if (cardStatus === 'installed') {
      actionsHtml += `<button type="button" class="ctox-button ctox-button--sm is-primary" data-card-action="open" aria-label="${escapeHtml(item.title)} öffnen">${escapeHtml(state.t('actionOpen', 'Öffnen'))}</button>`;
    } else if (item.installable) {
      actionsHtml += canInstallAppStoreItem(state, item)
        ? `<button type="button" class="ctox-button ctox-button--sm is-primary" data-card-action="install" aria-label="${escapeHtml(item.title)} installieren">${escapeHtml(state.t('actionInstall', 'Installieren'))}</button>`
        : disabledActionButtonHtml(
          state.t('actionInstall', 'Installieren'),
          appStorePermissionDeniedReason('install'),
          item.title,
        );
    }
    if (item.homepage) {
      actionsHtml += `<button type="button" class="ctox-button ctox-button--sm" data-card-action="repository" data-external-action="github" title="GitHub repository in new tab" aria-label="GitHub repository in new tab">GitHub ${externalLinkIcon()}</button>`;
    }
  } else if (item.kind === 'template') {
    actionsHtml += `<button type="button" class="ctox-button ctox-button--sm is-primary" data-card-action="open" aria-label="${escapeHtml(item.title)} erstellen">${escapeHtml(state.t('actionCreate', 'Erstellen'))}</button>`;
  } else if (item.kind === 'system') {
    actionsHtml += `<button type="button" class="ctox-button ctox-button--sm is-primary" data-card-action="open" aria-label="${escapeHtml(item.title)} öffnen">${escapeHtml(state.t('actionOpen', 'Öffnen'))}</button>`;
    actionsHtml += versionsButtonHtml(item);
  } else {
    // Local / Installed non-system apps
    actionsHtml += `
      <button type="button" class="ctox-button ctox-button--sm is-primary" data-card-action="open" aria-label="${escapeHtml(item.title)} öffnen">${escapeHtml(state.t('actionOpen', 'Öffnen'))}</button>
      ${actionButtonsForManagedItem(item, state)}
    `;
  }

  if (includeDetails && item.id !== 'create-scratch') {
    actionsHtml += `<button type="button" class="ctox-button ctox-button--sm ctox-button--ghost" data-card-action="details" aria-label="Details zu ${escapeHtml(item.title)} anzeigen">${escapeHtml(state.t('actionDetails', 'Details'))}</button>`;
  }

  actionsHtml += `</div>`;
  return actionsHtml;
}

// One entry, two shapes (Betreiber-Direktive 31.08.2026):
//   compact=false — a shard: bold title, one meta line of the fields that
//                   actually decide a store action, one state-badge meta line
//                   (modification / release / lifecycle / unverified source),
//                   then the action row. Generous padding. The description
//                   and the full field list live in the detail panel.
//   compact=true  — exactly ONE dense row: title left, one short meta right.
//                   Actions live in the detail panel, which a row click opens.
function renderCard(item, { compact = false } = {}) {
  const operation = operationForItem(item);
  const cardStatus = statusForCard(item, operation);
  const card = document.createElement('article');
  card.className = compact ? 'app-card app-card--row' : 'app-card app-card--shard';
  card.dataset.appId = item.id;
  card.dataset.contextModuleId = item.id;
  card.dataset.contextRecordId = item.id;
  card.dataset.contextRecordType = 'business_app';
  card.dataset.contextLabel = item.title || item.id;
  if (operation?.kind) card.dataset.operation = operation.kind;
  card.classList.toggle('active', item.id === state.selectedId);
  card.classList.toggle('is-operating', operation?.kind === 'running');
  card.tabIndex = 0;
  card.setAttribute('aria-selected', item.id === state.selectedId ? 'true' : 'false');
  card.setAttribute('aria-label', `${item.title}. ${statusLabel(cardStatus)}. ${item.category}.`);

  if (compact) {
    // Maximum density: one line, title + one short meta. Nothing wraps, so
    // every row keeps the same height whatever the catalog holds.
    card.innerHTML = `
      <span class="app-card-icon app-card-icon--row">${iconMarkupForItem(item)}</span>
      <h3 class="app-card-title">${escapeHtml(item.title)}</h3>
      <span class="app-card-rowmeta">${escapeHtml(rowMetaFor(item, cardStatus, operation))}</span>
    `;
    return card;
  }

  const actionsHtml = cardActionsHtml(item, operation, cardStatus);
  const operationHtml = operationMessageHtml(operation);

  card.innerHTML = `
    <div class="app-card-head">
      <div class="app-card-icon">
        ${iconMarkupForItem(item)}
        ${item.lifecycle?.runtimeInstalled ? `<span class="app-lifecycle-dot" data-state="${escapeHtml(item.lifecycle.state)}" title="${escapeAttr(item.lifecycle.title)}" aria-hidden="true"></span>` : ''}
      </div>
      <div class="app-card-meta">
        <h3 class="app-card-title">${escapeHtml(item.title)}</h3>
        <span class="app-card-metaline">${escapeHtml(shardMetaFor(item))}</span>
      </div>
      <span class="ctox-badge ${statusBadgeClass(cardStatus)}">${escapeHtml(statusLabel(cardStatus))}</span>
    </div>
    <div class="app-card-version-row">
      <span class="ctox-badge ${modStateBadgeClass(item.modification_status)}">${escapeHtml(item.modification_label)}</span>
      ${customizedUpdateBadgeHtml(item)}
      ${releaseProjectionBadgeHtml(item)}
      ${item.lifecycle?.runtimeInstalled ? `<span class="ctox-badge ${lifecycleStateBadgeClass(item.lifecycle.state)}" data-state="${escapeHtml(item.lifecycle.state)}" title="${escapeAttr(item.lifecycle.title)}">${escapeHtml(item.lifecycle.version)} · ${escapeHtml(item.lifecycle.text)}</span>` : ''}
      ${externalSourceBadgeHtml(item)}
    </div>
    ${actionsHtml}
    ${operationHtml}
  `;
  return card;
}

// Shard meta line: category plus the version that is actually live. The
// source stays off this line — the status badge next to the title already
// names it, and a third segment only pushed the line into an ellipsis.
function shardMetaFor(item) {
  const version = item.installed_version && item.installed_version !== '-'
    ? item.installed_version
    : item.available_version;
  return [item.category, version]
    .map((part) => String(part || '').trim())
    .filter((part) => part && part !== '-')
    .join(' · ');
}

// List meta: ONE short right-hand field. A running or failed operation
// outranks the resting status, because that is the state the row is in.
function rowMetaFor(item, cardStatus, operation) {
  if (operation?.kind === 'running' || operation?.kind === 'error') {
    return statusLabel(cardStatus);
  }
  return statusLabel(cardStatus) || item.category || '';
}

function releaseProjectionBadgeHtml(item) {
  const projection = item?.release_projection;
  if (!projection?.hasReleaseState || (!projection.currentVersion && projection.status === 'unreleased')) return '';
  const text = projection.currentVersion
    ? `Freigabe ${projection.currentVersion}`
    : (projection.statusLabel || 'Freigabe');
  const title = [
    projection.releaseLine,
    projection.rollbackLine,
    projection.dataAccess?.summary,
  ].filter(Boolean).join(' · ');
  return `<span class="ctox-badge ${releaseStatusBadgeClass(projection.status)}" data-release-status="${escapeHtml(projection.status || 'unknown')}" title="${escapeAttr(title)}">${escapeHtml(text)}</span>`;
}

// Lifecycle, release and modification states map onto the shared .ctox-badge
// state variants (shared/base.css §10). The data-state / data-release-status
// attributes stay as diagnostic hooks next to the visual variant class.
function lifecycleStateBadgeClass(state) {
  if (state === 'private') return 'is-warning';
  if (state === 'preview') return 'is-info';
  if (state === 'team') return 'is-success';
  if (state === 'restricted') return 'is-danger';
  return '';
}

function releaseStatusBadgeClass(status) {
  if (status === 'released') return 'is-success';
  if (status === 'rolled_back') return 'is-warning';
  return '';
}

function modStateBadgeClass(status) {
  if (status === 'clean') return 'is-success';
  if (status === 'modified') return 'is-warning';
  if (status === 'unreleased' || status === 'unknown') return 'is-danger';
  if (status === 'catalog') return 'is-info';
  return '';
}

function operationForItem(itemOrId) {
  const id = typeof itemOrId === 'string' ? itemOrId : itemOrId?.id;
  return id ? state.operations[id] || null : null;
}

function statusForCard(item, operation = operationForItem(item)) {
  if (operation?.kind === 'running') return 'installing';
  if (operation?.kind === 'error') return 'error';
  if (operation?.kind === 'success') return 'installed';
  return item?.status || '';
}

// Maps the card status onto the shared .ctox-badge state modifiers
// (shared/base.css §10). "local" keeps its module-specific purple tint.
function statusBadgeClass(status) {
  if (status === 'installed') return 'is-success';
  if (status === 'installing') return 'is-warning';
  if (status === 'error') return 'is-danger';
  if (status === 'core' || status === 'system' || status === 'template') return 'is-info';
  if (status === 'local') return 'app-badge-local';
  return '';
}

function progressButtonHtml(label) {
  return `
    <button type="button" class="ctox-button ctox-button--sm is-primary is-progress" disabled aria-disabled="true" aria-live="polite">
      <span class="card-btn-progress-label">${escapeHtml(label)}</span>
      <span class="card-btn-progress-track" aria-hidden="true"><span></span></span>
    </button>`;
}

// R11: kein Vorgangstext in der Karte; Befehlsstatus steht in der Zustandszone.
function operationMessageHtml() {
  return '';
}

function renderDetails() {
  const item = catalogItems().find((candidate) => candidate.id === state.selectedId);
  if (els.detail) els.detail.hidden = !state.drawerOpen;
  if (!state.drawerOpen) return;
  if (!item) {
    renderEmptyDetails();
    return;
  }
  if (els.detailIcon) els.detailIcon.innerHTML = iconMarkupForItem(item);
  if (els.detailTitle) els.detailTitle.textContent = item.title;
  if (els.detailVersion) els.detailVersion.textContent = item.lifecycle?.version || item.version;
  if (els.detailCategory) els.detailCategory.textContent = item.category;
  if (els.detailDeveloper) els.detailDeveloper.textContent = item.developer;
  if (els.detailLicense) els.detailLicense.textContent = item.license;
  if (els.detailSource) els.detailSource.textContent = item.source;
  if (els.detailStatus) els.detailStatus.textContent = statusLabel(item.status);
  if (els.detailActions) {
    const operation = operationForItem(item);
    els.detailActions.innerHTML = cardActionsHtml(item, operation, statusForCard(item, operation), { includeDetails: false });
  }
  if (els.detailCapture && els.detailCaptureImg) {
    const preview = previewUrlFor(item.id);
    els.detailCapture.hidden = !preview;
    if (preview) {
      els.detailCaptureImg.src = preview;
      els.detailCaptureImg.alt = `${item.title} – Produktaufnahme`;
    }
  }
  if (els.readme) {
    els.readme.replaceChildren(renderDocumentation(item));
  }
}

function renderEmptyDetails() {
  if (els.detail) els.detail.hidden = true;
}

function renderDocumentation(item) {
  const wrap = document.createElement('div');
  const releaseFacts = releaseFactLinesForItem(item);
  const lines = [
    item.description || '',
    item.installed_version ? item.installed_version : '',
    item.available_version ? item.available_version : '',
    item.lifecycle?.label ? `Sichtbarkeit: ${item.lifecycle.label} - ${item.lifecycle.reason}` : '',
    ...releaseFacts,
    item.update_reason ? `Update: ${item.update_reason}` : '',
    item.modification_label ? `Modifikation: ${item.modification_label}` : '',
    item.latest_release ? `Letztes Release: v${item.latest_release.version} (${item.latest_release.status})` : '',
    item.repo ? `Repository: ${item.repo}` : '',
    item.source_path ? `Source path: ${item.source_path}` : '',
    item.local_manifest_path ? `Local manifest: ${item.local_manifest_path}` : '',
    item.download_url ? `Installer archive: ${item.download_url}` : '',
    item.permissions?.length && !item.release_projection?.dataAccess?.hasReview
      ? `Deklarierte Datenbereiche: ${item.permissions.map(businessDataAreaLabel).join(', ')}`
      : '',
  ].filter(Boolean);
  for (const line of lines) {
    const p = document.createElement('p');
    p.textContent = line;
    wrap.append(p);
  }
  return wrap;
}

function releaseFactLinesForItem(item) {
  const projection = item?.release_projection || appReleaseProjection(item?.raw || item);
  if (!projection) return [];
  const lines = [];
  if (projection.hasReleaseState) {
    lines.push(`Freigabe: ${projection.releaseLine}`);
    if (projection.rollbackLine) lines.push(`Rollback: ${projection.rollbackLine}`);
  }
  if (projection.dataAccess?.summary) {
    lines.push(`Datenzugriff: ${projection.dataAccess.summary}`);
  }
  if (projection.dataAccess?.reviewNote) {
    lines.push(`Review: ${projection.dataAccess.reviewNote}`);
  }
  return lines;
}

// Installieren aus app_catalog (§16.29): ctox.app.update mit base_app_ver = null (die App gibt es auf der Instanz noch
// nicht; Vergleich-und-Setzen wie §16.22 A3 „null = neue App“), release_ver = Katalogversion.
async function installMarketplaceItem(item) {
  if (!canInstallAppStoreItem(state, item)) {
    reportStatus('warn', 'app-store.permission.denied', 'Du darfst diese App nicht installieren oder aktualisieren.');
    return;
  }
  const releaseVer = String(item.raw?.catalog?.app_ver || item.version || '');
  if (!releaseVer) return;
  runAppCommand('ctox.app.update', item, { release_ver: releaseVer }, {
    label: `Installation ${item.title}`,
    actionLabel: state.t('actionInstall', 'Installieren'),
    baseAppVer: null,
  });
}

async function installFromGithub() {
  if (!actorMay(state, BusinessOsPermissions.AppsInstall)) {
    reportStatus('warn', 'app-store.permission.denied', 'Du darfst keine Apps installieren.');
    return;
  }
  const repo = await askText('GitHub-Repository (owner/name):', '');
  if (!repo) return;
  if (!/^[\w.-]+\/[\w.-]+$/.test(repo)) {
    reportStatus('warn', 'app-store.input.invalid', `Ungültiges Repository: ${repo} (erwartet owner/name).`);
    return;
  }
  const gitRef = await askText('Branch / Tag / Commit (leer = HEAD):', 'main');
  const subpath = await askText('Pfad zum Modul im Repo (leer = Wurzel):', '');
  const moduleId = sanitizeId(await askText('Modul-ID (muss zur module.json im Repo passen):', ''));
  if (!moduleId) {
    reportStatus('warn', 'app-store.input.invalid', 'Modul-ID ist erforderlich.');
    return;
  }
  // v4-allow: text – Bestätigungsdialog (api.shell.dialog), kein Inhaltstext
  if (!await askConfirm('Aus EXTERNER Quelle installieren?', `Repo: ${repo}\nRef: ${gitRef || 'HEAD'}\nModul: ${moduleId}\n\nExterne Apps sind zunächst NICHT verifiziert und erhalten keine Datenrechte bis zur Prüfung.`, 'Installieren')) {
    return;
  }
  runStoreCommand({
    label: `Installation ${moduleId} aus GitHub`,
    commandType: 'ctox.app_store.install',
    moduleId,
    payload: {
      module_id: moduleId,
      source_kind: 'github',
      repo,
      git_ref: gitRef,
      subpath,
    },
  });
}

function pickZipFile() {
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.zip,application/zip';
    input.style.display = 'none';
    input.addEventListener('change', () => {
      const file = input.files && input.files[0] ? input.files[0] : null;
      input.remove();
      resolve(file);
    }, { once: true });
    (state.root || document.documentElement).appendChild(input);
    input.click();
  });
}

async function installFromZip() {
  if (!actorMay(state, BusinessOsPermissions.AppsInstall)) {
    reportStatus('warn', 'app-store.permission.denied', 'Du darfst keine Apps installieren.');
    return;
  }
  const file = await pickZipFile();
  if (!file) return;
  const moduleId = sanitizeId(await askText('Modul-ID (muss zur module.json im Zip passen):', ''));
  if (!moduleId) {
    reportStatus('warn', 'app-store.input.invalid', 'Modul-ID ist erforderlich.');
    return;
  }
  const subpath = await askText('Pfad zum Modul im Zip (leer = Wurzel):', '');
  // v4-allow: text – Bestätigungsdialog (api.shell.dialog), kein Inhaltstext
  if (!await askConfirm('App aus EXTERNER Zip-Datei installieren?', `Datei: ${file.name}\nModul: ${moduleId}\n\nExterne Apps sind zunächst NICHT verifiziert und erhalten keine Datenrechte bis zur Prüfung.`, 'Installieren')) {
    return;
  }
  // v4 (§10.2a Nr. 4): Datei über api.files.put statt Chunk-Collections; der Befehl nennt die Datei-ID und den
  // Inhalts-Hash (ref). `ref` ist lokal berechnet, kein Warten auf das Netz.
  let fileId;
  let fileRef;
  try {
    fileId = `appzip_${Date.now()}_${Math.random().toString(36).slice(2, 10)}`;
    const upload = state.api.files.put(file, { id: fileId, name: file.name || 'app.zip', mime: file.type || 'application/zip' });
    fileRef = await upload.ref;
  } catch (err) {
    reportStatus('error', 'app-store.upload.failed', `Upload fehlgeschlagen: ${err?.message || err}`);
    return;
  }
  clearStatus('app-store.upload.failed');
  runStoreCommand({
    label: `Installation ${moduleId} aus Zip`,
    commandType: 'ctox.app_store.install',
    moduleId,
    payload: {
      module_id: moduleId,
      source_kind: 'zip',
      file_id: fileId,
      file_ref: fileRef,
      subpath,
    },
  });
}

async function checkModuleUpdates(item) {
  runStoreCommand({
    label: `Update-Prüfung ${item.title}`,
    commandType: 'ctox.module.check_updates',
    moduleId: item.id,
    payload: { module_id: item.id },
  });
}

async function updateInstalledItem(item, { mode = 'vanilla' } = {}) {
  if (!canInstallAppStoreItem(state, item)) {
    reportStatus('warn', 'app-store.permission.denied', 'Du darfst diese App nicht aktualisieren.');
    return;
  }
  // GitHub-sourced apps update by re-installing from their pinned source.
  const src = item.app_source;
  if (src && src.kind === 'github' && src.repo) {
    runStoreCommand({
      label: `Update ${item.title} aus GitHub`,
      actionLabel: state.t('actionUpdate', 'Aktualisieren'),
      commandType: 'ctox.app_store.install',
      moduleId: item.id,
      payload: {
        module_id: item.id,
        source_kind: 'github',
        repo: src.repo,
        git_ref: src.ref || '',
        subpath: src.subpath || '',
      },
    });
    return;
  }
  // §16.29: ctox.app.update {app_id, base_app_ver, release_ver} – der Kern nimmt es nur für locally_modified=false an.
  // Eine angepasste App kommt hier nur nach dem Verwerfen-Dialog an (mode 'discard'); die ausdrückliche Zustimmung
  // trägt discard_local: true (V4-PORT.md OFFEN-3, Kern entscheidet).
  const releaseVer = String(item.raw?.catalog_release?.app_ver || '');
  if (!releaseVer) return;
  runAppCommand('ctox.app.update', item, { release_ver: releaseVer, ...(mode === 'discard' ? { discard_local: true } : {}) }, {
    label: `Update ${item.title}`,
    actionLabel: state.t('actionUpdate', 'Aktualisieren'),
  });
}

async function uninstallInstalledItem(item) {
  if (!canUninstallAppStoreItem(state, item)) {
    reportStatus('warn', 'app-store.permission.denied', 'Du darfst diese App nicht entfernen.');
    return;
  }
  if (!await askConfirm(`${item.title} entfernen?`, `Uninstall ${item.title}? Local source files will be removed from installed-modules.`, state.t('actionUninstall', 'Deinstallieren'))) return;
  // §16.29: ctox.app.remove {app_id, base_app_ver} – der Kern archiviert die App.
  runAppCommand('ctox.app.remove', item, {}, {
    label: `Entfernen ${item.title}`,
    actionLabel: state.t('actionUninstall', 'Deinstallieren'),
  });
}

function originLabel(origin) {
  return {
    install: 'Installation',
    manual_release: 'Release',
    rollback: 'Rollback',
    edit: 'Bearbeitung',
    creator_deploy: 'Creator',
  }[origin] || origin || 'Version';
}

async function openVersionsDialog(item) {
  const versions = await moduleBundleVersionsFor(item?.id, item.version_state, item);
  if (!versions.length) {
    reportStatus('info', 'app-store.versions.none', `Keine Versionen für ${item.title} vorhanden.`, item.id);
    return;
  }

  const overlay = document.createElement('div');
  overlay.className = 'ctox-modal app-store-version-dialog';
  const rows = versions.map((version) => {
    const date = version.created_at_ms ? new Date(version.created_at_ms).toLocaleString() : '';
    const seal = version.sealed ? '' : ' · offen';
    const meta = `#${version.seq} · ${escapeHtml(originLabel(version.origin))}${seal} · ${version.file_count || 0} Dateien · ${escapeHtml(date)}`;
    return `
      <li class="app-version-row">
        <div class="app-version-meta">
          <strong>${escapeHtml(version.label || originLabel(version.origin))}</strong>
          <span>${meta}</span>
        </div>
        <button type="button" class="ctox-button" data-rollback-version="${escapeHtml(version.version_id)}">Wiederherstellen</button>
      </li>`;
  }).join('');

  overlay.innerHTML = `
    <div class="ctox-modal-card" role="dialog" aria-modal="true" aria-label="Versionen von ${escapeHtml(item.title)}">
      <header class="ctox-modal-header">
        <h3 class="ctox-modal-title">Versionen – ${escapeHtml(item.title)}</h3>
        <button type="button" class="ctox-pane-icon" data-version-close aria-label="Schließen" title="Schließen">${actionIcon('close')}</button>
      </header>
      <div class="ctox-modal-body">
        <ul class="app-version-list">${rows}</ul>
      </div>
    </div>`;

  const close = () => {
    overlay.remove();
    window.removeEventListener('keydown', onEscape);
  };
  const onEscape = (event) => { if (event.key === 'Escape') close(); };

  overlay.addEventListener('click', async (event) => {
    if (event.target === overlay || event.target.closest('[data-version-close]')) {
      close();
      return;
    }
    const rollbackBtn = event.target.closest('[data-rollback-version]');
    if (!rollbackBtn) return;
    const versionId = rollbackBtn.dataset.rollbackVersion;
    // v4-allow: text – Bestätigungsdialog (api.shell.dialog), kein Inhaltstext
    if (!await askConfirm(`${item.title} zurücksetzen?`, `${item.title} auf diese Version zurücksetzen? Die aktuelle Quelle wird vorher als Wiederherstellungs-Version gesichert.`, 'Wiederherstellen')) return;
    close();
    await rollbackToVersion(item, versionId);
  });

  window.addEventListener('keydown', onEscape);
  els.root?.append(overlay);
}

// §9.5/§16.29: Rückfall = vorwärts zurückrollen (der Kern veröffentlicht die frühere Fassung als neue Version):
// ctox.app.rollback {app_id, base_app_ver, target_ver}. Ein älteres Schema geht nur mit Rück-Migration; fehlt sie,
// lehnt der Kern ab und die Ablehnung steht in der Zone.
function rollbackToVersion(item, versionId) {
  const versions = state.catalog?.version_states?.[item.id]?.versions || item.version_state?.versions || [];
  const target = versions.find((version) => version.version_id === versionId);
  const targetVer = String(target?.app_ver ?? String(versionId || '').split('@')[1] ?? '');
  if (!targetVer) return;
  runAppCommand('ctox.app.rollback', item, { target_ver: targetVer }, {
    label: `Zurücksetzen ${item.title}`,
    actionLabel: 'Wiederherstellen',
  });
}

// Eigener Entwurf (app_drafts, id '<nutzer>/<app>') → ctox.app.publish {app_id, base_app_ver, draft_id}.
function draftFor(appId) {
  if (!appId) return null;
  const suffix = `/${appId}`;
  return (state.platformDrafts || []).find((draft) => draft && !draft._deleted
    && (draft.app_id === appId || String(draft.id || '').endsWith(suffix))) || null;
}

function publishDraft(item) {
  const draft = draftFor(item.id);
  if (!draft) return;
  runAppCommand('ctox.app.publish', item, { draft_id: String(draft.id) }, {
    label: `Veröffentlichen ${item.title}`,
    actionLabel: state.t('actionPublish', 'Veröffentlichen'),
  });
}

async function openReleaseDialog(item) {
  const model = releaseWizardModel(item, state);
  const overlay = document.createElement('div');
  overlay.className = 'ctox-modal';
  // §16.29: ctox.app.release trägt nur den Kanal. Datenbereiche stehen zur Prüfung da (Nachweis, keine Rechte).
  const dataRows = model.dataAreas.map((area) => `
      <li class="app-release-data-row">
        <div>
          <strong>${escapeHtml(area.label)}</strong>
          <span>${escapeHtml(area.collection)}</span>
        </div>
      </li>`).join('');
  overlay.innerHTML = `
    <form class="ctox-modal-card ctox-modal-card--wide app-release-dialog" role="dialog" aria-modal="true" aria-label="Freigabe von ${escapeAttr(item.title)}">
      <header class="ctox-modal-header">
        <h3 class="ctox-modal-title">Freigabe - ${escapeHtml(item.title)}</h3>
        <button type="button" class="ctox-pane-icon" data-release-close aria-label="Schließen" title="Schließen">${actionIcon('close')}</button>
      </header>
      <div class="ctox-modal-body app-release-form">
        <label>
          <span class="ctox-field-label">Kanal</span>
          <select class="ctox-select" name="release_channel">
            <option value="team" ${model.releaseChannel === 'team' ? 'selected' : ''}>Team</option>
            <option value="restricted" ${model.releaseChannel === 'restricted' ? 'selected' : ''}>Eingeschränkt</option>
          </select>
        </label>
        ${dataRows ? `<section class="app-release-data-review" aria-label="Datenbereiche">
          <h4>Datenbereiche</h4>
          <ul>${dataRows}</ul>
        </section>` : ''}
      </div>
      <footer class="ctox-modal-footer">
        <button type="button" class="ctox-button" data-release-close>Abbrechen</button>
        <button type="submit" class="ctox-button is-primary">Freigeben</button>
      </footer>
    </form>`;

  const close = () => {
    overlay.remove();
    window.removeEventListener('keydown', onEscape);
  };
  const onEscape = (event) => { if (event.key === 'Escape') close(); };
  overlay.addEventListener('click', (event) => {
    if (event.target === overlay || event.target.closest('[data-release-close]')) close();
  });
  overlay.querySelector('form')?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const payload = releasePayloadFromForm(item, event.currentTarget);
    close();
    await releaseModule(item, payload);
  });
  window.addEventListener('keydown', onEscape);
  els.root?.append(overlay);
}

// Versionen live aus business_app_versions (rebuildCatalog → version_states), kein Nachladen, keine Warteschleife.
async function moduleBundleVersionsFor(moduleId, fallbackState = null, item = null) {
  const fallbackVersions = Array.isArray(fallbackState?.versions) ? fallbackState.versions : [];
  if (fallbackVersions.length) return fallbackVersions;
  const versions = state.catalog?.version_states?.[moduleId]?.versions;
  if (Array.isArray(versions) && versions.length) return versions;
  return versionSummariesFromLifecycle(item);
}

function versionSummariesFromLifecycle(item) {
  const releaseProjection = item?.release_projection || appReleaseProjection(item?.raw || item);
  const candidates = [
    releaseProjection?.current,
    releaseProjection?.rollbackTarget,
    item?.lifecycle?.release_state?.current,
    item?.lifecycle?.release_state?.rollback_target,
    item?.lifecycle?.rollback_target,
    item?.raw?.lifecycle?.release_state?.current,
    item?.raw?.lifecycle?.release_state?.rollback_target,
    item?.raw?.lifecycle?.rollback_target,
  ].filter((value) => value && typeof value === 'object');
  const seen = new Set();
  return candidates
    .map((version, index) => {
      const id = String(version.version_id || '').trim();
      if (!id || seen.has(id)) return null;
      seen.add(id);
      return {
        version_id: id,
        seq: Number(version.seq || version.version || candidates.length - index),
        origin: version.origin || (index === 0 ? 'manual_release' : 'install'),
        label: version.label || version.target_version || version.version || id,
        created_at_ms: Number(version.created_at_ms || item?.updated_at_ms || item?.raw?.updated_at_ms || Date.now()),
        sealed: version.sealed !== false,
        file_count: Number(version.file_count || 0),
      };
    })
    .filter(Boolean);
}

// §16.29: ctox.app.release {app_id, base_app_ver, channel}. Der Assistent prüft den Datenzugriff (Nachweis, keine
// Rechte); an den Kern geht nur der Kanal.
function releaseModule(item, payload) {
  runAppCommand('ctox.app.release', item, { channel: String(payload?.release_channel || 'team') }, {
    label: `Freigabe ${item.title}`,
    actionLabel: state.t('actionRelease', 'Freigeben'),
  });
}

function releasePayloadFromForm(item, form) {
  const data = new FormData(form);
  const readCollections = data.getAll('read_collections').map(String);
  const writeCollections = data.getAll('write_collections').map(String);
  return releasePayloadForWizard(item, {
    targetVersion: data.get('target_version'),
    releaseChannel: data.get('release_channel'),
    sourceVersionId: data.get('source_version_id'),
    rollbackVersionId: data.get('rollback_version_id'),
    responsibleUserIds: data.get('responsible_user_ids'),
    notes: data.get('notes'),
    readCollections,
    writeCollections,
  }, state);
}

// R5: Plattform-Befehl über api.command, ohne Warten. Status nur in die Zustandszone (failed → report, done → clear);
// die Karte zeigt solange einen gesperrten Fortschrittsknopf. Der Katalog aktualisiert sich live aus
// business_apps/business_app_versions, kein Nachladen, kein Shell-Ereignis.
// Aktive Version, wie dieser Client sie gerade sieht (business_apps) – die Basis für Vergleich-und-Setzen.
function activeVerOf(appId) {
  const row = (state.platformApps || []).find((app) => app?.id === appId && !app._deleted);
  return row?.active_ver == null ? null : String(row.active_ver);
}

// ctox.app.*: Payload {app_id, base_app_ver, …}. Ablehnung „Basis nicht mehr aktiv“ → Zone mit Aktion.
function runAppCommand(commandType, item, extra, { label, actionLabel = '', baseAppVer } = {}) {
  const appId = item.id;
  const base = baseAppVer === undefined ? activeVerOf(appId) : baseAppVer;
  return runStoreCommand({
    label,
    actionLabel,
    commandType,
    moduleId: appId,
    payload: { app_id: appId, base_app_ver: base, ...extra },
    conflict: { appId, base },
  });
}

// Ablehnung des Kerns (write_result rejected). stale_base/conflict = Basis nicht mehr aktiv → eigener Zoneneintrag je
// App mit Aktion „Auf aktuelle Version setzen“ (Muster v4/source/source-mode.js); sonst Text mit Begründung des Kerns.
function reportRejection({ code, label, commandType, result, ref, conflict }) {
  const reason = String(result?.reason || 'rejected');
  const detail = result?.detail ? String(result.detail) : '';
  if (conflict && (reason === 'stale_base' || reason === 'conflict')) {
    const active = activeVerOf(conflict.appId);
    const seen = conflict.base == null ? 'keine' : conflict.base;
    const now = active && active !== conflict.base ? ` (aktiv: ${active})` : '';
    reportStatus('warn', `app-store.conflict.${conflict.appId}`,
      `${label} abgelehnt: Basis ${seen} ist nicht mehr aktiv${now}.`, ref,
      { label: state.t('actionRebase', 'Auf aktuelle Version setzen'), run: () => rebaseView(conflict.appId) });
    return;
  }
  reportStatus('error', code, `${label} abgelehnt: ${detail || reason || commandType}`, ref);
}

// „Auf aktuelle Version setzen“: Eintrag weg, die App mit ihrer jetzt aktiven Version im Detail; der nächste Befehl
// nimmt diese Version als Basis. Kein automatisches Wiederholen (wie source-mode rebase: nur die Basis setzen).
function rebaseView(appId) {
  clearStatus(`app-store.conflict.${appId}`);
  if (!state.api) return;
  state.selectedId = appId;
  state.drawerOpen = true;
  render();
}

function runStoreCommand({ label, commandType, moduleId, payload, actionLabel = '', conflict = null }) {
  const api = state.api;
  if (!api) return null;
  const id = sanitizeId(moduleId);
  const code = `app-store.command.${commandType}`;
  const failedText = (reason) => `${label}: ${reason || commandType}`;
  let handle;
  try {
    handle = api.command(commandType, { ...payload }, { record_id: moduleId });
  } catch (error) {
    reportStatus('error', code, failedText(error?.message || String(error)), moduleId);
    return null;
  }
  setOperation(id, { kind: 'running', text: actionLabel || label, commandType, commandId: handle.id });
  let off = null;
  let finished = false;
  const finish = () => {
    finished = true;
    if (off) { state.commandOffs?.delete(off); off(); off = null; }
    if (state.operations[id]?.commandId === handle.id) {
      const next = { ...state.operations };
      delete next[id];
      state.operations = next;
      if (state.api === api) render();
    }
  };
  // write_result des Befehls: abgelehnt → Begründung des Kerns (bei ctox.app.* ggf. Konflikt mit Aktion).
  const result = Promise.resolve(handle.result).catch(() => null);
  result.then((r) => {
    if (state.api !== api || r?.status !== 'rejected') return;
    reportRejection({ code, label, commandType, result: r, ref: handle.id, conflict });
    finish();
  });
  off = handle.status.subscribe((value) => {
    if (state.api !== api) return;
    if (value?.status === 'failed') {
      // Ein abgelehnter Befehl meldet sich auch als failed; dann spricht nur die Ablehnung (oben).
      result.then((r) => {
        if (state.api !== api || r?.status === 'rejected') return;
        reportStatus('error', code, failedText(value.error), handle.id);
      });
      finish();
    } else if (value?.status === 'done') {
      clearStatus(code);
      if (conflict) clearStatus(`app-store.conflict.${conflict.appId}`);
      finish();
    }
  });
  if (finished && off) { off(); off = null; }
  if (off) state.commandOffs?.add(off);
  handle.local?.catch?.((error) => {
    if (state.api !== api) return;
    reportStatus('error', code, failedText(error?.message || String(error)), handle.id);
    finish();
  });
  return handle;
}

function setOperation(moduleId, operation) {
  const id = sanitizeId(moduleId);
  if (!id) return;
  state.operations = {
    ...state.operations,
    [id]: operation,
  };
  render();
}

function setScope(scope) {
  state.scope = scope;
  state.selectedId = '';
  // Scope switch is an intentional content change — the well starts at top.
  render({ resetScroll: true });
}

function updateScopeButtons() {
  if (!els.scopes) return;
  const counts = countsByScope();
  for (const button of els.scopes.querySelectorAll('[data-scope]')) {
    const scope = button.dataset.scope || 'marketplace';
    // `active` is the legacy module hook; `is-selected` is the kit list-item
    // selection state (shared/base.css §8). Keep both in sync.
    button.classList.toggle('active', scope === state.scope);
    button.classList.toggle('is-selected', scope === state.scope);
    button.setAttribute('aria-pressed', scope === state.scope ? 'true' : 'false');
    // Shard meta line: parenthesized count as plain inline text (zeros
    // included) — no badge-pill chrome on pure selectors.
    const count = button.querySelector('[data-scope-count]');
    if (count) count.textContent = `(${counts[scope] || 0})`;
  }
}

function countsByScope() {
  return {
    all: scopedCatalogItems('all').length,
    marketplace: scopedCatalogItems('marketplace').length,
    template: scopedCatalogItems('template').length,
    installed: scopedCatalogItems('installed').length,
    system: scopedCatalogItems('system').length,
    local: scopedCatalogItems('local').length,
  };
}

// R11: kein Lade-Overlay, keine Meldungszeile im Inhalt – Katalog ist live und lokal, Zustand in der Zone.
function renderMarketplaceState() {}

function renderMessage() {}

// R13: App-Sprung über api.shell.navigate (unbekanntes Ziel meldet die Shell selbst in die Zone).
function openModule(moduleId, params) {
  if (!moduleId || !state.api) return false;
  return state.api.shell.navigate(moduleId, params || {});
}

// Creator mit Rückweg: früher Tab-Speicher-Übergabe + Hash; jetzt Parameter an api.shell.navigate.
function openCreatorFromStore({ mode = 'scratch', upgrade = '', template = '' } = {}) {
  const hash = creatorHashFromStore({ mode, upgrade, template });
  const params = Object.fromEntries(new URLSearchParams(hash.split('?')[1] || '').entries());
  return openModule('creator', params);
}

function creatorHashFromStore({ mode = 'scratch', upgrade = '', template = '' } = {}) {
  const params = new URLSearchParams({
    source: 'app-store',
    return: 'app-store',
    mode,
  });
  if (upgrade) params.set('upgrade', upgrade);
  if (template) params.set('template', template);
  return `creator?${params.toString()}`;
}

function installedIds() {
  const modules = Array.isArray(state.catalog?.modules) ? state.catalog.modules : [];
  return new Set(modules.map((item) => item.id).filter(Boolean));
}

function marketplaceItemFor(id) {
  if (!id) return null;
  return state.marketplace.find((item) => item.id === id || item.module_id === id) || null;
}

function latestReleaseFor(moduleId) {
  const releases = state.catalog?.governance?.releases?.[moduleId];
  if (!Array.isArray(releases) || !releases.length) return null;
  return [...releases].sort((left, right) => Number(right.version || 0) - Number(left.version || 0))[0] || null;
}

function versionStateFor(moduleId) {
  const states = state.catalog?.version_states;
  if (!states || typeof states !== 'object') return null;
  return states[moduleId] || null;
}

function installedVersionLabel(item, release, kind) {
  if (kind === 'marketplace' || kind === 'template') return 'Nicht installiert';
  if (release?.version) return `Installiert: Release ${release.version}`;
  return `Installiert: ${item.version || 'unversioniert'}`;
}

function availableVersionLabel(remote, item, kind) {
  if (kind === 'template') return 'Template';
  const version = remote?.version || item?.version || '';
  if (!remote && kind !== 'marketplace') return 'Katalog: lokal';
  return `Katalog: ${version || 'unbekannt'}`;
}

function updateStateFor(item, remote, kind, moduleClass) {
  // Primary signal: the server-projected catalog/update diff. The native peer
  // sets lifecycle.update_available when an installed module's upstream catalog
  // bundle (modules/<source>) diverges from the bundle this instance installed.
  // This is the in-repo data path — no out-of-band GitHub fetch.
  const lifecycle = item?.lifecycle || {};
  if (lifecycle.update_available === true || item?.update_available === true) {
    const catalogVersion = String(lifecycle.catalog_version || '').trim();
    const installedVersion = String(lifecycle.installed_version || item?.version || '').trim();
    return {
      available: true,
      reason: catalogVersion
        ? `Katalog ${catalogVersion} verfügbar (installiert ${installedVersion || 'unversioniert'}).`
        : 'Eine neuere Katalog-Version ist verfügbar.',
    };
  }
  if (!['installed', 'local'].includes(kind)) {
    return { available: false, reason: kind === 'system' ? 'System-Module werden über CTOX selbst aktualisiert.' : '' };
  }
  // Fork-class apps are developed locally; never offer a destructive
  // download_url overwrite. (A genuine catalog update is handled by the diff
  // branch above and is guarded for customized apps.)
  if (moduleClass === 'fork') {
    return { available: false, reason: 'Fork-Apps werden lokal weiterentwickelt.' };
  }
  // Fallback: an explicitly linked external marketplace remote (non-catalog).
  if (remote?.download_url) {
    const comparison = compareVersions(remote.version || '', item.version || '');
    if (comparison > 0) {
      return { available: true, reason: `${remote.version} ist verfügbar, lokal ist ${item.version || 'unversioniert'}.` };
    }
    return { available: false, reason: 'Kein neueres Marketplace-Release sichtbar.' };
  }
  return { available: false, reason: 'Keine Marketplace-Quelle für Updates verknüpft.' };
}

function modificationStateFor(item, release, kind, resolvedId) {
  if (kind === 'marketplace' || kind === 'template') return { status: 'catalog', label: 'Katalog' };
  const versionState = versionStateFor(resolvedId || item.module_id || item.id);
  if (versionState) {
    if (!versionState.current_bundle_sha256 || !versionState.baseline_bundle_sha256) {
      return { status: 'unknown', label: 'Modifikation unbekannt' };
    }
    if (versionState.modified) return { status: 'modified', label: 'Modifiziert' };
    return { status: 'clean', label: 'Unverändert' };
  }
  if (!release) return { status: 'unreleased', label: 'Nicht released' };
  if (!item.manifest_sha256 || !release.manifest_sha256) return { status: 'unknown', label: 'Modifikation unbekannt' };
  if (item.manifest_sha256 === release.manifest_sha256) return { status: 'clean', label: 'Unverändert' };
  return { status: 'modified', label: 'Modifiziert' };
}

function actionButtonsForManagedItem(item, permissionState = state) {
  let html = '';
  if (item.update_available) {
    html += canInstallAppStoreItem(permissionState, item)
      ? `<button type="button" class="ctox-button ctox-button--sm" data-card-action="update" aria-label="${escapeHtml(item.title)} aktualisieren">${escapeHtml(state.t('actionUpdate', 'Aktualisieren'))}</button>`
      : disabledActionButtonHtml(
        state.t('actionUpdate', 'Aktualisieren'),
        appStorePermissionDeniedReason('update'),
        item.title,
      );
  }
  if (item.app_source && item.app_source.kind === 'github' && canInstallAppStoreItem(permissionState, item)) {
    html += `<button type="button" class="ctox-button ctox-button--sm ctox-button--ghost" data-card-action="check-updates" aria-label="${escapeHtml(item.title)} nach Updates suchen">${escapeHtml(state.t('actionCheckUpdates', 'Nach Updates suchen'))}</button>`;
  }
  if (item.editable && canEditAppStoreItem(permissionState, item)) {
    html += `<button type="button" class="ctox-button ctox-button--sm" data-card-action="edit" aria-label="${escapeHtml(item.title)} bearbeiten">${escapeHtml(state.t('actionEdit', 'Bearbeiten'))}</button>`;
    if (draftFor(item.id)) {
      html += `<button type="button" class="ctox-button ctox-button--sm" data-card-action="publish" aria-label="Entwurf von ${escapeHtml(item.title)} veröffentlichen">${escapeHtml(state.t('actionPublish', 'Veröffentlichen'))}</button>`;
    }
  }
  if (isReleaseCandidateItem(item)) {
    html += canReleaseAppStoreItem(permissionState, item)
      ? `<button type="button" class="ctox-button ctox-button--sm" data-card-action="release" aria-label="${escapeHtml(item.title)} freigeben">${escapeHtml(state.t('actionRelease', 'Freigeben'))}</button>`
      : disabledActionButtonHtml(
        state.t('actionRelease', 'Freigeben'),
        appStorePermissionDeniedReason('release'),
        item.title,
      );
  }
  html += versionsButtonHtml(item);
  if (item.deletable) {
    html += canUninstallAppStoreItem(permissionState, item)
      ? `<button type="button" class="ctox-button ctox-button--sm is-danger" data-card-action="uninstall" aria-label="${escapeHtml(item.title)} deinstallieren">${escapeHtml(state.t('actionUninstall', 'Deinstallieren'))}</button>`
      : disabledActionButtonHtml(
        state.t('actionUninstall', 'Deinstallieren'),
        appStorePermissionDeniedReason('uninstall'),
        item.title,
      );
  }
  return html;
}

function disabledActionButtonHtml(label, reason, itemTitle = '') {
  const aria = `${itemTitle ? `${itemTitle}: ` : ''}${label} nicht freigegeben. ${reason}`;
  return `<button type="button" class="ctox-button ctox-button--sm denied" disabled aria-disabled="true" title="${escapeAttr(reason)}" aria-label="${escapeAttr(aria)}" data-disabled-reason="${escapeAttr(reason)}">${escapeHtml(label)}</button>`;
}

function appStorePermissionDeniedReason(action) {
  if (action === 'install' || action === 'update') {
    return 'Installieren und Aktualisieren erfordert das App-Installationsrecht.';
  }
  if (action === 'uninstall') {
    return 'Nur Owner, Admins oder Personen mit Entfernungsrecht können diese App entfernen.';
  }
  if (action === 'release') {
    return 'Veröffentlichen erfordert das Freigaberecht.';
  }
  return 'Diese Aktion ist für deine Business-OS Rolle nicht freigegeben.';
}

function isReleaseCandidateItem(item) {
  return Boolean(
    item?.id
    && (
      item.lifecycle?.runtimeInstalled
      || item.install_scope === 'installed'
      || item.raw?.install_scope === 'installed'
      || item.raw?.source === 'installed'
    )
  );
}

function releaseWizardModel(item, permissionState = state) {
  const versions = Array.isArray(item?.version_state?.versions) ? item.version_state.versions : [];
  const releaseProjection = item?.release_projection || appReleaseProjection(item?.raw || item);
  const actor = actorContext(permissionState?.ctx?.session);
  const collections = releaseDataAreaCollections(item);
  return {
    moduleId: item?.id || item?.module_id || '',
    title: item?.title || item?.id || '',
    canRelease: canReleaseAppStoreItem(permissionState, item),
    targetVersion: releaseTargetVersion(item),
    releaseChannel: releaseProjection?.status === 'restricted' ? 'restricted' : 'team',
    sourceVersionId: String(versions[0]?.version_id || ''),
    rollbackVersionId: String(releaseProjection?.rollbackVersionId || versions[1]?.version_id || ''),
    responsibleUserIds: actor.id ? [actor.id] : [],
    notes: '',
    versions,
    dataAreas: collections.map((collection) => ({
      collection,
      label: businessDataAreaLabel(collection),
    })),
    lockedStateBehavior: 'App renders a locked data state until explicit Team data grants exist.',
  };
}

function releasePayloadForWizard(item, values = {}, permissionState = state) {
  const model = releaseWizardModel(item, permissionState);
  const collections = model.dataAreas.map((area) => area.collection);
  const readCollections = normalizedSelectedCollections(values.readCollections, collections);
  const writeCollections = normalizedSelectedCollections(values.writeCollections, collections);
  const responsibleUserIds = Array.isArray(values.responsibleUserIds)
    ? values.responsibleUserIds
    : String(values.responsibleUserIds || '').split(',');
  return {
    module_id: model.moduleId,
    target_version: String(values.targetVersion || model.targetVersion).trim(),
    release_channel: String(values.releaseChannel || model.releaseChannel || 'team').trim(),
    source_version_id: String(values.sourceVersionId || model.sourceVersionId || '').trim(),
    rollback_version_id: String(values.rollbackVersionId || model.rollbackVersionId || '').trim(),
    responsible_user_ids: responsibleUserIds.map((id) => String(id || '').trim()).filter(Boolean),
    notes: String(values.notes || model.notes || '').trim(),
    data_access_review: {
      completed: true,
      status: 'completed',
      reviewed_by: actorContext(permissionState?.ctx?.session).id,
      collections,
      read_collections: readCollections,
      write_collections: writeCollections,
      locked_read_collections: collections.filter((collection) => !readCollections.includes(collection)),
      locked_write_collections: collections.filter((collection) => !writeCollections.includes(collection)),
      locked_state_behavior: model.lockedStateBehavior,
      review_is_evidence_only: true,
      grants_implied: false,
      notes: 'App Store Freigabe-Review',
    },
  };
}

function releaseTargetVersion(item) {
  const rawVersion = String(item?.lifecycle?.version || item?.version || item?.raw?.version || '').replace(/^v/i, '').trim();
  if (/^\d+\.\d+\.\d+$/.test(rawVersion)) {
    const major = Number(rawVersion.split('.')[0]);
    return major >= 1 ? rawVersion : '1.0.0';
  }
  return '1.0.0';
}

function releaseDataAreaCollections(item) {
  const declared = [
    ...(Array.isArray(item?.permissions) ? item.permissions : []),
    ...(Array.isArray(item?.raw?.collections) ? item.raw.collections : []),
  ];
  return [...new Set(declared.map((collection) => String(collection || '').trim()).filter(Boolean))];
}

function normalizedSelectedCollections(selected, allowed) {
  const allowedSet = new Set(allowed);
  return [...new Set((selected || []).map((collection) => String(collection || '').trim()).filter((collection) => allowedSet.has(collection)))];
}

function versionsButtonHtml(item) {
  const count = item.version_state?.version_count || 0;
  if (count < 1) return '';
  return `<button type="button" class="ctox-button ctox-button--sm" data-card-action="versions" aria-label="Versionen von ${escapeHtml(item.title)} anzeigen">${escapeHtml(state.t('actionVersions', 'Versionen ({count})').replace('{count}', String(count)))}</button>`;
}

function compareVersions(left, right) {
  const parse = (value) => String(value || '')
    .replace(/^v/i, '')
    .split(/[.-]/)
    .map((part) => {
      const number = Number.parseInt(part, 10);
      return Number.isFinite(number) ? number : 0;
    });
  const a = parse(left);
  const b = parse(right);
  const length = Math.max(a.length, b.length, 1);
  for (let i = 0; i < length; i += 1) {
    const diff = (a[i] || 0) - (b[i] || 0);
    if (diff !== 0) return diff;
  }
  return 0;
}

function sortItems(left, right) {
  const rank = { marketplace: 0, template: 1, installed: 2, local: 3, system: 4 };
  return (rank[left.kind] ?? 9) - (rank[right.kind] ?? 9)
    || left.title.localeCompare(right.title);
}

function scopeTitle(scope) {
  const t = state.t;
  return {
    all: t('scopeTitleAll', 'Alle Anwendungen'),
    marketplace: t('scopeTitleMarketplace', 'Marketplace'),
    template: t('scopeTitleTemplate', 'Templates'),
    installed: t('scopeTitleInstalled', 'Installierte Apps'),
    system: t('scopeTitleSystem', 'System Apps'),
    local: t('scopeTitleLocal', 'Local Modules'),
  }[scope] || t('scopeTitleFallback', 'Applications');
}

// Translate static markup: data-t (textContent), data-t-placeholder,
// data-t-title, data-t-aria (aria-label). German markup text is the fallback.
function applyTranslations(root, t) {
  root.querySelectorAll('[data-t]').forEach((el) => {
    el.textContent = t(el.dataset.t, el.textContent.trim());
  });
  root.querySelectorAll('[data-t-placeholder]').forEach((el) => {
    el.setAttribute('placeholder', t(el.dataset.tPlaceholder, el.getAttribute('placeholder') || ''));
  });
  root.querySelectorAll('[data-t-title]').forEach((el) => {
    el.setAttribute('title', t(el.dataset.tTitle, el.getAttribute('title') || ''));
  });
  root.querySelectorAll('[data-t-aria]').forEach((el) => {
    el.setAttribute('aria-label', t(el.dataset.tAria, el.getAttribute('aria-label') || ''));
  });
}

function iconGlyphForItem(item) {
  if (item.kind === 'marketplace') return item.status === 'installed' ? '✓' : '↓';
  if (item.kind === 'template') return '+';
  if (item.kind === 'installed') return '✓';
  if (item.kind === 'system') return '◆';
  return '*';
}

function iconMarkupForItem(item) {
  const svg = sanitizeSvgIcon(item?.icon_svg || item?.raw?.layout?.icon_svg || item?.raw?.icon_svg || '');
  if (svg) return svg;
  return `<span class="app-card-icon-glyph">${escapeHtml(iconGlyphForItem(item))}</span>`;
}

function sanitizeSvgIcon(raw) {
  const value = String(raw || '').trim();
  if (!value || !value.startsWith('<svg')) return '';
  try {
    const doc = new DOMParser().parseFromString(value, 'image/svg+xml');
    const parserError = doc.querySelector('parsererror');
    const svg = doc.documentElement;
    if (parserError || !svg || svg.localName !== 'svg') return '';
    for (const blocked of [...svg.querySelectorAll('script, foreignObject, iframe, object, embed, style')]) {
      blocked.remove();
    }
    for (const element of [svg, ...svg.querySelectorAll('*')]) {
      for (const attr of [...element.attributes]) {
        const name = attr.name.toLowerCase();
        const attrValue = String(attr.value || '').trim().toLowerCase();
        if (name.startsWith('on') || attrValue.startsWith('javascript:') || attrValue.includes('url(javascript:')) {
          element.removeAttribute(attr.name);
        }
      }
    }
    svg.setAttribute('aria-hidden', 'true');
    svg.setAttribute('focusable', 'false');
    return svg.outerHTML;
  } catch {
    return '';
  }
}

function statusLabel(status) {
  return {
    available: 'Available',
    installed: 'Installed',
    installing: 'Installing',
    error: 'Fehler',
    template: 'Template',
    system: 'System',
    local: 'Local',
  }[status] || status;
}

function appCountLabel(count) {
  return state.t('appsCount', '{count} Apps').replace('{count}', String(count));
}

function externalLinkIcon() {
  return '<svg class="external-link-icon" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M7 17L17 7"></path><path d="M8 7h9v9"></path></svg>';
}

// R9: Actor-Spiegel aus api.actor (id, role, permissions); entschieden wird im Core.
function actorContext(permissionState = state) {
  const actor = permissionState?.actor || {};
  const role = String(actor.role || 'user');
  return {
    id: actor.id || '',
    display_name: actor.display_name || actor.id || '',
    role,
    is_admin: role === 'admin',
  };
}

// R7: Bestätigung/Eingabe über api.shell.dialog (Form offen, OFFEN-A); ohne Antwort gilt Nein bzw. leer.
async function askConfirm(title, text, confirmLabel) {
  if (!state.api) return false;
  const answer = await state.api.shell.dialog({ kind: 'confirm', title, text, confirm_label: confirmLabel, default: false });
  return answer === true;
}

async function askText(title, defaultValue = '') {
  if (!state.api) return '';
  const answer = await state.api.shell.dialog({ kind: 'prompt', title, text: title, default: defaultValue });
  return typeof answer === 'string' ? answer.trim() : '';
}

function sanitizeId(value) {
  return String(value || '').trim().toLowerCase().replace(/[^a-z0-9_-]+/g, '-').replace(/^-+|-+$/g, '');
}

function newId() {
  if (globalThis.crypto?.randomUUID) return crypto.randomUUID();
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function escapeAttr(value) {
  return escapeHtml(value);
}

export const __appStoreTestHooks = {
  actionButtonsForManagedItem,
  appStorePermissionDeniedReason,
  buildAppStoreAgentScopeView,
  canEditAppStoreItem,
  canInstallAppStoreItem,
  canModifyAppStoreAppForModule,
  canReleaseAppStoreItem,
  canSeeAppStoreModuleForAppVersion,
  canSeeModuleForAppVersion,
  canUninstallAppStoreItem,
  appCountLabel,
  appLifecycleBadge,
  appReleaseProjection,
  buildAppStoreExport,
  chooseCanonicalCatalogItem,
  compareVersions,
  creatorHashFromStore,
  customizedUpdateBadgeHtml,
  externalLinkIcon,
  isInstalledCatalogItem,
  isTemplateEntry,
  itemHasUpdate,
  itemMatchesScope,
  newerMainRelease,
  modificationStateFor,
  originLabel,
  operationMessageHtml,
  progressButtonHtml,
  releasePayloadForWizard,
  releaseFactLinesForItem,
  releaseProjectionBadgeHtml,
  releaseWizardModel,
  sanitizeId,
  shouldUseShelf,
  statusForCard,
  statusLabel,
  updateStateFor,
  versionsButtonHtml,
};

// R9: Rechte-Spiegel (nur Darstellung, entschieden wird im Core). Wie v3 (shared/permissions.js, Rollen-Rückfall):
// Owner/Chef/Admin dürfen alles, sonst muss das Recht in api.actor.permissions stehen (ersetzt die v3-Governance-Grants;
// 4.0 liefert Rechte ohne Modul-Scope und ohne Founder-Zuweisung, V4-PORT.md OFFEN-6). App ändern zusätzlich mit
// `apps.source.edit` (§9.2, PORTING R9).
function actorHasPermission(permissionState, permission) {
  const list = permissionState?.actor?.permissions;
  return Array.isArray(list) && list.includes(permission);
}

function actorMay(permissionState, permission) {
  return roleCanManage(actorContext(permissionState).role) || actorHasPermission(permissionState, permission);
}

function canModifyAppStoreAppForModule(permissionState, item) {
  const moduleId = String(item?.id || item?.module_id || '').trim();
  if (!moduleId) return false;
  return actorMay(permissionState, BusinessOsPermissions.AppsModify) || actorHasPermission(permissionState, 'apps.source.edit');
}

function canInstallAppStoreItem(permissionState, item) {
  const moduleId = String(item?.id || item?.module_id || '').trim();
  return Boolean(moduleId) && actorMay(permissionState, BusinessOsPermissions.AppsInstall);
}

function canEditAppStoreItem(permissionState, item) {
  return canModifyAppStoreAppForModule(permissionState, item);
}

function canUninstallAppStoreItem(permissionState, item) {
  const moduleId = String(item?.id || item?.module_id || '').trim();
  return Boolean(moduleId) && actorMay(permissionState, BusinessOsPermissions.AppsUninstall);
}

function canReleaseAppStoreItem(permissionState, item) {
  const moduleId = String(item?.id || item?.module_id || '').trim();
  if (!moduleId || !isReleaseCandidateItem(item)) return false;
  return actorMay(permissionState, BusinessOsPermissions.AppsRelease);
}

function appStorePermissionOptions(permissionState) {
  const actor = actorContext(permissionState);
  return {
    session: { user: { id: actor.id, role: actor.role } },
    governance: null,
    // v3: Founder-Zuweisung/Grant `apps.view` aus der Governance; 4.0: Recht im Actor-Spiegel (ohne Modul-Scope).
    canAccessNonPublic: actorHasPermission(permissionState, BusinessOsPermissions.AppsView),
  };
}

function canModifyAppStoreContext(state, context) {
  const moduleId = String(context?.app_id || context?.record_id || 'app-store').trim();
  return canModifyAppStoreAppForModule(state, { id: moduleId || 'app-store' });
}

function buildAppStoreAgentScopeView(state, context = {}, mode = 'data') {
  const moduleId = sanitizeId(context.app_id || context.record_id || 'app-store') || 'app-store';
  const canModify = mode === 'app' && canModifyAppStoreContext(state, context);
  const dataAccess = context.data_access && typeof context.data_access === 'object'
    ? context.data_access
    : {};
  return buildGlobalCtoxAgentScopeView({
    actor: actorContext(state),
    module: {
      id: moduleId,
      module_id: moduleId,
      title: context.app_title || context.label || moduleId,
      name: context.app_title || context.label || moduleId,
      version: context.app_version || '',
    },
    lifecycle: {
      versionLabel: context.app_version || '',
      version: context.app_version || '',
      state: context.app_visibility || context.app_status || (context.app_id ? 'unknown' : 'store'),
      label: context.app_visibility_label || context.app_status || (context.app_id ? '' : 'App Store'),
      public: context.app_visibility === 'team',
      runtimeInstalled: context.app_status === 'installed' || context.app_status === 'local',
      canManage: canModify,
    },
    dataAccess: {
      ...dataAccess,
      summary: dataAccess.summary
        || context.data_access_summary
        || (context.app_id ? 'Keine Datenbereiche deklariert' : 'App Store Suche und Katalogdaten'),
      declared: dataAccess.declared || dataAccess.declared_collections || dataAccess.declaredCollectionIds || [],
      granted: dataAccess.granted || dataAccess.granted_collections || dataAccess.grantedCollectionIds || [],
      locked: dataAccess.locked || dataAccess.locked_collections || dataAccess.lockedCollectionIds || [],
      grantsImplied: dataAccess.grantsImplied === true || dataAccess.grants_implied === true,
    },
    context: {
      ...context,
      module: moduleId,
      record_id: context.record_id || moduleId,
      record_type: context.record_type || (context.app_id ? 'app' : 'store'),
      label: context.label || context.app_title || moduleId,
    },
    canModify,
    externalActions: 'none',
  });
}
