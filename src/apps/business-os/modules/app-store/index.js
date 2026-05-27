import { CtoxResizer } from '../../shared/resizer.js';

const CTOX_REPO = 'metric-space-ai/ctox';
const CTOX_BRANCH = 'main';
const CTOX_APP_ROOT = 'src/apps/business-os';
const CTOX_TREE_URL = `https://api.github.com/repos/${CTOX_REPO}/git/trees/${CTOX_BRANCH}?recursive=1`;
const CTOX_RAW_ROOT = `https://raw.githubusercontent.com/${CTOX_REPO}/${CTOX_BRANCH}/${CTOX_APP_ROOT}`;
const CTOX_DOWNLOAD_URL = `https://github.com/${CTOX_REPO}/archive/refs/heads/${CTOX_BRANCH}.zip`;


const state = {
  ctx: null,
  catalog: null,
  marketplace: [],
  marketplaceStatus: 'idle',
  marketplaceMessage: '',
  selectedId: '',
  scope: 'marketplace',
  query: '',
  busy: false,
  status: null,
  unsubscribe: null,
  viewMode: 'grid',
  drawerOpen: false,
  contextMenu: null,
  contextMenuCleanup: null,
};

const els = {};

export async function mount(ctx) {
  state.ctx = ctx;
  ctx.host.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  ensureStylesheet();
  bindElements(ctx.host);
  wireEvents();
  await Promise.all([
    ctx.sync?.startCollection?.('business_module_catalog'),
    ctx.sync?.startCollection?.('business_commands'),
  ]);
  await loadCatalog();
  state.unsubscribe = ctx.db?.collection?.('business_module_catalog')
    ?.findOne('module-catalog')
    ?.$
    ?.subscribe?.((doc) => {
      const data = doc?.toJSON?.();
      if (data) {
        state.catalog = data;
        render();
      }
    }) || null;
  render();
  refreshMarketplace();

  // 5. Initialize CTOX unified context menu
  state.contextMenuCleanup = initAppStoreContextMenu(state);

  // Setup resizer
  const containerEl = ctx.host.querySelector('[data-app-store-root]') || ctx.host;
  const resizerEl = ctx.host.querySelector('.app-store-col-resizer');
  let resizerCleanup = null;
  if (resizerEl) {
    const resizer = new CtoxResizer({
      resizerEl,
      containerEl,
      cssVar: '--app-store-left-width',
      side: 'left',
      minWidth: 240,
      maxWidth: 500,
      onResize: (width) => localStorage.setItem('ctox.app-store.leftWidth', width)
    });
    resizerCleanup = () => resizer.destroy();
  }
  const leftWidth = localStorage.getItem('ctox.app-store.leftWidth') || '320';
  containerEl.style.setProperty('--app-store-left-width', `${leftWidth}px`);

  return () => {
    try { state.unsubscribe?.unsubscribe?.(); } catch {}
    state.contextMenuCleanup?.();
    state.contextMenu?.remove();
    state.contextMenu = null;
    resizerCleanup?.();
  };
}

function ensureStylesheet() {
  const href = new URL('./index.css', import.meta.url).pathname;
  if (document.head.querySelector(`link[href="${href}"]`)) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  document.head.append(link);
}

function bindElements(root) {
  els.search = root.querySelector('[data-search]');
  els.scopes = root.querySelector('[data-scope-list]');
  els.title = root.querySelector('[data-visible-category-title]');
  els.count = root.querySelector('[data-apps-count]');
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
  els.viewToggle = root.querySelector('[data-view-toggle]');
  els.loading = root.querySelector('[data-loading-spinner]');
  els.loadingText = root.querySelector('[data-loading-text]');
  els.refresh = root.querySelector('[data-refresh-marketplace]');
  els.message = root.querySelector('[data-store-message]');
  els.marketplaceState = root.querySelector('[data-marketplace-state]');
}

function wireEvents() {
  els.search?.addEventListener('input', () => {
    state.query = els.search.value.trim().toLowerCase();
    render();
  });
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

    if (actionBtn) {
      const actionType = actionBtn.dataset.cardAction;
      triggerCardAction(appId, actionType);
      return;
    }

    state.drawerOpen = true;
    render();
  });

  els.closeDrawer?.addEventListener('click', () => {
    state.drawerOpen = false;
    render();
  });

  els.viewToggle?.addEventListener('click', (event) => {
    const btn = event.target.closest('[data-view]');
    if (!btn) return;
    state.viewMode = btn.dataset.view || 'grid';
    render();
  });

  els.refresh?.addEventListener('click', () => refreshMarketplace({ force: true }));

  state.ctx.host.querySelector('#btn-create-scratch')?.addEventListener('click', () => {
    openCreatorFromStore({ mode: 'scratch' });
  });
}

