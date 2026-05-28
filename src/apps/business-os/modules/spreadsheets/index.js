import { showBusinessConfirm } from '../../shared/dialogs.js';
import { loadModuleMessages } from '../../shared/i18n.js';
import { HyperFormula } from '../../vendor/hyperformula.mjs';

const CSV_MIME = 'text/csv';
const JSON_MIME = 'application/json';
const CHUNK_SIZE = 256000;
const SPREADSHEET_RENDER_DEBOUNCE_MS = 80;
const SUPPORTED_IMPORT_EXTENSIONS = ['.csv', '.json'];

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

const SYSTEMATIC_SPREADSHEET_RUNBOOKS = [
  {
    id: 'spreadsheet.summarize',
    document_type: 'spreadsheet',
    title: 'Tabelle zusammenfassen',
    description: 'Fasse das ausgewählte Spreadsheet strukturiert zusammen, analysiere Gesamtsummen und identifiziere Trends.',
    command_type: 'spreadsheet.summarize',
    prompt_template: 'Fasse das ausgewählte Spreadsheet strukturiert zusammen. Analysiere Gesamtsummen, identifiziere Trends, beschreibe Ausreißer und erstelle eine managementtaugliche Zusammenfassung.'
  },
  {
    id: 'spreadsheet.audit-formulas',
    document_type: 'spreadsheet',
    title: 'Formeln auditieren',
    description: 'Finde sämtliche Formeln in dieser Tabelle und prüfe sie auf Fehler, Zirkelbezüge oder logische Inkonsistenzen.',
    command_type: 'spreadsheet.audit-formulas',
    prompt_template: 'Scanne diese Tabelle nach allen Formeln. Analysiere sie auf syntaktische Fehler, logische Inkonsistenzen, unvollständige Summenbereiche, fehlende oder fehlerhafte Referenzen und liefere Korrekturempfehlungen.'
  },
  {
    id: 'spreadsheet.risk-review',
    document_type: 'spreadsheet',
    title: 'Finanzielle Risikoanalyse',
    description: 'Identifiziere finanzielle Risiken, unplausible Kennzahlen, starke Margenabweichungen und auffällige Transaktionen.',
    command_type: 'spreadsheet.risk-review',
    prompt_template: 'Führe ein finanzielles Review dieser Daten aus. Suche nach auffälligen Margensprüngen, ungewöhnlichen Datenmustern, Budgetüberschreitungen und potenziellen betriebswirtschaftlichen Risiken. Gib konkrete Handlungsempfehlungen.'
  }
];

function applyStaticLabels(host, t) {
  const loadingText = host.querySelector('.module-loading-copy span');
  if (loadingText) {
    loadingText.textContent = t('workspaceLoading', 'Spreadsheets Workspace wird geladen.');
  }
}

export async function mount(ctx) {
  await ensureStyles();
  const messages = await loadModuleMessages(import.meta.url, ctx.locale || 'de', {});
  const t = (key, fallback, ...args) => {
    let val = key.split('.').reduce((acc, curr) => acc?.[curr], messages) ?? fallback ?? key;
    if (args.length) {
      args.forEach((arg, i) => {
        val = String(val).replace(`{${i}}`, arg);
      });
    }
    return val;
  };

  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  ctx.host.innerHTML = html;
  applyStaticLabels(ctx.host, t);

  const state = {
    ctx,
    jspreadsheetModule: null,
    jSuitesModule: null,
    jspreadsheetLoadPromise: null,
    spreadsheets: [],
    runbooks: [],
    selectedId: '',
    selectedVersion: null,
    editorHandle: null,
    spreadsheetContainer: null,
    autosaveTimer: null,
    autosavePromise: null,
    renderSerial: 0,
    switchSerial: 0,
    dirty: false,
    saving: false,
    searchQuery: '',
    statusFilter: 'all',
    tagFilter: 'all',
    sortBy: 'updated_desc',
    localSubscriptionCleanup: null,
    contextMenu: null,
    contextMenuCleanup: null,
    t,
    lang: ctx.locale === 'en' ? 'en' : 'de',
  };

  // Wire event handlers and load libs
  wireModule(state);
  state.contextMenuCleanup = initSpreadsheetsContextMenu(state);
  state.localSubscriptionCleanup = wireLocalRealtime(state);

  try {
    await ensureSeedRunbooks(ctx);
  } catch (error) {
    console.warn('[spreadsheets] ensureSeedRunbooks failed', error);
  }
  try {
    await refreshRunbooks(state);
  } catch (error) {
    console.warn('[spreadsheets] refreshRunbooks failed', error);
  }
  try {
    await refreshSpreadsheets(state);
  } catch (error) {
    console.warn('[spreadsheets] refreshSpreadsheets failed', error);
  }
  if (state.selectedId) {
    await loadSelectedVersion(state).catch((error) => {
      console.warn('[spreadsheets] initial selected version load failed', error);
    });
  }

  renderLeft(state);
  renderRight(state);
  renderCenter(state);

  return () => {
    if (state.autosaveTimer) clearTimeout(state.autosaveTimer);
    state.contextMenuCleanup?.();
    state.contextMenu?.remove();
    state.contextMenu = null;
    state.localSubscriptionCleanup?.();
    flushActiveSpreadsheetDraft(state).catch((error) => console.error('[spreadsheets] final draft save failed', error));
    if (state.spreadsheetContainer && state.jspreadsheetModule) {
      try { state.jspreadsheetModule.destroy(state.spreadsheetContainer); } catch {}
      state.spreadsheetContainer = null;
    }
    state.editorHandle = null;
  };
}

async function loadJSpreadsheetLib(state) {
  if (state.jspreadsheetModule) return state.jspreadsheetModule;
  if (!state.jspreadsheetLoadPromise) {
    state.jspreadsheetLoadPromise = import('../../vendor/jspreadsheet.mjs').then((mod) => {
      state.jspreadsheetModule = mod.jspreadsheet;
      state.jSuitesModule = mod.jSuites;
      return mod.jspreadsheet;
    });
  }
  return state.jspreadsheetLoadPromise;
}

function wireModule(state) {
  state.ctx.host.addEventListener('spreadsheets:refresh-left', () => renderLeft(state));
}

function wireLocalRealtime(state) {
  const collections = ['spreadsheets', 'spreadsheet_versions', 'spreadsheet_runbooks', 'spreadsheet_blob_chunks'];
  let timer = null;
  const schedule = () => {
    if (timer) return;
    timer = window.setTimeout(() => {
      timer = null;
      refreshSpreadsheetsFromLocal(state).catch((error) => {
        console.warn('[spreadsheets] local realtime render failed', error);
      });
    }, SPREADSHEET_RENDER_DEBOUNCE_MS);
  };
  const subscriptions = collections
    .map((collectionName) => state.ctx.db?.raw?.[collectionName]?.$?.subscribe?.(schedule) || null)
    .filter(Boolean);
  return () => {
    if (timer) window.clearTimeout(timer);
    timer = null;
    for (const sub of subscriptions) {
      try { sub.unsubscribe?.(); } catch {}
    }
  };
}

async function refreshSpreadsheetsFromLocal(state) {
  const previousSelectedVersionId = state.selectedVersion?.id || '';
  try {
    await Promise.all([
      refreshRunbooks(state).catch((err) => console.warn('[spreadsheets] background refreshRunbooks failed', err)),
      refreshSpreadsheets(state).catch((err) => console.warn('[spreadsheets] background refreshSpreadsheets failed', err)),
    ]);
  } catch (error) {
    console.warn('[spreadsheets] background refresh from local failed', error);
  }
  if (state.selectedId && previousSelectedVersionId !== selectedRecord(state)?.current_version_id) {
    await loadSelectedVersion(state).catch(() => null);
  }
  renderLeft(state);
  renderRight(state);
}

