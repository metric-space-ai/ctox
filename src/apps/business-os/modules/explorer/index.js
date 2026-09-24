import {
  FILE_CHUNK_HASH_SCHEME,
  FILE_CONTENT_HASH_SCHEME,
  readStoredFileFromDemandChunks,
  sha256Hex,
} from '../../shared/file-integrity.js?v=20260816-browser-sync-guards-v141';


const ROOT_ID = 'fs_root';
const CHUNK_SIZE = 16 * 1024;
const SPREADSHEET_EXTENSIONS = new Set(['csv', 'tsv', 'xlsx', 'json']);
const DOCUMENT_EXTENSIONS = new Set(['docx', 'md', 'markdown', 'txt']);

// UI strings resolved from ctx.locale at mount (shell remounts windows on
// language switch, so one active locale per session is sufficient).
const MESSAGES = {
  de: {
    selectionActions: 'Aktionen für Auswahl',
    recentCreated: 'Zuletzt erstellt',
    recentModified: 'Zuletzt geändert',
    recentCreatedMark: 'NEU',
    recentModifiedMark: 'ZEIT',
    upLevel: 'Eine Ebene höher',
    refresh: 'Aktualisieren',
    path: 'Pfad',
    newFolderCreate: 'Neuen Ordner erstellen',
    newFolder: 'Neuer Ordner',
    uploadFiles: 'Dateien hochladen',
    searchPlaceholder: 'Suchen',
    places: 'Orte',
    view: 'Ansicht',
    sortBy: 'Sortieren',
    sortModified: 'Geändert: neueste',
    sortCreated: 'Erstellt: neueste',
    sortName: 'Name',
    sortKind: 'Art',
    filesAria: 'Dateien',
    infoAria: 'Informationen',
    loadingFiles: 'Lade Dateien...',
    errorPrefix: 'Fehler',
    syncFailedPrefix: 'Synchronisierung fehlgeschlagen',
    collectionUnavailable: (id) => `Collection "${id}" ist nicht verfügbar.`,
    objectCount: (n) => `${n} Objekt${n === 1 ? '' : 'e'}`,
    colName: 'Name',
    colKind: 'Art',
    colModified: 'Geändert',
    colSize: 'Größe',
    open: 'Öffnen',
    previewLabel: 'Vorschau',
    toTrash: 'In Papierkorb',
    readyToDragOut: 'ist zum Herausziehen bereit.',
    dragOutFailedPrefix: 'Datei konnte nicht zum Herausziehen vorbereitet werden',
    noPreview: 'Keine integrierte Vorschau für diesen Dateityp.',
    loadsViaCtox: 'Der Inhalt wird beim Öffnen über CTOX geladen.',
    openInAppFailed: (app) => `Datei konnte nicht in ${app} geöffnet werden`,
    fileFallback: 'Datei',
    chooseFilesFor: (path) => `Wähle Dateien für ${path}.`,
    pickFiles: 'Dateien auswählen',
    cancel: 'Abbrechen',
    noFileSelectedYet: 'Noch keine Datei ausgewählt.',
    selectAtLeastOne: 'Wähle mindestens eine Datei aus.',
    save: 'Speichern',
    rename: 'Umbenennen',
    moveToTrashTitle: 'In Papierkorb verschieben',
    removedFromFolder: (label) => `"${label}" wird aus diesem Ordner entfernt.`,
    loadedCount: (n) => `${n} geladen`,
    noMatches: (q) => `Keine Treffer für "${q}".`,
    dataHiddenForFolder: 'Daten vorhanden, aber für diesen Ordner nicht sichtbar.',
    folderEmpty: 'Dieser Ordner ist leer.',
    noEntries: (kind) => `Keine ${kind}-Einträge verfügbar.`,
    contentNotAvailable: 'Dateiinhalt ist noch nicht über den Sync-Demand-Pfad verfügbar.',
    folder: 'Ordner',
    noSlashes: 'Name darf keine Schrägstriche enthalten.',
    nameExists: 'Name existiert bereits in diesem Ordner.',
    fileReadFailed: 'Datei konnte nicht gelesen werden.',
    openInApp: (app) => `In ${app} öffnen`,
    openInModule: 'Im Modul öffnen',
    noFileSelected: 'Keine Datei ausgewählt.',
    dropFilesHere: 'Dateien hier ablegen',
    visibleCount: (n) => `${n} sichtbar`,
    download: 'Herunterladen',
    showInModule: 'Im Modul anzeigen',
    downloadFailedPrefix: 'Download fehlgeschlagen',
    create: 'Erstellen',
    importLabel: 'Importieren',
  },
  en: {
    selectionActions: 'Actions for selection',
    recentCreated: 'Recently created',
    recentModified: 'Recently modified',
    recentCreatedMark: 'NEW',
    recentModifiedMark: 'TIME',
    upLevel: 'Up one level',
    refresh: 'Refresh',
    path: 'Path',
    newFolderCreate: 'Create new folder',
    newFolder: 'New folder',
    uploadFiles: 'Upload files',
    searchPlaceholder: 'Search',
    places: 'Places',
    view: 'View',
    sortBy: 'Sort',
    sortModified: 'Modified: newest',
    sortCreated: 'Created: newest',
    sortName: 'Name',
    sortKind: 'Kind',
    filesAria: 'Files',
    infoAria: 'Info',
    loadingFiles: 'Loading files...',
    errorPrefix: 'Error',
    syncFailedPrefix: 'Sync failed',
    collectionUnavailable: (id) => `Collection "${id}" is not available.`,
    objectCount: (n) => `${n} item${n === 1 ? '' : 's'}`,
    colName: 'Name',
    colKind: 'Kind',
    colModified: 'Modified',
    colSize: 'Size',
    open: 'Open',
    previewLabel: 'Preview',
    toTrash: 'Move to trash',
    readyToDragOut: 'is ready to drag out.',
    dragOutFailedPrefix: 'File could not be prepared for dragging out',
    noPreview: 'No built-in preview for this file type.',
    loadsViaCtox: 'Content loads through CTOX when opened.',
    openInAppFailed: (app) => `File could not be opened in ${app}`,
    fileFallback: 'File',
    chooseFilesFor: (path) => `Choose files for ${path}.`,
    pickFiles: 'Choose files',
    cancel: 'Cancel',
    noFileSelectedYet: 'No files selected yet.',
    selectAtLeastOne: 'Select at least one file.',
    save: 'Save',
    rename: 'Rename',
    moveToTrashTitle: 'Move to trash',
    removedFromFolder: (label) => `"${label}" will be removed from this folder.`,
    loadedCount: (n) => `${n} loaded`,
    noMatches: (q) => `No matches for "${q}".`,
    dataHiddenForFolder: 'Data exists but is not visible in this folder.',
    folderEmpty: 'This folder is empty.',
    noEntries: (kind) => `No ${kind} entries available.`,
    contentNotAvailable: 'File content is not yet available over the sync demand path.',
    folder: 'Folder',
    noSlashes: 'Name must not contain slashes.',
    nameExists: 'Name already exists in this folder.',
    fileReadFailed: 'File could not be read.',
    openInApp: (app) => `Open in ${app}`,
    openInModule: 'Open in module',
    noFileSelected: 'No file selected.',
    dropFilesHere: 'Drop files here',
    visibleCount: (n) => `${n} visible`,
    download: 'Download',
    showInModule: 'Show in module',
    downloadFailedPrefix: 'Download failed',
    create: 'Create',
    importLabel: 'Import',
  },
};
let T = MESSAGES.de;