async function triggerCardAction(appId, actionType) {
  const item = catalogItems().find((candidate) => candidate.id === appId);
  if (!item || state.busy) return;

  if (actionType === 'install') {
    await installMarketplaceItem(item);
  } else if (actionType === 'open') {
    if (item.id === 'create-scratch') {
      openCreatorFromStore({ mode: 'scratch' });
    } else if (item.launch_kind === 'desktop-app') {
      await state.ctx?.openDesktopApp?.(item.id);
    } else {
      openModule(item.id);
    }
  } else if (actionType === 'upgrade') {
    openCreatorFromStore({ mode: 'upgrade', upgrade: item.id });
  } else if (actionType === 'uninstall') {
    await uninstallInstalledItem(item);
  } else if (actionType === 'repository') {
    if (item.homepage) {
      window.open(item.homepage, '_blank', 'noopener,noreferrer');
    }
  } else if (actionType === 'details') {
    state.drawerOpen = true;
    render();
  }
}

async function loadCatalog() {
  const doc = await state.ctx.db?.collection?.('business_module_catalog')?.findOne('module-catalog').exec();
  state.catalog = doc?.toJSON?.() || { modules: [], templates: [], marketplace: [] };
  state.marketplace = normalizeMarketplace(state.catalog.marketplace || state.catalog.apps || []);
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
  if (!moduleId) return null;
  const relativePath = path.replace(`${CTOX_APP_ROOT}/`, '').replace('/module.json', '');
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
    source_path: relativePath,
    manifest_url: manifestUrl,
    download_url: CTOX_DOWNLOAD_URL,
    homepage: `https://github.com/${CTOX_REPO}/tree/${CTOX_BRANCH}/${path.replace('/module.json', '')}`,
    permissions: manifest.collections || [],
    installable: manifest.store?.installable !== false,
    updated_at: '',
  });
}

function catalogItems() {
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
      .filter((item) => !moduleIds.has(item.module_id || item.id))
      .map((item) => normalizeItem(item, 'marketplace')),
    ...templates.map((item) => normalizeItem(item, 'template')),
    ...modules
      .filter(isLaunchableModule)
      .map((item) => normalizeItem(item, moduleKind(item))),
    ...desktopApps
      .filter((item) => item?.id && !moduleIds.has(item.id))
      .map(normalizeDesktopAppItem),
  ];
  return uniqueCatalogItems(items).sort(sortItems);
}

function isLaunchableModule(item) {
  return item?.id && item.id !== 'desktop' && item.id !== 'notizen' && item.install_scope !== 'internal';
}

function moduleKind(item) {
  if (item?.core) return 'system';
  if (item?.install_scope === 'internal') return 'system';
  if (item?.install_scope === 'starter' || item?.source === 'starter') return 'starter';
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
    install_scope: item.install_scope || item.store?.install_scope || '',
    permissions: Array.isArray(item.permissions) ? item.permissions : (Array.isArray(item.collections) ? item.collections : []),
    installable: item.installable !== false && item.store?.installable !== false,
    raw: item,
  };
}

function normalizeItem(item, kind) {
  const id = sanitizeId(item.module_id || item.id || item.source_module || item.default_title || '');
  const status = statusForItem(item, kind);
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
    repo: item.repo || item.repository || '',
    download_url: item.download_url || '',
    source_path: item.source_path || '',
    manifest_url: item.manifest_url || '',
    homepage: item.homepage || '',
    install_scope: item.install_scope || item.raw?.install_scope || '',
    permissions: item.permissions || item.collections || [],
    installable: item.installable !== false && item.store?.installable !== false,
    raw: item,
  };
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
    install_scope: '',
    permissions: [],
    installable: false,
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
  if (kind === 'starter') return 'starter';
  if (kind === 'installed') return 'installed';
  return 'local';
}

function sourceLabel(item, kind) {
  if (kind === 'marketplace') return item.repo || item.source || 'GitHub';
  if (kind === 'template') return item.source_module || 'template-store';
  return item.source || kind;
}

