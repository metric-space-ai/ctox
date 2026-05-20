export const manifest = {
  id: 'explorer',
  title: 'Files',
  glyph: '▣',
  defaultWidth: 980,
  defaultHeight: 640,
};

const ROOT_ID = 'fs_root';
const CHUNK_SIZE = 256 * 1024;
const FILE_SOURCE = { id: 'desktop_files', label: 'Files', section: 'On this Desktop', mark: 'FS', moduleId: null, kind: 'File System', filesystem: true };
const SOURCES = [
  FILE_SOURCE,
  { id: 'documents', label: 'Documents', section: 'Business OS', mark: 'DOC', moduleId: 'documents', kind: 'Document' },
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
          <button type="button" data-explorer-new-folder>Neuer Ordner</button>
          <button type="button" data-explorer-upload>Upload</button>
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
              <button type="button" disabled>Icons</button>
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
  refs.newFolder.addEventListener('click', createFolder);
  refs.upload.addEventListener('click', () => refs.fileInput.click());
  refs.fileInput.addEventListener('change', async () => {
    await uploadFiles(refs.fileInput.files);
    refs.fileInput.value = '';
  });
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

  await ensureFileSystem(ctx.db);
  await selectSource(FILE_SOURCE);

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
      refs.table.replaceChildren(message(`Collection "${state.activeSource.id}" ist nicht verfügbar.`, 'error'));
      renderHeader();
      renderFooter();
      return;
    }
    try {
      const docs = await collection.find().exec();
      const data = docs.map((doc) => (typeof doc.toJSON === 'function' ? doc.toJSON() : doc));
      if (isFilesystemSource()) {
        state.folderDocs = new Map(data.filter((item) => item.kind === 'folder' && !item.is_deleted).map((item) => [item.id, item]));
        state.rows = data
          .filter((item) => !item.is_deleted && item.parent_id === state.currentFolderId)
          .map((item) => normalizeFileRow(item));
      } else {
        state.rows = data.map((item) => normalizeBusinessRow(item, state.activeSource));
      }
      renderHeader();
      renderRows();
    } catch (error) {
      console.error('[explorer] render failed:', error);
      state.rows = [];
      refs.table.replaceChildren(message(`Fehler: ${error?.message || error}`, 'error'));
      renderHeader();
      renderFooter();
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
  }

  function renderRows() {
    const rows = filteredRows();
    refs.count.textContent = `${rows.length} Objekt${rows.length === 1 ? '' : 'e'}`;
    if (!rows.length) {
      refs.table.replaceChildren(message(state.query ? 'Keine Treffer.' : 'Dieser Ort ist leer.'));
      refs.preview.innerHTML = emptyPreview();
      renderFooter(rows);
      return;
    }

    const table = document.createElement('div');
    table.className = 'app-explorer-grid';
    table.innerHTML = `
      <button class="app-explorer-grid-head app-explorer-grid-name" type="button" data-sort="name">Name</button>
      <button class="app-explorer-grid-head" type="button" data-sort="kind">Art</button>
      <button class="app-explorer-grid-head" type="button" data-sort="modified">Geändert</button>
      <div class="app-explorer-grid-head">Größe</div>
    `;
    for (const row of rows) table.append(rowNode(row));
    table.querySelectorAll('[data-sort]').forEach((button) => {
      button.classList.toggle('is-active', button.dataset.sort === state.sort);
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
    `;
    refs.preview.querySelector('[data-preview-open]')?.addEventListener('click', () => openRow(row));
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
      const blob = await readStoredFile(ctx.db, row.id, row.mimeType);
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
          },
        });
        return;
      }
      const blob = await readStoredFile(ctx.db, row.id, row.mimeType);
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

  async function goUp() {
    if (!isFilesystemSource() || state.currentFolderId === ROOT_ID) return;
    const folder = currentFolder();
    state.currentFolderId = folder?.parent_id || ROOT_ID;
    state.selectedId = '';
    await loadRows();
  }

  async function createFolder() {
    if (!isFilesystemSource()) return;
    const files = ctx.db?.collection?.('desktop_files');
    if (!files) return;
    const now = Date.now();
    const name = uniqueName('Neuer Ordner', state.rows.map((row) => row.label));
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

  async function renameFileRow(row) {
    const nextName = await askName(container, 'Umbenennen', row.label);
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

  function renderFooter(rows = filteredRows()) {
    refs.status.textContent = `${rows.length} Objekt${rows.length === 1 ? '' : 'e'} · ${isFilesystemSource() ? (currentFolder()?.path || '/') : state.activeSource.label}`;
  }

  function revokePreviewUrl() {
    if (!state.previewUrl) return;
    URL.revokeObjectURL(state.previewUrl);
    state.previewUrl = '';
  }

  return () => {
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
    { id: 'fs_downloads', parent_id: ROOT_ID, path: '/Downloads', name: 'Downloads', kind: 'folder', sort_index: 30 },
  ];
  for (const seed of seeds) {
    const existing = await files.findOne(seed.id).exec();
    const doc = {
      ...seed,
      mime_type: '',
      extension: '',
      size_bytes: 0,
      source: 'system',
      is_deleted: false,
      created_at_ms: now,
      updated_at_ms: now,
    };
    if (existing) await existing.incrementalPatch({ ...doc, created_at_ms: existing.created_at_ms || now });
    else await files.upsert(doc);
  }
}

async function storeFile(db, parentId, parentPath, name, file) {
  const files = db?.collection?.('desktop_files');
  const chunks = db?.collection?.('desktop_file_chunks');
  if (!files || !chunks) return;
  const now = Date.now();
  const id = `file_${now}_${Math.random().toString(36).slice(2, 10)}`;
  const dataUrl = await readFileAsDataUrl(file);
  const base64 = String(dataUrl).split(',')[1] || '';
  const total = Math.max(1, Math.ceil(base64.length / CHUNK_SIZE));
  for (let idx = 0; idx < total; idx += 1) {
    const data = base64.slice(idx * CHUNK_SIZE, (idx + 1) * CHUNK_SIZE);
    await chunks.upsert({
      id: `${id}_${idx}`,
      file_id: id,
      idx,
      total,
      encoding: 'base64',
      data,
      size_bytes: data.length,
      created_at_ms: now,
    });
  }
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
    sort_index: now,
    is_deleted: false,
    created_at_ms: now,
    updated_at_ms: now,
  });
}

async function readStoredFile(db, fileId, mimeType = 'application/octet-stream') {
  const chunks = db?.collection?.('desktop_file_chunks');
  if (!chunks) throw new Error('Datei-Chunks sind nicht verfügbar.');
  const docs = await chunks.find().exec();
  const allChunks = docs
    .map((doc) => (typeof doc.toJSON === 'function' ? doc.toJSON() : doc))
    .filter((chunk) => chunk.file_id === fileId);
  const latestCreatedAt = Math.max(0, ...allChunks.map((chunk) => Number(chunk.created_at_ms || 0)));
  const generation = allChunks.filter((chunk) => Number(chunk.created_at_ms || 0) === latestCreatedAt);
  const total = Number(generation[0]?.total || generation.length || 0);
  if (!generation.length || total <= 0) throw new Error('Dateiinhalt fehlt.');
  const data = generation
    .filter((chunk) => Number(chunk.idx) < total)
    .sort((a, b) => a.idx - b.idx)
    .map((chunk) => chunk.data)
    .join('');
  const binary = atob(data);
  const bytes = new Uint8Array(binary.length);
  for (let idx = 0; idx < binary.length; idx += 1) bytes[idx] = binary.charCodeAt(idx);
  return new Blob([bytes], { type: mimeType || 'application/octet-stream' });
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

function askName(container, title, value) {
  return new Promise((resolve) => {
    const overlay = document.createElement('div');
    overlay.className = 'app-explorer-name-dialog';
    overlay.innerHTML = `
      <form>
        <strong>${escapeHtml(title)}</strong>
        <input name="name" value="${escapeHtml(value)}" autocomplete="off">
        <div>
          <button type="button" data-cancel>Abbrechen</button>
          <button type="submit">Speichern</button>
        </div>
      </form>
    `;
    container.append(overlay);
    const form = overlay.querySelector('form');
    const input = overlay.querySelector('input');
    input?.focus();
    input?.select();
    const close = (nextValue) => {
      overlay.remove();
      resolve(String(nextValue || '').trim());
    };
    overlay.querySelector('[data-cancel]')?.addEventListener('click', () => close(''));
    form?.addEventListener('submit', (event) => {
      event.preventDefault();
      close(input?.value || '');
    });
  });
}

function readFileAsDataUrl(file) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.addEventListener('load', () => resolve(reader.result || ''));
    reader.addEventListener('error', () => reject(reader.error || new Error('Datei konnte nicht gelesen werden.')));
    reader.readAsDataURL(file);
  });
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
    .app-explorer-name-dialog button {
      border: 1px solid var(--hairline, var(--line));
      border-radius: 7px;
      background: color-mix(in srgb, var(--surface) 76%, var(--surface-2));
      color: var(--text);
      min-height: 28px;
      padding: 0 10px;
      font-weight: 720;
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
      grid-template-columns: minmax(220px, 1.4fr) minmax(110px, .65fr) minmax(130px, .55fr) minmax(90px, .45fr);
      align-content: start;
      min-width: 680px;
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
    }
    button.app-explorer-grid-head { cursor: pointer; }
    .app-explorer-grid-head.is-active { color: var(--text); }
    .app-explorer-row {
      display: contents;
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
    .app-explorer-name-dialog input {
      min-height: 32px;
      border: 1px solid var(--hairline, var(--line));
      border-radius: 8px;
      background: color-mix(in srgb, var(--bg) 48%, var(--surface));
      color: var(--text);
      padding: 0 10px;
      outline: 0;
    }
    .app-explorer-name-dialog form > div {
      display: flex;
      justify-content: flex-end;
      gap: 6px;
    }
    @media (max-width: 900px) {
      .app-explorer-body { grid-template-columns: 180px minmax(0, 1fr); }
      .app-explorer-preview { display: none; }
      .app-explorer-toolbar { grid-template-columns: auto minmax(0, 1fr) auto; }
      .app-explorer-search { grid-column: 1 / -1; }
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
