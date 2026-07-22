import { loadModuleMessages } from '../../shared/i18n.js';
import {
  appLifecycleBadge,
  appReleaseProjection,
  businessDataAreaLabel,
  canSeeModuleForAppVersion as lifecycleCanSeeModuleForAppVersion,
} from '../../shared/app-lifecycle.js?v=20260623-role-session';
import {
  BusinessOsPermissions,
  canInstallBusinessApps,
  canModifyBusinessModule,
  canUninstallBusinessApp,
  canUseBusinessPermission,
} from '../../shared/permissions.js?v=20260623-role-session';
import {
  buildGlobalCtoxAgentScopeView,
  renderGlobalCtoxAgentScopeHtml,
} from '../../shared/shell-permissions-ui.js?v=20260623-role-session';
import {
  base64ToBytes,
  sha256Hex,
  FILE_CONTENT_HASH_SCHEME,
  FILE_CHUNK_HASH_SCHEME,
} from '../../shared/file-integrity.js?v=20260603-active-chunk-query2';

const CTOX_REPO = 'metric-space-ai/ctox';
const CTOX_BRANCH = 'main';
const CTOX_APP_ROOT = 'src/apps/business-os';
const CTOX_TREE_URL = `https://api.github.com/repos/${CTOX_REPO}/git/trees/${CTOX_BRANCH}?recursive=1`;
const CTOX_RAW_ROOT = `https://raw.githubusercontent.com/${CTOX_REPO}/${CTOX_BRANCH}/${CTOX_APP_ROOT}`;
const CTOX_DOWNLOAD_URL = `https://github.com/${CTOX_REPO}/archive/refs/heads/${CTOX_BRANCH}.zip`;
const STORE_COMMAND_TIMEOUT_MS = 3 * 60 * 1000;
const DEMAND_ONLY_SYNC_COLLECTIONS = new Set([
  'desktop_file_chunks',
  'document_blob_chunks',
  'spreadsheet_blob_chunks',
]);