function filteredItems() {
  return scopedCatalogItems(state.scope).filter((item) => {
    const haystack = `${item.title} ${item.description} ${item.category} ${item.repo} ${item.source}`.toLowerCase();
    return !state.query || haystack.includes(state.query);
  });
}

function scopedCatalogItems(scope) {
  const items = catalogItems();
  const scoped = scope === 'all'
    ? items
    : items.filter((item) => itemMatchesScope(item, scope));
  return uniqueCatalogItems(scoped);
}

function itemMatchesScope(item, scope) {
  return scope === 'all' || item.kind === scope || item.status === scope;
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
    starter: 3,
    template: 4,
    marketplace: 5,
  };
  const existingRank = rank[existing.kind] ?? 9;
  const candidateRank = rank[candidate.kind] ?? 9;
  if (candidateRank < existingRank) return candidate;
  if (candidateRank > existingRank) return existing;
  if (candidate.status === 'installed' && existing.status !== 'installed') return candidate;
  return existing;
}

function render() {
  const items = filteredItems();
  updateScopeButtons();
  renderMarketplaceState();
  renderMessage();
  if (els.title) els.title.textContent = scopeTitle(state.scope);
  if (els.count) els.count.textContent = appCountLabel(items.length, state.scope, state.marketplaceStatus);

  if (els.grid) {
    els.grid.className = `store-card-grid ${state.viewMode === 'list' ? 'is-list-view' : ''}`;
    els.grid.replaceChildren(...renderCatalogBody(items));
  }

  if (els.viewToggle) {
    for (const btn of els.viewToggle.querySelectorAll('[data-view]')) {
      const active = btn.dataset.view === state.viewMode;
      btn.classList.toggle('active', active);
      btn.style.background = active ? 'rgba(255, 255, 255, 0.1)' : 'transparent';
      const svg = btn.querySelector('svg');
      if (svg) svg.style.color = active ? 'var(--accent, #e5a93c)' : 'var(--text-muted, #8e8e93)';
    }
  }

  if (!items.some((item) => item.id === state.selectedId)) {
    state.selectedId = items.length ? (items[0]?.id || '') : '';
  }
  renderDetails();
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
  empty.className = 'store-empty-state';
  empty.setAttribute('role', 'status');
  empty.innerHTML = `
    <strong>${escapeHtml(title)}</strong>
    <span>${escapeHtml(body)}</span>
  `;
  return empty;
}

function renderCard(item) {
  const card = document.createElement('article');
  card.className = 'app-card';
  card.dataset.appId = item.id;
  card.classList.toggle('active', item.id === state.selectedId);

  let actionsHtml = `<div class="app-card-actions">`;

  if (item.id === 'create-scratch') {
    actionsHtml += `<button type="button" class="card-btn primary" data-card-action="open">Erstellen</button>`;
  } else if (item.kind === 'marketplace') {
    if (item.status === 'installed') {
      actionsHtml += `<button type="button" class="card-btn primary" data-card-action="open">Öffnen</button>`;
    } else if (item.installable) {
      actionsHtml += `<button type="button" class="card-btn primary" data-card-action="install">Installieren</button>`;
    }
    if (item.homepage) {
      actionsHtml += `<button type="button" class="card-btn secondary external" data-card-action="repository" data-external-action="github" title="GitHub repository in new tab" aria-label="GitHub repository in new tab">GitHub ${externalLinkIcon()}</button>`;
    }
  } else if (item.kind === 'template') {
    actionsHtml += `<button type="button" class="card-btn primary" data-card-action="open">Erstellen</button>`;
  } else if (item.kind === 'system') {
    actionsHtml += `<button type="button" class="card-btn primary" data-card-action="open">Öffnen</button>`;
  } else if (item.kind === 'starter') {
    actionsHtml += `<button type="button" class="card-btn primary" data-card-action="open">Öffnen</button>`;
  } else {
    // Local / Installed non-system apps
    actionsHtml += `
      <button type="button" class="card-btn primary" data-card-action="open">Öffnen</button>
      <button type="button" class="card-btn warn" data-card-action="upgrade">Upgrade</button>
      <button type="button" class="card-btn danger" data-card-action="uninstall">Deinstallieren</button>
    `;
  }

  if (item.id !== 'create-scratch') {
    actionsHtml += `<button type="button" class="card-btn link" data-card-action="details">Details</button>`;
  }

  actionsHtml += `</div>`;

  card.innerHTML = `
    <div class="app-card-head">
      <div class="app-card-icon">${escapeHtml(iconForItem(item))}</div>
      <div class="app-card-meta">
        <h3 class="app-card-title">${escapeHtml(item.title)}</h3>
        <span class="app-card-category">${escapeHtml(item.category)}</span>
      </div>
    </div>
    <p class="app-card-desc">${escapeHtml(item.description || item.source)}</p>
    ${actionsHtml}
    <footer class="app-card-footer">
      <span class="app-status-badge ${escapeHtml(item.status)}">${escapeHtml(statusLabel(item.status))}</span>
      <span class="app-card-source">${escapeHtml(sourceShort(item))}</span>
    </footer>
  `;
  return card;
}

