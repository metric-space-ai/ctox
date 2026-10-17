// Explorer – App-API 4.0 (Shell v4, Strang UC4). Port-Protokoll: V4-PORT.md.
//
// Dateibaum: `desktop_files` (Eigentum desktop, hier foreign: Ordner/Einträge, Pfade). Dateiinhalt nur über
// api.files (put/get, §10.2a Nr. 4) und die Plattform-Collection `files` (Metadaten, ref). Keine Chunk-Collections,
// kein Demand-Loader. Business-OS-Quellen (documents, spreadsheets, …) als foreign Live-Abfragen.

const APP_ID = 'explorer';
const ROOT_ID = 'fs_root';
const FILE_HASH_SCHEME = 'sha256-bytes-v1'; // = files.ref (sha256 der Bytes)
const SPREADSHEET_EXTENSIONS = new Set(['csv', 'tsv', 'xlsx', 'json']);
const DOCUMENT_EXTENSIONS = new Set(['docx', 'md', 'markdown', 'txt']);
const SEARCH_LIMIT = 250;

// Eingebettete Rückfalltexte; api.shell.t('explorer.<key>') übersteuert Zeichenketten (OFFEN-B).
const MESSAGES = {
  de: {
    selectionActions: 'Aktionen für Auswahl',
    recentCreated: 'Zuletzt erstellt',
    recentModified: 'Zuletzt geändert',
    recentCreatedMark: 'NEU',
    recentModifiedMark: 'ZEIT',
    upLevel: 'Eine Ebene höher',
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
    errorPrefix: 'Fehler',
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
    folder: 'Ordner',
    noSlashes: 'Name darf keine Schrägstriche enthalten.',
    nameExists: 'Name existiert bereits in diesem Ordner.',
    fileReadFailed: 'Datei konnte nicht gelesen werden.',
    openInApp: (app) => `In ${app} öffnen`,
    openInModule: 'Im Modul öffnen',
    dropFilesHere: 'Dateien hier ablegen',
    download: 'Herunterladen',
    showInModule: 'Im Modul anzeigen',
    downloadFailedPrefix: 'Download fehlgeschlagen',
    create: 'Erstellen',
    importLabel: 'Importieren',
    moveLabel: 'Verschieben',
    srcFiles: 'Dateien',
    sectionDesktop: 'Dieser Desktop',
    sectionApps: 'Apps',
    srcDocuments: 'Dokumente',
    srcSpreadsheets: 'Tabellen',
    srcKnowledge: 'Wissen',
    srcMatching: 'Matching-Objekte',
    srcOutbound: 'Outbound',
    kindFileSystem: 'Dateisystem',
    kindFile: 'Datei',
    kindDocument: 'Dokument',
    kindSpreadsheet: 'Tabelle',
    kindKnowledge: 'Wissen',
    kindObject: 'Objekt',
    kindCompany: 'Unternehmen',
    kindPdf: 'PDF-Dokument',
    kindWord: 'Word-Dokument',
    kindMarkdown: 'Markdown',
    kindImage: 'Bild',
    kindText: 'Text',
    placeLabel: 'Ort',
    untitled: 'Unbenannt',
  },
  en: {
    selectionActions: 'Actions for selection',
    recentCreated: 'Recently created',
    recentModified: 'Recently modified',
    recentCreatedMark: 'NEW',
    recentModifiedMark: 'TIME',
    upLevel: 'Up one level',
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
    errorPrefix: 'Error',
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
    folder: 'Folder',
    noSlashes: 'Name must not contain slashes.',
    nameExists: 'Name already exists in this folder.',
    fileReadFailed: 'File could not be read.',
    openInApp: (app) => `Open in ${app}`,
    openInModule: 'Open in module',
    dropFilesHere: 'Drop files here',
    download: 'Download',
    showInModule: 'Show in module',
    downloadFailedPrefix: 'Download failed',
    create: 'Create',
    importLabel: 'Import',
    moveLabel: 'Move',
    srcFiles: 'Files',
    sectionDesktop: 'On this desktop',
    sectionApps: 'Apps',
    srcDocuments: 'Documents',
    srcSpreadsheets: 'Spreadsheets',
    srcKnowledge: 'Knowledge', // v4-allow: en
    srcMatching: 'Matching objects',
    srcOutbound: 'Outbound',
    kindFileSystem: 'File system',
    kindFile: 'File', // v4-allow: en
    kindDocument: 'Document', // v4-allow: en
    kindSpreadsheet: 'Spreadsheet',
    kindKnowledge: 'Knowledge', // v4-allow: en
    kindObject: 'Object', // v4-allow: en
    kindCompany: 'Company', // v4-allow: en
    kindPdf: 'PDF document',
    kindWord: 'Word document',
    kindMarkdown: 'Markdown',
    kindImage: 'Image', // v4-allow: en
    kindText: 'Text',
    placeLabel: 'Location',
    untitled: 'Untitled',
  },
};
let T = MESSAGES.de;

