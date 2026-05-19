const STATUS_KEY = 'ctox.businessOs.importer.status.v1';

export async function openUniversalImporter(ctx, config = {}) {
  await ensureImporterStyles();
  const drawer = document.createElement('aside');
  const side = config.side === 'left' ? 'left' : 'right';
  drawer.className = 'universal-importer-drawer';
  drawer.dataset.side = side;
  drawer.setAttribute('role', 'dialog');
  drawer.setAttribute('aria-modal', 'true');
  drawer.innerHTML = importerTemplate(config);
  document.body.append(drawer);

  const close = () => drawer.remove();
  drawer.querySelector('[data-action="close-importer"]')?.addEventListener('click', close);
  drawer.querySelector('[data-import-source]')?.addEventListener('change', () => updateImporterFields(drawer));
  drawer.querySelector('[data-action="submit-importer"]')?.addEventListener('click', async () => {
    const status = drawer.querySelector('[data-import-status]');
    const submitButton = drawer.querySelector('[data-action="submit-importer"]');
    submitButton?.setAttribute('disabled', 'disabled');
    status.textContent = config.submittingLabel || 'Import wird vorbereitet...';
    try {
      const payload = await buildImportPayload(drawer, config);
      const command = {
        id: payload.record_id,
        module: config.moduleId || 'business-os',
        type: config.commandType || `${config.moduleId || 'business-os'}.source.import`,
        record_id: payload.record_id,
        inbound_channel: config.inboundChannel || `business_os.${config.moduleId || 'importer'}`,
        payload,
        client_context: {
          source_module: config.moduleId || '',
          entity_type: config.entityType || '',
          ...(config.clientContext || {}),
        },
      };
      const runImport = async () => {
        const localResult = await config.onImport?.({ payload, command, drawer });
        let dispatchResult = null;
        if (config.dispatch !== false && localResult?.dispatch !== false) {
          dispatchResult = await dispatchImportCommand(ctx, command);
        }
        recordImportStatus({
          id: payload.record_id,
          module_id: config.moduleId || '',
          title: payload.title,
          source_type: payload.source_type,
          status: localResult?.status || dispatchResult?.status || 'imported',
          detail: localResult?.detail || dispatchResult?.error || '',
          updated_at_ms: Date.now(),
        });
        if (drawer.isConnected) {
          status.textContent = localResult?.message || config.doneLabel || 'Import angelegt.';
          if (config.closeOnDone !== false) window.setTimeout(close, 260);
        }
      };
      if (config.closeOnSubmit) {
        close();
        runImport().catch((error) => console.warn('[business-os importer] background import failed', error));
        return;
      }
      await runImport();
    } catch (error) {
      submitButton?.removeAttribute('disabled');
      status.textContent = error?.message || String(error);
    }
  });
  updateImporterFields(drawer);
  drawer.querySelector('[data-import-title]')?.focus();
  return drawer;
}

export async function dispatchImportCommand(ctx, command) {
  if (ctx?.commandBus?.dispatch) {
    return ctx.commandBus.dispatch(command);
  }
  const collection = ctx?.db?.raw?.business_commands;
  if (!collection) throw new Error('business_commands collection is required for RxDB commands');
  const commandId = command.id || `cmd_${crypto.randomUUID()}`;
  await collection.insert({
    id: commandId,
    command_id: commandId,
    module: command.module,
    command_type: command.type,
    record_id: command.record_id || '',
    status: 'pending_sync',
    inbound_channel: command.inbound_channel || command.module || '',
    payload: command.payload || {},
    client_context: command.client_context || {},
    updated_at_ms: Date.now(),
  });
  return { ok: true, command_id: commandId, status: 'pending_sync', transport: 'rxdb' };
}

export function readImportStatuses(moduleId = '') {
  try {
    const items = JSON.parse(window.localStorage.getItem(STATUS_KEY) || '[]');
    return moduleId ? items.filter((item) => item.module_id === moduleId) : items;
  } catch {
    return [];
  }
}

export function recordImportStatus(status) {
  const items = readImportStatuses().filter((item) => item.id !== status.id);
  items.unshift(status);
  window.localStorage.setItem(STATUS_KEY, JSON.stringify(items.slice(0, 80)));
}