function renderDetails() {
  const item = catalogItems().find((candidate) => candidate.id === state.selectedId);
  if (!state.drawerOpen) {
    if (els.detail) els.detail.classList.remove('is-open');
    return;
  }
  if (els.detail) els.detail.classList.add('is-open');
  if (!item) {
    renderEmptyDetails();
    return;
  }
  if (els.detailIcon) els.detailIcon.textContent = iconForItem(item);
  if (els.detailTitle) els.detailTitle.textContent = item.title;
  if (els.detailVersion) els.detailVersion.textContent = item.version;
  if (els.detailCategory) els.detailCategory.textContent = item.category;
  if (els.detailDeveloper) els.detailDeveloper.textContent = item.developer;
  if (els.detailLicense) els.detailLicense.textContent = item.license;
  if (els.detailSource) els.detailSource.textContent = item.source;
  if (els.detailStatus) els.detailStatus.textContent = statusLabel(item.status);
  if (els.readme) {
    els.readme.replaceChildren(renderDocumentation(item));
  }
}

function renderEmptyDetails() {
  if (els.detailIcon) els.detailIcon.textContent = '?';
  if (els.detailTitle) els.detailTitle.textContent = 'Keine App ausgewählt';
  if (els.detailVersion) els.detailVersion.textContent = '-';
  if (els.detailCategory) els.detailCategory.textContent = 'Empty';
  if (els.detailDeveloper) els.detailDeveloper.textContent = '-';
  if (els.detailLicense) els.detailLicense.textContent = '-';
  if (els.detailSource) els.detailSource.textContent = 'App Store';
  if (els.detailStatus) els.detailStatus.textContent = 'No selection';
  if (els.readme) {
    const empty = document.createElement('p');
    empty.className = 'store-detail-empty';
    empty.textContent = 'Wähle eine App oder ändere den Filter, um Details zu sehen.';
    els.readme.replaceChildren(empty);
  }
}

function renderDocumentation(item) {
  const wrap = document.createElement('div');
  const lines = [
    item.description || 'No documentation available yet.',
    item.repo ? `Repository: ${item.repo}` : '',
    item.source_path ? `Source path: ${item.source_path}` : '',
    item.download_url ? `Installer archive: ${item.download_url}` : '',
    item.permissions?.length ? `Collections: ${item.permissions.join(', ')}` : '',
  ].filter(Boolean);
  for (const line of lines) {
    const p = document.createElement('p');
    p.textContent = line;
    wrap.append(p);
  }
  return wrap;
}

async function installMarketplaceItem(item) {
  await runStoreCommand({
    label: `Installing ${item.title}...`,
    success: `${item.title} installed.`,
    commandType: 'ctox.app_store.install',
    moduleId: item.id,
    payload: {
      module_id: item.id,
      download_url: item.download_url,
      source_path: item.source_path,
      manifest_url: item.manifest_url,
    },
  });
}

async function installTemplateItem(item) {
  await runStoreCommand({
    label: `Creating ${item.title}...`,
    success: `${item.title} created from template.`,
    commandType: 'ctox.module.install_template',
    moduleId: item.id,
    payload: {
      template_id: item.id,
      title: item.raw.default_title || item.title,
    },
  });
}

