import {
  readStoredFileFromDemandChunks,
} from '../../shared/file-integrity.js?v=20260708-canonical-rechunk2';

const BUILD = '20260709-cv-print-parser-v26';
const MODULE_ID = 'cv-print-builder';
const PROFILE_MIME = 'application/vnd.ctox.cv-print-profile+json';
const CHUNK_SIZE = 16 * 1024;
const CONTENT_HASH_SCHEME = 'sha256-bytes-v1';
const CHUNK_HASH_SCHEME = 'sha256-base64-chunk-v1';
const DEMAND_ONLY_SYNC_COLLECTIONS = new Set([
  'desktop_file_chunks',
  'document_blob_chunks',
  'spreadsheet_blob_chunks',
]);
const REQUIRED_COLLECTIONS = [
  'documents',
  'document_versions',
  'desktop_files',
  'business_chats',
  'business_commands',
  'ctox_queue_tasks',
];
const LIVE_COLLECTIONS = [
  ...REQUIRED_COLLECTIONS,
];

const TEMPLATES = [
  { id: 'minimal', label: 'Minimal', description: 'Zeitloses Qualifikationsprofil ohne Schmuck.' },
  { id: 'classic', label: 'Klassisch', description: 'Formales Qualifikationsprofil mit ruhiger Typografie.' },
  { id: 'modern', label: 'Modern', description: 'Qualifikationsprofil mit Logo, Akzentlinie und kompakten Fakten.' },
];

const VIEW_LABELS = {
  original: 'Original',
  split: 'Beide',
  print: 'Print',
};

const COMMON_FIRST_NAMES = new Set([
  'anna',
  'julia',
  'sascha',
  'matthias',
  'simon',
  'mohamed',
  'mohammed',
  'muhammad',
  'ulrich',
  'michael',
  'christian',
  'thomas',
  'stefan',
  'stephan',
  'andreas',
  'martin',
  'sebastian',
  'daniel',
  'jan',
  'jens',
  'tobias',
  'patrick',
  'florian',
  'marco',
  'marcel',
  'niklas',
  'lena',
  'sarah',
  'sara',
  'sandra',
  'katharina',
  'christina',
  'nina',
  'laura',
  'melanie',
  'nadine',
  'anne',
  'maria',
]);

export async function mount(ctx) {
  await ensureStyles();
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  ctx.host.innerHTML = html;
  ctx.host.dataset.cvPrintBuilder = 'native';
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();

  const state = {
    ctx,
    host: ctx.host.querySelector('[data-cv-print-builder]'),
    items: [],
    selectedId: '',
    lastSelectedId: '',
    lastSelectedPhase: '',
    query: '',
    sortMode: 'updated_desc',
    viewMode: 'original',
    originalUrls: new Map(),
    disposers: [],
    refreshTimer: null,
    renderSerial: 0,
    ready: false,
    importing: false,
    bulkParsing: false,
    originalErrors: new Map(),
  };

  bindStaticEvents(state);
  subscribeToCollections(state);
  setModuleBusy(state, true);
  state.ready = await waitForRequiredCollectionsReady(state.ctx, REQUIRED_COLLECTIONS, 120000);
  setModuleBusy(state, !state.ready);
  await refresh(state);
  if (!state.ready) {
    notify(state, 'error', 'CV-Daten noch nicht synchronisiert', 'Import und Parsing bleiben gesperrt, bis die Business-OS Collections bereit sind.');
    scheduleReadinessRetry(state);
  }

  return () => {
    state.disposers.forEach((dispose) => dispose?.());
    state.originalUrls.forEach((url) => URL.revokeObjectURL(url));
    state.originalUrls.clear();
    clearTimeout(state.refreshTimer);
    ctx.host.replaceChildren();
    delete ctx.host.dataset.cvPrintBuilder;
  };
}

async function ensureStyles() {
  const href = `${new URL('./index.css', import.meta.url).pathname}?v=${BUILD}`;
  if (document.querySelector(`link[href="${href}"]`)) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  document.head.append(link);
}

function bindStaticEvents(state) {
  state.host.querySelector('[data-cv-new]')?.addEventListener('click', () => {
    if (!state.ready) {
      notify(state, 'info', 'Synchronisierung läuft', 'CVs können importiert werden, sobald die Arbeitsdaten geladen sind.');
      return;
    }
    state.host.querySelector('[data-cv-upload]')?.click();
  });
  state.host.querySelector('[data-cv-search]')?.addEventListener('input', (event) => {
    state.query = event.target.value || '';
    renderSidebar(state);
  });
  state.host.querySelector('[data-cv-sort]')?.addEventListener('change', (event) => {
    state.sortMode = event.target.value || 'updated_desc';
    renderSidebar(state);
  });
  state.host.querySelector('[data-cv-reparse-all]')?.addEventListener('click', async () => {
    const candidates = reparseCandidates(state);
    if (!state.ready && !candidates.length) {
      notify(state, 'info', 'Synchronisierung läuft', 'Parsing kann starten, sobald die Arbeitsdaten geladen sind.');
      return;
    }
    if (!candidates.length) {
      notify(state, 'info', 'Keine PDFs zum Parsen', 'Es wurden keine CVs mit lokaler PDF-Quelle gefunden.');
      return;
    }
    const confirmed = window.confirm(`${candidates.length} CV-PDF${candidates.length === 1 ? '' : 's'} erneut parsen? Bestehende Parser-Tasks werden nicht gelöscht, die CVs erhalten neue Parser-Aufträge.`);
    if (!confirmed) return;
    await reparseAllPdfs(state, candidates);
  });
  state.host.addEventListener('change', async (event) => {
    if (!event.target?.matches?.('[data-cv-upload]')) return;
    const files = Array.from(event.target.files || []);
    event.target.value = '';
    if (!files.length) return;
    if (!state.ready) {
      notify(state, 'info', 'Synchronisierung läuft', 'Bitte kurz warten, bis die CV-Liste geladen ist.');
      return;
    }
    try {
      await importPdfs(state, files);
    } catch (error) {
      notify(state, 'error', 'PDF konnte nicht importiert werden', String(error?.message || error));
    }
  });
  state.host.querySelector('[data-cv-logo-upload]')?.addEventListener('change', async (event) => {
    const file = event.target.files?.[0];
    event.target.value = '';
    if (!file) return;
    const dataUrl = await fileToDataUrl(file);
    await patchSelectedModel(state, (model) => ({
      ...model,
      print: {
        ...model.print,
        showLogo: true,
        logoDataUrl: dataUrl,
      },
    }));
  });
}

function subscribeToCollections(state) {
  const schedule = () => {
    clearTimeout(state.refreshTimer);
    state.refreshTimer = setTimeout(() => refresh(state), 80);
  };
  LIVE_COLLECTIONS.forEach((name) => {
    const col = getCollection(state.ctx, name);
    const sub = col?.$?.subscribe?.(schedule);
    if (sub?.unsubscribe) state.disposers.push(() => sub.unsubscribe());
  });
}

function setModuleBusy(state, busy) {
  state.host.toggleAttribute('data-cv-busy', Boolean(busy));
  const upload = state.host.querySelector('[data-cv-upload]');
  const newButton = state.host.querySelector('[data-cv-new]');
  if (upload) {
    upload.disabled = Boolean(busy);
    upload.toggleAttribute('disabled', Boolean(busy));
  }
  if (newButton) {
    newButton.disabled = Boolean(busy);
    newButton.toggleAttribute('disabled', Boolean(busy));
  }
  const reparseButton = state.host.querySelector('[data-cv-reparse-all]');
  if (reparseButton) {
    const disabled = Boolean(state.importing || state.bulkParsing) || !reparseCandidates(state).length;
    reparseButton.disabled = disabled;
    reparseButton.toggleAttribute('disabled', disabled);
  }
}

async function waitForRequiredCollectionsReady(ctx, collections, timeoutMs) {
  const localReady = await waitForLocalCollections(ctx, collections, Math.min(timeoutMs, 10000));
  if (!localReady) return false;
  warmRequiredCollectionSync(ctx, collections, timeoutMs);
  return true;
}

async function waitForLocalCollections(ctx, collections, timeoutMs) {
  const startedAt = Date.now();
  while (Date.now() - startedAt < timeoutMs) {
    try {
      requireCollections(ctx, collections);
      return true;
    } catch {
      await delay(250);
    }
  }
  return false;
}

function warmRequiredCollectionSync(ctx, collections, timeoutMs) {
  if (!ctx.sync?.startCollection && !ctx.sync?.leaseCollection) return;
  (async () => {
    const syncHandles = await startScopedSyncCollections(ctx, collections, 'cv-print-builder-warm', { optional: true });
    try {
      await Promise.race([
        Promise.all(syncHandles.handles.map((bridge) => waitForSyncBridgeReady(bridge, Math.min(timeoutMs, 30000)))).catch(() => {}),
        delay(Math.min(timeoutMs, 30000)),
      ]);
    } finally {
      await releaseSyncLeases(syncHandles.leases);
    }
  })().catch(() => {});
}

async function waitForBusinessOsCollectionsReady(collections, timeoutMs) {
  const statusApi = globalThis.CTOX_BUSINESS_OS_STATUS;
  if (!statusApi?.snapshot) return true;
  const startedAt = Date.now();
  while (Date.now() - startedAt < timeoutMs) {
    const status = await statusApi.snapshot({
      includeCounts: false,
      requiredCollections: collections,
      allowRestart: true,
    }).catch(() => null);
    if (isBusinessOsCollectionsReady(status, collections)) return true;
    await delay(750);
  }
  return false;
}

function isBusinessOsCollectionsReady(status, collections = []) {
  const checks = status?.checks || {};
  const entries = status?.sync?.initialSync?.entries || [];
  const entryByCollection = new Map(entries.map((entry) => [entry.collection, entry]));
  const strictCollections = collections.filter((collection) => collection !== 'desktop_file_chunks');
  const initialReplicationComplete = strictCollections.every((collection) => {
    const entry = entryByCollection.get(collection);
    return entry?.state === 'complete' || Boolean(entry?.initialReplicationAt);
  });
  const chunksReady = collections.includes('desktop_file_chunks')
    ? Boolean(entryByCollection.get('desktop_file_chunks')?.streamingReady)
    : true;
  return Boolean(
    checks.requiredCollectionsConnected
      && initialReplicationComplete
      && chunksReady
      && checks.requiredCollectionsCheckpointEpochAdvertised
      && checks.frameTransportRealtimeHealthy
      && checks.noCheckpointProtocolErrors
      && checks.noSchemaProtocolErrors
      && checks.noReplicationIoErrors
      && checks.noFailedCollections
      && checks.noStalledReconnect
  );
}

function scheduleReadinessRetry(state) {
  window.setTimeout(async () => {
    if (!state.host?.isConnected || state.ready) return;
    const ready = await waitForRequiredCollectionsReady(state.ctx, REQUIRED_COLLECTIONS, 30000);
    if (!ready) {
      scheduleReadinessRetry(state);
      return;
    }
    state.ready = true;
    setModuleBusy(state, false);
    await refresh(state);
    notify(state, 'success', 'CV-Daten synchronisiert', 'Import und Parsing sind bereit.');
  }, 3000);
}

async function refresh(state) {
  const [documents, versions, commands, queueTasks] = await Promise.all([
    findAll(state.ctx, 'documents'),
    findAll(state.ctx, 'document_versions'),
    findAll(state.ctx, 'business_commands'),
    findAll(state.ctx, 'ctox_queue_tasks'),
  ]);
  const versionById = new Map(versions.map((item) => [item.id, item]));
  const liveStatus = buildLiveStatusIndex(commands, queueTasks);
  state.items = documents
    .filter((doc) => doc.document_type === 'cv_print_profile' && !doc.is_deleted)
    .map((doc) => {
      const version = versionById.get(doc.current_version_id);
      return {
        record: doc,
        version,
        model: applyLiveWorkflowState(version?.model_json || null, doc, liveStatus),
      };
    })
    .filter((item) => item.model)
    .sort((a, b) => (b.record.updated_at_ms || 0) - (a.record.updated_at_ms || 0));

  if (!state.selectedId && state.items[0]) {
    state.selectedId = state.items[0].record.id;
    state.viewMode = defaultViewMode(state.items[0].model);
  }
  if (state.selectedId && !state.items.some((item) => item.record.id === state.selectedId)) {
    state.selectedId = state.items[0]?.record.id || '';
    state.viewMode = state.items[0] ? defaultViewMode(state.items[0].model) : 'original';
  }
  const selected = state.items.find((item) => item.record.id === state.selectedId);
  if (selected) {
    const phase = workflowPhase(selected.model);
    if (state.lastSelectedId !== selected.record.id || state.lastSelectedPhase !== phase) {
      state.viewMode = defaultViewMode(selected.model);
      state.lastSelectedId = selected.record.id;
      state.lastSelectedPhase = phase;
    } else if (!viewAllowed(selected.model, state.viewMode)) {
      state.viewMode = defaultViewMode(selected.model);
    }
  } else {
    state.lastSelectedId = '';
    state.lastSelectedPhase = '';
  }

  render(state);
}