export function parseDelimitedText(text, options = {}) {
  const lines = String(text || '')
    .replace(/\r\n/g, '\n')
    .replace(/\r/g, '\n')
    .split('\n')
    .filter((line) => line.trim());
  if (!lines.length) return [];
  const delimiter = options.delimiter || detectDelimiter(lines.slice(0, 5));
  const rows = lines.map((line) => splitDelimitedLine(line, delimiter));
  const header = rows[0].map(normalizeHeader);
  const hasHeader = header.some((name) => COMPANY_HEADER_KEYS.has(name) || DOMAIN_HEADER_KEYS.has(name));
  if (!hasHeader) {
    return rows.map((cells, index) => ({ __rowIndex: index, company: cleanCell(cells[0]), domain: cleanCell(cells[1]), raw: cells }));
  }
  return rows.slice(1).map((cells, index) => {
    const row = { __rowIndex: index, raw: cells };
    header.forEach((key, cellIndex) => {
      if (key) row[key] = cleanCell(cells[cellIndex]);
    });
    return row;
  });
}

export function renderUniversalImportDrawerMarkup(options = {}) {
  const recordLabel = options.recordLabel || 'Datensatz';
  const defaultSource = options.defaultSource || 'document';
  const submitLabel = options.submitLabel || 'Import an CTOX übergeben';
  const sourceButton = (id, label) => `
    <button
      class="import-source-button${id === defaultSource ? ' is-active' : ''}"
      type="button"
      data-import-source="${escapeHtml(id)}"
      aria-pressed="${id === defaultSource ? 'true' : 'false'}"
    >${escapeHtml(label)}</button>
  `;
  return `
    <div class="import-source-grid" aria-label="Importtyp">
      ${sourceButton('text', 'Freitext')}
      ${sourceButton('document', 'Document')}
      ${sourceButton('url', 'URL')}
      ${sourceButton('excel', 'Excel')}
    </div>

    <label class="drawer-field">
      <span>Importer</span>
      <select>
        <option>CTOX Auto Import</option>
        <option>URL / Scraper</option>
        <option>Datei / Archiv</option>
        <option>Excel / Tabellen</option>
        <option>Freitext Parser</option>
      </select>
    </label>

    <section class="import-panel" data-import-panel="text" ${defaultSource === 'text' ? '' : 'hidden'}>
      <label class="drawer-field">
        <span>Titel</span>
        <input type="text" data-import-field="title" placeholder="${escapeHtml(recordLabel)} benennen" />
      </label>
      <label class="drawer-field">
        <span>Freitext</span>
        <textarea rows="8" data-import-field="text" placeholder="Text einfügen, der strukturiert werden soll"></textarea>
      </label>
      <label class="drawer-field">
        <span>Importumfang</span>
        <select data-import-field="scope">
          <option>Ein Datensatz</option>
          <option>Mehrere Abschnitte als getrennte Datensätze</option>
        </select>
      </label>
    </section>

    <section class="import-panel" data-import-panel="document" ${defaultSource === 'document' ? '' : 'hidden'}>
      <label class="drawer-field">
        <span>Dokumente</span>
        <input type="file" data-import-field="files" multiple />
      </label>
      <label class="drawer-field">
        <span>Dokumenttyp</span>
        <select data-import-field="document_type">
          <option>Automatisch erkennen</option>
          <option>PDF</option>
          <option>Word / Text</option>
          <option>ZIP Archiv</option>
        </select>
      </label>
      <label class="drawer-field">
        <span>Importumfang</span>
        <select data-import-field="scope">
          <option>Jede Datei als eigener Datensatz</option>
          <option>Alle Dateien als einen Importjob zusammenführen</option>
          <option>Archivinhalt automatisch aufteilen</option>
        </select>
      </label>
    </section>

    <section class="import-panel" data-import-panel="url" ${defaultSource === 'url' ? '' : 'hidden'}>
      <label class="drawer-field">
        <span>URL</span>
        <input type="url" data-import-field="url" placeholder="https://..." />
      </label>
      <label class="drawer-field">
        <span>Importumfang</span>
        <select data-import-field="scope">
          <option>Nur diese URL lesen</option>
          <option>Mehrere URLs aus der Seite erkennen</option>
          <option>Verlinkte Unterseiten mitlesen</option>
        </select>
      </label>
      <label class="drawer-field">
        <span>Maximale Tiefe</span>
        <select data-import-field="depth">
          <option>1 Ebene</option>
          <option>2 Ebenen</option>
          <option>3 Ebenen</option>
        </select>
      </label>
    </section>

    <section class="import-panel" data-import-panel="excel" ${defaultSource === 'excel' ? '' : 'hidden'}>
      <label class="drawer-field">
        <span>Excel oder CSV</span>
        <input type="file" data-import-field="files" accept=".xlsx,.xls,.csv,.tsv" />
      </label>
      <label class="drawer-field">
        <span>Tabellenblatt</span>
        <input type="text" data-import-field="sheet" placeholder="Automatisch oder Name des Sheets" />
      </label>
      <label class="drawer-field">
        <span>Zeilenlogik</span>
        <select data-import-field="row_logic">
          <option>Eine Zeile = ein Datensatz</option>
          <option>Gruppierte Zeilen zusammenführen</option>
          <option>CTOX erkennt Datensatzgrenzen</option>
        </select>
      </label>
    </section>

    <button class="drawer-primary" type="button" data-import-run>${escapeHtml(submitLabel)}</button>
  `;
}