async function uninstallInstalledItem(item) {
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

async function runStoreCommand({ label, success, commandType, moduleId, payload }) {
  setBusy(true, label);
  try {
    const commandId = `cmd_${newId()}`;
    await state.ctx.commandBus.dispatch({
      id: commandId,
      module: 'app-store',
      type: commandType,
      record_id: moduleId,
      inbound_channel: 'business_os.app_store',
      payload,
      client_context: {
        source: 'business-os-app-store',
        module_id: moduleId,
        actor: actorContext(state.ctx.session),
      },
    });
    const result = await waitForCommandProjection(commandId);
    state.status = { kind: 'success', text: success, result };
    await loadCatalog();
    render();
  } catch (error) {
    state.status = { kind: 'error', text: error?.message || String(error) };
    render();
  } finally {
    setBusy(false);
  }
}

async function waitForCommandProjection(commandId, timeoutMs = 60000) {
  const collection = state.ctx.db?.collection?.('business_commands');
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const doc = await collection?.findOne(commandId).exec();
    const data = doc?.toJSON?.();
    if (data && data.status && data.status !== 'pending_sync') {
      if (data.status === 'failed') {
        const reason = data.error || data.result?.error || `Command ${commandId} failed`;
        throw new Error(reason);
      }
      return data;
    }
    await delay(300);
  }
  throw new Error(`Command ${commandId} wurde nicht synchronisiert.`);
}

function setBusy(busy, text = '') {
  state.busy = busy;
  if (els.action) els.action.disabled = busy;
  if (els.secondaryAction) els.secondaryAction.disabled = busy;
  const showLoading = busy || state.marketplaceStatus === 'loading';
  if (els.loading) els.loading.hidden = !showLoading;
  if (els.loadingText) {
    els.loadingText.textContent = text || state.marketplaceMessage || 'GitHub Discovery wird synchronisiert.';
  }
  if (els.refresh) els.refresh.disabled = busy || state.marketplaceStatus === 'loading';
}

function setScope(scope) {
  state.scope = scope;
  state.selectedId = '';
  render();
}

function updateScopeButtons() {
  if (!els.scopes) return;
  const counts = countsByScope();
  for (const button of els.scopes.querySelectorAll('[data-scope]')) {
    const scope = button.dataset.scope || 'marketplace';
    button.classList.toggle('active', scope === state.scope);
    const count = button.querySelector('[data-scope-count]');
    if (count) count.textContent = String(counts[scope] || 0);
  }
}

function countsByScope() {
  return {
    all: scopedCatalogItems('all').length,
    marketplace: scopedCatalogItems('marketplace').length,
    template: scopedCatalogItems('template').length,
    installed: scopedCatalogItems('installed').length,
    starter: scopedCatalogItems('starter').length,
    system: scopedCatalogItems('system').length,
    local: scopedCatalogItems('local').length,
  };
}