function render(state) {
  renderSidebar(state);
  renderStage(state);
  setModuleBusy(state, !state.ready || state.importing || state.bulkParsing);
}

function renderSidebar(state) {
  const list = state.host.querySelector('[data-cv-list]');
  const count = state.host.querySelector('[data-cv-count]');
  const visible = visibleItems(state);
  count.textContent = state.query.trim()
    ? `${visible.length}/${state.items.length} CVs`
    : `${state.items.length} CV${state.items.length === 1 ? '' : 's'}`;
  list.innerHTML = visible.length
    ? visible.map((item) => renderCandidateCard(state, item)).join('')
    : '<div class="ctox-empty">Keine Kandidaten gefunden.</div>';
  list.querySelectorAll('[data-cv-select]').forEach((button) => {
    button.addEventListener('click', (event) => {
      if (event.target.closest('[data-cv-action],[data-cv-view],[data-cv-template],[data-cv-toggle-anon],[data-cv-logo-control],[data-cv-open-task]')) return;
      const id = button.dataset.cvSelect;
      const item = state.items.find((candidate) => candidate.record.id === id);
      state.selectedId = id;
      state.viewMode = defaultViewMode(item?.model);
      state.lastSelectedId = id;
      state.lastSelectedPhase = workflowPhase(item?.model);
      render(state);
    });
  });
  list.querySelectorAll('[data-cv-action]').forEach((button) => {
    button.addEventListener('click', async () => {
      const item = getSelectedItem(state);
      if (!item) return;
      const phase = workflowPhase(item.model);
      try {
        if (phase === 'uploaded' || phase === 'error') {
          await startParsing(state, item);
        } else if (phase === 'review') {
          await approvePrint(state, item);
        } else if (phase === 'approved') {
          window.print();
        }
      } catch (error) {
        notify(state, 'error', 'Aktion fehlgeschlagen', String(error?.message || error));
      }
    });
  });
  list.querySelectorAll('[data-cv-open-task]').forEach((button) => {
    button.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
      openCtoxTask(button.dataset.cvTaskId || '', button.dataset.cvCommandId || '');
    });
  });
  list.querySelectorAll('[data-cv-view]').forEach((button) => {
    button.addEventListener('click', async () => {
      const requested = button.dataset.cvView;
      const item = getSelectedItem(state);
      if (!item || !viewAllowed(item.model, requested)) return;
      state.viewMode = requested;
      await patchSelectedModel(state, (model) => ({
        ...model,
        workflow: {
          ...model.workflow,
          view_mode: requested,
          updated_at_ms: Date.now(),
        },
      }), { quiet: true });
      render(state);
    });
  });
  list.querySelectorAll('[data-cv-template]').forEach((select) => {
    select.addEventListener('change', async () => {
      await patchSelectedModel(state, (model) => ({
        ...model,
        print: {
          ...model.print,
          template: select.value,
        },
      }));
    });
  });
  list.querySelectorAll('[data-cv-logo-control]').forEach((button) => {
    button.addEventListener('click', async () => {
      const item = getSelectedItem(state);
      if (!item) return;
      if (!item.model.print?.logoDataUrl) {
        state.host.querySelector('[data-cv-logo-upload]')?.click();
        return;
      }
      await patchSelectedModel(state, (model) => ({
        ...model,
        print: {
          ...model.print,
          showLogo: !model.print?.showLogo,
        },
      }));
    });
  });
  list.querySelectorAll('[data-cv-toggle-anon]').forEach((button) => {
    button.addEventListener('click', async () => {
      await patchSelectedModel(state, (model) => ({
        ...model,
        print: {
          ...model.print,
          anonymize: !model.print?.anonymize,
        },
      }));
    });
  });
}

function visibleItems(state) {
  const query = state.query.trim().toLowerCase();
  const candidates = query
    ? state.items.filter((item) => candidateSearchText(item).includes(query))
    : state.items.slice();
  return candidates.sort((a, b) => compareCandidates(a, b, state.sortMode));
}

function candidateSearchText(item) {
  const model = item.model || {};
  const candidate = model.candidate || {};
  return [
    displayCandidateName(model),
    candidate.currentRole,
    candidate.location,
    candidate.availability,
    model.source?.filename,
    item.record?.filename,
    workflowPhase(model),
    templateLabel(model.print?.template),
  ].filter(Boolean).join(' ').toLowerCase();
}

function compareCandidates(a, b, mode) {
  if (mode === 'name_asc') return displayCandidateName(a.model).localeCompare(displayCandidateName(b.model), 'de', { sensitivity: 'base' });
  if (mode === 'phase_asc') return phaseOrder(a.model) - phaseOrder(b.model)
    || displayCandidateName(a.model).localeCompare(displayCandidateName(b.model), 'de', { sensitivity: 'base' });
  if (mode === 'template_asc') return templateLabel(a.model.print?.template).localeCompare(templateLabel(b.model.print?.template), 'de', { sensitivity: 'base' })
    || displayCandidateName(a.model).localeCompare(displayCandidateName(b.model), 'de', { sensitivity: 'base' });
  return Number(b.record.updated_at_ms || 0) - Number(a.record.updated_at_ms || 0);
}

function phaseOrder(model) {
  return ({ error: 0, uploaded: 1, parsing: 2, review: 3, approved: 4 })[workflowPhase(model)] ?? 9;
}

function buildLiveStatusIndex(commands, queueTasks) {
  const byCommandId = new Map();
  const byRecordId = new Map();
  const tasksById = new Map();
  const tasksByCommandId = new Map();
  commands
    .filter(isCvPrintParseCommandProjection)
    .sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0))
    .forEach((command) => {
      const commandId = command.command_id || command.id;
      if (commandId && !byCommandId.has(commandId)) byCommandId.set(commandId, command);
      const recordId = command.record_id || command.payload?.document_id || command.client_context?.document_id;
      if (recordId && !byRecordId.has(recordId)) byRecordId.set(recordId, command);
    });
  queueTasks.forEach((task) => {
    if (task.id) tasksById.set(task.id, task);
    const commandId = task.command_id || task.business_os_command_id;
    if (commandId && !tasksByCommandId.has(commandId)) tasksByCommandId.set(commandId, task);
  });
  return { byCommandId, byRecordId, tasksById, tasksByCommandId };
}

function isCvPrintParseCommandProjection(command) {
  return command?.module === MODULE_ID
    || command?.client_context?.module === MODULE_ID
    || command?.client_context?.source_module === MODULE_ID
    || command?.payload?.source_module === MODULE_ID
    || command?.payload?.writeback_contract?.command_type === 'ctox.cv_print.apply_parse';
}

function applyLiveWorkflowState(model, record, liveStatus) {
  if (!model) return null;
  const next = structuredClone(model);
  next.workflow = next.workflow || {};
  const commandId = next.workflow.command_id || '';
  const taskId = next.workflow.task_id || '';
  const command = (commandId && liveStatus.byCommandId.get(commandId))
    || liveStatus.byRecordId.get(record.id)
    || null;
  const task = (taskId && liveStatus.tasksById.get(taskId))
    || (command?.task_id && liveStatus.tasksById.get(command.task_id))
    || (command?.command_id && liveStatus.tasksByCommandId.get(command.command_id))
    || null;
  const live = liveWorkflowStatus(command, task);
  if (command?.command_id && !next.workflow.command_id) next.workflow.command_id = command.command_id;
  if (task?.id && !next.workflow.task_id) next.workflow.task_id = task.id;
  if (command?.task_id && !next.workflow.task_id) next.workflow.task_id = command.task_id;
  if (live.status) next.workflow.task_status = live.status;
  if (live.error) next.workflow.error = live.error;

  const phase = workflowPhase(next);
  if (phase === 'parsing' && live.status === 'failed') {
    next.workflow.phase = 'error';
    next.workflow.view_mode = 'original';
  }
  return next;
}

function liveWorkflowStatus(command, task) {
  const rawStatus = [
    command?.status,
    command?.task_status,
    task?.status,
    task?.task_status,
    task?.route_status,
  ].find((value) => String(value || '').trim());
  const status = normalizeLiveStatus(rawStatus);
  const error = [
    command?.error,
    command?.status_note,
    command?.queue_status_note,
    task?.error,
    task?.status_note,
  ].find((value) => String(value || '').trim()) || '';
  return { status, error: String(error || '') };
}

function normalizeLiveStatus(value) {
  const status = String(value || '').trim().toLowerCase();
  if (!status) return '';
  if (['failed', 'error'].includes(status)) return 'failed';
  if (['handled', 'completed', 'complete', 'done'].includes(status)) return 'completed';
  if (['leased', 'running', 'active', 'in_progress'].includes(status)) return 'running';
  if (['pending', 'queued', 'accepted', 'pending_sync'].includes(status)) return 'queued';
  return status;
}

function renderCandidateCard(state, item) {
  const model = item.model;
  const candidate = model.candidate || {};
  const phase = workflowPhase(model);
  const selected = item.record.id === state.selectedId;
  const viewMode = selected ? state.viewMode : defaultViewMode(model);
  const action = actionForPhase(phase);
  const role = candidate.currentRole || 'CV Profil';
  const location = [candidate.location, candidate.availability].filter(Boolean).join(' · ');
  const template = normalizeTemplateId(model.print?.template || 'modern');
  const controlsDisabled = !state.ready || phase === 'uploaded' || phase === 'parsing';
  const approved = phase === 'approved';
  const templateOptions = TEMPLATES.map((entry) => (
    `<option value="${escapeHtml(entry.id)}"${entry.id === template ? ' selected' : ''}>${escapeHtml(entry.label)}</option>`
  )).join('');
  return `
    <article class="cv-card${selected ? ' is-selected' : ''}" data-cv-select="${escapeHtml(item.record.id)}" data-context-record-id="${escapeHtml(item.record.id)}" data-context-record-type="cv_profile" data-context-label="${escapeHtml(item.record.title || item.record.name || item.record.id)}">
      <div class="cv-card-main">
        <span class="ctox-avatar ctox-avatar--lg">${escapeHtml(initials(candidate.name || item.record.title))}</span>
        <div class="cv-card-info">
          <h3 class="cv-card-name">${escapeHtml(displayCandidateName(model))}</h3>
          <div class="cv-card-line cv-card-role">${escapeHtml(role)}</div>
          <div class="cv-card-file">${escapeHtml(model.source?.filename || item.record.filename || 'cv.pdf')}</div>
          <div class="cv-card-line">${escapeHtml(location || phaseLabel(phase))}</div>
        </div>
        <span class="ctox-badge ${phase === 'uploaded' ? 'is-warning' : 'is-info'}">${escapeHtml(statusShort(phase))}</span>
      </div>
      ${selected ? `
        <div class="cv-card-controls">
          <div class="cv-flow-row">
            <button class="cv-icon-btn is-primary" type="button" title="${escapeHtml(action.title)}" aria-label="${escapeHtml(action.title)}" data-cv-action ${phase === 'parsing' ? 'disabled' : ''}>
              ${action.icon}
            </button>
            <div class="ctox-pane-tabs" role="tablist" aria-label="Ansicht">
              ${Object.entries(VIEW_LABELS).map(([key, label]) => `
                <button type="button" data-cv-view="${key}" class="ctox-pane-tab${viewMode === key ? ' is-active' : ''}" ${viewAllowed(model, key) && !approved ? '' : 'disabled'}>${escapeHtml(label)}</button>
              `).join('')}
            </div>
          </div>
          <div class="cv-template-row">
            <select class="ctox-select" data-cv-template ${controlsDisabled || approved ? 'disabled' : ''} aria-label="Print Template">
              ${templateOptions}
            </select>
            <button class="ctox-pane-icon${model.print?.anonymize ? ' is-active' : ''}" type="button" data-cv-toggle-anon title="Anonymisieren" aria-label="Anonymisieren" ${controlsDisabled || approved ? 'disabled' : ''}>${iconEyeOff()}</button>
            <button class="ctox-pane-icon${model.print?.showLogo ? ' is-active' : ''}" type="button" data-cv-logo-control title="${model.print?.logoDataUrl ? 'Logo anzeigen' : 'Logo wählen'}" aria-label="${model.print?.logoDataUrl ? 'Logo anzeigen' : 'Logo wählen'}" ${controlsDisabled || approved ? 'disabled' : ''}>${iconImage()}</button>
          </div>
        </div>
        <div class="cv-card-foot">${phaseFootnoteHtml(model)}</div>
      ` : ''}
    </article>
  `;
}

