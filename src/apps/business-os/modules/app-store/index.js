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
  return () => {
    try { state.unsubscribe?.unsubscribe?.(); } catch {}
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

  document.querySelector('#btn-create-scratch')?.addEventListener('click', () => {
    openModule('creator');
  });
}

async function triggerCardAction(appId, actionType) {
  const item = catalogItems().find((candidate) => candidate.id === appId);
  if (!item || state.busy) return;

  if (actionType === 'install') {
    await installMarketplaceItem(item);
  } else if (actionType === 'open') {
    if (item.id === 'create-scratch') {
      openModule('creator');
    } else {
      openModule(item.id);
    }
  } else if (actionType === 'upgrade') {
    openModule(`creator?upgrade=${item.id}`);
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
    license: manifest.license || 'Apache-2.0',
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

  const scratchTemplate = {
    id: 'create-scratch',
    module_id: 'create-scratch',
    title: 'Neue App per KI-Prompt erstellen',
    description: 'Erstelle eine völlig freie, maßgeschneiderte App über einen einzigen deutschen Prompt.',
    category: 'Templates',
    version: 'v1',
    developer: 'KI Generator',
    license: 'Apache-2.0',
    source: 'creator',
    default_title: 'App von Scratch erstellen',
    collections: [],
    installable: true,
  };

  return [
    normalizeItem(scratchTemplate, 'template'),
    ...state.marketplace.map((item) => normalizeItem(item, 'marketplace')),
    ...templates.map((item) => normalizeItem(item, 'template')),
    ...modules.map((item) => normalizeItem(item, moduleKind(item))),
  ].sort(sortItems);
}

function moduleKind(item) {
  if (item?.core) return 'system';
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
    title: item.title || item.default_title || id,
    description: item.description || '',
    category: String(item.category || item.source || (item.core ? 'System' : 'Local')),
    version: item.version || item.release || 'v1',
    developer: item.developer || item.publisher || 'CTOX',
    license: item.license || 'Apache-2.0',
    source: sourceLabel(item, kind),
    repo: item.repo || item.repository || '',
    download_url: item.download_url || '',
    source_path: item.source_path || '',
    manifest_url: item.manifest_url || '',
    homepage: item.homepage || '',
    permissions: item.permissions || item.collections || [],
    installable: item.installable !== false && item.store?.installable !== false,
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

function filteredItems() {
  return catalogItems().filter((item) => {
    const matchesScope = state.scope === 'all' || item.kind === state.scope || item.status === state.scope;
    const haystack = `${item.title} ${item.description} ${item.category} ${item.repo} ${item.source}`.toLowerCase();
    return matchesScope && (!state.query || haystack.includes(state.query));
  });
}

function render() {
  const items = filteredItems();
  updateScopeButtons();
  renderMarketplaceState();
  renderMessage();
  if (els.title) els.title.textContent = scopeTitle(state.scope);
  if (els.count) els.count.textContent = `${items.length} Apps`;

  if (els.grid) {
    els.grid.className = `store-card-grid ${state.viewMode === 'list' ? 'is-list-view' : ''}`;
    els.grid.replaceChildren(...items.map(renderCard));
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
    state.selectedId = items[0]?.id || '';
  }
  renderDetails();
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
      actionsHtml += `<button type="button" class="card-btn secondary" data-card-action="repository">GitHub</button>`;
    }
  } else if (item.kind === 'template') {
    actionsHtml += `<button type="button" class="card-btn primary" data-card-action="open">Erstellen</button>`;
  } else if (item.kind === 'system') {
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
  if (!item || !state.drawerOpen) {
    if (els.detail) els.detail.classList.remove('visible');
    return;
  }
  if (els.detail) els.detail.classList.add('visible');
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
  if (els.loading) els.loading.hidden = !busy;
  if (els.loadingText) els.loadingText.textContent = text;
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
  const items = catalogItems();
  return {
    all: items.length,
    marketplace: items.filter((item) => item.kind === 'marketplace').length,
    template: items.filter((item) => item.kind === 'template').length,
    installed: items.filter((item) => item.kind === 'installed').length,
    system: items.filter((item) => item.kind === 'system').length,
    local: items.filter((item) => item.kind === 'local').length,
  };
}

function renderMarketplaceState() {
  if (!els.marketplaceState) return;
  els.marketplaceState.textContent = state.marketplaceMessage || `GitHub modules are loaded from ${CTOX_REPO}/${CTOX_APP_ROOT}/modules.`;
  els.marketplaceState.dataset.state = state.marketplaceStatus;
  if (els.refresh) els.refresh.disabled = state.marketplaceStatus === 'loading' || state.busy;
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
  const rank = { marketplace: 0, template: 1, installed: 2, local: 3, system: 4 };
  return (rank[left.kind] ?? 9) - (rank[right.kind] ?? 9)
    || left.title.localeCompare(right.title);
}

function scopeTitle(scope) {
  return {
    all: 'All Applications',
    marketplace: 'GitHub Marketplace',
    template: 'Templates',
    installed: 'Installed Apps',
    system: 'System Apps',
    local: 'Local Modules',
  }[scope] || 'Applications';
}

function iconForItem(item) {
  if (item.kind === 'marketplace') return item.status === 'installed' ? '✓' : 'GH';
  if (item.kind === 'template') return '+';
  if (item.kind === 'installed') return '✓';
  if (item.kind === 'system') return '◆';
  return '*';
}

function statusLabel(status) {
  return {
    available: 'Available',
    installed: 'Installed',
    template: 'Template',
    system: 'System',
    local: 'Local',
  }[status] || status;
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