async function refreshSpreadsheets(state) {
  const collection = state.ctx.db?.raw?.spreadsheets;
  const rawSpreadsheets = collection
    ? await collection.find({ sort: [{ updated_at_ms: 'desc' }] }).exec()
    : [];
  state.spreadsheets = rawSpreadsheets
    .map((doc) => normalizeSpreadsheetRecord(typeof doc.toJSON === 'function' ? doc.toJSON() : doc))
    .filter(isActiveSpreadsheetRecord);

  if (state.selectedId && !state.spreadsheets.some((record) => record.id === state.selectedId)) {
    state.selectedId = state.spreadsheets[0]?.id || '';
    state.selectedVersion = null;
  }
  if (!state.selectedId && state.spreadsheets[0]) state.selectedId = state.spreadsheets[0].id;
}

async function refreshRunbooks(state) {
  const collection = state.ctx.db?.raw?.spreadsheet_runbooks;
  const storedRunbooks = collection
    ? (await collection.find({ sort: [{ title: 'asc' }] }).exec()).map((doc) => doc.toJSON())
    : [];
  state.runbooks = mergeSpreadsheetRunbooks(storedRunbooks);
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
  requireSpreadsheetPersistence(state.ctx);
  const title = sanitizeTitle(input.title || `${state.t('newDocumentTitle', 'Neue Tabelle')} - ${new Date().toISOString().slice(0, 10)}`);
  if (!title) throw new Error(state.t('validationTitleRequired', 'Titel fehlt.'));
  const filename = ensureExtension(slugFilename(title), '.csv');
  const documentId = `sheet_${crypto.randomUUID()}`;
  const versionId = `${documentId}_v1`;
  const blobId = `${versionId}_blob`;
  const now = Date.now();

  const modelJson = {
    data: input.data || DEFAULT_GRID_DATA,
    columns: input.columns || DEFAULT_GRID_COLUMNS,
    nestedHeaders: input.nestedHeaders || null,
    mergeCells: input.mergeCells || null,
    style: input.style || null
  };

  // Convert to CSV string representation for raw blob persist
  const csvText = modelJson.data.map(row => row.map(val => `"${String(val).replace(/"/g, '""')}"`).join(',')).join('\n');
  const bytes = new TextEncoder().encode(csvText);

  await saveBlobChunks(state.ctx, {
    blobId,
    spreadsheetId: documentId,
    versionId,
    mimeType: CSV_MIME,
    bytes
  });

  await state.ctx.db.raw.spreadsheet_versions.insert({
    id: versionId,
    spreadsheet_id: documentId,
    version: 1,
    source_kind: 'created_blank',
    blob_id: blobId,
    model_json: modelJson,
    diagnostics: [],
    created_at_ms: now,
    updated_at_ms: now,
  });

  await state.ctx.db.raw.spreadsheets.insert({
    id: documentId,
    title,
    filename,
    mime_type: CSV_MIME,
    status: 'Draft',
    spreadsheet_type: 'jspreadsheet',
    owner_id: '',
    current_version_id: versionId,
    source_sha256: await sha256Hex(bytes),
    row_count: modelJson.data.length,
    col_count: modelJson.data[0]?.length || 0,
    diagnostics_count: 0,
    linked_records: [],
    tags: normalizeTags(input.tags),
    display_cache: {},
    index_text: title,
    is_deleted: false,
    created_at_ms: now,
    updated_at_ms: now,
  });

  state.selectedId = documentId;
  revealSelectedSpreadsheetInList(state);
  await refreshSpreadsheets(state);
  await loadSelectedVersion(state);
  renderLeft(state);
  renderRight(state);
  renderCenter(state);
}

async function importSpreadsheetFile(state, file, tags = []) {
  requireSpreadsheetPersistence(state.ctx);
  const validation = validateImportInput({ file });
  if (!validation.valid) {
    throw new Error(state.t(validation.key, validation.message));
  }
  const isJson = file.name.endsWith('.json') || file.type === JSON_MIME;
  const bytes = new Uint8Array(await file.arrayBuffer());
  const fileText = new TextDecoder().decode(bytes);

  const documentId = `sheet_${crypto.randomUUID()}`;
  const versionId = `${documentId}_v1`;
  const blobId = `${versionId}_blob`;
  const now = Date.now();

  let modelJson = { data: DEFAULT_GRID_DATA, columns: DEFAULT_GRID_COLUMNS };

  if (isJson) {
    try {
      const parsed = JSON.parse(fileText);
      if (Array.isArray(parsed)) {
        modelJson.data = parsed;
        // Make standard columns based on elements count
        const colCount = parsed[0]?.length || 6;
        modelJson.columns = Array.from({ length: colCount }, (_, i) => ({ type: 'text', title: String.fromCharCode(65 + i), width: '120px' }));
      } else if (parsed && typeof parsed === 'object') {
        modelJson = {
          data: parsed.data || DEFAULT_GRID_DATA,
          columns: parsed.columns || DEFAULT_GRID_COLUMNS,
          nestedHeaders: parsed.nestedHeaders || null,
          mergeCells: parsed.mergeCells || null,
          style: parsed.style || null
        };
      }
    } catch (err) {
      console.warn('Failed parsing JSON spreadsheet. Reverting to empty grid.', err);
      throw new Error(state.t('validationInvalidJson', 'JSON konnte nicht gelesen werden.'));
    }
  } else {
    // Parse CSV
    try {
      const rows = parseCSVContent(fileText);
      if (rows.length > 0) {
        modelJson.data = rows;
        const colCount = Math.max(...rows.map(r => r.length), 1);
        modelJson.columns = Array.from({ length: colCount }, (_, i) => ({ type: 'text', title: String.fromCharCode(65 + i), width: '120px' }));
      } else {
        throw new Error(state.t('validationEmptySpreadsheet', 'Die Datei enthält keine Tabellenzeilen.'));
      }
    } catch (err) {
      console.warn('Failed parsing CSV spreadsheet.', err);
      throw err;
    }
  }
  if (!Array.isArray(modelJson.data) || modelJson.data.length === 0) {
    throw new Error(state.t('validationEmptySpreadsheet', 'Die Datei enthält keine Tabellenzeilen.'));
  }
  modelJson = normalizeSpreadsheetModel(modelJson);

  await saveBlobChunks(state.ctx, {
    blobId,
    spreadsheetId: documentId,
    versionId,
    mimeType: isJson ? JSON_MIME : CSV_MIME,
    bytes
  });

  await state.ctx.db.raw.spreadsheet_versions.insert({
    id: versionId,
    spreadsheet_id: documentId,
    version: 1,
    source_kind: isJson ? 'imported_json' : 'imported_csv',
    blob_id: blobId,
    model_json: modelJson,
    diagnostics: [],
    created_at_ms: now,
    updated_at_ms: now,
  });

  await state.ctx.db.raw.spreadsheets.insert({
    id: documentId,
    title: titleFromFilename(file.name),
    filename: file.name,
    mime_type: isJson ? JSON_MIME : CSV_MIME,
    status: 'Imported',
    spreadsheet_type: 'jspreadsheet',
    owner_id: '',
    current_version_id: versionId,
    source_sha256: await sha256Hex(bytes),
    row_count: modelJson.data.length,
    col_count: modelJson.columns.length,
    diagnostics_count: 0,
    linked_records: [],
    tags: normalizeTags(tags),
    display_cache: {},
    index_text: titleFromFilename(file.name) + '\n' + modelJson.data.slice(0, 10).map(r => r.join(' ')).join('\n'),
    is_deleted: false,
    created_at_ms: now,
    updated_at_ms: now,
  });

  state.selectedId = documentId;
  revealSelectedSpreadsheetInList(state);
  await refreshSpreadsheets(state);
  await loadSelectedVersion(state);
  renderLeft(state);
  renderRight(state);
  renderCenter(state);
}