function renderStage(state) {
  const stage = state.host.querySelector('[data-cv-stage]');
  const item = getSelectedItem(state);
  if (!item) {
    stage.innerHTML = `
      <div class="ctox-empty">
        <div>
          <strong>Kein CV ausgewählt</strong>
          <div>Links einen PDF-CV anlegen.</div>
        </div>
      </div>
    `;
    return;
  }
  const phase = workflowPhase(item.model);
  if (phase === 'approved') state.viewMode = 'print';
  if (!viewAllowed(item.model, state.viewMode)) state.viewMode = defaultViewMode(item.model);
  const serial = ++state.renderSerial;
  const mode = state.viewMode;
  const views = [];
  if (mode === 'original' || mode === 'split') views.push(renderOriginalPane(state, item));
  if (mode === 'print' || mode === 'split') views.push(renderPrintPane(item));
  stage.innerHTML = `<div class="cv-view-grid${mode === 'split' ? ' is-split' : ''}">${views.join('')}</div>`;
  bindEditablePrintFields(state);
  bindFieldEditor(state);
  ensureOriginalUrl(state, item)
    .then(() => {
      if (serial === state.renderSerial) renderOriginalFrame(state, item);
    })
    .catch((error) => {
      const fileId = item.model.source?.desktop_file_id || '';
      if (fileId) state.originalErrors.set(fileId, error?.message || String(error || 'Original-PDF konnte nicht geladen werden.'));
      if (serial === state.renderSerial) renderOriginalFrame(state, item);
    });
}

function renderOriginalPane(state, item) {
  return `
    <section class="cv-pane" data-cv-original-pane>
      <header class="cv-pane-head">
        <strong>Original PDF</strong>
        <span>${escapeHtml(item.model.source?.filename || item.record.filename || '')}</span>
      </header>
      <div class="cv-pane-body" data-cv-original-body>
        <div class="cv-original-placeholder">PDF wird geladen.</div>
      </div>
    </section>
  `;
}

function renderOriginalFrame(state, item) {
  const body = state.host.querySelector('[data-cv-original-body]');
  if (!body) return;
  const fileId = item.model.source?.desktop_file_id || '';
  const error = state.originalErrors.get(fileId);
  if (error) {
    body.innerHTML = `
      <div class="cv-original-placeholder">
        <strong>Original-PDF konnte nicht geladen werden.</strong>
        <span>${escapeHtml(error)}</span>
      </div>
    `;
    return;
  }
  const url = state.originalUrls.get(fileId);
  if (!url) {
    body.innerHTML = '<div class="cv-original-placeholder">Original-PDF ist noch nicht lokal materialisiert.</div>';
    return;
  }
  body.innerHTML = `<iframe class="cv-original-frame" title="Original PDF" src="${escapeAttr(`${url}#toolbar=0&navpanes=0&view=FitH`)}"></iframe>`;
}

function renderPrintPane(item) {
  const model = item.model;
  const template = normalizeTemplateId(model.print?.template || 'modern');
  const editor = workflowPhase(model) === 'review' ? renderFieldEditor(model) : '';
  return `
    <section class="cv-pane">
      <header class="cv-pane-head">
        <strong>Druckansicht</strong>
        <span>${escapeHtml(templateLabel(template))}${workflowPhase(model) === 'review' ? ' · Korrekturmodus' : ''}</span>
      </header>
      <div class="cv-pane-body">
        <div class="cv-print-workarea${editor ? ' has-editor' : ''}">
          ${editor}
          <div class="cv-print-wrap">
          ${renderPrintSheet(model)}
          </div>
        </div>
      </div>
    </section>
  `;
}

function renderPrintSheet(model) {
  const template = normalizeTemplateId(model.print?.template || 'modern');
  if (template === 'minimal') return renderMinimalPrintSheet(model);
  if (template === 'classic') return renderClassicPrintSheet(model);
  return renderModernPrintSheet(model);
}

function renderModernPrintSheet(model) {
  const data = getQualificationPrintData(model);
  return `
    <article class="cv-print-sheet cv-qprofile cv-template-modern">
      ${renderQualificationHeader(data, 'modern')}
      <main class="cv-q-body">
        ${renderQualificationProfile(data)}
        ${renderQualificationMethods(data)}
        ${renderQualificationEntries('BERUFLICHER WERDEGANG / PROJEKTKOMPETENZ', data.career, 'career')}
        ${renderQualificationEntries('AUSBILDUNG', data.education, 'education')}
      </main>
      ${renderQualificationFooter(data)}
    </article>
  `;
}

function renderMinimalPrintSheet(model) {
  const data = getQualificationPrintData(model, { minimalLogo: true });
  return `
    <article class="cv-print-sheet cv-qprofile cv-template-minimal">
      ${renderQualificationHeader(data, 'minimal')}
      <main class="cv-q-body">
        ${renderQualificationProfile(data)}
        ${renderQualificationMethods(data)}
        ${renderQualificationEntries('BERUFLICHER WERDEGANG / PROJEKTKOMPETENZ', data.career, 'career')}
        ${renderQualificationEntries('AUSBILDUNG', data.education, 'education')}
      </main>
      ${renderQualificationFooter(data)}
    </article>
  `;
}

function renderClassicPrintSheet(model) {
  const data = getQualificationPrintData(model, { classicLogo: true });
  return `
    <article class="cv-print-sheet cv-qprofile cv-template-classic">
      ${renderQualificationHeader(data, 'classic')}
      <main class="cv-q-body">
        ${renderQualificationProfile(data)}
        ${renderQualificationMethods(data)}
        ${renderQualificationEntries('BERUFLICHER WERDEGANG / PROJEKTKOMPETENZ', data.career, 'career')}
        ${renderQualificationEntries('AUSBILDUNG', data.education, 'education')}
      </main>
      ${renderQualificationFooter(data)}
    </article>
  `;
}

function getQualificationPrintData(model, options = {}) {
  const candidate = model.candidate || {};
  const phase = workflowPhase(model);
  const editable = phase === 'review';
  const anonymize = Boolean(model.print?.anonymize);
  const logo = model.print?.showLogo ? renderLogo(model, options) : '';
  const name = anonymize ? anonymizedName(candidate.name) : displayCandidateName(model);
  const additional = candidate.additional || [];
  const cvMeta = {
    ...(candidate.cv?.meta || {}),
    ...(additionalValue(additional, 'cv.meta') || {}),
  };
  const role = candidate.currentRole || candidate.desiredPosition || cvMeta.currentPosition || 'CV Profil';
  const facts = buildQualificationFacts(candidate, cvMeta, anonymize);
  const skills = normalizeCvSkillsObject(candidate);
  const methods = buildMethodsFromCvSkills(skills, {
    ...cvMeta,
    languages: normalizeLanguageItems(candidate.languages || cvMeta.languages || []),
  });
  const career = normalizeExperienceToCareerEntries(normalizeTimeline(additional, 'cv.experience'));
  const education = normalizeEducationToEntries(normalizeTimeline(additional, 'cv.education'));
  const footer = {
    userName: 'Steffen Ratschan',
    companyLine: 'pmX GmbH | Kegelenstr. 3 | 70372 Stuttgart',
    contactLine: anonymize ? 'Anonymisiert' : contactLine(candidate),
  };
  return { candidate, editable, anonymize, logo, name, role, facts, methods, career, education, footer };
}

function renderQualificationHeader(data, theme) {
  const logo = theme === 'minimal' ? '' : data.logo;
  return `
    <header class="cv-q-head">
      <div>
        <h1>QUALIFIKATIONSPROFIL</h1>
        <div class="cv-q-rule"></div>
      </div>
      ${logo}
    </header>
  `;
}

function renderQualificationProfile(data) {
  return `
    <section class="cv-q-profile">
      <div class="cv-q-avatar">${escapeHtml(initials(data.name))}</div>
      <div>
        <h2>${escapeHtml(data.name)}</h2>
        <div class="cv-q-role">${escapeHtml(data.role)}</div>
        <dl class="cv-q-facts">
          ${data.facts.map((fact) => `
            <div>
              <dt>${escapeHtml(fact.label)}:</dt>
              <dd>${escapeHtml(fact.value || '-')}</dd>
            </div>
          `).join('')}
        </dl>
      </div>
    </section>
  `;
}

function renderQualificationMethods(data) {
  return `
    <section class="cv-q-section">
      <h3>METHODEN- / SYSTEMKOMPETENZ / SPRACHKENNTNISSE</h3>
      <dl class="cv-q-methods">
        ${data.methods.length ? data.methods.map((item) => `
          <div>
            <dt>${escapeHtml(item.label)}</dt>
            <dd>${escapeHtml(item.value)}</dd>
          </div>
        `).join('') : '<div><dt>Kompetenzen</dt><dd>Nach Parsing korrigieren</dd></div>'}
      </dl>
    </section>
  `;
}

function renderQualificationEntries(title, entries, kind) {
  return `
    <section class="cv-q-section">
      <h3>${escapeHtml(title)}</h3>
      <div class="cv-q-entries">
        ${entries.length ? entries.map((entry) => renderQualificationEntry(entry, kind)).join('') : `
          <article class="cv-q-entry">
            <div class="cv-q-when">-</div>
            <div><strong>Keine Einträge hinterlegt</strong></div>
          </article>
        `}
      </div>
    </section>
  `;
}

function renderQualificationEntry(entry, kind) {
  const title = kind === 'education' ? entry.title : entry.title;
  const org = kind === 'education' ? entry.org : entry.employer;
  const bullets = Array.isArray(entry.bullets) ? entry.bullets : [];
  const extra = Array.isArray(entry.extra) ? entry.extra : [];
  return `
    <article class="cv-q-entry">
      <div class="cv-q-when">${escapeHtml([entry.from, entry.to].filter(Boolean).join(' - ') || '-')}</div>
      <div class="cv-q-entry-main">
        <strong>${escapeHtml(title || 'Station')}</strong>
        ${org ? `<span>${escapeHtml(org)}</span>` : ''}
        ${extra.map((item) => `<em>${escapeHtml(item)}</em>`).join('')}
        ${bullets.length ? `<ul>${bullets.map((bullet) => `<li>${escapeHtml(bullet.text || bullet)}</li>`).join('')}</ul>` : ''}
      </div>
    </article>
  `;
}

function renderQualificationFooter(data) {
  const line = data.footer.companyLine || '';
  return `
    <footer class="cv-q-foot">
      <div>
        <strong>${escapeHtml(data.footer.userName || '-')}</strong>
        <span>${escapeHtml(line)}</span>
        <span>${escapeHtml(data.footer.contactLine || '-')}</span>
      </div>
      <div>${escapeHtml(new Date().toLocaleDateString('de-DE'))}</div>
    </footer>
  `;
}

