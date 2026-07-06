import { showBusinessConfirm } from '../../shared/dialogs.js';

const state = {
  ctx: null,
  t: (key, fallback) => fallback ?? key,
  appId: '',
  appTitle: '',
  appDesc: '',
  appCategory: '',
  appLayout: '',
  appCollections: [],
  appVersion: '0.1.0',
  contextMenu: null,
  contextMenuCleanup: null,
  resizerCleanup: null,
  catalogSubscription: null,
  commandSubscription: null,
  installedApps: [],
  creatorRequests: [],
  isDeploying: false
};

export function normalizeModuleId(value) {
  return String(value || '')
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9-]/g, '-')
    .replace(/-+/g, '-')
    .replace(/^-|-$/g, '');
}

export function normalizeCollectionName(value) {
  return String(value || '')
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_]/g, '_')
    .replace(/_+/g, '_')
    .replace(/^_|_$/g, '');
}

export function deriveModuleIdFromRequest(request, now = Date.now()) {
  const words = String(request || '')
    .replace(/[^\p{Letter}\p{Number}\s-]/gu, ' ')
    .split(/\s+/)
    .map((word) => word.trim())
    .filter((word) => word.length > 2)
    .slice(0, 5)
    .join(' ');
  const slug = normalizeModuleId(words).slice(0, 60).replace(/-+$/g, '');
  return slug || `business-app-${now}`;
}

export function titleFromModuleId(moduleId) {
  const text = String(moduleId || '').replace(/[-_]+/g, ' ').trim();
  if (!text) return 'Business OS App';
  return text.replace(/\b\w/g, (match) => match.toUpperCase());
}

export function validateCreatorSpec({ appId, appTitle, appDesc, appCollections }) {
  const errors = [];
  if (String(appId || '').trim() && !normalizeModuleId(appId)) errors.push('Modul-ID ist ungültig.');
  if (String(appTitle || '').length > 120) errors.push('Titel ist zu lang.');
  if (String(appDesc || '').length > 500) errors.push('Beschreibung ist zu lang.');
  const collections = Array.isArray(appCollections) ? appCollections.map(normalizeCollectionName).filter(Boolean) : [];
  if (collections.length > 6) errors.push('Zu viele Datentabellen als Vorgabe.');
  return errors;
}

export function computeCreatorActionState({ request, appId, appTitle, appDesc, appCollections, isDeploying = false }) {
  const requestText = String(request || '').trim();
  const validationErrors = validateCreatorSpec({ appId, appTitle, appDesc, appCollections });
  const hasRequest = Boolean(requestText);
  const isBusy = Boolean(isDeploying);
  const deployReady = hasRequest && validationErrors.length === 0 && !isBusy;
  let diagnostic = 'Auftrag fehlt. Beschreibe zuerst die App.';
  if (isDeploying) diagnostic = 'CTOX App-Auftrag läuft.';
  else if (hasRequest && validationErrors.length > 0) diagnostic = validationErrors[0];
  else if (deployReady) diagnostic = 'Bereit. CTOX baut die App aus diesem Auftrag.';

  return { hasRequest, validationErrors, deployReady, diagnostic };
}

export function normalizeCreatorInstalledApps(catalog) {
  const modules = Array.isArray(catalog?.modules) ? catalog.modules : [];
  return modules
    .filter((mod) => {
      const entry = String(mod?.entry || '');
      const source = String(mod?.source || mod?.store?.distribution || '').toLowerCase();
      return mod?.id
        && mod.id !== 'creator'
        && (
          entry.startsWith('installed-modules/')
          || source === 'installed'
          || source.includes('installed-module')
        );
    })
    .map((mod) => ({
      id: normalizeModuleId(mod.id),
      title: String(mod.title || mod.id),
      description: String(mod.description || mod.store?.summary || ''),
      category: String(mod.category || mod.source || 'Custom'),
      version: String(mod.version || '0.1.0'),
      entry: String(mod.entry || ''),
    }))
    .filter((mod) => mod.id)
    .sort((a, b) => a.title.localeCompare(b.title, 'de'));
}

export function normalizeCreatorRequestSuggestions(commands, limit = 5) {
  const items = Array.isArray(commands) ? commands : [];
  return items
    .filter((command) => {
      const payload = command?.payload || {};
      const type = String(command?.command_type || command?.type || '');
      const module = String(command?.module || payload.module || '');
      const source = String(command?.client_context?.source || payload.source || '');
      return module === 'creator'
        || source === 'business-os-creator'
        || type === 'ctox.business_os.app.modify'
        || type === 'business_os.chat.task';
    })
    .map((command) => {
      const payload = command?.payload || {};
      const context = payload.context || {};
      const request = String(payload.instruction || payload.request || payload.user_message || command?.title || '').trim();
      return {
        id: String(command?.id || command?.command_id || `${Date.now()}-${request}`),
        title: String(payload.title || context.app_title || command?.title || 'CTOX App-Auftrag'),
        request,
        status: String(command?.status || 'pending'),
        updated_at_ms: Number(command?.updated_at_ms || command?.created_at_ms || 0),
      };
    })
    .filter((item) => item.request)
    .sort((a, b) => b.updated_at_ms - a.updated_at_ms)
    .slice(0, limit);
}

