import { readStoredFileFromChunks } from './file-integrity.js?v=20260605-rxdb-cancel1';

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

  // In-memory list of files currently selected (either locally or from Business OS)
  const stagedFiles = [];
  drawer.stagedFiles = stagedFiles;

  // Business OS virtual filesystem navigation state
  let currentFolderId = 'fs_root';
  let folderDocs = new Map();
  let allFileDocs = [];

  function formatBytes(bytes) {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  }

  const stagedContainer = drawer.querySelector('[data-staged-files-container]');
  const stagedListEl = drawer.querySelector('[data-staged-files-list]');

  function renderStagedFiles() {
    if (!stagedListEl) return;
    if (stagedFiles.length === 0) {
      stagedContainer.hidden = true;
      stagedListEl.innerHTML = '';
      return;
    }
    stagedContainer.hidden = false;
    stagedListEl.innerHTML = stagedFiles.map((file, index) => {
      const originClass = file.source === 'business-os' ? 'origin-bos' : 'origin-local';
      const originLabel = file.source === 'business-os' ? 'Business OS' : 'Lokal';
      return `
        <div class="staged-file-item">
          <span class="file-icon-badge">${escapeHtml(file.name.split('.').pop()?.toUpperCase() || 'FILE')}</span>
          <div class="staged-file-details">
            <span class="file-name" title="${escapeHtml(file.name)}">${escapeHtml(file.name)}</span>
            <span class="file-meta">${formatBytes(file.size)} • <span class="file-origin-badge ${originClass}">${originLabel}</span></span>
          </div>
          <button type="button" class="remove-staged-file-btn" data-index="${index}" title="Datei entfernen">×</button>
        </div>
      `;
    }).join('');

    stagedListEl.querySelectorAll('.remove-staged-file-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        const index = parseInt(btn.dataset.index, 10);
        stagedFiles.splice(index, 1);
        renderStagedFiles();
      });
    });
  }

  // Local drag & drop
  const dropZone = drawer.querySelector('[data-drag-drop-zone]');
  const localFileInput = drawer.querySelector('[data-local-file-input]');

  if (dropZone && localFileInput) {
    dropZone.addEventListener('click', () => {
      localFileInput.click();
    });

    localFileInput.addEventListener('change', async () => {
      if (localFileInput.files?.length) {
        await addLocalFiles(localFileInput.files);
        localFileInput.value = '';
      }
    });

    dropZone.addEventListener('dragover', (e) => {
      e.preventDefault();
      dropZone.classList.add('dragover');
    });

    dropZone.addEventListener('dragleave', () => {
      dropZone.classList.remove('dragover');
    });

    dropZone.addEventListener('drop', async (e) => {
      e.preventDefault();
      dropZone.classList.remove('dragover');
      if (e.dataTransfer?.files?.length) {
        await addLocalFiles(e.dataTransfer.files);
      }
    });
  }

  async function addLocalFiles(fileList) {
    for (const file of Array.from(fileList)) {
      if (stagedFiles.some(f => f.name === file.name && f.size === file.size)) {
        continue;
      }
      try {
        const base64 = await fileToBase64(file);
        stagedFiles.push({
          name: file.name,
          type: file.type || guessMimeType(file.name),
          size: file.size,
          lastModified: file.lastModified,
          base64: base64,
          source: 'local'
        });
      } catch (err) {
        console.error('Failed to read local file:', err);
      }
    }
    renderStagedFiles();
  }

  // Business OS Explorer
  async function loadExplorerFiles() {
    const listEl = drawer.querySelector('[data-explorer-file-list]');
    if (!listEl) return;

    listEl.innerHTML = '<div class="explorer-loading">Lade Dateien...</div>';

    const db = ctx?.db;
    const collection = db?.collection?.('desktop_files');
    if (!collection) {
      listEl.innerHTML = '<div class="explorer-error">Fehler: Keine Verbindung zur OS-Datenbank.</div>';
      return;
    }

    try {
      const docs = await collection.find().exec();
      const data = docs.map(doc => (typeof doc.toJSON === 'function' ? doc.toJSON() : doc));

      allFileDocs = data.filter(item => !item.is_deleted);
      folderDocs = new Map(allFileDocs.filter(item => item.kind === 'folder').map(item => [item.id, item]));

      renderExplorerRows();
    } catch (err) {
      console.error('[importer explorer] failed to load files:', err);
      listEl.innerHTML = '<div class="explorer-error">Ladefehler.</div>';
    }
  }

  function renderExplorerRows() {
    const listEl = drawer.querySelector('[data-explorer-file-list]');
    const pathEl = drawer.querySelector('[data-explorer-path]');
    const upBtn = drawer.querySelector('[data-explorer-up]');
    if (!listEl) return;

    if (upBtn) {
      upBtn.disabled = currentFolderId === 'fs_root';
    }

    const currentFolder = folderDocs.get(currentFolderId) || { id: 'fs_root', name: 'Files', path: '/' };
    if (pathEl) {
      pathEl.textContent = currentFolder.name;
      pathEl.title = currentFolder.path;
    }

    const items = allFileDocs.filter(item => item.parent_id === currentFolderId);

    if (items.length === 0) {
      listEl.innerHTML = '<div class="explorer-empty">Der Ordner ist leer.</div>';
      return;
    }

    items.sort((a, b) => {
      if (a.kind === b.kind) {
        return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' });
      }
      return a.kind === 'folder' ? -1 : 1;
    });

    listEl.innerHTML = items.map(item => {
      const isFolder = item.kind === 'folder';
      const fileExt = isFolder ? 'DIR' : (item.name.split('.').pop()?.toUpperCase() || 'FILE');
      const iconClass = isFolder ? 'icon-folder' : 'icon-file';
      const sizeStr = isFolder ? '' : formatBytes(item.size_bytes || 0);

      return `
        <button type="button" class="explorer-row-item ${isFolder ? 'is-dir' : 'is-file'}" data-id="${item.id}" data-kind="${item.kind}">
          <span class="item-icon-indicator ${iconClass}">${fileExt}</span>
          <div class="item-info">
            <span class="item-name" title="${escapeHtml(item.name)}">${escapeHtml(item.name)}</span>
            ${isFolder ? '' : `<span class="item-size">${sizeStr}</span>`}
          </div>
        </button>
      `;
    }).join('');

    listEl.querySelectorAll('.explorer-row-item').forEach(itemEl => {
      const id = itemEl.dataset.id;
      const kind = itemEl.dataset.kind;

      if (kind === 'folder') {
        itemEl.addEventListener('dblclick', () => {
          currentFolderId = id;
          renderExplorerRows();
        });
        itemEl.addEventListener('click', () => {
          listEl.querySelectorAll('.explorer-row-item').forEach(el => el.classList.remove('is-selected'));
          itemEl.classList.add('is-selected');
        });
      } else {
        itemEl.addEventListener('click', async () => {
          listEl.querySelectorAll('.explorer-row-item').forEach(el => el.classList.remove('is-selected'));
          itemEl.classList.add('is-selected');
          await selectVirtualFile(id);
        });
      }
    });
  }

  async function selectVirtualFile(fileId) {
    const fileDoc = allFileDocs.find(item => item.id === fileId);
    if (!fileDoc) return;

    if (stagedFiles.some(f => f.name === fileDoc.name && f.size === fileDoc.size_bytes)) {
      return;
    }

    const listEl = drawer.querySelector('[data-explorer-file-list]');
    listEl.innerHTML = `<div class="explorer-loading">Lade "${escapeHtml(fileDoc.name)}" ...</div>`;

    try {
      const db = ctx?.db;
      const chunksColl = db?.collection?.('desktop_file_chunks');
      if (!chunksColl) throw new Error('desktop_file_chunks collection not found');

      const docs = await chunksColl.find().exec();
      const allChunks = docs.map(doc => (typeof doc.toJSON === 'function' ? doc.toJSON() : doc));
      const blob = await readStoredFileFromChunks(allChunks, fileId, fileDoc.mime_type || guessMimeType(fileDoc.name), {
        contentGenerationId: fileDoc.content_generation_id || '',
        contentHash: fileDoc.content_hash || '',
        contentHashScheme: fileDoc.content_hash_scheme || '',
      });
      const base64 = await blobToBase64(blob);

      stagedFiles.push({
        name: fileDoc.name,
        type: fileDoc.mime_type || guessMimeType(fileDoc.name),
        size: fileDoc.size_bytes || 0,
        lastModified: fileDoc.updated_at_ms || Date.now(),
        base64: base64,
        source: 'business-os',
        id: fileDoc.id
      });

      renderStagedFiles();
    } catch (err) {
      ctx?.reportFileIntegrityError?.(err, {
        fileId,
        mimeType: fileDoc.mime_type || guessMimeType(fileDoc.name),
        contentState: fileDoc.content_state || '',
        contentGenerationId: fileDoc.content_generation_id || '',
        contentHashScheme: fileDoc.content_hash_scheme || '',
      });
      console.error('Failed to load virtual file chunks:', err);
      alert(`Fehler beim Laden der Datei: ${err.message}`);
    } finally {
      renderExplorerRows();
    }
  }

  const upBtn = drawer.querySelector('[data-explorer-up]');
  if (upBtn) {
    upBtn.addEventListener('click', () => {
      if (currentFolderId === 'fs_root') return;
      const currentFolder = folderDocs.get(currentFolderId);
      currentFolderId = currentFolder?.parent_id || 'fs_root';
      renderExplorerRows();
    });
  }

  loadExplorerFiles().catch(err => console.warn('Failed to load explorer files', err));
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