export function bindUniversalImportSourceSwitching(root) {
  root.querySelectorAll('[data-import-source]').forEach((sourceButton) => {
    sourceButton.addEventListener('click', () => {
      setUniversalImportSource(root, sourceButton.dataset.importSource || 'document');
    });
  });
}

export function setUniversalImportSource(root, source) {
  root.querySelectorAll('[data-import-source]').forEach((button) => {
    const active = button.dataset.importSource === source;
    button.classList.toggle('is-active', active);
    button.setAttribute('aria-pressed', active ? 'true' : 'false');
  });
  root.querySelectorAll('[data-import-panel]').forEach((panel) => {
    panel.hidden = panel.dataset.importPanel !== source;
  });
}

export async function buildUniversalImportCommandPayloadFromDrawer(drawer, options = {}) {
  const sourceType =
    drawer.querySelector('[data-import-source].is-active')?.dataset.importSource ||
    drawer.querySelector('[data-import-panel]:not([hidden])')?.dataset.importPanel ||
    'document';
  const panel = drawer.querySelector(`[data-import-panel="${sourceType}"]`);
  const readValue = (name) => panel?.querySelector(`[data-import-field="${name}"]`)?.value || '';
  const files = await readMatchingStyleImportFiles(panel);
  const recordId = options.recordId || `import_${options.column || options.entityType || 'source'}_${sourceType}_${Date.now()}`;
  return {
    record_id: recordId,
    title: options.title || `${options.recordLabel || options.entityType || 'Quelle'} Import`,
    module_id: options.moduleId || '',
    column: options.column || '',
    entity_type: options.entityType || '',
    source_type: sourceType,
    parser: options.parser || 'ctox.auto_import',
    definition: options.definition || {},
    source: {
      title: readValue('title'),
      text: readValue('text'),
      url: readValue('url'),
      scope: readValue('scope'),
      depth: readValue('depth'),
      sheet: readValue('sheet'),
      row_logic: readValue('row_logic'),
      document_type: readValue('document_type'),
      files
    }
  };
}

async function readMatchingStyleImportFiles(panel) {
  const input = panel?.querySelector('input[type="file"][data-import-field="files"]');
  if (!input?.files?.length) return [];
  const files = [];
  for (const file of Array.from(input.files)) {
    files.push({
      name: file.name,
      type: file.type || 'application/octet-stream',
      size: file.size,
      lastModified: file.lastModified,
      base64: await fileToBase64(file)
    });
  }
  return files;
}

export function extractCompanyRowsFromText(text) {
  const rows = parseDelimitedText(text);
  return rows
    .map((row, index) => normalizeCompanyRow(row, index))
    .filter((row) => row.name);
}

export function normalizeCompanyRow(row, index = 0) {
  const companyKeys = ['company', 'unternehmen', 'firma', 'organisation', 'organization', 'account', 'name', 'companyname'];
  const domainKeys = ['domain', 'website', 'url', 'webseite', 'homepage'];
  const cityKeys = ['city', 'ort', 'stadt'];
  const countryKeys = ['country', 'land'];
  const name = firstValue(row, companyKeys) || cleanCompanyName(row.raw?.[0] || '');
  const website = normalizeUrl(firstValue(row, domainKeys) || row.domain || '');
  return {
    row_index: Number.isFinite(row.__rowIndex) ? row.__rowIndex : index,
    name: cleanCompanyName(name),
    website,
    domain: website ? domainFromUrl(website) : '',
    city: firstValue(row, cityKeys) || '',
    country: firstValue(row, countryKeys) || '',
    raw: row,
  };
}

