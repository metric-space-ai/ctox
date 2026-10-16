import { autoWirePaneGrammar } from 'ctox:shared/pane-grammar.js';
import { contextChatModeMarkup, normalizeContextChatMode, sendContextChat } from 'ctox:shared/context-chat.js';
import { createOfficeBridgeV4, putFile } from './office-bridge-v4.js';

const CSV_MIME = 'text/csv';
const TSV_MIME = 'text/tab-separated-values';
const XLSX_MIME = 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet';
const SPREADSHEET_RENDER_DEBOUNCE_MS = 80;
const SUPPORTED_IMPORT_EXTENSIONS = ['.csv', '.tsv', '.xlsx'];
// Automation remains an explicit action, never a permanent workspace column.
const USER_IMPORT_KIND = 'user_import';
const RESEARCH_GENERATED_KIND = 'research_generated';
const UNRESOLVED_SOURCE_KIND = 'unresolved_source';

const DEFAULT_GRID_DATA = [
  ['Produkt', 'Q1 Sales', 'Q2 Sales', 'Q3 Sales', 'Q4 Sales', 'Gesamt'],
  ['Premium Widget', '12500', '14200', '15800', '18900', '=SUM(B2:E2)'],
  ['Standard Gadget', '8400', '9100', '9800', '10500', '=SUM(B3:E3)'],
  ['Basic Service', '2300', '2500', '2900', '3100', '=SUM(B4:E4)'],
  ['Total', '=SUM(B2:B4)', '=SUM(C2:C4)', '=SUM(D2:D4)', '=SUM(E2:E4)', '=SUM(F2:F4)']
];

const DEFAULT_GRID_COLUMNS = [
  { type: 'text', title: 'A', width: '150px' },
  { type: 'numeric', title: 'B', width: '100px', mask: '$ #.##0,00' },
  { type: 'numeric', title: 'C', width: '100px', mask: '$ #.##0,00' },
  { type: 'numeric', title: 'D', width: '100px', mask: '$ #.##0,00' },
  { type: 'numeric', title: 'E', width: '100px', mask: '$ #.##0,00' },
  { type: 'numeric', title: 'F', width: '120px', mask: '$ #.##0,00' }
];

const BLANK_GRID_DATA = [Array.from({ length: 10 }, () => '')];
const BLANK_GRID_COLUMNS = Array.from({ length: 10 }, (_, index) => ({
  type: 'text',
  title: String.fromCharCode(65 + index),
  width: '120px',
}));

// Runbook-Auswahl; die Anweisungstexte liegen im App-Paket (prompts/<id>.md, Strang SKL-B3), die Handler
// spreadsheet.* füllen sie. prompt_template verweist nur noch auf das Template.
const SYSTEMATIC_SPREADSHEET_RUNBOOKS = [
  {
    id: 'spreadsheet.summarize',
    document_type: 'spreadsheet',
    title: 'Tabelle zusammenfassen',
    description: 'Fasse das ausgewählte Spreadsheet strukturiert zusammen, analysiere Gesamtsummen und identifiziere Trends.', // v4-allow: text – Runbook-Auftrag an den Agenten
    command_type: 'spreadsheet.summarize',
    prompt_template: 'prompts/spreadsheet-summarize.md'
  },
  {
    id: 'spreadsheet.audit-formulas',
    document_type: 'spreadsheet',
    title: 'Formeln auditieren',
    description: 'Finde sämtliche Formeln in dieser Tabelle und prüfe sie auf Fehler, Zirkelbezüge oder logische Inkonsistenzen.', // v4-allow: text – Runbook-Auftrag an den Agenten
    command_type: 'spreadsheet.audit-formulas',
    prompt_template: 'prompts/spreadsheet-audit-formulas.md'
  },
  {
    id: 'spreadsheet.risk-review',
    document_type: 'spreadsheet',
    title: 'Finanzielle Risikoanalyse',
    description: 'Identifiziere finanzielle Risiken, unplausible Kennzahlen, starke Margenabweichungen und auffällige Transaktionen.', // v4-allow: text – Runbook-Auftrag an den Agenten
    command_type: 'spreadsheet.risk-review',
    prompt_template: 'prompts/spreadsheet-risk-review.md'
  }
];

const SNAPSHOT_VIEW_KEYS = Object.freeze(['selectedId', 'searchQuery', 'statusFilter', 'tagFilter', 'sortBy', 'listView', 'libraryOpen']);

function langFrom(locale) {
  return String(locale || '').toLowerCase().startsWith('en') ? 'en' : 'de';
}

// api.shell.t: 'spreadsheets.<key>' übersteuert, sonst eingebetteter Rückfall (app-api.d.ts OFFEN-B).
function createTranslator(api) {
  return (key, fallback, ...args) => {
    const full = `spreadsheets.${key}`;
    let translated = '';
    try { translated = api.shell.t(full); } catch {}
    let val = translated && translated !== full ? translated : (fallback ?? key);
    args.forEach((arg, i) => { val = String(val).replace(`{${i}}`, arg); });
    return val;
  };
}

// Abos und Zustand je Montage (PORTING §4 Falle 1).
export async function mount(root, api, { restored } = {}) {
  if (!root.dataset.app) root.dataset.app = api.app?.id || 'spreadsheets';
  const t = createTranslator(api);
  // CSS lädt die Shell aus dem Manifest; index.html liefert der Service Worker versioniert (§16.15).
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  root.innerHTML = html;
  api.shell.setTitle(t('spreadsheetsTitle', 'Tabellen'));

  const state = {
    api,
    root,
    disposers: [],
    officeBridge: createOfficeBridgeV4(api, 'spreadsheet'),
    ctoxSpreadsheetsModule: null,
    officeEngine: 'ctox_spreadsheets',
    spreadsheets: [],
    rawSpreadsheets: [],
    runbooks: mergeSpreadsheetRunbooks([]),
    selectedId: '',
    selectedVersion: null,
    versionLoad: null,
    versionSubscription: null,
    disposed: false,
    editorHandle: null,
    spreadsheetContainer: null,
    renderSerial: 0,
    switchSerial: 0,
    dirty: false,
    saving: false,
    searchQuery: '',
    statusFilter: 'all',
    tagFilter: 'all',
    sortBy: 'updated_desc',
    filtersOpen: false,
    libraryOpen: false,
    listView: 'list',
    openFilePromise: Promise.resolve(),
    contextMenu: null,
    contextMenuCleanup: null,
    t,
    lang: langFrom(api.shell.locale.get()),
  };
  applyRestoredView(state, plainSnapshot(restored));

  state.disposers.push(wireLocalRealtime(state));
  state.disposers.push(api.shell.locale.subscribe((locale) => {
    const lang = langFrom(locale);
    if (lang === state.lang) return;
    state.lang = lang;
    renderLeft(state);
  }));
  state.disposers.push(api.shell.onRoute((event) => handleRoute(state, event?.params || {})));
  const onBeforeUnload = (event) => {
    const handle = state.editorHandle;
    if (handle?.kind !== 'ctox-spreadsheets'
      || !(state.dirty || state.saving || handle.saving || handle.saveTimer != null)) return;
    event.preventDefault();
    event.returnValue = '';
  };
  window.addEventListener('beforeunload', onBeforeUnload);
  ensureSeedRunbooks(state);
  if (state.selectedId) await loadSelectedVersion(state).catch((error) => reportSpreadsheetError(state, 'spreadsheets.version.load', error));
  renderLeft(state);
  renderRight(state);
  renderCenter(state);

  return {
    async unmount() {
      state.disposed = true;
      window.removeEventListener('beforeunload', onBeforeUnload);
      state.contextMenuCleanup?.();
      state.contextMenu?.remove();
      state.contextMenu = null;
      state.versionSubscription?.off?.();
      state.versionSubscription = null;
      for (const dispose of state.disposers.splice(0)) {
        try { dispose(); } catch (error) { console.warn('[spreadsheets] unsubscribe failed', error); }
      }
      state.officeBridge.dispose();
      if (state.editorHandle?.kind === 'ctox-spreadsheets') {
        await state.editorHandle.destroy().catch((error) => console.error('[spreadsheets] editor cleanup failed', error));
      }
      state.editorHandle = null;
      root.replaceChildren();
    },
    snapshot: () => spreadsheetsSnapshot(state),
  };
}

function plainSnapshot(value) {
  return value && typeof value === 'object' && !Array.isArray(value) ? value : null;
}

// Fensterzustand für den Live-Remount (PORTING §2): Auswahl, Suche, Filter, Ansicht, Listen-Scroll.
function spreadsheetsSnapshot(state) {
  const snap = { v: 1 };
  for (const key of SNAPSHOT_VIEW_KEYS) snap[key] = state[key];
  snap.listScrollTop = state.root?.querySelector('[data-spreadsheets-list]')?.scrollTop || 0;
  return snap;
}

function applyRestoredView(state, restored) {
  if (!restored || restored.v !== 1) return;
  for (const key of SNAPSHOT_VIEW_KEYS) {
    if (key in restored && typeof restored[key] === typeof state[key]) state[key] = restored[key];
  }
  if (Number.isFinite(restored.listScrollTop)) state.pendingListScrollTop = restored.listScrollTop;
}

// Deep-Link über api.shell.onRoute statt eventBus 'desktop-app:open-file': {record|spreadsheetId, file_ref|ref,
// name, mime, sourceFileId, …}. Unbekannter Datensatz → Zustandszone (R13).
function handleRoute(state, params = {}) {
  if (state.disposed) return;
  const fileRef = String(params.file_ref || params.ref || '').trim();
  if (fileRef) enqueueSpreadsheetOpenFile(state, { ...params, ref: fileRef });
  const recordId = String(params.record || params.spreadsheetId || '').trim();
  if (!recordId) return;
  const code = 'spreadsheets.route.unknown_record';
  if (!state.spreadsheets.some((record) => record.id === recordId)) {
    state.api.status.report({ level: 'warn', code, text: state.t('linkedSpreadsheetUnavailable', 'Verknüpfte Tabelle {0} ist hier nicht vorhanden.', recordId) });
    return;
  }
  state.api.status.clear(code);
  if (state.selectedId === recordId) return;
  saveSpreadsheetBeforeLeaving(state).then((ok) => {
    if (!ok || state.disposed) return;
    state.selectedId = recordId;
    state.selectedVersion = null;
    return loadSelectedVersion(state).then(() => { renderLeft(state); renderCenter(state); });
  }).catch((error) => reportSpreadsheetError(state, 'spreadsheets.route.open', error));
}

// Fehler und Hinweise nur in die Zustandszone (R11); stabile Codes je Fall.
function reportSpreadsheetError(state, code, error, prefix = '') {
  if (state.disposed) return;
  const message = error?.message || String(error || '');
  state.api.status.report({ level: 'error', code, text: `${prefix ? `${prefix} ` : ''}${message}` });
}

function enqueueSpreadsheetOpenFile(state, input) {
  if (!input?.file && !input?.ref) return state.openFilePromise;
  state.openFilePromise = state.openFilePromise
    .then(() => openSpreadsheetFile(state, input))
    .catch((error) => {
      console.error('[spreadsheets] opening file from Files failed', error);
      reportSpreadsheetError(state, 'spreadsheets.file.open', error, state.t('fileOpenFailed', 'Datei konnte nicht geöffnet werden:'));
      return null;
    });
  return state.openFilePromise;
}

async function openSpreadsheetFile(state, input) {
  const selectedId = state.selectedId;
  const handle = state.editorHandle;
  if (!await saveSpreadsheetBeforeLeaving(state)) return;
  if (state.selectedId !== selectedId || state.editorHandle !== handle) return;
  const file = input?.file || await fileFromRef(state, input);
  if (state.disposed) return;
  const validation = validateImportInput({ file });
  if (!validation.valid) throw new Error(state.t(validation.key, validation.message));
  const ingestion = await resolveSpreadsheetIngestion(state, input);
  assertSpreadsheetIngestionAllowed(ingestion);
  const bytes = new Uint8Array(await file.arrayBuffer());
  const sourceSha = await sha256Hex(bytes);
  assertSpreadsheetSnapshotBytes(ingestion, sourceSha);
  const existing = spreadsheetBySourceSha(state.spreadsheets, sourceSha, ingestion);
  if (state.selectedId !== selectedId || state.editorHandle !== handle) return;
  if (!await saveSpreadsheetBeforeLeaving(state)) return;
  if (state.selectedId !== selectedId || state.editorHandle !== handle) return;
  if (existing) {
    state.selectedId = existing.id;
    state.selectedVersion = null;
    await loadSelectedVersion(state);
    renderLeft(state);
    renderRight(state);
    renderCenter(state);
    return existing;
  }
  await importSpreadsheetFile(state, file, [], ingestion);
  return selectedRecord(state);
}