function buildQualificationFacts(candidate, cvMeta, anonymize) {
  const facts = [];
  const highestDegree = candidate.highestDegree || cvMeta.highestDegree || '';
  const degree = candidate.degree || cvMeta.degree || '';
  const birthDate = candidate.birthDate || cvMeta.birthDate || '';
  const nationality = candidate.nationality || cvMeta.nationality || '';
  const availability = candidate.availability || cvMeta.availabilityFrom || '';
  if (highestDegree) facts.push({ label: 'Höchster Abschluss', value: highestDegree });
  if (degree) facts.push({ label: 'Fachrichtung', value: degree });
  if (birthDate) facts.push({ label: anonymize ? 'Geburtsjahr' : 'Geburtsdatum', value: anonymize ? yearFromDateText(birthDate) : formatCvDisplayDate(birthDate) });
  if (nationality) facts.push({ label: 'Nationalität', value: nationality });
  if (availability) facts.push({ label: 'Verfügbarkeit', value: formatCvDisplayDate(availability) });
  if (!facts.length) facts.push({ label: 'Profil', value: 'Felder prüfen' });
  return facts;
}

function normalizeExperienceToCareerEntries(experience) {
  return (Array.isArray(experience) ? experience : []).map((entry) => {
    const title = firstNonEmpty(entry?.job_title, entry?.title, entry?.role, entry?.jobTitle, entry?.position) || 'Position';
    const employer = firstNonEmpty(entry?.employer, entry?.company, entry?.companyName, entry?.org) || '';
    const from = firstNonEmpty(entry?.start_date, entry?.startDate, entry?.start, entry?.from);
    const to = firstNonEmpty(entry?.end_date, entry?.endDate, entry?.end, entry?.to) || 'heute';
    const bullets = valueToLines(entry?.job_description || entry?.description || entry?.summary)
      .slice(0, 12)
      .map((text) => ({ text }));
    return { from, to, title, employer, bullets };
  });
}

function normalizeEducationToEntries(education) {
  return (Array.isArray(education) ? education : []).map((entry) => {
    const title = firstNonEmpty(entry?.degree, entry?.title, entry?.program) || 'Ausbildung';
    const org = firstNonEmpty(entry?.institution, entry?.school, entry?.university, entry?.provider, entry?.org) || '';
    const from = firstNonEmpty(entry?.start_date, entry?.startDate, entry?.start, entry?.from);
    const to = firstNonEmpty(entry?.end_date, entry?.endDate, entry?.end, entry?.to) || 'dato';
    const extra = [entry?.major, entry?.location, entry?.specialization].filter((item) => String(item || '').trim());
    const bullets = valueToLines(entry?.details || entry?.description || entry?.summary)
      .slice(0, 8)
      .map((text) => ({ text }));
    return { from, to, title, org, extra, bullets };
  });
}

function buildMethodsFromCvSkills(cvSkills, candidateMeta) {
  const out = [];
  const fach = cvSkills.Fachkenntnisse || cvSkills.fachkenntnisse || cvSkills.skills;
  const sprachen = cvSkills.Sprachkenntnisse || cvSkills.sprachkenntnisse || cvSkills.languages;
  const other = cvSkills['Weitere Fähigkeiten'] || cvSkills['Weitere Faehigkeiten'] || cvSkills.other_skills;
  if (fach) out.push({ label: 'Fachkenntnisse', value: valueToLines(fach).join(', ') });
  if (other) out.push({ label: 'Weitere Fähigkeiten', value: valueToLines(other).join(', ') });
  if (sprachen) out.push({ label: 'Sprachkenntnisse', value: valueToLines(sprachen).join(', ') });
  if (!sprachen) {
    const languageText = normalizeLanguageItems(candidateMeta?.languages || [])
      .map((item) => [item.label || item.code || item.language || item.name, item.level].filter(Boolean).join(' '))
      .filter(Boolean)
      .join(', ');
    if (languageText) out.push({ label: 'Sprachkenntnisse', value: languageText });
  }
  return out.filter((item) => item.value);
}

function renderFieldEditor(model) {
  const candidate = model.candidate || {};
  const cv = getCvEditorData(model);
  return `
    <aside class="cv-field-editor" data-cv-field-editor>
      <header>
        <strong>Felder</strong>
        <span>Ninja CV-Profil</span>
      </header>
      <section class="cv-editor-block">
        <h4>Stammdaten</h4>
        <div class="cv-editor-grid">
          ${renderEditorInput('Name', 'candidate.name', displayCandidateName(model))}
          ${renderEditorInput('Rolle', 'candidate.currentRole', candidate.currentRole || '')}
          ${renderEditorInput('Ort', 'candidate.location', candidate.location || '')}
          ${renderEditorInput('Verfügbarkeit', 'candidate.availability', candidate.availability || '')}
          ${renderEditorInput('Abschluss', 'candidate.highestDegree', candidate.highestDegree || '')}
          ${renderEditorInput('Fachrichtung', 'candidate.degree', candidate.degree || '')}
          ${renderEditorInput('Geburtsdatum', 'candidate.birthDate', candidate.birthDate || '')}
          ${renderEditorInput('Nationalität', 'candidate.nationality', candidate.nationality || '')}
        </div>
      </section>
      <section class="cv-editor-block">
        <div class="cv-editor-title-row">
          <h4>Beruflicher Werdegang</h4>
          <button type="button" data-cv-editor-action="add-experience">+</button>
        </div>
        <div data-cv-editor-list="experience">
          ${renderExperienceEditorRows(cv.experience)}
        </div>
      </section>
      <section class="cv-editor-block">
        <div class="cv-editor-title-row">
          <h4>Ausbildung</h4>
          <button type="button" data-cv-editor-action="add-education">+</button>
        </div>
        <div data-cv-editor-list="education">
          ${renderEducationEditorRows(cv.education)}
        </div>
      </section>
      <section class="cv-editor-block">
        <h4>Skills</h4>
        ${renderSkillEditor('Fachkenntnisse', cv.skills.Fachkenntnisse || cv.skills.skills || [])}
        ${renderSkillEditor('Sprachkenntnisse', cv.skills.Sprachkenntnisse || cv.skills.languages || [])}
        ${renderSkillEditor('Weitere Fähigkeiten', cv.skills['Weitere Fähigkeiten'] || cv.skills['Weitere Faehigkeiten'] || cv.skills.other_skills || [])}
      </section>
    </aside>
  `;
}

function renderEditorInput(label, path, value) {
  return `
    <label>
      <span>${escapeHtml(label)}</span>
      <input data-cv-field-path="${escapeAttr(path)}" value="${escapeAttr(value || '')}" />
    </label>
  `;
}

function renderExperienceEditorRows(items) {
  if (!items.length) return '<div class="cv-editor-empty">Keine Stationen.</div>';
  return items.map((item, index) => `
    <article class="cv-editor-row" data-cv-entry-kind="experience" data-cv-index="${index}">
      <button type="button" data-cv-editor-action="remove-experience" data-cv-index="${index}" aria-label="Station entfernen">x</button>
      <input data-cv-entry-field="job_title" value="${escapeAttr(firstNonEmpty(item.job_title, item.title, item.role, item.jobTitle))}" placeholder="Position" />
      <input data-cv-entry-field="employer" value="${escapeAttr(firstNonEmpty(item.employer, item.company, item.companyName))}" placeholder="Arbeitgeber" />
      <input data-cv-entry-field="start_date" value="${escapeAttr(firstNonEmpty(item.start_date, item.startDate, item.start, item.from))}" placeholder="Start" />
      <input data-cv-entry-field="end_date" value="${escapeAttr(firstNonEmpty(item.end_date, item.endDate, item.end, item.to))}" placeholder="Ende" />
      <textarea data-cv-entry-field="job_description" rows="2" placeholder="Aufgaben / Erfolge">${escapeHtml(valueToLines(item.job_description || item.description || item.summary).join('\n'))}</textarea>
    </article>
  `).join('');
}

function renderEducationEditorRows(items) {
  if (!items.length) return '<div class="cv-editor-empty">Keine Ausbildung.</div>';
  return items.map((item, index) => `
    <article class="cv-editor-row" data-cv-entry-kind="education" data-cv-index="${index}">
      <button type="button" data-cv-editor-action="remove-education" data-cv-index="${index}" aria-label="Ausbildung entfernen">x</button>
      <input data-cv-entry-field="degree" value="${escapeAttr(firstNonEmpty(item.degree, item.title, item.program))}" placeholder="Abschluss" />
      <input data-cv-entry-field="institution" value="${escapeAttr(firstNonEmpty(item.institution, item.school, item.university, item.provider))}" placeholder="Institution" />
      <input data-cv-entry-field="major" value="${escapeAttr(item.major || '')}" placeholder="Hauptfach" />
      <input data-cv-entry-field="specialization" value="${escapeAttr(item.specialization || '')}" placeholder="Schwerpunkt" />
      <input data-cv-entry-field="start_date" value="${escapeAttr(firstNonEmpty(item.start_date, item.startDate, item.start, item.from))}" placeholder="Start" />
      <input data-cv-entry-field="end_date" value="${escapeAttr(firstNonEmpty(item.end_date, item.endDate, item.end, item.to))}" placeholder="Ende" />
      <textarea data-cv-entry-field="details" rows="2" placeholder="Details">${escapeHtml(valueToLines(item.details || item.description || item.summary).join('\n'))}</textarea>
    </article>
  `).join('');
}

function renderSkillEditor(label, value) {
  return `
    <label class="cv-editor-skill">
      <span>${escapeHtml(label)}</span>
      <textarea data-cv-skill-group="${escapeAttr(label)}" rows="2">${escapeHtml(valueToLines(value).join('\n'))}</textarea>
    </label>
  `;
}

function bindFieldEditor(state) {
  const editor = state.host.querySelector('[data-cv-field-editor]');
  if (!editor) return;
  editor.querySelectorAll('[data-cv-field-path]').forEach((input) => {
    input.addEventListener('blur', async () => {
      const path = input.dataset.cvFieldPath;
      const value = input.value.trim();
      await patchSelectedModel(state, (model) => syncCandidateMetaAdditional(setPath({ ...model }, path, value)));
    });
  });
  editor.querySelectorAll('[data-cv-entry-field]').forEach((input) => {
    input.addEventListener('blur', async () => {
      await patchSelectedModel(state, (model) => patchCvEditorData(model, readCvEditorData(editor)));
    });
  });
  editor.querySelectorAll('[data-cv-skill-group]').forEach((input) => {
    input.addEventListener('blur', async () => {
      await patchSelectedModel(state, (model) => patchCvEditorData(model, readCvEditorData(editor)));
    });
  });
  editor.querySelectorAll('[data-cv-editor-action]').forEach((button) => {
    button.addEventListener('click', async () => {
      await patchSelectedModel(state, (model) => applyEditorButtonAction(model, button.dataset.cvEditorAction, Number(button.dataset.cvIndex || -1)));
    });
  });
}

function getCvEditorData(model) {
  const candidate = model.candidate || {};
  return {
    experience: normalizeTimeline(candidate.additional, 'cv.experience'),
    education: normalizeTimeline(candidate.additional, 'cv.education'),
    skills: normalizeCvSkillsObject(candidate),
  };
}

function readCvEditorData(editor) {
  const experience = Array.from(editor.querySelectorAll('[data-cv-entry-kind="experience"]')).map((row) => ({
    job_title: row.querySelector('[data-cv-entry-field="job_title"]')?.value.trim() || '',
    employer: row.querySelector('[data-cv-entry-field="employer"]')?.value.trim() || '',
    start_date: row.querySelector('[data-cv-entry-field="start_date"]')?.value.trim() || '',
    end_date: row.querySelector('[data-cv-entry-field="end_date"]')?.value.trim() || '',
    job_description: valueToLines(row.querySelector('[data-cv-entry-field="job_description"]')?.value || ''),
  })).filter((row) => Object.values(row).some((value) => Array.isArray(value) ? value.length : value));
  const education = Array.from(editor.querySelectorAll('[data-cv-entry-kind="education"]')).map((row) => ({
    degree: row.querySelector('[data-cv-entry-field="degree"]')?.value.trim() || '',
    institution: row.querySelector('[data-cv-entry-field="institution"]')?.value.trim() || '',
    major: row.querySelector('[data-cv-entry-field="major"]')?.value.trim() || '',
    specialization: row.querySelector('[data-cv-entry-field="specialization"]')?.value.trim() || '',
    start_date: row.querySelector('[data-cv-entry-field="start_date"]')?.value.trim() || '',
    end_date: row.querySelector('[data-cv-entry-field="end_date"]')?.value.trim() || '',
    details: valueToLines(row.querySelector('[data-cv-entry-field="details"]')?.value || ''),
  })).filter((row) => Object.values(row).some((value) => Array.isArray(value) ? value.length : value));
  const skills = {};
  editor.querySelectorAll('[data-cv-skill-group]').forEach((field) => {
    const key = field.dataset.cvSkillGroup;
    const values = valueToLines(field.value || '');
    if (key && values.length) skills[key] = values;
  });
  return { experience, education, skills };
}