export async function filePayloadFromInput(input) {
  const files = Array.from(input?.files || []);
  return Promise.all(files.map(async (file) => ({
    name: file.name,
    type: file.type || guessMimeType(file.name),
    size: file.size,
    lastModified: file.lastModified,
    text: isTextLikeFile(file) ? await file.text() : '',
    base64: await fileToBase64(file),
  })));
}

export function decodeBase64Utf8(base64) {
  try {
    const binary = atob(String(base64 || '').replace(/^data:[^,]+,/, ''));
    const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
    return new TextDecoder('utf-8').decode(bytes);
  } catch {
    return '';
  }
}

async function buildImportPayload(drawer, config) {
  const sourceType = drawer.querySelector('[data-import-source]')?.value || 'text';
  const title = drawer.querySelector('[data-import-title]')?.value?.trim()
    || config.defaultTitle
    || `${config.title || 'Import'} ${new Date().toLocaleString()}`;
  const text = drawer.querySelector('[data-import-text]')?.value || '';
  const url = drawer.querySelector('[data-import-url]')?.value?.trim() || '';
  const filterPrompt = drawer.querySelector('[data-import-filter-prompt]')?.value?.trim() || config.defaultFilterPrompt || '';
  const fileInput = drawer.querySelector('[data-import-files]');
  const files = await filePayloadFromInput(fileInput);
  if (sourceType === 'text' && !text.trim()) throw new Error('Bitte Text oder Zeilen einfügen.');
  if (sourceType === 'url' && !url) throw new Error('Bitte eine URL angeben.');
  if ((sourceType === 'document' || sourceType === 'table') && files.length === 0) throw new Error('Bitte mindestens eine Datei auswählen.');
  return {
    record_id: `import_${Date.now()}_${crypto.randomUUID()}`,
    title,
    module_id: config.moduleId || '',
    entity_type: config.entityType || '',
    source_type: sourceType,
    parser: config.parser || 'universal_importer.v1',
    definition: config.definition || {},
    filter_prompt: filterPrompt,
    source: {
      title,
      text,
      url,
      files,
      depth: Number(drawer.querySelector('[data-import-depth]')?.value || 1),
      filter_prompt: filterPrompt,
    },
    created_at_ms: Date.now(),
  };
}

function importerTemplate(config) {
  const sources = config.sources || [
    { id: 'text', label: 'Text' },
    { id: 'document', label: 'Dokument' },
    { id: 'url', label: 'URL' },
    { id: 'excel', label: 'Excel' },
  ];
  return `
    <div class="universal-importer-panel">
      <header>
        <div>
          <span>${escapeHtml(config.kicker || 'Business OS Importer')}</span>
          <h2>${escapeHtml(config.title || 'Importjob anlegen')}</h2>
        </div>
        <button type="button" class="universal-importer-icon" data-action="close-importer" aria-label="Importer schließen">×</button>
      </header>
      <label>
        <span>Titel</span>
        <input data-import-title value="${escapeHtml(config.defaultTitle || '')}" placeholder="${escapeHtml(config.titlePlaceholder || 'Importjob benennen')}" />
      </label>
      <label>
        <span>Importtyp</span>
        <select data-import-source>
          ${sources.map((source) => `<option value="${escapeHtml(source.id)}">${escapeHtml(source.label)}</option>`).join('')}
        </select>
      </label>
      <div data-source-panel="text">
        <label>
          <span>Inhalt</span>
          <textarea data-import-text placeholder="${escapeHtml(config.textPlaceholder || 'Eine Firma pro Zeile oder CSV mit Header einfügen')}"></textarea>
        </label>
      </div>
      <div data-source-panel="excel document">
        <label>
          <span>Datei</span>
          <input data-import-files type="file" multiple accept=".csv,.tsv,.txt,.md,.json,.xlsx,.xls,.docx,.pdf" />
        </label>
      </div>
      <div data-source-panel="url">
        <label>
          <span>URL</span>
          <input data-import-url placeholder="https://..." />
        </label>
        <label>
          <span>Importmodus</span>
          <select data-import-depth>
            <option value="1">Nur diese Seite auslesen</option>
            <option value="2" selected>Liste lesen und Unternehmensseiten verfolgen</option>
            <option value="3">Liste, Unternehmensseiten und weitere Unterseiten prüfen</option>
          </select>
        </label>
        <p class="universal-importer-help universal-importer-field-help">
          Für Aussteller- oder Firmenlisten reicht normalerweise der mittlere Modus. Kontakte werden später in der Pipeline qualifiziert.
        </p>
      </div>
      <label>
        <span>${escapeHtml(config.filterPromptLabel || 'On-the-fly Filter')}</span>
        <textarea
          data-import-filter-prompt
          rows="3"
          placeholder="${escapeHtml(config.filterPromptPlaceholder || 'z.B. nur deutsche Unternehmen, nur Hersteller, keine Dienstleister')}"
        >${escapeHtml(config.defaultFilterPrompt || '')}</textarea>
      </label>
      ${config.helperText ? `<p class="universal-importer-help">${escapeHtml(config.helperText)}</p>` : ''}
      <footer>
        <span data-import-status></span>
        <button type="button" data-action="submit-importer">${escapeHtml(config.submitLabel || 'Importieren')}</button>
      </footer>
    </div>
  `;
}