function messagesFor(api) {
  const lang = String(api?.shell?.locale?.get?.() || 'de').startsWith('en') ? 'en' : 'de';
  const base = MESSAGES[lang] || MESSAGES.de;
  const out = { __lang: lang };
  for (const [key, value] of Object.entries(base)) {
    if (typeof value !== 'string') { out[key] = value; continue; }
    const full = `${APP_ID}.${key}`;
    const v = api?.shell?.t ? api.shell.t(full) : full;
    out[key] = v && v !== full ? v : value;
  }
  return out;
}

const FILE_SOURCE = { id: 'desktop_files', get label() { return T.srcFiles; }, get section() { return T.sectionDesktop; }, mark: 'FS', moduleId: null, kind: 'kindFileSystem', filesystem: true, fileCollection: true };
const RECENT_CREATED_SOURCE = { id: 'recent_created', collectionId: 'desktop_files', get label() { return T.recentCreated; }, get section() { return T.sectionDesktop; }, get mark() { return T.recentCreatedMark; }, moduleId: null, kind: 'kindFile', fileCollection: true, recentSort: 'created' };
const RECENT_MODIFIED_SOURCE = { id: 'recent_modified', collectionId: 'desktop_files', get label() { return T.recentModified; }, get section() { return T.sectionDesktop; }, get mark() { return T.recentModifiedMark; }, moduleId: null, kind: 'kindFile', fileCollection: true, recentSort: 'modified' };
const SOURCES = [
  FILE_SOURCE,
  RECENT_CREATED_SOURCE,
  RECENT_MODIFIED_SOURCE,
  { id: 'documents', get label() { return T.srcDocuments; }, get section() { return T.sectionApps; }, mark: 'DOC', moduleId: 'documents', kind: 'kindDocument' },
  { id: 'spreadsheets', get label() { return T.srcSpreadsheets; }, get section() { return T.sectionApps; }, mark: 'XLS', moduleId: 'spreadsheets', kind: 'kindSpreadsheet' },
  { id: 'knowledge_items', get label() { return T.srcKnowledge; }, get section() { return T.sectionApps; }, mark: 'KNO', moduleId: 'knowledge', kind: 'kindKnowledge' },
  { id: 'matching_objects', get label() { return T.srcMatching; }, get section() { return T.sectionApps; }, mark: 'MAT', moduleId: 'matching', kind: 'kindObject' },
  { id: 'outbound_companies', get label() { return T.srcOutbound; }, get section() { return T.sectionApps; }, mark: 'OUT', moduleId: 'outbound', kind: 'kindCompany' },
];

const SORTERS = {
  name: (a, b) => labelFor(a).localeCompare(labelFor(b), undefined, { sensitivity: 'base' }),
  kind: (a, b) => kindFor(a).localeCompare(kindFor(b), undefined, { sensitivity: 'base' }),
  modified: (a, b) => timestampFor(b) - timestampFor(a),
  created: (a, b) => Number(b.raw?.created_at_ms || 0) - Number(a.raw?.created_at_ms || 0),
};