export async function mount(ctx) {
  state.ctx = ctx;

  // 1. Inject module scoped stylesheet dynamically
  await ensureStyles();

  // 1b. Load locale messages (German markup text is the fallback)
  const messages = await loadCreatorMessages(ctx.locale);
  state.t = (key, fallback) => messages[key] ?? fallback ?? key;

  // 2. Fetch and render raw index.html structure
  const html = await fetch(new URL('./index.html', import.meta.url)).then(res => res.text());
  ctx.host.innerHTML = html;
  applyCreatorTranslations(ctx.host, state.t);

  // 3. Wire UI events
  wireUi(ctx.host);

  // 4. Load catalog-backed right rail data
  await startCreatorDataStreams(ctx, ctx.host);

  // 5. Initialize CTOX unified context menu
  state.contextMenuCleanup = initCreatorContextMenu(state);

  // 6. Setup column resizer
  state.resizerCleanup = setupResizers(ctx.host);

  return () => {
    state.contextMenuCleanup?.();
    state.resizerCleanup?.();
    cleanupSubscription(state.catalogSubscription);
    cleanupSubscription(state.commandSubscription);
    state.catalogSubscription = null;
    state.commandSubscription = null;
    state.contextMenu?.remove();
    state.contextMenu = null;
    console.log('[creator] Module unmounted and cleaned up.');
  };
}

async function ensureStyles() {
  if (document.querySelector('link[data-module-styles="creator"]')) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = new URL('./index.css', import.meta.url).href;
  link.dataset.moduleStyles = 'creator';
  document.head.append(link);
}

function setupResizers(host) {
  // Column resizing is now owned by the shell-global resizer (setupModuleResizers
  // in app.js), which wires the `.ctox-column-resizer[data-resizer-var]` handles in
  // index.html declaratively (drag + keyboard + per-module localStorage). We must
  // NOT DIY-wire them here or each handle gets double-wired. Return a no-op teardown;
  // the mount() call site keeps a valid cleanup reference.
  return () => {};

  // eslint-disable-next-line no-unreachable
  const containerEl = host.querySelector('[data-creator-root]') || host;
  const resizers = [];
  const configs = [
    {
      side: 'left',
      selector: '[data-resizer="left"]',
      cssVar: '--creator-left-width',
      storageKey: 'ctox.creator.layout.leftWidth',
      defaultWidth: 320,
      minWidth: 260,
      maxWidth: 550,
    },
    {
      side: 'right',
      selector: '[data-resizer="right"]',
      cssVar: '--creator-right-width',
      storageKey: 'ctox.creator.layout.rightWidth',
      defaultWidth: 300,
      minWidth: 240,
      maxWidth: 520,
    },
  ];

  for (const config of configs) {
    const resizerEl = host.querySelector(config.selector);
    if (!resizerEl) continue;

    const savedWidth = parseInt(localStorage.getItem(config.storageKey) || '', 10);
    const initialWidth = Number.isFinite(savedWidth) ? savedWidth : config.defaultWidth;
    containerEl.style.setProperty(config.cssVar, `${initialWidth}px`);

    resizers.push(new CtoxResizer({
      resizerEl,
      containerEl,
      cssVar: config.cssVar,
      side: config.side,
      minWidth: config.minWidth,
      maxWidth: config.maxWidth,
      onResize: (width) => {
        localStorage.setItem(config.storageKey, String(Math.round(width)));
      }
    }));
  }

  return () => {
    for (const resizer of resizers) resizer.destroy();
  };
}

async function startCreatorDataStreams(ctx, host) {
  await Promise.allSettled([
    ctx.sync?.startCollection?.('business_module_catalog'),
    ctx.sync?.startCollection?.('business_commands'),
  ]);

  const catalogColl = getCollection(ctx, 'business_module_catalog');
  const commandColl = getCollection(ctx, 'business_commands');

  try {
    const catalogDoc = await catalogColl?.findOne?.('module-catalog')?.exec?.();
    state.installedApps = normalizeCreatorInstalledApps(catalogDoc?.toJSON?.() || {});
  } catch (error) {
    addConsoleLog(`[WARN] Modulkatalog konnte nicht geladen werden: ${error.message}`, 'warning');
  }

  try {
    const commandDocs = await commandColl?.find?.()?.exec?.();
    state.creatorRequests = normalizeCreatorRequestSuggestions(commandDocs?.map((doc) => doc?.toJSON?.() || doc) || []);
  } catch (error) {
    addConsoleLog(`[WARN] CTOX App-Auftraege konnten nicht geladen werden: ${error.message}`, 'warning');
  }

  state.catalogSubscription = catalogColl?.findOne?.('module-catalog')?.$?.subscribe?.((doc) => {
    state.installedApps = normalizeCreatorInstalledApps(doc?.toJSON?.() || {});
    renderCreatorRightRail(host);
  }) || null;

  state.commandSubscription = commandColl?.find?.()?.$?.subscribe?.((docs) => {
    state.creatorRequests = normalizeCreatorRequestSuggestions(docs?.map((doc) => doc?.toJSON?.() || doc) || []);
    renderCreatorRightRail(host);
  }) || null;

  renderCreatorRightRail(host);
}

