import { ARCHETYPE } from './core/archetype.mjs';
import { COLLECTION, normalizeRecord, visibleRecords } from './core/records.mjs';
import { buildSignatureCommand } from './core/automation.mjs';
import { REQUEST_NOTE } from './core/request.mjs';

const MODULE_ID = '__MODULE_ID__';

export async function mount(ctx) {
  if (!ctx?.host) throw new Error(`[${MODULE_ID}] mount(ctx) requires ctx.host`);
  await ensureStyles();
  const locale = ctx.locale === 'en' ? 'en' : 'de';
  const copy = await loadLocale(locale);
  ctx.host.innerHTML = await loadMarkup();
  const root = ctx.host.querySelector('[data-starter-root]');
  const list = root.querySelector('[data-record-list]');
  const form = root.querySelector('[data-record-form]');
  const empty = root.querySelector('[data-empty-state]');
  const preview = root.querySelector('[data-record-preview]');
  const inspector = root.querySelector('[data-record-inspector]');
  const search = root.querySelector('[data-record-search]');
  const statusFilter = root.querySelector('[data-status-filter]');
  const runStatus = root.querySelector('[data-run-status]');
  const runButton = root.querySelector('[data-action="run-signature"]');
  const saveState = root.querySelector('[data-save-state]');
  const stateStack = root.querySelector('[data-state-stack]');
  const offlineState = root.querySelector('[data-offline-state]');
  const permissionState = root.querySelector('[data-permission-state]');
  const errorState = root.querySelector('[data-error-state]');
  const errorMessage = root.querySelector('[data-error-message]');
  const state = { records: [], selectedId: '', query: '', status: '', descending: true, contextCleanups: [] };
  const canWrite = ctx.permissions?.canWriteCollection?.(COLLECTION) !== false;
  applySharedCopy(root, copy);
  applyArchetypeCopy(root, locale);
  root.querySelector('[data-request-note]').textContent = REQUEST_NOTE;
  search.placeholder = copy.search;
  search.setAttribute('aria-label', copy.search);
  runStatus.textContent = copy.ready;
  saveState.textContent = copy.saved;
  root.querySelector('[data-action="create-record"]').disabled = !canWrite;
  root.querySelector('[data-action="edit"]').disabled = !canWrite;
  permissionState.hidden = canWrite;
  updateOnlineState();
  updateStateStack();

  const collection = getCollection(ctx);
  await readRecords();
  const subscription = collection?.$?.subscribe?.(() => readRecords().catch(reportError));

  const clickHandler = async (event) => {
    const actionNode = event.target.closest('[data-action]');
    const recordNode = event.target.closest('[data-record-id]');
    if (recordNode && !actionNode) selectRecord(recordNode.dataset.recordId);
    if (!actionNode) return;
    const action = actionNode.dataset.action;
    if (action === 'create-record') openForm();
    if (action === 'edit') openForm(selectedRecord());
    if (action === 'cancel-edit') closeForm();
    if (action === 'sort') {
      state.descending = !state.descending;
      render();
    }
    if (action === 'import') await importRecords();
    if (action === 'run-signature') await runSignature();
    if (action === 'request-permission') await requestPermission();
  };
  const inputHandler = () => {
    state.query = search.value;
    state.status = statusFilter.value;
    render();
  };
  const submitHandler = async (event) => {
    event.preventDefault();
    if (!canWrite) return showPermissionState();
    const data = new FormData(form);
    const existing = selectedRecord();
    const record = normalizeRecord({
      ...existing,
      title: data.get('title'),
      notes: data.get('notes'),
      updated_at_ms: Date.now()
    });
    saveState.textContent = copy.saving;
    try {
      await writeRecord(record);
      state.selectedId = record.id;
      saveState.textContent = copy.saved;
      clearError();
      closeForm();
    } catch (error) {
      saveState.textContent = copy.failed;
      reportError(error);
    }
  };
  const onlineHandler = () => updateOnlineState();
  root.addEventListener('click', clickHandler);
  search.addEventListener('input', inputHandler);
  statusFilter.addEventListener('change', inputHandler);
  form.addEventListener('submit', submitHandler);
  window.addEventListener('online', onlineHandler);
  window.addEventListener('offline', onlineHandler);

  return () => {
    subscription?.unsubscribe?.();
    for (const cleanup of state.contextCleanups.splice(0)) cleanup?.();
    root.removeEventListener('click', clickHandler);
    search.removeEventListener('input', inputHandler);
    statusFilter.removeEventListener('change', inputHandler);
    form.removeEventListener('submit', submitHandler);
    window.removeEventListener('online', onlineHandler);
    window.removeEventListener('offline', onlineHandler);
    ctx.host.replaceChildren();
  };

  async function readRecords() {
    if (!collection?.find) {
      state.records = [];
      render();
      return;
    }
    const docs = await collection.find().exec();
    state.records = docs.map((doc) => normalizeRecord(doc.toJSON?.() || doc));
    if (state.selectedId && !state.records.some((record) => record.id === state.selectedId)) state.selectedId = '';
    render();
  }

  async function writeRecord(record) {
    if (!collection) throw new Error(`Collection ${COLLECTION} is unavailable`);
    if (collection.upsert) await collection.upsert(record);
    else await collection.insert(record);
    state.records = [record, ...state.records.filter((item) => item.id !== record.id)];
    render();
  }

  function render() {
    const records = visibleRecords(state.records, state.query, state.status)
      .sort((a, b) => (state.descending ? -1 : 1) * (Number(a.updated_at_ms) - Number(b.updated_at_ms)));
    list.replaceChildren();
    if (!records.length) {
      const node = document.createElement('div');
      node.className = 'ctox-empty';
      const heading = document.createElement('strong');
      heading.textContent = copy.no_matches;
      const detail = document.createElement('span');
      detail.textContent = copy.no_matches_hint;
      node.append(heading, detail);
      list.append(node);
    }
    for (const record of records) {
      const button = document.createElement('button');
      button.type = 'button';
      button.className = `ctox-list-item${record.id === state.selectedId ? ' is-selected' : ''}`;
      button.dataset.recordId = record.id;
      button.dataset.contextRecordId = record.id;
      button.dataset.contextRecordType = 'record';
      button.dataset.contextLabel = record.title;
      button.innerHTML = `<strong>${escapeHtml(record.title)}</strong><small>${escapeHtml(record.status)}</small>`;
      list.append(button);
    }
    renderInspector();
    registerContextTargets();
  }

  function renderInspector() {
    const record = selectedRecord();
    empty.hidden = Boolean(record);
    preview.hidden = !record || !form.hidden;
    if (!record) {
      inspector.replaceChildren();
      runButton.disabled = true;
      return;
    }
    runButton.disabled = false;
    preview.querySelector('[data-workbench-preview-title]').textContent = (locale === 'de' ? ARCHETYPE.de?.workbench : ARCHETYPE.workbench) || ARCHETYPE.workbench;
    preview.querySelector('[data-preview-title]').textContent = record.title;
    preview.querySelector('[data-preview-status]').textContent = record.status;
    preview.querySelector('[data-preview-notes]').textContent = record.notes || '—';
    preview.querySelector('[data-preview-updated]').textContent = `${copy.updated}: ${new Date(record.updated_at_ms).toLocaleString(locale)}`;
    inspector.innerHTML = `
      <dt>Title</dt><dd>${escapeHtml(record.title)}</dd>
      <dt>Status</dt><dd><span class="ctox-badge">${escapeHtml(record.status)}</span></dd>
      <dt>${escapeHtml(copy.updated)}</dt><dd>${escapeHtml(new Date(record.updated_at_ms).toLocaleString(locale))}</dd>
    `;
  }

  function selectRecord(id) {
    state.selectedId = id;
    render();
  }

  function selectedRecord() {
    return state.records.find((record) => record.id === state.selectedId) || null;
  }

  function openForm(record = null) {
    if (!canWrite) return showPermissionState();
    empty.hidden = true;
    preview.hidden = true;
    form.hidden = false;
    form.elements.title.value = record?.title || '';
    form.elements.notes.value = record?.notes || '';
    root.querySelector('[data-form-heading]').textContent = record
      ? interpolate(copy.edit_record, { title: record.title })
      : copy.new_record;
    form.elements.title.focus();
  }

  function closeForm() {
    form.hidden = true;
    form.reset();
    empty.hidden = Boolean(selectedRecord());
    preview.hidden = !selectedRecord();
  }

  async function runSignature() {
    const record = selectedRecord();
    if (!record) return;
    runButton.setAttribute('aria-busy', 'true');
    runStatus.textContent = copy.queued;
    try {
      const result = await ctx.commandBus.dispatch(buildSignatureCommand(record, ARCHETYPE));
      runStatus.textContent = result?.status || copy.queued;
      clearError();
    } catch (error) {
      runStatus.textContent = copy.failed;
      runStatus.className = 'ctox-badge is-danger';
      reportError(error);
    } finally {
      runButton.removeAttribute('aria-busy');
    }
  }

  async function importRecords() {
    if (!canWrite) return showPermissionState();
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'application/json,.json';
    input.addEventListener('change', async () => {
      const file = input.files?.[0];
      if (!file) return;
      try {
        const parsed = JSON.parse(await file.text());
        const rows = Array.isArray(parsed) ? parsed : parsed.records;
        if (!Array.isArray(rows)) throw new Error(copy.import_error);
        for (const row of rows) await writeRecord(normalizeRecord(row));
        clearError();
      } catch (error) {
        reportError(error);
      }
    }, { once: true });
    input.click();
  }

  function registerContextTargets() {
    for (const cleanup of state.contextCleanups.splice(0)) cleanup?.();
    if (!ctx.contextActions?.register) return;
    for (const node of list.querySelectorAll('[data-record-id]')) {
      const id = node.dataset.recordId;
      const cleanup = ctx.contextActions.register(node, {
        surface: `${ARCHETYPE.id}.list`,
        pane: 'navigation',
        entity: { collection: COLLECTION, type: 'record', id, label: node.dataset.contextLabel },
        selection: () => ({ ids: [id] }),
        actions: ['context.ask', 'data.modify', 'app.modify']
      });
      if (typeof cleanup === 'function') state.contextCleanups.push(cleanup);
    }
  }

  function showPermissionState() {
    permissionState.hidden = false;
    updateStateStack();
    ctx.notifications?.show?.({ type: 'warning', title: copy.permission_title, message: copy.permission_notice });
  }

  function reportError(error) {
    const message = error?.message || String(error);
    errorMessage.textContent = message;
    errorState.hidden = false;
    updateStateStack();
    ctx.notifications?.show?.({ type: 'error', title: copy.error_title, message });
  }

  function clearError() {
    errorState.hidden = true;
    errorMessage.textContent = '';
    updateStateStack();
  }

  function updateOnlineState() {
    offlineState.hidden = navigator.onLine !== false;
    updateStateStack();
  }

  function updateStateStack() {
    stateStack.hidden = offlineState.hidden && permissionState.hidden && errorState.hidden;
  }

  async function requestPermission() {
    try {
      await ctx.contextActions.dispatch('data', {
        target: selectedRecord()
          ? Array.from(list.querySelectorAll('[data-record-id]'))
            .find((node) => node.dataset.recordId === selectedRecord().id)
          : root,
        prompt: copy.permission_request,
        title: copy.request_permission
      });
      clearError();
    } catch (error) {
      reportError(error);
    }
  }
}