const FILE_SOURCE = { id: 'desktop_files', label: 'Files', section: 'On this Desktop', mark: 'FS', moduleId: null, kind: 'File System', filesystem: true, fileCollection: true };
const RECENT_CREATED_SOURCE = { id: 'recent_created', collectionId: 'desktop_files', get label() { return T.recentCreated; }, section: 'On this Desktop', get mark() { return T.recentCreatedMark; }, moduleId: null, kind: 'File', fileCollection: true, recentSort: 'created' };
const RECENT_MODIFIED_SOURCE = { id: 'recent_modified', collectionId: 'desktop_files', get label() { return T.recentModified; }, section: 'On this Desktop', get mark() { return T.recentModifiedMark; }, moduleId: null, kind: 'File', fileCollection: true, recentSort: 'modified' };
const SOURCES = [
  FILE_SOURCE,
  RECENT_CREATED_SOURCE,
  RECENT_MODIFIED_SOURCE,
  { id: 'documents', label: 'Documents', section: 'Business OS', mark: 'DOC', moduleId: 'documents', kind: 'Document' },
  { id: 'spreadsheets', label: 'Spreadsheets', section: 'Business OS', mark: 'XLS', moduleId: 'spreadsheets', kind: 'Spreadsheet' },
  { id: 'knowledge_items', label: 'Knowledge', section: 'Business OS', mark: 'KNO', moduleId: 'knowledge', kind: 'Knowledge' },
  { id: 'matching_objects', label: 'Matching Objects', section: 'Business OS', mark: 'MAT', moduleId: 'matching', kind: 'Object' },
  { id: 'outbound_companies', label: 'Outbound', section: 'Business OS', mark: 'OUT', moduleId: 'outbound', kind: 'Company' },
];

const SORTERS = {
  name: (a, b) => labelFor(a).localeCompare(labelFor(b), undefined, { sensitivity: 'base' }),
  kind: (a, b) => kindFor(a).localeCompare(kindFor(b), undefined, { sensitivity: 'base' }),
  modified: (a, b) => timestampFor(b) - timestampFor(a),
  created: (a, b) => Number(b.raw?.created_at_ms || 0) - Number(a.raw?.created_at_ms || 0),
};