function getCollection(ctx, name) {
  return ctx.db?.collection?.(name) || null;
}

function cleanupSubscription(subscription) {
  if (typeof subscription === 'function') {
    subscription();
    return;
  }
  subscription?.unsubscribe?.();
}

function renderCreatorRightRail(host) {
  const installedList = host.querySelector('[data-creator-installed-list]');
  const installedEmpty = host.querySelector('[data-creator-installed-empty]');
  const requestsList = host.querySelector('[data-creator-requests-list]');
  const requestsEmpty = host.querySelector('[data-creator-requests-empty]');

  if (installedList && installedEmpty) {
    installedList.innerHTML = state.installedApps.map(renderInstalledAppCard).join('');
    installedEmpty.hidden = state.installedApps.length > 0;
    installedList.hidden = state.installedApps.length === 0;
  }

  if (requestsList && requestsEmpty) {
    requestsList.innerHTML = state.creatorRequests.map(renderCreatorRequestCard).join('');
    requestsEmpty.hidden = state.creatorRequests.length > 0;
    requestsList.hidden = state.creatorRequests.length === 0;
  }
}

function renderInstalledAppCard(app) {
  return `
    <article class="creator-mini-card" data-creator-installed-app="${escapeHtml(app.id)}" data-context-record-id="${escapeHtml(app.id)}" data-context-record-type="application" data-context-label="${escapeHtml(app.title || app.id)}">
      <div class="creator-mini-card-main">
        <strong>${escapeHtml(app.title)}</strong>
        <span>${escapeHtml(app.category)} · ${escapeHtml(app.version)}</span>
        ${app.description ? `<p>${escapeHtml(app.description)}</p>` : ''}
      </div>
      <div class="creator-mini-actions">
        <button type="button" class="ctox-icon-button" data-open-installed-app="${escapeHtml(app.id)}" title="App öffnen" aria-label="${escapeHtml(app.title)} öffnen">
          ${creatorActionIcon('open')}
        </button>
        <button type="button" class="ctox-icon-button" data-upgrade-installed-app="${escapeHtml(app.id)}" title="Upgrade vorbereiten" aria-label="${escapeHtml(app.title)} Upgrade vorbereiten">
          ${creatorActionIcon('upload')}
        </button>
      </div>
    </article>
  `;
}

// Monochrome stroke icon in the shared action-icon style. Falls back to the
// shell-provided ctx.getActionIcon when available (same glyph set).
function creatorActionIcon(name, size = 16) {
  const shellIcon = state.ctx?.getActionIcon?.(name, size);
  if (shellIcon) return shellIcon;
  const paths = {
    open: 'M14 5h5v5M19 5l-8 8M11 5H5v14h14v-6',
    upload: 'M12 15V4M12 4 8 8M12 4l4 4M5 19h14',
    download: 'M12 4v11M12 15l-4-4M12 15l4-4M5 19h14',
    close: 'M6 6l12 12M18 6L6 18',
  };
  const d = paths[name] || paths.close;
  return `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="${d}"/></svg>`;
}

function renderCreatorRequestCard(item) {
  const request = item.request.length > 140 ? `${item.request.slice(0, 137)}...` : item.request;
  return `
    <article class="creator-mini-card" data-creator-request="${escapeHtml(item.id)}" data-context-record-id="${escapeHtml(item.id)}" data-context-record-type="app-request" data-context-label="${escapeHtml(item.title || item.id)}">
      <div class="creator-mini-card-main">
        <strong>${escapeHtml(item.title)}</strong>
        <span>${escapeHtml(item.status)}</span>
        <p>${escapeHtml(request)}</p>
      </div>
      <div class="creator-mini-actions">
        <button type="button" class="ctox-icon-button" data-use-creator-request="${escapeHtml(item.id)}" title="Auftrag uebernehmen" aria-label="Auftrag uebernehmen">
          ${creatorActionIcon('download')}
        </button>
      </div>
    </article>
  `;
}