function spreadsheetBySourceSha(records = [], sourceSha = '', ingestion = null) {
  const expected = String(sourceSha || '').trim().toLowerCase();
  if (!expected) return null;
  return records.find((record) => String(record?.source_sha256 || '').trim().toLowerCase() === expected
    && (!ingestion || spreadsheetIngestionMatches(record, ingestion))) || null;
}

function spreadsheetIngestionMatches(record, ingestion) {
  const lineage = record?.knowledge_lineage || {};
  const research = isResearchDerivedSpreadsheet([record], arrayValue(record?.linked_records));
  if (ingestion.kind !== RESEARCH_GENERATED_KIND) {
    return !research && lineage.open_purpose !== 'snapshot_report';
  }
  return research && lineage.open_purpose === 'snapshot_report'
    && lineage.evidence_eligible === false
    && JSON.stringify(normalizeReportSnapshot(lineage.report_snapshot))
      === JSON.stringify(ingestion.report_snapshot);
}

// Datei aus api.files öffnen: ref → Blob; Name/Typ aus dem files-Dokument.
async function fileFromRef(state, input) {
  const ref = String(input.ref || '').trim();
  const meta = state.api.query('files', { selector: { ref }, limit: 1 }).get()[0] || null;
  const blob = await state.api.files.get(ref);
  const name = String(input.name || meta?.name || 'tabelle.xlsx');
  return new File([blob], name, { type: String(input.mime || meta?.mime || blob.type || '') });
}

// Lesevorschau (FIX9b): Raster aus model_json der Version (erste 200 Zeilen, 26 Spalten), nur lesen. Ohne Daten
// (z. B. XLSX vor dem Office-Prepare) bleibt sie verborgen – kein Wartetext.
const READ_PREVIEW_MAX_ROWS = 200;
const READ_PREVIEW_MAX_COLS = 26;
function spreadsheetReadPreviewHtml(version) {
  const data = Array.isArray(version?.model_json?.data) ? version.model_json.data : [];
  const rows = data.slice(0, READ_PREVIEW_MAX_ROWS).map((row) => (Array.isArray(row) ? row : [row]));
  if (!rows.some((row) => row.some((cell) => String(cell ?? '').trim()))) return '';
  const cols = Math.min(READ_PREVIEW_MAX_COLS, Math.max(1, ...rows.map((row) => row.length)));
  const letters = Array.from({ length: cols }, (_, i) => String.fromCharCode(65 + i));
  const head = `<thead><tr><th scope="col"></th>${letters.map((l) => `<th scope="col">${l}</th>`).join('')}</tr></thead>`;
  const body = rows.map((row, r) => `<tr><th scope="row">${r + 1}</th>${letters.map((_, c) => {
    const value = row[c] ?? '';
    const numeric = value !== '' && Number.isFinite(Number(value));
    return `<td${numeric ? ' class="is-num"' : ''}>${escapeHtml(value)}</td>`;
  }).join('')}</tr>`).join('');
  return `<table>${head}<tbody>${body}</tbody></table>`;
}

function renderSpreadsheetReadPreview(state, host, version) {
  host?.querySelector?.('[data-spreadsheets-read-preview]')?.remove();
  const html = spreadsheetReadPreviewHtml(version);
  if (!html || typeof host?.prepend !== 'function') return null;
  const preview = document.createElement('div');
  preview.className = 'spreadsheets-read-preview';
  preview.dataset.spreadsheetsReadPreview = '';
  preview.setAttribute('role', 'region');
  preview.setAttribute('aria-label', state.t('readPreview', 'Lesevorschau'));
  preview.innerHTML = html;
  host.prepend(preview);
  return preview;
}

// Vorwärmen (FIX9b): das Office-Bibliotheksmodul lädt im Leerlauf, sobald Tabellen da sind; die
// Bibliothek selbst bleibt unverändert.
function scheduleOfficePrewarm(state) {
  if (state.officePrewarmScheduled || state.ctoxSpreadsheetsModule || !state.spreadsheets?.length) return;
  state.officePrewarmScheduled = true;
  const run = () => {
    if (state.disposed) return;
    loadCtoxSpreadsheetsModule(state).catch(() => { state.officePrewarmScheduled = false; });
  };
  if (typeof requestIdleCallback === 'function') requestIdleCallback(run, { timeout: 2000 });
  else setTimeout(run, 200);
}

async function loadCtoxSpreadsheetsModule(state) {
  if (!state.ctoxSpreadsheetsModule) {
    state.ctoxSpreadsheetsModule = await import('ctox:lib/ctox-office@1/ctox-office-spreadsheet.mjs');
  }
  return state.ctoxSpreadsheetsModule;
}

// Live-Abos statt ctx.db, Readiness und Demand-Loadern (R3/R4); 80 ms gebündeltes Rendern.
function wireLocalRealtime(state) {
  const api = state.api;
  const changed = new Set();
  let timer = null;
  let disposed = false;
  const flush = () => {
    timer = null;
    if (disposed || state.disposed) return;
    const batch = new Set(changed);
    changed.clear();
    refreshSpreadsheetsFromLocal(state, batch, () => !disposed && !state.disposed)
      .catch((error) => reportSpreadsheetError(state, 'spreadsheets.render', error));
  };
  state.notifyLocalChange = (name) => {
    changed.add(name);
    if (!timer) timer = setTimeout(flush, SPREADSHEET_RENDER_DEBOUNCE_MS);
  };
  const offs = [];
  offs.push(api.query('spreadsheets', { sort: [{ updated_at_ms: 'desc' }] }).subscribe((rows) => {
    state.rawSpreadsheets = Array.isArray(rows) ? rows : [];
    refreshSpreadsheets(state);
    state.notifyLocalChange('spreadsheets');
  }));
  offs.push(api.query('spreadsheet_runbooks', { sort: [{ title: 'asc' }] }).subscribe((rows) => {
    state.runbooks = mergeSpreadsheetRunbooks(Array.isArray(rows) ? rows : []);
    state.notifyLocalChange('spreadsheet_runbooks');
  }));
  return () => {
    disposed = true;
    if (timer) clearTimeout(timer);
    timer = null;
    state.notifyLocalChange = () => {};
    for (const off of offs.splice(0)) {
      try { off(); } catch {}
    }
  };
}

// Ein Versionsabo je Auswahl: neue/geänderte Versionen erscheinen live (refreshSpreadsheetsFromLocal).
function ensureVersionSubscription(state, spreadsheetId) {
  if (state.disposed) return;
  const id = String(spreadsheetId || '');
  if (state.versionSubscription?.spreadsheetId === id) return;
  state.versionSubscription?.off?.();
  state.versionSubscription = null;
  if (!id) return;
  const off = state.api.query('spreadsheet_versions', { selector: { spreadsheet_id: id } }).subscribe(() => {
    state.notifyLocalChange?.('spreadsheet_versions');
  });
  state.versionSubscription = { spreadsheetId: id, off };
}

function spreadsheetDraftProtected(state) {
  const handle = state.editorHandle;
  return handle?.kind === 'ctox-spreadsheets'
    && handle.recordId === state.selectedId
    && Boolean(state.dirty || state.saving || handle.saving);
}

async function saveSpreadsheetBeforeLeaving(state) {
  const handle = state.editorHandle;
  const selectedId = state.selectedId;
  if (handle?.kind === 'ctox-spreadsheets' && (state.dirty || state.saving || handle.saving)) {
    await handle.save({ reason: 'final' });
    if (state.editorHandle !== handle || state.selectedId !== selectedId) return false;
    if (state.dirty || state.saving || handle.saving) {
      throw new Error(state.t('spreadsheetChangesRemainUnsaved', 'Weitere Änderungen sind noch ungespeichert. Bitte nach dem Speichern erneut versuchen.'));
    }
  }
  return true;
}

async function refreshSpreadsheetsFromLocal(state, changed = null, isActive = () => true) {
  if (!isActive()) return;
  const all = changed === null;
  if ((all || changed.has('spreadsheets') || changed.has('spreadsheet_versions'))
    && state.selectedId && !spreadsheetDraftProtected(state)
    && state.selectedVersion?.id !== selectedRecord(state)?.current_version_id) {
    const load = currentSpreadsheetVersionLoad(state);
    const version = load?.status === 'error' ? null : await loadSelectedVersion(state, isActive);
    if (!isActive()) return;
    if (version || currentSpreadsheetVersionLoad(state)?.status === 'error') await renderCenter(state);
  }
  if (!isActive()) return;
  renderLeft(state);
  renderRight(state);
}

function refreshSpreadsheets(state) {
  const selectedBeforeRefresh = selectedRecord(state);
  state.spreadsheets = (state.rawSpreadsheets || [])
    .map((doc) => normalizeSpreadsheetRecord(doc))
    .filter(isActiveSpreadsheetRecord);
  scheduleOfficePrewarm(state);

  if (state.selectedId && !state.spreadsheets.some((record) => record.id === state.selectedId)) {
    // Die Live-Abfrage ist lokal vollständig: fehlt der Datensatz, ist er gelöscht oder nie angekommen.
    // Offener Entwurf im Editor hält die Auswahl (wie vorher über requireRevision).
    if (selectedBeforeRefresh && spreadsheetDraftProtected(state)) {
      state.spreadsheets.unshift(selectedBeforeRefresh);
    } else {
      state.selectedId = state.spreadsheets[0]?.id || '';
      state.selectedVersion = null;
    }
  }
  if (!state.selectedId && state.spreadsheets[0]) state.selectedId = state.spreadsheets[0].id;
  ensureVersionSubscription(state, state.selectedId);
}

function mergeSpreadsheetRunbooks(stored = []) {
  const byId = new Map();
  [...SYSTEMATIC_SPREADSHEET_RUNBOOKS, ...stored].forEach((runbook) => {
    const id = runbook.id || runbook.command_type;
    if (!id) return;
    byId.set(id, {
      ...runbook,
      id,
      document_type: runbook.document_type || 'spreadsheet',
      title: runbook.title || id,
      command_type: runbook.command_type || id,
      prompt_template: runbook.prompt_template || runbook.description || '',
    });
  });
  return Array.from(byId.values()).sort((a, b) => String(a.title || '').localeCompare(String(b.title || '')));
}

function selectedRecord(state) {
  return state.spreadsheets.find((item) => item.id === state.selectedId) || null;
}

async function createNewSpreadsheet(state, input = {}) {
  const selectedId = state.selectedId;
  const handle = state.editorHandle;
  if (!await saveSpreadsheetBeforeLeaving(state)) return;
  if (state.selectedId !== selectedId || state.editorHandle !== handle) return;
  const title = sanitizeTitle(input.title || nextBlankSpreadsheetTitle(state));
  if (!title) throw new Error(state.t('validationTitleRequired', 'Titel fehlt.'));
  const filename = ensureExtension(slugFilename(title), '.csv');
  const documentId = `sheet_${crypto.randomUUID()}`;
  const versionId = `${documentId}_v1`;
  const blobId = `${versionId}_blob`;
  const now = Date.now();

  const modelJson = {
    data: input.data || BLANK_GRID_DATA,
    columns: input.columns || BLANK_GRID_COLUMNS,
    nestedHeaders: input.nestedHeaders || null,
    mergeCells: input.mergeCells || null,
    style: input.style || null
  };

  // Convert to CSV string representation for raw blob persist
  const csvText = rowsToCsv(modelJson.data);
  const bytes = new TextEncoder().encode(csvText);

  const sourceSha = await sha256Hex(bytes);
  await saveBlobChunks(state, {
    blobId,
    spreadsheetId: documentId,
    versionId,
    mimeType: CSV_MIME,
    bytes,
    name: filename,
  });

  await state.api.put('spreadsheet_versions', {
    id: versionId,
    spreadsheet_id: documentId,
    version: 1,
    source_kind: 'created_blank',
    blob_id: blobId,
    source_sha256: sourceSha,
    model_json: modelJson,
    diagnostics: [],
    created_at_ms: now,
    updated_at_ms: now,
  }).local;

  await state.api.put('spreadsheets', {
    id: documentId,
    title,
    filename,
    mime_type: CSV_MIME,
    status: 'Draft',
    spreadsheet_type: 'csv',
    owner_id: '',
    current_version_id: versionId,
    source_sha256: sourceSha,
    row_count: input.data ? modelJson.data.length : 0,
    col_count: input.data ? (modelJson.data[0]?.length || 0) : 0,
    diagnostics_count: 0,
    linked_records: [],
    tags: normalizeTags(input.tags),
    display_cache: {},
    index_text: title,
    is_deleted: false,
    created_at_ms: now,
    updated_at_ms: now,
  }).local;

  if (state.selectedId !== selectedId || state.editorHandle !== handle) return;
  if (!await saveSpreadsheetBeforeLeaving(state)) return;
  if (state.selectedId !== selectedId || state.editorHandle !== handle) return;
  state.selectedId = documentId;
  revealSelectedSpreadsheetInList(state);
  refreshSpreadsheets(state);
  await loadSelectedVersion(state);
  renderLeft(state);
  renderRight(state);
  renderCenter(state);
}