function applyEditorButtonAction(model, action, index) {
  const candidate = { ...(model.candidate || {}) };
  const cv = getCvEditorData(model);
  if (action === 'add-experience') cv.experience.push({ job_title: '', employer: '', start_date: '', end_date: '', job_description: [] });
  if (action === 'add-education') cv.education.push({ degree: '', institution: '', major: '', specialization: '', start_date: '', end_date: '', details: [] });
  if (action === 'remove-experience' && index >= 0) cv.experience.splice(index, 1);
  if (action === 'remove-education' && index >= 0) cv.education.splice(index, 1);
  return patchCvEditorData({ ...model, candidate }, cv);
}

function patchCvEditorData(model, cv) {
  const candidate = { ...(model.candidate || {}) };
  candidate.additional = upsertAdditionalValue(candidate.additional, 'cv.experience', 'Berufserfahrung (CV)', cv.experience || []);
  candidate.additional = upsertAdditionalValue(candidate.additional, 'cv.education', 'Ausbildung (CV)', cv.education || []);
  candidate.additional = upsertAdditionalValue(candidate.additional, 'cv.skills', 'Skills (CV)', cv.skills || {});
  candidate.skills = valueToLines(cv.skills?.Fachkenntnisse || cv.skills?.skills || []);
  candidate.languages = valueToLines(cv.skills?.Sprachkenntnisse || cv.skills?.languages || []).map((label) => ({ label }));
  return syncCandidateMetaAdditional({ ...model, candidate });
}

function syncCandidateMetaAdditional(model) {
  const candidate = { ...(model.candidate || {}) };
  const currentMeta = additionalValue(candidate.additional, 'cv.meta') || {};
  const nextMeta = {
    ...currentMeta,
    birthDate: candidate.birthDate || '',
    nationality: candidate.nationality || '',
    highestDegree: candidate.highestDegree || '',
    degree: candidate.degree || '',
    availabilityFrom: candidate.availability || currentMeta.availabilityFrom || '',
    languages: normalizeLanguageItems(candidate.languages || currentMeta.languages || []),
  };
  candidate.additional = upsertAdditionalValue(candidate.additional, 'cv.meta', 'Stammdaten (CV)', nextMeta);
  return { ...model, candidate };
}

function upsertAdditionalValue(additional, key, label, value) {
  const list = Array.isArray(additional) ? structuredClone(additional) : [];
  const index = list.findIndex((item) => item?.key === key);
  const entry = { key, label, type: 'json', value };
  if (index >= 0) list[index] = { ...list[index], ...entry };
  else list.push(entry);
  return list;
}

function normalizeCvSkillsObject(candidate) {
  const raw = additionalValue(candidate.additional, 'cv.skills') || candidate.cv?.skills || {};
  if (raw && typeof raw === 'object' && !Array.isArray(raw)) return structuredClone(raw);
  const skills = {};
  const fach = Array.isArray(candidate.skills) ? candidate.skills : Array.isArray(raw) ? raw : [];
  if (fach.length) skills.Fachkenntnisse = fach.map(labelOf).filter(Boolean);
  const languages = normalizeLanguageItems(candidate.languages || []);
  if (languages.length) {
    skills.Sprachkenntnisse = languages.map((item) => [item.label || item.code || item.language || item.name, item.level].filter(Boolean).join(' ')).filter(Boolean);
  }
  return skills;
}

function normalizeLanguageItems(value) {
  if (!Array.isArray(value)) return [];
  return value.map((item) => {
    if (typeof item === 'string') return { label: item };
    if (!item || typeof item !== 'object') return null;
    return { ...item, label: item.label || item.language || item.name || item.code || '' };
  }).filter((item) => item && String(item.label || item.code || '').trim());
}

function valueToLines(value) {
  if (Array.isArray(value)) return value.map(labelOf).map((item) => String(item || '').trim()).filter(Boolean);
  if (typeof value === 'string') return value.split(/\r?\n|;|•|\u2022/g).map((item) => item.trim()).filter(Boolean);
  if (value && typeof value === 'object') return Object.values(value).flatMap(valueToLines);
  return [];
}

function firstNonEmpty(...values) {
  for (const value of values) {
    const text = String(value || '').trim();
    if (text) return text;
  }
  return '';
}

function formatCvDisplayDate(value) {
  const text = String(value || '').trim();
  if (!text) return '';
  const iso = text.match(/^(\d{4})-(\d{2})-(\d{2})$/);
  if (iso) return `${iso[3]}.${iso[2]}.${iso[1]}`;
  return text;
}

function yearFromDateText(value) {
  return String(value || '').match(/\b(19\d{2}|20\d{2}|2100)\b/)?.[1] || '';
}

function bindEditablePrintFields(state) {
  state.host.querySelectorAll('[data-cv-edit-field]').forEach((node) => {
    node.addEventListener('blur', async () => {
      const path = node.dataset.cvEditField;
      const value = node.textContent.trim();
      await patchSelectedModel(state, (model) => setPath({ ...model }, path, value), { quiet: true });
    });
  });
}

async function importPdfs(state, files) {
  const pdfs = files.filter((file) => /^application\/pdf$/i.test(file.type) || /\.pdf$/i.test(file.name));
  if (!pdfs.length) throw new Error('Bitte mindestens eine PDF-Datei auswählen.');
  const failures = [];
  const imported = [];
  state.importing = true;
  setModuleBusy(state, true);
  try {
    for (const file of pdfs) {
      try {
        imported.push(await importPdf(state, file, { refresh: false, select: false }));
      } catch (error) {
        failures.push(`${file.name}: ${error?.message || error}`);
      }
    }
    if (imported.length) {
      const selectedId = imported[imported.length - 1].documentId;
      state.selectedId = selectedId;
      state.lastSelectedId = '';
      state.lastSelectedPhase = '';
      state.viewMode = 'original';
      await refresh(state);
      if (state.selectedId !== selectedId) {
        state.selectedId = selectedId;
        state.lastSelectedId = '';
        state.lastSelectedPhase = '';
        await refresh(state);
      }
    }
  } finally {
    state.importing = false;
    setModuleBusy(state, !state.ready);
  }
  if (failures.length) {
    throw new Error(failures.join('\n'));
  }
  notify(state, 'success', pdfs.length === 1 ? 'PDF importiert' : `${pdfs.length} PDFs importiert`, 'Originalansicht ist bereit. Parsing kann pro Kandidat gestartet werden.');
}

async function importPdf(state, file, options = {}) {
  if (!/^application\/pdf$/i.test(file.type) && !/\.pdf$/i.test(file.name)) {
    throw new Error('Bitte eine PDF-Datei auswählen.');
  }
  requireCollections(state.ctx, ['documents', 'document_versions', 'desktop_files', 'desktop_file_chunks']);
  const bytes = new Uint8Array(await file.arrayBuffer());
  const sha = await sha256Hex(bytes);
  const now = Date.now();
  const documentId = `cv_${crypto.randomUUID()}`;
  const versionId = `${documentId}_v1`;
  const fileId = `desktop_file_${documentId}`;
  const generationId = `${fileId}_g1`;
  const base64 = uint8ToBase64(bytes);
  const identity = candidateIdentityFromFilename(file.name);
  const title = identity.name;
  const model = createInitialModel({
    documentId,
    versionId,
    fileId,
    generationId,
    filename: file.name,
    title,
    identity,
    sha,
    size: file.size,
    now,
  });

  await saveDesktopFile(state.ctx, {
    fileId,
    generationId,
    documentId,
    filename: file.name,
    mimeType: file.type || 'application/pdf',
    size: file.size,
    sha,
    base64,
    now,
  });

  await getCollection(state.ctx, 'document_versions').insert({
    id: versionId,
    document_id: documentId,
    version: 1,
    source_kind: 'cv_pdf_upload',
    blob_id: fileId,
    model_json: model,
    diagnostics: [],
    created_at_ms: now,
    updated_at_ms: now,
  });

  await getCollection(state.ctx, 'documents').insert({
    id: documentId,
    title,
    filename: file.name,
    mime_type: PROFILE_MIME,
    status: 'uploaded',
    document_type: 'cv_print_profile',
    owner_id: '',
    current_version_id: versionId,
    source_sha256: sha,
    page_count: 0,
    diagnostics_count: 0,
    linked_records: [
      { collection: 'desktop_files', id: fileId, role: 'source_pdf' },
    ],
    display_cache: {
      candidate_name: title,
      phase: 'uploaded',
      template: 'minimal',
    },
    index_text: `${title} ${file.name}`,
    is_deleted: false,
    created_at_ms: now,
    updated_at_ms: now,
  });

  state.originalUrls.set(fileId, URL.createObjectURL(file));
  if (options.select !== false) {
    state.selectedId = documentId;
    state.viewMode = 'original';
  }
  if (options.refresh !== false) await refresh(state);
  return { documentId, fileId };
}

async function saveDesktopFile(ctx, input) {
  await getCollection(ctx, 'desktop_files').insert({
    id: input.fileId,
    parent_id: '',
    path: '',
    local_path: '',
    virtual_path: `/cv-print-builder/${input.filename}`,
    name: input.filename,
    kind: 'file',
    mime_type: input.mimeType,
    extension: 'pdf',
    size_bytes: input.size,
    owner_id: '',
    source: MODULE_ID,
    linked_collection: 'documents',
    linked_record_id: input.documentId,
    content_ref: '',
    content_state: 'available',
    content_hash: input.sha,
    content_hash_scheme: CONTENT_HASH_SCHEME,
    content_generation_id: input.generationId,
    mtime_ms: input.now,
    content_synced_at_ms: input.now,
    sort_index: input.now,
    is_deleted: false,
    created_at_ms: input.now,
    updated_at_ms: input.now,
  });
  const total = Math.ceil(input.base64.length / CHUNK_SIZE) || 1;
  const chunkRows = await Promise.all(Array.from({ length: total }, async (_, idx) => {
    const data = input.base64.slice(idx * CHUNK_SIZE, (idx + 1) * CHUNK_SIZE);
    const chunkHash = await sha256Hex(new TextEncoder().encode(data));
    return {
      id: canonicalDesktopFileChunkId(input.fileId, input.generationId, idx),
      file_id: input.fileId,
      generation_id: input.generationId,
      content_hash: input.sha,
      content_hash_scheme: CONTENT_HASH_SCHEME,
      idx,
      total,
      encoding: 'base64',
      data,
      chunk_hash: chunkHash,
      chunk_hash_scheme: CHUNK_HASH_SCHEME,
      size_bytes: data.length,
      created_at_ms: Date.now(),
    };
  }));
  await writeChunkDocuments(getCollection(ctx, 'desktop_file_chunks'), chunkRows);
}