function wireUi(host) {
  const inputId = host.querySelector('#input-app-id');
  const inputTitle = host.querySelector('#input-app-title');
  const inputDesc = host.querySelector('#input-app-desc');
  const selectCategory = host.querySelector('#select-app-category');
  const selectLayout = host.querySelector('#select-app-layout');
  const btnAddColl = host.querySelector('#btn-add-collection');
  const inputNewColl = host.querySelector('#input-new-collection');
  const btnDeploy = host.querySelector('#btn-deploy-app');
  const inputRequest = host.querySelector('#app-request-input');
  const requestDiagnostics = host.querySelector('#creator-request-diagnostics');
  const syncDot = host.querySelector('#deploy-sync-dot');
  const syncText = host.querySelector('#deploy-sync-text');
  state.isDeploying = false;

  // Accordion Expand/Collapse Trigger
  const accordionTrigger = host.querySelector('#expert-accordion-btn');
  const accordionContent = host.querySelector('#expert-accordion-content');
  const accordionChevron = host.querySelector('.accordion-chevron');
  accordionTrigger.addEventListener('click', () => {
    const isCollapsed = accordionContent.classList.contains('is-collapsed');
    accordionTrigger.setAttribute('aria-expanded', String(isCollapsed));
    if (isCollapsed) {
      accordionContent.classList.remove('is-collapsed');
      accordionChevron.style.transform = 'rotate(180deg)';
    } else {
      accordionContent.classList.add('is-collapsed');
      accordionChevron.style.transform = 'rotate(0deg)';
    }
  });

  const syncStateFromInputs = () => {
    state.appId = normalizeModuleId(inputId.value);
    if (inputId.value !== state.appId) inputId.value = state.appId;
    state.appTitle = inputTitle.value.trim();
    state.appDesc = inputDesc.value.trim();
    state.appCategory = selectCategory.value || '';
    state.appLayout = selectLayout.value || '';

    updateCreatorActionState();
  };

  const updateCreatorActionState = () => {
    const actionState = computeCreatorActionState({
      request: inputRequest.value,
      appId: inputId.value,
      appTitle: inputTitle.value,
      appDesc: inputDesc.value,
      appCollections: state.appCollections,
      isDeploying: state.isDeploying
    });
    btnDeploy.disabled = !actionState.deployReady;
    btnDeploy.setAttribute('aria-disabled', String(btnDeploy.disabled));
    btnDeploy.title = actionState.deployReady
      ? 'App-Erstellung durch CTOX starten'
      : actionState.diagnostic;
    btnDeploy.dataset.state = actionState.deployReady ? 'ready' : 'blocked';
    if (requestDiagnostics) {
      requestDiagnostics.textContent = actionState.diagnostic;
      requestDiagnostics.dataset.state = actionState.deployReady ? 'ready' : actionState.hasRequest ? 'pending' : 'blocked';
    }
    if (!state.isDeploying && syncText && syncDot) {
      syncDot.style.background = '';
      syncText.textContent = actionState.diagnostic;
      syncDot.className = actionState.deployReady ? 'sync-dot is-ready' : 'sync-dot is-blocked';
    }
    return actionState;
  };

  inputRequest.addEventListener('input', () => {
    updateCreatorActionState();
  });

  [inputId, inputTitle, inputDesc, selectCategory, selectLayout].forEach(el => {
    el.addEventListener('input', () => syncStateFromInputs());
  });

  // DB Collection Visual builder in advanced accordion
  const renderCollectionsList = (h) => {
    const listEl = h.querySelector('#collections-list');
    listEl.innerHTML = '';
    state.appCollections.forEach((coll, idx) => {
      const row = document.createElement('div');
      row.className = 'collection-row';
      row.innerHTML = `
        <span style="font-family: var(--font-mono); font-size: 11px; color: var(--accent); flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">${coll}</span>
        <button type="button" class="ctox-icon-button is-danger" data-remove-idx="${idx}" aria-label="Datentabelle ${coll} entfernen" title="Datentabelle entfernen">
          ${creatorActionIcon('close')}
        </button>
      `;
      row.querySelector('[data-remove-idx]').addEventListener('click', async (e) => {
        const removeIdx = parseInt(e.currentTarget.getAttribute('data-remove-idx'), 10);
        const name = state.appCollections[removeIdx];
        const confirmed = await showBusinessConfirm(`Datentabelle "${name}" aus den optionalen Vorgaben entfernen?`, {
          title: 'Datentabelle entfernen',
          confirmLabel: 'Entfernen',
          cancelLabel: 'Abbrechen',
          kind: 'danger'
        });
        if (!confirmed) return;
        state.appCollections.splice(removeIdx, 1);
        renderCollectionsList(h);
        syncStateFromInputs();
      });
      listEl.appendChild(row);
    });
  };

  btnAddColl.addEventListener('click', () => {
    const newName = normalizeCollectionName(inputNewColl.value);
    if (!newName) return;
    if (state.appCollections.includes(newName)) {
      addConsoleLog(`[WARN] Datentabelle '${newName}' existiert bereits.`, 'warning');
      return;
    }
    state.appCollections.push(newName);
    inputNewColl.value = '';
    renderCollectionsList(host);
    syncStateFromInputs();
    addConsoleLog(`[INFO] Datentabelle '${newName}' hinzugefügt.`, 'info');
  });

  inputNewColl.addEventListener('keydown', (event) => {
    if (event.key !== 'Enter') return;
    event.preventDefault();
    btnAddColl.click();
  });

  host.querySelector('[data-creator-right-body]')?.addEventListener('click', (event) => {
    const openButton = event.target.closest('[data-open-installed-app]');
    const upgradeButton = event.target.closest('[data-upgrade-installed-app]');
    const requestButton = event.target.closest('[data-use-creator-request]');

    if (openButton) {
      window.location.hash = `#${encodeURIComponent(openButton.dataset.openInstalledApp || '')}`;
      return;
    }

    if (upgradeButton) {
      window.location.hash = `#creator?upgrade=${encodeURIComponent(upgradeButton.dataset.upgradeInstalledApp || '')}`;
      return;
    }

    if (requestButton) {
      const request = state.creatorRequests.find((item) => item.id === requestButton.dataset.useCreatorRequest);
      if (!request) return;
      inputRequest.value = request.request;
      updateCreatorActionState();
      addConsoleLog(`[INFO] CTOX App-Auftrag '${request.title}' uebernommen.`, 'info');
      inputRequest.focus();
    }
  });

  renderCollectionsList(host);
  updateCreatorActionState();

  // Install / Deploy Button
  btnDeploy.addEventListener('click', async () => {
    try {
      const currentRequest = inputRequest.value.trim();
      if (!currentRequest) {
        state.ctx.notifications.show({
          title: 'Auftrag fehlt',
          message: 'Bitte beschreibe die App, bevor du den CTOX-Auftrag startest.',
          type: 'warning'
        });
        addConsoleLog('[BLOCKED] App-Auftrag verhindert: Beschreibung fehlt.', 'warning');
        updateCreatorActionState();
        return;
      }

      const actionState = updateCreatorActionState();
      if (!actionState.deployReady) {
        addConsoleLog(`[BLOCKED] App-Auftrag verhindert: ${actionState.diagnostic}`, 'warning');
        return;
      }

      const previewCommand = buildAppCreateCommand({
        appId: inputId.value,
        appTitle: inputTitle.value,
        appDesc: inputDesc.value,
        appCategory: selectCategory.value,
        appLayout: selectLayout.value,
        appCollections: state.appCollections,
        appVersion: state.appVersion,
        instruction: currentRequest,
        actor: null,
      });
      const confirmed = await showBusinessConfirm(`CTOX soll die App "${previewCommand.payload.app_title}" (${previewCommand.payload.module_id}) jetzt bauen? Die App wird als runtime-installed Business-OS-Modul erstellt.`, {
        title: 'App-Erstellung starten',
        confirmLabel: 'Starten',
        cancelLabel: 'Abbrechen'
      });
      if (!confirmed) {
        addConsoleLog('[INFO] App-Erstellung abgebrochen. Es wurde kein CTOX-Auftrag angelegt.', 'info');
        return;
      }

      await triggerAppDeployment(host, updateCreatorActionState);
    } catch (e) {
      console.error('[ERROR] triggerAppDeployment failed:', e);
      state.isDeploying = false;
      updateCreatorActionState();
    }
  });

  // Intercept and parse hash parameters for Upgrade preloading
  (async () => {
    const hash = window.location.hash || '';
    const queryStr = hash.includes('?') ? hash.split('?')[1] : '';
    const params = new URLSearchParams(queryStr);
    const upgradeAppId = params.get('upgrade');

    if (upgradeAppId) {
      try {
        addConsoleLog(`[INFO] Lade bestehende App für Änderung von '${upgradeAppId}'...`, 'info');
        const manifestUrl = `installed-modules/${upgradeAppId}/module.json`;
        const manifest = await fetch(manifestUrl).then(res => {
          if (!res.ok) throw new Error(`App '${upgradeAppId}' konnte nicht geladen werden.`);
          return res.json();
        });

        if (inputId) inputId.value = manifest.id || upgradeAppId;
        if (inputTitle) inputTitle.value = manifest.title || '';
        if (inputDesc) inputDesc.value = manifest.description || '';
        if (selectCategory) selectCategory.value = manifest.category || 'Management';
        if (selectLayout) selectLayout.value = manifest.layout?.shell || 'full-workspace';
        if (inputRequest) inputRequest.value = `Ändere ${manifest.title || upgradeAppId}: ${manifest.description || ''}`;
        state.appVersion = /^\d+\.\d+\.\d+$/.test(String(manifest.version || ''))
          ? String(manifest.version)
          : '0.1.0';
        const baseCollections = Array.isArray(manifest.collections) ? manifest.collections : [];
        state.appCollections = baseCollections;

        renderCollectionsList(host);
        syncStateFromInputs();

        addConsoleLog(`[SUCCESS] App-Kontext für '${manifest.title || upgradeAppId}' geladen. Passe den Auftrag an und starte CTOX.`, 'success');
        updateCreatorActionState();
      } catch (err) {
        addConsoleLog(`[ERROR] Fehler beim Laden des Upgrades: ${err.message}`, 'error');
        updateCreatorActionState();
      }
    }
  })();
}