function nextBlankSpreadsheetTitle(state) {
  const base = state.t('newDocumentTitle', 'Neue Tabelle');
  const titles = new Set(state.spreadsheets.map((record) => String(record.title || '').trim().toLocaleLowerCase()));
  if (!titles.has(base.toLocaleLowerCase())) return base;
  let suffix = 2;
  while (titles.has(`${base} ${suffix}`.toLocaleLowerCase())) suffix += 1;
  return `${base} ${suffix}`;
}

async function requestBlankSpreadsheet(state) {
  if (state.creatingBlankSpreadsheet) return;
  state.creatingBlankSpreadsheet = true;
  try {
    await createNewSpreadsheet(state);
    state.api.status.clear('spreadsheets.create');
  } catch (error) {
    console.error('[spreadsheets] blank spreadsheet creation failed', error);
    reportSpreadsheetError(state, 'spreadsheets.create', error, state.t('spreadsheetCreateFailed', 'Tabelle konnte nicht erstellt werden:'));
  } finally {
    state.creatingBlankSpreadsheet = false;
  }
}

async function importSpreadsheetFile(state, file, tags = [], ingestionInput = {}) {
  const selectedId = state.selectedId;
  const handle = state.editorHandle;
  if (!await saveSpreadsheetBeforeLeaving(state)) return;
  if (state.selectedId !== selectedId || state.editorHandle !== handle) return;
  const ingestion = normalizeSpreadsheetIngestion(ingestionInput.file ? ingestionInput : { ...ingestionInput, file });
  assertSpreadsheetIngestionAllowed(ingestion);
  const validation = validateImportInput({ file });
  if (!validation.valid) {
    throw new Error(state.t(validation.key, validation.message));
  }
  const isXlsx = file.name.toLowerCase().endsWith('.xlsx') || file.type === XLSX_MIME;
  const isTsv = file.name.toLowerCase().endsWith('.tsv') || file.type === TSV_MIME;
  const bytes = new Uint8Array(await file.arrayBuffer());
  const sourceSha = await sha256Hex(bytes);
  // Snapshot-Digest prüft allein openSpreadsheetFile (vor Wiederverwendung UND Import); nur dieser Weg trägt
  // open_purpose 'snapshot_report' hierher, der Upload-Weg nie.
  const fileText = isXlsx ? '' : new TextDecoder().decode(bytes);

  const documentId = `sheet_${crypto.randomUUID()}`;
  const versionId = `${documentId}_v1`;
  const blobId = `${versionId}_blob`;
  const now = Date.now();

  let modelJson = { data: DEFAULT_GRID_DATA, columns: DEFAULT_GRID_COLUMNS };

  if (isXlsx) {
    modelJson = {
      format: 'xlsx',
      summary: { filename: file.name, bytes: bytes.byteLength },
      data: [],
      columns: [],
    };
  } else {
    // Parse delimited text for list metadata. The native Office prepare step
    // remains authoritative for the canonical XLSX representation.
    try {
      const rows = parseCSVContent(fileText, isTsv ? '\t' : ',');
      if (rows.length > 0) {
        modelJson.data = rows;
        const colCount = Math.max(...rows.map(r => r.length), 1);
        modelJson.columns = Array.from({ length: colCount }, (_, i) => ({ type: 'text', title: String.fromCharCode(65 + i), width: '120px' }));
      } else {
        throw new Error(state.t('validationEmptySpreadsheet', 'Die Datei enthält keine Tabellenzeilen.'));
      }
    } catch (err) {
      console.warn('Failed parsing delimited spreadsheet.', err);
      throw err;
    }
  }
  if (!isXlsx && (!Array.isArray(modelJson.data) || modelJson.data.length === 0)) {
    throw new Error(state.t('validationEmptySpreadsheet', 'Die Datei enthält keine Tabellenzeilen.'));
  }
  modelJson = normalizeSpreadsheetModel(modelJson);

  await saveBlobChunks(state, {
    blobId,
    spreadsheetId: documentId,
    versionId,
    mimeType: isXlsx ? XLSX_MIME : isTsv ? TSV_MIME : CSV_MIME,
    bytes,
    name: file.name,
  });

  await state.api.put('spreadsheet_versions', {
    id: versionId,
    spreadsheet_id: documentId,
    version: 1,
    source_kind: ingestion.kind === RESEARCH_GENERATED_KIND
      ? RESEARCH_GENERATED_KIND
      : isXlsx ? 'imported_xlsx' : isTsv ? 'imported_tsv' : 'imported_csv',
    ingestion_kind: ingestion.kind,
    blob_id: blobId,
    source_sha256: sourceSha,
    model_json: modelJson,
    diagnostics: [],
    linked_records: ingestion.linkedRecords,
    source_receipt_snapshot_hashes: ingestion.sourceReceiptSnapshotHashes,
    knowledge_version: ingestion.knowledgeVersion,
    knowledge_lineage: ingestion.knowledgeLineage,
    created_at_ms: now,
    updated_at_ms: now,
  }).local;

  await state.api.put('spreadsheets', {
    id: documentId,
    title: titleFromFilename(file.name),
    filename: file.name,
    mime_type: isXlsx ? XLSX_MIME : isTsv ? TSV_MIME : CSV_MIME,
    status: 'Imported',
    source_kind: ingestion.kind,
    ingestion_kind: ingestion.kind,
    spreadsheet_type: isXlsx ? 'xlsx' : isTsv ? 'tsv' : 'csv',
    owner_id: '',
    current_version_id: versionId,
    source_sha256: sourceSha,
    row_count: isXlsx ? 0 : modelJson.data.length,
    col_count: isXlsx ? 0 : modelJson.columns.length,
    diagnostics_count: 0,
    linked_records: ingestion.linkedRecords,
    source_receipt_snapshot_hashes: ingestion.sourceReceiptSnapshotHashes,
    knowledge_version: ingestion.knowledgeVersion,
    knowledge_lineage: ingestion.knowledgeLineage,
    tags: normalizeTags(tags),
    display_cache: {},
    index_text: titleFromFilename(file.name) + (isXlsx ? '' : '\n' + modelJson.data.slice(0, 10).map(r => r.join(' ')).join('\n')),
    is_deleted: false,
    created_at_ms: now,
    updated_at_ms: now,
  }).local;

  if (state.selectedId !== selectedId || state.editorHandle !== handle) return;
  if (!await saveSpreadsheetBeforeLeaving(state)) return;
  if (state.selectedId !== selectedId || state.editorHandle !== handle) return;
  state.selectedId = documentId;
  revealSelectedSpreadsheetInList(state);
  refreshSpreadsheets(state);
  await loadSelectedVersion(state);
  renderLeft(state);
  renderRight(state);
  renderCenter(state);
}

async function resolveSpreadsheetIngestion(state, input = {}) {
  const candidates = [input];
  const sourceFileId = String(input.sourceFileId || input.source_file_id || '').trim();
  if (sourceFileId) {
    const sourceFile = readSpreadsheetSourceRecord(state, 'desktop_files', sourceFileId);
    if (!sourceFile) {
      return {
        ...normalizeSpreadsheetIngestion({ candidates }),
        kind: UNRESOLVED_SOURCE_KIND,
        valid: false,
        message: `Spreadsheet source file "${sourceFileId}" could not be resolved.`,
      };
    }
    candidates.push(sourceFile);
    const linkedCollection = String(sourceFile.linked_collection || '').trim();
    const linkedRecordId = String(sourceFile.linked_record_id || '').trim();
    if (linkedCollection && linkedRecordId && linkedCollection !== 'desktop_files') {
      const linkedRecord = readSpreadsheetSourceRecord(state, linkedCollection, linkedRecordId);
      if (linkedRecord) candidates.push(linkedRecord);
    }
  }
  return normalizeSpreadsheetIngestion({ candidates });
}

// Herkunftsdatensatz einer geöffneten Datei (fremde Collection, nur lesen; V4-PORT.md OFFEN-2). Ohne Leserecht null.
function readSpreadsheetSourceRecord(state, collectionName, recordId) {
  try {
    return state.api.get(collectionName, recordId).get() || null;
  } catch {
    return null;
  }
}

function normalizeSpreadsheetIngestion(input = {}) {
  const candidates = Array.isArray(input.candidates)
    ? input.candidates.filter((candidate) => candidate && typeof candidate === 'object')
    : [input];
  const objects = candidates.flatMap((candidate) => nestedLineageObjects(candidate));
  const linkedRecords = uniqueJsonRecords(objects.flatMap((object) => [
    ...arrayValue(object.linked_records),
    ...arrayValue(object.linkedRecords),
    ...lineageRecords(object.source_receipt_links, 'source_receipt'),
    ...lineageRecords(object.source_receipts, 'source_receipt'),
    ...lineageRecords(object.sourceReferences, 'source_receipt'),
    ...lineageRecords(object.source_references, 'source_receipt'),
    ...stringArrayValue(object.source_ids).map((id) => ({ kind: 'source_receipt', id })),
    ...jsonStringArrayValue(object.source_ids_json).map((id) => ({ kind: 'source_receipt', id })),
  ]));
  const sourceReceiptSnapshotHashes = uniqueStrings([
    ...objects.flatMap((object) => stringArrayValue(object.source_receipt_snapshot_hashes)),
    ...objects.flatMap((object) => stringArrayValue(object.snapshot_hashes)),
    ...linkedRecords.flatMap((record) => [
      record.snapshot_hash,
      record.source_receipt_snapshot_hash,
      record.receipt_snapshot_hash,
    ]),
  ]);
  const claimIds = uniqueStrings(objects.flatMap((object) => [
    ...stringArrayValue(object.claim_ids),
    ...stringArrayValue(object.claimIds),
  ]));
  const evidenceIds = uniqueStrings(objects.flatMap((object) => [
    ...stringArrayValue(object.evidence_ids),
    ...stringArrayValue(object.evidenceIds),
  ]));
  const normalizedLinkedRecords = uniqueJsonRecords([
    ...linkedRecords,
    ...claimIds.map((id) => ({ kind: 'claim', id })),
    ...evidenceIds.map((id) => ({ kind: 'evidence', id })),
  ]);
  // The explicit opening purpose survives the second normalization in import.
  // Caller validity/evidence assertions never authorize the evidence path.
  const openPurpose = candidates[0]?.open_purpose;
  const reportSnapshot = openPurpose === 'snapshot_report'
    ? normalizeReportSnapshot(candidates[0]?.report_snapshot) : null;
  const kind = openPurpose === 'snapshot_report'
    || isResearchDerivedSpreadsheet(objects, normalizedLinkedRecords)
    ? RESEARCH_GENERATED_KIND
    : USER_IMPORT_KIND;
  const knowledgeVersion = firstKnowledgeVersion(objects, normalizedLinkedRecords);
  const lineage = {
    kind,
    ingestion_kind: kind,
    open_purpose: openPurpose,
    report_snapshot: reportSnapshot,
    linkedRecords: normalizedLinkedRecords,
    sourceReceiptSnapshotHashes,
    knowledgeVersion,
    knowledgeLineage: {
      source_receipt_snapshot_hashes: sourceReceiptSnapshotHashes,
      knowledge_version: knowledgeVersion,
      ...(openPurpose === 'snapshot_report' ? {
        open_purpose: openPurpose,
        report_snapshot: reportSnapshot,
        evidence_eligible: false,
      } : {}),
    },
  };
  const validation = validateSpreadsheetLineage(lineage);
  return { ...lineage, ...validation };
}

function assertSpreadsheetIngestionAllowed(ingestion) {
  if (ingestion?.valid) return;
  const error = new Error(ingestion?.message || 'Spreadsheet ingestion lineage is incomplete.');
  error.code = 'SPREADSHEET_LINEAGE_REQUIRED';
  throw error;
}

function validateSpreadsheetLineage(ingestion = {}) {
  if (ingestion.kind !== RESEARCH_GENERATED_KIND) return { valid: true, message: '' };
  if (ingestion.open_purpose === 'snapshot_report' && ingestion.report_snapshot) {
    return { valid: true, message: '' };
  }
  return {
    valid: false,
    message: 'This spreadsheet needs confirmed provenance before it can be opened as evidence.',
  };
}