async function startParsing(state, item) {
  if (!state.ctx.commandBus?.dispatch) {
    throw new Error('CTOX commandBus ist nicht verfügbar. CV Parsing muss als Business-OS Chat-Task gestartet werden.');
  }
  requireCollections(state.ctx, ['business_chats', 'documents', 'document_versions', 'desktop_files', 'desktop_file_chunks']);
  const now = Date.now();
  const chatId = `chat_cv_${item.record.id}`;
  const commandId = `cmd_${crypto.randomUUID()}`;
  const taskInstruction = buildParserTaskInstruction(item);
  const markParsing = (model) => ({
    ...model,
    workflow: {
      ...model.workflow,
      phase: 'parsing',
      view_mode: 'original',
      chat_id: chatId,
      command_id: commandId,
      task_id: '',
      task_status: 'queued',
      error: '',
      updated_at_ms: now,
    },
  });
  item.model = markParsing(structuredClone(item.model));
  state.viewMode = 'original';
  state.lastSelectedId = item.record.id;
  state.lastSelectedPhase = 'parsing';
  render(state);
  let parsingPatchApplied = false;
  try {
    await patchItemModel(state, item, markParsing, {
      documentStatus: 'parsing',
      displayPhase: 'parsing',
    });
    parsingPatchApplied = true;
  } catch (error) {
    console.warn('[cv-print-builder] optimistic parsing status patch failed; dispatching task anyway', error);
  }
  if (parsingPatchApplied) await refresh(state);

  try {
    const sourcePrepare = await ensureParseSourceReady(state, item);
    if (!sourcePrepare.generation_id) {
      throw new Error(sourcePrepare.warning || 'PDF-Daten konnten nicht lokal vorbereitet werden.');
    }
    if (!sourcePrepare.ready) {
      console.warn('[cv-print-builder] dispatching parser task with native source fallback', {
        fileId: sourcePrepare.file_id,
        generationId: sourcePrepare.generation_id,
        warning: sourcePrepare.warning || '',
      });
    }
    await upsertBusinessChat(state.ctx, {
      id: chatId,
      title: `CV Parsing · ${displayCandidateName(item.model)}`,
      message: taskInstruction,
      now,
    });
    const dispatchResult = await dispatchCvParserCommand(state, {
      id: commandId,
      module: MODULE_ID,
      command_type: 'business_os.chat.task',
      type: 'business_os.chat.task',
      record_id: item.record.id,
      inbound_channel: MODULE_ID,
      sync_collections: [
        'desktop_file_chunks',
        'desktop_files',
        'documents',
        'document_versions',
        'business_chats',
        'business_commands',
        'ctox_queue_tasks',
      ],
      payload: {
        title: `CV strukturieren: ${item.model.source?.filename || item.record.filename}`,
        instruction: taskInstruction,
        chat_id: chatId,
        message_id: `msg_${crypto.randomUUID()}`,
        conversation: [],
        inbound_channel: MODULE_ID,
        source_module: MODULE_ID,
        skill: 'ctox-cv-print-parser',
        skill_id: 'ctox-cv-print-parser',
        source_file_id: item.model.source?.desktop_file_id,
        generation_id: item.model.source?.generation_id,
        filename: item.model.source?.filename || item.record.filename,
        mime_type: 'application/pdf',
        size_bytes: item.model.source?.size_bytes || 0,
        sha256: item.model.source?.sha256 || '',
        document_id: item.record.id,
        version_id: item.version.id,
        source_prepare: sourcePrepare,
        record_snapshot: parserRecordSnapshot(item),
        writeback_contract: {
          command_type: 'ctox.cv_print.apply_parse',
          target_collection: 'document_versions',
          document_id: item.record.id,
          expected_model_schema: 'ctox.cv_print_profile.v1',
        },
      },
      client_context: {
        surface: 'business-os-cv-print-builder',
        action: 'parse_cv_pdf',
        module: MODULE_ID,
        document_id: item.record.id,
        version_id: item.version.id,
        desktop_file_id: item.model.source?.desktop_file_id,
        chat_id: chatId,
        source_prepare_ready: sourcePrepare.ready,
        source_prepare_warning: sourcePrepare.warning || '',
      },
    });
    const taskId = String(dispatchResult?.task_id || dispatchResult?.taskId || '').trim();
    const taskStatus = String(dispatchResult?.task_status || dispatchResult?.status || 'queued').trim();
    warmRequiredCollectionSync(state.ctx, [
      'desktop_file_chunks',
      'desktop_files',
      'business_chats',
      'business_commands',
      'ctox_queue_tasks',
    ], 30000);
    await patchItemModel(state, item, (model) => ({
      ...model,
      workflow: {
        ...model.workflow,
        phase: 'parsing',
        chat_id: chatId,
        command_id: commandId,
        task_id: taskId || model.workflow?.task_id || '',
        task_status: taskStatus || model.workflow?.task_status || 'queued',
        updated_at_ms: Date.now(),
      },
    }), {
      documentStatus: 'parsing',
      displayPhase: 'parsing',
    });
  } catch (error) {
    await patchItemModel(state, item, (model) => ({
      ...model,
      workflow: {
        ...model.workflow,
        phase: 'error',
        chat_id: chatId,
        command_id: model.workflow?.command_id || commandId,
        task_status: 'dispatch_failed',
        error: error?.message || String(error),
        updated_at_ms: Date.now(),
      },
    }), {
      documentStatus: 'error',
      displayPhase: 'error',
    });
    throw error;
  } finally {
    state.viewMode = 'original';
    await refresh(state);
  }
}

function reparseCandidates(state) {
  return state.items.filter((item) => {
    const source = item.model?.source || {};
    return Boolean(item.record?.id && item.version?.id && source.desktop_file_id);
  });
}

async function reparseAllPdfs(state, candidates = reparseCandidates(state)) {
  const originalSelection = state.selectedId;
  const originalViewMode = state.viewMode;
  const failures = [];
  let started = 0;
  state.bulkParsing = true;
  setModuleBusy(state, true);
  try {
    for (const candidate of candidates) {
      const item = state.items.find((entry) => entry.record.id === candidate.record.id) || candidate;
      try {
        state.selectedId = item.record.id;
        await startParsing(state, item);
        started += 1;
      } catch (error) {
        failures.push(`${displayCandidateName(item.model)}: ${error?.message || error}`);
      }
    }
  } finally {
    state.bulkParsing = false;
    state.selectedId = state.items.some((item) => item.record.id === originalSelection)
      ? originalSelection
      : state.items[0]?.record.id || '';
    state.viewMode = originalViewMode || 'original';
    await refresh(state);
    setModuleBusy(state, !state.ready);
  }
  if (failures.length) {
    notify(state, 'error', `${started}/${candidates.length} Parser-Tasks gestartet`, failures.slice(0, 3).join('\n'));
    return;
  }
  notify(state, 'success', `${started} Parser-Tasks gestartet`, 'Alle CV-PDFs wurden erneut an CTOX uebergeben.');
}

async function dispatchCvParserCommand(state, command) {
  try {
    return await state.ctx.commandBus.dispatch({
      ...command,
      wait_timeout_ms: 90000,
    });
  } catch (error) {
    const message = String(error?.message || error || '');
    const commandId = error?.command_id || command.id || command.command_id || '';
    if (commandId && /wartet noch auf die Rueckmeldung/i.test(message)) {
      return {
        ok: true,
        command_id: commandId,
        status: 'pending_sync',
        task_id: '',
        task_status: 'syncing',
        transport: 'rxdb-command-bus-timeout',
      };
    }
    throw error;
  }
}

async function ensureParseSourceReady(state, item) {
  const source = item.model?.source || {};
  const fileId = source.desktop_file_id || '';
  const generationId = source.generation_id || '';
  if (!fileId) {
    throw new Error('CV-Quelle ist unvollständig: PDF-Datei fehlt.');
  }
  if (!generationId) {
    return {
      ready: false,
      file_id: fileId,
      generation_id: '',
      warning: 'CV-Quelle hat keine Generation-ID.',
    };
  }
  try {
    await Promise.race([
      Promise.resolve()
        .then(() => verifyDesktopFileSourceAvailable(state.ctx, {
          fileId,
          generationId,
          contentHash: source.sha256 || '',
        }))
        .then(() => flushFileCollectionsForDispatch(state.ctx, 60000)),
      timeoutAfter(60000, 'PDF-Sync-Vorbereitung konnte nicht innerhalb von 60s abgeschlossen werden.'),
    ]);
    return { ready: true, file_id: fileId, generation_id: generationId };
  } catch (error) {
    const warning = `PDF-Daten konnten nicht lokal vorbereitet werden: ${String(error?.message || error || 'PDF-Sync-Vorbereitung ist fehlgeschlagen.')}`;
    console.warn('[cv-print-builder] parse source preparation did not complete before dispatch', {
      fileId,
      generationId,
      warning,
    });
    return {
      ready: false,
      file_id: fileId,
      generation_id: generationId,
      warning,
    };
  }
}

function parserRecordSnapshot(item) {
  const model = item.model || {};
  const candidate = model.candidate || {};
  return {
    document_id: item.record?.id || '',
    version_id: item.version?.id || '',
    source: {
      desktop_file_id: model.source?.desktop_file_id || '',
      generation_id: model.source?.generation_id || '',
      filename: model.source?.filename || item.record?.filename || '',
      mime_type: model.source?.mime_type || 'application/pdf',
      size_bytes: model.source?.size_bytes || 0,
      sha256: model.source?.sha256 || '',
    },
    candidate: {
      name: displayCandidateName(model),
      firstName: candidate.firstName || '',
      lastName: candidate.lastName || '',
      currentRole: candidate.currentRole || '',
      location: candidate.location || '',
      availability: candidate.availability || '',
      highestDegree: candidate.highestDegree || '',
      degree: candidate.degree || '',
    },
    workflow: {
      phase: workflowPhase(model),
      template: normalizeTemplateId(model.print?.template || 'minimal'),
    },
    field_contract: {
      schema: 'ctox.cv_print_profile.v1',
      additional_keys: ['cv.experience', 'cv.education', 'cv.skills', 'cv.meta'],
      reference: 'NinjaWorkflowTool_Extension/find-job-for-candidate qualification profile',
    },
  };
}

async function syncFileCollections(ctx) {
  if (!ctx.sync?.startCollection && !ctx.sync?.leaseCollection) return;
  const syncHandles = await startScopedSyncCollections(
    ctx,
    ['desktop_files', 'desktop_file_chunks'],
    'cv-print-builder-file-sync',
  );
  try {
    await Promise.all(syncHandles.handles.map((bridge) => waitForSyncBridgeReady(bridge, 15000, { allowPush: true })));
  } finally {
    await releaseSyncLeases(syncHandles.leases);
  }
}

async function flushFileCollectionsForDispatch(ctx, timeoutMs = 30000) {
  if (!ctx.sync?.startCollection && !ctx.sync?.leaseCollection) return;
  const syncHandles = await startScopedSyncCollections(
    ctx,
    ['desktop_files', 'desktop_file_chunks'],
    'cv-print-builder-dispatch-flush',
  );
  try {
    await Promise.all(syncHandles.handles.map((bridge) => waitForSyncBridgeReady(bridge, timeoutMs, { allowPush: true })));
  } finally {
    await releaseSyncLeases(syncHandles.leases);
  }
}

async function waitForSyncBridgeReady(bridge, timeoutMs = 10000, options = {}) {
  const bridgeState = syncBridgeFromHandle(bridge)?.state;
  if (!bridgeState) return;
  const runWithTimeout = (promise) => Promise.race([
    Promise.resolve(promise).catch(() => {}),
    delay(timeoutMs),
  ]);
  await Promise.race([
    Promise.resolve()
      .then(() => bridgeState.awaitInSync?.() || bridgeState.awaitInitialReplication?.())
      .catch(() => {}),
    delay(timeoutMs),
  ]);
  if (options.allowPush && typeof bridgeState.pushToRemotePeers === 'function') {
    await runWithTimeout(bridgeState.pushToRemotePeers());
  } else if (options.allowPush && typeof bridgeState.awaitInSync === 'function') {
    await runWithTimeout(bridgeState.awaitInSync());
  }
}