export async function mount(ctx) {
  const container = ctx.host;
  ensureModuleStyles();
  T = MESSAGES[ctx?.locale === 'en' ? 'en' : 'de'];
  const state = {
    activeSource: FILE_SOURCE,
    currentFolderId: ROOT_ID,
    folderDocs: new Map(),
    query: '',
    sort: 'modified',
    selectedId: '',
    rows: [],
    previewUrl: '',
    dragExports: new Map(),
    lastLoad: null,
    searchTimer: null,
  };

  container.innerHTML = await loadModuleMarkup();
  applyStaticMarkupLabels(container);

  const refs = {
    root: container.querySelector('[data-explorer-root]'),
    sources: container.querySelector('[data-explorer-sources]'),
    search: container.querySelector('[data-explorer-search]'),
    path: container.querySelector('[data-explorer-path]'),
    title: container.querySelector('[data-explorer-title]'),
    count: container.querySelector('[data-explorer-count]'),
    table: container.querySelector('[data-explorer-table]'),
    status: container.querySelector('[data-explorer-status]'),
    preview: container.querySelector('[data-explorer-preview]'),
    up: container.querySelector('[data-explorer-up]'),
    refresh: container.querySelector('[data-explorer-refresh]'),
    newFolder: container.querySelector('[data-explorer-new-folder]'),
    upload: container.querySelector('[data-explorer-upload]'),
    selectionActions: container.querySelector('[data-explorer-selection-actions]'),
    fileInput: container.querySelector('[data-explorer-file-input]'),
    sort: container.querySelector('[data-explorer-sort]'),
  };

  renderSidebar();
  refs.search.addEventListener('input', () => {
    state.query = refs.search.value.trim();
    renderRows();
    if (!isFileCollectionSource()) return;
    if (state.searchTimer) window.clearTimeout(state.searchTimer);
    state.searchTimer = window.setTimeout(() => {
      state.searchTimer = null;
      void loadRows();
    }, 220);
  });
  refs.sort.addEventListener('change', () => {
    state.sort = refs.sort.value || 'modified';
    renderRows();
  });
  refs.up.addEventListener('click', goUp);
  refs.refresh.addEventListener('click', loadRows);
  refs.newFolder.addEventListener('click', promptCreateFolder);
  refs.upload.addEventListener('click', openUploadDialog);
  refs.selectionActions.addEventListener('click', (event) => {
    const row = filteredRows().find((entry) => entry.id === state.selectedId);
    if (!row) return;
    const bounds = refs.selectionActions.getBoundingClientRect();
    showRowActions(row, {
      clientX: bounds.left, clientY: bounds.bottom,
      preventDefault: () => event.preventDefault(),
      stopPropagation: () => event.stopPropagation(),
    });
  });
  refs.root.addEventListener('dragover', (event) => {
    if (!canAcceptFileDrop() || !dataTransferContainsFiles(event.dataTransfer)) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy';
    refs.root.classList.add('is-dragging-files');
  });
  refs.root.addEventListener('dragleave', () => refs.root.classList.remove('is-dragging-files'));
  refs.root.addEventListener('drop', async (event) => {
    if (!canAcceptFileDrop() || !dataTransferContainsFiles(event.dataTransfer)) return;
    event.preventDefault();
    event.stopPropagation();
    refs.root.classList.remove('is-dragging-files');
    await uploadFiles(event.dataTransfer?.files);
  });

  let disposed = false;
  refs.table.replaceChildren(message(T.loadingFiles));
  refs.preview.innerHTML = emptyPreview();
  renderHeader();
  Promise.resolve()
    .then(async () => {
      const fileBridge = await ctx.sync?.startCollection?.('desktop_files');
      if (fileBridge) await waitForReplicationBridge(fileBridge, 'desktop_files');
      await ensureFileSystem(ctx.db);
      if (disposed) return;
      await selectSource(FILE_SOURCE);
    })
    .catch((error) => {
      if (disposed) return;
      console.error('[explorer] background initialization failed:', error);
      state.lastLoad = {
        ok: false,
        reason: 'load_error',
        total: 0,
        visible: 0,
        message: `${T.errorPrefix}: ${error?.message || error}`,
      };
      renderHeader();
      renderRows();
    });

  async function selectSource(source) {
    state.activeSource = source;
    state.selectedId = '';
    refs.search.value = '';
    state.query = '';
    state.sort = source.recentSort || state.sort;
    refs.sort.value = state.sort;
    refs.root.classList.toggle('is-filesystem', Boolean(source.filesystem));
    renderSidebar();
    if (source.moduleId) await ctx.ensureModuleData?.(source.moduleId);
    await loadRows();
  }

  async function loadRows() {
    refs.selectionActions.disabled = true;
    refs.table.replaceChildren(message(T.loadingFiles));
    refs.preview.innerHTML = emptyPreview();
    revokePreviewUrl();
    try {
      const collectionId = sourceCollectionId(state.activeSource);
      const bridge = await ctx.sync?.startCollection?.(collectionId);
      if (bridge) await waitForReplicationBridge(bridge, collectionId);
    } catch (error) {
      state.rows = [];
      state.lastLoad = {
        ok: false,
        reason: 'sync_error',
        total: 0,
        visible: 0,
        message: `${T.syncFailedPrefix}: ${error?.message || error}`,
      };
      renderHeader();
      renderRows();
      return;
    }
    const collectionId = sourceCollectionId(state.activeSource);
    const collection = ctx.db?.collection?.(collectionId);
    if (!collection) {
      state.rows = [];
      state.lastLoad = {
        ok: false,
        reason: 'missing_collection',
        total: 0,
        visible: 0,
        message: T.collectionUnavailable(collectionId),
      };
      renderHeader();
      renderRows();
      return;
    }
    try {
      const docs = await collection.find(activeDocumentQueryForSource(state.activeSource, state.query)).exec();
      if (disposed) return;
      const data = docs.map((doc) => (typeof doc.toJSON === 'function' ? doc.toJSON() : doc));
      const activeData = data.filter((item) => !item.is_deleted);
      if (isFileCollectionSource()) {
        state.folderDocs = new Map(activeData.filter((item) => item.kind === 'folder').map((item) => [item.id, item]));
      } else {
        state.folderDocs = new Map();
      }
      state.rows = state.activeSource.recentSort || (isFilesystemSource() && state.query)
        ? activeData.filter((item) => item.kind !== 'folder').map(normalizeFileRow)
        : normalizeRowsForSource(data, state.activeSource, state.currentFolderId);
      state.lastLoad = {
        ok: true,
        reason: '',
        total: activeData.length,
        visible: state.rows.length,
        message: '',
      };
      renderHeader();
      renderRows();
    } catch (error) {
      if (disposed) return;
      console.error('[explorer] render failed:', error);
      state.rows = [];
      state.lastLoad = {
        ok: false,
        reason: 'load_error',
        total: 0,
        visible: 0,
        message: `${T.errorPrefix}: ${error?.message || error}`,
      };
      renderHeader();
      renderRows();
    }
  }

  function renderSidebar() {
    refs.sources.innerHTML = '';
    const bySection = new Map();
    for (const source of SOURCES) {
      if (!bySection.has(source.section)) bySection.set(source.section, []);
      bySection.get(source.section).push(source);
    }
    for (const [section, items] of bySection.entries()) {
      const group = document.createElement('section');
      group.className = 'app-explorer-sidebar-group';
      group.innerHTML = `<h3>${escapeHtml(section)}</h3>`;
      for (const source of items) {
        const button = document.createElement('button');
        button.type = 'button';
        button.className = 'app-explorer-source';
        button.classList.toggle('is-active', state.activeSource.id === source.id);
        button.setAttribute('aria-pressed', state.activeSource.id === source.id ? 'true' : 'false');
        button.innerHTML = `
          <span class="app-explorer-source-mark">${escapeHtml(source.mark)}</span>
          <span>${escapeHtml(source.label)}</span>
        `;
        button.addEventListener('click', () => selectSource(source));
        group.append(button);
      }
      refs.sources.append(group);
    }
  }

  function renderHeader() {
    const folder = currentFolder();
    const label = isFilesystemSource() ? (folder?.path || '/').replace(/^\//, 'Files / ') : state.activeSource.label;
    refs.path.textContent = label === 'Files / ' ? 'Files' : label;
    refs.title.textContent = isFilesystemSource() ? folder?.name || 'Files' : state.activeSource.label;
    refs.up.disabled = !isFilesystemSource() || state.currentFolderId === ROOT_ID;
    refs.newFolder.hidden = !isFilesystemSource();
    refs.upload.hidden = !canAcceptFileDrop();
    refs.refresh.setAttribute('aria-label', `${T.refresh}: ${state.activeSource.label}`);
  }

  function renderRows() {
    refs.selectionActions.disabled = true;
    const rows = filteredRows();
    refs.count.textContent = T.objectCount(rows.length);
    if (state.lastLoad && !state.lastLoad.ok) {
      refs.table.replaceChildren(message(state.lastLoad.message, 'error'));
      refs.preview.innerHTML = emptyPreview(state.lastLoad.message);
      renderFooter(rows);
      return;
    }
    if (!rows.length) {
      refs.table.replaceChildren(message(emptyStateText()));
      refs.preview.innerHTML = emptyPreview(emptyStateText());
      renderFooter(rows);
      return;
    }

    const table = document.createElement('div');
    table.className = 'app-explorer-grid';
    table.setAttribute('role', 'grid');
    table.innerHTML = `
      <div class="app-explorer-grid-header" role="row">
        <button class="app-explorer-grid-head app-explorer-grid-name" type="button" data-sort="name" role="columnheader">${T.colName}</button>
        <button class="app-explorer-grid-head" type="button" data-sort="kind" role="columnheader">${T.colKind}</button>
        <button class="app-explorer-grid-head" type="button" data-sort="modified" role="columnheader">${T.colModified}</button>
        <div class="app-explorer-grid-head" role="columnheader">${T.colSize}</div>
      </div>
    `;
    for (const row of rows) table.append(rowNode(row));
    table.querySelectorAll('[data-sort]').forEach((button) => {
      button.classList.toggle('is-active', button.dataset.sort === state.sort);
      button.setAttribute('aria-sort', button.dataset.sort === state.sort ? 'descending' : 'none');
      button.addEventListener('click', () => {
        state.sort = button.dataset.sort || 'modified';
        refs.sort.value = state.sort;
        renderRows();
      });
    });
    refs.table.replaceChildren(table);
    const selected = rows.find((row) => row.id === state.selectedId) || rows[0];
    selectRow(selected);
    renderFooter(rows);
  }

  function filteredRows() {
    const query = state.query.toLowerCase();
    const rows = query
      ? state.rows.filter((row) => `${row.label} ${row.kind} ${row.status} ${row.path || ''} ${row.localPath || ''}`.toLowerCase().includes(query))
      : state.rows;
    return [...rows].sort(SORTERS[state.sort] || SORTERS.modified);
  }

  function rowNode(row) {
    const item = document.createElement('button');
    item.type = 'button';
    item.className = 'app-explorer-row';
    item.dataset.id = row.id;
    item.dataset.contextRecordId = row.id;
    item.dataset.contextRecordType = row.isFolder ? 'folder' : row.sourceId;
    item.dataset.contextLabel = row.label;
    item.dataset.contextCollection = row.sourceId;
    item.setAttribute('aria-label', `${row.label}, ${row.kind}`);
    item.innerHTML = `
      <span class="app-explorer-file">
        <span class="app-explorer-file-icon" data-kind="${escapeHtml(row.iconKind)}">${escapeHtml(row.mark)}</span>
        <span class="app-explorer-file-name">${escapeHtml(row.label)}</span>
      </span>
      <span>${escapeHtml(row.kind)}</span>
      <span>${escapeHtml(row.modified)}</span>
      <span>${escapeHtml(row.sizeLabel || row.status || '')}</span>
    `;
    item.addEventListener('click', () => selectRow(row));
    item.addEventListener('dblclick', () => openRow(row));
    if (row.sourceId === FILE_SOURCE.id && !row.isFolder) {
      item.draggable = true;
      item.title = `${row.label} ziehen oder doppelklicken`;
      const prepare = () => {
        void prepareDragExport(row).catch(() => undefined);
      };
      item.addEventListener('pointerenter', prepare);
      item.addEventListener('focus', prepare);
      item.addEventListener('mousedown', prepare);
      item.addEventListener('dragstart', (event) => startFileDrag(event, row));
      item.addEventListener('dragend', () => item.classList.remove('is-dragging-out'));
    }
    item.addEventListener('keydown', (event) => {
      if (event.key === 'Enter') {
        event.preventDefault();
        openRow(row);
      }
    });
    return item;
  }

  function showRowActions(row, event) {
    if (!ctx.contextMenu) return;
    const actions = [
      { label: row.isFolder ? T.open : T.previewLabel, icon: '↗', action: () => openRow(row) },
    ];
    if (row.sourceId === FILE_SOURCE.id) {
      actions.push(
        { type: 'separator' },
        ...(!row.isFolder ? [{ label: T.download, icon: '↓', action: () => downloadRow(row) }] : []),
        { label: T.rename, icon: '✎', action: () => renameFileRow(row) },
        { label: T.toTrash, icon: '⌫', action: () => trashFileRow(row) }
      );
    } else {
      actions.push(
        { type: 'separator' },
        { label: T.showInModule, icon: '⌁', action: () => openRow(row) }
      );
    }
    ctx.contextMenu.show(event, actions);
  }

  function selectRow(row) {
    if (!row) return;
    state.selectedId = row.id;
    refs.selectionActions.disabled = !ctx.contextMenu;
    refs.table.querySelectorAll('.app-explorer-row').forEach((node) => {
      node.classList.toggle('is-selected', node.dataset.id === row.id);
      node.setAttribute('aria-selected', node.dataset.id === row.id ? 'true' : 'false');
    });
    renderPreview(row);
    if (row.sourceId === FILE_SOURCE.id && !row.isFolder) {
      void prepareDragExport(row).catch(() => undefined);
    }
  }

  async function prepareDragExport(row) {
    const existing = state.dragExports.get(row.id);
    if (existing?.url) return existing;
    if (existing?.promise) return existing.promise;
    const promise = (async () => {
      const mimeType = normalizedMimeType(row);
      const blob = await readStoredFile(ctx, row.id, mimeType, row);
      const bytes = new Uint8Array(await blob.arrayBuffer());
      const entry = {
        blob,
        bytes,
        mimeType,
        name: safeDownloadName(row.label),
        url: URL.createObjectURL(blob),
      };
      state.dragExports.set(row.id, entry);
      return entry;
    })().catch((error) => {
      state.dragExports.delete(row.id);
      throw error;
    });
    state.dragExports.set(row.id, { promise });
    return promise;
  }

  function startFileDrag(event, row) {
    const prepared = state.dragExports.get(row.id);
    if (!prepared?.url) {
      event.preventDefault();
      void prepareDragExport(row)
        .then(() => ctx.notifications?.info?.(`${row.label} ${T.readyToDragOut}`))
        .catch((error) => renderDragError(row, error));
      return;
    }
    const desktopBridge = globalThis.ctoxBusinessOsDesktop;
    if (typeof desktopBridge?.startFileDrag === 'function') {
      event.preventDefault();
      desktopBridge.startFileDrag({
        name: prepared.name,
        mimeType: prepared.mimeType,
        bytes: prepared.bytes,
      });
    } else if (event.dataTransfer) {
      event.dataTransfer.effectAllowed = 'copy';
      event.dataTransfer.setData('DownloadURL', `${prepared.mimeType}:${prepared.name}:${prepared.url}`);
      event.dataTransfer.setData('text/uri-list', prepared.url);
      event.dataTransfer.setData('text/plain', prepared.name);
    }
    event.currentTarget?.classList.add('is-dragging-out');
  }

  function renderDragError(row, error) {
    ctx.reportFileIntegrityError?.(error, {
      fileId: row.id,
      mimeType: row.mimeType,
      contentState: row.contentState,
      contentGenerationId: row.contentGenerationId,
      contentHashScheme: row.contentHashScheme,
      operation: 'drag_export',
    });
    const body = refs.preview.querySelector('[data-preview-body]');
    if (body) {
      body.innerHTML = `<p class="app-explorer-message is-error">${escapeHtml(T.dragOutFailedPrefix)}: ${escapeHtml(error?.message || error)}</p>`;
    }
  }

  function renderPreview(row) {
    revokePreviewUrl();
    refs.preview.innerHTML = `
      <div class="app-explorer-preview-card">
        <span class="app-explorer-preview-icon">${escapeHtml(row.mark)}</span>
        <strong>${escapeHtml(row.label)}</strong>
        <small>${escapeHtml(row.kind)}</small>
      </div>
      <div data-preview-body></div>
      <dl>
        <dt>Ort</dt><dd>${escapeHtml(row.path || state.activeSource.label)}</dd>
        <dt>${T.colSize}</dt><dd>${escapeHtml(row.sizeLabel || '-')}</dd>
        <dt>${T.colModified}</dt><dd>${escapeHtml(row.modified || '-')}</dd>
        <dt>ID</dt><dd>${escapeHtml(row.id)}</dd>
      </dl>
      <button type="button" data-preview-open>${escapeHtml(openLabelFor(row))}</button>
      ${row.sourceId === FILE_SOURCE.id && !row.isFolder ? '<button type="button" data-preview-download>Herunterladen</button>' : ''}
    `;
    refs.preview.querySelector('[data-preview-open]')?.addEventListener('click', () => openRow(row));
    refs.preview.querySelector('[data-preview-download]')?.addEventListener('click', () => downloadRow(row));
    if (row.sourceId === FILE_SOURCE.id && !row.isFolder) renderStoredFilePreview(row);
  }

  async function renderStoredFilePreview(row) {
    const body = refs.preview.querySelector('[data-preview-body]');
    if (!body) return;
    if (!isPreviewable(row)) {
      body.innerHTML = `<p class="app-explorer-preview-empty">${escapeHtml(T.noPreview)}</p>`;
      return;
    }
    if (row.contentState === 'lazy' || row.contentState === 'missing') {
      body.innerHTML = `<p class="app-explorer-preview-empty">${escapeHtml(T.loadsViaCtox)}</p>`;
      return;
    }
    try {
      const blob = await readStoredFile(ctx, row.id, row.mimeType, row);
      if (state.selectedId !== row.id) return;
      state.previewUrl = URL.createObjectURL(blob);
      if (row.mimeType.startsWith('image/')) {
        body.innerHTML = `<img class="app-explorer-image-preview" src="${state.previewUrl}" alt="">`;
      } else {
        body.innerHTML = '<pre class="app-explorer-text-preview" data-text-preview></pre>';
        const text = await blob.text();
        const pre = body.querySelector('[data-text-preview]');
        if (pre) pre.textContent = text.slice(0, 12000);
      }
    } catch (error) {
      ctx.reportFileIntegrityError?.(error, {
        fileId: row.id,
        mimeType: row.mimeType,
        contentState: row.contentState,
        contentGenerationId: row.contentGenerationId,
        contentHashScheme: row.contentHashScheme,
      });
      body.innerHTML = `<p class="app-explorer-message is-error">Vorschau konnte nicht geladen werden: ${escapeHtml(error?.message || error)}</p>`;
    }
  }

  async function openRow(row) {
    if (row.sourceId === FILE_SOURCE.id) {
      if (row.isFolder) {
        state.currentFolderId = row.id;
        state.selectedId = '';
        await loadRows();
        return;
      }
      const associatedApp = associatedAppFor(row);
      if (associatedApp && typeof ctx.openDesktopApp === 'function') {
        try {
          const blob = await readStoredFile(ctx, row.id, normalizedMimeType(row), row);
          const file = new File([blob], row.label, {
            type: normalizedMimeType(row),
            lastModified: timestampFor(row.raw) || Date.now(),
          });
          await ctx.openDesktopApp(associatedApp, {
            args: {
              openFile: {
                file,
                sourceFileId: row.id,
                sourcePath: row.localPath || row.path,
              },
            },
          });
          return;
        } catch (error) {
          renderOpenError(row, associatedApp, error);
          return;
        }
      }
      if (typeof ctx.openDesktopApp === 'function') {
        ctx.openDesktopApp('file-viewer', {
          title: row.label,
          args: {
            fileId: row.id,
            name: row.label,
            mimeType: row.mimeType,
            sizeBytes: row.sizeBytes,
            path: row.localPath || row.path,
            source: row.source,
            contentState: row.contentState,
            contentHash: row.contentHash,
            contentHashScheme: row.contentHashScheme,
            contentGenerationId: row.contentGenerationId,
          },
        });
        return;
      }
      let blob;
      try {
        blob = await readStoredFile(ctx, row.id, row.mimeType, row);
      } catch (error) {
        ctx.reportFileIntegrityError?.(error, {
          fileId: row.id,
          mimeType: row.mimeType,
          contentState: row.contentState,
          contentGenerationId: row.contentGenerationId,
          contentHashScheme: row.contentHashScheme,
        });
        throw error;
      }
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement('a');
      anchor.href = url;
      anchor.download = row.label;
      anchor.rel = 'noopener';
      anchor.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
      return;
    }
    if (row?.moduleId) location.hash = `#${encodeURIComponent(row.moduleId)}?record=${encodeURIComponent(row.id)}`;
  }

  function renderOpenError(row, appId, error) {
    ctx.reportFileIntegrityError?.(error, {
      fileId: row.id,
      mimeType: row.mimeType,
      contentState: row.contentState,
      contentGenerationId: row.contentGenerationId,
      contentHashScheme: row.contentHashScheme,
      targetApp: appId,
    });
    const body = refs.preview.querySelector('[data-preview-body]');
    if (body) {
      body.innerHTML = `<p class="app-explorer-message is-error">${escapeHtml(T.openInAppFailed(appTitle(appId)))}: ${escapeHtml(error?.message || error)}</p>`;
    }
  }

  async function downloadRow(row) {
    if (row.sourceId !== FILE_SOURCE.id || row.isFolder) return;
    try {
      const blob = await readStoredFile(ctx, row.id, row.mimeType, row);
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement('a');
      anchor.href = url;
      anchor.download = row.label;
      anchor.rel = 'noopener';
      anchor.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (error) {
      ctx.reportFileIntegrityError?.(error, {
        fileId: row.id,
        mimeType: row.mimeType,
        contentState: row.contentState,
        contentGenerationId: row.contentGenerationId,
        contentHashScheme: row.contentHashScheme,
      });
      const body = refs.preview.querySelector('[data-preview-body]');
      if (body) {
        body.innerHTML = `<p class="app-explorer-message is-error">${escapeHtml(T.downloadFailedPrefix)}: ${escapeHtml(error?.message || error)}</p>`;
      }
    }
  }

  async function goUp() {
    if (!isFilesystemSource() || state.currentFolderId === ROOT_ID) return;
    const folder = currentFolder();
    state.currentFolderId = folder?.parent_id || ROOT_ID;
    state.selectedId = '';
    await loadRows();
  }

  async function createFolder() {
    const name = await askName(container, T.newFolder, '', {
      submitLabel: T.create,
      existingNames: state.rows.map((row) => row.label),
    });
    if (!name) return;
    await persistFolder(name);
  }

  async function promptCreateFolder() {
    await createFolder();
  }

  async function persistFolder(name) {
    if (!isFilesystemSource()) return;
    const files = ctx.db?.collection?.('desktop_files');
    if (!files) return;
    const now = Date.now();
    const parent = currentFolder();
    const path = joinPath(parent?.path || '/', name);
    await files.upsert({
      id: `folder_${now}_${Math.random().toString(36).slice(2, 8)}`,
      parent_id: state.currentFolderId,
      path,
      name,
      kind: 'folder',
      mime_type: '',
      extension: '',
      size_bytes: 0,
      source: 'user',
      sort_index: now,
      is_deleted: false,
      created_at_ms: now,
      updated_at_ms: now,
    });
    await loadRows();
  }

  async function uploadFiles(fileList) {
    if (!canAcceptFileDrop() || !fileList?.length) return;
    const targetFolder = isFilesystemSource() ? currentFolder() : state.folderDocs.get(ROOT_ID);
    const targetFolderId = isFilesystemSource() ? state.currentFolderId : ROOT_ID;
    const targetPath = targetFolder?.path || '/';
    const filesCollection = ctx.db?.collection?.('desktop_files');
    const existingDocs = await filesCollection?.find({}).exec?.() || [];
    const existingNames = existingDocs
      .map((doc) => (typeof doc.toJSON === 'function' ? doc.toJSON() : doc))
      .filter((entry) => !entry.is_deleted && entry.parent_id === targetFolderId)
      .map((entry) => entry.name);
    for (const file of [...fileList]) {
      const name = uniqueName(file.name || T.fileFallback, existingNames);
      existingNames.push(name);
      await storeFile(ctx.db, targetFolderId, targetPath, name, file);
    }
    if (!isFilesystemSource()) {
      state.activeSource = FILE_SOURCE;
      state.currentFolderId = targetFolderId;
      state.selectedId = '';
      state.query = '';
      refs.search.value = '';
      refs.root.classList.add('is-filesystem');
      renderSidebar();
    }
    await loadRows();
  }

  async function openUploadDialog() {
    if (!isFilesystemSource()) return;
    const overlay = document.createElement('div');
    overlay.className = 'app-explorer-upload-dialog';
    overlay.innerHTML = `
      <form role="dialog" aria-modal="true" aria-label="${T.uploadFiles}">
        <strong>${T.uploadFiles}</strong>
        <p>${escapeHtml(T.chooseFilesFor(currentFolder()?.path || '/'))}</p>
        <button type="button" class="app-explorer-dropzone" data-pick-files>${T.pickFiles}</button>
        <ul data-upload-list></ul>
        <div class="app-explorer-dialog-error" data-upload-error role="alert"></div>
        <div class="app-explorer-dialog-actions">
          <button type="button" data-cancel>${T.cancel}</button>
          <button type="submit" data-submit disabled>${T.importLabel}</button>
        </div>
      </form>
    `;
    container.append(overlay);
    const selected = [];
    const list = overlay.querySelector('[data-upload-list]');
    const submit = overlay.querySelector('[data-submit]');
    const error = overlay.querySelector('[data-upload-error]');
    const close = () => {
      if (refs.fileInput.onchange) refs.fileInput.onchange = null;
      overlay.remove();
    };
    const renderSelection = () => {
      if (!list || !submit) return;
      list.replaceChildren(...selected.map((file) => {
        const item = document.createElement('li');
        item.textContent = `${file.name || T.fileFallback} · ${formatBytes(file.size || 0)}`;
        return item;
      }));
      submit.disabled = selected.length === 0;
      if (error) error.textContent = selected.length ? '' : T.noFileSelectedYet;
    };
    overlay.querySelector('[data-pick-files]')?.addEventListener('click', () => refs.fileInput.click());
    overlay.querySelector('[data-cancel]')?.addEventListener('click', close);
    overlay.addEventListener('dragover', (event) => {
      event.preventDefault();
      overlay.classList.add('is-dragging-files');
    });
    overlay.addEventListener('dragleave', () => overlay.classList.remove('is-dragging-files'));
    overlay.addEventListener('drop', (event) => {
      event.preventDefault();
      overlay.classList.remove('is-dragging-files');
      selected.splice(0, selected.length, ...(event.dataTransfer?.files ? [...event.dataTransfer.files] : []));
      renderSelection();
    });
    refs.fileInput.onchange = () => {
      selected.splice(0, selected.length, ...(refs.fileInput.files ? [...refs.fileInput.files] : []));
      refs.fileInput.value = '';
      renderSelection();
    };
    overlay.querySelector('form')?.addEventListener('submit', async (event) => {
      event.preventDefault();
      if (!selected.length) {
        if (error) error.textContent = T.selectAtLeastOne;
        return;
      }
      if (submit) submit.disabled = true;
      await uploadFiles(selected);
      close();
    });
    renderSelection();
  }

  async function renameFileRow(row) {
    const nextName = await askName(container, T.rename, row.label, {
      submitLabel: T.save,
      existingNames: state.rows.filter((item) => item.id !== row.id).map((item) => item.label),
    });
    if (!nextName || nextName === row.label) return;
    const files = ctx.db?.collection?.('desktop_files');
    const doc = await files?.findOne(row.id).exec();
    if (!doc) return;
    const parent = currentFolder();
    await doc.incrementalPatch({
      name: nextName,
      path: joinPath(parent?.path || '/', nextName),
      updated_at_ms: Date.now(),
    });
    await loadRows();
  }

  async function trashFileRow(row) {
    const confirmed = await confirmAction(container, T.moveToTrashTitle, T.removedFromFolder(row.label));
    if (!confirmed) return;
    const files = ctx.db?.collection?.('desktop_files');
    const doc = await files?.findOne(row.id).exec();
    await doc?.incrementalPatch({ is_deleted: true, updated_at_ms: Date.now() });
    await loadRows();
  }

  function currentFolder() {
    return state.folderDocs.get(state.currentFolderId) || { id: ROOT_ID, parent_id: '', path: '/', name: 'Files' };
  }

  function isFilesystemSource() {
    return state.activeSource.filesystem === true;
  }

  function canAcceptFileDrop() {
    return isFilesystemSource() || Boolean(state.activeSource.recentSort);
  }

  function isFileCollectionSource() {
    return state.activeSource.fileCollection === true;
  }

  function sourceCollectionId(source) {
    return source.collectionId || source.id;
  }

  function activeDocumentQueryForSource(source, searchQuery = '') {
    const query = String(searchQuery || '').trim();
    if (!source?.fileCollection || !query) return {};
    const pattern = escapeRegex(query);
    return {
      selector: {
        $or: [
          { name: { $regex: pattern } },
          { path: { $regex: pattern } },
          { virtual_path: { $regex: pattern } },
        ],
      },
      sort: [{ updated_at_ms: 'desc' }],
      limit: 250,
      requireRevision: `files-search:${query.toLocaleLowerCase()}`,
    };
  }

  function renderFooter(rows = filteredRows()) {
    const sourceLabel = isFilesystemSource() ? (currentFolder()?.path || '/') : state.activeSource.label;
    const sourceState = state.lastLoad?.ok === false ? T.errorPrefix : T.loadedCount(state.lastLoad?.total ?? rows.length);
    refs.status.textContent = `${T.visibleCount(rows.length)} · ${sourceState} · ${sourceLabel}`;
  }

  function revokePreviewUrl() {
    if (!state.previewUrl) return;
    URL.revokeObjectURL(state.previewUrl);
    state.previewUrl = '';
  }

  function emptyStateText() {
    if (state.query) return T.noMatches(state.query);
    if (state.lastLoad?.ok && state.lastLoad.total > 0 && state.lastLoad.visible === 0) {
      return T.dataHiddenForFolder;
    }
    return isFilesystemSource()
      ? T.folderEmpty
      : T.noEntries(state.activeSource.kind);
  }

  return () => {
    disposed = true;
    if (state.searchTimer) window.clearTimeout(state.searchTimer);
    revokePreviewUrl();
    for (const entry of state.dragExports.values()) {
      if (entry?.url) URL.revokeObjectURL(entry.url);
    }
    state.dragExports.clear();
    container.replaceChildren();
  };
}

function escapeRegex(value) {
  return String(value || '').replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

async function ensureFileSystem(db) {
  const files = db?.collection?.('desktop_files');
  if (!files) return;
  const now = Date.now();
  const seeds = [
    { id: ROOT_ID, parent_id: '', path: '/', name: 'Files', kind: 'folder', sort_index: 0 },
    { id: 'fs_desktop', parent_id: ROOT_ID, path: '/Desktop', name: 'Desktop', kind: 'folder', sort_index: 10 },
    { id: 'fs_documents', parent_id: ROOT_ID, path: '/Documents', name: 'Documents', kind: 'folder', sort_index: 20 },
    { id: 'fs_spreadsheets', parent_id: ROOT_ID, path: '/Spreadsheets', name: 'Spreadsheets', kind: 'folder', sort_index: 25 },
    { id: 'fs_downloads', parent_id: ROOT_ID, path: '/Downloads', name: 'Downloads', kind: 'folder', sort_index: 30 },
    { id: 'fs_ctox', parent_id: ROOT_ID, path: '/CTOX', name: 'CTOX', kind: 'folder', sort_index: 40 },
  ];
  for (const seed of seeds) {
    const existing = await files.findOne(seed.id).exec();
    const expected = {
      ...seed,
      mime_type: '',
      extension: '',
      size_bytes: 0,
      source: 'system',
      is_deleted: false,
    };
    if (!existing) {
      await files.upsert({ ...expected, created_at_ms: now, updated_at_ms: now });
      continue;
    }
    const current = existing?.toJSON?.() || existing;
    const patch = Object.fromEntries(
      Object.entries(expected).filter(([key, value]) => current?.[key] !== value),
    );
    if (Object.keys(patch).length > 0) {
      await existing.incrementalPatch({ ...patch, updated_at_ms: now });
    }
  }
}

async function storeFile(db, parentId, parentPath, name, file) {
  const files = db?.collection?.('desktop_files');
  const chunks = db?.collection?.('desktop_file_chunks');
  if (!files || !chunks) return;
  const now = Date.now();
  const id = `file_${now}_${Math.random().toString(36).slice(2, 10)}`;
  const bytes = await fileToUint8(file);
  const base64 = uint8ToBase64(bytes);
  const total = Math.max(1, Math.ceil(base64.length / CHUNK_SIZE));
  const contentHash = await sha256Hex(bytes);
  const generationId = `gen_${now}_${contentHash.slice(0, 12)}`;
  const chunkRows = await Promise.all(Array.from({ length: total }, async (_, idx) => {
    const data = base64.slice(idx * CHUNK_SIZE, (idx + 1) * CHUNK_SIZE);
    return {
      id: `${id}_${generationId}_${idx}`,
      file_id: id,
      generation_id: generationId,
      content_hash: contentHash,
      content_hash_scheme: FILE_CONTENT_HASH_SCHEME,
      idx,
      total,
      encoding: 'base64',
      data,
      chunk_hash: await sha256Hex(data),
      chunk_hash_scheme: FILE_CHUNK_HASH_SCHEME,
      size_bytes: data.length,
      created_at_ms: now,
    };
  }));
  await writeChunkDocuments(chunks, chunkRows);
  await files.upsert({
    id,
    parent_id: parentId,
    path: joinPath(parentPath, name),
    name,
    kind: 'file',
    mime_type: file.type || mimeFromName(name),
    extension: extensionFor(name),
    size_bytes: file.size || 0,
    source: 'upload',
    content_ref: id,
    content_state: 'available',
    content_hash: contentHash,
    content_hash_scheme: FILE_CONTENT_HASH_SCHEME,
    content_generation_id: generationId,
    content_synced_at_ms: now,
    sort_index: now,
    is_deleted: false,
    created_at_ms: now,
    updated_at_ms: now,
  });
}

async function readStoredFile(ctx, fileId, mimeType = 'application/octet-stream', options = {}) {
  const loader = await fileDemandLoaderFor(ctx).catch(() => null);
  if (loader?.fetchFile) {
    const chunks = await loader.fetchFile(fileId);
    return readStoredFileFromDemandChunks(chunks, mimeType, options);
  }
  throw new Error(T.contentNotAvailable);
}

async function fileDemandLoaderFor(ctx) {
  if (!ctx?.sync?.startCollection) return null;
  const bridge = await ctx.sync.startCollection('desktop_files');
  await waitForReplicationBridge(bridge, 'desktop_files');
  return bridge?.state?.demandFileLoader || null;
}

async function waitForReplicationBridge(bridge, collection, timeoutMs = 20000) {
  const state = bridge?.state;
  const wait = typeof state?.awaitInSync === 'function'
    ? state.awaitInSync.bind(state)
    : typeof state?.awaitInitialReplication === 'function'
      ? state.awaitInitialReplication.bind(state)
      : null;
  if (!wait) return;
  await Promise.race([
    wait(),
    new Promise((_, reject) => {
      setTimeout(() => reject(new Error(`${collection} data did not become ready in time`)), timeoutMs);
    }),
  ]);
}

function normalizeFileRow(data) {
  const isFolder = data.kind === 'folder';
  return {
    raw: data,
    id: String(data.id),
    sourceId: FILE_SOURCE.id,
    label: data.name || 'Unbenannt',
    kind: isFolder ? T.folder : mimeKind(data.mime_type || mimeFromName(data.name || '')),
    mark: isFolder ? 'DIR' : markFor(data, FILE_SOURCE),
    iconKind: isFolder ? 'folder' : iconKindFor(data, FILE_SOURCE),
    status: data.source || '',
    modified: formatTimestamp(timestampFor(data)),
    moduleId: null,
    path: data.virtual_path || data.path || '',
    localPath: data.local_path || data.path || '',
    virtualPath: data.virtual_path || data.path || '',
    isFolder,
    mimeType: data.mime_type || mimeFromName(data.name || ''),
    sizeBytes: Number(data.size_bytes || 0),
    sizeLabel: isFolder ? '-' : formatBytes(data.size_bytes || 0),
    source: data.source || '',
    contentState: data.content_state || '',
    contentHash: data.content_hash || '',
    contentHashScheme: data.content_hash_scheme || '',
    contentGenerationId: data.content_generation_id || '',
  };
}

function normalizeBusinessRow(data, source) {
  const label = labelFor(data);
  return {
    raw: data,
    id: String(data.id || label || crypto.randomUUID()),
    sourceId: source.id,
    label,
    kind: kindFor(data, source),
    mark: markFor(data, source),
    iconKind: iconKindFor(data, source),
    status: statusFor(data),
    modified: formatTimestamp(timestampFor(data)),
    moduleId: source.moduleId,
    path: source.label,
    sizeLabel: statusFor(data),
  };
}

function normalizeRowsForSource(data, source, currentFolderId = ROOT_ID) {
  const activeData = data.filter((item) => !item.is_deleted);
  if (source.filesystem) {
    return activeData
      .filter((item) => item.parent_id === currentFolderId)
      .map((item) => normalizeFileRow(item));
  }
  return activeData.map((item) => normalizeBusinessRow(item, source));
}

function askName(container, title, value, options = {}) {
  return new Promise((resolve) => {
    const overlay = document.createElement('div');
    overlay.className = 'app-explorer-name-dialog';
    overlay.innerHTML = `
      <form role="dialog" aria-modal="true" aria-label="${escapeHtml(title)}">
        <strong>${escapeHtml(title)}</strong>
        <input name="name" value="${escapeHtml(value)}" autocomplete="off" aria-describedby="app-explorer-name-error">
        <p id="app-explorer-name-error" class="app-explorer-dialog-error" data-name-error role="alert"></p>
        <div class="app-explorer-dialog-actions">
          <button type="button" data-cancel>${T.cancel}</button>
          <button type="submit">${escapeHtml(options.submitLabel || T.save)}</button>
        </div>
      </form>
    `;
    container.append(overlay);
    const form = overlay.querySelector('form');
    const input = overlay.querySelector('input');
    const error = overlay.querySelector('[data-name-error]');
    const submit = overlay.querySelector('[type="submit"]');
    const existing = new Set((options.existingNames || []).map((name) => String(name).toLowerCase()));
    input?.focus();
    input?.select();
    const close = (nextValue) => {
      overlay.remove();
      resolve(String(nextValue || '').trim());
    };
    const validate = () => {
      const name = String(input?.value || '').trim();
      const problem = validateEntryName(name, existing);
      if (error) error.textContent = problem;
      if (submit) submit.disabled = Boolean(problem);
      return !problem;
    };
    input?.addEventListener('input', validate);
    overlay.addEventListener('keydown', (event) => {
      if (event.key === 'Escape') close('');
    });
    overlay.querySelector('[data-cancel]')?.addEventListener('click', () => close(''));
    form?.addEventListener('submit', (event) => {
      event.preventDefault();
      if (!validate()) return;
      close(input?.value || '');
    });
    validate();
  });
}

function confirmAction(container, title, messageText) {
  return new Promise((resolve) => {
    const overlay = document.createElement('div');
    overlay.className = 'app-explorer-name-dialog';
    overlay.innerHTML = `
      <form role="dialog" aria-modal="true" aria-label="${escapeHtml(title)}">
        <strong>${escapeHtml(title)}</strong>
        <p>${escapeHtml(messageText)}</p>
        <div class="app-explorer-dialog-actions">
          <button type="button" data-cancel>${T.cancel}</button>
          <button type="submit" class="is-danger">Verschieben</button>
        </div>
      </form>
    `;
    container.append(overlay);
    const close = (value) => {
      overlay.remove();
      resolve(Boolean(value));
    };
    overlay.querySelector('[data-cancel]')?.addEventListener('click', () => close(false));
    overlay.addEventListener('keydown', (event) => {
      if (event.key === 'Escape') close(false);
    });
    overlay.querySelector('form')?.addEventListener('submit', (event) => {
      event.preventDefault();
      close(true);
    });
    overlay.querySelector('button')?.focus();
  });
}

function validateEntryName(name, existingNames = new Set()) {
  if (!name) return 'Name ist erforderlich.';
  if (/[\\/]/.test(name)) return T.noSlashes;
  if (name === '.' || name === '..') return 'Dieser Name ist reserviert.';
  if (existingNames.has(String(name).toLowerCase())) return T.nameExists;
  return '';
}

async function fileToUint8(file) {
  if (!file || typeof file.arrayBuffer !== 'function') {
    throw new Error(T.fileReadFailed);
  }
  return new Uint8Array(await file.arrayBuffer());
}

function uint8ToBase64(bytes) {
  let binary = '';
  for (let idx = 0; idx < bytes.length; idx += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(idx, idx + 0x8000));
  }
  return btoa(binary);
}

async function writeChunkDocuments(collection, docs) {
  if (!docs.length) return;
  if (typeof collection.bulkUpsert === 'function') {
    await collection.bulkUpsert(docs);
    return;
  }
  if (typeof collection.bulkInsert === 'function') {
    await collection.bulkInsert(docs);
    return;
  }
  for (const doc of docs) {
    await collection.upsert(doc);
  }
}

function isPreviewable(row) {
  return row.mimeType?.startsWith('image/') || row.mimeType?.startsWith('text/') || ['application/json', 'application/xml'].includes(row.mimeType);
}

function associatedAppFor(row = {}) {
  if (row.isFolder) return '';
  const extension = extensionFor(row.label || row.path || '');
  const mimeType = String(row.mimeType || '').toLowerCase();
  if (SPREADSHEET_EXTENSIONS.has(extension)
    || mimeType === 'text/csv'
    || mimeType === 'text/tab-separated-values'
    || mimeType.includes('spreadsheetml')) return 'spreadsheets';
  if (DOCUMENT_EXTENSIONS.has(extension)
    || mimeType === 'text/markdown'
    || mimeType.includes('wordprocessingml')) return 'documents';
  return '';
}

function normalizedMimeType(row = {}) {
  const fromName = mimeFromName(row.label || row.path || '');
  const current = String(row.mimeType || '').trim().toLowerCase();
  if (!current || current === 'application/octet-stream' || current === 'text/plain') return fromName;
  return current;
}

function dataTransferContainsFiles(dataTransfer) {
  if (!dataTransfer) return false;
  if (Number(dataTransfer.files?.length || 0) > 0) return true;
  return Array.from(dataTransfer.types || []).includes('Files');
}

function safeDownloadName(value) {
  const name = String(value || T.fileFallback)
    .replace(/[\u0000-\u001f<>:"/\\|?*]+/g, '_')
    .replace(/^\.+/, '')
    .trim();
  return (name || T.fileFallback).slice(0, 180);
}

function openLabelFor(row = {}) {
  const appId = row.sourceId === FILE_SOURCE.id ? associatedAppFor(row) : '';
  if (appId) return T.openInApp(appTitle(appId));
  return row.sourceId === FILE_SOURCE.id ? T.open : T.openInModule;
}

function appTitle(appId) {
  if (appId === 'spreadsheets') return 'Spreadsheets';
  if (appId === 'documents') return 'Documents';
  return 'der passenden App';
}

function labelFor(data) {
  return data.title || data.label || data.name || data.subject || data.filename || data.id || 'Unbenannt';
}

function kindFor(data, source = null) {
  if (data.mime_type) return mimeKind(data.mime_type);
  if (data.document_type) return data.document_type.replace(/_/g, ' ');
  return source?.kind || data.kind || 'Object';
}

function markFor(data, source) {
  if (data.kind === 'folder') return 'DIR';
  if (data.mime_type?.includes('pdf')) return 'PDF';
  if (data.mime_type?.includes('word') || data.filename?.endsWith?.('.docx') || data.name?.endsWith?.('.docx')) return 'DOC';
  if (data.mime_type?.includes('markdown') || data.filename?.endsWith?.('.md') || data.name?.endsWith?.('.md')) return 'MD';
  if (data.mime_type?.startsWith?.('image/')) return 'IMG';
  return source.mark;
}

function iconKindFor(data, source) {
  if (data.kind === 'folder') return 'folder';
  if (data.mime_type?.includes('pdf')) return 'pdf';
  if (data.mime_type?.includes('word') || data.filename?.endsWith?.('.docx') || data.name?.endsWith?.('.docx')) return 'doc';
  if (data.mime_type?.startsWith?.('image/')) return 'image';
  return source.id;
}

function statusFor(data) {
  return data.status || data.qualification_status || data.research_status || data.kind || '';
}

function timestampFor(data) {
  return Number(data.updated_at_ms || data.created_at_ms || 0);
}

function formatTimestamp(ts) {
  if (!ts) return '';
  try {
    return new Date(ts).toLocaleString(undefined, { dateStyle: 'short', timeStyle: 'short' });
  } catch {
    return '';
  }
}

function mimeKind(mime) {
  if (!mime) return 'File';
  if (mime.includes('pdf')) return 'PDF document';
  if (mime.includes('word')) return 'Word document';
  if (mime.includes('markdown')) return 'Markdown';
  if (mime.startsWith('image/')) return 'Image';
  if (mime.startsWith('text/')) return 'Text';
  return mime.split('/').at(-1) || 'File';
}

function mimeFromName(name) {
  const extension = extensionFor(name);
  if (extension === 'txt') return 'text/plain';
  if (extension === 'md' || extension === 'markdown') return 'text/markdown';
  if (extension === 'csv') return 'text/csv';
  if (extension === 'tsv') return 'text/tab-separated-values';
  if (extension === 'json') return 'application/json';
  if (extension === 'xlsx') return 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet';
  if (extension === 'docx') return 'application/vnd.openxmlformats-officedocument.wordprocessingml.document';
  if (extension === 'png') return 'image/png';
  if (extension === 'jpg' || extension === 'jpeg') return 'image/jpeg';
  if (extension === 'gif') return 'image/gif';
  if (extension === 'pdf') return 'application/pdf';
  return 'application/octet-stream';
}

function extensionFor(name) {
  return String(name || '').split('.').pop()?.toLowerCase() || '';
}

function formatBytes(value) {
  const bytes = Number(value || 0);
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function uniqueName(baseName, existingNames) {
  const existing = new Set(existingNames);
  if (!existing.has(baseName)) return baseName;
  const dot = baseName.lastIndexOf('.');
  const stem = dot > 0 ? baseName.slice(0, dot) : baseName;
  const ext = dot > 0 ? baseName.slice(dot) : '';
  let index = 2;
  while (existing.has(`${stem} ${index}${ext}`)) index += 1;
  return `${stem} ${index}${ext}`;
}

function joinPath(parent, name) {
  const prefix = parent && parent !== '/' ? parent.replace(/\/$/, '') : '';
  return `${prefix}/${name}`.replace(/\/+/g, '/');
}

function message(text, variant) {
  const p = document.createElement('p');
  p.className = `app-explorer-message${variant === 'error' ? ' is-error' : ''}`;
  p.textContent = text;
  return p;
}

function emptyPreview() {
  return `<div class="app-explorer-preview-empty">${T.noFileSelected}</div>`;
}

async function loadModuleMarkup() {
  const markupVersion = String(import.meta.url).split('?v=')[1] || '';
  const markupHref = new URL('./index.html', import.meta.url).pathname + (markupVersion ? `?v=${markupVersion}` : '');
  const html = await fetch(markupHref).then((response) => response.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}

function ensureModuleStyles() {
  const styleId = 'explorer-module-styles';
  if (document.getElementById(styleId)) return;
  const cssVersion = String(import.meta.url).split('?v=')[1] || '';
  const cssHref = new URL('./index.css', import.meta.url).pathname + (cssVersion ? `?v=${cssVersion}` : '');
  const link = document.createElement('link');
  link.id = styleId;
  link.rel = 'stylesheet';
  link.href = cssHref;
  document.head.append(link);
}

function applyStaticMarkupLabels(container) {
  const labels = {
    '[data-explorer-up]': ['aria-label', T.upLevel],
    '[data-explorer-refresh]': ['aria-label', T.refresh],
    '[data-explorer-address]': ['aria-label', T.path],
    '[data-explorer-new-folder]': ['aria-label', T.newFolderCreate],
    '[data-explorer-upload]': ['aria-label', T.uploadFiles],
    '[data-explorer-selection-actions]': ['aria-label', T.selectionActions],
    '[data-explorer-search]': ['placeholder', T.searchPlaceholder],
    '[data-explorer-sources]': ['aria-label', T.places],
    '[data-explorer-view-toggle]': ['aria-label', T.view],
    '[data-explorer-sort]': ['aria-label', T.sortBy],
    '[data-explorer-table]': ['aria-label', T.filesAria],
    '[data-explorer-preview]': ['aria-label', T.infoAria],
  };
  for (const [selector, [attribute, value]] of Object.entries(labels)) {
    container.querySelector(selector)?.setAttribute(attribute, value);
  }
  const text = {
    '[data-explorer-new-folder-label]': T.newFolder,
    '[data-explorer-sort-modified]': T.sortModified,
    '[data-explorer-sort-created]': T.sortCreated,
    '[data-explorer-sort-name]': T.sortName,
    '[data-explorer-sort-kind]': T.sortKind,
  };
  for (const [selector, value] of Object.entries(text)) {
    const node = container.querySelector(selector);
    if (node) node.textContent = value;
  }
  const root = container.querySelector('[data-explorer-root]');
  if (root) root.dataset.dropLabel = T.dropFilesHere;
}

function escapeHtml(value) {
  return String(value ?? '').replace(/[&<>"']/g, (char) => ({
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#39;',
  }[char]));
}

export const __explorerTestHooks = {
  FILE_SOURCE,
  SOURCES,
  ensureFileSystem,
  associatedAppFor,
  dataTransferContainsFiles,
  formatBytes,
  joinPath,
  mimeFromName,
  normalizedMimeType,
  normalizeBusinessRow,
  normalizeFileRow,
  normalizeRowsForSource,
  safeDownloadName,
  storeFile,
  uniqueName,
  validateEntryName,
};