function updateImporterFields(drawer) {
  const selected = drawer.querySelector('[data-import-source]')?.value || 'text';
  for (const panel of drawer.querySelectorAll('[data-source-panel]')) {
    const values = String(panel.dataset.sourcePanel || '').split(/\s+/);
    panel.hidden = !values.includes(selected);
  }
}

async function ensureImporterStyles() {
  const href = new URL('./universal-importer.css', import.meta.url).pathname;
  if (document.querySelector(`link[href="${href}"]`)) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  document.head.append(link);
}

function detectDelimiter(lines) {
  const candidates = [';', '\t', ','];
  return candidates
    .map((delimiter) => ({ delimiter, count: lines.reduce((sum, line) => sum + splitDelimitedLine(line, delimiter).length, 0) }))
    .sort((a, b) => b.count - a.count)[0]?.delimiter || ';';
}

function splitDelimitedLine(line, delimiter) {
  const cells = [];
  let current = '';
  let quoted = false;
  for (let index = 0; index < line.length; index += 1) {
    const char = line[index];
    const next = line[index + 1];
    if (char === '"' && next === '"') {
      current += '"';
      index += 1;
    } else if (char === '"') {
      quoted = !quoted;
    } else if (char === delimiter && !quoted) {
      cells.push(current);
      current = '';
    } else {
      current += char;
    }
  }
  cells.push(current);
  return cells;
}

const COMPANY_HEADER_KEYS = new Set(['company', 'unternehmen', 'firma', 'organisation', 'organization', 'account', 'name', 'companyname']);
const DOMAIN_HEADER_KEYS = new Set(['domain', 'website', 'url', 'webseite', 'homepage']);

function normalizeHeader(value) {
  return String(value || '').trim().toLowerCase().replace(/[\s_-]+/g, '');
}

function cleanCell(value) {
  return String(value || '').trim().replace(/^"|"$/g, '').trim();
}

function cleanCompanyName(value) {
  return cleanCell(value).replace(/\s+/g, ' ');
}

function firstValue(row, keys) {
  for (const key of keys) {
    const normalized = normalizeHeader(key);
    if (row[normalized]) return cleanCell(row[normalized]);
    if (row[key]) return cleanCell(row[key]);
  }
  return '';
}

function normalizeUrl(value) {
  const text = cleanCell(value);
  if (!text) return '';
  if (/^https?:\/\//i.test(text)) return text;
  if (/^[\w.-]+\.[a-z]{2,}(\/.*)?$/i.test(text)) return `https://${text}`;
  return text;
}

function domainFromUrl(value) {
  try {
    return new URL(normalizeUrl(value)).hostname.replace(/^www\./, '');
  } catch {
    return String(value || '').replace(/^https?:\/\//, '').replace(/^www\./, '').split('/')[0];
  }
}

function isTextLikeFile(file) {
  return /\.(csv|tsv|txt|md|json)$/i.test(file.name) || /^text\//i.test(file.type || '');
}

function guessMimeType(name) {
  if (/\.csv$/i.test(name)) return 'text/csv';
  if (/\.tsv$/i.test(name)) return 'text/tab-separated-values';
  if (/\.txt$/i.test(name)) return 'text/plain';
  return 'application/octet-stream';
}

function fileToBase64(file) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result || '').split(',')[1] || '');
    reader.onerror = () => reject(reader.error || new Error('Datei konnte nicht gelesen werden.'));
    reader.readAsDataURL(file);
  });
}

function escapeHtml(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#039;');
}