function renderMarketplaceState() {
  if (!els.marketplaceState) return;
  const counts = countsByScope();
  els.marketplaceState.textContent = marketplaceStateLabel({
    status: state.marketplaceStatus,
    message: state.marketplaceMessage,
    marketplaceCount: counts.marketplace,
    installedCount: counts.installed,
  });
  els.marketplaceState.dataset.state = state.marketplaceStatus;
  if (els.refresh) {
    const refreshBusy = state.marketplaceStatus === 'loading' || state.busy;
    els.refresh.disabled = refreshBusy;
    els.refresh.textContent = state.marketplaceStatus === 'loading' ? 'Synchronisiere GitHub...' : 'Refresh GitHub';
    els.refresh.title = refreshBusy
      ? 'GitHub Discovery läuft bereits.'
      : `GitHub Discovery aus ${CTOX_REPO} aktualisieren.`;
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
}

function openModule(moduleId) {
  if (!moduleId) return;
  window.location.hash = moduleId;
}

function openCreatorFromStore({ mode = 'scratch', upgrade = '' } = {}) {
  const hash = creatorHashFromStore({ mode, upgrade });
  try {
    sessionStorage.setItem('ctox.app-store.creatorReturnContext', JSON.stringify({
      source: 'app-store',
      return_hash: '#app-store',
      mode,
      upgrade,
      created_at: new Date().toISOString(),
    }));
  } catch {}
  openModule(hash);
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

function mergeMarketplace(primary, fallback) {
  const map = new Map();
  for (const item of [...fallback, ...primary]) {
    map.set(item.id, item);
  }
  return [...map.values()].sort((left, right) => left.title.localeCompare(right.title));
}

function sortItems(left, right) {
  const rank = { marketplace: 0, template: 1, installed: 2, starter: 3, local: 4, system: 5 };
  return (rank[left.kind] ?? 9) - (rank[right.kind] ?? 9)
    || left.title.localeCompare(right.title);
}

function scopeTitle(scope) {
  return {
    all: 'All Applications',
    marketplace: 'GitHub Marketplace',
    template: 'Templates',
    installed: 'Installed Apps',
    starter: 'Starter Apps',
    system: 'System Apps',
    local: 'Local Modules',
  }[scope] || 'Applications';
}

function iconForItem(item) {
  if (item.kind === 'marketplace') return item.status === 'installed' ? '✓' : 'GH';
  if (item.kind === 'template') return '+';
  if (item.kind === 'installed') return '✓';
  if (item.kind === 'starter') return '★';
  if (item.kind === 'system') return '◆';
  return '*';
}

function statusLabel(status) {
  return {
    available: 'Available',
    installed: 'Installed',
    starter: 'Starter',
    template: 'Template',
    system: 'System',
    local: 'Local',
  }[status] || status;
}

function appCountLabel(count, scope, marketplaceStatus) {
  const suffix = count === 1 ? 'App' : 'Apps';
  if (scope === 'marketplace' && marketplaceStatus === 'loading') {
    return `${count} ${suffix} · Sync`;
  }
  return `${count} ${suffix}`;
}

function marketplaceStateLabel({ status, message, marketplaceCount, installedCount }) {
  if (status === 'loading') return message || `GitHub Discovery läuft. Installierte Apps bleiben sichtbar.`;
  if (status === 'ready') return message || `${marketplaceCount} GitHub Module gefunden. ${installedCount} installierte Apps lokal gezählt.`;
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

export const __appStoreTestHooks = {
  appCountLabel,
  chooseCanonicalCatalogItem,
  creatorHashFromStore,
  emptyCatalogBody,
  emptyCatalogTitle,
  externalLinkIcon,
  itemMatchesScope,
  marketplaceStateLabel,
  sanitizeId,
  statusLabel,
};

function initAppStoreContextMenu(state) {
  state.contextMenu?.remove();
  const menu = document.createElement('div');
  menu.className = 'ctox-context-menu app-store-context-menu';
  menu.hidden = true;
  document.body.append(menu);
  state.contextMenu = menu;

  const handleContextMenu = (event) => {
    if (state.ctx.module?.id !== 'app-store') return;
    const context = appStoreCommandContextFromElement(state, event.target);
    event.preventDefault();
    event.stopPropagation();
    renderAppStoreContextMenu(state, context, event.clientX, event.clientY);
  };
  const handleOutsideClick = (event) => {
    if (state.contextMenu?.contains(event.target)) return;
    hideAppStoreContextMenu(state);
  };
  const handleEscape = (event) => {
    if (event.key === 'Escape') hideAppStoreContextMenu(state);
  };

  state.ctx.host.addEventListener('contextmenu', handleContextMenu);
  window.addEventListener('click', handleOutsideClick, { capture: true });
  window.addEventListener('keydown', handleEscape);

  return () => {
    state.ctx.host.removeEventListener('contextmenu', handleContextMenu);
    window.removeEventListener('click', handleOutsideClick, { capture: true });
    window.removeEventListener('keydown', handleEscape);
    hideAppStoreContextMenu(state);
    state.contextMenu?.remove();
    state.contextMenu = null;
  };
}

function hideAppStoreContextMenu(state) {
  if (state.contextMenu) state.contextMenu.hidden = true;
}

function canModifyAppStoreApp(state) {
  if (typeof state.ctx.canModifyModule === 'function' && state.ctx.canModifyModule()) return true;
  const user = state.ctx.session?.user || {};
  const role = String(user.role || (user.is_admin ? 'admin' : 'user')).trim().toLowerCase().replace(/^business_os_/, '');
  return ['admin', 'chef'].includes(role);
}

function appStoreCommandContextFromElement(state, target) {
  const element = target?.nodeType === Node.ELEMENT_NODE ? target : target?.parentElement;

  const card = element?.closest('[data-app-id]');
  const appId = card?.dataset?.appId || '';
  const item = appId ? catalogItems().find((candidate) => candidate.id === appId) : null;

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
    app_category: item?.category || '',
    app_source: item?.source || '',
    active_search: state.query || '',
    active_scope: state.scope || 'marketplace',
    selected_text: String(window.getSelection?.()?.toString?.() || '').trim().slice(0, 1000),
    clicked_text: String(element?.innerText || element?.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 500),
  };
}

function renderAppStoreContextMenu(state, context, x, y) {
  ensureCtoxContextMenuStyles();
  const canModifyApp = canModifyAppStoreApp(state);
  state.contextMenu.innerHTML = `
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
          <label><input type="radio" name="contextMode" value="app" /> App modifizieren</label>
        </div>
      ` : ''}
      <textarea data-app-store-context-message placeholder="Was soll CTOX im App Store tun oder anpassen?"></textarea>
      <footer>
        <span data-app-store-context-status></span>
        <button type="submit">Senden</button>
      </footer>
    </form>
  `;
  state.contextMenu.hidden = false;
  state.contextMenu.style.left = '0px';
  state.contextMenu.style.top = '0px';
  const rect = state.contextMenu.getBoundingClientRect();
  const clampNumber = (val, min, max) => Math.min(max, Math.max(min, val));
  const maxLeft = Math.max(8, window.innerWidth - rect.width - 8);
  const maxTop = Math.max(8, window.innerHeight - rect.height - 8);
  state.contextMenu.style.left = `${clampNumber(x, 8, maxLeft)}px`;
  state.contextMenu.style.top = `${clampNumber(y, 8, maxTop)}px`;

  const form = state.contextMenu.querySelector('[data-app-store-context-chat-form]');
  const textarea = state.contextMenu.querySelector('[data-app-store-context-message]');
  state.contextMenu.querySelector('[data-app-store-context-close]')?.addEventListener('click', () => hideAppStoreContextMenu(state));
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

  const safeMode = mode === 'app' && canModifyAppStoreApp(state) ? 'app' : 'data';
  if (!document.querySelector('[data-ctox-chat-root]')) {
    if (status) status.textContent = 'Chat ist noch nicht bereit.';
    return;
  }
  if (status) status.textContent = 'Oeffne Chat...';
  const title = `${safeMode === 'app' ? 'App Store App modifizieren' : 'Store durchsuchen'} · ${context.label || 'App Store'}`;
  const instruction = safeMode === 'app'
    ? `Modifiziere die App-Store-App anhand dieser Admin-Anweisung. Kontext nur als UI-Bezug verwenden, App-Store-Daten/Katalog selbst nicht als primäres Ziel verändern.\n\n${trimmed}`
    : trimmed;

  window.dispatchEvent(new CustomEvent('ctox-business-os-chat-submit', {
    detail: {
      text: trimmed,
      module: 'app-store',
      source_title: 'App Store',
      command_type: safeMode === 'app' ? 'ctox.business_os.app.modify' : 'business_os.chat.task',
      record_id: safeMode === 'app' ? 'app-store' : (context.record_id || 'app-store'),
      title,
      instruction,
      payload: {
        title,
        instruction,
        prompt: trimmed,
        user_message: trimmed,
        mode: safeMode,
        target: safeMode === 'app' ? 'app' : 'data',
        context,
        thread_key: 'business-os/app-store',
      },
      client_context: {
        action: 'context-chat',
        mode: safeMode,
        column: context.column,
        record_type: context.record_type,
        app_id: context.app_id || '',
        active_search: context.active_search || '',
        active_scope: context.active_scope || '',
      },
    },
  }));
  hideAppStoreContextMenu(state);
}

function ensureCtoxContextMenuStyles() {
  if (document.getElementById('ctox-unified-context-menu-style')) return;
  const style = document.createElement('style');
  style.id = 'ctox-unified-context-menu-style';
  style.textContent = `
    .ctox-context-menu {
      position: absolute;
      z-index: 2400;
      width: min(560px, calc(100vw - 24px));
      max-width: calc(100% - 16px);
      overflow: hidden;
      border: 1px solid var(--bo-border, var(--border, #d8e1e5));
      border-radius: var(--radius-panel, 12px);
      background: color-mix(in srgb, var(--bo-surface, var(--surface, #fff)) 75%, transparent);
      backdrop-filter: blur(16px);
      -webkit-backdrop-filter: blur(16px);
      box-shadow: 0 18px 50px rgba(0, 0, 0, 0.25);
      padding: 6px;
      font-family: system-ui, -apple-system, sans-serif;
      animation: ctox-menu-fade-in 0.15s ease-out;
    }
    @keyframes ctox-menu-fade-in {
      from { opacity: 0; transform: scale(0.97); }
      to { opacity: 1; transform: scale(1); }
    }
    .ctox-context-menu form {
      display: grid;
      grid-template-columns: minmax(0, 1fr);
      gap: 10px;
      min-width: 0;
      padding: 12px;
      margin: 0;
    }
    .ctox-context-menu form header,
    .ctox-context-menu form footer {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 10px;
      min-width: 0;
    }
    .ctox-context-menu .ctox-context-mode {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 6px;
      min-width: 0;
    }
    .ctox-context-menu .ctox-context-mode label {
      display: flex;
      align-items: center;
      gap: 7px;
      min-width: 0;
      min-height: 30px;
      border: 1px solid var(--bo-border, var(--border, #d8e1e5));
      border-radius: var(--radius-control, 6px);
      color: var(--bo-muted, var(--muted, #64747c));
      font-size: 11.5px;
      font-weight: 760;
      padding: 0 8px;
      cursor: pointer;
      background: var(--bo-surface-muted, var(--surface-2, #eef3f7));
      margin: 0;
    }
    .ctox-context-menu .ctox-context-mode label:hover {
      border-color: var(--bo-accent, #23665f);
    }
    .ctox-context-menu .ctox-context-mode input {
      margin: 0;
      accent-color: var(--bo-accent, #23665f);
    }
    .ctox-context-menu form header div {
      min-width: 0;
    }
    .ctox-context-menu form strong,
    .ctox-context-menu form span {
      display: block;
      min-width: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .ctox-context-menu form strong {
      color: var(--bo-text, var(--text, #18222d));
      font-size: 12.5px;
      font-weight: 820;
    }
    .ctox-context-menu form span {
      color: var(--bo-muted, var(--muted, #64747c));
      font-size: 11px;
      font-weight: 700;
    }
    .ctox-context-menu form footer > span {
      display: flex;
      align-items: center;
      gap: 6px;
      flex-wrap: wrap;
      white-space: normal;
      font-size: 11px;
      color: var(--bo-muted, var(--muted, #64747c));
    }
    .ctox-context-menu form textarea {
      width: 100%;
      box-sizing: border-box;
      min-height: 92px;
      max-height: 180px;
      min-width: 0;
      border: 1px solid var(--bo-border, var(--border, #d8e1e5));
      border-radius: var(--radius-control, 6px);
      background: var(--bo-surface-muted, var(--surface-2, #eef3f7));
      color: var(--bo-text, var(--text, #18222d));
      font: 12.5px/1.4 system-ui, -apple-system, "Segoe UI", sans-serif;
      padding: 9px;
      resize: vertical;
    }
    .ctox-context-menu form textarea:focus {
      outline: none;
      border-color: var(--bo-accent, #23665f);
      box-shadow: 0 0 0 2px color-mix(in srgb, var(--bo-accent, #23665f) 25%, transparent);
    }
    .ctox-context-menu form button {
      flex: 0 0 auto;
      min-height: 30px;
      border: 1px solid var(--bo-border, var(--border, #d8e1e5));
      border-radius: var(--radius-control, 6px);
      background: var(--bo-surface-muted, var(--surface-2, #eef3f7));
      color: var(--bo-text, var(--text, #18222d));
      font: inherit;
      font-size: 12px;
      font-weight: 760;
      cursor: pointer;
      padding: 0 10px;
    }
    .ctox-context-menu form button:hover {
      background: color-mix(in srgb, var(--bo-text, #18222d) 8%, var(--bo-surface-muted, #eef3f7));
    }
    .ctox-context-menu form button[type="submit"] {
      border-color: var(--bo-accent, #23665f);
      background: color-mix(in srgb, var(--bo-accent, #23665f) 14%, var(--bo-surface, #fff));
      color: var(--bo-accent, #23665f);
    }
    .ctox-context-menu form button[type="submit"]:hover {
      background: color-mix(in srgb, var(--bo-accent, #23665f) 22%, var(--bo-surface, #fff));
    }
    .ctox-context-menu form button[type="button"][aria-label="Schließen"],
    .ctox-context-menu form [data-creator-context-close],
    .ctox-context-menu form [data-reports-context-close],
    .ctox-context-menu form [data-shiftflow-context-close],
    .ctox-context-menu form [data-app-store-context-close],
    .ctox-context-menu form [data-context-close] {
      width: 30px;
      min-width: 30px;
      padding: 0;
      text-align: center;
      font-size: 18px;
      border: none;
      background: none;
      color: var(--bo-muted, var(--muted, #64747c));
      cursor: pointer;
    }
  `;
  document.head.append(style);
}