function addConsoleLog(text, type = '') {
  console.log(text);
  const container = document.querySelector('#console-logs-container');
  if (!container) return;
  const el = document.createElement('div');
  el.className = `console-log-entry ${type}`;
  el.textContent = text;
  container.appendChild(el);
  container.scrollTop = container.scrollHeight;
}

export function buildAppCreateCommand({
  appId,
  appTitle,
  appDesc,
  appCategory,
  appLayout,
  appCollections,
  appVersion,
  instruction,
  actor,
  now = Date.now(),
}) {
  const request = String(instruction || appDesc || appTitle || '').trim();
  if (!request) throw new Error('App request is required');
  const moduleId = normalizeModuleId(appId) || deriveModuleIdFromRequest(request, now);
  const collections = Array.isArray(appCollections)
    ? appCollections.map(normalizeCollectionName).filter(Boolean)
    : [];
  const version = /^\d+\.\d+\.\d+$/.test(String(appVersion || '').trim())
    ? String(appVersion).trim()
    : '0.1.0';
  const title = String(appTitle || titleFromModuleId(moduleId)).trim();
  const description = String(appDesc || request.slice(0, 220)).trim();

  return {
    command_id: `app-create-${moduleId}-${now}`,
    module: 'creator',
    type: 'ctox.business_os.app.create',
    command_type: 'ctox.business_os.app.create',
    record_id: moduleId,
    payload: {
      title: `Create ${title}`,
      instruction: request,
      module_id: moduleId,
      app_id: moduleId,
      app_title: title,
      description,
      category: String(appCategory || '').trim(),
      layout_hint: String(appLayout || '').trim(),
      collections_hint: collections,
      desired_version: version,
      install_target: 'runtime-installed-module',
      target: 'app',
      mode: 'app',
      required_skills: ['business-os-app-module-development'],
    },
    client_context: {
      source: 'business-os-creator',
      target: 'app',
      mode: 'app',
      module_id: moduleId,
      app_id: moduleId,
      install_target: 'runtime-installed-module',
      actor: actor || null,
    },
  };
}