const state = {
  ctx: null,
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
  unsubscribe: null,
  viewMode: 'shelf',
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
function previewUrlFor(id) {
  return PREVIEW_IDS.has(id) ? new URL(`./assets/previews/${id}.png`, import.meta.url).pathname : '';
}

const els = {};

export async function mount(ctx) {
  state.ctx = ctx;
  const messages = await loadModuleMessages(import.meta.url, ctx.locale).catch(() => ({}));
  state.t = (key, fallback) => messages[key] ?? fallback ?? key;
  ctx.host.innerHTML = await loadModuleMarkup();
  applyTranslations(ctx.host, state.t);
  ensureStylesheet();
  bindElements(ctx.host);
  wireEvents();
  await Promise.all([
    ctx.sync?.startCollection?.('business_module_catalog'),
    ctx.sync?.startCollection?.('business_commands'),
  ]);
  await loadCatalog();
  applyCatalogMarketplaceState();
  state.unsubscribe = ctx.db?.collection?.('business_module_catalog')
    ?.findOne('module-catalog')
    ?.$
    ?.subscribe?.((doc) => {
      const data = doc?.toJSON?.();
      if (data) {
        state.catalog = mergeShellModulesIntoCatalog(data);
        state.marketplace = normalizeMarketplace(state.catalog.marketplace || state.catalog.apps || []);
        applyCatalogMarketplaceState();
        render();
      }
    }) || null;
  render();

  // Column resizing is owned by the shell-global resizer (setupModuleResizers
  // in app.js), wired declaratively from the `.ctox-column-resizer[data-resizer-var]`
  // handle inside the `[data-resize-frame]` root — including width persistence.

  return () => {
    try { state.unsubscribe?.unsubscribe?.(); } catch {}
  };
}

function ensureStylesheet() {
  if (document.head.querySelector('link[data-app-store-style]')) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  const styleUrl = new URL('./index.css', import.meta.url);
  // Inherit the module's own cache-buster (index.js is imported with
  // ?v=<build>): fresh JS must never render against a stale cached sheet.
  const version = String(import.meta.url).split('?v=')[1] || '20260722-app-store-grammar-v1';
  styleUrl.searchParams.set('v', version);
  link.href = styleUrl.href;
  link.dataset.appStoreStyle = 'true';
  document.head.append(link);
}

async function loadModuleMarkup() {
  // Markup inherits the JS cache-buster — like the stylesheet, a deploy must
  // never leave fresh JS binding against stale cached markup (same contract
  // as ctox/coding-agents/knowledge/threads).
  const version = String(import.meta.url).split('?v=')[1] || '20260722-app-store-grammar-v1';
  const markupHref = new URL('./index.html', import.meta.url).pathname + (version ? `?v=${version}` : '');
  const html = await fetch(markupHref).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}

function bindElements(root) {
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
  els.viewButtons = [...root.querySelectorAll('[data-pg-view]')];
  els.loading = root.querySelector('[data-loading-spinner]');
  els.loadingText = root.querySelector('[data-loading-text]');
  els.refresh = root.querySelector('[data-refresh-marketplace]');
  els.message = root.querySelector('[data-store-message]');
  els.toggleExtras = root.querySelector('[data-toggle-sidebar-extras]');
  els.sidebarExtras = root.querySelector('[data-sidebar-extras]');
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
  state.ctx.host.addEventListener('keydown', (event) => {
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

  els.refresh?.addEventListener('click', () => refreshMarketplace({ force: true }));

  els.toggleExtras?.addEventListener('click', () => {
    if (!els.sidebarExtras) return;
    const isHidden = els.sidebarExtras.toggleAttribute('hidden');
    els.toggleExtras.setAttribute('aria-pressed', isHidden ? 'false' : 'true');
  });

  state.ctx.host.querySelector('[data-action="create-scratch"]')?.addEventListener('click', () => {
    openCreatorFromStore({ mode: 'scratch' });
  });

  state.ctx.host.querySelector('[data-action="install-github"]')?.addEventListener('click', () => {
    installFromGithub();
  });

  state.ctx.host.querySelector('[data-action="install-zip"]')?.addEventListener('click', () => {
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
  // Canonical pair is cards|list; the retail-box shelf IS this app's cards
  // rendering (see the shelf section below).
  state.viewMode = detail.view === 'list' ? 'list' : 'shelf';
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
    const customized = item.modification_status === 'modified' || item.modification_status === 'customized';
    if (customized
      && !confirm(`${item.title} hat lokale Änderungen. Ein Update überschreibt sie. Vor dem Update wird automatisch eine Wiederherstellungs-Version angelegt – fortfahren?`)) {
      return;
    }
    await updateInstalledItem(item, { mode: customized ? 'discard' : 'vanilla' });
  } else if (actionType === 'check-updates') {
    await checkModuleUpdates(item);
  } else if (actionType === 'versions') {
    await openVersionsDialog(item);
  } else if (actionType === 'open') {
    if (item.id === 'create-scratch') {
      openCreatorFromStore({ mode: 'scratch' });
    } else if (item.launch_kind === 'desktop-app') {
      await state.ctx?.openDesktopApp?.(item.id);
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

async function loadCatalog() {
  const doc = await state.ctx.db?.collection?.('business_module_catalog')?.findOne('module-catalog').exec();
  state.catalog = mergeShellModulesIntoCatalog(doc?.toJSON?.() || { modules: [], templates: [], marketplace: [] });
  state.marketplace = normalizeMarketplace(state.catalog.marketplace || state.catalog.apps || []);
}

function applyCatalogMarketplaceState() {
  if (state.marketplaceStatus === 'loading') return;
  state.marketplaceStatus = state.marketplace.length ? 'ready' : 'idle';
  state.marketplaceMessage = state.marketplace.length
    ? `${state.marketplace.length} projizierte Marketplace-Module geladen`
    : 'GitHub Discovery ist bereit und startet nur manuell.';
}

function mergeShellModulesIntoCatalog(catalog) {
  const modules = Array.isArray(catalog?.modules) ? [...catalog.modules] : [];
  const known = new Set(modules.map((item) => item?.id).filter(Boolean));
  const shellModules = Array.isArray(state.ctx?.modules) ? state.ctx.modules : [];
  for (const module of shellModules) {
    if (!module?.id || known.has(module.id)) continue;
    modules.push({
      ...module,
      source: module.source || (module.core ? 'core' : 'shell'),
      install_scope: module.install_scope || (module.core ? 'core' : 'installed'),
      default_installed: module.default_installed !== false,
    });
    known.add(module.id);
  }
  return {
    ...catalog,
    modules,
    templates: Array.isArray(catalog?.templates) ? catalog.templates : [],
    marketplace: Array.isArray(catalog?.marketplace) ? catalog.marketplace : [],
  };
}

async function refreshMarketplace({ force = false } = {}) {
  if (state.marketplaceStatus === 'loading' && !force) return;
  state.marketplaceStatus = 'loading';
  state.marketplaceMessage = `Lade Module aus ${CTOX_REPO}`;
  render();
  try {
    const remote = await loadRemoteMarketplace();
    state.marketplace = mergeMarketplace(remote, normalizeMarketplace(state.catalog?.marketplace || []));
    state.marketplaceStatus = state.marketplace.length ? 'ready' : 'empty';
    state.marketplaceMessage = state.marketplace.length
      ? `${state.marketplace.length} GitHub Module gefunden`
      : `Keine Module in ${CTOX_REPO}/${CTOX_APP_ROOT}/modules gefunden.`;
  } catch (error) {
    state.marketplaceStatus = state.marketplace.length ? 'stale' : 'error';
    state.marketplaceMessage = error?.message || String(error);
  }
  render();
}

async function loadRemoteMarketplace() {
  return discoverCtoxRepoModules();
}

async function discoverCtoxRepoModules() {
  const response = await fetch(CTOX_TREE_URL, { cache: 'no-store' });
  if (!response.ok) {
    throw new Error(`CTOX GitHub discovery failed: ${response.status} ${response.statusText}`);
  }
  const data = await response.json();
  const paths = Array.isArray(data.tree) ? data.tree : [];
  const manifests = paths
    .filter((entry) => entry.type === 'blob' && /^src\/apps\/business-os\/modules\/[^/]+\/module\.json$/.test(entry.path || ''))
    .map((entry) => entry.path);
  const settled = await Promise.allSettled(manifests.map(manifestPathToMarketplaceItem));
  return settled
    .filter((result) => result.status === 'fulfilled' && result.value)
    .map((result) => result.value);
}

async function manifestPathToMarketplaceItem(path) {
  const manifestUrl = `https://raw.githubusercontent.com/${CTOX_REPO}/${CTOX_BRANCH}/${path}`;
  const manifestResponse = await fetch(manifestUrl, { cache: 'no-store' });
  if (!manifestResponse.ok) return null;
  const manifest = await manifestResponse.json();
  const moduleId = sanitizeId(manifest.id || path.split('/').at(-2));
  if (!moduleId || String(manifest.install_scope || '').trim().toLowerCase() !== 'store') return null;
  const repoSourcePath = path.replace('/module.json', '');
  return normalizeMarketplaceItem({
    id: moduleId,
    module_id: moduleId,
    title: manifest.title || moduleId,
    description: manifest.description || '',
    category: manifest.category || 'CTOX',
    version: manifest.version || CTOX_BRANCH,
    developer: manifest.developer || 'CTOX',
    license: manifest.license || 'AGPL-3.0-only',
    repo: CTOX_REPO,
    source: 'ctox-github',
    source_path: repoSourcePath,
    manifest_url: manifestUrl,
    download_url: CTOX_DOWNLOAD_URL,
    homepage: `https://github.com/${CTOX_REPO}/tree/${CTOX_BRANCH}/${path.replace('/module.json', '')}`,
    permissions: manifest.collections || [],
    install_scope: 'store',
    store: manifest.store || {},
    installable: manifest.store?.installable !== false,
    updated_at: '',
  });
}

function catalogItems() {
  return uniqueCatalogItems(rawCatalogItems()).sort(sortItems);
}

function rawCatalogItems() {
  const modules = Array.isArray(state.catalog?.modules) ? state.catalog.modules : [];
  const templates = Array.isArray(state.catalog?.templates) ? state.catalog.templates : [];
  const moduleIds = new Set(modules.map((item) => item?.id).filter(Boolean));
  const desktopApps = Array.isArray(state.ctx?.desktopApps) ? state.ctx.desktopApps : [];

  const scratchTemplate = {
    id: 'create-scratch',
    module_id: 'create-scratch',
    title: 'Neue App per KI-Prompt erstellen',
    description: 'Erstelle eine völlig freie, maßgeschneiderte App über einen einzigen deutschen Prompt.',
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
    ...desktopApps
      .filter((item) => item?.id && !moduleIds.has(item.id))
      .map(normalizeDesktopAppItem),
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

function normalizeMarketplace(items) {
  return items.map(normalizeMarketplaceItem).filter(Boolean);
}

function normalizeMarketplaceItem(item) {
  const id = sanitizeId(item.module_id || item.id || item.name || '');
  const downloadUrl = item.download_url || item.archive_url || item.zipball_url || '';
  if (!id || !downloadUrl) return null;
  return {
    id,
    module_id: id,
    title: item.title || item.name || id,
    description: item.description || '',
    category: String(item.category || 'Marketplace'),
    version: item.version || item.release || 'latest',
    developer: item.developer || item.publisher || item.owner || repoOwner(item.repo) || 'GitHub',
    license: item.license || 'unknown',
    source: item.source || 'github',
    repo: item.repo || item.repository || '',
    download_url: downloadUrl,
    source_path: item.source_path || '',
    manifest_url: item.manifest_url || '',
    homepage: item.homepage || item.html_url || '',
    icon_svg: item.layout?.icon_svg || item.icon_svg || '',
    install_scope: item.install_scope || item.store?.install_scope || '',
    permissions: Array.isArray(item.permissions) ? item.permissions : (Array.isArray(item.collections) ? item.collections : []),
    installable: item.installable !== false && item.store?.installable !== false,
    raw: item,
  };
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
    category: String(item.category || item.source || (item.core ? 'System' : 'Local')),
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

function normalizeDesktopAppItem(item) {
  return {
    id: sanitizeId(item.id || ''),
    kind: 'local',
    status: 'local',
    launch_kind: 'desktop-app',
    title: item.title || item.id,
    description: 'Packaged desktop utility available from the Business OS launcher.',
    category: 'Desktop Apps',
    version: 'v1',
    developer: 'CTOX',
    license: 'AGPL-3.0-only',
    source: 'desktop-app',
    repo: '',
    download_url: '',
    source_path: '',
    manifest_url: '',
    homepage: '',
    icon_svg: item.layout?.icon_svg || item.icon_svg || '',
    install_scope: '',
    permissions: [],
    installable: false,
    module_class: 'maintained',
    editable: false,
    deletable: false,
    manifest_sha256: '',
    local_manifest_path: '',
    installed_version: 'Installiert: Desktop',
    available_version: 'Katalog: lokal',
    update_available: false,
    update_reason: '',
    modification_status: 'clean',
    modification_label: 'Unverändert',
    version_state: null,
    latest_release: null,
    raw: item,
  };
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
  // The standing sync-state line is gone; the compact discovery signal folds
  // into this one footer line (full state text rides on the refresh icon's
  // tooltip via marketplaceStateLabel).
  const marketplaceSuffix = state.scope === 'marketplace'
    && ['ready', 'stale'].includes(state.marketplaceStatus)
    && state.marketplace.length
    ? ` · ${state.marketplace.length} GitHub`
    : '';
  const footerText = `${appCountLabel(items.length, state.scope, state.marketplaceStatus)}${marketplaceSuffix}`;
  if (pg?.setFooter) pg.setFooter(footerText);
  else {
    const node = els.centerPane?.querySelector('[data-pg-footer]');
    if (node) node.textContent = footerText;
  }
  const leftFooter = els.leftPane?.querySelector('[data-pg-footer]');
  if (leftFooter) leftFooter.textContent = `${catalogItems().length} Apps insgesamt`;
}

// Updates band: installed apps whose catalog counterpart advertises a newer
// version (plus anything the lifecycle projection already flags).
function itemHasUpdate(item) {
  if (!isInstalledCatalogItem(item)) return false;
  if (item.update_available === true || item.lifecycle?.updateAvailable === true) return true;
  const installed = String(item.installed_version || '').replace(/^v/, '');
  const available = String(item.available_version || '').replace(/^v/, '');
  return Boolean(installed && available && installed !== '-' && available !== '-' && installed !== available);
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

  const shelfMode = state.viewMode === 'shelf' && !state.shelfUnavailable;
  // Data re-renders never move the operator: preserve the well's scroll
  // offset across the list rebuild (intentional resets — search/view/band/
  // filter/scope — pass resetScroll because the content set changed). The
  // shell scroll guard backs this up; the reset lands after its restore.
  const scrollTop = resetScroll ? 0 : (els.well?.scrollTop || 0);
  if (els.shelfStage) els.shelfStage.hidden = !shelfMode;
  if (els.grid) {
    els.grid.hidden = shelfMode;
    if (!shelfMode) {
      els.grid.className = 'store-card-grid is-list-view';
      els.grid.replaceChildren(...renderCatalogBody(items));
    }
  }
  if (els.well && !shelfMode) {
    if (resetScroll) requestAnimationFrame(() => { els.well.scrollTop = 0; });
    else els.well.scrollTop = scrollTop;
  }
  if (shelfMode) syncShelf(items);

  // Mirror the shell-wired view toggle for programmatic state (shelf
  // fallback forces list mode and locks the cards/shelf button).
  for (const btn of els.viewButtons || []) {
    const active = btn.dataset.pgView === (shelfMode ? 'cards' : 'list');
    btn.classList.toggle('is-active', active);
    btn.setAttribute('aria-pressed', active ? 'true' : 'false');
    if (btn.dataset.pgView === 'cards') btn.disabled = state.shelfUnavailable;
  }

  if (state.selectedId && !items.some((item) => item.id === state.selectedId)) {
    state.selectedId = '';
    state.drawerOpen = false;
  }
  renderDetails();
}

// ---------------------------------------------------------------------------
// Retail-box shelf (vendor/store-shelf): the "cards" rendering of this app.
// Load lazily; a WebGL/import failure falls back to the list rendering.
// ---------------------------------------------------------------------------
async function ensureShelf() {
  if (state.shelf || state.shelfUnavailable) return state.shelf;
  if (!els.shelfCanvas || !els.shelfStage || !els.shelfScroll || !els.shelfTrack) return null;
  // Single-flight: every render calls this; concurrent imports must never
  // create competing shelf instances on the same canvas (the empty last one
  // would own the WebGL context and the boxes would never show).
  if (state.shelfPromise) return state.shelfPromise;
  state.shelfPromise = buildShelf();
  return state.shelfPromise;
}

async function buildShelf() {
  try {
    const mod = await import('../../vendor/store-shelf/store-shelf.mjs');
    state.shelf = mod.createStoreShelf(els.shelfCanvas, {
      apps: [],
      locale: (state.ctx?.locale || 'de').startsWith('en') ? 'en' : 'de',
      scrollContainer: els.shelfScroll,
      track: els.shelfTrack,
      stage: els.shelfStage,
      onSelect: (id) => {
        state.selectedId = id;
        state.drawerOpen = true;
        render();
      },
    });
  } catch (err) {
    console.warn('[app-store] shelf unavailable, falling back to list', err);
    state.shelfUnavailable = true;
    state.viewMode = 'list';
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
    iconUrl: locallyServed ? new URL(`../${item.id}/icon.svg`, import.meta.url).pathname : '',
    screenshots: preview ? [preview] : [],
  };
}

async function syncShelf(items) {
  const shelf = await ensureShelf();
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
        setTimeout(() => { state.shelfRetryPending = false; render(); }, 600);
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

function renderCatalogBody(items) {
  if (items.length) return items.map(renderCard);
  return [renderEmptyCatalogState({
    title: emptyCatalogTitle(state.scope, state.query, state.marketplaceStatus),
    body: emptyCatalogBody(state.scope, state.query, state.marketplaceStatus, state.marketplaceMessage),
  })];
}

function renderEmptyCatalogState({ title, body }) {
  const empty = document.createElement('section');
  empty.className = 'ctox-empty store-empty-state';
  empty.setAttribute('role', 'status');
  empty.innerHTML = `
    <strong>${escapeHtml(title)}</strong>
    <span>${escapeHtml(body)}</span>
  `;
  return empty;
}

function cardActionsHtml(item, operation, cardStatus, { includeDetails = true } = {}) {
  let actionsHtml = `<div class="app-card-actions">`;

  if (operation?.kind === 'running') {
    actionsHtml += progressButtonHtml(operation.text || `${item.title} wird installiert...`);
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

function renderCard(item) {
  const operation = operationForItem(item);
  const cardStatus = statusForCard(item, operation);
  const card = document.createElement('article');
  card.className = 'app-card';
  card.dataset.appId = item.id;
  card.dataset.contextRecordId = item.id;
  card.dataset.contextRecordType = 'business_app';
  card.dataset.contextLabel = item.title || item.id;
  if (operation?.kind) card.dataset.operation = operation.kind;
  card.classList.toggle('active', item.id === state.selectedId);
  card.classList.toggle('is-operating', operation?.kind === 'running');
  card.tabIndex = 0;
  card.setAttribute('aria-selected', item.id === state.selectedId ? 'true' : 'false');
  card.setAttribute('aria-label', `${item.title}. ${statusLabel(cardStatus)}. ${item.category}.`);

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
        <span class="app-card-category">${escapeHtml(item.category)}</span>
      </div>
    </div>
    <p class="app-card-desc">${escapeHtml(item.description || item.source)}</p>
    <div class="app-card-version-row">
      <span class="ctox-badge">${escapeHtml(item.installed_version)}</span>
      <span class="ctox-badge">${escapeHtml(item.available_version)}</span>
      ${item.lifecycle?.runtimeInstalled ? `<span class="ctox-badge ${lifecycleStateBadgeClass(item.lifecycle.state)}" data-state="${escapeHtml(item.lifecycle.state)}" title="${escapeAttr(item.lifecycle.title)}">${escapeHtml(item.lifecycle.version)} · ${escapeHtml(item.lifecycle.text)}</span>` : ''}
      ${releaseProjectionBadgeHtml(item)}
      <span class="ctox-badge ${modStateBadgeClass(item.modification_status)}">${escapeHtml(item.modification_label)}</span>
      ${externalSourceBadgeHtml(item)}
    </div>
    ${actionsHtml}
    ${operationHtml}
    <footer class="app-card-footer">
      <span class="ctox-badge ${statusBadgeClass(cardStatus)}">${escapeHtml(statusLabel(cardStatus))}</span>
      <span class="app-card-source">${escapeHtml(sourceShort(item))}</span>
    </footer>
  `;
  return card;
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

function operationMessageHtml(operation) {
  if (!operation?.text) return '';
  const kind = operation.kind || 'running';
  return `<div class="app-card-operation" data-kind="${escapeHtml(kind)}" role="status">${escapeHtml(operation.text)}</div>`;
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
  if (els.detailIcon) els.detailIcon.textContent = '?';
  if (els.detailTitle) els.detailTitle.textContent = state.t('drawerNoSelection', 'Keine App ausgewählt');
  if (els.detailVersion) els.detailVersion.textContent = '-';
  if (els.detailCategory) els.detailCategory.textContent = 'Empty';
  if (els.detailDeveloper) els.detailDeveloper.textContent = '-';
  if (els.detailLicense) els.detailLicense.textContent = '-';
  if (els.detailSource) els.detailSource.textContent = 'App Store';
  if (els.detailStatus) els.detailStatus.textContent = state.t('statusNoSelection', 'No selection');
  if (els.readme) {
    const empty = document.createElement('p');
    empty.className = 'store-detail-empty';
    empty.textContent = state.t('emptyDetails', 'Wähle eine App oder ändere den Filter, um Details zu sehen.');
    els.readme.replaceChildren(empty);
  }
}

function renderDocumentation(item) {
  const wrap = document.createElement('div');
  const releaseFacts = releaseFactLinesForItem(item);
  const lines = [
    item.description || 'No documentation available yet.',
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

async function installMarketplaceItem(item, { update = false } = {}) {
  if (!canInstallAppStoreItem(state, item)) {
    state.status = { kind: 'error', text: 'Du darfst diese App nicht installieren oder aktualisieren.' };
    render();
    return;
  }
  await runStoreCommand({
    label: update ? `Updating ${item.title}...` : `Installing ${item.title}...`,
    success: update ? `${item.title} updated.` : `${item.title} installed.`,
    commandType: 'ctox.app_store.install',
    moduleId: item.id,
    payload: {
      module_id: item.id,
      source_kind: item.repo ? 'github' : 'url',
      repo: item.repo || '',
      git_ref: item.repo ? CTOX_BRANCH : '',
      subpath: item.repo ? item.source_path : '',
      download_url: item.download_url,
      source_path: item.source_path,
      manifest_url: item.manifest_url,
    },
  });
}

async function installFromGithub() {
  if (!canInstallBusinessApps(appStorePermissionOptions(state))) {
    state.status = { kind: 'error', text: 'Du darfst keine Apps installieren.' };
    render();
    return;
  }
  const repo = (window.prompt('GitHub-Repository (owner/name):', '') || '').trim();
  if (!repo) return;
  if (!/^[\w.-]+\/[\w.-]+$/.test(repo)) {
    state.status = { kind: 'error', text: `Ungültiges Repository: ${repo} (erwartet owner/name).` };
    render();
    return;
  }
  const gitRef = (window.prompt('Branch / Tag / Commit (leer = HEAD):', 'main') || '').trim();
  const subpath = (window.prompt('Pfad zum Modul im Repo (leer = Wurzel):', '') || '').trim();
  const moduleId = sanitizeId((window.prompt('Modul-ID (muss zur module.json im Repo passen):', '') || '').trim());
  if (!moduleId) {
    state.status = { kind: 'error', text: 'Modul-ID ist erforderlich.' };
    render();
    return;
  }
  if (!window.confirm(`Aus EXTERNER Quelle installieren?\n\nRepo: ${repo}\nRef: ${gitRef || 'HEAD'}\nModul: ${moduleId}\n\nExterne Apps sind zunächst NICHT verifiziert und erhalten keine Datenrechte bis zur Prüfung.`)) {
    return;
  }
  await runStoreCommand({
    label: `Installiere ${moduleId} aus GitHub...`,
    success: `${moduleId} installiert (externe Quelle – nicht verifiziert).`,
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

function bytesToBase64(bytes) {
  let binary = '';
  const block = 0x8000;
  for (let i = 0; i < bytes.length; i += block) {
    binary += String.fromCharCode.apply(null, bytes.subarray(i, i + block));
  }
  return btoa(binary);
}

// Write an uploaded .zip into the desktop file/chunk store over the RxDB data
// plane (no HTTP). Mirrors the proven chat-attachment chunk format so the native
// verified-decode accepts it; the native installer reads it back by file_id.
async function uploadZipToChunkStore(file) {
  const db = state.ctx?.db;
  const syncHandles = await startScopedSyncCollections(
    ['desktop_files', 'desktop_file_chunks'],
    'app-store-zip-upload',
  );
  const filesColl = db?.collection?.('desktop_files');
  const chunksColl = db?.collection?.('desktop_file_chunks');
  if (!filesColl || !chunksColl) throw new Error('Datenbank nicht verfügbar.');
  try {
    const bytes = new Uint8Array(await file.arrayBuffer());
    const base64 = bytesToBase64(bytes);
    const CHUNK = 16 * 1024;
    const total = Math.max(1, Math.ceil(base64.length / CHUNK));
    const now = Date.now();
    const contentHash = await sha256Hex(base64ToBytes(base64));
    const fileId = `appzip_${now}_${Math.random().toString(36).slice(2, 10)}`;
    const generationId = `gen_${now}_${contentHash.slice(0, 12)}`;
    const chunkRows = await Promise.all(Array.from({ length: total }, async (_, idx) => {
      const data = base64.slice(idx * CHUNK, (idx + 1) * CHUNK);
      return {
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
      };
    }));
    await writeChunkDocuments(chunksColl, chunkRows);
    const virtualPath = `/app-store-uploads/${file.name || 'app.zip'}`;
    await filesColl.upsert({
      id: fileId,
      parent_id: '',
      path: virtualPath,
      local_path: '',
      virtual_path: virtualPath,
      name: file.name || 'app.zip',
      kind: 'file',
      mime_type: file.type || 'application/zip',
      extension: 'zip',
      size_bytes: bytes.length,
      owner_id: '',
      source: 'app-store-upload',
      content_ref: fileId,
      content_state: 'available',
      content_hash: contentHash,
      content_hash_scheme: FILE_CONTENT_HASH_SCHEME,
      content_generation_id: generationId,
      content_synced_at_ms: now,
      sort_index: now,
      is_deleted: false,
      created_at_ms: now,
      updated_at_ms: now,
    });
    await flushScopedSyncCollections(syncHandles);
    return fileId;
  } finally {
    await releaseSyncLeases(syncHandles.leases);
  }
}

async function writeChunkDocuments(collection, rows) {
  const docs = Array.isArray(rows) ? rows.filter(Boolean) : [];
  if (!docs.length) return;
  if (typeof collection.bulkUpsert === 'function') {
    await collection.bulkUpsert(docs);
    return;
  }
  for (const doc of docs) {
    await collection.upsert(doc);
  }
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
    document.body.appendChild(input);
    input.click();
  });
}

async function installFromZip() {
  if (!canInstallBusinessApps(appStorePermissionOptions(state))) {
    state.status = { kind: 'error', text: 'Du darfst keine Apps installieren.' };
    render();
    return;
  }
  const file = await pickZipFile();
  if (!file) return;
  const moduleId = sanitizeId((window.prompt('Modul-ID (muss zur module.json im Zip passen):', '') || '').trim());
  if (!moduleId) {
    state.status = { kind: 'error', text: 'Modul-ID ist erforderlich.' };
    render();
    return;
  }
  const subpath = (window.prompt('Pfad zum Modul im Zip (leer = Wurzel):', '') || '').trim();
  if (!window.confirm(`App aus EXTERNER Zip-Datei installieren?\n\nDatei: ${file.name}\nModul: ${moduleId}\n\nExterne Apps sind zunächst NICHT verifiziert und erhalten keine Datenrechte bis zur Prüfung.`)) {
    return;
  }
  let fileId;
  try {
    state.status = { kind: 'info', text: `Lade ${file.name} hoch...` };
    render();
    fileId = await uploadZipToChunkStore(file);
    // Give the chunk store a moment to replicate to the native peer before the
    // install command tries to read it back.
    await new Promise((resolve) => setTimeout(resolve, 1200));
  } catch (err) {
    state.status = { kind: 'error', text: `Upload fehlgeschlagen: ${err?.message || err}` };
    render();
    return;
  }
  await runStoreCommand({
    label: `Installiere ${moduleId} aus Zip...`,
    success: `${moduleId} installiert (externe Quelle – nicht verifiziert).`,
    commandType: 'ctox.app_store.install',
    moduleId,
    payload: {
      module_id: moduleId,
      source_kind: 'zip',
      file_id: fileId,
      subpath,
    },
  });
}

async function checkModuleUpdates(item) {
  await runStoreCommand({
    label: `Suche Updates für ${item.title}...`,
    success: `Update-Prüfung für ${item.title} abgeschlossen.`,
    commandType: 'ctox.module.check_updates',
    moduleId: item.id,
    payload: { module_id: item.id },
  });
}

async function updateInstalledItem(item, { mode = 'vanilla' } = {}) {
  if (!canInstallAppStoreItem(state, item)) {
    state.status = { kind: 'error', text: 'Du darfst diese App nicht aktualisieren.' };
    render();
    return;
  }
  // GitHub-sourced apps update by re-installing from their pinned source.
  const src = item.app_source;
  if (src && src.kind === 'github' && src.repo) {
    await runStoreCommand({
      label: `Aktualisiere ${item.title} aus GitHub...`,
      success: `${item.title} aus GitHub aktualisiert.`,
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
  const baseline = item.version_state?.installed_from_bundle_sha256
    || item.version_state?.baseline_bundle_sha256
    || '';
  await runStoreCommand({
    label: `Updating ${item.title}...`,
    success: `${item.title} updated.`,
    commandType: 'ctox.module.update',
    moduleId: item.id,
    payload: {
      module_id: item.id,
      mode,
      expected_baseline_sha256: baseline,
    },
  });
}

async function uninstallInstalledItem(item) {
  if (!canUninstallAppStoreItem(state, item)) {
    state.status = { kind: 'error', text: 'Du darfst diese App nicht entfernen.' };
    render();
    return;
  }
  if (!confirm(`Uninstall ${item.title}? Local source files will be removed from installed-modules.`)) return;
  await runStoreCommand({
    label: `Uninstalling ${item.title}...`,
    success: `${item.title} uninstalled.`,
    commandType: 'ctox.app_store.uninstall',
    moduleId: item.id,
    payload: {
      module_id: item.id,
    },
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
    state.status = { kind: 'error', text: `Keine Versionen für ${item.title} vorhanden.` };
    render();
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
        <button type="button" class="ctox-pane-icon" data-version-close aria-label="Schließen" title="Schließen">${state.ctx?.getActionIcon?.('close') || ''}</button>
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
    if (!confirm(`${item.title} auf diese Version zurücksetzen? Die aktuelle Quelle wird vorher als Wiederherstellungs-Version gesichert.`)) return;
    close();
    await rollbackToVersion(item, versionId);
  });

  window.addEventListener('keydown', onEscape);
  document.body.append(overlay);
}

async function rollbackToVersion(item, versionId) {
  await runStoreCommand({
    label: `${item.title} wird zurückgesetzt...`,
    success: `${item.title} auf die gewählte Version zurückgesetzt.`,
    commandType: 'ctox.module.rollback_version',
    moduleId: item.id,
    payload: {
      module_id: item.id,
      version_id: versionId,
    },
  });
}

async function openReleaseDialog(item) {
  const model = releaseWizardModel(item, state);
  const overlay = document.createElement('div');
  overlay.className = 'ctox-modal';
  const dataRows = model.dataAreas.length
    ? model.dataAreas.map((area) => `
      <li class="app-release-data-row">
        <div>
          <strong>${escapeHtml(area.label)}</strong>
          <span>${escapeHtml(area.collection)}</span>
        </div>
        <label>
          <input type="checkbox" name="read_collections" value="${escapeAttr(area.collection)}">
          Lesen freigegeben
        </label>
        <label>
          <input type="checkbox" name="write_collections" value="${escapeAttr(area.collection)}">
          Schreiben freigegeben
        </label>
      </li>`).join('')
    : '<li class="app-release-data-row is-empty">Keine Datenbereiche im Manifest deklariert.</li>';
  const sourceOptions = releaseVersionOptionsHtml(model.versions, model.sourceVersionId, 'Aktuelle Quelle');
  const rollbackOptions = releaseVersionOptionsHtml(model.versions, model.rollbackVersionId, 'Kein Rollback-Ziel');
  overlay.innerHTML = `
    <form class="ctox-modal-card ctox-modal-card--wide app-release-dialog" role="dialog" aria-modal="true" aria-label="Freigabe von ${escapeAttr(item.title)}">
      <header class="ctox-modal-header">
        <h3 class="ctox-modal-title">Freigabe vorbereiten - ${escapeHtml(item.title)}</h3>
        <button type="button" class="ctox-pane-icon" data-release-close aria-label="Schließen" title="Schließen">${state.ctx?.getActionIcon?.('close') || ''}</button>
      </header>
      <div class="ctox-modal-body app-release-form">
        <label>
          <span class="ctox-field-label">Zielversion</span>
          <input class="ctox-input" name="target_version" value="${escapeAttr(model.targetVersion)}" required pattern="\\d+\\.\\d+\\.\\d+">
        </label>
        <label>
          <span class="ctox-field-label">Sichtbarkeit nach Freigabe</span>
          <select class="ctox-select" name="release_channel">
            <option value="team" ${model.releaseChannel === 'team' ? 'selected' : ''}>Team</option>
            <option value="restricted" ${model.releaseChannel === 'restricted' ? 'selected' : ''}>Eingeschränkt</option>
          </select>
        </label>
        <label>
          <span class="ctox-field-label">Quell-Snapshot</span>
          <select class="ctox-select" name="source_version_id">${sourceOptions}</select>
        </label>
        <label>
          <span class="ctox-field-label">Rollback-Ziel</span>
          <select class="ctox-select" name="rollback_version_id">${rollbackOptions}</select>
        </label>
        <label>
          <span class="ctox-field-label">App-Verantwortliche</span>
          <input class="ctox-input" name="responsible_user_ids" value="${escapeAttr(model.responsibleUserIds.join(', '))}" placeholder="user-id, user-id">
        </label>
        <label>
          <span class="ctox-field-label">Release-Notiz</span>
          <textarea class="ctox-textarea" name="notes" rows="3" placeholder="Was wird für das Team freigegeben?">${escapeHtml(model.notes)}</textarea>
        </label>
        <section class="app-release-data-review" aria-label="Datenzugriff Review">
          <h4>Datenzugriff Review</h4>
          <p>Datenrechte werden hier nur geprüft und dokumentiert. Fehlende Team-Rechte bleiben als gesperrte Datenbereiche sichtbar.</p>
          <ul>${dataRows}</ul>
        </section>
      </div>
      <footer class="ctox-modal-footer">
        <button type="button" class="ctox-button" data-release-close>Abbrechen</button>
        <button type="submit" class="ctox-button is-primary">Freigabe senden</button>
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
  document.body.append(overlay);
}

async function moduleBundleVersionsFor(moduleId, fallbackState = null, item = null) {
  const fallbackVersions = Array.isArray(fallbackState?.versions) ? fallbackState.versions : [];
  if (fallbackVersions.length) return fallbackVersions;
  const expectedCount = Number(fallbackState?.version_count || 0);
  const deadline = expectedCount > 0 ? Date.now() + 15_000 : Date.now();
  let lastVersions = [];
  do {
    try {
      const doc = await state.ctx?.db?.collection?.('business_module_catalog')?.findOne('module-catalog').exec();
      const data = doc?.toJSON?.();
      const versions = data?.version_states?.[moduleId]?.versions;
      if (Array.isArray(versions)) {
        lastVersions = versions;
        if (versions.length || expectedCount === 0) return versions;
      }
    } catch {
      // Runtime-installed app schema changes can briefly restart the native
      // peer. If the catalog says versions exist, keep the UI action pending
      // for a short bounded window instead of showing a false empty state.
    }
    if (Date.now() < deadline) await delay(250);
  } while (Date.now() < deadline);
  return lastVersions.length ? lastVersions : versionSummariesFromLifecycle(item);
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

function releaseVersionOptionsHtml(versions, selectedId, emptyLabel) {
  const options = [`<option value="">${escapeHtml(emptyLabel)}</option>`];
  for (const version of versions) {
    const id = String(version?.version_id || '');
    if (!id) continue;
    const label = [
      version.label || originLabel(version.origin),
      version.seq ? `#${version.seq}` : '',
      version.created_at_ms ? new Date(version.created_at_ms).toLocaleString() : '',
    ].filter(Boolean).join(' · ');
    options.push(`<option value="${escapeAttr(id)}" ${id === selectedId ? 'selected' : ''}>${escapeHtml(label || id)}</option>`);
  }
  return options.join('');
}

async function releaseModule(item, payload) {
  await runStoreCommand({
    label: `${item.title} wird freigegeben...`,
    success: `${item.title} wurde zur Team-Version freigegeben.`,
    commandType: 'ctox.module.release',
    moduleId: item.id,
    payload,
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

async function runStoreCommand({ label, success, commandType, moduleId, payload }) {
  setOperation(moduleId, {
    kind: 'running',
    text: label,
    commandType,
    startedAt: Date.now(),
  });
  setBusy(true, label);
  try {
    const commandId = `cmd_${newId()}`;
    const result = await state.ctx.commandBus.dispatch({
      id: commandId,
      wait_timeout_ms: STORE_COMMAND_TIMEOUT_MS,
      module: 'app-store',
      type: commandType,
      record_id: moduleId,
      inbound_channel: 'business_os.app_store',
      payload,
      client_context: {
        source: 'business-os-app-store',
        module_id: moduleId,
        command_wait_timeout_ms: STORE_COMMAND_TIMEOUT_MS,
        actor: actorContext(state.ctx.session),
      },
    }, { until: 'accepted', timeoutMs: STORE_COMMAND_TIMEOUT_MS });
    await loadCatalog();
    if ([
      'ctox.app_store.install',
      'ctox.app_store.uninstall',
      'ctox.module.install_template',
      'ctox.module.update',
      'ctox.module.rollback_version',
      'ctox.module.release',
    ].includes(commandType)) {
      window.dispatchEvent(new CustomEvent('ctox-business-os-modules-changed', {
        detail: {
          source: 'app-store',
          command_type: commandType,
          module_id: moduleId,
        },
      }));
    }
    setOperation(moduleId, {
      kind: 'success',
      text: success,
      commandType,
      result,
      completedAt: Date.now(),
    });
    state.status = { kind: 'success', text: success, result };
    render();
  } catch (error) {
    const text = error?.message || String(error);
    setOperation(moduleId, {
      kind: 'error',
      text,
      commandType,
      completedAt: Date.now(),
    });
    state.status = { kind: 'error', text };
    render();
  } finally {
    setBusy(false);
  }
}

function setBusy(busy, text = '') {
  state.busy = busy;
  const showLoading = busy || state.marketplaceStatus === 'loading';
  if (els.loading) els.loading.hidden = !showLoading;
  if (els.loadingText) {
    els.loadingText.textContent = text || state.marketplaceMessage || 'GitHub Discovery wird synchronisiert.';
  }
  if (els.refresh) els.refresh.disabled = busy || state.marketplaceStatus === 'loading';
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

function availableMarketplaceCount() {
  const installed = installedIds();
  return state.marketplace.filter((item) => !installed.has(item.module_id || item.id)).length;
}

function renderMarketplaceState() {
  const counts = countsByScope();
  // No standing sync-state line: the full discovery state rides on the
  // refresh header icon's tooltip (marketplaceStateLabel), the compact
  // signal folds into the center pane footer, and progress surfaces through
  // the loading overlay while a sync actually runs.
  if (els.refresh) {
    const refreshBusy = state.marketplaceStatus === 'loading' || state.busy;
    els.refresh.disabled = refreshBusy;
    els.refresh.title = marketplaceStateLabel({
      status: state.marketplaceStatus,
      message: state.marketplaceMessage,
      discoveredCount: state.marketplace.length,
      availableCount: availableMarketplaceCount(),
      installedCount: counts.installed,
    });
  }
  const showLoading = state.marketplaceStatus === 'loading' || state.busy;
  if (els.loading) els.loading.hidden = !showLoading;
  if (els.loadingText && showLoading) {
    els.loadingText.textContent = state.marketplaceMessage || 'GitHub Discovery wird synchronisiert.';
  }
  if (state.ctx?.host) {
    const root = state.ctx.host.querySelector('[data-app-store-root]');
    root?.toggleAttribute('aria-busy', showLoading);
  }
}

function renderMessage() {
  if (!els.message) return;
  if (!state.status) {
    els.message.hidden = true;
    els.message.textContent = '';
    return;
  }
  els.message.hidden = false;
  els.message.textContent = state.status.text;
  els.message.dataset.kind = state.status.kind;
  // Kit callout state (shared/base.css §15b); data-kind stays as the data hook.
  els.message.classList.toggle('is-danger', state.status.kind === 'error');
  els.message.classList.toggle('is-success', state.status.kind !== 'error');
}

function openModule(moduleId) {
  if (!moduleId) return;
  window.location.hash = moduleId;
}

function openCreatorFromStore({ mode = 'scratch', upgrade = '' } = {}) {
  try {
    sessionStorage.setItem('ctox.app-store.creatorReturnContext', JSON.stringify({
      source: 'app-store',
      return_hash: '#app-store',
      mode,
      upgrade,
      created_at: new Date().toISOString(),
    }));
  } catch {}
  // The creator left the module catalog (the importer took its slot); the
  // create-from-scratch surface lives on as the windowed desktop app.
  const openApp = state.ctx?.openApp || state.ctx?.openDesktopApp;
  if (typeof openApp === 'function') {
    void openApp('creator', { args: { source: 'app-store', mode, upgrade } });
    return;
  }
  openModule(creatorHashFromStore({ mode, upgrade }));
}

function creatorHashFromStore({ mode = 'scratch', upgrade = '' } = {}) {
  const params = new URLSearchParams({
    source: 'app-store',
    return: 'app-store',
    mode,
  });
  if (upgrade) params.set('upgrade', upgrade);
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
    return { available: false, reason: 'Fork-Apps werden lokal weiterentwickelt. Für Upstream-Patches einen Agent beauftragen oder neu forken.' };
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
  const aria = `${itemTitle ? `${itemTitle}: ` : ''}${label} nicht verfügbar. ${reason}`;
  return `<button type="button" class="ctox-button ctox-button--sm denied" disabled aria-disabled="true" title="${escapeAttr(reason)}" aria-label="${escapeAttr(aria)}" data-disabled-reason="${escapeAttr(reason)}">${escapeHtml(label)}</button>`;
}

function appStorePermissionDeniedReason(action) {
  if (action === 'install' || action === 'update') {
    return 'Nur Owner, Admins oder Personen mit App-Installationsrecht können Apps installieren oder aktualisieren.';
  }
  if (action === 'uninstall') {
    return 'Nur Owner, Admins oder Personen mit Entfernungsrecht können diese App entfernen.';
  }
  if (action === 'release') {
    return 'Nur Owner, Admins oder Personen mit Freigaberecht können eine Team-Version veröffentlichen.';
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

function mergeMarketplace(primary, fallback) {
  const map = new Map();
  for (const item of [...fallback, ...primary]) {
    map.set(item.id, item);
  }
  return [...map.values()].sort((left, right) => left.title.localeCompare(right.title));
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
    marketplace: t('scopeTitleMarketplace', 'GitHub Marketplace'),
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
  if (item.kind === 'marketplace') return item.status === 'installed' ? '✓' : 'GH';
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

function appCountLabel(count, scope, marketplaceStatus) {
  const label = state.t('appsCount', '{count} Apps').replace('{count}', String(count));
  if (scope === 'marketplace' && marketplaceStatus === 'loading') {
    return `${label} · Sync`;
  }
  return label;
}

function marketplaceStateLabel({
  status,
  message,
  marketplaceCount = 0,
  discoveredCount = marketplaceCount,
  availableCount = marketplaceCount,
  installedCount,
}) {
  if (status === 'loading') return message || `GitHub Discovery läuft. Installierte Apps bleiben sichtbar.`;
  if (status === 'ready') {
    const base = message || `${discoveredCount} GitHub Module gefunden.`;
    return `${base} ${availableCount} noch nicht lokal vorhanden. ${installedCount} installierte Apps lokal gezählt.`;
  }
  if (status === 'empty') return message || 'Keine GitHub Module gefunden. Installierte Apps bleiben lokal verfügbar.';
  if (status === 'stale') return `GitHub Sync fehlgeschlagen. Zeige letzten Stand: ${message || 'Unbekannter Fehler'}`;
  if (status === 'error') return `GitHub Sync fehlgeschlagen: ${message || 'Unbekannter Fehler'}`;
  return `GitHub modules are loaded from ${CTOX_REPO}/${CTOX_APP_ROOT}/modules. Installed: ${installedCount}.`;
}

function emptyCatalogTitle(scope, query, marketplaceStatus) {
  if (scope === 'marketplace' && marketplaceStatus === 'loading') return 'GitHub Discovery läuft';
  if (scope === 'marketplace' && marketplaceStatus === 'error') return 'GitHub Discovery fehlgeschlagen';
  if (query) return 'Keine Apps gefunden';
  return 'Keine Apps in dieser Kategorie';
}

function emptyCatalogBody(scope, query, marketplaceStatus, marketplaceMessage = '') {
  if (scope === 'marketplace' && marketplaceStatus === 'loading') return 'Der Katalog wird gerade mit GitHub synchronisiert.';
  if (scope === 'marketplace' && marketplaceStatus === 'error') return marketplaceMessage || 'Der letzte GitHub Refresh konnte nicht geladen werden.';
  if (query) return `Kein Katalogeintrag passt zu "${query}".`;
  return 'Wechsle die Kategorie oder aktualisiere GitHub Discovery.';
}

function externalLinkIcon() {
  return '<svg class="external-link-icon" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M7 17L17 7"></path><path d="M8 7h9v9"></path></svg>';
}

function sourceShort(item) {
  if (item.repo) return item.repo.split('/').slice(-1)[0];
  return item.source || item.kind;
}

function repoOwner(repo = '') {
  return String(repo).split('/')[0] || '';
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

function sanitizeId(value) {
  return String(value || '').trim().toLowerCase().replace(/[^a-z0-9_-]+/g, '-').replace(/^-+|-+$/g, '');
}

function newId() {
  if (globalThis.crypto?.randomUUID) return crypto.randomUUID();
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

async function startScopedSyncCollections(collections, reason) {
  const sync = state.ctx?.sync;
  if (!sync?.startCollection && !sync?.leaseCollection) return { handles: [], leases: [] };
  const leases = [];
  const handles = [];
  for (const collection of collections || []) {
    const handle = await startScopedSyncCollection(sync, collection, reason, leases);
    if (handle) handles.push(handle);
  }
  return {
    handles,
    leases,
  };
}

async function startScopedSyncCollection(sync, collection, reason, leases) {
  if (DEMAND_ONLY_SYNC_COLLECTIONS.has(collection)) {
    if (typeof sync?.leaseCollection === 'function') {
      const lease = await sync.leaseCollection(collection, reason);
      leases.push(lease);
      return lease;
    }
    throw new Error(`${collection} requires sync.leaseCollection().`);
  }
  return sync?.startCollection?.(collection);
}

async function flushScopedSyncCollections(syncHandles) {
  const handles = syncHandles?.handles || [];
  await Promise.all(handles.map((handle) => waitForSyncBridgeReady(handle, 15000, { allowPush: true })));
}

async function releaseSyncLeases(leases) {
  await Promise.all((leases || []).map((lease) => lease?.release?.().catch(() => null)));
}

async function waitForSyncBridgeReady(handle, timeoutMs = 10000, options = {}) {
  const state = syncBridgeFromHandle(handle)?.state;
  if (!state) return;
  const runWithTimeout = (promise) => Promise.race([
    Promise.resolve(promise).catch(() => {}),
    delay(timeoutMs),
  ]);
  await Promise.race([
    Promise.resolve()
      .then(() => state.awaitInSync?.() || state.awaitInitialReplication?.())
      .catch(() => {}),
    delay(timeoutMs),
  ]);
  if (options.allowPush && typeof state.pushToRemotePeers === 'function') {
    await runWithTimeout(state.pushToRemotePeers());
  } else if (options.allowPush && typeof state.awaitInSync === 'function') {
    await runWithTimeout(state.awaitInSync());
  }
}

function syncBridgeFromHandle(handle) {
  return handle?.bridge || handle;
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
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
  appStoreContextChatDetail,
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
  chooseCanonicalCatalogItem,
  compareVersions,
  creatorHashFromStore,
  emptyCatalogBody,
  emptyCatalogTitle,
  externalLinkIcon,
  isInstalledCatalogItem,
  itemMatchesScope,
  marketplaceStateLabel,
  modificationStateFor,
  originLabel,
  operationMessageHtml,
  progressButtonHtml,
  releasePayloadForWizard,
  releaseFactLinesForItem,
  releaseProjectionBadgeHtml,
  releaseWizardModel,
  sanitizeId,
  statusForCard,
  statusLabel,
  updateStateFor,
  versionsButtonHtml,
};

function ensureAppStoreContextMenuElement(state) {
  if (state.contextMenu?.isConnected) return state.contextMenu;
  const menu = document.createElement('div');
  menu.className = 'ctox-context-menu app-store-context-menu';
  menu.hidden = true;
  document.body.append(menu);
  state.contextMenu = menu;
  return menu;
}

function hideAppStoreContextMenu(state) {
  if (state.contextMenu) state.contextMenu.hidden = true;
}

function canModifyAppStoreAppForModule(state, item) {
  return canModifyBusinessModule(item, appStorePermissionOptions(state));
}

function canInstallAppStoreItem(state, item) {
  const moduleId = String(item?.id || item?.module_id || '').trim();
  if (!moduleId) return false;
  return canInstallBusinessApps({
    ...appStorePermissionOptions(state),
    scopeType: 'module',
    scopeId: moduleId,
  });
}

function canEditAppStoreItem(state, item) {
  return canModifyAppStoreAppForModule(state, item);
}

function canUninstallAppStoreItem(state, item) {
  return canUninstallBusinessApp(item, appStorePermissionOptions(state));
}

function canReleaseAppStoreItem(state, item) {
  const moduleId = String(item?.id || item?.module_id || '').trim();
  if (!moduleId || !isReleaseCandidateItem(item)) return false;
  return canUseBusinessPermission({
    ...appStorePermissionOptions(state),
    permission: BusinessOsPermissions.AppsRelease,
    scopeType: 'module',
    scopeId: moduleId,
  });
}

function appStorePermissionOptions(state) {
  return {
    session: state?.ctx?.session || null,
    governance: state?.catalog?.governance || state?.ctx?.governance || null,
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
    actor: actorContext(state?.ctx?.session),
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

function appStoreCommandContextFromElement(state, target) {
  const element = target?.nodeType === Node.ELEMENT_NODE ? target : target?.parentElement;

  const card = element?.closest('[data-app-id]');
  const appId = card?.dataset?.appId || '';
  const item = appId ? catalogItems().find((candidate) => candidate.id === appId) : null;
  const projection = item?.release_projection || appReleaseProjection(item?.raw || item);

  return {
    module: 'app-store',
    column: state.drawerOpen ? 'detail' : 'grid',
    record_type: item ? 'app' : 'store',
    record_id: item?.id || '',
    label: item?.title || state.query || 'App Store',
    app_id: item?.id || '',
    app_title: item?.title || '',
    app_description: item?.description || '',
    app_developer: item?.developer || '',
    app_version: item?.version || '',
    app_status: item?.status || '',
    app_visibility: item?.lifecycle?.state || '',
    app_visibility_label: item?.lifecycle?.label || '',
    app_category: item?.category || '',
    app_source: item?.source || '',
    data_access: projection?.dataAccess || null,
    data_access_summary: projection?.dataAccess?.summary || '',
    active_search: state.query || '',
    active_scope: state.scope || 'marketplace',
    selected_text: String(window.getSelection?.()?.toString?.() || '').trim().slice(0, 1000),
    clicked_text: String(element?.innerText || element?.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 500),
  };
}

function renderAppStoreContextMenu(state, context, x, y) {
  const canModifyApp = canModifyAppStoreContext(state, context);
  const agentScope = buildAppStoreAgentScopeView(state, context, canModifyApp ? 'app' : 'data');
  const menu = ensureAppStoreContextMenuElement(state);
  menu.innerHTML = `
    <form class="app-store-context-chat" data-app-store-context-chat-form>
      <header>
        <div>
          <strong>Chat to CTOX</strong>
          <span>${escapeHtml(context.label || 'App Store')}</span>
        </div>
        <button type="button" data-app-store-context-close aria-label="Schließen">×</button>
      </header>
      ${canModifyApp ? `
        <div class="ctox-context-mode" role="radiogroup" aria-label="CTOX Aufgabe">
          <label><input type="radio" name="contextMode" value="data" checked /> Mit Daten arbeiten</label>
          <label><input type="radio" name="contextMode" value="app" /> App ändern</label>
        </div>
      ` : ''}
      ${renderGlobalCtoxAgentScopeHtml({ view: agentScope })}
      <textarea data-app-store-context-message placeholder="Was soll CTOX im App Store tun oder anpassen?"></textarea>
      <footer>
        <span data-app-store-context-status></span>
        <button type="submit">Senden</button>
      </footer>
    </form>
  `;
  menu.hidden = false;
  menu.style.left = '0px';
  menu.style.top = '0px';
  const rect = menu.getBoundingClientRect();
  const clampNumber = (val, min, max) => Math.min(max, Math.max(min, val));
  const maxLeft = Math.max(8, window.innerWidth - rect.width - 8);
  const maxTop = Math.max(8, window.innerHeight - rect.height - 8);
  menu.style.left = `${clampNumber(x, 8, maxLeft)}px`;
  menu.style.top = `${clampNumber(y, 8, maxTop)}px`;

  const form = menu.querySelector('[data-app-store-context-chat-form]');
  const textarea = menu.querySelector('[data-app-store-context-message]');
  menu.querySelector('[data-app-store-context-close]')?.addEventListener('click', () => hideAppStoreContextMenu(state));
  form?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const mode = canModifyApp ? (new FormData(form).get('contextMode') || 'data') : 'data';
    await dispatchAppStoreContextChat(state, context, textarea?.value || '', mode);
  });
  requestAnimationFrame(() => textarea?.focus());
}

async function dispatchAppStoreContextChat(state, context, message, mode = 'data') {
  const trimmed = String(message || '').trim();
  const status = state.contextMenu?.querySelector('[data-app-store-context-status]');
  if (!trimmed) {
    if (status) status.textContent = 'Nachricht fehlt.';
    return;
  }

  const safeMode = mode === 'app' && canModifyAppStoreContext(state, context) ? 'app' : 'data';
  if (!document.querySelector('[data-ctox-chat-root]')) {
    if (status) status.textContent = 'Chat ist noch nicht bereit.';
    return;
  }
  if (status) status.textContent = 'Oeffne Chat...';
  window.dispatchEvent(new CustomEvent('ctox-business-os-chat-submit', {
    detail: appStoreContextChatDetail(state, context, trimmed, safeMode),
  }));
  hideAppStoreContextMenu(state);
}

function appStoreContextChatDetail(state, context, message, mode = 'data') {
  const safeMode = mode === 'app' && canModifyAppStoreContext(state, context) ? 'app' : 'data';
  const targetModuleId = sanitizeId(context?.app_id || context?.record_id || 'app-store') || 'app-store';
  const label = context?.label || context?.app_title || targetModuleId || 'App Store';
  const agentScope = buildAppStoreAgentScopeView(state, context, safeMode);
  const title = `${safeMode === 'app' ? 'App ändern' : 'Store durchsuchen'} · ${label}`;
  const instruction = safeMode === 'app'
    ? `Modifiziere die ausgewählte Business-OS-App "${label}". Zielmodul: ${targetModuleId}.\n\n${message}`
    : message;
  return {
    text: message,
    module: 'app-store',
    source_title: 'App Store',
    command_type: safeMode === 'app' ? 'ctox.business_os.app.modify' : 'business_os.chat.task',
    record_id: safeMode === 'app' ? targetModuleId : (context?.record_id || 'app-store'),
    title,
    instruction,
    payload: {
      title,
      instruction,
      prompt: message,
      user_message: message,
      mode: safeMode,
      target: safeMode === 'app' ? 'app' : 'data',
      module_id: safeMode === 'app' ? targetModuleId : undefined,
      app_id: safeMode === 'app' ? targetModuleId : (context?.app_id || ''),
      context,
      thread_key: 'business-os/app-store',
    },
    client_context: {
      source: 'business-os-app-store',
      module: 'app-store',
      source_module: 'app-store',
      action: 'context-chat',
      mode: safeMode,
      target: safeMode === 'app' ? 'app' : 'data',
      column: context?.column,
      record_type: context?.record_type,
      record_id: context?.record_id || targetModuleId,
      module_id: targetModuleId,
      app_id: context?.app_id || targetModuleId,
      actor: agentScope.actor,
      visible_scope: agentScope,
      active_search: context?.active_search || '',
      active_scope: context?.active_scope || '',
    },
  };
}