export async function mount(root, api, { restored } = {}) {
  if (!root.dataset.app) root.dataset.app = APP_ID;
  T = messagesFor(api);
  const snap = isPlainObject(restored) ? restored : {};
  const state = {
    activeSource: SOURCES.find((s) => s.id === snap.sourceId) || FILE_SOURCE,
    currentFolderId: typeof snap.currentFolderId === 'string' && snap.currentFolderId ? snap.currentFolderId : ROOT_ID,
    folderDocs: new Map(),
    docs: [],
    query: typeof snap.query === 'string' ? snap.query : '',
    sort: SORTERS[snap.sort] ? snap.sort : 'modified',
    selectedId: typeof snap.selectedId === 'string' ? snap.selectedId : '',
    rows: [],
    previewUrl: '',
    dragExports: new Map(),
    searchTimer: null,
    filesById: new Map(),
    seeded: false,
  };
  const offs = [];
  let sourceOff = null;
  let disposed = false;

  root.innerHTML = await loadModuleMarkup();
  applyStaticMarkupLabels(root);

  const refs = {
    root: root.querySelector('[data-explorer-root]'),
    sources: root.querySelector('[data-explorer-sources]'),
    search: root.querySelector('[data-explorer-search]'),
    path: root.querySelector('[data-explorer-path]'),
    title: root.querySelector('[data-explorer-title]'),
    count: root.querySelector('[data-explorer-count]'),
    table: root.querySelector('[data-explorer-table]'),
    preview: root.querySelector('[data-explorer-preview]'),
    up: root.querySelector('[data-explorer-up]'),
    newFolder: root.querySelector('[data-explorer-new-folder]'),
    upload: root.querySelector('[data-explorer-upload]'),
    selectionActions: root.querySelector('[data-explorer-selection-actions]'),
    fileInput: root.querySelector('[data-explorer-file-input]'),
    sort: root.querySelector('[data-explorer-sort]'),
  };

  const report = (code, text, level = 'error', ref) => api.status.report({ level, code: `${APP_ID}.${code}`, text, ...(ref ? { ref } : {}) });
  const clear = (code) => api.status.clear(`${APP_ID}.${code}`);
  const errText = (error) => error?.code || error?.message || String(error);
  function watchWrite(handle, code) {
    handle?.local?.then(() => clear(code), (error) => report(code, `${T.errorPrefix}: ${errText(error)}`));
    return handle;
  }

  refs.search.value = state.query;
  refs.sort.value = state.sort;
  api.shell.setTitle('Explorer');
  renderSidebar();
  renderHeader();
  refs.search.addEventListener('input', () => {
    state.query = refs.search.value.trim();
    renderRows();
    if (!isFileCollectionSource()) return;
    if (state.searchTimer) window.clearTimeout(state.searchTimer);
    state.searchTimer = window.setTimeout(() => { state.searchTimer = null; subscribeSource(); }, 220);
  });
  refs.sort.addEventListener('change', () => { state.sort = refs.sort.value || 'modified'; renderRows(); });
  refs.up.addEventListener('click', goUp);
  refs.newFolder.addEventListener('click', createFolder);
  refs.upload.addEventListener('click', openUploadDialog);
  refs.selectionActions.addEventListener('click', () => {
    const row = filteredRows().find((entry) => entry.id === state.selectedId);
    if (!row) return;
    const bounds = refs.selectionActions.getBoundingClientRect();
    showRowActions(row, { x: bounds.left, y: bounds.bottom });
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

  // Plattform-Metadaten der Dateien (ref je Datei-ID), live.
  offs.push(api.query('files', {}).subscribe((rows) => {
    state.filesById = new Map(rows.map((row) => [row.id, row]));
  }));
  offs.push(api.shell.locale.subscribe(() => {
    const next = messagesFor(api);
    if (JSON.stringify(Object.keys(next).map((k) => typeof next[k] === 'string' ? next[k] : k)) === JSON.stringify(Object.keys(T).map((k) => typeof T[k] === 'string' ? T[k] : k))) return;
    T = next;
    applyStaticMarkupLabels(root);
    renderSidebar();
    recompute();
  }));
  offs.push(api.shell.onRoute((ev) => {
    const params = ev?.params || {};
    const source = SOURCES.find((s) => s.id === params.source);
    if (params.source && !source) { report('route.unknown_source', `${T.errorPrefix}: ${params.source}`); return; }
    if (source) selectSource(source, { folderId: typeof params.folder_id === 'string' ? params.folder_id : ROOT_ID });
  }));
  subscribeSource();

  function subscribeSource() {
    sourceOff?.();
    sourceOff = null;
    const collectionId = sourceCollectionId(state.activeSource);
    let live;
    try {
      live = api.query(collectionId, activeDocumentQueryForSource(state.activeSource, state.query));
    } catch (error) {
      state.docs = [];
      report(`source.${collectionId}`, `${T.errorPrefix}: ${collectionId} – ${errText(error)}`);
      recompute();
      return;
    }
    clear(`source.${collectionId}`);
    sourceOff = live.subscribe((docs) => {
      if (disposed) return;
      state.docs = docs;
      if (collectionId === 'desktop_files' && !state.query && !state.seeded) {
        state.seeded = true;
        ensureFileSystem(api, docs, watchWrite);
      }
      recompute();
    });
  }

  function recompute() {
    const activeData = state.docs.filter((item) => !item.is_deleted);
    state.folderDocs = isFileCollectionSource()
      ? new Map(activeData.filter((item) => item.kind === 'folder').map((item) => [item.id, item]))
      : new Map();
    state.rows = state.activeSource.recentSort || (isFilesystemSource() && state.query)
      ? activeData.filter((item) => item.kind !== 'folder').map(normalizeFileRow)
      : normalizeRowsForSource(state.docs, state.activeSource, state.currentFolderId);
    renderHeader();
    renderRows();
  }

  function selectSource(source, { folderId = state.currentFolderId } = {}) {
    state.activeSource = source;
    state.currentFolderId = folderId;
    state.selectedId = '';
    refs.search.value = '';
    state.query = '';
    state.sort = source.recentSort || state.sort;
    refs.sort.value = state.sort;
    refs.root.classList.toggle('is-filesystem', Boolean(source.filesystem));
    renderSidebar();
    subscribeSource();
  }

  function renderSidebar() {
    refs.sources.innerHTML = '';
    refs.root.classList.toggle('is-filesystem', Boolean(state.activeSource.filesystem));
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
        button.dataset.sourceId = source.id;
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
    refs.path.textContent = label === 'Files / ' ? T.srcFiles : label.replace(/^Files \/ /, `${T.srcFiles} / `);
    // FIX11 (G4): tiefe Pfade kuerzen in der Adresszeile wie im Finder; der volle Pfad steht im Tooltip.
    refs.path.title = refs.path.textContent;
    refs.title.textContent = isFilesystemSource() ? (state.currentFolderId === ROOT_ID ? T.srcFiles : folder?.name || T.srcFiles) : state.activeSource.label;
    refs.up.disabled = !isFilesystemSource() || state.currentFolderId === ROOT_ID;
    refs.newFolder.hidden = !isFilesystemSource();
    refs.upload.hidden = !canAcceptFileDrop();
  }

  function renderRows() {
    refs.selectionActions.disabled = true;
    const rows = filteredRows();
    refs.count.textContent = rows.length ? T.objectCount(rows.length) : '';
    if (!rows.length) {
      refs.table.replaceChildren();
      revokePreviewUrl();
      refs.preview.replaceChildren();
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
    // Rechtsklick über das Standard-Kontextmenü der Shell (v4/shell/app-host.js bindContextMenu, App-Plattform-Regel:
    // kein app-eigener contextmenu-Handler). Öffnen/Umbenennen/Papierkorb kommen als 'ctox:context-action' zurück;
    // Herunterladen und „In App zeigen“ bleiben im Aktionen-Knopf der Auswahl (showRowActions).
    item.dataset.contextRecordId = row.id;
    item.dataset.contextRecordType = row.isFolder ? 'folder' : 'file';
    item.dataset.contextLabel = row.label;
    item.dataset.contextActions = row.sourceId === FILE_SOURCE.id ? 'open rename delete' : 'open';
    item.addEventListener('ctox:context-action', (event) => {
      const action = event.detail?.action;
      selectRow(row);
      if (action === 'open') openRow(row);
      else if (action === 'rename' && row.sourceId === FILE_SOURCE.id) renameFileRow(row);
      else if (action === 'delete' && row.sourceId === FILE_SOURCE.id) trashFileRow(row);
    });
    if (row.sourceId === FILE_SOURCE.id && !row.isFolder) {
      item.draggable = true;
      item.title = `${row.label} ziehen oder doppelklicken`;
      const prepare = () => { void prepareDragExport(row).catch(() => undefined); };
      item.addEventListener('pointerenter', prepare);
      item.addEventListener('focus', prepare);
      item.addEventListener('mousedown', prepare);
      item.addEventListener('dragstart', (event) => startFileDrag(event, row));
      item.addEventListener('dragend', () => item.classList.remove('is-dragging-out'));
    }
    item.addEventListener('keydown', (event) => {
      if (event.key === 'Enter') { event.preventDefault(); openRow(row); }
    });
    return item;
  }

  async function showRowActions(row, at) {
    const items = [{ id: 'open', label: row.isFolder ? T.open : T.previewLabel, icon: '↗' }];
    if (row.sourceId === FILE_SOURCE.id) {
      items.push(
        { type: 'separator' },
        ...(!row.isFolder ? [{ id: 'download', label: T.download, icon: '↓' }] : []),
        { id: 'rename', label: T.rename, icon: '✎' },
        { id: 'trash', label: T.toTrash, icon: '⌫' },
      );
    } else {
      items.push({ type: 'separator' }, { id: 'module', label: T.showInModule, icon: '⌁' });
    }
    const choice = await api.shell.menu({ x: Math.round(at?.x || 0), y: Math.round(at?.y || 0), items });
    const id = typeof choice === 'string' ? choice : choice?.id;
    if (id === 'open' || id === 'module') openRow(row);
    else if (id === 'download') downloadRow(row);
    else if (id === 'rename') renameFileRow(row);
    else if (id === 'trash') trashFileRow(row);
  }

  function selectRow(row) {
    if (!row) return;
    state.selectedId = row.id;
    refs.selectionActions.disabled = false;
    refs.table.querySelectorAll('.app-explorer-row').forEach((node) => {
      node.classList.toggle('is-selected', node.dataset.id === row.id);
      node.setAttribute('aria-selected', node.dataset.id === row.id ? 'true' : 'false');
    });
    renderPreview(row);
    if (row.sourceId === FILE_SOURCE.id && !row.isFolder) void prepareDragExport(row).catch(() => undefined);
  }

  function refFor(row) {
    const meta = state.filesById.get(row.id);
    if (meta?.ref) return meta.ref;
    const raw = row.raw || {};
    if (raw.content_hash_scheme === FILE_HASH_SCHEME && isSha256(raw.content_hash)) return raw.content_hash;
    if (isSha256(raw.content_ref)) return raw.content_ref;
    return '';
  }

  async function readRowBlob(row) {
    const ref = refFor(row);
    if (!ref) {
      const error = new Error(row.label);
      error.code = 'not_found';
      throw error;
    }
    return api.files.get(ref);
  }

  async function prepareDragExport(row) {
    const existing = state.dragExports.get(row.id);
    if (existing?.url) return existing;
    if (existing?.promise) return existing.promise;
    if (!refFor(row)) return null;
    const promise = (async () => {
      const mimeType = normalizedMimeType(row);
      const blob = await readRowBlob(row);
      const bytes = new Uint8Array(await blob.arrayBuffer());
      const entry = { blob, bytes, mimeType, name: safeDownloadName(row.label), url: URL.createObjectURL(blob) };
      if (disposed) { URL.revokeObjectURL(entry.url); return entry; }
      state.dragExports.set(row.id, entry);
      return entry;
    })().catch((error) => { state.dragExports.delete(row.id); throw error; });
    state.dragExports.set(row.id, { promise });
    return promise;
  }

  function startFileDrag(event, row) {
    const prepared = state.dragExports.get(row.id);
    if (!prepared?.url) {
      event.preventDefault();
      void prepareDragExport(row)
        .then((entry) => { if (entry) report('drag.ready', `${row.label} ${T.readyToDragOut}`, 'info'); })
        .catch((error) => report('drag', `${T.dragOutFailedPrefix}: ${errText(error)}`));
      return;
    }
    clear('drag.ready');
    const desktopBridge = globalThis.ctoxBusinessOsDesktop;
    if (typeof desktopBridge?.startFileDrag === 'function') {
      event.preventDefault();
      desktopBridge.startFileDrag({ name: prepared.name, mimeType: prepared.mimeType, bytes: prepared.bytes });
    } else if (event.dataTransfer) {
      event.dataTransfer.effectAllowed = 'copy';
      event.dataTransfer.setData('DownloadURL', `${prepared.mimeType}:${prepared.name}:${prepared.url}`);
      event.dataTransfer.setData('text/uri-list', prepared.url);
      event.dataTransfer.setData('text/plain', prepared.name);
    }
    event.currentTarget?.classList.add('is-dragging-out');
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
        <dt>${T.placeLabel}</dt><dd>${escapeHtml(row.path || state.activeSource.label)}</dd>
        <dt>${T.colSize}</dt><dd>${escapeHtml(row.sizeLabel || '-')}</dd>
        <dt>${T.colModified}</dt><dd>${escapeHtml(row.modified || '-')}</dd>
      </dl>
      <button type="button" data-preview-open>${escapeHtml(openLabelFor(row))}</button>
      ${row.sourceId === FILE_SOURCE.id && !row.isFolder ? `<button type="button" data-preview-download>${escapeHtml(T.download)}</button>` : ''}
    `;
    refs.preview.querySelector('[data-preview-open]')?.addEventListener('click', () => openRow(row));
    refs.preview.querySelector('[data-preview-download]')?.addEventListener('click', () => downloadRow(row));
    if (row.sourceId === FILE_SOURCE.id && !row.isFolder) renderStoredFilePreview(row);
  }

  async function renderStoredFilePreview(row) {
    const body = refs.preview.querySelector('[data-preview-body]');
    if (!body || !isPreviewable(row)) return;
    if (row.contentState === 'lazy' || row.contentState === 'missing' || !refFor(row)) return;
    try {
      const blob = await readRowBlob(row);
      if (state.selectedId !== row.id || disposed) return;
      clear('preview');
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
      if (state.selectedId !== row.id || disposed) return;
      report('preview', `${row.label}: ${errText(error)}`, 'warn');
    }
  }

  function openRow(row) {
    if (row.sourceId === FILE_SOURCE.id) {
      if (row.isFolder) {
        state.currentFolderId = row.id;
        state.selectedId = '';
        recompute();
        return;
      }
      const ref = refFor(row);
      const associatedApp = associatedAppFor(row);
      if (associatedApp) {
        if (!ref) { report('open', `${T.openInAppFailed(appTitle(associatedApp))}: ${row.label}`); return; }
        clear('open');
        api.shell.navigate(associatedApp, {
          open_file: { ref, name: row.label, mime: normalizedMimeType(row), source_file_id: row.id, source_path: row.localPath || row.path },
        });
        return;
      }
      clear('open');
      api.shell.navigate('file-viewer', {
        ...(ref ? { ref } : {}),
        file_id: row.id,
        name: row.label,
        mime: row.mimeType,
        size: row.sizeBytes,
        path: row.localPath || row.path,
        content_state: row.contentState,
      });
      return;
    }
    if (row?.moduleId) api.shell.navigate(row.moduleId, { record: row.id });
  }

  async function downloadRow(row) {
    if (row.sourceId !== FILE_SOURCE.id || row.isFolder) return;
    try {
      const blob = await readRowBlob(row);
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement('a');
      anchor.href = url;
      anchor.download = row.label;
      anchor.rel = 'noopener';
      anchor.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
      clear('download');
    } catch (error) {
      report('download', `${T.downloadFailedPrefix}: ${row.label} – ${errText(error)}`);
    }
  }

  function goUp() {
    if (!isFilesystemSource() || state.currentFolderId === ROOT_ID) return;
    const folder = currentFolder();
    state.currentFolderId = folder?.parent_id || ROOT_ID;
    state.selectedId = '';
    recompute();
  }

  async function createFolder() {
    if (!isFilesystemSource()) return;
    const name = await askName(root, T.newFolder, '', {
      submitLabel: T.create,
      existingNames: state.rows.map((row) => row.label),
    });
    if (!name || disposed) return;
    const now = Date.now();
    const parent = currentFolder();
    watchWrite(api.put('desktop_files', {
      id: `folder_${now}_${Math.random().toString(36).slice(2, 8)}`,
      parent_id: state.currentFolderId,
      path: joinPath(parent?.path || '/', name),
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
    }), 'write.folder');
  }

  async function uploadFiles(fileList) {
    if (!canAcceptFileDrop() || !fileList?.length) return;
    const targetFolderId = isFilesystemSource() ? state.currentFolderId : ROOT_ID;
    const targetFolder = state.folderDocs.get(targetFolderId) || currentFolder();
    const targetPath = targetFolder?.path || '/';
    const existingNames = state.docs
      .filter((entry) => !entry.is_deleted && entry.parent_id === targetFolderId)
      .map((entry) => entry.name);
    for (const file of [...fileList]) {
      const name = uniqueName(file.name || T.fileFallback, existingNames);
      existingNames.push(name);
      try {
        const { upload, write } = await storeFile(api, targetFolderId, targetPath, name, file);
        watchWrite(upload, 'upload');
        watchWrite(write, 'upload');
      } catch (error) {
        report('upload', `${T.fileReadFailed} ${name}: ${errText(error)}`);
      }
    }
    if (!isFilesystemSource()) selectSource(FILE_SOURCE, { folderId: targetFolderId });
  }

  function openUploadDialog() {
    if (!isFilesystemSource()) return;
    const overlay = document.createElement('div');
    overlay.className = 'app-explorer-upload-dialog';
    overlay.innerHTML = `
      <form role="dialog" aria-modal="true" aria-label="${T.uploadFiles}">
        <strong>${T.uploadFiles}</strong>
        <p>${escapeHtml(T.chooseFilesFor(currentFolder()?.path || '/'))}</p>
        <button type="button" class="app-explorer-dropzone" data-pick-files>${T.pickFiles}</button>
        <ul data-upload-list></ul>
        <div class="app-explorer-dialog-actions">
          <button type="button" data-cancel>${T.cancel}</button>
          <button type="submit" data-submit disabled>${T.importLabel}</button>
        </div>
      </form>
    `;
    refs.root.append(overlay);
    const selected = [];
    const list = overlay.querySelector('[data-upload-list]');
    const submit = overlay.querySelector('[data-submit]');
    const close = () => { if (refs.fileInput.onchange) refs.fileInput.onchange = null; overlay.remove(); };
    const renderSelection = () => {
      list.replaceChildren(...selected.map((file) => {
        const item = document.createElement('li');
        item.textContent = `${file.name || T.fileFallback} · ${formatBytes(file.size || 0)}`;
        return item;
      }));
      submit.disabled = selected.length === 0;
    };
    overlay.querySelector('[data-pick-files]')?.addEventListener('click', () => refs.fileInput.click());
    overlay.querySelector('[data-cancel]')?.addEventListener('click', close);
    // Escape schließt wie „Abbrechen“; Fokus in den Dialog, damit die Taste ihn erreicht (Rundgang RUND1: der
    // Upload-Dialog ließ sich nur per Maus schließen und verdeckte die App).
    overlay.addEventListener('keydown', (event) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      event.stopPropagation();
      close();
    });
    overlay.addEventListener('dragover', (event) => { event.preventDefault(); overlay.classList.add('is-dragging-files'); });
    overlay.addEventListener('dragleave', () => overlay.classList.remove('is-dragging-files'));
    overlay.addEventListener('drop', (event) => {
      event.preventDefault();
      event.stopPropagation();
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
      if (!selected.length) return;
      submit.disabled = true;
      await uploadFiles(selected);
      close();
    });
    renderSelection();
    overlay.querySelector('[data-pick-files]')?.focus({ preventScroll: true });
  }

  async function renameFileRow(row) {
    const nextName = await askName(root, T.rename, row.label, {
      submitLabel: T.save,
      existingNames: state.rows.filter((item) => item.id !== row.id).map((item) => item.label),
    });
    if (!nextName || nextName === row.label || disposed) return;
    const parent = currentFolder();
    watchWrite(api.patch('desktop_files', row.id, {
      name: nextName,
      path: joinPath(parent?.path || '/', nextName),
      updated_at_ms: Date.now(),
    }), 'write.rename');
  }

  async function trashFileRow(row) {
    const confirmed = await api.shell.dialog({
      kind: 'confirm', title: T.moveToTrashTitle, text: T.removedFromFolder(row.label), confirm_label: T.moveLabel, default: false,
    });
    if (confirmed !== true || disposed) return;
    watchWrite(api.patch('desktop_files', row.id, { is_deleted: true, updated_at_ms: Date.now() }), 'write.trash');
  }

  function currentFolder() {
    return state.folderDocs.get(state.currentFolderId) || { id: ROOT_ID, parent_id: '', path: '/', name: 'Files' };
  }
  function isFilesystemSource() { return state.activeSource.filesystem === true; }
  function canAcceptFileDrop() { return isFilesystemSource() || Boolean(state.activeSource.recentSort); }
  function isFileCollectionSource() { return state.activeSource.fileCollection === true; }
  function revokePreviewUrl() {
    if (!state.previewUrl) return;
    URL.revokeObjectURL(state.previewUrl);
    state.previewUrl = '';
  }

  return {
    unmount() {
      disposed = true;
      if (state.searchTimer) window.clearTimeout(state.searchTimer);
      sourceOff?.();
      sourceOff = null;
      for (const off of offs.splice(0)) off();
      revokePreviewUrl();
      for (const entry of state.dragExports.values()) if (entry?.url) URL.revokeObjectURL(entry.url);
      state.dragExports.clear();
      root.replaceChildren();
    },
    snapshot() {
      return {
        v: 1,
        sourceId: state.activeSource.id,
        currentFolderId: state.currentFolderId,
        query: refs.search?.value ?? state.query,
        sort: state.sort,
        selectedId: state.selectedId,
      };
    },
  };
}

function isPlainObject(v) { return Boolean(v) && typeof v === 'object' && !Array.isArray(v); }
function isSha256(v) { return typeof v === 'string' && /^[0-9a-f]{64}$/.test(v); }

function sourceCollectionId(source) {
  return source.collectionId || source.id;
}

function escapeRegex(value) {
  return String(value || '').replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function activeDocumentQueryForSource(source, searchQuery = '') {
  const query = String(searchQuery || '').trim();
  if (!source?.fileCollection || !query) return {};
  const pattern = escapeRegex(query);
  return {
    selector: {
      $or: [
        { name: { $regex: pattern, $options: 'i' } },
        { path: { $regex: pattern, $options: 'i' } },
        { virtual_path: { $regex: pattern, $options: 'i' } },
      ],
    },
    sort: [{ updated_at_ms: 'desc' }],
    limit: SEARCH_LIMIT,
  };
}

const SYSTEM_FOLDERS = [
  { id: ROOT_ID, parent_id: '', path: '/', name: 'Files', kind: 'folder', sort_index: 0 },
  { id: 'fs_desktop', parent_id: ROOT_ID, path: '/Desktop', name: 'Desktop', kind: 'folder', sort_index: 10 },
  { id: 'fs_documents', parent_id: ROOT_ID, path: '/Documents', name: 'Documents', kind: 'folder', sort_index: 20 },
  { id: 'fs_spreadsheets', parent_id: ROOT_ID, path: '/Spreadsheets', name: 'Spreadsheets', kind: 'folder', sort_index: 25 },
  { id: 'fs_downloads', parent_id: ROOT_ID, path: '/Downloads', name: 'Downloads', kind: 'folder', sort_index: 30 },
  { id: 'fs_ctox', parent_id: ROOT_ID, path: '/CTOX', name: 'CTOX', kind: 'folder', sort_index: 40 },
];

/** Systemordner anlegen/angleichen (vorher bei jedem Laden; jetzt einmal je Montage über die erste Live-Emission). */
function ensureFileSystem(api, docs = [], watch = (h) => h) {
  const byId = new Map(docs.map((doc) => [doc.id, doc]));
  const now = Date.now();
  const handles = [];
  for (const seed of SYSTEM_FOLDERS) {
    const expected = { ...seed, mime_type: '', extension: '', size_bytes: 0, source: 'system', is_deleted: false };
    const current = byId.get(seed.id);
    if (!current) {
      handles.push(watch(api.put('desktop_files', { ...expected, created_at_ms: now, updated_at_ms: now }), 'write.seed'));
      continue;
    }
    const patch = Object.fromEntries(Object.entries(expected).filter(([key, value]) => current?.[key] !== value));
    if (Object.keys(patch).length > 0) handles.push(watch(api.patch('desktop_files', seed.id, { ...patch, updated_at_ms: now }), 'write.seed'));
  }
  return handles;
}

/** Inhalt über api.files.put (Blob + Metadaten in `files`), Baumeintrag in desktop_files mit ref (= sha256). */
async function storeFile(api, parentId, parentPath, name, file) {
  if (!file || typeof file.arrayBuffer !== 'function') throw new Error(T.fileReadFailed);
  const now = Date.now();
  const id = `file_${now}_${Math.random().toString(36).slice(2, 10)}`;
  const mime = file.type || mimeFromName(name);
  const upload = api.files.put(file, { id, name, mime });
  const ref = await upload.ref;
  const write = api.put('desktop_files', {
    id,
    parent_id: parentId,
    path: joinPath(parentPath, name),
    name,
    kind: 'file',
    mime_type: mime,
    extension: extensionFor(name),
    size_bytes: file.size || 0,
    source: 'upload',
    content_ref: ref,
    content_state: 'available',
    content_hash: ref,
    content_hash_scheme: FILE_HASH_SCHEME,
    sort_index: now,
    is_deleted: false,
    created_at_ms: now,
    updated_at_ms: now,
  });
  return { id, ref, upload, write };
}

function normalizeFileRow(data) {
  const isFolder = data.kind === 'folder';
  return {
    raw: data,
    id: String(data.id),
    sourceId: FILE_SOURCE.id,
    label: data.name || T.untitled,
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

function validateEntryName(name, existingNames = new Set()) {
  if (!name) return 'Name ist erforderlich.';
  if (/[\\/]/.test(name)) return T.noSlashes;
  if (name === '.' || name === '..') return 'Dieser Name ist reserviert.';
  if (existingNames.has(String(name).toLowerCase())) return T.nameExists;
  return '';
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
  return data.title || data.label || data.name || data.subject || data.filename || data.id || T.untitled;
}

// Art-Spalte: Quell-Art (Schlüssel kind*) bzw. MIME-Art über die App-Texte; Statuswerte über die Glossar-Begriffe
// der Shell (v4/frame/glossary.js STATUSES, gespiegelt). Unbekannte Rohwerte gehen unverändert durch.
function kindFor(data, source = null) {
  if (data.mime_type) return mimeKind(data.mime_type);
  if (data.document_type) return data.document_type.replace(/_/g, ' ');
  if (source?.kind) return T[source.kind] || source.kind;
  return data.kind || T.kindObject;
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

// Spiegel der Glossar-Statusbegriffe (v4/frame/glossary.js STATUSES), nur für deutsche Oberfläche.
const STATUS_DE = {
  // v4-allow: label – Statuswerte der Datensätze als Spaltenbeschriftung (Spiegel glossary.js), kein Zustandstext der App
  new: 'Neu', open: 'Offen', pending: 'Wartend', in_progress: 'In Bearbeitung', running: 'Läuft', queued: 'In Warteschlange',
  scheduled: 'Geplant', active: 'Aktiv', inactive: 'Inaktiv', paused: 'Pausiert', draft: 'Entwurf', ready: 'Bereit',
  review: 'Prüfung', in_review: 'In Prüfung', approved: 'Freigegeben', accepted: 'Angenommen', rejected: 'Abgelehnt',
  declined: 'Abgelehnt', blocked: 'Blockiert', done: 'Erledigt', completed: 'Abgeschlossen', complete: 'Abgeschlossen',
  closed: 'Geschlossen', resolved: 'Gelöst', failed: 'Fehlgeschlagen', error: 'Fehler', cancelled: 'Abgebrochen',
  canceled: 'Abgebrochen', archived: 'Archiviert', deleted: 'Gelöscht', expired: 'Abgelaufen', sent: 'Gesendet',
  qualified: 'Qualifiziert', unknown: 'Unbekannt',
};

function statusFor(data) {
  // `kind` ist keine Statusangabe (stand vorher roh als „confirm“/„folder“ in der Spalte Größe/Status).
  const raw = String(data.status || data.qualification_status || data.research_status || '');
  if (T.__lang !== 'en') {
    const de = STATUS_DE[raw.trim().toLowerCase().replace(/[\s-]+/g, '_')];
    if (de) return de;
  }
  return raw;
}

function timestampFor(data) {
  return Number(data.updated_at_ms || data.created_at_ms || 0);
}

function formatTimestamp(ts) {
  if (!ts) return '';
  try {
    return new Date(ts).toLocaleString(T.__lang === 'en' ? 'en' : 'de-DE', { dateStyle: 'short', timeStyle: 'short' });
  } catch {
    return '';
  }
}

function mimeKind(mime) {
  if (!mime) return T.kindFile;
  if (mime.includes('pdf')) return T.kindPdf;
  if (mime.includes('word')) return T.kindWord;
  if (mime.includes('markdown')) return T.kindMarkdown;
  if (mime.startsWith('image/')) return T.kindImage;
  if (mime.startsWith('text/')) return T.kindText;
  return mime.split('/').at(-1) || T.kindFile;
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

function escapeHtml(value) {
  return String(value ?? '').replace(/[&<>"']/g, (char) => ({
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#39;',
  }[char]));
}

async function loadModuleMarkup() {
  const html = await fetch(new URL('./index.html', import.meta.url)).then((response) => response.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}

function applyStaticMarkupLabels(container) {
  const labels = {
    '[data-explorer-up]': ['aria-label', T.upLevel],
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

export const __explorerTestHooks = {
  FILE_SOURCE,
  SOURCES,
  SYSTEM_FOLDERS,
  activeDocumentQueryForSource,
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