async function triggerAppDeployment(host, updateCreatorActionState = () => {}) {
  const syncDot = host.querySelector('#deploy-sync-dot');
  const syncText = host.querySelector('#deploy-sync-text');
  const btnDeploy = host.querySelector('#btn-deploy-app');

  const request = host.querySelector('#app-request-input')?.value?.trim() || '';
  const appId = state.appId;
  const appTitle = state.appTitle;
  const appDesc = state.appDesc;
  const collections = state.appCollections;
  const appLayout = state.appLayout;
  const appVersion = /^\d+\.\d+\.\d+$/.test(String(state.appVersion || '').trim())
    ? String(state.appVersion).trim()
    : '0.1.0';

  if (!request) {
    state.ctx.notifications.show({
      title: 'Fehler beim Vorbereiten',
      message: 'Bitte beschreibe die gewünschte App.',
      type: 'error'
    });
    addConsoleLog('[FEHLER] App-Auftrag fehlt.', 'error');
    return;
  }

  // Visual lock UI
  state.isDeploying = true;
  btnDeploy.disabled = true;
  syncDot.className = 'sync-dot is-saving';
  syncText.textContent = state.t('deploySaving', 'Lege CTOX-Auftrag an...');
  updateCreatorActionState();

  addConsoleLog('==================================================', 'info');
  addConsoleLog('[START] Übergabe an CTOX App Creator Agent...', 'info');

  try {
    const actorContext = (session) => {
      const user = session?.user || {};
      return {
        id: user.id || 'admin',
        display_name: user.display_name || user.name || 'Admin',
        role: user.role || 'admin',
        is_admin: user.is_admin !== undefined ? Boolean(user.is_admin) : true,
      };
    };

    const command = buildAppCreateCommand({
      appId,
      appTitle,
      appDesc,
      appCategory: state.appCategory,
      appLayout,
      appCollections: collections,
      appVersion,
      instruction: request,
      actor: actorContext(state.ctx.session),
    });

    addConsoleLog(`[QUEUE] Sende ${command.command_type} für ${command.payload.module_id}...`, 'info');
    const result = await state.ctx.commandBus.dispatch(command);

    addConsoleLog('==================================================', 'success');
    addConsoleLog(`[SUCCESS] CTOX App-Erstellung für '${command.payload.app_title}' wurde gestartet.`, 'success');
    if (result?.task_id) addConsoleLog(`[TASK] ${result.task_id}`, 'info');

    state.ctx.notifications.show({
      title: 'App-Erstellung gestartet',
      message: `CTOX baut '${command.payload.app_title}' jetzt als Business-OS-App.`,
      type: 'success'
    });

    syncDot.className = 'sync-dot';
    syncText.textContent = state.t('deployInstalled', 'CTOX-Auftrag angelegt');
    state.isDeploying = false;
    updateCreatorActionState();

  } catch (error) {
    addConsoleLog(`[FEHLER] App-Auftrag konnte nicht angelegt werden: ${error.message}`, 'error');
    console.error(error);

    state.ctx.notifications.show({
      title: 'App-Erstellung fehlgeschlagen',
      message: `Der CTOX-Auftrag konnte nicht angelegt werden: ${error.message}`,
      type: 'error'
    });

    syncDot.className = 'sync-dot';
    syncDot.style.background = 'var(--danger)';
    syncText.textContent = state.t('deployFailed', 'Fehler beim Speichern');
    state.isDeploying = false;
    updateCreatorActionState();
  }
}