async function startScopedSyncCollections(ctx, collections, reason, options = {}) {
  const leases = [];
  const handles = [];
  for (const collection of collections || []) {
    try {
      const handle = await startScopedSyncCollection(ctx.sync, collection, reason, leases);
      if (handle) handles.push(handle);
    } catch (error) {
      if (!options.optional) throw error;
    }
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

async function releaseSyncLeases(leases) {
  await Promise.all((leases || []).map((lease) => lease?.release?.().catch(() => null)));
}

function syncBridgeFromHandle(handle) {
  return handle?.bridge || handle;
}

async function verifyDesktopFileSourceAvailable(ctx, { fileId, generationId, contentHash = '' }) {
  await readDesktopFileFromDemand(ctx, fileId, 'application/pdf', {
    contentGenerationId: generationId,
    contentHash,
    contentHashScheme: contentHash ? CONTENT_HASH_SCHEME : '',
  });
}

async function writeChunkDocuments(collection, rows) {
  const docs = Array.isArray(rows) ? rows.filter(Boolean) : [];
  if (!docs.length) return;
  if (typeof collection.bulkUpsert === 'function') {
    await collection.bulkUpsert(docs);
    return;
  }
  for (const doc of docs) {
    if (typeof collection.upsert === 'function') {
      await collection.upsert(doc);
      continue;
    }
    const existing = await collection.findOne(doc.id).exec().catch(() => null);
    if (existing?.incrementalPatch) {
      await existing.incrementalPatch(doc);
    } else {
      await collection.insert(doc);
    }
  }
}

function canonicalDesktopFileChunkId(fileId, generationId, idx) {
  return `${fileId}_${generationId}_${idx}`;
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function timeoutAfter(ms, message) {
  return new Promise((_, reject) => setTimeout(() => reject(new Error(message)), ms));
}

function buildParserPrompt(item) {
  return buildParserTaskInstruction(item);
}

function buildParserTaskInstruction(item) {
  const source = item.model.source || {};
  return [
    'Skill: `ctox-cv-print-parser`.',
    'Strukturiere den CTOX-vorextrahierten Abschnitt "CV PDF extracted text" als Ninja-kompatibles Qualifikationsprofil.',
    'Ausfuehrung nur ueber CTOX desktop_files/desktop_file_chunks, RxDB/WebRTC und CTOX PDF-Stack.',
    'Keine Tools, keine Shell, kein PDF erneut oeffnen, keine HTTP- oder Ninja-Services.',
    'Antwort exakt als ein minifiziertes JSON-Objekt, kein Markdown, keine Analyse.',
    'Schema: {"schema":"ctox.cv_print_profile.v1","workflow":{"phase":"review","diagnostics":[]},',
    '"candidate":{name,firstName,lastName,currentRole,location,availability,email,phone,birthDate,nationality,highestDegree,degree,languages[],skills[],additional[]}}.',
    'candidate.additional muss exakt diese CV-Keys enthalten:',
    '- cv.experience: Array mit {job_title,employer,location,start_date,end_date,job_description[]}.',
    '- cv.education: Array mit {degree,institution,major,specialization,location,start_date,end_date,details[]}.',
    '- cv.skills: Objekt mit Gruppen, mindestens Fachkenntnisse[], Sprachkenntnisse[], Weitere Fähigkeiten[].',
    '- cv.meta: Objekt mit birthDate,nationality,highestDegree,degree,availabilityFrom,languages[],source_filename.',
    'Alle erkennbaren Stationen behalten; nicht künstlich auf wenige Einträge kürzen.',
    'Fehlende Werte leer lassen, nichts erfinden.',
    `- document_id: ${item.record.id}`,
    `- version_id: ${item.version.id}`,
    `- desktop_file_id: ${source.desktop_file_id || ''}`,
    `- filename: ${source.filename || item.record.filename || ''}`,
    'Der CTOX Writeback `ctox.cv_print.apply_parse` persistiert das JSON als neue document_versions-Version.',
  ].join('\n');
}

async function approvePrint(state, item) {
  const approveModel = (model) => ({
    ...model,
    workflow: {
      ...model.workflow,
      phase: 'approved',
      view_mode: 'print',
      approved_at_ms: Date.now(),
      updated_at_ms: Date.now(),
    },
  });
  await patchItemModel(state, item, approveModel, {
    documentStatus: 'approved',
    displayPhase: 'approved',
  });
  const approvedModel = approveModel(structuredClone(item.model));
  item.model = approvedModel;
  if (item.version) {
    item.version.model_json = approvedModel;
    item.version.diagnostics = approvedModel.workflow?.diagnostics || item.version.diagnostics || [];
    item.version.updated_at_ms = Date.now();
  }
  item.record.status = 'approved';
  item.record.display_cache = {
    ...(item.record.display_cache || {}),
    phase: 'approved',
    candidate_name: displayCandidateName(approvedModel),
    template: normalizeTemplateId(approvedModel.print?.template || 'minimal'),
  };
  item.record.updated_at_ms = Date.now();
  state.viewMode = 'print';
  state.lastSelectedId = item.record.id;
  state.lastSelectedPhase = 'approved';
  render(state);
}

async function upsertBusinessChat(ctx, input) {
  const col = getCollection(ctx, 'business_chats');
  const existing = await col.findOne(input.id).exec();
  const message = {
    id: `msg_${crypto.randomUUID()}`,
    role: 'user',
    content: input.message,
    createdAt: input.now,
    source_module: MODULE_ID,
  };
  if (existing?.incrementalPatch) {
    const json = existing.toJSON();
    await existing.incrementalPatch({
      title: input.title,
      open: true,
      minimized: false,
      messages: [...(json.messages || []), message],
      draft: '',
      updated_at_ms: input.now,
    });
    return;
  }
  await col.insert({
    id: input.id,
    title: input.title,
    open: true,
    minimized: false,
    owner_user_id: '',
    lastTrackingId: '',
    messages: [message],
    draft: '',
    createdAt: input.now,
    updated_at_ms: input.now,
  });
}

async function readDesktopFileFromDemand(ctx, fileId, mimeType = 'application/octet-stream', options = {}) {
  const chunks = await fetchDesktopFileDemandChunks(ctx, fileId);
  return readStoredFileFromDemandChunks(chunks, mimeType, options);
}

async function fetchDesktopFileDemandChunks(ctx, fileId) {
  const loader = await fileDemandLoaderFor(ctx).catch(() => null);
  if (!loader?.fetchFile) {
    throw new Error('PDF-Daten sind noch nicht über den Sync-Demand-Pfad verfügbar.');
  }
  return loader.fetchFile(fileId);
}

async function fileDemandLoaderFor(ctx) {
  if (!ctx?.sync?.startCollection) return null;
  const bridge = await ctx.sync.startCollection('desktop_files');
  await waitForSyncBridgeReady(bridge, 15000);
  return bridge?.state?.demandFileLoader || null;
}

async function ensureOriginalUrl(state, item) {
  const fileId = item.model.source?.desktop_file_id;
  const generationId = item.model.source?.generation_id || '';
  if (!fileId || state.originalUrls.has(fileId)) return;
  const source = item.model.source || {};
  const blob = await readDesktopFileFromDemand(state.ctx, fileId, 'application/pdf', {
    contentGenerationId: generationId,
    contentHash: source.sha256 || '',
    contentHashScheme: source.sha256 ? CONTENT_HASH_SCHEME : '',
  });
  state.originalErrors.delete(fileId);
  state.originalUrls.set(fileId, URL.createObjectURL(blob));
}

async function patchSelectedModel(state, updater, options = {}) {
  const item = getSelectedItem(state);
  if (!item) return;
  await patchItemModel(state, item, updater, options);
  if (!options.quiet) await refresh(state);
}

async function patchItemModel(state, item, updater, options = {}) {
  const now = Date.now();
  const versionDoc = await getCollection(state.ctx, 'document_versions').findOne(item.version.id).exec();
  if (!versionDoc?.incrementalPatch) throw new Error('CV Version konnte nicht aktualisiert werden.');
  const versionJson = versionDoc.toJSON ? versionDoc.toJSON() : {};
  const current = structuredClone(versionJson.model_json || item.model);
  const next = updater(current);
  next.workflow = {
    ...next.workflow,
    updated_at_ms: now,
  };
  await versionDoc.incrementalPatch({
    model_json: next,
    diagnostics: next.workflow?.diagnostics || versionJson.diagnostics || item.version.diagnostics || [],
    updated_at_ms: now,
  });
  const documentDoc = await getCollection(state.ctx, 'documents').findOne(item.record.id).exec();
  if (documentDoc?.incrementalPatch) {
    await documentDoc.incrementalPatch({
      title: displayCandidateName(next),
      status: options.documentStatus || documentDoc.toJSON().status || workflowPhase(next),
      diagnostics_count: (next.workflow?.diagnostics || []).length,
      display_cache: {
        ...(documentDoc.toJSON().display_cache || {}),
        candidate_name: displayCandidateName(next),
        phase: options.displayPhase || workflowPhase(next),
        template: normalizeTemplateId(next.print?.template || 'minimal'),
      },
      index_text: buildIndexText(next),
      updated_at_ms: now,
    });
  }
}

function createInitialModel(input) {
  return {
    schema: 'ctox.cv_print_profile.v1',
    source: {
      desktop_file_id: input.fileId,
      generation_id: input.generationId,
      filename: input.filename,
      mime_type: 'application/pdf',
      size_bytes: input.size,
      sha256: input.sha,
      imported_at_ms: input.now,
    },
    candidate: {
      id: input.documentId,
      name: input.title,
      firstName: input.identity?.firstName || '',
      lastName: input.identity?.lastName || '',
      currentRole: '',
      desiredPosition: '',
      location: '',
      availability: '',
      email: '',
      phone: '',
      highestDegree: '',
      degree: '',
      nationality: '',
      birthDate: '',
      languages: [],
      skills: [],
      softSkills: [],
      summary: '',
      additional: [
        { key: 'cv.education', label: 'Ausbildung (CV)', value: [] },
        { key: 'cv.experience', label: 'Berufserfahrung (CV)', value: [] },
        { key: 'cv.skills', label: 'Skills (CV)', value: {} },
        { key: 'cv.meta', label: 'Stammdaten (CV)', value: {} },
      ],
    },
    print: {
      template: 'minimal',
      anonymize: false,
      showLogo: true,
      logoDataUrl: '',
      preset: 'standard',
      overrides: {},
    },
    workflow: {
      phase: 'uploaded',
      view_mode: 'original',
      chat_id: '',
      command_id: '',
      task_id: '',
      task_status: '',
      diagnostics: [],
      created_at_ms: input.now,
      updated_at_ms: input.now,
    },
  };
}

function getCollection(ctx, name) {
  const collection = ctx?.db?.collection?.(name);
  if (!collection) throw new Error(`Business-OS Collection fehlt: ${name}`);
  return collection;
}

async function findAll(ctx, name) {
  const collection = ctx?.db?.collection?.(name);
  if (!collection?.find) return [];
  const docs = await collection.find().exec();
  return docs.map((doc) => doc?.toJSON ? doc.toJSON() : doc);
}

function requireCollections(ctx, names) {
  names.forEach((name) => getCollection(ctx, name));
}

function getSelectedItem(state) {
  return state.items.find((item) => item.record.id === state.selectedId) || null;
}

function workflowPhase(model) {
  return model?.workflow?.phase || 'uploaded';
}

function defaultViewMode(model) {
  const phase = workflowPhase(model);
  if (phase === 'review') return model.workflow?.view_mode || 'split';
  if (phase === 'approved') return 'print';
  return 'original';
}

function viewAllowed(model, view) {
  const phase = workflowPhase(model);
  if (phase === 'approved') return view === 'print';
  if (view === 'original') return true;
  return phase === 'review';
}

function actionForPhase(phase) {
  if (phase === 'review') return { title: 'Print freigeben', icon: iconCheck() };
  if (phase === 'approved') return { title: 'Drucken', icon: iconPrinter() };
  return { title: 'Parsing starten', icon: iconPlay() };
}

function phaseLabel(phase) {
  return ({
    uploaded: 'PDF geladen',
    parsing: 'Parsing läuft',
    review: 'Korrektur',
    approved: 'Freigegeben',
    error: 'Fehler',
  })[phase] || phase;
}

function statusShort(phase) {
  return ({
    uploaded: 'PDF',
    parsing: 'Parsing',
    review: 'Korrektur',
    approved: 'Freigabe',
    error: 'Fehler',
  })[phase] || 'CV';
}

function phaseFootnoteHtml(model) {
  const phase = workflowPhase(model);
  if (phase === 'uploaded') return 'PDF geladen. Als Nächstes Parsing starten.';
  if (phase === 'parsing') {
    if (model.workflow?.task_status === 'syncing') return 'PDF wird an CTOX synchronisiert. Flow wird angelegt.';
    const tracking = taskTracking(model);
    if (!tracking.taskId && !tracking.commandId) return 'Task läuft. Flow wird angelegt.';
    const label = tracking.taskId ? shortTaskId(tracking.taskId) : shortTaskId(tracking.commandId);
    return `Task läuft · <button class="cv-task-link" type="button" data-cv-open-task data-cv-task-id="${escapeAttr(tracking.taskId)}" data-cv-command-id="${escapeAttr(tracking.commandId)}" title="${escapeAttr([tracking.taskId, tracking.commandId].filter(Boolean).join(' · '))}">Flow öffnen <span>${escapeHtml(label)}</span></button>`;
  }
  if (phase === 'review') return `${escapeHtml(templateLabel(model.print?.template))} · Korrektur und Template prüfen.`;
  if (phase === 'approved') return `${escapeHtml(templateLabel(model.print?.template))} · Druckansicht freigegeben.`;
  if (phase === 'error') {
    const tracking = taskTracking(model);
    const error = clipText(model.workflow?.error || 'Parser-Task fehlgeschlagen.', 74);
    const link = tracking.taskId || tracking.commandId
      ? ` · <button class="cv-task-link is-error" type="button" data-cv-open-task data-cv-task-id="${escapeAttr(tracking.taskId)}" data-cv-command-id="${escapeAttr(tracking.commandId)}" title="${escapeAttr([tracking.taskId, tracking.commandId].filter(Boolean).join(' · '))}">Flow öffnen</button>`
      : '';
    return `Parsing fehlgeschlagen: ${escapeHtml(error)}${link}`;
  }
  return 'Status prüfen.';
}

function taskTracking(model) {
  return {
    taskId: String(model?.workflow?.task_id || model?.workflow?.tracking_id || '').trim(),
    commandId: String(model?.workflow?.command_id || '').trim(),
  };
}

function shortTaskId(value) {
  const text = String(value || '').trim();
  if (!text) return 'Task';
  const tail = text.includes('::') ? text.split('::').pop() : text;
  return tail.length > 12 ? tail.slice(0, 6) + '…' + tail.slice(-4) : tail;
}

function clipText(value, max) {
  const text = String(value || '').replace(/\s+/g, ' ').trim();
  return text.length > max ? text.slice(0, Math.max(0, max - 1)) + '…' : text;
}

function openCtoxTask(taskId, commandId) {
  if (!taskId && !commandId) return;
  const focus = {
    taskId,
    commandId,
    sourceModule: MODULE_ID,
    openedAt: Date.now(),
  };
  const params = new URLSearchParams();
  if (taskId) params.set('task_id', taskId);
  if (commandId) params.set('command_id', commandId);
  const hash = params.toString() ? `ctox?${params.toString()}` : 'ctox';
  try {
    parent.sessionStorage.setItem('ctox.businessOs.focusTask', JSON.stringify(focus));
    parent.dispatchEvent(new CustomEvent('ctox-business-os-focus-task', { detail: focus }));
    parent.location.hash = hash;
  } catch {
    try {
      sessionStorage.setItem('ctox.businessOs.focusTask', JSON.stringify(focus));
    } catch {}
    window.dispatchEvent(new CustomEvent('ctox-business-os-focus-task', { detail: focus }));
    location.hash = hash;
  }
}

function templateLabel(id) {
  return TEMPLATES.find((entry) => entry.id === normalizeTemplateId(id))?.label || 'Modern';
}

function normalizeTemplateId(id) {
  if (id === 'neutral') return 'minimal';
  if (id === 'klassisch') return 'classic';
  if (id === 'modern' || id === 'minimal' || id === 'classic') return id;
  return 'modern';
}

function displayCandidateName(model) {
  const candidate = model?.candidate || {};
  return candidate.name || [candidate.firstName, candidate.lastName].filter(Boolean).join(' ') || model?.source?.filename?.replace(/\.pdf$/i, '') || 'Neuer CV';
}

function titleFromFilename(filename) {
  return candidateIdentityFromFilename(filename).name;
}

function candidateIdentityFromFilename(filename) {
  const rawBase = String(filename || 'Neuer CV').replace(/\.[^.]+$/, '');
  const base = rawBase
    .replace(/\([^)]*\)/g, ' ')
    .replace(/[_-]+/g, ' ')
    .replace(/\b(?:lebenslauf|curriculum|vitae|cv|resume|jan|januar|feb|februar|mar|maerz|märz|apr|april|mai|jun|juni|jul|juli|aug|august|sep|sept|september|okt|oktober|nov|november|dez|dezember)\d*\b/gi, ' ')
    .replace(/\b(?:kein|upload|final|neu|new|copy|kopie|version|stand|profil)\b/gi, ' ')
    .replace(/\b\d{2,8}\b/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
  const words = base
    .split(/\s+/)
    .map((word) => word.replace(/^[^\p{L}]+|[^\p{L}]+$/gu, ''))
    .filter((word) => word && /\p{L}/u.test(word));
  const usable = words.length ? words.slice(0, 4) : ['Neuer', 'CV'];
  const ordered = maybeFlipLastFirst(usable);
  const display = ordered.map(formatNameToken).join(' ').trim() || 'Neuer CV';
  return {
    name: display,
    firstName: ordered[0] ? formatNameToken(ordered[0]) : '',
    lastName: ordered.length > 1 ? ordered.slice(1).map(formatNameToken).join(' ') : '',
  };
}

function maybeFlipLastFirst(words) {
  if (words.length !== 2) return words;
  const first = normalizeNameToken(words[0]);
  const second = normalizeNameToken(words[1]);
  if (COMMON_FIRST_NAMES.has(second) && !COMMON_FIRST_NAMES.has(first)) {
    return [words[1], words[0]];
  }
  return words;
}

function normalizeNameToken(value) {
  return String(value || '')
    .normalize('NFD')
    .replace(/\p{Diacritic}/gu, '')
    .toLowerCase();
}

function formatNameToken(value) {
  return String(value || '')
    .split(/([^\p{L}]+)/u)
    .map((part) => /\p{L}/u.test(part) ? part.charAt(0).toUpperCase() + part.slice(1) : part)
    .join('');
}

function initials(name) {
  const parts = String(name || 'CV').trim().split(/\s+/).filter(Boolean);
  return (parts[0]?.[0] || 'C') + (parts[1]?.[0] || parts[0]?.[1] || 'V');
}

function anonymizedName(name) {
  const parts = String(name || '').trim().split(/\s+/).filter(Boolean);
  if (!parts.length) return 'Kandidat';
  return `${parts[0]} ${parts[1] ? parts[1][0] + '.' : ''}`.trim();
}

function contactLine(candidate) {
  return [candidate.email, candidate.phone].filter(Boolean).join(' · ') || 'Kontakt nachtragen';
}

function renderLogo(model, options = {}) {
  const logo = model.print?.logoDataUrl;
  if (logo) return `<div class="cv-print-logo"><img alt="Logo" src="${escapeAttr(logo)}"></div>`;
  if (options.minimalLogo) return '';
  if (options.classicLogo) return '<div class="cv-print-logo cv-print-logo-classic">CV</div>';
  return '<div class="cv-print-logo">ctox</div>';
}

function normalizeArray(value) {
  if (!Array.isArray(value)) return [];
  return value.filter((item) => item !== null && item !== undefined && String(labelOf(item)).trim());
}

function normalizeTimeline(additional, key) {
  const value = additionalValue(additional, key);
  return Array.isArray(value) ? value : [];
}

function additionalValue(additional, key) {
  if (Array.isArray(additional)) {
    return additional.find((item) => item?.key === key)?.value;
  }
  if (additional && typeof additional === 'object') {
    return additional[key];
  }
  return null;
}

function renderTimeline(items, fallback) {
  if (!items.length) return `<li>${escapeHtml(fallback)}</li>`;
  return items.map((item) => {
    const title = item.title || item.position || item.degree || item.school || item.company || item.name || labelOf(item);
    const meta = [item.company, item.org, item.institution, item.location, item.from || item.start, item.to || item.end || item.until].filter(Boolean).join(' · ');
    const description = item.details || item.description || item.summary || '';
    return `<li><strong>${escapeHtml(title)}</strong>${meta ? `<span>${escapeHtml(meta)}</span>` : ''}${description ? `<p>${escapeHtml(description)}</p>` : ''}</li>`;
  }).join('');
}

function renderClassicTimeline(items, fallback) {
  if (!items.length) return `<li>${escapeHtml(fallback)}</li>`;
  return items.map((item) => {
    const title = item.title || item.position || item.degree || item.school || item.company || item.name || labelOf(item);
    const period = [item.from || item.start, item.to || item.end || item.until].filter(Boolean).join(' - ');
    const place = [item.company, item.org, item.institution, item.location].filter(Boolean).join(' · ');
    const description = item.details || item.description || item.summary || '';
    return `
      <li>
        <span class="cv-classic-period">${escapeHtml(period || 'Zeitraum')}</span>
        <span>
          <strong>${escapeHtml(title)}</strong>
          ${place ? `<em>${escapeHtml(place)}</em>` : ''}
          ${description ? `<p>${escapeHtml(description)}</p>` : ''}
        </span>
      </li>
    `;
  }).join('');
}

function renderTags(items, fallback) {
  return items.map((item) => `<li>${escapeHtml(labelOf(item))}</li>`).join('') || `<li>${escapeHtml(fallback)}</li>`;
}

function renderSimpleList(items, fallback) {
  return items.map((item) => `<li>${escapeHtml(labelOf(item))}</li>`).join('') || `<li>${escapeHtml(fallback)}</li>`;
}

function labelOf(item) {
  if (typeof item === 'string') return item;
  return item?.label || item?.name || item?.title || item?.skill || item?.language || JSON.stringify(item);
}

function editableAttr(editable, path) {
  return editable ? `contenteditable="true" spellcheck="false" data-cv-edit-field="${escapeAttr(path)}"` : '';
}

function setPath(model, path, value) {
  const parts = String(path).split('.');
  let target = model;
  for (const part of parts.slice(0, -1)) {
    target[part] = { ...(target[part] || {}) };
    target = target[part];
  }
  target[parts.at(-1)] = value;
  return model;
}

function buildIndexText(model) {
  const candidate = model.candidate || {};
  return [
    displayCandidateName(model),
    candidate.currentRole,
    candidate.location,
    candidate.availability,
    candidate.summary,
    ...normalizeArray(candidate.skills).map(labelOf),
    ...normalizeArray(candidate.languages).map(labelOf),
  ].filter(Boolean).join(' ').slice(0, 20000);
}

async function sha256Hex(bytes) {
  const hash = await crypto.subtle.digest('SHA-256', bytes);
  return Array.from(new Uint8Array(hash)).map((value) => value.toString(16).padStart(2, '0')).join('');
}

function uint8ToBase64(bytes) {
  let binary = '';
  for (let i = 0; i < bytes.length; i += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return btoa(binary);
}


function fileToDataUrl(file) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result || ''));
    reader.onerror = () => reject(reader.error || new Error('Datei konnte nicht gelesen werden.'));
    reader.readAsDataURL(file);
  });
}