function normalizeReportSnapshot(snapshot) {
  if (!snapshot || typeof snapshot !== 'object' || Array.isArray(snapshot)) return null;
  const { source_module, source_collection, source_record_ids, captured_at_ms, file_sha256 } = snapshot;
  if (typeof source_module !== 'string' || !source_module.trim()
    || typeof source_collection !== 'string' || !source_collection.trim()
    || !Array.isArray(source_record_ids) || !source_record_ids.length
    || source_record_ids.some((id) => typeof id !== 'string' || !id.trim())
    || !Number.isSafeInteger(captured_at_ms) || captured_at_ms <= 0
    || !Number.isFinite(new Date(captured_at_ms).getTime())
    || typeof file_sha256 !== 'string' || !/^[a-f0-9]{64}$/i.test(file_sha256)) return null;
  return {
    source_module: source_module.trim(),
    source_collection: source_collection.trim(),
    source_record_ids: source_record_ids.map((id) => id.trim()),
    captured_at_ms,
    file_sha256: file_sha256.toLowerCase(),
  };
}

function assertSpreadsheetSnapshotBytes(ingestion, sourceSha) {
  if (ingestion.open_purpose !== 'snapshot_report') return;
  if (ingestion.report_snapshot?.file_sha256 === sourceSha) return;
  const error = new Error('The report snapshot does not match the supplied spreadsheet bytes.');
  error.code = 'SPREADSHEET_LINEAGE_REQUIRED';
  throw error;
}

function nestedLineageObjects(value) {
  const objects = [value];
  for (const key of ['lineage', 'knowledge_lineage', 'knowledgeLineage', 'provenance', 'metadata', 'payload', 'source_record', 'sourceRecord']) {
    if (value?.[key] && typeof value[key] === 'object' && !Array.isArray(value[key])) objects.push(value[key]);
  }
  return objects;
}

function lineageRecords(value, defaultKind) {
  if (!Array.isArray(value)) return [];
  return value.map((record) => {
    if (record && typeof record === 'object' && !Array.isArray(record)) {
      return record.kind || record.type || record.record_type ? record : { kind: defaultKind, ...record };
    }
    const id = String(record || '').trim();
    return id ? { kind: defaultKind, id } : null;
  }).filter(Boolean);
}

function isResearchDerivedSpreadsheet(objects, linkedRecords) {
  const values = objects.flatMap((object) => [
    object.source_kind,
    object.sourceKind,
    object.ingestion_kind,
    object.ingestionKind,
    object.source,
    object.source_module,
    object.sourceModule,
  ]).map((value) => String(value || '').trim().toLowerCase());
  return objects.some((object) => object.research_derived === true || object.researchDerived === true || object.is_research_derived === true)
    || values.some((value) => value === RESEARCH_GENERATED_KIND || value === 'research' || value === 'business-os-research' || value.includes('research-derived'))
    || linkedRecords.some((record) => {
      const kind = recordKind(record);
      return kind === 'research_task' || kind === 'knowledge_domain' || isResearchSourceReference(record);
    });
}

function isResearchSourceReference(record = {}) {
  const kind = recordKind(record);
  if (!kind && [record.source_id, record.receipt_id, record.claim_id, record.evidence_id].some((value) => String(value || '').trim())) return true;
  return kind === 'source'
    || kind.includes('source_receipt')
    || kind.includes('source-receipt')
    || kind === 'claim'
    || kind.includes('claim')
    || kind === 'evidence'
    || kind.includes('evidence');
}

function hasResearchReferenceId(record = {}) {
  return [record.id, record.record_id, record.source_id, record.receipt_id, record.claim_id, record.evidence_id]
    .some((value) => String(value || '').trim());
}

function recordKind(record = {}) {
  return String(record.kind || record.type || record.record_type || record.role || '').trim().toLowerCase();
}

function firstKnowledgeVersion(objects, linkedRecords) {
  for (const object of objects) {
    for (const key of ['knowledge_version', 'knowledgeVersion']) {
      if (hasKnowledgeVersion(object[key])) return object[key];
    }
    const versionId = String(object.knowledge_version_id || object.knowledgeVersionId || '').trim();
    if (versionId) return { version_id: versionId };
  }
  const linked = linkedRecords.find((record) => hasKnowledgeVersion(record.knowledge_version || record.knowledgeVersion));
  return linked?.knowledge_version || linked?.knowledgeVersion || null;
}

function hasKnowledgeVersion(value) {
  if (typeof value === 'string') return value.trim().length > 0;
  return Boolean(value && typeof value === 'object' && !Array.isArray(value)
    && Object.values(value).some((item) => String(item || '').trim()));
}

function isSnapshotHash(value) {
  return /^sha256:[0-9a-f]{64}$/i.test(String(value || '').trim());
}

function arrayValue(value) {
  return Array.isArray(value) ? value.filter((item) => item && typeof item === 'object') : [];
}

function stringArrayValue(value) {
  if (Array.isArray(value)) return value.map((item) => String(item || '').trim()).filter(Boolean);
  if (typeof value === 'string' && value.trim()) return [value.trim()];
  return [];
}

function jsonStringArrayValue(value) {
  if (typeof value !== 'string' || !value.trim()) return [];
  try {
    const parsed = JSON.parse(value);
    return Array.isArray(parsed) ? stringArrayValue(parsed) : [];
  } catch {
    return [];
  }
}

function uniqueStrings(values) {
  return [...new Set(values.map((value) => String(value || '').trim()).filter(Boolean))];
}

