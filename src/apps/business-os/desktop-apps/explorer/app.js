import {
  FILE_CHUNK_HASH_SCHEME,
  FILE_CONTENT_HASH_SCHEME,
  readStoredFileFromDemandChunks,
  sha256Hex,
} from '../../shared/file-integrity.js?v=20260708-canonical-rechunk1';

export const manifest = {
  id: 'explorer',
  title: 'Files',
  glyph: '▣',
  defaultWidth: 980,
  defaultHeight: 640,
};

const ROOT_ID = 'fs_root';
const CHUNK_SIZE = 16 * 1024;
const FILE_SOURCE = { id: 'desktop_files', label: 'Files', section: 'On this Desktop', mark: 'FS', moduleId: null, kind: 'File System', filesystem: true };
const SOURCES = [
  FILE_SOURCE,
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
};

export async function mount(container, ctx) {
  ensureStyles();
  const state = {
    activeSource: FILE_SOURCE,
    currentFolderId: ROOT_ID,
    folderDocs: new Map(),
    query: '',
    sort: 'modified',
    selectedId: '',
    rows: [],
    previewUrl: '',
    lastLoad: null,
  };

  container.innerHTML = `
    <div class="app-explorer" data-explorer-root>
      <header class="app-explorer-toolbar">
        <div class="app-explorer-nav" aria-label="Navigation">
          <button type="button" data-explorer-up aria-label="Eine Ebene höher">⌃</button>
          <button type="button" data-explorer-refresh aria-label="Aktualisieren">↻</button>
        </div>
        <div class="app-explorer-address" aria-label="Pfad">
          <span>Business OS</span>
          <b data-explorer-path>Files</b>
        </div>
        <div class="app-explorer-actions">
          <button type="button" data-explorer-new-folder aria-label="Neuen Ordner erstellen"><span aria-hidden="true">＋</span><span>Neuer Ordner</span></button>
          <button type="button" data-explorer-upload aria-label="Dateien hochladen"><span aria-hidden="true">⇧</span><span>Upload</span></button>
          <input data-explorer-file-input type="file" multiple hidden>
        </div>
        <label class="app-explorer-search">
          <span aria-hidden="true">⌕</span>
          <input data-explorer-search placeholder="Suchen">
        </label>
      </header>
      <div class="app-explorer-body">
        <aside class="app-explorer-sidebar" data-explorer-sources aria-label="Orte"></aside>
        <main class="app-explorer-main">
          <div class="app-explorer-heading">
            <div>
              <strong data-explorer-title>Files</strong>
              <span data-explorer-count></span>
            </div>
            <div class="app-explorer-view-toggle" aria-label="Ansicht">
              <button type="button" class="is-active">Details</button>
            </div>
          </div>
          <section class="app-explorer-table" data-explorer-table aria-label="Dateien"></section>
          <footer class="app-explorer-status" data-explorer-status></footer>
        </main>
        <aside class="app-explorer-preview" data-explorer-preview aria-label="Informationen"></aside>
      </div>
    </div>
  `;

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
    fileInput: container.querySelector('[data-explorer-file-input]'),
  };

  renderSidebar();
  refs.search.addEventListener('input', () => {
    state.query = refs.search.value.trim();
    renderRows();
  });
  refs.up.addEventListener('click', goUp);
  refs.refresh.addEventListener('click', loadRows);
  refs.newFolder.addEventListener('click', promptCreateFolder);
  refs.upload.addEventListener('click', openUploadDialog);
  refs.root.addEventListener('dragover', (event) => {
    if (!isFilesystemSource()) return;
    event.preventDefault();
    refs.root.classList.add('is-dragging-files');
  });
  refs.root.addEventListener('dragleave', () => refs.root.classList.remove('is-dragging-files'));
  refs.root.addEventListener('drop', async (event) => {
    if (!isFilesystemSource()) return;
    event.preventDefault();
    refs.root.classList.remove('is-dragging-files');
    await uploadFiles(event.dataTransfer?.files);
  });

  let disposed = false;
  refs.table.replaceChildren(message('Lade Dateien...'));
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
        message: `Fehler: ${error?.message || error}`,
      };
      renderHeader();
      renderRows();
    });

  async function selectSource(source) {
    state.activeSource = source;
    state.selectedId = '';
    refs.search.value = '';
    state.query = '';
    refs.root.classList.toggle('is-filesystem', Boolean(source.filesystem));
    renderSidebar();
    await loadRows();
  }

  async function loadRows() {
    refs.table.replaceChildren(message('Lade Dateien...'));
    refs.preview.innerHTML = emptyPreview();
    revokePreviewUrl();
    const collection = ctx.db?.collection?.(state.activeSource.id);
    if (!collection) {
      state.rows = [];
      state.lastLoad = {
        ok: false,
        reason: 'missing_collection',
        total: 0,
        visible: 0,
        message: `Collection "${state.activeSource.id}" ist nicht verfügbar.`,
      };
      renderHeader();
      renderRows();
      return;
    }
    try {
      const docs = await collection.find(activeDocumentQueryForSource(state.activeSource)).exec();
      if (disposed) return;
      const data = docs.map((doc) => (typeof doc.toJSON === 'function' ? doc.toJSON() : doc));
      const activeData = data.filter((item) => !item.is_deleted);
      if (isFilesystemSource()) {
        state.folderDocs = new Map(activeData.filter((item) => item.kind === 'folder').map((item) => [item.id, item]));
      } else {
        state.folderDocs = new Map();
      }
      state.rows = normalizeRowsForSource(data, state.activeSource, state.currentFolderId);
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
        message: `Fehler: ${error?.message || error}`,
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
    refs.upload.hidden = !isFilesystemSource();
    refs.refresh.setAttribute('aria-label', `Aktualisieren: ${state.activeSource.label}`);
  }

  function renderRows() {
    const rows = filteredRows();
    refs.count.textContent = `${rows.length} Objekt${rows.length === 1 ? '' : 'e'}`;
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
        <button class="app-explorer-grid-head app-explorer-grid-name" type="button" data-sort="name" role="columnheader">Name</button>
        <button class="app-explorer-grid-head" type="button" data-sort="kind" role="columnheader">Art</button>
        <button class="app-explorer-grid-head" type="button" data-sort="modified" role="columnheader">Geändert</button>
        <div class="app-explorer-grid-head" role="columnheader">Größe</div>
      </div>
    `;
    for (const row of rows) table.append(rowNode(row));
    table.querySelectorAll('[data-sort]').forEach((button) => {
      button.classList.toggle('is-active', button.dataset.sort === state.sort);
      button.setAttribute('aria-sort', button.dataset.sort === state.sort ? 'descending' : 'none');
      button.addEventListener('click', () => {
        state.sort = button.dataset.sort || 'modified';
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
      ? state.rows.filter((row) => `${row.label} ${row.kind} ${row.status}`.toLowerCase().includes(query))
      : state.rows;
    return [...rows].sort(SORTERS[state.sort] || SORTERS.modified);
  }

  function rowNode(row) {
    const item = document.createElement('button');
    item.type = 'button';
    item.className = 'app-explorer-row';
    item.dataset.id = row.id;
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
    item.addEventListener('keydown', (event) => {
      if (event.key === 'Enter') {
        event.preventDefault();
        openRow(row);
      }
    });
    item.addEventListener('contextmenu', (event) => {
      if (!ctx.contextMenu) return;
      const actions = [
        { label: row.isFolder ? 'Öffnen' : 'Vorschau', icon: '↗', action: () => openRow(row) },
      ];
      if (row.sourceId === FILE_SOURCE.id) {
        actions.push(
          { type: 'separator' },
          ...(!row.isFolder ? [{ label: 'Herunterladen', icon: '↓', action: () => downloadRow(row) }] : []),
          { label: 'Umbenennen', icon: '✎', action: () => renameFileRow(row) },
          { label: 'In Papierkorb', icon: '⌫', action: () => trashFileRow(row) }
        );
      } else {
        actions.push(
          { type: 'separator' },
          { label: 'Im Modul anzeigen', icon: '⌁', action: () => openRow(row) }
        );
      }
      ctx.contextMenu.show(event, actions);
    });
    return item;
  }

  function selectRow(row) {
    if (!row) return;
    state.selectedId = row.id;
    refs.table.querySelectorAll('.app-explorer-row').forEach((node) => {
      node.classList.toggle('is-selected', node.dataset.id === row.id);
      node.setAttribute('aria-selected', node.dataset.id === row.id ? 'true' : 'false');
    });
    renderPreview(row);
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
        <dt>Größe</dt><dd>${escapeHtml(row.sizeLabel || '-')}</dd>
        <dt>Geändert</dt><dd>${escapeHtml(row.modified || '-')}</dd>
        <dt>ID</dt><dd>${escapeHtml(row.id)}</dd>
      </dl>
      <button type="button" data-preview-open>${row.sourceId === FILE_SOURCE.id ? 'Öffnen' : 'Im Modul öffnen'}</button>
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
      body.innerHTML = '<p class="app-explorer-preview-empty">Keine integrierte Vorschau für diesen Dateityp.</p>';
      return;
    }
    if (row.contentState === 'lazy' || row.contentState === 'missing') {
      body.innerHTML = '<p class="app-explorer-preview-empty">Der Inhalt wird beim Öffnen über CTOX geladen.</p>';
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
        body.innerHTML = `<p class="app-explorer-message is-error">Download fehlgeschlagen: ${escapeHtml(error?.message || error)}</p>`;
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
    const name = await askName(container, 'Neuer Ordner', '', {
      submitLabel: 'Erstellen',
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
    if (!isFilesystemSource() || !fileList?.length) return;
    const existingNames = state.rows.map((row) => row.label);
    for (const file of [...fileList]) {
      const name = uniqueName(file.name || 'Datei', existingNames);
      existingNames.push(name);
      await storeFile(ctx.db, state.currentFolderId, currentFolder()?.path || '/', name, file);
    }
    await loadRows();
  }

  async function openUploadDialog() {
    if (!isFilesystemSource()) return;
    const overlay = document.createElement('div');
    overlay.className = 'app-explorer-upload-dialog';
    overlay.innerHTML = `
      <form role="dialog" aria-modal="true" aria-label="Dateien hochladen">
        <strong>Dateien hochladen</strong>
        <p>Wähle Dateien für ${escapeHtml(currentFolder()?.path || '/')}.</p>
        <button type="button" class="app-explorer-dropzone" data-pick-files>Dateien auswählen</button>
        <ul data-upload-list></ul>
        <div class="app-explorer-dialog-error" data-upload-error role="alert"></div>
        <div class="app-explorer-dialog-actions">
          <button type="button" data-cancel>Abbrechen</button>
          <button type="submit" data-submit disabled>Importieren</button>
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
        item.textContent = `${file.name || 'Datei'} · ${formatBytes(file.size || 0)}`;
        return item;
      }));
      submit.disabled = selected.length === 0;
      if (error) error.textContent = selected.length ? '' : 'Noch keine Datei ausgewählt.';
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
        if (error) error.textContent = 'Wähle mindestens eine Datei aus.';
        return;
      }
      if (submit) submit.disabled = true;
      await uploadFiles(selected);
      close();
    });
    renderSelection();
  }

  async function renameFileRow(row) {
    const nextName = await askName(container, 'Umbenennen', row.label, {
      submitLabel: 'Speichern',
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
    const confirmed = await confirmAction(container, 'In Papierkorb verschieben', `"${row.label}" wird aus diesem Ordner entfernt.`);
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

  function activeDocumentQueryForSource(source) {
    return {};
  }

  function renderFooter(rows = filteredRows()) {
    const sourceLabel = isFilesystemSource() ? (currentFolder()?.path || '/') : state.activeSource.label;
    const sourceState = state.lastLoad?.ok === false ? 'Fehler' : `${state.lastLoad?.total ?? rows.length} geladen`;
    refs.status.textContent = `${rows.length} sichtbar · ${sourceState} · ${sourceLabel}`;
  }

  function revokePreviewUrl() {
    if (!state.previewUrl) return;
    URL.revokeObjectURL(state.previewUrl);
    state.previewUrl = '';
  }

  function emptyStateText() {
    if (state.query) return `Keine Treffer für "${state.query}".`;
    if (state.lastLoad?.ok && state.lastLoad.total > 0 && state.lastLoad.visible === 0) {
      return 'Daten vorhanden, aber für diesen Ordner nicht sichtbar.';
    }
    return isFilesystemSource()
      ? 'Dieser Ordner ist leer.'
      : `Keine ${state.activeSource.kind}-Einträge verfügbar.`;
  }

  return () => {
    disposed = true;
    revokePreviewUrl();
    container.replaceChildren();
  };
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
  throw new Error('Dateiinhalt ist noch nicht über den Sync-Demand-Pfad verfügbar.');
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
      setTimeout(() => reject(new Error(`${collection} replication did not become ready in time`)), timeoutMs);
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
    kind: isFolder ? 'Ordner' : mimeKind(data.mime_type || mimeFromName(data.name || '')),
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
          <button type="button" data-cancel>Abbrechen</button>
          <button type="submit">${escapeHtml(options.submitLabel || 'Speichern')}</button>
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
          <button type="button" data-cancel>Abbrechen</button>
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
  if (/[\\/]/.test(name)) return 'Name darf keine Schrägstriche enthalten.';
  if (name === '.' || name === '..') return 'Dieser Name ist reserviert.';
  if (existingNames.has(String(name).toLowerCase())) return 'Name existiert bereits in diesem Ordner.';
  return '';
}

async function fileToUint8(file) {
  if (!file || typeof file.arrayBuffer !== 'function') {
    throw new Error('Datei konnte nicht gelesen werden.');
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
  if (extension === 'txt' || extension === 'md' || extension === 'csv') return 'text/plain';
  if (extension === 'json') return 'application/json';
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
  return '<div class="app-explorer-preview-empty">Keine Datei ausgewählt.</div>';
}

function ensureStyles() {
  if (document.getElementById('app-explorer-styles')) return;
  const style = document.createElement('style');
  style.id = 'app-explorer-styles';
  style.textContent = `
    .app-explorer {
      position: relative;
      display: grid;
      grid-template-rows: 48px minmax(0, 1fr);
      height: 100%;
      min-height: 0;
      background: var(--surface);
      color: var(--text);
      font: 12px/1.35 ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }
    .app-explorer.is-dragging-files::after {
      content: "Dateien hier ablegen";
      position: absolute;
      inset: 54px 14px 14px;
      display: grid;
      place-items: center;
      z-index: 4;
      border: 1px dashed color-mix(in srgb, var(--accent) 58%, var(--line));
      border-radius: 10px;
      background: color-mix(in srgb, var(--accent) 10%, var(--surface));
      color: var(--accent);
      font-weight: 780;
      pointer-events: none;
    }
    .app-explorer-toolbar {
      display: grid;
      grid-template-columns: auto minmax(190px, 1fr) auto minmax(150px, 240px);
      align-items: center;
      gap: 8px;
      padding: 8px 10px;
      border-bottom: 1px solid var(--hairline, var(--line));
      background: color-mix(in srgb, var(--surface) 92%, var(--surface-2));
    }
    .app-explorer-nav,
    .app-explorer-actions,
    .app-explorer-view-toggle { display: inline-flex; gap: 4px; }
    .app-explorer-nav button,
    .app-explorer-actions button,
    .app-explorer-view-toggle button,
    .app-explorer-preview button,
    .app-explorer-name-dialog button,
    .app-explorer-upload-dialog button {
      border: 1px solid var(--hairline, var(--line));
      border-radius: 7px;
      background: color-mix(in srgb, var(--surface) 76%, var(--surface-2));
      color: var(--text);
      min-height: 28px;
      padding: 0 10px;
      font-weight: 720;
    }
    .app-explorer-actions button {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: 6px;
      white-space: nowrap;
    }
    .app-explorer-nav button { width: 30px; padding: 0; font-size: 15px; }
    .app-explorer-address {
      display: flex;
      align-items: center;
      gap: 7px;
      min-width: 0;
      height: 30px;
      border: 1px solid var(--hairline, var(--line));
      border-radius: 8px;
      background: color-mix(in srgb, var(--bg) 44%, var(--surface));
      padding: 0 10px;
      color: var(--muted);
    }
    .app-explorer-address b { color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .app-explorer-address span::after { content: "/"; margin-left: 7px; color: var(--muted); }
    .app-explorer-search {
      display: grid;
      grid-template-columns: auto minmax(0, 1fr);
      align-items: center;
      gap: 6px;
      height: 30px;
      border: 1px solid var(--hairline, var(--line));
      border-radius: 999px;
      background: color-mix(in srgb, var(--bg) 44%, var(--surface));
      color: var(--muted);
      padding: 0 10px;
    }
    .app-explorer-search input {
      min-width: 0;
      border: 0;
      outline: 0;
      background: transparent;
      color: var(--text);
    }
    .app-explorer-body {
      display: grid;
      grid-template-columns: 210px minmax(0, 1fr) 220px;
      min-height: 0;
    }
    .app-explorer-sidebar {
      border-right: 1px solid var(--hairline, var(--line));
      background: color-mix(in srgb, var(--surface-2) 58%, var(--surface));
      padding: 10px 8px;
      overflow: auto;
    }
    .app-explorer-sidebar-group + .app-explorer-sidebar-group { margin-top: 14px; }
    .app-explorer-sidebar h3 {
      margin: 0 8px 6px;
      color: var(--muted);
      font-size: 10px;
      font-weight: 800;
      letter-spacing: .04em;
      text-transform: uppercase;
    }
    .app-explorer-source {
      width: 100%;
      display: grid;
      grid-template-columns: 34px minmax(0, 1fr);
      align-items: center;
      gap: 8px;
      min-height: 34px;
      border: 1px solid transparent;
      border-radius: 8px;
      background: transparent;
      color: var(--text);
      padding: 0 8px;
      text-align: left;
    }
    .app-explorer-source:hover { background: color-mix(in srgb, var(--surface) 72%, transparent); }
    .app-explorer-source.is-active {
      border-color: color-mix(in srgb, var(--accent) 34%, var(--line));
      background: color-mix(in srgb, var(--accent) 12%, var(--surface));
      color: var(--accent);
    }
    .app-explorer-source-mark,
    .app-explorer-file-icon {
      display: grid;
      place-items: center;
      border: 1px solid var(--hairline, var(--line));
      border-radius: 6px;
      background: color-mix(in srgb, var(--surface) 82%, var(--surface-2));
      color: var(--muted);
      font-size: 9px;
      font-weight: 850;
      letter-spacing: 0;
    }
    .app-explorer-source-mark { width: 28px; height: 24px; }
    .app-explorer-main {
      display: grid;
      grid-template-rows: 44px minmax(0, 1fr) 28px;
      min-width: 0;
      min-height: 0;
      background: color-mix(in srgb, var(--bg) 34%, var(--surface));
    }
    .app-explorer-heading {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      padding: 8px 12px;
      border-bottom: 1px solid var(--hairline, var(--line));
    }
    .app-explorer-heading > div:first-child { display: grid; gap: 1px; min-width: 0; }
    .app-explorer-heading strong { font-size: 13px; }
    .app-explorer-heading span { color: var(--muted); font-size: 11px; }
    .app-explorer-view-toggle button { min-height: 26px; color: var(--muted); }
    .app-explorer-view-toggle button.is-active { color: var(--text); border-color: color-mix(in srgb, var(--accent) 28%, var(--line)); }
    .app-explorer-table {
      min-height: 0;
      overflow: auto;
    }
    .app-explorer-grid {
      display: grid;
      align-content: start;
      width: 100%;
      min-width: 0;
    }
    .app-explorer-grid-header,
    .app-explorer-row {
      display: grid;
      grid-template-columns: minmax(0, 1.55fr) minmax(0, .65fr) minmax(0, .7fr) minmax(0, .45fr);
      width: 100%;
      min-width: 0;
    }
    .app-explorer-grid-head {
      position: sticky;
      top: 0;
      z-index: 1;
      height: 30px;
      border: 0;
      border-bottom: 1px solid var(--hairline, var(--line));
      border-right: 1px solid color-mix(in srgb, var(--line) 54%, transparent);
      background: color-mix(in srgb, var(--surface) 92%, var(--surface-2));
      color: var(--muted);
      padding: 0 10px;
      text-align: left;
      font-size: 11px;
      font-weight: 760;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    button.app-explorer-grid-head { cursor: pointer; }
    .app-explorer-grid-head.is-active { color: var(--text); }
    .app-explorer-row {
      border: 0;
      background: transparent;
      padding: 0;
      text-align: left;
      cursor: default;
      min-width: 0;
    }
    .app-explorer-row > span {
      display: flex;
      align-items: center;
      min-width: 0;
      min-height: 34px;
      border: 0;
      border-bottom: 1px solid color-mix(in srgb, var(--line) 46%, transparent);
      background: transparent;
      color: var(--text);
      padding: 0 10px;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .app-explorer-row:hover > span,
    .app-explorer-row:focus-visible > span {
      background: color-mix(in srgb, var(--surface-2) 72%, transparent);
      outline: none;
    }
    .app-explorer-row.is-selected > span {
      background: color-mix(in srgb, var(--accent) 14%, var(--surface));
    }
    .app-explorer-file { gap: 10px; }
    .app-explorer-file-icon {
      flex: 0 0 28px;
      width: 28px;
      height: 24px;
      color: var(--accent);
    }
    .app-explorer-file-icon[data-kind="folder"] {
      color: var(--warning, #e2b84c);
    }
    .app-explorer-file-name {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      font-weight: 650;
    }
    .app-explorer-status {
      display: flex;
      align-items: center;
      border-top: 1px solid var(--hairline, var(--line));
      color: var(--muted);
      padding: 0 12px;
      font-size: 11px;
    }
    .app-explorer-preview {
      border-left: 1px solid var(--hairline, var(--line));
      background: color-mix(in srgb, var(--surface) 92%, var(--surface-2));
      padding: 14px;
      overflow: auto;
    }
    .app-explorer-preview-card {
      display: grid;
      justify-items: center;
      gap: 6px;
      padding: 8px 0 14px;
      border-bottom: 1px solid var(--hairline, var(--line));
      text-align: center;
    }
    .app-explorer-preview-icon {
      display: grid;
      place-items: center;
      width: 58px;
      height: 48px;
      border: 1px solid var(--hairline, var(--line));
      border-radius: 9px;
      background: color-mix(in srgb, var(--surface-2) 72%, var(--surface));
      color: var(--accent);
      font-weight: 850;
      font-size: 12px;
    }
    .app-explorer-preview-card strong {
      max-width: 100%;
      overflow-wrap: anywhere;
      font-size: 13px;
    }
    .app-explorer-preview-card small,
    .app-explorer-preview-empty {
      color: var(--muted);
    }
    .app-explorer-preview dl {
      display: grid;
      grid-template-columns: 72px minmax(0, 1fr);
      gap: 8px;
      margin: 14px 0;
      font-size: 11px;
    }
    .app-explorer-preview dt { color: var(--muted); }
    .app-explorer-preview dd {
      margin: 0;
      min-width: 0;
      overflow-wrap: anywhere;
    }
    .app-explorer-preview button {
      width: 100%;
      color: var(--accent);
      border-color: color-mix(in srgb, var(--accent) 34%, var(--line));
    }
    .app-explorer-image-preview {
      display: block;
      max-width: 100%;
      max-height: 180px;
      object-fit: contain;
      margin: 12px auto 0;
      border-radius: 8px;
      border: 1px solid var(--hairline, var(--line));
    }
    .app-explorer-text-preview {
      max-height: 190px;
      overflow: auto;
      margin: 12px 0 0;
      padding: 10px;
      border: 1px solid var(--hairline, var(--line));
      border-radius: 8px;
      background: color-mix(in srgb, var(--bg) 54%, var(--surface));
      color: var(--text);
      white-space: pre-wrap;
      overflow-wrap: anywhere;
      font: 11px/1.45 ui-monospace, SFMono-Regular, Menlo, monospace;
    }
    .app-explorer-message {
      margin: 0;
      padding: 18px;
      color: var(--muted);
      font-size: 12px;
    }
    .app-explorer-message.is-error { color: var(--danger); }
    .app-explorer-name-dialog {
      position: absolute;
      inset: 0;
      z-index: 8;
      display: grid;
      place-items: center;
      background: color-mix(in srgb, var(--bg) 42%, transparent);
    }
    .app-explorer-upload-dialog {
      position: absolute;
      inset: 0;
      z-index: 8;
      display: grid;
      place-items: center;
      background: color-mix(in srgb, var(--bg) 42%, transparent);
    }
    .app-explorer-name-dialog form {
      display: grid;
      gap: 10px;
      width: min(340px, calc(100% - 40px));
      border: 1px solid var(--hairline, var(--line));
      border-radius: 10px;
      background: var(--surface);
      padding: 14px;
      box-shadow: var(--shadow-2, 0 18px 50px rgba(0,0,0,.35));
    }
    .app-explorer-upload-dialog form {
      display: grid;
      gap: 10px;
      width: min(420px, calc(100% - 40px));
      border: 1px solid var(--hairline, var(--line));
      border-radius: 10px;
      background: var(--surface);
      padding: 14px;
      box-shadow: var(--shadow-2, 0 18px 50px rgba(0,0,0,.35));
    }
    .app-explorer-upload-dialog p,
    .app-explorer-name-dialog p {
      margin: 0;
      color: var(--muted);
      font-size: 12px;
    }
    .app-explorer-name-dialog input {
      min-height: 32px;
      border: 1px solid var(--hairline, var(--line));
      border-radius: 8px;
      background: color-mix(in srgb, var(--bg) 48%, var(--surface));
      color: var(--text);
      padding: 0 10px;
      outline: 0;
    }
    .app-explorer-dropzone {
      min-height: 84px;
      border: 1px dashed color-mix(in srgb, var(--accent) 44%, var(--line));
      border-radius: 9px;
      background: color-mix(in srgb, var(--accent) 8%, var(--surface));
      color: var(--accent);
      font-weight: 780;
    }
    .app-explorer-upload-dialog.is-dragging-files .app-explorer-dropzone {
      background: color-mix(in srgb, var(--accent) 14%, var(--surface));
    }
    .app-explorer-upload-dialog ul {
      display: grid;
      gap: 4px;
      max-height: 120px;
      overflow: auto;
      margin: 0;
      padding: 0;
      list-style: none;
      color: var(--text);
      font-size: 12px;
    }
    .app-explorer-dialog-error {
      min-height: 16px;
      margin: 0;
      color: var(--danger);
      font-size: 11px;
    }
    .app-explorer-dialog-actions {
      display: flex;
      justify-content: flex-end;
      gap: 6px;
    }
    .app-explorer-dialog-actions .is-danger {
      border-color: color-mix(in srgb, var(--danger) 42%, var(--line));
      color: var(--danger);
    }
    @container business-app-window (max-width: 900px) {
      .app-explorer-body { grid-template-columns: 180px minmax(0, 1fr); }
      .app-explorer-preview { display: none; }
      .app-explorer-toolbar { grid-template-columns: auto minmax(0, 1fr) auto; }
      .app-explorer-search { grid-column: 1 / -1; }
    }
    @container business-app-window (max-width: 640px) {
      .app-explorer {
        grid-template-rows: auto minmax(0, 1fr);
      }
      .app-explorer-toolbar {
        grid-template-columns: auto minmax(0, 1fr);
      }
      .app-explorer-actions,
      .app-explorer-search {
        grid-column: 1 / -1;
      }
      .app-explorer-actions button {
        flex: 1 1 0;
      }
      .app-explorer-body {
        grid-template-columns: minmax(0, 1fr);
        grid-template-rows: auto minmax(0, 1fr);
      }
      .app-explorer-sidebar {
        display: flex;
        gap: 10px;
        border-right: 0;
        border-bottom: 1px solid var(--hairline, var(--line));
        overflow-x: auto;
        overflow-y: hidden;
      }
      .app-explorer-sidebar-group {
        display: grid;
        grid-auto-flow: column;
        grid-auto-columns: max-content;
        align-items: center;
        gap: 6px;
      }
      .app-explorer-sidebar-group + .app-explorer-sidebar-group { margin-top: 0; }
      .app-explorer-sidebar h3 { margin: 0 2px 0 0; white-space: nowrap; }
      .app-explorer-source {
        width: auto;
        min-width: 128px;
      }
      .app-explorer-grid-header,
      .app-explorer-row {
        grid-template-columns: minmax(132px, 1fr) minmax(72px, .55fr) minmax(86px, .65fr);
      }
      .app-explorer-grid-head:nth-child(4),
      .app-explorer-row > span:nth-child(4) {
        display: none;
      }
    }
  `;
  document.head.appendChild(style);
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
  formatBytes,
  joinPath,
  mimeFromName,
  normalizeBusinessRow,
  normalizeFileRow,
  normalizeRowsForSource,
  storeFile,
  uniqueName,
  validateEntryName,
};