export function tabularCellsToCompanyRows(matrix, options = {}) {
  const rows = Array.isArray(matrix)
    ? matrix
      .map((row) => (Array.isArray(row) ? row.map((cell) => cleanCell(cell)) : []))
      .filter((row) => row.some(Boolean))
    : [];
  if (!rows.length) return [];
  const header = rows[0].map(normalizeHeader);
  const hasHeader = header.some((name) => COMPANY_HEADER_KEYS.has(name) || DOMAIN_HEADER_KEYS.has(name))
    || isSingleColumnListHeader(rows);
  const dataRows = hasHeader ? rows.slice(1) : rows;
  if (hasHeader && !header.some((name) => COMPANY_HEADER_KEYS.has(name) || DOMAIN_HEADER_KEYS.has(name))) {
    return dataRows
      .map((cells, index) => normalizeCompanyRow({ __rowIndex: index, company: cells[0] || '', raw: cells }, index))
      .filter((row) => row.name);
  }
  if (!hasHeader) {
    return dataRows
      .map((cells, index) => normalizeCompanyRow({ __rowIndex: index, company: cells[0] || '', domain: cells[1] || '', raw: cells }, index))
      .filter((row) => row.name);
  }
  return dataRows
    .map((cells, index) => {
      const row = { __rowIndex: index, raw: cells };
      header.forEach((key, cellIndex) => {
        if (key) row[key] = cells[cellIndex] || '';
      });
      return normalizeCompanyRow(row, index);
    })
    .filter((row) => row.name);
}