function uniqueJsonRecords(records) {
  const seen = new Set();
  return records.filter((record) => {
    if (!record || typeof record !== 'object' || Array.isArray(record)) return false;
    const key = JSON.stringify(record);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function parseCSVContent(text, delimiter = ',') {
  // Simple yet robust CSV parses that handles quotes
  const lines = [];
  let row = [""];
  let insideQuote = false;

  for (let i = 0; i < text.length; i++) {
    const char = text[i];
    const nextChar = text[i + 1];

    if (char === '"') {
      if (insideQuote && nextChar === '"') {
        row[row.length - 1] += '"';
        i++; // skip next quote
      } else {
        insideQuote = !insideQuote;
      }
    } else if (char === delimiter && !insideQuote) {
      row.push("");
    } else if ((char === '\r' || char === '\n') && !insideQuote) {
      if (char === '\r' && nextChar === '\n') {
        i++;
      }
      lines.push(row);
      row = [""];
    } else {
      row[row.length - 1] += char;
    }
  }
  if (row.length > 1 || row[0] !== "") {
    lines.push(row);
  }
  return lines;
}

// Version aus dem lokalen Stand (api.get/query sind synchron und vollständig). Fehlt sie, kommt sie live über
// das Versionsabo nach; kein Ersatzkopf, keine Replikationswartezeit (R4).
async function loadSelectedVersion(state, isActive = () => true) {
  if (state.disposed || !isActive()) return null;
  if (spreadsheetDraftProtected(state)) return null;
  const record = selectedRecord(state);
  if (!record) {
    if (!(state.editorHandle?.kind === 'ctox-spreadsheets'
      && (state.dirty || state.saving || state.editorHandle.saving))) {
      state.selectedVersion = null;
    }
    return null;
  }
  const handle = state.editorHandle;
  const selection = state.selectedId;
  const versionId = record.current_version_id;
  const load = { selection, versionId, status: 'loading', error: null };
  state.versionLoad = load;
  ensureVersionSubscription(state, record.id);
  let doc = versionId ? state.api.get('spreadsheet_versions', versionId).get() : null;
  if (!doc) {
    doc = state.api.query('spreadsheet_versions', {
      selector: { spreadsheet_id: record.id },
      sort: [{ updated_at_ms: 'desc' }],
      limit: 1,
    }).get()[0] || null;
  }
  if (!doc) {
    load.status = 'missing';
    if (handle?.recordId !== selection) {
      state.selectedVersion = null;
      state.dirty = false;
      state.saving = false;
    }
    return null;
  }
  // Live-Zeilen sind eingefroren; der Editor arbeitet auf einer eigenen Kopie.
  state.selectedVersion = structuredClone(doc);
  load.status = 'ready';
  state.dirty = false;
  state.saving = false;
  return state.selectedVersion;
}

/* --------------------------------------------------------------------------
   Shell-V2-Rahmen: die Pane-Koepfe sind STATISCHES Markup in index.html
   (Vorbild: modules/knowledge, modules/tickets). Der Renderer klont die
   Vorlage und fuellt nur noch Texte, Beschriftungen, Icons und Zustaende —
   die Rahmen-Geometrie steht damit im Markup, nicht in JS-Zeichenketten.
   -------------------------------------------------------------------------- */
function sheetLabelParts(raw) {
  const value = String(raw || '');
  const separator = value.indexOf('|');
  return separator === -1 ? [value, value] : [value.slice(0, separator), value.slice(separator + 1)];
}

function fillPaneHead(state, root) {
  root.querySelectorAll('[data-sheet-text]').forEach((el) => {
    const [key, fallback] = sheetLabelParts(el.dataset.sheetText);
    el.textContent = state.t(key, fallback);
  });
  root.querySelectorAll('[data-sheet-placeholder]').forEach((el) => {
    const [key, fallback] = sheetLabelParts(el.dataset.sheetPlaceholder);
    el.setAttribute('placeholder', state.t(key, fallback));
  });
  root.querySelectorAll('[data-sheet-label]').forEach((el) => {
    const [key, fallback] = sheetLabelParts(el.dataset.sheetLabel);
    const label = state.t(key, fallback);
    el.setAttribute('aria-label', label);
    if (el.tagName === 'BUTTON') el.setAttribute('title', label);
  });
  root.querySelectorAll('[data-sheet-icon]').forEach((el) => {
    el.insertAdjacentHTML('afterbegin', actionIcon(state, el.dataset.sheetIcon));
  });
  return root;
}

function paneHeadFragment(state, name) {
  const template = state.root.querySelector(`template[data-spreadsheets-head="${name}"]`);
  if (!template) return document.createDocumentFragment();
  return fillPaneHead(state, template.content.cloneNode(true));
}

function renderLeft(state) {
  const wrap = state.root.querySelector('[data-spreadsheets-explorer]');
  if (!wrap) return;
  const visible = visibleSpreadsheets(state);
  const selected = selectedRecord(state);
  if (!wrap.dataset.officeBound) {
    fillPaneHead(state, wrap);
    bindLeftControls(state, wrap);
    wrap.dataset.officeBound = 'true';
  }

  const exportButton = wrap.querySelector('[data-spreadsheets-export]');
  if (exportButton) exportButton.disabled = !selected;
  const search = wrap.querySelector('[data-spreadsheets-search]');
  if (search && document.activeElement !== search) search.value = state.searchQuery;
  const sortSelect = wrap.querySelector('[data-spreadsheets-sort]');
  if (sortSelect) sortSelect.value = state.sortBy;
  const statusSelect = wrap.querySelector('[data-spreadsheets-status]');
  if (statusSelect) statusSelect.value = state.statusFilter;
  const tagSelect = wrap.querySelector('[data-spreadsheets-tag]');
  if (tagSelect) {
    const options = tagFilterOptions(state);
    if (tagSelect.dataset.optionSignature !== options) {
      tagSelect.innerHTML = options;
      tagSelect.dataset.optionSignature = options;
    }
  }
  const list = wrap.querySelector('[data-spreadsheets-list]');
  wrap.dataset.officeView = state.listView || 'list';
  populateSpreadsheetList(state, list, visible);
  autoWirePaneGrammar(state.root);
  wrap.__ctoxPaneGrammar?.refreshDot();
  // Textdiät Regel 5: Zähler nur, wenn ein Filter die Liste kürzt.
  wrap.__ctoxPaneGrammar?.setFooter(hasActiveListFilters(state) ? `${visible.length} ${state.lang === 'en' ? 'filtered' : 'gefiltert'}` : '');
  applySpreadsheetSelection(state);
}

function populateSpreadsheetList(state, list, records = visibleSpreadsheets(state)) {
  const signature = JSON.stringify([records]);
  if (list.dataset.officeSignature === signature) return;
  const scrollTop = list.scrollTop;
  list.dataset.officeSignature = signature;
  list.replaceChildren();
  // Leere oder weggefilterte Liste rendert nichts (R11); Anlegen/Import stehen im Kopf, Filter-Reset im Filterband.
  if (records.length === 0) return;

  for (const record of records) {
    const card = document.createElement('article');
    card.className = 'spreadsheets-card';
    card.dataset.contextModule = 'spreadsheets';
    card.dataset.contextRecordType = 'spreadsheet';
    card.dataset.contextRecordId = record.id;
    card.dataset.contextLabel = record.title || record.filename || record.id;
    card.setAttribute('aria-current', String(record.id === state.selectedId));

    const button = document.createElement('button');
    button.type = 'button';
    button.className = `ctox-list-item spreadsheets-card-main${record.id === state.selectedId ? ' is-selected' : ''}`;
    button.dataset.sheetId = record.id;

    button.innerHTML = `
      <strong>${escapeHtml(record.title)}</strong>
      <small>${escapeHtml(record.spreadsheet_type?.toUpperCase() || 'XLSX')} · ${escapeHtml(new Date(record.updated_at_ms).toLocaleDateString(state.lang === 'en' ? 'en-GB' : 'de-DE'))}</small>
    `;
    button.title = record.filename || record.title;

    const manageBtn = document.createElement('button');
    manageBtn.type = 'button';
    manageBtn.className = 'ctox-pane-icon spreadsheets-card-manage';
    manageBtn.dataset.sheetId = record.id;
    // FIX9b: „Weitere Aktionen“ (⋯) statt Zahnrad ohne erkennbaren Zweck.
    manageBtn.innerHTML = actionIcon(state, 'more');
    manageBtn.title = state.t('sheetActions', 'Weitere Aktionen');
    manageBtn.setAttribute('aria-label', `${record.title || record.filename || ''} – ${state.t('sheetActions', 'Weitere Aktionen')}`.replace(/^ – /, ''));

    card.append(button, manageBtn);
    list.append(card);
  }
  list.scrollTop = Number.isFinite(state.pendingListScrollTop) ? state.pendingListScrollTop : scrollTop;
  state.pendingListScrollTop = null;
}

function applySpreadsheetSelection(state) {
  for (const card of state.root.querySelectorAll('.spreadsheets-card')) {
    const selected = card.dataset.contextRecordId === state.selectedId;
    card.setAttribute('aria-current', String(selected));
    card.querySelector('.spreadsheets-card-main')?.classList.toggle('is-selected', selected);
  }
}

function setSpreadsheetLibraryOpen(state, open) {
  state.libraryOpen = open;
  state.root.querySelector('[data-spreadsheets-module]')?.classList.toggle('is-library-open', open);
  const backdrop = state.root.querySelector('[data-spreadsheets-library-backdrop]');
  if (backdrop) backdrop.hidden = !open;
  state.root.querySelector('[data-spreadsheets-library-toggle]')?.setAttribute('aria-expanded', String(open));
}

function bindLeftControls(state, wrap) {
  wrap.querySelector('[data-spreadsheets-new]').addEventListener('click', () => {
    void requestBlankSpreadsheet(state);
  });

  wrap.querySelector('[data-spreadsheets-import-open]').addEventListener('click', () => {
    openImportModal(state);
  });

  const exportBtn = wrap.querySelector('[data-spreadsheets-export]');
  if (exportBtn) {
    exportBtn.addEventListener('click', () => {
      openExportModal(state);
    });
  }

  wrap.addEventListener('ctox-pane-grammar-change', ({ detail }) => {
    state.searchQuery = detail.search;
    state.sortBy = detail.filters.sort;
    state.statusFilter = detail.filters.status;
    state.tagFilter = detail.filters.tag;
    state.listView = detail.view;
    wrap.querySelector('[data-spreadsheets-list]').scrollTop = 0;
    renderLeft(state);
  });
  state.root.querySelector('[data-spreadsheets-library-backdrop]')?.addEventListener('click', () => setSpreadsheetLibraryOpen(state, false));

  wrap.addEventListener('click', (e) => {
    const mainBtn = e.target.closest('.spreadsheets-card-main');
    if (mainBtn) {
      const sheetId = mainBtn.dataset.sheetId;
      if (sheetId && sheetId !== state.selectedId) {
        const selectedId = state.selectedId;
        const handle = state.editorHandle;
        (async () => {
          if (!await saveSpreadsheetBeforeLeaving(state)) return;
          if (state.selectedId !== selectedId || state.editorHandle !== handle) return;
          state.selectedId = sheetId;
          setSpreadsheetLibraryOpen(state, false);
          renderLeft(state);
          await loadSelectedVersion(state);
          if (state.selectedId !== sheetId || state.editorHandle !== handle) return;
          await renderCenter(state);
          renderRight(state);
        })().catch((error) => {
          reportSpreadsheetError(state, 'spreadsheets.switch', error);
        });
      }
      return;
    }

    const manageBtn = e.target.closest('.spreadsheets-card-manage');
    if (manageBtn) {
      const sheetId = manageBtn.dataset.sheetId;
      openManageDrawer(state, sheetId);
    }
  });
}

function tagFilterOptions(state) {
  const allTags = new Set();
  state.spreadsheets.forEach(doc => (doc.tags || []).forEach(t => allTags.add(t)));
  let html = `<option value="all" ${state.tagFilter === 'all' ? 'selected' : ''}>${escapeHtml(state.t('allTags', 'Alle Tags'))}</option>`;
  html += `<option value="untagged" ${state.tagFilter === 'untagged' ? 'selected' : ''}>${escapeHtml(state.t('untagged', 'Ohne Tags'))}</option>`;
  allTags.forEach(tag => {
    html += `<option value="${escapeHtml(tag)}" ${state.tagFilter === tag ? 'selected' : ''}>${escapeHtml(tag)}</option>`;
  });
  return html;
}

function normalizeSpreadsheetRecord(record = {}) {
  const title = sanitizeTitle(record.title || record.filename || record.id || '');
  const filename = String(record.filename || ensureExtension(slugFilename(title || record.id || 'spreadsheet'), '.csv')).trim();
  const isXlsx = filename.toLowerCase().endsWith('.xlsx');
  return {
    ...record,
    id: String(record.id || '').trim(),
    title: title || stateLessSpreadsheetTitleFallback(record),
    filename: filename || 'spreadsheet.csv',
    mime_type: record.mime_type || (isXlsx ? XLSX_MIME : filename.toLowerCase().endsWith('.tsv') ? TSV_MIME : CSV_MIME),
    status: normalizeSpreadsheetStatus(record.status || 'Draft'),
    spreadsheet_type: record.spreadsheet_type || (isXlsx ? 'xlsx' : filename.toLowerCase().endsWith('.tsv') ? 'tsv' : 'csv'),
    current_version_id: String(record.current_version_id || ''),
    row_count: Number(record.row_count || 0),
    col_count: Number(record.col_count || 0),
    diagnostics_count: Number(record.diagnostics_count || 0),
    index_text: String(record.index_text || ''),
    tags: normalizeTags(record.tags || []),
    updated_at_ms: Number(record.updated_at_ms || record.created_at_ms || 0),
  };
}

function stateLessSpreadsheetTitleFallback(record = {}) {
  return String(record.id || '').trim() || 'Neue Tabelle';
}

function normalizeSpreadsheetStatus(status) {
  const value = String(status || '').trim();
  return ['Draft', 'Imported', 'Review', 'Final'].includes(value) ? value : 'Draft';
}

// Kit badge modifier for a record status (base .ctox-badge stays neutral).
function statusBadgeClass(status) {
  if (status === 'Final') return 'is-success';
  if (status === 'Review') return 'is-warning';
  return '';
}

function isActiveSpreadsheetRecord(record = {}) {
  return Boolean(record.id) && record.is_deleted !== true;
}

function normalizeSpreadsheetModel(model = {}) {
  const data = Array.isArray(model.data) && model.data.length
    ? model.data.map((row) => Array.isArray(row) ? row : [String(row ?? '')])
    : DEFAULT_GRID_DATA;
  const maxColumns = Math.max(...data.map((row) => Array.isArray(row) ? row.length : 0), 1);
  const columns = Array.isArray(model.columns) && model.columns.length
    ? model.columns
    : Array.from({ length: maxColumns }, (_, i) => ({ type: 'text', title: String.fromCharCode(65 + i), width: '120px' }));
  return {
    data,
    columns,
    nestedHeaders: model.nestedHeaders || null,
    mergeCells: model.mergeCells || null,
    style: model.style || null,
  };
}

function hasActiveListFilters(state) {
  return Boolean(
    state.searchQuery.trim()
    || state.statusFilter !== 'all'
    || state.tagFilter !== 'all'
  );
}

function revealSelectedSpreadsheetInList(state) {
  state.searchQuery = '';
  state.statusFilter = 'all';
  state.tagFilter = 'all';
}

function spreadsheetMetaLabel(state, record) {
  const rows = Number(record.row_count || 0);
  const cols = Number(record.col_count || 0);
  const size = rows && cols
    ? state.t('spreadsheetSizeLabel', '{0} Zeilen · {1} Spalten', rows, cols)
    : state.t('spreadsheetSizeUnknown', 'Größe unbekannt');
  const qualityLabel = Number(record.diagnostics_count || 0) > 0
    ? state.t('needsReviewLabel', 'Prüfen')
    : state.t('readyLabel', 'Bereit');
  return `${size} · ${qualityLabel}`;
}

function visibleSpreadsheets(state) {
  let result = [...state.spreadsheets];
  const query = state.searchQuery.trim().toLowerCase();
  if (query) {
    result = result.filter(doc =>
      (doc.title || '').toLowerCase().includes(query) ||
      (doc.filename || '').toLowerCase().includes(query)
    );
  }
  if (state.statusFilter !== 'all') {
    result = result.filter(doc => doc.status === state.statusFilter);
  }
  if (state.tagFilter !== 'all') {
    if (state.tagFilter === 'untagged') {
      result = result.filter(doc => !doc.tags || doc.tags.length === 0);
    } else {
      result = result.filter(doc => (doc.tags || []).includes(state.tagFilter));
    }
  }

  result.sort((a, b) => {
    if (state.sortBy === 'updated_desc') return b.updated_at_ms - a.updated_at_ms;
    if (state.sortBy === 'updated_asc') return a.updated_at_ms - b.updated_at_ms;
    if (state.sortBy === 'title_asc') return (a.title || '').localeCompare(b.title || '');
    if (state.sortBy === 'status') return (a.status || '').localeCompare(b.status || '');
    return 0;
  });

  return result;
}

function currentSpreadsheetVersionLoad(state) {
  const load = state.versionLoad;
  const record = selectedRecord(state);
  return !state.disposed && record && load?.selection === state.selectedId
    && load.versionId === record.current_version_id ? load : null;
}

async function renderCenter(state) {
  if (state.disposed || !state.root.isConnected) return;
  const record = selectedRecord(state);
  const shell = state.root.querySelector('[data-spreadsheets-editor]');
  if (!shell) return;
  const handle = state.editorHandle;
  const load = currentSpreadsheetVersionLoad(state);
  if (record && handle?.kind === 'ctox-spreadsheets' && handle.recordId === record.id
    && (spreadsheetDraftProtected(state)
      || ((!load || load.status === 'ready') && handle.versionId === state.selectedVersion?.id))) {
    return handle;
  }
  if (handle?.kind === 'ctox-spreadsheets') {
    handle.destroy().catch((error) => console.error('[spreadsheets] editor cleanup failed', error));
  }
  state.editorHandle = null;
  state.spreadsheetContainer = null;

  if (!record) {
    const hasFilters = hasActiveListFilters(state);
    // The pane head stays even without a selection: shell V2 hangs the version
    // menu on `.ctox-pane-header .ctox-pane-title` of the module host, so the
    // empty state must not drop the head or the app loses version history and
    // source access. Only the kicker and the record actions fall away.
    const emptyHead = paneHeadFragment(state, 'editor');
    emptyHead.querySelector('[data-spreadsheets-editor-kicker]')?.remove();
    emptyHead.querySelector('[data-spreadsheets-editor-actions]')?.remove();
    // Ohne Auswahl: nur der Kopf, kein Leer-/Hinweistext (R11).
    void hasFilters;
    shell.replaceChildren(emptyHead);
    bindSpreadsheetLibraryToggle(state, shell);
    return;
  }

  // Load editor UI frame

  const head = paneHeadFragment(state, 'editor');
  const kicker = head.querySelector('[data-spreadsheets-editor-kicker]');
  if (kicker) {
    kicker.textContent = record.filename;
    const origin = record.knowledge_lineage?.report_snapshot;
    if (record.knowledge_lineage?.open_purpose === 'snapshot_report' && origin) {
      const label = state.t('snapshotReportOriginUnverified', 'Bericht aus gespeichertem Stand · Herkunft ungeprüft · Kein Nachweis');
      kicker.textContent += ` · ${label}`;
      kicker.setAttribute('title', `${label}: ${origin.source_module} / ${origin.source_collection} / ${origin.source_record_ids.join(', ')} / ${new Date(origin.captured_at_ms).toISOString()}`);
    }
  }
  const titleEl = head.querySelector('[data-spreadsheets-editor-title]');
  if (titleEl) {
    titleEl.textContent = record.title;
    titleEl.setAttribute('title', record.title);
  }
  const badge = head.querySelector('[data-spreadsheets-dirty-indicator]');
  if (badge) {
    // Metadata readiness is not editor readiness: opening the source blob can
    // still fail. Never advertise a saved spreadsheet before editor.open succeeds.
    badge.hidden = true;
    badge.classList.toggle('is-dirty', state.dirty);
    badge.classList.toggle('is-saving', state.saving);
  }

  const editorCanvas = document.createElement('div');
  editorCanvas.className = 'spreadsheets-editor-canvas';
  editorCanvas.setAttribute('data-spreadsheets-canvas', '');
  // Kein Lade-Kasten (R11): der Editor erscheint, sobald er montiert ist.
  shell.replaceChildren(head, editorCanvas);
  bindSpreadsheetLibraryToggle(state, shell);

  shell.querySelector('[data-spreadsheets-add-row]')?.remove();
  shell.querySelector('[data-spreadsheets-add-col]')?.remove();
  const canvas = shell.querySelector('[data-spreadsheets-canvas]');
  // Fehler/fehlende Version → Zustandszone (R11), nie als Kasten im Inhalt. Die Version erscheint live.
  if (!isOfficeSpreadsheetRecord(record)) {
    state.api.status.report({ level: 'warn', code: 'spreadsheets.format.unsupported', text: `${state.t('unsupportedSpreadsheetFormat', 'Nicht unterstütztes Tabellenformat.')} ${state.t('supportedSpreadsheetFormats', 'Bitte XLSX, CSV oder TSV verwenden.')}`, ref: record.id });
    return;
  }
  state.api.status.clear('spreadsheets.format.unsupported');
  if (load?.status === 'missing' || !state.selectedVersion) {
    let backlog = 0;
    try { backlog = Number(state.api.metrics.get()?.backlog || 0); } catch {}
    if (backlog === 0) {
      state.api.status.report({ level: 'warn', code: 'spreadsheets.version.missing', text: `${state.t('noSavedVersionFound', 'Zu dieser Tabelle wurde keine gespeicherte Version gefunden.')} ${record.title || record.id}`, ref: record.id });
    }
    return;
  }
  state.api.status.clear('spreadsheets.version.missing');
  // FIX9b: sofort lesbar – Lesevorschau aus der letzten Fassung, der Editor startet dahinter.
  renderSpreadsheetReadPreview(state, canvas, state.selectedVersion);
  try {
    const mounted = await mountCtoxSpreadsheets(state, canvas, record, state.selectedVersion);
    if (badge && mounted && state.editorHandle === mounted
      && !state.disposed && canvas.isConnected && state.selectedId === record.id) {
      badge.hidden = false;
    }
    state.api.status.clear('spreadsheets.editor');
  } catch (error) {
    if (canvas.isConnected && state.selectedId === record.id) {
      console.error('[spreadsheets] editor open failed', error);
      reportSpreadsheetError(state, 'spreadsheets.editor', error, state.t('editorLoadFailed', 'Editor konnte nicht geladen werden:'));
    }
  }
}

function bindSpreadsheetLibraryToggle(state, shell) {
  const toggle = shell.querySelector('[data-spreadsheets-library-toggle]');
  toggle?.setAttribute('aria-expanded', String(Boolean(state.libraryOpen)));
  toggle?.addEventListener('click', () => setSpreadsheetLibraryOpen(state, !state.libraryOpen));
}

function isOfficeSpreadsheetRecord(record) {
  return record?.spreadsheet_type === 'xlsx'
    || record?.spreadsheet_type === 'csv'
    || record?.spreadsheet_type === 'tsv'
    || record?.mime_type === XLSX_MIME
    || record?.mime_type === CSV_MIME
    || record?.mime_type === TSV_MIME
    || /\.(xlsx|csv|tsv)$/i.test(String(record?.filename || ''));
}

async function mountCtoxSpreadsheets(state, host, record, version) {
  if (state.officeEngine !== 'ctox_spreadsheets') {
    throw new Error(`Unsupported spreadsheet office engine: ${state.officeEngine}`);
  }
  let disposed = false;
  let errorReported = false;
  const listeners = [];
  const mount = document.createElement('div');
  mount.className = 'spreadsheets-ctox-spreadsheets-frame';
  mount.style.cssText = 'width:100%;height:100%;min-height:0';
  const handle = {
    kind: 'ctox-spreadsheets',
    recordId: record.id,
    versionId: version.id,
    activity: 0,
    saving: false,
    editor: null,
    saveTimer: null,
    async destroy() {
      disposed = true;
      clearSaveTimer();
      for (const remove of listeners.splice(0)) {
        try { remove?.(); } catch (error) { console.error('[spreadsheets] listener cleanup failed', error); }
      }
      const editor = handle.editor;
      handle.editor = null;
      if (state.editorHandle === handle) {
        state.editorHandle = null;
        state.spreadsheetContainer = null;
      }
      // Only remove this mount, never a newer editor's host contents.
      mount.remove();
      await editor?.destroy();
    },
    save: (options) => handle.editor.save(options),
    export: async () => (await handle.editor.export({ format: 'xlsx' })).bytes,
    focus: () => handle.editor.focus(),
    inspect: () => handle.editor.inspect(),
  };
  const isActive = () => state.editorHandle === handle && state.selectedId === record.id;
  const isCurrent = () => !disposed && isActive() && host.isConnected;
  function clearSaveTimer() {
    if (handle.saveTimer !== null) window.clearTimeout(handle.saveTimer);
    handle.saveTimer = null;
  }
  function scheduleSave() {
    clearSaveTimer();
    // The saved event alone decides whether an in-flight save needs a successor.
    if (!isCurrent() || handle.saving || !state.dirty) return;
    handle.saveTimer = window.setTimeout(async () => {
      handle.saveTimer = null;
      if (!isCurrent() || handle.saving || !state.dirty) return;
      try {
        await handle.editor.save({ reason: 'autosave' });
      } catch (error) {
        if (!errorReported) onError(error);
      }
    }, 900);
  }
  function onError(error) {
    const detail = error?.error || error;
    const code = String(detail?.code || error?.code || '');
    const message = String(error?.message || detail?.message || error);
    console.error('[spreadsheets] editor save failed', `code=${code}`, `message=${message}`, error);
    if (!isCurrent()) return;
    errorReported = true;
    handle.activity += 1;
    clearSaveTimer();
    handle.saving = false;
    state.saving = false;
    markSpreadsheetAsDirty(state);
    state.api.status.report({ level: 'error', code: 'spreadsheets.editor.save', text: message });
    // No automatic retry after errors (in particular version conflicts).
  }
  state.editorHandle = handle;
  try {
    const { createCtoxSpreadsheetsEditor } = await loadCtoxSpreadsheetsModule(state);
    if (!isCurrent()) { await handle.destroy(); return null; }
    for (const child of [...(host.children || [])]) {
      if (!child.matches?.('[data-spreadsheets-read-preview]')) child.remove();
    }
    const preview = host.querySelector?.('[data-spreadsheets-read-preview]');
    // Hinter der Lesevorschau startet der Editor unsichtbar (mit Layout, damit das iframe initialisiert).
    if (preview && !preview.hidden) mount.classList?.add('is-warming');
    host.append(mount);
    // 4.0 spiegelt keine Collection-Schreibrechte; der Core entscheidet (V4-PORT.md OFFEN-7).
    const canWrite = true;
    const editor = await createCtoxSpreadsheetsEditor({
      host: mount,
      bridge: state.officeBridge,
      locale: state.lang,
      theme: document.documentElement.dataset.theme || 'system',
      permissions: { read: true, write: canWrite, export: true, comment: canWrite, review: false },
    });
    handle.editor = editor;
    if (!isCurrent()) { await handle.destroy(); return null; }
    listeners.push(editor.on('dirty', () => {
      if (!isCurrent()) return;
      handle.activity += 1;
      markSpreadsheetAsDirty(state);
      scheduleSave();
    }));
    listeners.push(editor.on('saving', () => {
      if (!isCurrent()) return;
      errorReported = false;
      clearSaveTimer();
      handle.activity += 1;
      handle.saving = true;
      state.saving = true;
      markSpreadsheetAsDirty(state);
    }));
    listeners.push(editor.on('saved', (payload = {}) => {
      if (!isCurrent() || !payload.versionId
        || (payload.recordId && payload.recordId !== handle.recordId)) return;
      handle.activity += 1;
      handle.versionId = payload.versionId;
      if (state.selectedVersion) {
        state.selectedVersion.id = payload.versionId;
        state.selectedVersion.version_id = payload.versionId;
      }
      record.current_version_id = payload.versionId;
      const current = state.spreadsheets.find((item) => item.id === handle.recordId);
      if (current) current.current_version_id = payload.versionId;
      clearSaveTimer();
      handle.saving = false;
      state.saving = false;
      if (payload.dirty) {
        markSpreadsheetAsDirty(state);
        scheduleSave();
      } else {
        markSpreadsheetAsSaved(state);
      }
    }));
    listeners.push(editor.on('error', onError));
    await editor.open({ recordId: record.id, versionId: version.id });
    if (!isCurrent()) { await handle.destroy(); return null; }
    host.querySelector?.('[data-spreadsheets-read-preview]')?.remove();
    mount.classList?.remove('is-warming');
    state.spreadsheetContainer = mount;
    return handle;
  } catch (error) {
    try { await handle.destroy(); } catch (cleanupError) {
      console.error('[spreadsheets] editor cleanup failed', cleanupError);
    }
    throw error;
  }
}

// Serialize one CSV cell with minimal RFC-4180 quoting: only quote when the
// value contains a delimiter, quote, newline, or leading/trailing whitespace.
// Numeric and plain cells are emitted raw so their type survives a CSV
// round-trip (force-quoting every cell turned every value into a string on
// re-import).
function escapeCsvCell(value) {
  const str = String(value ?? '');
  if (/[",\r\n]/.test(str) || str !== str.trim()) {
    return `"${str.replace(/"/g, '""')}"`;
  }
  return str;
}

// Serialize a 2D array of cells to CSV text.
function rowsToCsv(rows) {
  return (rows || []).map(row => (row || []).map(escapeCsvCell).join(',')).join('\n');
}

function markSpreadsheetAsDirty(state) {
  state.dirty = true;

  // Nur der Punkt zeigt den Zustand (Klassen), kein Zustandstext im Inhalt (R11).
  const badge = state.root.querySelector('[data-spreadsheets-dirty-indicator]');
  if (badge) badge.className = `ctox-badge spreadsheets-dirty-badge is-dirty${state.saving ? ' is-saving' : ''}`;

}

function markSpreadsheetAsSaved(state) {
  state.dirty = false;
  state.saving = false;
  const badge = state.root.querySelector('[data-spreadsheets-dirty-indicator]');
  if (badge) badge.className = 'ctox-badge spreadsheets-dirty-badge';
}

// 4.0 hat nur den Inhaltsbereich (R2): keine rechte Shell-Spalte mehr, nichts zu tun.
function renderRight() {}

function dispatchSpreadsheetRunbook(state, input) {
  const runbook = state.runbooks.find(r => r.id === input.runbookId);
  return sendSpreadsheetsCommand(state, {
    type: runbook?.command_type || input.runbookId || 'spreadsheet.summarize',
    record_id: input.record.id,
    payload: {
      spreadsheet_id: input.record.id,
      version_id: input.versionId || input.record.current_version_id,
      prompt: input.prompt || '',
      runbook_id: runbook?.id || input.runbookId,
      source_action: input.sourceAction || 'spreadsheet_runbook'
    },
    client_context: {
      surface: 'business-os-spreadsheets',
      filename: input.record.filename,
      document_type: 'spreadsheet',
      action: input.sourceAction || 'spreadsheet_runbook'
    }
  });
}

// api.command: nie warten (R5); failed/lokal ungültig → spreadsheets.command.<type>, done → clear; Abo bis unmount.
function sendSpreadsheetsCommand(state, { type, module = 'spreadsheets', record_id: recordId = '', payload = {}, client_context: clientContext = {} }) {
  const api = state.api;
  const code = `spreadsheets.command.${type}`;
  let handle;
  try {
    handle = api.command(type, { ...payload, record_id: recordId, module, client_context: clientContext }, { record_id: recordId });
  } catch (error) {
    reportSpreadsheetError(state, code, error, `${type}:`);
    return null;
  }
  let off = null;
  let finished = false;
  const stop = () => { finished = true; off?.(); off = null; };
  off = handle.status.subscribe((value) => {
    if (value?.status === 'failed') {
      api.status.report({ level: 'error', code, text: `${type}: ${value.error || 'failed'}`, ref: handle.id });
      stop();
    } else if (value?.status === 'done') {
      api.status.clear(code);
      stop();
    }
  });
  if (finished) off?.();
  handle.local.catch((error) => {
    api.status.report({ level: 'error', code, text: `${type}: ${error?.code || 'local'} ${error?.message || error}`, ref: handle.id });
  });
  state.disposers.push(stop);
  return handle;
}

function readNewSpreadsheetInput(form) {
  const formData = new FormData(form);
  return {
    title: formData.get('title')?.toString() || '',
    tags: formData.get('tags')?.toString() || '',
  };
}

function validateNewSpreadsheetInput(input = {}) {
  const title = sanitizeTitle(input.title || '');
  if (!title) return { valid: false, key: 'validationTitleRequired', message: 'Titel fehlt.' };
  return { valid: true, message: '' };
}

// Rundgang RUND2: keine Fehlerzeile im Formular (Owner-Regel: Zustand nur in der Zustandszone). Ein leeres
// Pflichtfeld sperrt nur den Absendeknopf; eine falsche Datei meldet die Zone (spreadsheets.import.file).
function updateNewSpreadsheetSubmitState(state, form) {
  if (!form) return false;
  const validation = validateNewSpreadsheetInput(readNewSpreadsheetInput(form));
  setFormValidationState(form, validation.valid, '');
  return validation.valid;
}

function readImportInput(form) {
  const fileInput = form?.querySelector('[data-import-file]');
  return {
    file: fileInput?.files?.[0] || null,
  };
}

function validateImportInput(input = {}) {
  const file = input.file;
  if (!isFileLike(file)) {
    return { valid: false, key: 'validationFileRequired', message: 'Datei erforderlich.' };
  }
  if (!isSupportedSpreadsheetFile(file)) {
    return { valid: false, key: 'validationUnsupportedFile', message: 'Nur XLSX, CSV oder TSV.' };
  }
  return { valid: true, message: '' };
}

function updateImportSubmitState(state, form) {
  if (!form) return false;
  const validation = validateImportInput(readImportInput(form));
  setFormValidationState(form, validation.valid, '');
  if (validation.key === 'validationUnsupportedFile') {
    state.api?.status?.report({ level: 'warn', code: 'spreadsheets.import.file', text: state.t(validation.key, validation.message) });
  } else {
    state.api?.status?.clear('spreadsheets.import.file');
  }
  return validation.valid;
}

function setFormValidationState(form, isValid, message = '') {
  const submit = form.querySelector('button[type="submit"]');
  const status = form.querySelector('[data-spreadsheets-form-status]');
  if (submit) {
    submit.disabled = !isValid;
    submit.setAttribute('aria-disabled', String(!isValid));
  }
  if (status) {
    status.textContent = isValid ? '' : message;
    status.hidden = isValid || !message;
  }
}

function isFileLike(file) {
  return Boolean(file && typeof file.name === 'string' && file.name.trim());
}

function isSupportedSpreadsheetFile(file) {
  if (!isFileLike(file)) return false;
  const name = file.name.toLowerCase();
  return SUPPORTED_IMPORT_EXTENSIONS.some((ext) => name.endsWith(ext))
    || file.type === CSV_MIME
    || file.type === TSV_MIME
    || file.type === XLSX_MIME;
}

function focusFirstDrawerControl(wrapper) {
  const target = wrapper.querySelector('input:not([disabled]), select:not([disabled]), textarea:not([disabled]), button:not([disabled])');
  if (target instanceof HTMLElement) {
    requestAnimationFrame(() => target.focus({ preventScroll: true }));
  }
}

// Drawer im App-Fenster (OFFEN-D: 4.0 hat keine Drawer-API; nichts außerhalb von root).
function openDrawer(state, body) {
  closeDrawers(state);
  const drawer = document.createElement('aside');
  drawer.className = 'spreadsheets-app-drawer';
  drawer.dataset.spreadsheetsAppDrawer = 'left';
  drawer.append(body);
  (state.root.querySelector('[data-spreadsheets-module]') || state.root).append(drawer);
}

function closeDrawers(state) {
  state.root?.querySelectorAll('[data-spreadsheets-app-drawer]').forEach((node) => node.remove());
  state.api?.status?.clear('spreadsheets.import.file');
}

function openNewSpreadsheetDrawer(state) {
  const wrapper = document.createElement('div');
  wrapper.className = 'spreadsheets-drawer-form';
  wrapper.innerHTML = `
    <h3>${escapeHtml(state.t('newDocumentTitle', 'Neue Tabelle'))}</h3>
    <p class="spreadsheets-drawer-copy">${escapeHtml(state.t('newSpreadsheetDescription', 'Erstellt einen gespeicherten Tabellenentwurf mit Beispielstruktur.'))}</p>
    <form data-spreadsheets-new-form novalidate>
      <label>
        <span class="ctox-field-label">${escapeHtml(state.t('title', 'Titel'))}</span>
        <input class="ctox-input" name="title" type="text" value="${escapeHtml(`${state.t('newDocumentTitle', 'Neue Tabelle')} - ${new Date().toISOString().slice(0, 10)}`)}" required data-new-title>
      </label>
      <label>
        <span class="ctox-field-label">${escapeHtml(state.t('tags', 'Tags (kommagetrennt)'))}</span>
        <input class="ctox-input" name="tags" type="text" placeholder="Budget, Forecast" data-new-tags>
      </label>
      <p class="spreadsheets-form-status" role="status" data-spreadsheets-form-status></p>
      <div class="spreadsheets-drawer-actions">
        <button type="button" class="ctox-button" data-drawer-cancel>${escapeHtml(state.t('cancel', 'Abbrechen'))}</button>
        <button type="submit" class="ctox-button is-primary">${escapeHtml(state.t('createDraft', 'Entwurf erstellen'))}</button>
      </div>
    </form>
  `;

  wrapper.querySelector('[data-drawer-cancel]').addEventListener('click', () => {
    closeDrawers(state);
  });

  const form = wrapper.querySelector('[data-spreadsheets-new-form]');
  updateNewSpreadsheetSubmitState(state, form);
  form.addEventListener('input', () => updateNewSpreadsheetSubmitState(state, form));
  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    if (!updateNewSpreadsheetSubmitState(state, form)) return;
    const submit = form.querySelector('button[type="submit"]');
    const input = readNewSpreadsheetInput(form);
    try {
      if (submit) {
        submit.disabled = true;
        submit.setAttribute('aria-disabled', 'true');
      }
      closeDrawers(state);
      await createNewSpreadsheet(state, input);
      state.api.status.clear('spreadsheets.create');
    } catch (err) {
      console.error(err);
      reportSpreadsheetError(state, 'spreadsheets.create', err, 'Fehler beim Erstellen:');
    }
  });

  openDrawer(state, wrapper);
  focusFirstDrawerControl(wrapper);
}

function openImportModal(state) {
  const wrapper = document.createElement('div');
  wrapper.className = 'spreadsheets-drawer-form';
  wrapper.innerHTML = `
    <form data-spreadsheets-import-form novalidate>
      <label>
        <span class="ctox-field-label">${escapeHtml(state.t('file', 'Datei auswählen (XLSX, CSV oder TSV)'))}</span>
        <input class="ctox-input" type="file" accept=".xlsx,.csv,.tsv" required data-import-file>
      </label>
      <label>
        <span class="ctox-field-label">${escapeHtml(state.t('tags', 'Tags (kommagetrennt)'))}</span>
        <input class="ctox-input" type="text" placeholder="Sales, Q2, Forecast" data-import-tags>
      </label>
      <p class="spreadsheets-form-status" role="status" data-spreadsheets-form-status></p>
      <div class="spreadsheets-drawer-actions">
        <button type="button" class="ctox-button" data-drawer-cancel>${escapeHtml(state.t('cancel', 'Abbrechen'))}</button>
        <button type="submit" class="ctox-button is-primary" disabled aria-disabled="true">${escapeHtml(state.t('import', 'Importieren'))}</button>
      </div>
    </form>
  `;

  wrapper.querySelector('[data-drawer-cancel]').addEventListener('click', () => {
    closeDrawers(state);
  });

  const form = wrapper.querySelector('form');
  updateImportSubmitState(state, form);
  form.addEventListener('change', () => updateImportSubmitState(state, form));
  form.addEventListener('input', () => updateImportSubmitState(state, form));
  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    if (!updateImportSubmitState(state, form)) return;
    const fileInput = wrapper.querySelector('[data-import-file]');
    const file = fileInput.files[0];
    if (!file) return;

    const tagsInput = wrapper.querySelector('[data-import-tags]');
    const tags = tagsInput.value.split(',').map(t => t.trim()).filter(Boolean);

    closeDrawers(state);
    try {
      await importSpreadsheetFile(state, file, tags);
      state.api.status.clear('spreadsheets.import');
    } catch (err) {
      console.error(err);
      reportSpreadsheetError(state, 'spreadsheets.import', err, 'Fehler beim Importieren:');
    }
  });

  openDrawer(state, wrapper);
  focusFirstDrawerControl(wrapper);
}