function initCreatorContextMenu(state) {
  state.contextMenu?.remove();
  const menu = document.createElement('div');
  menu.className = 'ctox-context-menu creator-context-menu';
  menu.hidden = true;
  document.body.append(menu);
  state.contextMenu = menu;

  const handleContextMenu = (event) => {
    if (state.ctx.module?.id !== 'creator') return;
    const context = creatorCommandContextFromElement(state, event.target);
    event.preventDefault();
    event.stopPropagation();
    renderCreatorContextMenu(state, context, event.clientX, event.clientY);
  };
  const handleOutsideClick = (event) => {
    if (state.contextMenu?.contains(event.target)) return;
    hideCreatorContextMenu(state);
  };
  const handleEscape = (event) => {
    if (event.key === 'Escape') hideCreatorContextMenu(state);
  };

  state.ctx.host.addEventListener('contextmenu', handleContextMenu);
  window.addEventListener('click', handleOutsideClick, { capture: true });
  window.addEventListener('keydown', handleEscape);

  return () => {
    state.ctx.host.removeEventListener('contextmenu', handleContextMenu);
    window.removeEventListener('click', handleOutsideClick, { capture: true });
    window.removeEventListener('keydown', handleEscape);
    hideCreatorContextMenu(state);
  };
}

function hideCreatorContextMenu(state) {
  if (state.contextMenu) state.contextMenu.hidden = true;
}

function canModifyCreatorApp(state) {
  if (typeof state.ctx.canModifyModule === 'function' && state.ctx.canModifyModule()) return true;
  const user = state.ctx.session?.user || {};
  const role = String(user.role || (user.is_admin ? 'admin' : 'user')).trim().toLowerCase().replace(/^business_os_/, '');
  return ['admin', 'chef'].includes(role);
}

function creatorCommandContextFromElement(state, target) {
  const element = target?.nodeType === Node.ELEMENT_NODE ? target : target?.parentElement;

  return {
    module: 'creator',
    column: 'workspace',
    record_type: 'app-request',
    record_id: state.appId || 'creator',
    label: state.appTitle || 'Creator App Request',
    app_id: state.appId || '',
    app_title: state.appTitle || '',
    app_desc: state.appDesc || '',
    app_category: state.appCategory || '',
    app_layout: state.appLayout || '',
    app_collections: state.appCollections || [],
    selected_text: String(window.getSelection?.()?.toString?.() || '').trim().slice(0, 1000),
    clicked_text: String(element?.innerText || element?.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 500),
  };
}