function parseCSVContent(text) {
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
    } else if (char === ',' && !insideQuote) {
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

async function loadSelectedVersion(state) {
  const record = selectedRecord(state);
  if (!record) {
    state.selectedVersion = null;
    return null;
  }
  try {
    let doc = record.current_version_id
      ? await withTimeout(
        state.ctx.db.raw.spreadsheet_versions.findOne(record.current_version_id).exec(),
        4500,
        `Version ${record.current_version_id} konnte nicht geladen werden.`,
      )
      : null;
    if (!doc) {
      const fallback = await withTimeout(
        state.ctx.db.raw.spreadsheet_versions.find({
          selector: { spreadsheet_id: record.id },
          sort: [{ updated_at_ms: 'desc' }],
          limit: 1,
        }).exec(),
        4500,
        `Keine Versionen für ${record.id} gefunden.`,
      );
      doc = fallback[0] || null;
      if (doc) {
        const versionJson = doc.toJSON();
        const recordDoc = await state.ctx.db.raw.spreadsheets.findOne(record.id).exec();
        await recordDoc?.incrementalPatch({ current_version_id: versionJson.id });
        record.current_version_id = versionJson.id;
      }
    }
    state.selectedVersion = doc?.toJSON() || null;
  } catch (err) {
    console.warn('[spreadsheets] loadSelectedVersion failed gracefully', err);
    state.selectedVersion = null;
  }
  state.dirty = false;
  state.saving = false;
  return state.selectedVersion;
}

function renderLeft(state) {
  const wrap = document.createElement('div');
  wrap.className = 'spreadsheets-explorer';
  const visible = visibleSpreadsheets(state);
  const selected = selectedRecord(state);

  wrap.innerHTML = `
    <header class="ctox-pane-header">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">Dateien</span>
          <h2 class="ctox-pane-title spreadsheets-column-title">${escapeHtml(state.t('spreadsheetsTitle', 'Spreadsheets'))}</h2>
        </div>
        <div class="ctox-pane-actions spreadsheets-column-actions">
          <button class="ctox-pane-icon spreadsheets-column-icon" type="button" aria-label="${escapeHtml(state.t('createWordDocument', 'Neue Tabelle erstellen'))}" title="${escapeHtml(state.t('createWordDocument', 'Neue Tabelle erstellen'))}" data-spreadsheets-new>${iconSvg('new')}</button>
          <button class="ctox-pane-icon spreadsheets-column-icon" type="button" aria-label="${escapeHtml(state.t('importDocument', 'Tabelle importieren'))}" title="${escapeHtml(state.t('importDocument', 'Tabelle importieren'))}" data-spreadsheets-import-open>${iconSvg('import')}</button>
          <button class="ctox-pane-icon spreadsheets-column-icon" type="button" aria-label="${escapeHtml(state.t('exportSelected', 'Ausgewählte Tabelle exportieren'))}" title="${escapeHtml(state.t('exportSelected', 'Ausgewählte Tabelle exportieren'))}" data-spreadsheets-export ${selected ? '' : 'disabled'}>${iconSvg('export')}</button>
        </div>
      </div>
      <div class="ctox-pane-tools spreadsheets-filter-bar">
        <input class="ctox-pane-search" type="search" placeholder="${escapeHtml(state.t('searchPlaceholder', 'Tabelle suchen...'))}" aria-label="${escapeHtml(state.t('searchLabel', 'Tabellen suchen'))}" data-spreadsheets-search value="${escapeHtml(state.searchQuery)}">
        <select class="ctox-pane-filter spreadsheets-filter-control" aria-label="${escapeHtml(state.t('sortLabel', 'Tabellen sortieren'))}" data-spreadsheets-sort>
          <option value="updated_desc" ${state.sortBy === 'updated_desc' ? 'selected' : ''}>${escapeHtml(state.t('sortByNewest', 'Neueste zuerst'))}</option>
          <option value="updated_asc" ${state.sortBy === 'updated_asc' ? 'selected' : ''}>${escapeHtml(state.t('sortByOldest', 'Älteste zuerst'))}</option>
          <option value="title_asc" ${state.sortBy === 'title_asc' ? 'selected' : ''}>${escapeHtml(state.t('sortByTitle', 'Titel A-Z'))}</option>
          <option value="status" ${state.sortBy === 'status' ? 'selected' : ''}>${escapeHtml(state.t('sortByStatus', 'Status'))}</option>
        </select>
        <select class="ctox-pane-sort spreadsheets-filter-control" aria-label="${escapeHtml(state.t('statusFilterLabel', 'Tabellenstatus filtern'))}" data-spreadsheets-status>
          <option value="all" ${state.statusFilter === 'all' ? 'selected' : ''}>${escapeHtml(state.t('filterAll', 'Alle'))}</option>
          <option value="Imported" ${state.statusFilter === 'Imported' ? 'selected' : ''}>Imported</option>
          <option value="Draft" ${state.statusFilter === 'Draft' ? 'selected' : ''}>Draft</option>
          <option value="Review" ${state.statusFilter === 'Review' ? 'selected' : ''}>Review</option>
          <option value="Final" ${state.statusFilter === 'Final' ? 'selected' : ''}>Final</option>
        </select>
        <select class="ctox-pane-sort spreadsheets-filter-control" aria-label="${escapeHtml(state.t('tagFilterLabel', 'Tabellen-Tags filtern'))}" data-spreadsheets-tag>
          ${tagFilterOptions(state)}
        </select>
      </div>
    </header>
  `;

  const list = document.createElement('div');
  list.className = 'spreadsheets-list';
  list.dataset.spreadsheetsList = 'true';
  populateSpreadsheetList(state, list, visible);
  wrap.append(list);
  bindLeftControls(state, wrap);
  state.ctx.left.replaceChildren(wrap);
}

function populateSpreadsheetList(state, list, records = visibleSpreadsheets(state)) {
  list.replaceChildren();
  if (records.length === 0) {
    const hasRecords = state.spreadsheets.length > 0;
    const empty = document.createElement('div');
    empty.className = 'spreadsheets-empty';
    empty.style.padding = '30px 10px';
    empty.innerHTML = `
      <strong>${escapeHtml(hasRecords ? state.t('noMatches', 'Keine Treffer') : state.t('noDocuments', 'Keine Tabellen'))}</strong>
      <span>${escapeHtml(hasRecords ? state.t('adjustSearchFilter', 'Suche oder Filter anpassen.') : state.t('importPrompt', 'Über das Import-Icon CSV oder JSON hinzufügen.'))}</span>
    `;
    list.append(empty);
    return;
  }

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
    button.className = 'spreadsheets-card-main';
    button.dataset.sheetId = record.id;

    const tagsHtml = (record.tags || []).map(t => `<span class="badge badge-tag" style="background: var(--surface-2, #eef3f7); color: var(--accent, #2b6f73); font-size: 10px; margin-inline-end: 4px; padding: 2px 4px; border-radius: 4px;">${escapeHtml(t)}</span>`).join('');

    button.innerHTML = `
      <strong>${escapeHtml(record.title)}</strong>
      <span class="spreadsheets-card-filename">${escapeHtml(record.filename)}</span>
      <div style="margin-top: 4px; display: flex; flex-wrap: wrap; gap: 4px;">
        <span class="badge" style="background: var(--accent, #2b6f73); color: #fff; font-size: 10px; padding: 2px 4px; border-radius: 4px;">${escapeHtml(record.status)}</span>
        ${tagsHtml}
      </div>
      <small class="spreadsheets-card-diagnostics">${escapeHtml(spreadsheetMetaLabel(state, record))}</small>
      <small style="margin-top: 6px; font-size: 10px; color: var(--muted, #687684); display: block;">Updated: ${new Date(record.updated_at_ms).toLocaleString()}</small>
    `;

    const manageBtn = document.createElement('button');
    manageBtn.type = 'button';
    manageBtn.className = 'spreadsheets-card-manage';
    manageBtn.dataset.sheetId = record.id;
    manageBtn.innerHTML = iconSvg('gear');
    manageBtn.title = escapeHtml(state.t('manageDocument', 'Tabelle verwalten'));

    card.append(button, manageBtn);
    list.append(card);
  }
}