export async function extractCompanyRowsFromWorkbookFile(file, options = {}) {
  if (!/\.(xlsx)$/i.test(file?.name || '')) return [];
  const bytes = file.base64
    ? base64ToBytes(file.base64)
    : new Uint8Array(await file.arrayBuffer?.() || []);
  if (!bytes.length) return [];
  const zip = await readZipEntries(bytes);
  const workbookXml = await zipText(zip, 'xl/workbook.xml');
  if (!workbookXml) return [];
  const workbook = parseXml(workbookXml);
  const relsXml = await zipText(zip, 'xl/_rels/workbook.xml.rels');
  const relTargets = workbookRelationshipTargets(relsXml);
  const sheetEntry = selectWorkbookSheet(workbook, relTargets, options.sheet || '');
  if (!sheetEntry?.path) return [];
  const sharedStrings = await readSharedStrings(zip);
  const sheetXml = await zipText(zip, sheetEntry.path);
  const matrix = sheetXmlToMatrix(sheetXml, sharedStrings);
  return tabularCellsToCompanyRows(matrix, options);
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
  let files = [];
  if (sourceType === 'document' || sourceType === 'excel') {
    files = drawer.stagedFiles || [];
  } else {
    const fileInput = drawer.querySelector('[data-import-files]');
    files = await filePayloadFromInput(fileInput);
  }
  if (sourceType === 'text' && !text.trim()) throw new Error('Bitte Text oder Zeilen einfügen.');
  if (sourceType === 'url' && !url) throw new Error('Bitte eine URL angeben.');
  if ((sourceType === 'document' || sourceType === 'table' || sourceType === 'excel') && files.length === 0) throw new Error('Bitte mindestens eine Datei auswählen.');
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
        <span class="importer-field-title" style="display: block; margin-bottom: 6px; color: var(--muted, oklch(0.48 0.015 235)); font-size: 12px; font-weight: 700; text-transform: uppercase;">Dateien auswählen</span>
        <div class="importer-split-layout">
          <!-- Left side: Drag & Drop upload -->
          <div class="importer-drag-drop-zone" data-drag-drop-zone>
            <input type="file" multiple class="importer-hidden-file-input" data-local-file-input accept=".csv,.tsv,.txt,.md,.json,.xlsx,.xls,.docx,.pdf" />
            <div class="drag-drop-content">
              <svg class="upload-icon" viewBox="0 0 24 24" width="28" height="28" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path>
                <polyline points="17 8 12 3 7 8"></polyline>
                <line x1="12" y1="3" x2="12" y2="15"></line>
              </svg>
              <strong>Datei ablegen</strong>
              <span>oder klicken</span>
            </div>
          </div>

          <!-- Right side: Business OS File Explorer Widget -->
          <div class="importer-bos-explorer">
            <header class="explorer-widget-header">
              <button type="button" class="explorer-up-btn" data-explorer-up title="Eine Ebene höher">⌃</button>
              <div class="explorer-path-container">
                <span class="explorer-path-label" data-explorer-path>Files</span>
              </div>
            </header>
            <div class="explorer-file-list" data-explorer-file-list>
              <div class="explorer-loading">Lade Business OS Dateien...</div>
            </div>
          </div>
        </div>

        <!-- Staged files display -->
        <div class="importer-staged-files" data-staged-files-container hidden>
          <span class="staged-title">Ausgewählte Dateien:</span>
          <div class="staged-list" data-staged-files-list></div>
        </div>
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

function isSingleColumnListHeader(rows) {
  if (rows.length < 3) return false;
  const first = normalizeHeader(rows[0]?.find(Boolean) || '');
  if (!first) return false;
  const firstRowValues = rows[0].filter(Boolean);
  const secondRowValues = rows[1]?.filter(Boolean) || [];
  if (firstRowValues.length !== 1 || secondRowValues.length !== 1) return false;
  return ['unternehmen', 'firmen', 'firma', 'company', 'companies', 'accounts', 'personalvermittler', 'liste'].includes(first);
}

function base64ToBytes(base64) {
  try {
    const binary = atob(String(base64 || '').replace(/^data:[^,]+,/, ''));
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
    return bytes;
  } catch {
    return new Uint8Array();
  }
}

async function readZipEntries(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const eocdOffset = findEndOfCentralDirectory(view);
  if (eocdOffset < 0) throw new Error('Excel-Datei konnte nicht gelesen werden: ZIP-Verzeichnis fehlt.');
  const centralDirectoryOffset = view.getUint32(eocdOffset + 16, true);
  const totalEntries = view.getUint16(eocdOffset + 10, true);
  const entries = new Map();
  let cursor = centralDirectoryOffset;
  for (let entryIndex = 0; entryIndex < totalEntries; entryIndex += 1) {
    if (view.getUint32(cursor, true) !== 0x02014b50) break;
    const method = view.getUint16(cursor + 10, true);
    const compressedSize = view.getUint32(cursor + 20, true);
    const uncompressedSize = view.getUint32(cursor + 24, true);
    const nameLength = view.getUint16(cursor + 28, true);
    const extraLength = view.getUint16(cursor + 30, true);
    const commentLength = view.getUint16(cursor + 32, true);
    const localOffset = view.getUint32(cursor + 42, true);
    const name = utf8FromBytes(bytes.slice(cursor + 46, cursor + 46 + nameLength));
    const localNameLength = view.getUint16(localOffset + 26, true);
    const localExtraLength = view.getUint16(localOffset + 28, true);
    const dataOffset = localOffset + 30 + localNameLength + localExtraLength;
    entries.set(name, {
      method,
      compressedSize,
      uncompressedSize,
      data: bytes.slice(dataOffset, dataOffset + compressedSize),
    });
    cursor += 46 + nameLength + extraLength + commentLength;
  }
  return entries;
}

function findEndOfCentralDirectory(view) {
  const minOffset = Math.max(0, view.byteLength - 65557);
  for (let offset = view.byteLength - 22; offset >= minOffset; offset -= 1) {
    if (view.getUint32(offset, true) === 0x06054b50) return offset;
  }
  return -1;
}

async function zipText(entries, path) {
  const entry = entries.get(path.replace(/^\/+/, ''));
  if (!entry) return '';
  const bytes = await unzipEntry(entry);
  return utf8FromBytes(bytes);
}

async function unzipEntry(entry) {
  if (entry.method === 0) return entry.data;
  if (entry.method !== 8) throw new Error('Excel-Datei nutzt ein nicht unterstütztes ZIP-Kompressionsverfahren.');
  if (typeof DecompressionStream !== 'function') {
    throw new Error('Dieser Browser kann Excel-Dateien ohne Hintergrundparser nicht entpacken.');
  }
  const blob = new Blob([entry.data]);
  for (const format of ['deflate-raw', 'deflate']) {
    try {
      const stream = blob.stream().pipeThrough(new DecompressionStream(format));
      const buffer = await new Response(stream).arrayBuffer();
      const bytes = new Uint8Array(buffer);
      if (!entry.uncompressedSize || bytes.length === entry.uncompressedSize) return bytes;
    } catch {
      // Try the next browser-supported deflate wrapper.
    }
  }
  throw new Error('Excel-Datei konnte nicht entpackt werden.');
}

function utf8FromBytes(bytes) {
  return new TextDecoder('utf-8').decode(bytes);
}

function parseXml(text) {
  return new DOMParser().parseFromString(String(text || ''), 'application/xml');
}

function workbookRelationshipTargets(relsXml) {
  const rels = new Map();
  if (!relsXml) return rels;
  const doc = parseXml(relsXml);
  doc.querySelectorAll('Relationship').forEach((rel) => {
    const id = rel.getAttribute('Id') || '';
    const target = rel.getAttribute('Target') || '';
    if (id && target) rels.set(id, normalizeWorkbookTarget(target));
  });
  return rels;
}

function normalizeWorkbookTarget(target) {
  const path = String(target || '').replace(/^\/+/, '');
  return path.startsWith('xl/') ? path : `xl/${path}`;
}

function selectWorkbookSheet(workbook, relTargets, preferredSheet = '') {
  const sheets = Array.from(workbook.querySelectorAll('sheet')).map((sheet) => {
    const relId = sheet.getAttribute('r:id') || sheet.getAttribute('id') || sheet.getAttributeNS('http://schemas.openxmlformats.org/officeDocument/2006/relationships', 'id') || '';
    return {
      name: sheet.getAttribute('name') || '',
      path: relTargets.get(relId) || '',
    };
  });
  if (!sheets.length) return null;
  const preferred = normalizeHeader(preferredSheet);
  return (preferred && sheets.find((sheet) => normalizeHeader(sheet.name) === preferred)) || sheets[0];
}

async function readSharedStrings(zip) {
  const xml = await zipText(zip, 'xl/sharedStrings.xml');
  if (!xml) return [];
  const doc = parseXml(xml);
  return Array.from(doc.querySelectorAll('si')).map((item) => Array.from(item.querySelectorAll('t')).map((node) => node.textContent || '').join(''));
}

function sheetXmlToMatrix(sheetXml, sharedStrings) {
  if (!sheetXml) return [];
  const doc = parseXml(sheetXml);
  const rows = [];
  doc.querySelectorAll('sheetData row').forEach((rowEl) => {
    const cells = [];
    rowEl.querySelectorAll('c').forEach((cellEl) => {
      const ref = cellEl.getAttribute('r') || '';
      const columnIndex = columnIndexFromCellRef(ref);
      cells[columnIndex] = cellValue(cellEl, sharedStrings);
    });
    rows.push(cells.map((cell) => cell || ''));
  });
  return rows;
}

function columnIndexFromCellRef(ref) {
  const letters = String(ref || '').match(/^[A-Z]+/i)?.[0]?.toUpperCase() || 'A';
  let value = 0;
  for (const letter of letters) value = value * 26 + (letter.charCodeAt(0) - 64);
  return Math.max(0, value - 1);
}

function cellValue(cellEl, sharedStrings) {
  const type = cellEl.getAttribute('t') || '';
  if (type === 'inlineStr') {
    return Array.from(cellEl.querySelectorAll('is t')).map((node) => node.textContent || '').join('');
  }
  const raw = cellEl.querySelector('v')?.textContent || '';
  if (type === 's') return sharedStrings[Number(raw)] || '';
  if (type === 'b') return raw === '1' ? 'true' : 'false';
  return raw;
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

async function blobToBase64(blob) {
  return fileToBase64(blob);
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