function renderCreatorContextMenu(state, context, x, y) {
  ensureCtoxContextMenuStyles();
  const canModifyApp = canModifyCreatorApp(state);
  state.contextMenu.innerHTML = `
    <form class="creator-context-chat" data-creator-context-chat-form>
      <header>
        <div>
          <strong>Chat to CTOX</strong>
          <span>${escapeHtml(context.label || 'Creator')}</span>
        </div>
        <button type="button" data-creator-context-close aria-label="Schließen">×</button>
      </header>
      ${canModifyApp ? `
        <div class="ctox-context-mode" role="radiogroup" aria-label="CTOX Aufgabe">
          <label><input type="radio" name="contextMode" value="data" checked /> Mit Daten arbeiten</label>
          <label><input type="radio" name="contextMode" value="app" /> App modifizieren</label>
        </div>
      ` : ''}
      <textarea data-creator-context-message placeholder="Was soll CTOX mit diesem App-Auftrag tun?"></textarea>
      <footer>
        <span data-creator-context-status></span>
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

  const form = state.contextMenu.querySelector('[data-creator-context-chat-form]');
  const textarea = state.contextMenu.querySelector('[data-creator-context-message]');
  state.contextMenu.querySelector('[data-creator-context-close]')?.addEventListener('click', () => hideCreatorContextMenu(state));
  form?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const mode = canModifyApp ? (new FormData(form).get('contextMode') || 'data') : 'data';
    await dispatchCreatorContextChat(state, context, textarea?.value || '', mode);
  });
  requestAnimationFrame(() => textarea?.focus());
}

async function dispatchCreatorContextChat(state, context, message, mode = 'data') {
  const trimmed = String(message || '').trim();
  const status = state.contextMenu?.querySelector('[data-creator-context-status]');
  if (!trimmed) {
    if (status) status.textContent = 'Nachricht fehlt.';
    return;
  }

  const safeMode = mode === 'app' && canModifyCreatorApp(state) ? 'app' : 'data';
  if (!document.querySelector('[data-ctox-chat-root]')) {
    if (status) status.textContent = 'Chat ist noch nicht bereit.';
    return;
  }
  if (status) status.textContent = 'Oeffne Chat...';
  const title = `${safeMode === 'app' ? 'Creator App modifizieren' : 'App-Auftrag bearbeiten'} · ${context.label || 'Creator'}`;
  const instruction = safeMode === 'app'
    ? `Modifiziere die App-Creator-App anhand dieser Admin-Anweisung. Kontext nur als UI-Bezug verwenden, App-Auftraege selbst nicht als primäres Ziel verändern.\n\n${trimmed}`
    : trimmed;

  window.dispatchEvent(new CustomEvent('ctox-business-os-chat-submit', {
    detail: {
      text: trimmed,
      module: 'creator',
      source_title: 'App Creator',
      command_type: safeMode === 'app' ? 'ctox.business_os.app.modify' : 'business_os.chat.task',
      record_id: safeMode === 'app' ? 'creator' : (context.record_id || 'creator'),
      title,
      instruction,
      payload: {
        title,
        instruction,
        request: trimmed,
        user_message: trimmed,
        mode: safeMode,
        target: safeMode === 'app' ? 'app' : 'data',
        context,
        thread_key: 'business-os/creator',
      },
      client_context: {
        action: 'context-chat',
        mode: safeMode,
        column: context.column,
        record_type: context.record_type,
        app_id: context.app_id,
        app_title: context.app_title,
      },
    },
  }));
  hideCreatorContextMenu(state);
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
    }
    .ctox-context-menu header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      border-bottom: 1px solid var(--bo-border, var(--border, #e5e5ea));
      padding-bottom: 10px;
    }
    .ctox-context-menu header strong {
      font-size: 14px;
      color: var(--bo-text, var(--text, #1c1c1e));
    }
    .ctox-context-menu header span {
      display: block;
      font-size: 11px;
      color: var(--bo-text-muted, var(--text-muted, #8e8e93));
      margin-top: 2px;
    }
    .ctox-context-menu button[type="button"] {
      border: none;
      background: transparent;
      color: var(--bo-text-muted, var(--text-muted, #8e8e93));
      cursor: pointer;
      font-size: 20px;
      line-height: 1;
      padding: 4px 8px;
    }
    .ctox-context-menu .ctox-context-mode {
      display: flex;
      gap: 16px;
      background: var(--bo-surface-2, var(--surface-2, #f2f2f7));
      border-radius: 8px;
      padding: 8px 12px;
    }
    .ctox-context-menu .ctox-context-mode label {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      font-size: 12px;
      font-weight: 500;
      color: var(--bo-text, var(--text, #1c1c1e));
      cursor: pointer;
    }
    .ctox-context-menu textarea {
      width: 100%;
      height: 90px;
      border: 1px solid var(--bo-border, var(--border, #d8e1e5));
      border-radius: 8px;
      background: var(--bo-surface-3, var(--surface-3, #fff));
      color: var(--bo-text, var(--text, #000));
      padding: 8px 12px;
      font-size: 13px;
      font-family: inherit;
      resize: vertical;
    }
    .ctox-context-menu textarea:focus {
      outline: none;
      border-color: var(--bo-accent, var(--accent, #e5a93c));
    }
    .ctox-context-menu footer {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      border-top: 1px solid var(--bo-border, var(--border, #e5e5ea));
      padding-top: 10px;
    }
    .ctox-context-menu footer span {
      font-size: 12px;
      color: var(--bo-accent, var(--accent, #e5a93c));
    }
    .ctox-context-menu footer button[type="submit"] {
      border: none;
      border-radius: 6px;
      background: var(--bo-accent-gradient, var(--accent-gradient, #e5a93c));
      color: #fff;
      font-size: 13px;
      font-weight: 600;
      padding: 6px 16px;
      cursor: pointer;
    }
  `;
  document.head.append(style);
}

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}


// --- Creator module i18n -----------------------------------------------------
// Loads locales/<lang>.json for the creator UI itself (the request templates
// carry their own labels). German markup text is the fallback.
async function loadCreatorMessages(locale) {
  const lang = locale === 'en' ? 'en' : 'de';
  try {
    const response = await fetch(new URL(`./locales/${lang}.json`, import.meta.url));
    if (!response.ok) throw new Error(String(response.status));
    return await response.json();
  } catch {
    return {};
  }
}

function applyCreatorTranslations(root, t) {
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