function applyArchetypeCopy(root, locale) {
  const labels = locale === 'de' ? { ...ARCHETYPE, ...(ARCHETYPE.de || {}) } : ARCHETYPE;
  root.querySelector('[data-archetype-title]').textContent = labels.title;
  root.querySelector('[data-navigation-title]').textContent = labels.navigation;
  root.querySelector('[data-workbench-title]').textContent = labels.workbench;
  root.querySelector('[data-inspector-title]').textContent = labels.inspector;
  root.querySelector('[data-action="run-signature"]').textContent = labels.signature_action;
}

function applySharedCopy(root, copy) {
  for (const node of root.querySelectorAll('[data-copy]')) {
    const value = copy[node.dataset.copy];
    if (value) node.textContent = value;
  }
}

function interpolate(template, values = {}) {
  return String(template || '').replace(/\{([^}]+)\}/g, (_match, key) => String(values[key] ?? ''));
}

function getCollection(ctx) {
  try { return ctx.db.collection(COLLECTION); } catch { return null; }
}

async function loadMarkup() {
  const response = await fetch(new URL('./index.html', import.meta.url));
  if (!response.ok) throw new Error(`Starter markup failed: HTTP ${response.status}`);
  return response.text();
}

async function loadLocale(locale) {
  const response = await fetch(new URL(`./locales/${locale}.json`, import.meta.url));
  if (!response.ok) throw new Error(`Starter locale failed: HTTP ${response.status}`);
  return response.json();
}

async function ensureStyles() {
  const key = `starter-${MODULE_ID}`;
  if (document.querySelector(`link[data-starter-style="${key}"]`)) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = new URL('./index.css', import.meta.url).href;
  link.dataset.starterStyle = key;
  document.head.append(link);
}

function escapeHtml(value) {
  return String(value ?? '').replace(/[&<>"']/g, (character) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'
  })[character]);
}