function bindLeftControls(state, wrap) {
  wrap.querySelector('[data-spreadsheets-new]').addEventListener('click', () => {
    openNewSpreadsheetDrawer(state);
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

  const searchInput = wrap.querySelector('[data-spreadsheets-search]');
  searchInput.addEventListener('input', (e) => {
    state.searchQuery = e.target.value;
    renderLeft(state);
  });

  wrap.querySelector('[data-spreadsheets-sort]').addEventListener('change', (e) => {
    state.sortBy = e.target.value;
    renderLeft(state);
  });

  wrap.querySelector('[data-spreadsheets-status]').addEventListener('change', (e) => {
    state.statusFilter = e.target.value;
    renderLeft(state);
  });

  wrap.querySelector('[data-spreadsheets-tag]').addEventListener('change', (e) => {
    state.tagFilter = e.target.value;
    renderLeft(state);
  });

  wrap.addEventListener('click', (e) => {
    const mainBtn = e.target.closest('.spreadsheets-card-main');
    if (mainBtn) {
      const sheetId = mainBtn.dataset.sheetId;
      if (sheetId && sheetId !== state.selectedId) {
        state.selectedId = sheetId;
        renderLeft(state);
        loadSelectedVersion(state).then(() => {
          renderCenter(state);
          renderRight(state);
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
  return {
    ...record,
    id: String(record.id || '').trim(),
    title: title || stateLessSpreadsheetTitleFallback(record),
    filename: filename || 'spreadsheet.csv',
    mime_type: record.mime_type || (filename.toLowerCase().endsWith('.json') ? JSON_MIME : CSV_MIME),
    status: normalizeSpreadsheetStatus(record.status || 'Draft'),
    spreadsheet_type: record.spreadsheet_type || 'jspreadsheet',
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

async function renderCenter(state) {
  const record = selectedRecord(state);
  const shell = state.ctx.host.querySelector('[data-spreadsheets-editor]');
  if (!shell) return;

  if (!record) {
    const hasFilters = hasActiveListFilters(state);
    shell.innerHTML = `
      <div class="spreadsheets-empty">
        <strong>${escapeHtml(hasFilters ? state.t('noMatches', 'Keine Treffer') : state.t('noDocumentSelected', 'Keine Tabelle ausgewählt.'))}</strong>
        <span>${escapeHtml(hasFilters ? state.t('adjustSearchFilter', 'Suche oder Filter anpassen.') : state.t('noDocumentSelectedPrompt', 'Links eine Tabelle importieren oder auswählen.'))}</span>
      </div>
    `;
    return;
  }

  // Load editor UI frame
  const isDirtyClass = state.dirty ? 'is-dirty' : '';
  const saveLabel = state.saving ? state.t('saving', 'Speichert...') : (state.dirty ? state.t('unsavedChanges', 'Ungespeicherte Änderungen') : state.t('saved', 'Gespeichert'));

  shell.innerHTML = `
    <header class="spreadsheets-editor-header">
      <div class="spreadsheets-editor-title-area">
        <div class="spreadsheets-editor-title" title="${escapeHtml(record.title)}">${escapeHtml(record.title)}</div>
        <div class="spreadsheets-dirty-badge ${isDirtyClass} ${state.saving ? 'is-saving' : ''}" data-spreadsheets-dirty-indicator>
          <i class="indicator-dot"></i>
          <span>${escapeHtml(saveLabel)}</span>
        </div>
      </div>
      <div class="spreadsheets-editor-actions">
        <button type="button" data-spreadsheets-add-row>${iconSvg('plus')} Zeile</button>
        <button type="button" data-spreadsheets-add-col>${iconSvg('plus')} Spalte</button>
      </div>
    </header>
    <div class="spreadsheets-editor-canvas" data-spreadsheets-canvas>
      <div class="spreadsheets-loading">
        <strong>${escapeHtml(state.t('loadingEditor', 'Editor wird geladen...'))}</strong>
      </div>
    </div>
  `;

  // Bind center actions
  shell.querySelector('[data-spreadsheets-add-row]').addEventListener('click', () => {
    state.editorHandle?.insertRow();
  });
  shell.querySelector('[data-spreadsheets-add-col]').addEventListener('click', () => {
    state.editorHandle?.insertColumn();
  });

  const canvas = shell.querySelector('[data-spreadsheets-canvas]');

  try {
    const jspread = await loadJSpreadsheetLib(state);

    // Destroy previous grid if any
    if (state.spreadsheetContainer) {
      try { jspread.destroy(state.spreadsheetContainer); } catch {}
      state.spreadsheetContainer = null;
    }
    state.editorHandle = null;

    canvas.innerHTML = '';
    const container = document.createElement('div');
    canvas.appendChild(container);
    state.spreadsheetContainer = container;

    if (!state.selectedVersion?.model_json) {
      canvas.innerHTML = `
        <div class="spreadsheets-error">
          <strong>${escapeHtml(state.t('noSavedVersionFound', 'Zu dieser Tabelle wurde keine gespeicherte Version gefunden.'))}</strong>
          <span>${escapeHtml(state.t('loadVersionRepairPrompt', 'Bitte erneut importieren oder den Datensatz verwalten.'))}</span>
        </div>
      `;
      return;
    }

    const versionData = normalizeSpreadsheetModel(state.selectedVersion.model_json);

    const worksheet = await new Promise((resolve, reject) => {
      const gridConfig = {
        worksheets: [{
          data: versionData.data,
          columns: versionData.columns,
          nestedHeaders: versionData.nestedHeaders || null,
          mergeCells: versionData.mergeCells || null,
          style: versionData.style || null,
          minDimensions: [6, 10],
        }],
        tableOverflow: true,
        tableHeight: '100%',
        tableWidth: '100%',
        parseFormulas: false, // Use our custom HyperFormula ESM engine
        onload: (spreadsheet) => {
          if (spreadsheet.worksheets && spreadsheet.worksheets[0]) {
            resolve(spreadsheet.worksheets[0]);
          } else {
            reject(new Error("No worksheet created"));
          }
        },
        onchange: () => {
          markSpreadsheetAsDirty(state);
          recalculateSpreadsheet(state);
        },
        oninsertrow: () => {
          markSpreadsheetAsDirty(state);
          recalculateSpreadsheet(state);
        },
        oninsertcolumn: () => {
          markSpreadsheetAsDirty(state);
          recalculateSpreadsheet(state);
        },
        ondeleterow: () => {
          markSpreadsheetAsDirty(state);
          recalculateSpreadsheet(state);
        },
        ondeletecolumn: () => {
          markSpreadsheetAsDirty(state);
          recalculateSpreadsheet(state);
        },
        onmerge: () => {
          markSpreadsheetAsDirty(state);
          recalculateSpreadsheet(state);
        },
        onchangestyle: () => {
          markSpreadsheetAsDirty(state);
        }
      };

      try {
        jspread(container, gridConfig);
      } catch (err) {
        reject(err);
      }
    });

    state.editorHandle = worksheet;
    recalculateSpreadsheet(state);
  } catch (err) {
    canvas.innerHTML = `
      <div class="spreadsheets-error">
        <strong>${escapeHtml(state.t('editorLoadFailed', 'Editor konnte nicht geladen werden:'))}</strong>
        <span>${escapeHtml(err.message)}</span>
      </div>
    `;
  }
}

function recalculateSpreadsheet(state) {
  if (!state.editorHandle) return;

  // Guard against recursion during visual DOM cell updates
  if (state.isRecalculating) return;
  state.isRecalculating = true;

  try {
    const rawData = state.editorHandle.getData();
    const columns = state.editorHandle.options?.columns || [];
    // Rebuild HyperFormula using the raw data containing formulas
    const hf = HyperFormula.buildFromArray(rawData);

    for (let r = 0; r < rawData.length; r++) {
      for (let c = 0; c < rawData[r].length; c++) {
        const val = rawData[r][c];
        if (typeof val === 'string' && val.startsWith('=')) {
          const calcVal = hf.getCellValue({ sheet: 0, col: c, row: r });
          const cellElement = state.editorHandle.getCell(c, r);
          if (cellElement) {
            const colDef = columns[c] || {};
            const displayVal = formatCellForDisplay(calcVal, colDef);
            cellElement.textContent = displayVal;
            if (String(calcVal).startsWith('#')) {
              cellElement.classList.add('formula-error');
            } else {
              cellElement.classList.remove('formula-error');
            }
          }
        } else {
          const cellElement = state.editorHandle.getCell(c, r);
          if (cellElement) {
            cellElement.classList.remove('formula-error');
          }
        }
      }
    }
  } catch (err) {
    console.error("HyperFormula recalculation failed:", err);
  } finally {
    state.isRecalculating = false;
  }
}

// Apply the column's numeric mask (e.g. "$ #.##0,00") to a calculated value so
// formula results in numeric columns render with the same formatting as the
// typed numeric cells above them. Mirrors jSuites' mask conventions: '.' is the
// thousands separator and ',' is the decimal separator.
function formatCellForDisplay(value, colDef) {
  if (value === null || value === undefined || value === '') return '';
  const type = colDef?.type;
  const mask = colDef?.mask;
  if (type !== 'numeric' || !mask) {
    return String(value);
  }
  const num = Number(value);
  if (!Number.isFinite(num)) return String(value);

  // Detect prefix/suffix around the numeric template.
  const numericTemplateMatch = mask.match(/[0#.,]+/);
  if (!numericTemplateMatch) return String(num);
  const template = numericTemplateMatch[0];
  const prefix = mask.slice(0, numericTemplateMatch.index);
  const suffix = mask.slice(numericTemplateMatch.index + template.length);

  // Determine decimal precision from the template's fractional part.
  const decimalIdx = template.lastIndexOf(',');
  const decimals = decimalIdx >= 0 ? template.length - decimalIdx - 1 : 0;

  const fixed = num.toFixed(decimals);
  const [intPartRaw, fracPart = ''] = fixed.split('.');
  const sign = intPartRaw.startsWith('-') ? '-' : '';
  const intDigits = sign ? intPartRaw.slice(1) : intPartRaw;
  const intWithGroups = intDigits.replace(/\B(?=(\d{3})+(?!\d))/g, '.');
  const body = decimals > 0 ? `${intWithGroups},${fracPart}` : intWithGroups;

  return `${prefix}${sign}${body}${suffix}`;
}

function markSpreadsheetAsDirty(state) {
  if (state.dirty) return;
  state.dirty = true;

  const badge = state.ctx.host.querySelector('[data-spreadsheets-dirty-indicator]');
  if (badge) {
    badge.className = 'spreadsheets-dirty-badge is-dirty';
    badge.querySelector('span').textContent = state.t('unsavedChanges', 'Ungespeicherte Änderungen');
  }

  if (state.autosaveTimer) clearTimeout(state.autosaveTimer);
  state.autosaveTimer = setTimeout(() => {
    saveActiveSpreadsheetDraft(state).catch(err => console.error('Auto-save failed:', err));
  }, 900);
}

async function saveActiveSpreadsheetDraft(state) {
  if (!state.dirty || !state.editorHandle || !state.selectedId) return;

  state.saving = true;
  const badge = state.ctx.host.querySelector('[data-spreadsheets-dirty-indicator]');
  if (badge) {
    badge.className = 'spreadsheets-dirty-badge is-saving';
    badge.querySelector('span').textContent = state.t('saving', 'Speichert...');
  }

  try {
    const rawData = state.editorHandle.getData();
    // Retrieve complete styles, merges, and columns layout safely
    const mergeData = state.editorHandle.getMerge?.() || null;
    const styleData = state.editorHandle.getStyle?.() || null;

    // Build columns configurations
    const columnsData = (state.editorHandle.options.columns || []).map((col, idx) => ({
      type: col.type || 'text',
      title: col.title || String.fromCharCode(65 + idx),
      width: col.width || '120px',
      mask: col.mask || null,
      decimal: col.decimal || null
    }));

    const modelJson = {
      data: rawData,
      columns: columnsData,
      nestedHeaders: state.editorHandle.options.nestedHeaders || null,
      mergeCells: mergeData,
      style: styleData
    };

    const docVersionId = state.selectedVersion?.id || `${state.selectedId}_v1`;
    const docBlobId = state.selectedVersion?.blob_id || `${docVersionId}_blob`;
    const now = Date.now();

    // Serialize to CSV text representing the raw blob
    const csvText = modelJson.data.map(row => row.map(val => `"${String(val).replace(/"/g, '""')}"`).join(',')).join('\n');
    const bytes = new TextEncoder().encode(csvText);

    // Delete previous blob chunks first to avoid stacking duplicate indices
    const chunkCollection = state.ctx.db.raw.spreadsheet_blob_chunks;
    const oldChunks = await chunkCollection.find({ selector: { blob_id: docBlobId } }).exec();
    await Promise.all(oldChunks.map(chunk => chunk.remove()));

    // Persist new chunks
    await saveBlobChunks(state.ctx, {
      blobId: docBlobId,
      spreadsheetId: state.selectedId,
      versionId: docVersionId,
      mimeType: CSV_MIME,
      bytes
    });

    // Update version model JSON
    const versionDoc = await state.ctx.db.raw.spreadsheet_versions.findOne(docVersionId).exec();
    if (versionDoc) {
      await versionDoc.incrementalPatch({
        model_json: modelJson,
        updated_at_ms: now
      });
    }

    // Update parent metadata
    const sheetDoc = await state.ctx.db.raw.spreadsheets.findOne(state.selectedId).exec();
    if (sheetDoc) {
      await sheetDoc.incrementalPatch({
        row_count: modelJson.data.length,
        col_count: modelJson.columns.length,
        source_sha256: await sha256Hex(bytes),
        index_text: sheetDoc.toJSON().title + '\n' + modelJson.data.slice(0, 10).map(r => r.join(' ')).join('\n'),
        updated_at_ms: now
      });
    }

    state.dirty = false;
    state.saving = false;

    if (badge) {
      badge.className = 'spreadsheets-dirty-badge';
      badge.querySelector('span').textContent = state.t('saved', 'Gespeichert');
    }

    // Trigger explorer refresh
    await refreshSpreadsheets(state);
    state.ctx.host.dispatchEvent(new CustomEvent('spreadsheets:refresh-left'));
  } catch (err) {
    state.saving = false;
    if (badge) {
      badge.className = 'spreadsheets-dirty-badge is-dirty';
      badge.querySelector('span').textContent = state.t('saveFailed', 'Fehler beim Speichern');
    }
    throw err;
  }
}

async function flushActiveSpreadsheetDraft(state) {
  if (state.dirty) {
    await saveActiveSpreadsheetDraft(state);
  }
}

function renderRight(state) {
  const wrap = document.createElement('div');
  wrap.className = 'spreadsheets-runbooks';
  const record = selectedRecord(state);

  let listHtml = '';
  for (const runbook of state.runbooks) {
    listHtml += `
      <div class="spreadsheets-runbook-card" data-runbook-id="${escapeHtml(runbook.id)}">
        <strong>${escapeHtml(runbook.title)}</strong>
        <span>${escapeHtml(runbook.description || runbook.prompt_template)}</span>
      </div>
    `;
  }

  wrap.innerHTML = `
    <header class="ctox-pane-header">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">Automatisierung</span>
          <h2 class="ctox-pane-title spreadsheets-column-title">${escapeHtml(state.t('runbook', 'Runbook'))}</h2>
        </div>
      </div>
    </header>
    <div class="spreadsheets-runbook-list" data-spreadsheets-runbooks-list>
      ${listHtml}
    </div>
    <div class="spreadsheets-runbook-workbench">
      <textarea placeholder="${escapeHtml(state.t('prompt', 'Prompt an CTOX senden...'))}" data-spreadsheets-prompt></textarea>
      <button type="button" data-spreadsheets-send ${record ? '' : 'disabled'}>
        ${iconSvg('play')} ${escapeHtml(state.t('send', 'Prompt senden'))}
      </button>
    </div>
  `;

  // Bind right runbook controls
  wrap.addEventListener('pointerdown', (event) => {
    if (event.target.closest('[data-spreadsheets-prompt], .spreadsheets-runbook-card, [data-spreadsheets-send]')) {
      relinquishSpreadsheetGridFocus(state);
    }
  }, { capture: true });
  wrap.addEventListener('focusin', (event) => {
    if (event.target.closest('[data-spreadsheets-prompt]')) {
      relinquishSpreadsheetGridFocus(state);
    }
  });

  const runbookCards = wrap.querySelectorAll('.spreadsheets-runbook-card');
  let selectedRunbookId = SYSTEMATIC_SPREADSHEET_RUNBOOKS[0].id;

  runbookCards.forEach(card => {
    if (card.dataset.runbookId === selectedRunbookId) {
      card.classList.add('is-active');
    }
    card.addEventListener('click', () => {
      runbookCards.forEach(c => c.classList.remove('is-active'));
      card.classList.add('is-active');
      selectedRunbookId = card.dataset.runbookId;

      // Auto-populate textarea prompt with template
      const rb = state.runbooks.find(r => r.id === selectedRunbookId);
      if (rb) {
        wrap.querySelector('[data-spreadsheets-prompt]').value = rb.prompt_template;
      }
    });
  });

  // Prepopulate prompt box
  const initialRb = state.runbooks.find(r => r.id === selectedRunbookId);
  if (initialRb) {
    wrap.querySelector('[data-spreadsheets-prompt]').value = initialRb.prompt_template;
  }

  const sendBtn = wrap.querySelector('[data-spreadsheets-send]');
  if (sendBtn) {
    sendBtn.addEventListener('click', async () => {
      const promptBox = wrap.querySelector('[data-spreadsheets-prompt]');
      const promptText = promptBox.value.trim();
      if (!promptText || !record) return;

      sendBtn.disabled = true;
      const initialLabel = sendBtn.innerHTML;
      sendBtn.textContent = 'Executing...';

      try {
        await flushActiveSpreadsheetDraft(state);
        await dispatchSpreadsheetRunbook(state, {
          record,
          versionId: record.current_version_id,
          runbookId: selectedRunbookId,
          prompt: promptText,
          sourceAction: 'spreadsheet_runbook'
        });

        // Show success visual response
        promptBox.value = '';
        state.ctx.notifications?.success('Spreadsheet Runbook erfolgreich in CTOX Queue eingereiht.');
      } catch (err) {
        console.error(err);
        state.ctx.notifications?.error(`Fehler beim Ausführen des Runbooks: ${err.message}`);
      } finally {
        sendBtn.disabled = false;
        sendBtn.innerHTML = initialLabel;
      }
    });
  }

  state.ctx.right.replaceChildren(wrap);
}

function relinquishSpreadsheetGridFocus(state) {
  try { state.editorHandle?.closeEditor?.(); } catch {}
  try { state.editorHandle?.resetSelection?.(); } catch {}
  const active = document.activeElement;
  if (active && state.ctx.host.contains(active) && active.closest?.('[data-spreadsheets-canvas]')) {
    active.blur?.();
  }
}

async function dispatchSpreadsheetRunbook(state, input) {
  const runbook = state.runbooks.find(r => r.id === input.runbookId);
  return state.ctx.commandBus.dispatch({
    module: 'spreadsheets',
    type: runbook?.command_type || input.runbookId || 'spreadsheet.summarize',
    record_id: input.record.id,
    payload: {
      spreadsheet_id: input.record.id,
      version_id: input.versionId || input.record.current_version_id,
      prompt: input.prompt || '',
      runbook_id: runbook?.id || input.runbookId,
      prompt_template: runbook?.prompt_template || '',
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

function updateNewSpreadsheetSubmitState(state, form) {
  if (!form) return false;
  const validation = validateNewSpreadsheetInput(readNewSpreadsheetInput(form));
  const message = validation.valid ? '' : state.t(validation.key, validation.message);
  setFormValidationState(form, validation.valid, message);
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
    return { valid: false, key: 'validationUnsupportedFile', message: 'Nur CSV oder JSON.' };
  }
  return { valid: true, message: '' };
}

function updateImportSubmitState(state, form) {
  if (!form) return false;
  const validation = validateImportInput(readImportInput(form));
  const message = validation.valid ? '' : state.t(validation.key, validation.message);
  setFormValidationState(form, validation.valid, message);
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
    || file.type === JSON_MIME;
}

function focusFirstDrawerControl(wrapper) {
  const target = wrapper.querySelector('input:not([disabled]), select:not([disabled]), textarea:not([disabled]), button:not([disabled])');
  if (target instanceof HTMLElement) {
    requestAnimationFrame(() => target.focus({ preventScroll: true }));
  }
}

function openNewSpreadsheetDrawer(state) {
  const wrapper = document.createElement('div');
  wrapper.className = 'spreadsheets-drawer-form';
  wrapper.innerHTML = `
    <h3>${escapeHtml(state.t('newDocumentTitle', 'Neue Tabelle'))}</h3>
    <p class="spreadsheets-drawer-copy">${escapeHtml(state.t('newSpreadsheetDescription', 'Erstellt einen gespeicherten Tabellenentwurf mit Beispielstruktur.'))}</p>
    <form data-spreadsheets-new-form novalidate>
      <label>
        <span>${escapeHtml(state.t('title', 'Titel'))}</span>
        <input name="title" type="text" value="${escapeHtml(`${state.t('newDocumentTitle', 'Neue Tabelle')} - ${new Date().toISOString().slice(0, 10)}`)}" required data-new-title>
      </label>
      <label style="margin-top: 8px;">
        <span>${escapeHtml(state.t('tags', 'Tags (kommagetrennt)'))}</span>
        <input name="tags" type="text" placeholder="Budget, Forecast" data-new-tags>
      </label>
      <p class="spreadsheets-form-status" role="status" data-spreadsheets-form-status></p>
      <div class="spreadsheets-drawer-actions">
        <button type="button" data-drawer-cancel>${escapeHtml(state.t('cancel', 'Abbrechen'))}</button>
        <button type="submit">${escapeHtml(state.t('createDraft', 'Entwurf erstellen'))}</button>
      </div>
    </form>
  `;

  wrapper.querySelector('[data-drawer-cancel]').addEventListener('click', () => {
    state.ctx.closeDrawers();
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
        submit.textContent = state.t('creatingDraft', 'Entwurf wird erstellt...');
      }
      state.ctx.closeDrawers();
      await createNewSpreadsheet(state, input);
      state.ctx.notifications?.success(state.t('draftCreated', 'Tabellenentwurf erstellt.'));
    } catch (err) {
      console.error(err);
      state.ctx.notifications?.error(`Fehler beim Erstellen: ${err.message}`);
    }
  });

  state.ctx.openLeftDrawer(wrapper);
  focusFirstDrawerControl(wrapper);
}

function openImportModal(state) {
  const wrapper = document.createElement('div');
  wrapper.className = 'spreadsheets-drawer-form';
  wrapper.innerHTML = `
    <form data-spreadsheets-import-form novalidate>
      <label>
        <span>${escapeHtml(state.t('file', 'Datei auswählen (CSV oder JSON)'))}</span>
        <input type="file" accept=".csv,.json" required data-import-file>
      </label>
      <label style="margin-top: 8px;">
        <span>${escapeHtml(state.t('tags', 'Tags (kommagetrennt)'))}</span>
        <input type="text" placeholder="Sales, Q2, Forecast" data-import-tags>
      </label>
      <p class="spreadsheets-form-status" role="status" data-spreadsheets-form-status></p>
      <div class="spreadsheets-drawer-actions">
        <button type="button" data-drawer-cancel>${escapeHtml(state.t('cancel', 'Abbrechen'))}</button>
        <button type="submit" disabled aria-disabled="true">${escapeHtml(state.t('import', 'Importieren'))}</button>
      </div>
    </form>
  `;

  wrapper.querySelector('[data-drawer-cancel]').addEventListener('click', () => {
    state.ctx.closeDrawers();
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

    state.ctx.closeDrawers();
    try {
      await importSpreadsheetFile(state, file, tags);
      state.ctx.notifications?.success(`Datei ${file.name} erfolgreich importiert.`);
    } catch (err) {
      console.error(err);
      state.ctx.notifications?.error(`Fehler beim Importieren: ${err.message}`);
    }
  });

  state.ctx.openLeftDrawer(wrapper);
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
        <span>${escapeHtml(state.t('documentType', 'Exportformat'))}</span>
        <select data-export-format>
          <option value="csv">CSV (Kommagetrennt)</option>
          <option value="json">JSON (JSpreadsheet Struktur)</option>
        </select>
      </label>
      <div class="spreadsheets-drawer-actions" style="margin-top: 12px;">
        <button type="button" data-drawer-cancel>${escapeHtml(state.t('cancel', 'Abbrechen'))}</button>
        <button type="submit">${escapeHtml(state.t('export', 'Exportieren'))}</button>
      </div>
    </form>
  `;

  wrapper.querySelector('[data-drawer-cancel]').addEventListener('click', () => {
    state.ctx.closeDrawers();
  });

  wrapper.querySelector('form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const format = wrapper.querySelector('[data-export-format]').value;
    state.ctx.closeDrawers();

    try {
      if (!state.editorHandle) throw new Error('Editor nicht initialisiert.');
      await flushActiveSpreadsheetDraft(state);

      const rawData = state.editorHandle.getData();
      let content = '';
      let fileExt = '.csv';
      let mime = CSV_MIME;

      if (format === 'json') {
        const mergeData = state.editorHandle.getMerge?.() || null;
        const styleData = state.editorHandle.getStyle?.() || null;
        const columnsData = (state.editorHandle.options.columns || []).map((col, idx) => ({
          type: col.type || 'text',
          title: col.title || String.fromCharCode(65 + idx),
          width: col.width || '120px',
          mask: col.mask || null
        }));

        const modelJson = {
          data: rawData,
          columns: columnsData,
          nestedHeaders: state.editorHandle.options.nestedHeaders || null,
          mergeCells: mergeData,
          style: styleData
        };
        content = JSON.stringify(modelJson, null, 2);
        fileExt = '.json';
        mime = JSON_MIME;
      } else {
        // CSV Format
        content = rawData.map(row => row.map(val => `"${String(val).replace(/"/g, '""')}"`).join(',')).join('\n');
      }

      const downloadName = ensureExtension(slugFilename(record.title || 'export'), fileExt);
      const blob = new Blob([content], { type: mime });
      const url = URL.createObjectURL(blob);
      const link = document.createElement('a');
      link.href = url;
      link.download = downloadName;
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);
      setTimeout(() => URL.revokeObjectURL(url), 1000);

      state.ctx.notifications?.success(`Export abgeschlossen: ${downloadName}`);
    } catch (err) {
      console.error(err);
      state.ctx.notifications?.error(`Fehler beim Exportieren: ${err.message}`);
    }
  });

  state.ctx.openLeftDrawer(wrapper);
}

async function openManageDrawer(state, id) {
  const doc = await state.ctx.db.raw.spreadsheets.findOne(id).exec();
  if (!doc) return;
  const data = doc.toJSON();

  const wrapper = document.createElement('div');
  wrapper.className = 'spreadsheets-drawer-form';
  wrapper.innerHTML = `
    <h3>${escapeHtml(state.t('manageDocumentTitle', 'Tabelle verwalten'))}</h3>
    <form>
      <label>
        <span>${escapeHtml(state.t('title', 'Titel'))}</span>
        <input type="text" data-field="title" value="${escapeHtml(data.title)}" required>
      </label>
      <label style="margin-top: 8px;">
        <span>${escapeHtml(state.t('status', 'Status'))}</span>
        <select data-field="status">
          <option value="Draft" ${data.status === 'Draft' ? 'selected' : ''}>Draft</option>
          <option value="Imported" ${data.status === 'Imported' ? 'selected' : ''}>Imported</option>
          <option value="Review" ${data.status === 'Review' ? 'selected' : ''}>Review</option>
          <option value="Final" ${data.status === 'Final' ? 'selected' : ''}>Final</option>
        </select>
      </label>
      <label style="margin-top: 8px;">
        <span>${escapeHtml(state.t('tags', 'Tags (kommagetrennt)'))}</span>
        <input type="text" data-field="tags" value="${escapeHtml((data.tags || []).join(', '))}">
      </label>
      <div class="spreadsheets-drawer-actions" style="margin-top: 16px;">
        <button type="button" class="danger-button" data-action="delete">${escapeHtml(state.t('delete', 'Tabelle löschen'))}</button>
        <button type="submit" style="grid-column: 2;">${escapeHtml(state.t('save', 'Speichern'))}</button>
      </div>
    </form>
  `;

  wrapper.querySelector('form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const titleVal = wrapper.querySelector('[data-field="title"]').value.trim();
    const statusVal = wrapper.querySelector('[data-field="status"]').value;
    const tagsVal = wrapper.querySelector('[data-field="tags"]').value.split(',').map(t => t.trim()).filter(Boolean);

    try {
      await doc.incrementalPatch({
        title: titleVal,
        status: statusVal,
        tags: tagsVal,
        updated_at_ms: Date.now()
      });
      state.ctx.closeDrawers();
      state.ctx.notifications?.success('Änderungen erfolgreich gespeichert.');
      await refreshSpreadsheets(state);
      renderLeft(state);
      if (state.selectedId === id) {
        renderCenter(state);
      }
    } catch (err) {
      console.error(err);
      state.ctx.notifications?.error(`Fehler beim Speichern: ${err.message}`);
    }
  });

  wrapper.querySelector('[data-action="delete"]').addEventListener('click', async () => {
    const confirmed = await showBusinessConfirm(
      state.ctx,
      state.t('deleteConfirmTitle', 'Tabelle löschen'),
      state.t('deleteConfirmMessage', `Tabelle "${data.title}" wirklich unwiderruflich löschen?`, data.title)
    );
    if (!confirmed) return;

    try {
      await doc.incrementalPatch({ is_deleted: true, updated_at_ms: Date.now() });
      state.ctx.closeDrawers();
      state.ctx.notifications?.success('Tabelle erfolgreich gelöscht.');

      if (state.selectedId === id) {
        state.selectedId = '';
        state.selectedVersion = null;
        if (state.spreadsheetContainer && state.jspreadsheetModule) {
          try { state.jspreadsheetModule.destroy(state.spreadsheetContainer); } catch {}
          state.spreadsheetContainer = null;
        }
        state.editorHandle = null;
      }

      await refreshSpreadsheets(state);
      renderLeft(state);
      renderCenter(state);
    } catch (err) {
      console.error(err);
      state.ctx.notifications?.error(`Fehler beim Löschen: ${err.message}`);
    }
  });

  state.ctx.openLeftDrawer(wrapper);
}

function initSpreadsheetsContextMenu(state) {
  state.contextMenu?.remove();
  const menu = document.createElement('div');
  menu.className = 'ctox-context-menu spreadsheets-context-menu';
  menu.hidden = true;
  document.body.append(menu);
  state.contextMenu = menu;

  const handleContextMenu = (event) => {
    if (state.ctx.module?.id !== 'spreadsheets') return;
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

  state.ctx.host.addEventListener('contextmenu', handleContextMenu);
  window.addEventListener('click', handleOutsideClick, { capture: true });
  window.addEventListener('keydown', handleEscape);

  return () => {
    state.ctx.host.removeEventListener('contextmenu', handleContextMenu);
    window.removeEventListener('click', handleOutsideClick, { capture: true });
    window.removeEventListener('keydown', handleEscape);
    hideSpreadsheetsContextMenu(state);
  };
}

function hideSpreadsheetsContextMenu(state) {
  if (state.contextMenu) state.contextMenu.hidden = true;
}

function canModifySpreadsheetsApp(state) {
  if (typeof state.ctx.canModifyModule === 'function' && state.ctx.canModifyModule()) return true;
  const user = state.ctx.session?.user || {};
  const role = String(user.role || (user.is_admin ? 'admin' : 'user')).trim().toLowerCase().replace(/^business_os_/, '');
  return ['admin', 'chef'].includes(role);
}

function spreadsheetCommandContextFromElement(state, target) {
  const element = target?.nodeType === Node.ELEMENT_NODE ? target : target?.parentElement;
  const record = selectedRecord(state);
  const column = state.ctx.left?.contains?.(element) ? 'spreadsheets' : 'editor';

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
  ensureCtoxContextMenuStyles();
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
      ${canModifyApp ? `
        <div class="ctox-context-mode" role="radiogroup" aria-label="${escapeHtml(state.t('chatActionLabel', 'CTOX Aufgabe'))}">
          <label><input type="radio" name="contextMode" value="data" checked /> ${escapeHtml(state.t('chatWorkDataLabel', 'Mit Daten arbeiten'))}</label>
          <label><input type="radio" name="contextMode" value="app" /> ${escapeHtml(state.t('chatModifyAppLabel', 'App modifizieren'))}</label>
        </div>
      ` : ''}
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
    const mode = canModifyApp ? (new FormData(form).get('contextMode') || 'data') : 'data';
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

  const safeMode = mode === 'app' && canModifySpreadsheetsApp(state) ? 'app' : 'data';
  const record = selectedRecord(state);
  if (!document.querySelector('[data-ctox-chat-root]')) {
    if (status) status.textContent = state.t('chatNotReady', 'Chat ist noch nicht bereit.');
    return;
  }
  if (status) status.textContent = state.t('chatOpening', 'Öffne Chat...');
  const title = `${safeMode === 'app' ? 'Spreadsheets App modifizieren' : 'Spreadsheet bearbeiten'} · ${context.label || 'Spreadsheets'}`;
  const instruction = safeMode === 'app'
    ? `Modifiziere die Spreadsheets-App anhand dieser Admin-Anweisung. Kontext nur als UI-Bezug verwenden, Tabellendaten selbst nicht als primäres Ziel verändern.\n\n${trimmed}`
    : trimmed;

  window.dispatchEvent(new CustomEvent('ctox-business-os-chat-submit', {
    detail: {
      text: trimmed,
      module: 'spreadsheets',
      source_title: 'Spreadsheets',
      command_type: safeMode === 'app' ? 'ctox.business_os.app.modify' : 'business_os.chat.task',
      record_id: safeMode === 'app' ? 'spreadsheets' : (record?.id || 'spreadsheets'),
      title,
      instruction,
      payload: {
        title,
        instruction,
        prompt: trimmed,
        user_message: trimmed,
        mode: safeMode,
        target: safeMode === 'app' ? 'app' : 'data',
        selected_spreadsheet: record,
        context,
        thread_key: 'business-os/spreadsheets',
      },
      client_context: {
        action: 'context-chat',
        mode: safeMode,
        column: context.column,
        record_type: context.record_type,
        spreadsheet_id: record?.id || '',
        filename: record?.filename || '',
      },
    },
  }));
  hideSpreadsheetsContextMenu(state);
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
    .ctox-context-menu form [data-context-close] {
      width: 30px;
      min-width: 30px;
      padding: 0;
      text-align: center;
      font-size: 18px;
    }
  `;
  document.head.append(style);
}

async function ensureSeedRunbooks(ctx) {
  const collection = ctx.db?.raw?.spreadsheet_runbooks;
  if (!collection) return;
  const existing = await collection.find().exec();
  const now = Date.now();
  const existingIds = new Set(existing.map((doc) => doc.toJSON().id));

  for (const runbook of SYSTEMATIC_SPREADSHEET_RUNBOOKS) {
    if (existingIds.has(runbook.id)) continue;
    await collection.insert({
      ...runbook,
      created_at_ms: now,
      updated_at_ms: now,
    });
  }
}

function requireSpreadsheetPersistence(ctx) {
  const raw = ctx?.db?.raw;
  if (!raw?.spreadsheets || !raw?.spreadsheet_versions || !raw?.spreadsheet_blob_chunks) {
    throw new Error('CTOX spreadsheet persistence layer is unavailable. Spreadsheet data must be persisted via RxDB collections.');
  }
}

// Helpers
async function ensureStyles() {
  if (document.querySelector('link[data-spreadsheets-style]')) return;

  const linkJsuites = document.createElement('link');
  linkJsuites.rel = 'stylesheet';
  linkJsuites.href = new URL('../../vendor/jsuites.css', import.meta.url).href;
  linkJsuites.dataset.jsuitesStyle = 'true';
  document.head.append(linkJsuites);

  const linkJspread = document.createElement('link');
  linkJspread.rel = 'stylesheet';
  linkJspread.href = new URL('../../vendor/jspreadsheet.css', import.meta.url).href;
  linkJspread.dataset.jspreadsheetStyle = 'true';
  document.head.append(linkJspread);

  const linkModule = document.createElement('link');
  linkModule.rel = 'stylesheet';
  linkModule.href = new URL('./index.css', import.meta.url).href;
  linkModule.dataset.spreadsheetsStyle = 'true';
  document.head.append(linkModule);
}

async function sha256Hex(bytes) {
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

function uint8ToBase64(bytes) {
  let binary = '';
  for (let i = 0; i < bytes.length; i += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return btoa(binary);
}

function base64ToUint8(base64) {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

function saveBlobChunks(ctx, input) {
  const base64 = uint8ToBase64(input.bytes);
  const total = Math.ceil(base64.length / CHUNK_SIZE) || 1;
  const promises = [];
  for (let idx = 0; idx < total; idx += 1) {
    promises.push(
      ctx.db.raw.spreadsheet_blob_chunks.insert({
        id: `${input.blobId}_${idx}`,
        blob_id: input.blobId,
        spreadsheet_id: input.spreadsheetId,
        version_id: input.versionId,
        idx,
        total,
        mime_type: input.mimeType,
        encoding: 'base64',
        data: base64.slice(idx * CHUNK_SIZE, (idx + 1) * CHUNK_SIZE),
        created_at_ms: Date.now(),
      })
    );
  }
  return Promise.all(promises);
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

async function withTimeout(promise, ms, message) {
  let timeoutId;
  const timeoutPromise = new Promise((_, reject) => {
    timeoutId = setTimeout(() => reject(new Error(message)), ms);
  });
  return Promise.race([promise, timeoutPromise]).finally(() => clearTimeout(timeoutId));
}

export const __spreadsheetsTestHooks = {
  hasActiveListFilters,
  isActiveSpreadsheetRecord,
  normalizeSpreadsheetRecord,
  normalizeSpreadsheetModel,
  validateImportInput,
  validateNewSpreadsheetInput,
  visibleSpreadsheets,
};

function iconSvg(name) {
  const SVGS = {
    new: `<svg viewBox="0 0 16 16"><path d="M8 1v14M1 8h14" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>`,
    import: `<svg viewBox="0 0 16 16"><path d="M8 1v9M5 7l3 3 3-3M1 12h14v2H1z" fill="currentColor"/></svg>`,
    export: `<svg viewBox="0 0 16 16"><path d="M8 10V1M5 4l3-3 3 3M1 12h14v2H1z" fill="currentColor"/></svg>`,
    gear: `<svg viewBox="0 0 16 16"><path d="M8 5a3 3 0 1 0 0 6 3 3 0 0 0 0-6zm-6.2 1.8l-1.3-.3a1 1 0 0 1-.7-.8V4.3a1 1 0 0 1 .7-.9l1.3-.3.3-1.3.9-.9a1 1 0 0 1 1-.2l1.2.5h1.6l1.2-.5a1 1 0 0 1 1 .2l.9.9.3 1.3 1.3.3c.5.1.8.5.8.9v1.4c0 .5-.3.8-.8.9l-1.3.3-.3 1.3-.9.9a1 1 0 0 1-1 .2l-1.2-.5H6.8l-1.2.5a1 1 0 0 1-1-.2l-.9-.9-.3-1.3z" fill="none" stroke="currentColor" stroke-width="1.5"/></svg>`,
    plus: `<svg viewBox="0 0 16 16" width="12" height="12"><path d="M8 2v12M2 8h12" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>`,
    play: `<svg viewBox="0 0 16 16" width="12" height="12"><path d="M3 2l10 6-10 6z" fill="currentColor"/></svg>`
  };
  return SVGS[name] || '';
}