function openExportModal(state) {
  const record = selectedRecord(state);
  if (!record) return;

  const wrapper = document.createElement('div');
  wrapper.className = 'spreadsheets-drawer-form';
  wrapper.innerHTML = `
    <h3>Tabelle Exportieren</h3>
    <form data-spreadsheets-export-form>
      <label>
        <span class="ctox-field-label">${escapeHtml(state.t('documentType', 'Exportformat'))}</span>
        <select class="ctox-select" data-export-format>
          <option value="xlsx">XLSX (Office Open XML)</option>
        </select>
      </label>
      <div class="spreadsheets-drawer-actions">
        <button type="button" class="ctox-button" data-drawer-cancel>${escapeHtml(state.t('cancel', 'Abbrechen'))}</button>
        <button type="submit" class="ctox-button is-primary">${escapeHtml(state.t('export', 'Exportieren'))}</button>
      </div>
    </form>
  `;

  wrapper.querySelector('[data-drawer-cancel]').addEventListener('click', () => {
    closeDrawers(state);
  });

  wrapper.querySelector('form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const format = wrapper.querySelector('[data-export-format]').value;
    closeDrawers(state);

    try {
      if (!state.editorHandle) throw new Error('Editor nicht initialisiert.');
      if (format !== 'xlsx' || state.editorHandle.kind !== 'ctox-spreadsheets') {
        throw new Error('CTOX Spreadsheets-Editor ist nicht geöffnet.');
      }
      const bytes = await state.editorHandle.export();
      const downloadName = ensureExtension(slugFilename(record.title || 'export'), '.xlsx');
      downloadBlob(bytes, XLSX_MIME, downloadName, state.root);
      state.api.status.clear('spreadsheets.export');
    } catch (err) {
      console.error(err);
      reportSpreadsheetError(state, 'spreadsheets.export', err, 'Fehler beim Exportieren:');
    }
  });

  openDrawer(state, wrapper);
}

