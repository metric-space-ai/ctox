#!/usr/bin/env node
import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';

function usage() {
  return [
    'Usage: node src/apps/business-os/scripts/scaffold-app-module.mjs <module> [--installed|--source] [--workspace <path>] [--title <title>] [--collection <name>] [--description <text>] [--force] [--repair-missing] [--json]',
    '',
    'Creates or repairs a structurally complete CTOX Business OS app module scaffold.',
  ].join('\n');
}

function parseArgs(argv) {
  const result = {
    moduleId: null,
    mode: null,
    workspace: process.cwd(),
    title: null,
    collection: null,
    description: null,
    force: false,
    repairMissing: false,
    json: false,
  };
  for (let idx = 0; idx < argv.length; idx += 1) {
    const arg = argv[idx];
    if (arg === '--source') {
      result.mode = 'source';
    } else if (arg === '--installed') {
      result.mode = 'installed';
    } else if (arg === '--workspace') {
      const value = argv[idx + 1];
      if (!value) throw new Error('--workspace requires a path');
      result.workspace = value;
      idx += 1;
    } else if (arg === '--title') {
      const value = argv[idx + 1];
      if (!value) throw new Error('--title requires a value');
      result.title = value;
      idx += 1;
    } else if (arg === '--collection') {
      const value = argv[idx + 1];
      if (!value) throw new Error('--collection requires a value');
      result.collection = value;
      idx += 1;
    } else if (arg === '--description') {
      const value = argv[idx + 1];
      if (!value) throw new Error('--description requires a value');
      result.description = value;
      idx += 1;
    } else if (arg === '--force') {
      result.force = true;
    } else if (arg === '--repair-missing') {
      result.repairMissing = true;
    } else if (arg === '--json') {
      result.json = true;
    } else if (arg.startsWith('-')) {
      throw new Error(`unknown option: ${arg}`);
    } else if (!result.moduleId) {
      result.moduleId = arg;
    } else {
      throw new Error(`unexpected argument: ${arg}`);
    }
  }
  if (!result.moduleId || /[\\/]/.test(result.moduleId) || result.moduleId === '.' || result.moduleId === '..') {
    throw new Error('module id is required and must be a single path segment');
  }
  result.workspace = resolve(result.workspace);
  result.mode ||= existsSync(join(result.workspace, 'runtime')) ? 'installed' : 'source';
  if (result.mode !== 'installed' && result.mode !== 'source') {
    throw new Error('mode must be installed or source');
  }
  if (result.force && result.repairMissing) {
    throw new Error('--repair-missing cannot be combined with --force');
  }
  result.title ||= titleFromId(result.moduleId);
  result.collection ||= `${snakeName(result.moduleId)}_records`;
  if (!/^[a-z][a-z0-9_]*$/.test(result.collection)) {
    throw new Error('--collection must be snake_case and start with a letter');
  }
  result.description ||= `${result.title} workspace for records, owner status, due dates, and CTOX chat task follow-up.`;
  return result;
}

const shellCollections = new Set([
  'business_module_catalog',
  'ctox_runtime_settings',
  'business_commands',
  'ctox_queue_tasks',
]);

function titleFromId(value) {
  return String(value)
    .replace(/[_-]+/g, ' ')
    .replace(/\b\w/g, (char) => char.toUpperCase())
    .trim() || 'Business OS App';
}