function notify(state, type, title, message) {
  state.ctx.notifications?.show?.({ type, title, message, time: 8000 });
  if (!state.ctx.notifications?.show) console[type === 'error' ? 'error' : 'log'](`[${MODULE_ID}] ${title}: ${message}`);
}

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

function escapeAttr(value) {
  return escapeHtml(value).replace(/`/g, '&#096;');
}

function iconPlay() {
  return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M8 5v14l11-7z"/></svg>';
}

function iconCheck() {
  return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m5 12 4 4L19 6"/></svg>';
}

function iconPrinter() {
  return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M7 8V4h10v4"/><path d="M7 17H5a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2h-2"/><path d="M7 14h10v6H7z"/></svg>';
}

function iconImage() {
  return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="4" y="5" width="16" height="14" rx="2"/><path d="m8 15 3-3 2 2 2-3 3 4"/><circle cx="9" cy="9" r="1"/></svg>';
}

function iconEyeOff() {
  return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m3 3 18 18"/><path d="M10.6 10.6a2 2 0 0 0 2.8 2.8"/><path d="M9.5 5.3A10.4 10.4 0 0 1 12 5c5 0 8.5 4.5 9.5 7a12 12 0 0 1-3 4.1"/><path d="M6.2 6.8A12 12 0 0 0 2.5 12c1 2.5 4.5 7 9.5 7a10 10 0 0 0 4-.8"/></svg>';
}