function downloadBlob(content, mime, downloadName, host) {
  const blob = new Blob([content], { type: mime });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = downloadName;
  host.appendChild(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

async function openManageDrawer(state, id) {
  const data = state.api.get('spreadsheets', id).get();
  if (!data) return;

  const wrapper = document.createElement('div');
  wrapper.className = 'spreadsheets-drawer-form';
  wrapper.innerHTML = `
    <h3>${escapeHtml(state.t('manageDocumentTitle', 'Tabelle verwalten'))}</h3>
    <form>
      <label>
        <span class="ctox-field-label">${escapeHtml(state.t('title', 'Titel'))}</span>
        <input class="ctox-input" type="text" data-field="title" value="${escapeHtml(data.title)}" required>
      </label>
      <label>
        <span class="ctox-field-label">${escapeHtml(state.t('status', 'Status'))}</span>
        <select class="ctox-select" data-field="status">
          <option value="Draft" ${data.status === 'Draft' ? 'selected' : ''}>Draft</option>
          <option value="Imported" ${data.status === 'Imported' ? 'selected' : ''}>Imported</option>
          <option value="Review" ${data.status === 'Review' ? 'selected' : ''}>Review</option>
          <option value="Final" ${data.status === 'Final' ? 'selected' : ''}>Final</option>
        </select>
      </label>
      <label>
        <span class="ctox-field-label">${escapeHtml(state.t('tags', 'Tags (kommagetrennt)'))}</span>
        <input class="ctox-input" type="text" data-field="tags" value="${escapeHtml((data.tags || []).join(', '))}">
      </label>
      <div class="spreadsheets-drawer-actions">
        <button type="button" class="ctox-button is-danger" data-action="delete">${escapeHtml(state.t('delete', 'Tabelle löschen'))}</button>
        <button type="submit" class="ctox-button is-primary">${escapeHtml(state.t('save', 'Speichern'))}</button>
      </div>
    </form>
  `;

  wrapper.querySelector('form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const titleVal = wrapper.querySelector('[data-field="title"]').value.trim();
    const statusVal = wrapper.querySelector('[data-field="status"]').value;
    const tagsVal = wrapper.querySelector('[data-field="tags"]').value.split(',').map(t => t.trim()).filter(Boolean);

    try {
      await state.api.patch('spreadsheets', id, {
        title: titleVal,
        status: statusVal,
        tags: tagsVal,
        updated_at_ms: Date.now()
      }).local;
      closeDrawers(state);
      state.api.status.clear('spreadsheets.manage.save');
      refreshSpreadsheets(state);
      renderLeft(state);
      if (state.selectedId === id) {
        renderCenter(state);
      }
    } catch (err) {
      console.error(err);
      reportSpreadsheetError(state, 'spreadsheets.manage.save', err, 'Fehler beim Speichern:');
    }
  });

  wrapper.querySelector('[data-action="delete"]').addEventListener('click', async () => {
    // Bestätigung im App-Fenster (R7); destruktiv → default:false.
    const confirmed = await state.api.shell.dialog({
      kind: 'confirm',
      title: state.t('deleteConfirmTitle', 'Tabelle löschen'),
      text: state.t('deleteConfirmMessage', `Tabelle "${data.title}" wirklich unwiderruflich löschen?`, data.title),
      confirm_label: state.t('delete', 'Tabelle löschen'),
      default: false,
    });
    if (!confirmed) return;

    try {
      await state.api.patch('spreadsheets', id, { is_deleted: true, updated_at_ms: Date.now() }).local;
      closeDrawers(state);
      state.api.status.clear('spreadsheets.manage.delete');

      if (state.selectedId === id) {
        state.selectedId = '';
        state.selectedVersion = null;
        if (state.editorHandle?.kind === 'ctox-spreadsheets') await state.editorHandle.destroy();
        state.spreadsheetContainer = null;
        state.editorHandle = null;
      }

      refreshSpreadsheets(state);
      renderLeft(state);
      renderCenter(state);
    } catch (err) {
      console.error(err);
      reportSpreadsheetError(state, 'spreadsheets.manage.delete', err, 'Fehler beim Löschen:');
    }
  });

  openDrawer(state, wrapper);
}

function initSpreadsheetsContextMenu(state) {
  state.contextMenu?.remove();
  const moduleHost = state.root;
  if (!moduleHost) return () => {};
  const menu = document.createElement('div');
  menu.className = 'ctox-context-menu spreadsheets-context-menu';
  menu.hidden = true;
  moduleHost.append(menu);
  state.contextMenu = menu;

  const handleContextMenu = (event) => {
    const context = spreadsheetCommandContextFromElement(state, event.target);
    event.preventDefault();
    event.stopPropagation();
    renderSpreadsheetsContextMenu(state, context, event.clientX, event.clientY);
  };
  const handleOutsideClick = (event) => {
    if (state.contextMenu?.contains(event.target)) return;
    hideSpreadsheetsContextMenu(state);
  };
  const handleEscape = (event) => {
    if (event.key === 'Escape') hideSpreadsheetsContextMenu(state);
  };

  window.addEventListener('click', handleOutsideClick, { capture: true });
  window.addEventListener('keydown', handleEscape);

  return () => {
    window.removeEventListener('click', handleOutsideClick, { capture: true });
    window.removeEventListener('keydown', handleEscape);
    hideSpreadsheetsContextMenu(state);
  };
}

function hideSpreadsheetsContextMenu(state) {
  if (state.contextMenu) state.contextMenu.hidden = true;
}

function canModifySpreadsheetsApp(state) {
  // Nur Spiegel (R9); die Entscheidung trifft der Core.
  let permissions = [];
  try { permissions = state.api.actor.permissions.get() || []; } catch {}
  if (permissions.includes('apps.source.edit')) return true;
  const user = { role: state.api.actor.role };
  const role = String(user.role || (user.is_admin ? 'admin' : 'user')).trim().toLowerCase().replace(/^business_os_/, '');
  return ['admin', 'chef'].includes(role);
}

function spreadsheetCommandContextFromElement(state, target) {
  const element = target?.nodeType === Node.ELEMENT_NODE ? target : target?.parentElement;
  const record = selectedRecord(state);
  const column = state.root.querySelector('[data-spreadsheets-explorer]')?.contains?.(element) ? 'spreadsheets' : 'editor';

  return {
    module: 'spreadsheets',
    column,
    record_type: record ? 'spreadsheet' : 'module',
    record_id: record?.id || '',
    label: record?.title || record?.filename || '',
    filename: record?.filename || '',
    selected_text: String(window.getSelection?.()?.toString?.() || '').trim().slice(0, 1000),
    clicked_text: String(element?.innerText || element?.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 500),
  };
}

function renderSpreadsheetsContextMenu(state, context, x, y) {
  const canModifyApp = canModifySpreadsheetsApp(state);
  state.contextMenu.innerHTML = `
    <form class="spreadsheets-context-chat" data-spreadsheets-context-chat-form>
      <header>
        <div>
          <strong>${escapeHtml(state.t('chatToCtox', 'Chat to CTOX'))}</strong>
          <span>${escapeHtml(context.label || 'Spreadsheets')}</span>
        </div>
        <button type="button" data-spreadsheets-context-close aria-label="${escapeHtml(state.t('close', 'Schließen'))}">×</button>
      </header>
      <div class="ctox-context-mode" role="radiogroup" aria-label="${escapeHtml(state.t('chatActionLabel', 'CTOX Aufgabe'))}">
        ${contextChatModeMarkup({ canModifyApp, escape: escapeHtml, labels: { data: state.t('chatWorkDataLabel', 'Mit Daten arbeiten'), ask: state.t('chatAnswerLabel', 'Frage beantworten'), app: state.t('chatModifyAppLabel', 'App modifizieren') } })}
      </div>
      <textarea data-spreadsheets-context-message placeholder="${escapeHtml(state.t('chatPlaceholder', 'Was soll CTOX hier tun oder prüfen?'))}"></textarea>
      <footer>
        <span data-spreadsheets-context-status></span>
        <button type="submit">${escapeHtml(state.t('send', 'Senden'))}</button>
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

  const form = state.contextMenu.querySelector('[data-spreadsheets-context-chat-form]');
  const textarea = state.contextMenu.querySelector('[data-spreadsheets-context-message]');
  state.contextMenu.querySelector('[data-spreadsheets-context-close]')?.addEventListener('click', () => hideSpreadsheetsContextMenu(state));
  form?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const mode = new FormData(form).get('contextMode') || 'data';
    await dispatchSpreadsheetsContextChat(state, context, textarea?.value || '', mode);
  });
  requestAnimationFrame(() => textarea?.focus());
}

async function dispatchSpreadsheetsContextChat(state, context, message, mode = 'data') {
  const trimmed = String(message || '').trim();
  const status = state.contextMenu?.querySelector('[data-spreadsheets-context-status]');
  if (!trimmed) {
    if (status) status.textContent = state.t('chatMissingMessage', 'Nachricht fehlt.');
    return;
  }

  const canModifyApp = canModifySpreadsheetsApp(state);
  const safeMode = normalizeContextChatMode(mode, { canModifyApp });
  const record = selectedRecord(state);
  const titlePrefix = safeMode === 'app'
    ? 'Spreadsheets App modifizieren'
    : safeMode === 'ask'
      ? state.t('chatAnswerLabel', 'Frage beantworten')
      : 'Spreadsheet bearbeiten';
  const title = `${titlePrefix} · ${context.label || 'Spreadsheets'}`;
  // Kontext-Auftrag an die Chat-App ctox über den Baustein (Strang SKL-D, Kern §16.40 commands_out): Anweisungstext,
  // Skill und Rückweg liegen bei den Handlern ctox.chat.task / ctox.app.modify.request; der Sprung öffnet den Chat.
  sendContextChat(state.api, {
    source_app: 'spreadsheets',
    record_ref: record?.id || context.record_id || '',
    selection: context.selected_text || '',
    text: trimmed,
    mode: safeMode,
    canModifyApp,
    title,
    thread_key: 'business-os/spreadsheets',
    context,
  }, { send: (type, payload, opts) => sendSpreadsheetsCommand(state, { type, record_id: opts.record_id, payload, client_context: { action: 'context-chat', mode: safeMode } }) });
  hideSpreadsheetsContextMenu(state);
}

// Seed-Runbooks lokal anlegen (api.put; Core bestätigt über die Outbox).
function ensureSeedRunbooks(state) {
  const existingIds = new Set(state.api.query('spreadsheet_runbooks', {}).get().map((doc) => doc.id));
  const now = Date.now();
  for (const runbook of SYSTEMATIC_SPREADSHEET_RUNBOOKS) {
    if (existingIds.has(runbook.id)) continue;
    state.api.put('spreadsheet_runbooks', {
      ...runbook,
      created_at_ms: now,
      updated_at_ms: now,
    }).local.catch((error) => reportSpreadsheetError(state, 'spreadsheets.runbooks.seed', error));
  }
}

async function sha256Hex(bytes) {
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

// Datei-Bytes über api.files (id = blob_id, ref = sha256); keine Chunk-Collection.
function saveBlobChunks(state, input) {
  return putFile(state.api, input.blobId, input.bytes, input.mimeType, input.name);
}

function sanitizeTitle(val) {
  return String(val || '').trim().replace(/[\r\n\t]+/g, ' ');
}

function slugFilename(val) {
  return String(val || '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '') || 'untitled';
}

function ensureExtension(filename, ext) {
  return filename.endsWith(ext) ? filename : filename + ext;
}

function titleFromFilename(filename) {
  const withoutPath = filename.split(/[\\/]/).pop() || '';
  const lastDot = withoutPath.lastIndexOf('.');
  const base = lastDot >= 0 ? withoutPath.slice(0, lastDot) : withoutPath;
  return base.replace(/[-_]+/g, ' ').trim();
}

function normalizeTags(val) {
  if (Array.isArray(val)) return val.map(t => String(t).trim()).filter(Boolean);
  if (typeof val === 'string') return val.split(',').map(t => t.trim()).filter(Boolean);
  return [];
}

function escapeHtml(val) {
  return String(val ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}


export const __spreadsheetsTestHooks = {
  spreadsheetReadPreviewHtml,
  wireLocalRealtime,
  refreshSpreadsheetsFromLocal,
  loadSelectedVersion,
  spreadsheetsSnapshot,
  applyRestoredView,
  currentSpreadsheetVersionLoad,
  renderCenter,
  openSpreadsheetFile,
  hasActiveListFilters,
  isActiveSpreadsheetRecord,
  normalizeSpreadsheetRecord,
  normalizeSpreadsheetModel,
  isOfficeSpreadsheetRecord,
  spreadsheetBySourceSha,
  normalizeSpreadsheetIngestion,
  validateSpreadsheetLineage,
  assertSpreadsheetIngestionAllowed,
  resolveSpreadsheetIngestion,
  validateImportInput,
  validateNewSpreadsheetInput,
  visibleSpreadsheets,
  saveBlobChunks,
  escapeCsvCell,
  rowsToCsv,
};

// Module-only glyphs with no shared/icons.js equivalent, drawn in the same
// stroke style as actionIconPaths (fill: none, currentColor, 1.8 stroke).
const SPREADSHEETS_LOCAL_ICON_PATHS = Object.freeze({
  addRow: 'M4 6h16M4 12h10M4 18h6M16 15v6M13 18h6',
  addColumn: 'M6 4v16M12 4v10M18 4v6M15 16h6M18 13v6',
});

// Standard action glyphs (shared/icons.js actionIconPaths). 4.0 hat keine Icon-API (OFFEN-D): die App zeichnet
// ihre Glyphen selbst.
const SPREADSHEETS_FALLBACK_ACTION_ICON_PATHS = Object.freeze({
  add: 'M12 5v14M5 12h14',
  filter: 'M4 6h16M7 12h10M10 18h4',
  upload: 'M12 15V4M12 4 8 8M12 4l4 4M5 19h14',
  export: 'M12 3v11M12 3 8 7M12 3l4 4M5 12v7h14v-7',
  settings: 'M12 8.5a3.5 3.5 0 1 1 0 7 3.5 3.5 0 0 1 0-7ZM12 3v2.2M12 18.8V21M21 12h-2.2M5.2 12H3M18.4 5.6l-1.6 1.6M7.2 16.8l-1.6 1.6M18.4 18.4l-1.6-1.6M7.2 7.2 5.6 5.6',
  play: 'M8 5.5v13l10-6.5-10-6.5Z',
  folder: 'M4 6h5l2 2h9v11H4V6Z',
  more: 'M6 12h.01M12 12h.01M18 12h.01',
});

function strokeIconSvg(name, path, size = 16, strokeWidth = 1.8) {
  return `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="${strokeWidth}" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" class="ctox-action-icon ctox-action-${name}"><path d="${path}"></path></svg>`;
}

function actionIcon(state, name, size = 16, strokeWidth = 1.8) {
  if (SPREADSHEETS_LOCAL_ICON_PATHS[name]) {
    return strokeIconSvg(name, SPREADSHEETS_LOCAL_ICON_PATHS[name], size, strokeWidth);
  }
  return strokeIconSvg(name, SPREADSHEETS_FALLBACK_ACTION_ICON_PATHS[name] || SPREADSHEETS_FALLBACK_ACTION_ICON_PATHS.more, size, strokeWidth);
}