function snakeName(value) {
  const normalized = String(value)
    .replace(/[^A-Za-z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .toLowerCase();
  return /^[a-z]/.test(normalized) ? normalized : `app_${normalized || 'records'}`;
}

function cssName(value) {
  return String(value)
    .replace(/[^A-Za-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .toLowerCase() || 'business-os-app';
}

function installedAppRootFor(workspace) {
  const runtimeAppRoot = join(workspace, 'runtime/business-os');
  if (existsSync(join(workspace, 'runtime')) || existsSync(runtimeAppRoot)) {
    return runtimeAppRoot;
  }
  return join(workspace, 'business-os');
}

function moduleDirFor(options) {
  if (options.mode === 'installed') {
    return join(installedAppRootFor(options.workspace), 'installed-modules', options.moduleId);
  }
  return join(options.workspace, 'src/apps/business-os/modules', options.moduleId);
}

function rel(workspace, path) {
  return relative(workspace, path).split(sep).join('/');
}

function writeAtomic(path, content) {
  mkdirSync(dirname(path), { recursive: true });
  const tempPath = `${path}.tmp-${process.pid}`;
  writeFileSync(tempPath, content);
  renameSync(tempPath, path);
}

function writeJson(path, value) {
  writeAtomic(path, `${JSON.stringify(value, null, 2)}\n`);
}

function assertWritableTarget(dir, force) {
  if (!existsSync(dir)) return;
  const existing = [
    'module.json',
    'collections.schema.json',
    'schema.js',
    'index.html',
    'index.css',
    'index.js',
    'icon.svg',
    'core/automation.mjs',
    'core/records.mjs',
  ].filter((name) => existsSync(join(dir, name)));
  if (existing.length > 0 && !force) {
    throw new Error(`module directory already contains app files (${existing.join(', ')}); rerun with --force only when resetting this generated app is intentional`);
  }
}

function schemaFor(collectionName) {
  return {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 120 },
      title: { type: 'string' },
      status: { type: 'string' },
      owner: { type: 'string' },
      due_at_ms: { type: 'number' },
      notes: { type: 'string' },
      created_at_ms: { type: 'number' },
      updated_at_ms: { type: 'number' },
      is_deleted: { type: 'boolean' },
    },
    required: ['id', 'title', 'status', 'updated_at_ms', 'is_deleted'],
    indexes: [
      'updated_at_ms',
      'status',
      ['is_deleted', 'updated_at_ms'],
    ],
    additionalProperties: true,
  };
}

function moduleManifest(options) {
  const entryPrefix = options.mode === 'installed'
    ? `installed-modules/${options.moduleId}`
    : `modules/${options.moduleId}`;
  return {
    id: options.moduleId,
    title: options.title,
    description: options.description,
    version: '0.1.0',
    entry: `${entryPrefix}/index.html`,
    install_scope: options.mode === 'installed' ? 'installed' : 'store',
    collections: ['business_commands', options.collection],
    layout: {
      shell: 'full-workspace',
      left: 'Records',
      center: 'Detail',
    },
    category: 'Business',
    developer: 'CTOX',
    license: 'AGPL-3.0-only',
    tags: domainTags(options),
    store: {
      summary: options.description,
      repository: 'metric-space-ai/ctox',
      source_path: options.mode === 'installed'
        ? `installed-modules/${options.moduleId}`
        : `src/apps/business-os/modules/${options.moduleId}`,
      installable: options.mode !== 'installed',
      editable_after_install: true,
      distribution: options.mode === 'installed'
        ? 'ctox-runtime-installed-module'
        : 'ctox-source-module',
    },
    default_installed: false,
  };
}

function domainTags(options) {
  const generic = new Set(['app', 'business', 'business-os', 'ctox', 'module', 'records', 'record']);
  const tokens = `${options.moduleId} ${options.title}`
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .map((token) => token.trim())
    .filter((token) => token.length >= 3 && !generic.has(token));
  return ['business-os', ...Array.from(new Set(tokens)).slice(0, 4), 'workflow'];
}

function schemaJs(options) {
  const schema = JSON.stringify(schemaFor(options.collection), null, 2)
    .replace(/"([^"]+)":/g, '$1:')
    .replace(/"/g, "'");
  return `const recordSchema = ${schema};

export const collections = {
  ${options.collection}: recordSchema,
};

export const migrationStrategies = {
  ${options.collection}: {},
};
`;
}

function automationJs(options) {
  return `const MODULE_ID = ${JSON.stringify(options.moduleId)};
const COLLECTION_NAME = ${JSON.stringify(options.collection)};

export function buildFollowUpCommand(record = {}) {
  const recordId = String(record.id || 'demo');
  const title = String(record.title || 'Record');
  const prompt = 'Review "' + title + '" in ${escapeTemplateText(options.title)} and create the next CTOX follow-up if action is required.';
  return {
    id: 'cmd_' + MODULE_ID + '_' + recordId,
    module: MODULE_ID,
    type: 'business_os.chat.task',
    command_type: 'business_os.chat.task',
    record_id: recordId,
    payload: {
      title: 'Review: ' + title,
      instruction: prompt,
      prompt,
      source_module: MODULE_ID,
      source_collection: COLLECTION_NAME,
      record_snapshot: { ...record, id: recordId },
      outbound_channel: 'business_os_chat',
      response_channel: 'business_os_chat',
    },
    client_context: {
      source: MODULE_ID,
      surface: MODULE_ID + '.follow-up',
      module_id: MODULE_ID,
      collection: COLLECTION_NAME,
    },
  };
}
`;
}

function escapeTemplateText(value) {
  return String(value).replace(/\\/g, '\\\\').replace(/'/g, "\\'");
}

function recordsJs(options) {
  return `export const MODULE_ID = ${JSON.stringify(options.moduleId)};
export const COLLECTION_NAME = ${JSON.stringify(options.collection)};

export function nowMs() {
  return Date.now();
}

export function createRecord(input = {}, time = nowMs()) {
  const title = String(input.title || '').trim() || 'New record';
  return {
    id: String(input.id || 'rec_' + time),
    title,
    status: normalizeStatus(input.status),
    owner: String(input.owner || '').trim(),
    due_at_ms: Number(input.due_at_ms || 0),
    notes: String(input.notes || '').trim(),
    created_at_ms: Number(input.created_at_ms || time),
    updated_at_ms: Number(input.updated_at_ms || time),
    is_deleted: Boolean(input.is_deleted),
  };
}

export function normalizeStatus(value) {
  const status = String(value || '').trim().toLowerCase();
  if (status === 'done' || status === 'blocked') return status;
  return 'open';
}

export function visibleRecords(records = []) {
  return records
    .filter((record) => !record.is_deleted)
    .sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0));
}

export function summarizeRecords(records = []) {
  const visible = visibleRecords(records);
  return {
    total: visible.length,
    open: visible.filter((record) => normalizeStatus(record.status) === 'open').length,
    blocked: visible.filter((record) => normalizeStatus(record.status) === 'blocked').length,
    done: visible.filter((record) => normalizeStatus(record.status) === 'done').length,
  };
}
`;
}

function indexHtml(options) {
  const klass = cssName(options.moduleId);
  return `<main class="${klass}-module" data-module-root>
  <section class="${klass}-pane ${klass}-list-pane" aria-label="Records">
    <header class="${klass}-header">
      <div>
        <h1 data-title>${escapeHtml(options.title)}</h1>
        <p data-summary>0 records</p>
      </div>
      <button type="button" data-action="new">New</button>
    </header>
    <div class="${klass}-toolbar">
      <input type="search" data-search placeholder="Search records" />
      <select data-status-filter>
        <option value="all">All</option>
        <option value="open">Open</option>
        <option value="blocked">Blocked</option>
        <option value="done">Done</option>
      </select>
    </div>
    <div class="${klass}-records" data-records></div>
  </section>
  <section class="${klass}-pane ${klass}-detail-pane" aria-label="Detail">
    <header class="${klass}-detail-header">
      <div>
        <h2 data-detail-title>Select a record</h2>
        <p data-detail-meta>Use the list to open or create a record.</p>
      </div>
      <button type="button" data-action="follow-up" disabled>Follow up</button>
    </header>
    <form class="${klass}-form" data-form>
      <label>Title<input name="title" required /></label>
      <label>Status<select name="status"><option value="open">Open</option><option value="blocked">Blocked</option><option value="done">Done</option></select></label>
      <label>Owner<input name="owner" /></label>
      <label>Due date<input name="due_at" type="date" /></label>
      <label class="${klass}-wide">Notes<textarea name="notes" rows="6"></textarea></label>
      <div class="${klass}-actions">
        <button type="submit">Save</button>
        <button type="button" data-action="delete" disabled>Archive</button>
      </div>
    </form>
    <p class="${klass}-message" data-message></p>
  </section>
</main>
`;
}

function escapeHtml(value) {
  return String(value)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function indexCss(options) {
  const klass = cssName(options.moduleId);
  return `.${klass}-module {
  --${klass}-accent: #2563eb;
  --${klass}-soft: #eff6ff;
  display: grid;
  grid-template-columns: minmax(280px, 360px) minmax(0, 1fr);
  gap: 12px;
  height: 100%;
  padding: 12px;
  box-sizing: border-box;
  color: var(--text);
  font-family: var(--font-sans, system-ui, sans-serif);
}

.${klass}-pane {
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  border: 1px solid var(--line, var(--hairline));
  border-radius: var(--panel-radius, 10px);
  background: var(--surface);
  overflow: hidden;
}

.${klass}-header,
.${klass}-detail-header {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  padding: 14px;
  border-bottom: 1px solid var(--line, var(--hairline));
}

.${klass}-header h1,
.${klass}-detail-header h2 {
  margin: 0;
  font-size: 16px;
  line-height: 1.25;
}

.${klass}-header p,
.${klass}-detail-header p,
.${klass}-message {
  margin: 4px 0 0;
  color: var(--muted);
  font-size: 12px;
}

.${klass}-toolbar {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 120px;
  gap: 8px;
  padding: 10px;
  border-bottom: 1px solid var(--line, var(--hairline));
}

.${klass}-toolbar input,
.${klass}-toolbar select,
.${klass}-form input,
.${klass}-form select,
.${klass}-form textarea {
  width: 100%;
  box-sizing: border-box;
  border: 1px solid var(--line, var(--hairline));
  border-radius: var(--control-radius, 8px);
  background: var(--surface-2);
  color: var(--text);
  padding: 7px 9px;
  font: inherit;
  font-size: 13px;
}

.${klass}-records {
  flex: 1 1 auto;
  overflow: auto;
  padding: 8px;
}

.${klass}-record {
  width: 100%;
  text-align: left;
  border: 1px solid transparent;
  border-radius: var(--control-radius, 8px);
  background: transparent;
  color: inherit;
  padding: 10px;
  cursor: pointer;
}

.${klass}-record:hover,
.${klass}-record.is-selected {
  border-color: var(--${klass}-accent);
  background: var(--${klass}-soft);
}

.${klass}-record strong,
.${klass}-record span {
  display: block;
}

.${klass}-record span {
  margin-top: 3px;
  color: var(--muted);
  font-size: 12px;
}

.${klass}-form {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
  padding: 14px;
  overflow: auto;
}

.${klass}-form label {
  display: flex;
  flex-direction: column;
  gap: 5px;
  color: var(--muted);
  font-size: 12px;
}

.${klass}-wide,
.${klass}-actions,
.${klass}-message {
  grid-column: 1 / -1;
}

.${klass}-actions {
  display: flex;
  gap: 8px;
}

.${klass}-module button {
  border: 1px solid var(--line, var(--hairline));
  border-radius: var(--control-radius, 8px);
  background: var(--surface-2);
  color: var(--text);
  padding: 7px 11px;
  cursor: pointer;
}

.${klass}-module button:not([disabled]):hover {
  border-color: var(--${klass}-accent);
}

.${klass}-module button[disabled] {
  opacity: 0.55;
  cursor: not-allowed;
}

@media (max-width: 760px) {
  .${klass}-module {
    grid-template-columns: minmax(0, 1fr);
  }
  .${klass}-form {
    grid-template-columns: minmax(0, 1fr);
  }
}
`;
}

function indexJs(options) {
  const klass = cssName(options.moduleId);
  return `import { buildFollowUpCommand } from './core/automation.mjs';
import { COLLECTION_NAME, createRecord, normalizeStatus, summarizeRecords, visibleRecords } from './core/records.mjs';

const state = {
  ctx: null,
  records: [],
  selectedId: '',
  search: '',
  status: 'all',
  subscription: null,
};

function attachStylesheetOnce() {
  if (document.querySelector('link[data-module-styles="${klass}"]')) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = new URL('./index.css', import.meta.url).href;
  link.dataset.moduleStyles = '${klass}';
  document.head.append(link);
}

function collectionFrom(ctx) {
  return ctx?.db?.collection?.(COLLECTION_NAME) || ctx?.db?.[COLLECTION_NAME] || null;
}

function toPlain(document) {
  return document?.toJSON ? document.toJSON() : document;
}

async function loadRecords() {
  const collection = collectionFrom(state.ctx);
  if (!collection?.find) {
    state.records = [];
    render();
    return;
  }
  const docs = await collection.find().exec();
  state.records = docs.map(toPlain);
  render();
}

function watchRecords() {
  const collection = collectionFrom(state.ctx);
  const query = collection?.find?.();
  if (!query?.$?.subscribe) return null;
  return query.$.subscribe((docs) => {
    state.records = docs.map(toPlain);
    render();
  });
}

async function saveRecord(record) {
  const collection = collectionFrom(state.ctx);
  if (!collection) {
    mergeLocal(record);
    renderMessage('Collection is not available yet; changes are kept in this view.');
    return;
  }
  const existing = await collection.findOne(record.id).exec();
  if (existing?.incrementalPatch) {
    await existing.incrementalPatch(record);
  } else if (existing?.patch) {
    await existing.patch(record);
  } else if (existing) {
    mergeLocal(record);
  } else {
    await collection.insert(record);
  }
  mergeLocal(record);
}

function mergeLocal(record) {
  const next = state.records.filter((item) => item.id !== record.id);
  next.push(record);
  state.records = next;
}

function selectedRecord() {
  return state.records.find((record) => record.id === state.selectedId && !record.is_deleted) || null;
}

function filteredRecords() {
  const term = state.search.trim().toLowerCase();
  return visibleRecords(state.records).filter((record) => {
    if (state.status !== 'all' && normalizeStatus(record.status) !== state.status) return false;
    if (!term) return true;
    return [record.title, record.owner, record.notes].some((value) => String(value || '').toLowerCase().includes(term));
  });
}

function render() {
  const host = state.ctx?.host;
  if (!host) return;
  const summary = summarizeRecords(state.records);
  const summaryNode = host.querySelector('[data-summary]');
  if (summaryNode) summaryNode.textContent = summary.total + ' records, ' + summary.open + ' open, ' + summary.blocked + ' blocked';
  const list = host.querySelector('[data-records]');
  if (list) {
    const records = filteredRecords();
    list.innerHTML = records.length
      ? records.map((record) => recordButton(record)).join('')
      : '<p class="${klass}-message">No records match the current view.</p>';
  }
  fillForm(selectedRecord());
}

function recordButton(record) {
  const selected = record.id === state.selectedId ? ' is-selected' : '';
  const subtitle = [normalizeStatus(record.status), record.owner || 'No owner'].join(' · ');
  return '<button type="button" class="${klass}-record' + selected + '" data-record-id="' + escapeHtml(record.id) + '"><strong>' + escapeHtml(record.title) + '</strong><span>' + escapeHtml(subtitle) + '</span></button>';
}

function fillForm(record) {
  const host = state.ctx?.host;
  const form = host?.querySelector('[data-form]');
  const followUp = host?.querySelector('[data-action="follow-up"]');
  const archive = host?.querySelector('[data-action="delete"]');
  const title = host?.querySelector('[data-detail-title]');
  const meta = host?.querySelector('[data-detail-meta]');
  if (!form) return;
  if (!record) {
    form.reset();
    if (title) title.textContent = 'Select a record';
    if (meta) meta.textContent = 'Use the list to open or create a record.';
    if (followUp) followUp.disabled = true;
    if (archive) archive.disabled = true;
    return;
  }
  form.elements.namedItem('title').value = record.title || '';
  form.elements.namedItem('status').value = normalizeStatus(record.status);
  form.elements.namedItem('owner').value = record.owner || '';
  form.elements.namedItem('due_at').value = dateInput(record.due_at_ms);
  form.elements.namedItem('notes').value = record.notes || '';
  if (title) title.textContent = record.title || 'Record';
  if (meta) meta.textContent = normalizeStatus(record.status) + ' · ' + (record.owner || 'No owner');
  if (followUp) followUp.disabled = !state.ctx?.commandBus?.dispatch;
  if (archive) archive.disabled = false;
}

function dateInput(ms) {
  if (!ms) return '';
  const date = new Date(ms);
  if (Number.isNaN(date.getTime())) return '';
  return date.toISOString().slice(0, 10);
}

function dateMs(value) {
  if (!value) return 0;
  const time = new Date(value + 'T00:00:00Z').getTime();
  return Number.isFinite(time) ? time : 0;
}

function renderMessage(message) {
  const node = state.ctx?.host?.querySelector('[data-message]');
  if (node) node.textContent = message || '';
}

function wireEvents() {
  const host = state.ctx.host;
  host.addEventListener('input', (event) => {
    if (event.target.matches('[data-search]')) {
      state.search = event.target.value || '';
      render();
    }
  });
  host.addEventListener('change', (event) => {
    if (event.target.matches('[data-status-filter]')) {
      state.status = event.target.value || 'all';
      render();
    }
  });
  host.addEventListener('click', async (event) => {
    const recordButton = event.target.closest('[data-record-id]');
    if (recordButton) {
      state.selectedId = recordButton.getAttribute('data-record-id') || '';
      renderMessage('');
      render();
      return;
    }
    const action = event.target.closest('[data-action]')?.getAttribute('data-action');
    if (action === 'new') {
      const record = createRecord({ title: 'New record' });
      state.selectedId = record.id;
      await saveRecord(record);
      render();
    } else if (action === 'delete') {
      const record = selectedRecord();
      if (!record) return;
      await saveRecord({ ...record, is_deleted: true, updated_at_ms: Date.now() });
      state.selectedId = '';
      renderMessage('Record archived.');
      render();
    } else if (action === 'follow-up') {
      const record = selectedRecord();
      if (!record || !state.ctx?.commandBus?.dispatch) return;
      const command = buildFollowUpCommand(record);
      await state.ctx.commandBus.dispatch(command);
      renderMessage('Follow-up sent to CTOX.');
    }
  });
  host.querySelector('[data-form]')?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const form = event.currentTarget;
    const current = selectedRecord() || createRecord({ title: form.elements.namedItem('title').value });
    const record = createRecord({
      ...current,
      title: form.elements.namedItem('title').value,
      status: form.elements.namedItem('status').value,
      owner: form.elements.namedItem('owner').value,
      due_at_ms: dateMs(form.elements.namedItem('due_at').value),
      notes: form.elements.namedItem('notes').value,
      updated_at_ms: Date.now(),
    });
    state.selectedId = record.id;
    await saveRecord(record);
    renderMessage('Record saved.');
    render();
  });
}

function escapeHtml(value) {
  return String(value || '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

export async function mount(ctx) {
  state.ctx = ctx;
  attachStylesheetOnce();
  ctx.host.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  wireEvents();
  await loadRecords();
  state.subscription = watchRecords();
  return () => {
    if (state.subscription?.unsubscribe) state.subscription.unsubscribe();
    ctx.host.innerHTML = '';
  };
}
`;
}

function iconSvg() {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" role="img" aria-label="Business OS app">
  <rect x="4" y="4" width="16" height="16" rx="3" fill="#eff6ff" stroke="#2563eb" stroke-width="1.8"/>
  <path d="M8 9h8M8 13h8M8 17h5" stroke="#2563eb" stroke-width="1.8" stroke-linecap="round"/>
</svg>
`;
}

function localeDe(options) {
  return {
    title: options.title,
    new_record: 'Neuer Eintrag',
    save: 'Speichern',
    follow_up: 'Nachfassen',
  };
}

function localeEn(options) {
  return {
    title: options.title,
    new_record: 'New record',
    save: 'Save',
    follow_up: 'Follow up',
  };
}

function testJs(options) {
  return `import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { buildFollowUpCommand } from '../core/automation.mjs';
import { createRecord, summarizeRecords, visibleRecords } from '../core/records.mjs';

const manifest = JSON.parse(readFileSync(new URL('../module.json', import.meta.url), 'utf8'));
assert.equal(manifest.id, ${JSON.stringify(options.moduleId)});
assert.ok(manifest.collections.includes(${JSON.stringify(options.collection)}));

const schemaDoc = JSON.parse(readFileSync(new URL('../collections.schema.json', import.meta.url), 'utf8'));
assert.equal(schemaDoc.schema_format, 'ctox-business-os-module-collections-v1');
assert.ok(schemaDoc.collections[${JSON.stringify(options.collection)}]);

const open = createRecord({ id: 'open', title: 'Open item', status: 'open' }, 1000);
const blocked = createRecord({ id: 'blocked', title: 'Blocked item', status: 'blocked' }, 2000);
const archived = createRecord({ id: 'archived', title: 'Archived item', is_deleted: true }, 3000);
assert.equal(visibleRecords([open, blocked, archived]).length, 2);
assert.deepEqual(summarizeRecords([open, blocked, archived]), { total: 2, open: 1, blocked: 1, done: 0 });

const command = buildFollowUpCommand(open);
assert.equal(command.type, 'business_os.chat.task');
assert.equal(command.command_type, 'business_os.chat.task');
assert.equal(command.payload.source_collection, ${JSON.stringify(options.collection)});
assert.deepEqual(command.payload.record_snapshot, open);
`;
}

function contractTestJs(options) {
  return `import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import * as automation from '../core/automation.mjs';
import { summarizeRecords, visibleRecords } from '../core/records.mjs';

const shellCollections = new Set([
  'business_module_catalog',
  'ctox_runtime_settings',
  'business_commands',
  'ctox_queue_tasks',
]);

const manifest = JSON.parse(readFileSync(new URL('../module.json', import.meta.url), 'utf8'));
assert.equal(manifest.id, ${JSON.stringify(options.moduleId)});
assert.equal(manifest.entry, ${JSON.stringify(options.mode === 'installed' ? `installed-modules/${options.moduleId}/index.html` : `modules/${options.moduleId}/index.html`)});
assert.equal(manifest.install_scope, ${JSON.stringify(options.mode === 'installed' ? 'installed' : 'store')});

const schemaDoc = JSON.parse(readFileSync(new URL('../collections.schema.json', import.meta.url), 'utf8'));
assert.equal(schemaDoc.schema_format, 'ctox-business-os-module-collections-v1');
const moduleCollections = (manifest.collections || []).filter((name) => !shellCollections.has(name));
assert.ok(moduleCollections.length > 0, 'module owns at least one collection');
for (const name of moduleCollections) {
  assert.ok(schemaDoc.collections?.[name], 'schema exists for ' + name);
}

const sample = {
  id: 'rec_demo',
  title: 'Demo record',
  status: 'open',
  item: { id: 'item_demo', sku: 'SKU-1', name: 'Demo Item', title: 'Demo Item' },
  lowRows: [],
  records: [],
};
const visible = visibleRecords([sample]);
const summary = summarizeRecords([sample]);
assert.equal(visible.length, 1);
assert.equal(summary.total, 1);
const candidateArgs = [
  [sample],
  [sample.item],
  [sample.item, sample.lowRows],
  [sample.item, sample.lowRows, sample],
  [],
];
const builders = Object.entries(automation)
  .filter(([name, value]) => typeof value === 'function' && /command|follow|task|automation/i.test(name));
let command = null;
for (const [, build] of builders) {
  for (const args of candidateArgs) {
    try {
      const result = build(...args);
      if (result && typeof result === 'object' && (result.type === 'business_os.chat.task' || result.command_type === 'business_os.chat.task')) {
        command = result;
        break;
      }
    } catch {
      // Try the next candidate shape.
    }
  }
  if (command) break;
}
assert.ok(command, 'automation exports a CTOX chat task command builder');
assert.equal(command.type, 'business_os.chat.task');
assert.equal(command.command_type, 'business_os.chat.task');
assert.ok(command.payload && Object.prototype.hasOwnProperty.call(command.payload, 'record_snapshot'));
`;
}

function readJson(path, fallback) {
  if (!existsSync(path)) return fallback;
  return JSON.parse(readFileSync(path, 'utf8'));
}

function existingManifestFor(dir) {
  return readJson(join(dir, 'module.json'), null) || {};
}

function existingSchemaDocFor(dir) {
  return readJson(join(dir, 'collections.schema.json'), null) || {};
}

function primaryCollectionFor(options, dir) {
  const manifest = existingManifestFor(dir);
  const manifestCollection = (Array.isArray(manifest.collections) ? manifest.collections : [])
    .find((name) => typeof name === 'string' && !shellCollections.has(name));
  if (manifestCollection) return manifestCollection;
  const schemaDoc = existingSchemaDocFor(dir);
  const schemaCollection = Object.keys(schemaDoc.collections || {})
    .find((name) => !shellCollections.has(name));
  return schemaCollection || options.collection;
}

function repairOptionsFor(options, dir) {
  const manifest = existingManifestFor(dir);
  return {
    ...options,
    title: typeof manifest.title === 'string' && manifest.title.trim() ? manifest.title.trim() : options.title,
    description: typeof manifest.description === 'string' && manifest.description.trim()
      ? manifest.description.trim()
      : options.description,
    collection: primaryCollectionFor(options, dir),
  };
}

function writeAtomicIfMissing(dir, relativePath, content, written) {
  const path = join(dir, relativePath);
  if (existsSync(path)) return;
  writeAtomic(path, content);
  written.push(relativePath);
}

function writeJsonIfMissing(dir, relativePath, value, written) {
  const path = join(dir, relativePath);
  if (existsSync(path)) return;
  writeJson(path, value);
  written.push(relativePath);
}

function hasTestFile(dir) {
  const testDir = join(dir, 'tests');
  if (!existsSync(testDir)) return false;
  return readdirSync(testDir).some((name) => name.endsWith('.test.mjs'));
}

function writeRepairMissingScaffold(options) {
  const dir = moduleDirFor(options);
  mkdirSync(join(dir, 'core'), { recursive: true });
  mkdirSync(join(dir, 'locales'), { recursive: true });
  mkdirSync(join(dir, 'tests'), { recursive: true });
  const repairOptions = repairOptionsFor(options, dir);
  const manifest = moduleManifest(repairOptions);
  const written = [];

  writeJsonIfMissing(dir, 'module.json', manifest, written);
  writeJsonIfMissing(dir, 'collections.schema.json', {
    schema_format: 'ctox-business-os-module-collections-v1',
    collections: {
      [repairOptions.collection]: schemaFor(repairOptions.collection),
    },
  }, written);
  writeAtomicIfMissing(dir, 'schema.js', schemaJs(repairOptions), written);
  writeAtomicIfMissing(dir, 'core/automation.mjs', automationJs(repairOptions), written);
  writeAtomicIfMissing(dir, 'core/records.mjs', recordsJs(repairOptions), written);
  writeAtomicIfMissing(dir, 'index.html', indexHtml(repairOptions), written);
  writeAtomicIfMissing(dir, 'index.css', indexCss(repairOptions), written);
  writeAtomicIfMissing(dir, 'index.js', indexJs(repairOptions), written);
  writeAtomicIfMissing(dir, 'icon.svg', iconSvg(), written);
  writeJsonIfMissing(dir, 'locales/de.json', localeDe(repairOptions), written);
  writeJsonIfMissing(dir, 'locales/en.json', localeEn(repairOptions), written);
  if (!hasTestFile(dir)) {
    writeAtomic(join(dir, `tests/${options.moduleId}.contract.test.mjs`), contractTestJs(repairOptions));
    written.push(`tests/${options.moduleId}.contract.test.mjs`);
  }

  return {
    ok: true,
    repaired: true,
    module_id: options.moduleId,
    mode: options.mode,
    module_dir: rel(options.workspace, dir),
    collection: repairOptions.collection,
    files: written,
  };
}

function updateRegistry(options, manifest) {
  if (options.mode !== 'source') return null;
  const registryPath = join(options.workspace, 'src/apps/business-os/modules/registry.json');
  const registry = readJson(registryPath, { modules: [] });
  const modules = Array.isArray(registry.modules) ? registry.modules : [];
  const nextEntry = {
    id: manifest.id,
    title: manifest.title,
    entry: manifest.entry,
    install_scope: manifest.install_scope,
    collections: manifest.collections,
  };
  const nextModules = modules.filter((item) => item?.id !== manifest.id).concat(nextEntry);
  writeJson(registryPath, { ...registry, modules: nextModules });
  return registryPath;
}

function writeScaffold(options) {
  const dir = moduleDirFor(options);
  assertWritableTarget(dir, options.force);
  if (options.force && existsSync(dir)) {
    rmSync(dir, { recursive: true, force: true });
  }
  mkdirSync(join(dir, 'core'), { recursive: true });
  mkdirSync(join(dir, 'locales'), { recursive: true });
  mkdirSync(join(dir, 'tests'), { recursive: true });

  const manifest = moduleManifest(options);
  writeJson(join(dir, 'module.json'), manifest);
  writeJson(join(dir, 'collections.schema.json'), {
    schema_format: 'ctox-business-os-module-collections-v1',
    collections: {
      [options.collection]: schemaFor(options.collection),
    },
  });
  writeAtomic(join(dir, 'schema.js'), schemaJs(options));
  writeAtomic(join(dir, 'core/automation.mjs'), automationJs(options));
  writeAtomic(join(dir, 'core/records.mjs'), recordsJs(options));
  writeAtomic(join(dir, 'index.html'), indexHtml(options));
  writeAtomic(join(dir, 'index.css'), indexCss(options));
  writeAtomic(join(dir, 'index.js'), indexJs(options));
  writeAtomic(join(dir, 'icon.svg'), iconSvg());
  writeJson(join(dir, 'locales/de.json'), localeDe(options));
  writeJson(join(dir, 'locales/en.json'), localeEn(options));
  writeAtomic(join(dir, `tests/${options.moduleId}.test.mjs`), testJs(options));

  const written = [
    'module.json',
    'collections.schema.json',
    'schema.js',
    'core/automation.mjs',
    'core/records.mjs',
    'index.html',
    'index.css',
    'index.js',
    'icon.svg',
    'locales/de.json',
    'locales/en.json',
    `tests/${options.moduleId}.test.mjs`,
  ];
  let registryPath = null;
  if (options.mode === 'source') {
    writeAtomic(join(dir, 'README.md'), `# ${options.title}\n\nBusiness OS app module scaffold.\n`);
    const planPath = join(options.workspace, 'docs', `business-os-${options.moduleId}-implementation-plan.md`);
    writeAtomic(planPath, [
      `# ${options.title} Implementation Plan`,
      '',
      '- Keep the module focused on one primary record workflow.',
      '- Persist durable records through the declared module collection.',
      '- Validate the module before release.',
      '',
    ].join('\n'));
    registryPath = updateRegistry(options, manifest);
    written.push('README.md', rel(options.workspace, planPath));
    if (registryPath) written.push(rel(options.workspace, registryPath));
  }
  return {
    ok: true,
    module_id: options.moduleId,
    mode: options.mode,
    module_dir: rel(options.workspace, dir),
    collection: options.collection,
    files: written,
  };
}

let options;
try {
  options = parseArgs(process.argv.slice(2));
  const result = options.repairMissing
    ? writeRepairMissingScaffold(options)
    : writeScaffold(options);
  if (options.json) {
    console.log(JSON.stringify(result, null, 2));
  } else {
    console.log(`Business OS app scaffold ${result.repaired ? 'repair' : 'OK'}: ${result.module_id} (${result.mode} mode)`);
    console.log(`module_dir: ${result.module_dir}`);
    console.log(`collection: ${result.collection}`);
  }
} catch (error) {
  console.error(error.message);
  console.error(usage());
  process.exit(1);
}
