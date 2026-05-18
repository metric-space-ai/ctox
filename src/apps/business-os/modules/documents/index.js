const DOCX_MIME = 'application/vnd.openxmlformats-officedocument.wordprocessingml.document';
const MARKDOWN_MIME = 'text/markdown';
const CHUNK_SIZE = 256000;
const DOCX_TOOLBAR_VISIBILITY_KEY = 'ctox.businessOs.documents.docxToolbarVisible';
const DOCUMENT_RENDER_DEBOUNCE_MS = 80;

export async function mount(ctx) {
  await ensureStyles();
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  ctx.host.innerHTML = html;

  const state = {
    ctx,
    formatModule: null,
    formatModuleLoadPromise: null,
    superdocModule: null,
    documents: [],
    runbooks: [],
    selectedId: '',
    selectedVersion: null,
    editorHandle: null,
    superdocSaveTimer: null,
    superdocSavePromise: null,
    renderSerial: 0,
    switchSerial: 0,
    needsFinalSave: false,
    dirty: false,
    searchQuery: '',
    statusFilter: 'all',
    tagFilter: 'all',
    sortBy: 'updated_desc',
    docxToolbarVisible: localStorage.getItem(DOCX_TOOLBAR_VISIBILITY_KEY) !== 'false',
    localSubscriptionCleanup: null,
    contextMenu: null,
    contextMenuCleanup: null,
  };

  wireModule(state);
  state.contextMenuCleanup = initDocumentsContextMenu(state);
  state.localSubscriptionCleanup = wireLocalRealtime(state);
  await ensureSeedRunbooks(ctx);
  await refreshRunbooks(state);
  await refreshDocuments(state);
  renderLeft(state);
  renderRight(state);
  renderCenter(state);
  return () => {
    if (state.superdocSaveTimer) clearTimeout(state.superdocSaveTimer);
    state.contextMenuCleanup?.();
    state.contextMenu?.remove();
    state.contextMenu = null;
    state.localSubscriptionCleanup?.();
    flushActiveSuperDocDraft(state).catch((error) => console.error('[documents] final SuperDoc draft save failed', error));
    state.editorHandle?.destroy?.();
  };
}

async function loadDocumentFormatModule() {
  return import('../../vendor/document-format.mjs');
}

async function ensureDocumentFormatModule(state) {
  if (state.formatModule) return state.formatModule;
  if (!state.formatModuleLoadPromise) {
    state.formatModuleLoadPromise = loadDocumentFormatModule().then((formatModule) => {
      state.formatModule = formatModule;
      return formatModule;
    });
  }
  return state.formatModuleLoadPromise;
}

async function loadSuperDocModule(state) {
  if (state.superdocModule) return state.superdocModule;
  ensureSuperDocStyles();
  state.superdocModule = await import('../../vendor/superdoc.mjs');
  return state.superdocModule;
}

function wireModule(state) {
  state.ctx.host.addEventListener('documents:refresh-left', () => renderLeft(state));
}

function initDocumentsContextMenu(state) {
  state.contextMenu?.remove();
  const menu = document.createElement('div');
  menu.className = 'ctox-context-menu documents-context-menu';
  menu.hidden = true;
  document.body.append(menu);
  state.contextMenu = menu;

  const handleContextMenu = (event) => {
    if (state.ctx.module?.id !== 'documents') return;
    const context = documentCommandContextFromElement(state, event.target);
    event.preventDefault();
    event.stopPropagation();
    renderDocumentsContextMenu(state, context, event.clientX, event.clientY);
  };
  const handleOutsideClick = (event) => {
    if (state.contextMenu?.contains(event.target)) return;
    hideDocumentsContextMenu(state);
  };
  const handleEscape = (event) => {
    if (event.key === 'Escape') hideDocumentsContextMenu(state);
  };

  state.ctx.host.addEventListener('contextmenu', handleContextMenu);
  state.ctx.left?.addEventListener?.('contextmenu', handleContextMenu);
  state.ctx.right?.addEventListener?.('contextmenu', handleContextMenu);
  window.addEventListener('click', handleOutsideClick, { capture: true });
  window.addEventListener('keydown', handleEscape);

  return () => {
    state.ctx.host.removeEventListener('contextmenu', handleContextMenu);
    state.ctx.left?.removeEventListener?.('contextmenu', handleContextMenu);
    state.ctx.right?.removeEventListener?.('contextmenu', handleContextMenu);
    window.removeEventListener('click', handleOutsideClick, { capture: true });
    window.removeEventListener('keydown', handleEscape);
    hideDocumentsContextMenu(state);
  };
}

function hideDocumentsContextMenu(state) {
  if (state.contextMenu) state.contextMenu.hidden = true;
}

function documentCommandContextFromElement(state, target) {
  const element = target?.nodeType === Node.ELEMENT_NODE ? target : target?.parentElement;
  const recordElement = element?.closest?.('[data-context-record-id], [data-document-id], [data-document-manage]');
  const recordId = recordElement?.dataset.contextRecordId
    || recordElement?.dataset.documentId
    || recordElement?.dataset.documentManage
    || state.selectedId
    || '';
  const record = state.documents.find((item) => item.id === recordId) || selectedRecord(state);
  const field = element?.closest?.('input, textarea, select, button');
  const column = recordElement?.dataset.documentsColumn
    || (state.ctx.left?.contains?.(element) ? 'documents' : state.ctx.right?.contains?.(element) ? 'runbooks' : 'editor');

  return {
    module: 'documents',
    column,
    field: field?.name || field?.dataset.documentsRunbook || field?.dataset.action || '',
    record_type: record ? 'document' : 'module',
    record_id: record?.id || '',
    label: record?.title || record?.filename || '',
    filename: record?.filename || '',
    document_type: record?.document_type || '',
    selected_text: String(window.getSelection?.()?.toString?.() || '').trim().slice(0, 1000),
    clicked_text: String(element?.innerText || element?.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 500),
  };
}

function renderDocumentsContextMenu(state, context, x, y) {
  const canModifyApp = canModifyDocumentsApp(state);
  state.contextMenu.innerHTML = `
    <form class="documents-context-chat" data-documents-context-chat-form>
      <header>
        <div>
          <strong>Chat to CTOX</strong>
          <span>${escapeHtml(documentContextSummary(context))}</span>
        </div>
        <button type="button" data-documents-context-close aria-label="Schließen">×</button>
      </header>
      ${canModifyApp ? `
        <div class="documents-context-mode" role="radiogroup" aria-label="CTOX Aufgabe">
          <label><input type="radio" name="contextMode" value="data" checked /> Mit Daten arbeiten</label>
          <label><input type="radio" name="contextMode" value="app" /> App modifizieren</label>
        </div>
      ` : ''}
      <textarea data-documents-context-message placeholder="Was soll CTOX hier tun oder prüfen?"></textarea>
      <footer>
        <span data-documents-context-status></span>
        <button type="submit">Senden</button>
      </footer>
    </form>
  `;
  state.contextMenu.hidden = false;
  state.contextMenu.style.left = '0px';
  state.contextMenu.style.top = '0px';
  const rect = state.contextMenu.getBoundingClientRect();
  const maxLeft = Math.max(8, window.innerWidth - rect.width - 8);
  const maxTop = Math.max(8, window.innerHeight - rect.height - 8);
  state.contextMenu.style.left = `${clampNumber(x, 8, maxLeft)}px`;
  state.contextMenu.style.top = `${clampNumber(y, 8, maxTop)}px`;

  const form = state.contextMenu.querySelector('[data-documents-context-chat-form]');
  const textarea = state.contextMenu.querySelector('[data-documents-context-message]');
  state.contextMenu.querySelector('[data-documents-context-close]')?.addEventListener('click', () => hideDocumentsContextMenu(state));
  form?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const mode = canModifyApp ? (new FormData(form).get('contextMode') || 'data') : 'data';
    await dispatchDocumentsContextChat(state, context, textarea?.value || '', mode);
  });
  requestAnimationFrame(() => textarea?.focus());
}

function canModifyDocumentsApp(state) {
  if (typeof state.ctx.canModifyModule === 'function' && state.ctx.canModifyModule()) return true;
  const user = state.ctx.session?.user || {};
  const role = String(user.role || (user.is_admin ? 'admin' : 'user')).trim().toLowerCase().replace(/^business_os_/, '');
  return ['admin', 'chef'].includes(role);
}

function documentContextSummary(context) {
  return [context.column || 'module', context.record_type || '', context.label || context.filename || context.record_id || '']
    .filter(Boolean)
    .join(' · ') || 'Documents';
}

async function dispatchDocumentsContextChat(state, context, message, mode = 'data') {
  const trimmed = String(message || '').trim();
  const status = state.contextMenu?.querySelector('[data-documents-context-status]');
  if (!trimmed) {
    if (status) status.textContent = 'Nachricht fehlt.';
    return;
  }

  const safeMode = mode === 'app' && canModifyDocumentsApp(state) ? 'app' : 'data';
  const record = state.documents.find((item) => item.id === context.record_id) || selectedRecord(state);
  const runbookId = defaultRunbookId(state);
  const runbook = state.runbooks.find((item) => item.id === runbookId || item.command_type === runbookId) || null;
  if (!document.querySelector('[data-ctox-chat-root]')) {
    if (status) status.textContent = 'Chat ist noch nicht bereit.';
    return;
  }
  if (status) status.textContent = 'Oeffne Chat...';
  const title = `${safeMode === 'app' ? 'Documents App modifizieren' : 'Documents bearbeiten'} · ${context.label || record?.title || context.column || 'Documents'}`;
  const instruction = safeMode === 'app'
    ? `Modifiziere die Documents-App anhand dieser Admin-Anweisung. Kontext nur als UI-Bezug verwenden, Dokumentdaten selbst nicht als primäres Ziel verändern.\n\n${trimmed}`
    : trimmed;
  window.dispatchEvent(new CustomEvent('ctox-business-os-chat-submit', {
    detail: {
      text: trimmed,
      module: 'documents',
      source_title: 'Documents',
      command_type: safeMode === 'app' ? 'ctox.business_os.app.modify' : 'business_os.chat.task',
      record_id: safeMode === 'app' ? 'documents' : (record?.id || context.record_id || 'documents'),
      title,
      instruction,
      payload: {
        title,
        instruction,
        prompt: trimmed,
        user_message: trimmed,
        mode: safeMode,
        target: safeMode === 'app' ? 'app' : 'data',
        selected_document: record || null,
        selected_version_id: record?.current_version_id || '',
        selected_runbook: runbook,
        context,
        thread_key: 'business-os/documents',
      },
      client_context: {
        action: 'context-chat',
        mode: safeMode,
        column: context.column,
        record_type: context.record_type,
        document_id: record?.id || '',
        filename: record?.filename || context.filename || '',
      },
    },
  }));
  hideDocumentsContextMenu(state);
}

function wireLocalRealtime(state) {
  const collections = ['documents', 'document_versions', 'document_runbooks', 'document_blob_chunks'];
  let timer = null;
  const schedule = () => {
    if (timer) return;
    timer = window.setTimeout(() => {
      timer = null;
      refreshDocumentsFromLocal(state).catch((error) => {
        console.warn('[documents] local realtime render failed', error);
      });
    }, DOCUMENT_RENDER_DEBOUNCE_MS);
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

async function refreshDocumentsFromLocal(state) {
  const previousSelectedVersionId = state.selectedVersion?.id || '';
  await Promise.all([
    refreshRunbooks(state),
    refreshDocuments(state),
  ]);
  if (state.selectedId && previousSelectedVersionId !== selectedRecord(state)?.current_version_id) {
    await loadSelectedVersion(state).catch(() => null);
  }
  renderLeft(state);
  renderRight(state);
}

async function refreshDocuments(state) {
  const collection = state.ctx.db?.raw?.documents;
  state.documents = collection
    ? (await collection.find({ selector: { is_deleted: false }, sort: [{ updated_at_ms: 'desc' }] }).exec()).map((doc) => doc.toJSON())
    : [];
  if (!state.selectedId && state.documents[0]) state.selectedId = state.documents[0].id;
}

async function refreshRunbooks(state) {
  const collection = state.ctx.db?.raw?.document_runbooks;
  state.runbooks = collection
    ? (await collection.find({ sort: [{ title: 'asc' }] }).exec()).map((doc) => doc.toJSON())
    : [];
}

async function createMarkdownDocument(state, input = {}) {
  const title = sanitizeDocumentTitle(input.title || `markdown-${new Date().toISOString().slice(0, 10)}`);
  const prompt = String(input.prompt || '').trim();
  const text = prompt
    ? `# ${title}\n\n${prompt}\n`
    : `# ${title}\n\nKlicke in diesen Absatz und schreibe direkt weiter.\n`;
  const file = new File([text], ensureExtension(slugFilename(title), '.md'), { type: MARKDOWN_MIME });
  return importDocumentFile(state, file, {
    applyRunbook: Boolean(input.runbookId || prompt),
    prompt,
    runbookId: input.runbookId,
    tags: input.tags,
    sourceAction: 'create_document',
  });
}

async function importDocumentFile(state, file, workflow = {}) {
  requireDocumentPersistence(state.ctx);
  if (!isSupportedDocumentFile(file)) {
    renderError(state, 'Nur .docx, .md oder .markdown Dateien werden akzeptiert.');
    return null;
  }
  const isMarkdown = isMarkdownFile(file);
  const bytes = new Uint8Array(await file.arrayBuffer());
  const documentId = `doc_${crypto.randomUUID()}`;
  const versionId = `${documentId}_v1`;
  const blobId = `${versionId}_blob`;
  const now = Date.now();
  const formatModule = await ensureDocumentFormatModule(state);
  const parsed = isMarkdown
    ? formatModule.importMarkdown(new TextDecoder().decode(bytes))
    : await formatModule.importDocx(bytes);
  const indexText = formatModule.getDocumentText(parsed.document).slice(0, 20000);
  const sha = await sha256Hex(bytes);
  const mimeType = isMarkdown ? MARKDOWN_MIME : DOCX_MIME;
  const documentType = isMarkdown ? 'markdown_document' : 'word_document';
  const tags = normalizeTags(workflow.tags);

  await saveBlobChunks(state.ctx, {
    blobId,
    documentId,
    versionId,
    mimeType,
    bytes,
  });

  await state.ctx.db.raw.document_versions.insert({
    id: versionId,
    document_id: documentId,
    version: 1,
    source_kind: isMarkdown ? 'imported_markdown' : 'imported_docx',
    blob_id: blobId,
    model_json: parsed.document,
    diagnostics: parsed.diagnostics,
    created_at_ms: now,
    updated_at_ms: now,
  });

  await state.ctx.db.raw.documents.insert({
    id: documentId,
    title: titleFromFilename(file.name),
    filename: file.name,
    description: '',
    mime_type: mimeType,
    status: 'Imported',
    document_type: documentType,
    owner_id: '',
    current_version_id: versionId,
    source_sha256: sha,
    page_count: 0,
    diagnostics_count: parsed.diagnostics.length,
    linked_records: [],
    tags,
    display_cache: {},
    index_text: indexText,
    is_deleted: false,
    created_at_ms: now,
    updated_at_ms: now,
  });

  state.selectedId = documentId;
  const record = {
    id: documentId,
    title: titleFromFilename(file.name),
    filename: file.name,
    description: '',
    mime_type: mimeType,
    status: 'Imported',
    document_type: documentType,
    owner_id: '',
    current_version_id: versionId,
    source_sha256: sha,
    page_count: 0,
    diagnostics_count: parsed.diagnostics.length,
    linked_records: [],
    tags,
    display_cache: {},
    index_text: indexText,
    is_deleted: false,
    created_at_ms: now,
    updated_at_ms: now,
  };
  if (workflow.applyRunbook) {
    await dispatchDocumentRunbook(state, {
      record,
      versionId,
      runbookId: workflow.runbookId,
      prompt: workflow.prompt,
      sourceAction: workflow.sourceAction || 'import_document',
    });
  }
  await refreshDocuments(state);
  renderLeft(state);
  renderRight(state);
  await loadSelectedVersion(state);
  renderCenter(state);
  return record;
}

async function loadSelectedVersion(state) {
  const record = selectedRecord(state);
  if (!record?.current_version_id) {
    state.selectedVersion = null;
    return null;
  }
  let doc = await withTimeout(
    state.ctx.db.raw.document_versions.findOne(record.current_version_id).exec(),
    4500,
    `Version ${record.current_version_id} konnte nicht geladen werden.`,
  );
  if (!doc) {
    const fallback = await withTimeout(
      state.ctx.db.raw.document_versions.find({
        selector: { document_id: record.id },
        sort: [{ updated_at_ms: 'desc' }],
        limit: 1,
      }).exec(),
      4500,
      `Keine Versionen fuer ${record.id} gefunden.`,
    );
    doc = fallback[0] || null;
    if (doc) {
      const versionJson = doc.toJSON();
      const recordDoc = await state.ctx.db.raw.documents.findOne(record.id).exec();
      await recordDoc?.incrementalPatch({ current_version_id: versionJson.id });
      record.current_version_id = versionJson.id;
    }
  }
  state.selectedVersion = doc?.toJSON() || null;
  state.dirty = false;
  return state.selectedVersion;
}

function renderLeft(state) {
  const wrap = document.createElement('div');
  wrap.className = 'documents-explorer';
  const visible = visibleDocuments(state);
  const selected = selectedRecord(state);
  wrap.innerHTML = `
    <div class="documents-column-head">
      <div class="documents-column-title">Documents</div>
      <div class="documents-column-actions">
        <button class="documents-column-icon" type="button" aria-label="Markdown-Dokument erstellen" title="Markdown-Dokument erstellen" data-documents-new-markdown>${iconSvg('new')}</button>
        <button class="documents-column-icon" type="button" aria-label="Dokument importieren" title="Dokument importieren" data-documents-import-open>${iconSvg('import')}</button>
        <button class="documents-column-icon" type="button" aria-label="Ausgewähltes Dokument exportieren" title="Ausgewähltes Dokument exportieren" data-documents-export ${selected ? '' : 'disabled'}>${iconSvg('export')}</button>
      </div>
    </div>
    <div class="documents-column-tools">
      <input type="search" placeholder="Dokument suchen..." aria-label="Dokumente suchen" data-documents-search value="${escapeHtml(state.searchQuery)}">
      <div class="documents-column-sort">
        <select aria-label="Dokumente sortieren" data-documents-sort>
          <option value="updated_desc" ${state.sortBy === 'updated_desc' ? 'selected' : ''}>Neueste zuerst</option>
          <option value="updated_asc" ${state.sortBy === 'updated_asc' ? 'selected' : ''}>Älteste zuerst</option>
          <option value="title_asc" ${state.sortBy === 'title_asc' ? 'selected' : ''}>Titel A-Z</option>
          <option value="status" ${state.sortBy === 'status' ? 'selected' : ''}>Status</option>
        </select>
        <select aria-label="Dokumentstatus filtern" data-documents-status>
          <option value="all" ${state.statusFilter === 'all' ? 'selected' : ''}>Alle</option>
          <option value="Imported" ${state.statusFilter === 'Imported' ? 'selected' : ''}>Imported</option>
          <option value="Draft" ${state.statusFilter === 'Draft' ? 'selected' : ''}>Draft</option>
          <option value="Review" ${state.statusFilter === 'Review' ? 'selected' : ''}>Review</option>
          <option value="Final" ${state.statusFilter === 'Final' ? 'selected' : ''}>Final</option>
        </select>
      </div>
      <select aria-label="Dokument-Tags filtern" data-documents-tag>
        ${tagFilterOptions(state)}
      </select>
    </div>
  `;
  const list = document.createElement('div');
  list.className = 'documents-list';
  list.dataset.documentsList = 'true';
  populateDocumentList(state, list, visible);
  wrap.append(list);
  bindLeftControls(state, wrap);
  state.ctx.left.replaceChildren(wrap);
}

function populateDocumentList(state, list, records = visibleDocuments(state)) {
  list.replaceChildren();
  for (const record of records) {
    const card = document.createElement('article');
    card.className = 'documents-card';
    card.dataset.contextModule = 'documents';
    card.dataset.contextRecordType = 'document';
    card.dataset.contextRecordId = record.id;
    card.dataset.contextLabel = record.title || record.filename || record.id;
    card.dataset.documentsColumn = 'documents';
    card.setAttribute('aria-current', String(record.id === state.selectedId));
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'documents-card-main';
    button.dataset.documentId = record.id;
    button.innerHTML = `
      <strong>${escapeHtml(record.title)}</strong>
      <span class="documents-card-filename">${escapeHtml(record.filename)}</span>
      ${documentDescription(record) ? `<span class="documents-card-description">${escapeHtml(documentDescription(record))}</span>` : ''}
      <small>${escapeHtml(record.status)} · ${escapeHtml(record.document_type === 'markdown_document' ? 'Markdown' : 'DOCX')}</small>
      ${renderTagPills(record)}
    `;
    button.addEventListener('click', () => {
      switchSelectedDocument(state, record.id).catch((error) => {
        console.error('[documents] document switch failed', error);
        renderError(state, `Dokumentwechsel fehlgeschlagen: ${error?.message || error}`);
      });
    });
    const manage = document.createElement('button');
    manage.type = 'button';
    manage.className = 'documents-card-manage';
    manage.dataset.documentManage = record.id;
    manage.title = `${record.title} verwalten`;
    manage.setAttribute('aria-label', `${record.title} verwalten`);
    manage.innerHTML = iconSvg('settings');
    manage.addEventListener('click', () => openManageDocumentDrawer(state, record));
    card.append(button, manage);
    list.append(card);
  }
  if (!records.length) {
    const empty = document.createElement('div');
    empty.className = 'documents-empty';
    empty.innerHTML = state.documents.length
      ? '<strong>Keine Treffer</strong><span>Suche oder Filter anpassen.</span>'
      : '<strong>No documents</strong><span>Über das Import-Icon DOCX oder Markdown hinzufügen.</span>';
    list.append(empty);
  }
}

async function switchSelectedDocument(state, documentId) {
  if (!documentId) return;
  if (documentId === state.selectedId && state.selectedVersion) {
    renderCenter(state);
    return;
  }
  const switchSerial = (state.switchSerial || 0) + 1;
  state.switchSerial = switchSerial;
  const previousRecord = selectedRecord(state);
  try {
    await withTimeout(
      flushActiveSuperDocDraft(state, previousRecord, { allowFailure: true }),
      2500,
      'Automatische Draft-Speicherung beim Dokumentwechsel hat zu lange gedauert.',
    );
  } catch (error) {
    console.warn('[documents] continuing document switch after draft save failed', error);
  }
  if (state.switchSerial !== switchSerial) return;
  state.selectedId = documentId;
  state.selectedVersion = null;
  renderLeft(state);
  renderRight(state);
  const host = state.ctx.host.querySelector('[data-documents-editor]');
  if (host) host.innerHTML = '<div class="documents-loading"><strong>Lade Dokument</strong><span>Dokumentwechsel läuft.</span></div>';
  try {
    await loadSelectedVersion(state);
  } catch (error) {
    if (state.switchSerial !== switchSerial) return;
    state.selectedVersion = null;
    renderLeft(state);
    renderRight(state);
    renderError(state, `Dokument konnte nicht geladen werden: ${error?.message || error}`);
    return;
  }
  if (state.switchSerial !== switchSerial) return;
  renderLeft(state);
  renderRight(state);
  renderCenter(state);
}

function bindLeftControls(state, wrap) {
  wrap.querySelector('[data-documents-import-open]')?.addEventListener('click', () => {
    openImportDrawer(state);
  });
  wrap.querySelector('[data-documents-new-markdown]')?.addEventListener('click', () => {
    openNewDocumentDrawer(state);
  });
  wrap.querySelector('[data-documents-export]')?.addEventListener('click', () => openExportDrawer(state));
  wrap.querySelector('[data-documents-search]')?.addEventListener('input', (event) => {
    state.searchQuery = event.currentTarget.value || '';
    const list = wrap.querySelector('[data-documents-list]');
    if (list) populateDocumentList(state, list);
  });
  wrap.querySelector('[data-documents-sort]')?.addEventListener('change', (event) => {
    state.sortBy = event.currentTarget.value || 'updated_desc';
    renderLeft(state);
  });
  wrap.querySelector('[data-documents-status]')?.addEventListener('change', (event) => {
    state.statusFilter = event.currentTarget.value || 'all';
    renderLeft(state);
  });
  wrap.querySelector('[data-documents-tag]')?.addEventListener('change', (event) => {
    state.tagFilter = event.currentTarget.value || 'all';
    renderLeft(state);
  });
}

function openManageDocumentDrawer(state, record) {
  const body = document.createElement('div');
  body.className = 'drawer-body documents-action-drawer';
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <h2>Dokument verwalten</h2>
        <p>${escapeHtml(record.filename)} · ${escapeHtml(record.document_type === 'markdown_document' ? 'Markdown' : 'DOCX')}</p>
      </div>
      <button class="icon-button" type="button" data-documents-drawer-close aria-label="Schließen">×</button>
    </header>
    <form class="documents-drawer-form" data-documents-manage-form>
      <label>
        <span>Titel</span>
        <input name="title" value="${escapeHtml(record.title)}" placeholder="Dokumenttitel">
      </label>
      <label>
        <span>Status</span>
        <select name="status">
          ${documentStatusOptions(record.status)}
        </select>
      </label>
      <label>
        <span>Beschreibung</span>
        <textarea name="description" placeholder="Kurzbeschreibung für die Dokumentliste">${escapeHtml(documentDescription(record))}</textarea>
      </label>
      <label>
        <span>Tags</span>
        <input name="tags" value="${escapeHtml(documentTags(record).join(', '))}" placeholder="angebot, vertrag, kunde-a">
      </label>
      <div class="documents-drawer-actions documents-drawer-actions-three">
        <button type="button" data-documents-drawer-cancel>Abbrechen</button>
        <button class="documents-danger-button" type="button" data-documents-delete>Dokument löschen</button>
        <button type="submit">Speichern</button>
      </div>
    </form>
  `;
  wireDrawerClose(state, body);
  body.querySelector('[data-documents-delete]')?.addEventListener('click', async () => {
    const confirmed = window.confirm(`Dokument "${record.title}" löschen?`);
    if (!confirmed) return;
    await deleteDocument(state, record.id);
    state.ctx.closeDrawers();
  });
  body.querySelector('[data-documents-manage-form]')?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    await updateDocumentMetadata(state, record.id, {
      title: form.get('title')?.toString() || record.title,
      status: form.get('status')?.toString() || record.status,
      description: form.get('description')?.toString() || '',
      tags: form.get('tags')?.toString() || '',
    });
    state.ctx.closeDrawers();
  });
  state.ctx.openLeftDrawer(body);
}

async function deleteDocument(state, documentId) {
  const target = state.documents.find((record) => record.id === documentId);
  if (!target) return;

  if (state.selectedId === documentId) {
    await flushActiveSuperDocDraft(state, target, { allowFailure: true }).catch((error) => {
      console.warn('[documents] continuing delete after draft save failed', error);
    });
  }

  const doc = await state.ctx.db.raw.documents.findOne(documentId).exec();
  if (doc) {
    await doc.incrementalPatch({
      is_deleted: true,
      updated_at_ms: Date.now(),
    });
  }

  if (state.selectedId === documentId) {
    const next = state.documents.find((record) => record.id !== documentId && !record.is_deleted);
    state.selectedId = next?.id || '';
    state.selectedVersion = null;
    state.dirty = false;
  }

  await refreshDocuments(state);
  if (state.selectedId) {
    await loadSelectedVersion(state).catch((error) => {
      console.warn('[documents] selected replacement version load failed', error);
      state.selectedVersion = null;
    });
  }
  renderLeft(state);
  renderRight(state);
  renderCenter(state);
}

async function updateDocumentMetadata(state, documentId, input) {
  const sourceRecord = state.documents.find((record) => record.id === documentId);
  if (!sourceRecord) return;
  const patch = {
    title: sanitizeDocumentTitle(input.title || sourceRecord.title),
    status: normalizeDocumentStatus(input.status, sourceRecord.status),
    description: sanitizeDocumentDescription(input.description),
    tags: normalizeTags(input.tags),
    updated_at_ms: Date.now(),
  };
  const doc = await state.ctx.db.raw.documents.findOne(documentId).exec();
  if (doc) await doc.incrementalPatch(patch);
  Object.assign(sourceRecord, patch);
  state.selectedId = documentId;
  await refreshDocuments(state);
  renderLeft(state);
  renderRight(state);
}

function openNewDocumentDrawer(state) {
  const body = document.createElement('div');
  body.className = 'drawer-body documents-action-drawer';
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <h2>Neues Dokument</h2>
        <p>Markdown-Dokument anlegen und optional direkt ein CTOX Runbook mit Prompt starten.</p>
      </div>
      <button class="icon-button" type="button" data-documents-drawer-close aria-label="Schließen">×</button>
    </header>
    <form class="documents-drawer-form" data-documents-new-form>
      <label>
        <span>Titel</span>
        <input name="title" value="markdown-${new Date().toISOString().slice(0, 10)}" placeholder="Dokumenttitel">
      </label>
      <label>
        <span>Runbook</span>
        <select name="runbook">${runbookOptions(state, '', { includeNone: true })}</select>
      </label>
      <label>
        <span>Tags</span>
        <input name="tags" placeholder="angebot, vertrag, kunde-a">
      </label>
      <label>
        <span>Prompt</span>
        <textarea name="prompt" placeholder="Optionaler Startprompt für das neue Dokument oder Runbook"></textarea>
      </label>
      <button class="documents-knowledge-link" type="button" data-documents-open-knowledge>${iconSvg('knowledge')} CTOX Knowledge öffnen</button>
      <div class="documents-drawer-actions">
        <button type="button" data-documents-drawer-cancel>Abbrechen</button>
        <button type="submit">Dokument erstellen</button>
      </div>
    </form>
  `;
  wireDrawerClose(state, body);
  body.querySelector('[data-documents-open-knowledge]')?.addEventListener('click', () => openKnowledgeRunbooks(state));
  body.querySelector('[data-documents-new-form]')?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    await createMarkdownDocument(state, {
      title: form.get('title')?.toString() || '',
      runbookId: form.get('runbook')?.toString() || '',
      prompt: form.get('prompt')?.toString() || '',
      tags: form.get('tags')?.toString() || '',
    });
    state.ctx.closeDrawers();
  });
  state.ctx.openLeftDrawer(body);
}

function openImportDrawer(state) {
  const body = document.createElement('div');
  body.className = 'drawer-body documents-action-drawer';
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <h2>Dokument importieren</h2>
        <p>Datei auswählen, Importmodus festlegen und optional direkt ein Runbook anwenden.</p>
      </div>
      <button class="icon-button" type="button" data-documents-drawer-close aria-label="Schließen">×</button>
    </header>
    <form class="documents-drawer-form" data-documents-import-form>
      <label>
        <span>Datei</span>
        <input type="file" name="file" accept=".docx,.md,.markdown,application/vnd.openxmlformats-officedocument.wordprocessingml.document,text/markdown,text/plain">
      </label>
      <label>
        <span>Import-Modus</span>
        <select name="importMode" data-documents-import-mode>
          <option value="direct">1:1 übernehmen</option>
          <option value="runbook">Runbook direkt anwenden</option>
        </select>
      </label>
      <label>
        <span>Runbook</span>
        <select name="runbook" data-documents-runbook-select disabled>${runbookOptions(state, defaultRunbookId(state))}</select>
      </label>
      <label>
        <span>Tags</span>
        <input name="tags" placeholder="angebot, vertrag, kunde-a">
      </label>
      <label>
        <span>Prompt</span>
        <textarea name="prompt" data-documents-runbook-prompt disabled placeholder="Optionaler Prompt für das Runbook beim Import"></textarea>
      </label>
      <button class="documents-knowledge-link" type="button" data-documents-open-knowledge>${iconSvg('knowledge')} CTOX Knowledge öffnen</button>
      <div class="documents-drawer-actions">
        <button type="button" data-documents-drawer-cancel>Abbrechen</button>
        <button type="submit">Importieren</button>
      </div>
    </form>
  `;
  wireDrawerClose(state, body);
  body.querySelector('[data-documents-open-knowledge]')?.addEventListener('click', () => openKnowledgeRunbooks(state));
  body.querySelector('[data-documents-import-mode]')?.addEventListener('change', (event) => {
    const enabled = event.currentTarget.value === 'runbook';
    body.querySelector('[data-documents-runbook-select]').disabled = !enabled;
    body.querySelector('[data-documents-runbook-prompt]').disabled = !enabled;
  });
  body.querySelector('[data-documents-import-form]')?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const file = form.get('file');
    if (!(file instanceof File) || !file.name) {
      renderError(state, 'Bitte zuerst im Import-Dialog eine DOCX- oder Markdown-Datei auswählen.');
      return;
    }
    const importMode = form.get('importMode')?.toString() || 'direct';
    await importDocumentFile(state, file, {
      applyRunbook: importMode === 'runbook',
      runbookId: form.get('runbook')?.toString() || '',
      prompt: form.get('prompt')?.toString() || '',
      tags: form.get('tags')?.toString() || '',
      sourceAction: importMode === 'runbook' ? 'import_with_runbook' : 'direct_import',
    });
    state.ctx.closeDrawers();
  });
  state.ctx.openLeftDrawer(body);
}

function openExportDrawer(state) {
  const record = selectedRecord(state);
  const body = document.createElement('div');
  body.className = 'drawer-body documents-action-drawer';
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <h2>Dokument exportieren</h2>
        <p>${record ? escapeHtml(record.title) : 'Kein Dokument ausgewählt.'}</p>
      </div>
      <button class="icon-button" type="button" data-documents-drawer-close aria-label="Schließen">×</button>
    </header>
    <form class="documents-drawer-form" data-documents-export-form>
      <label>
        <span>Format</span>
        <select name="format" ${record ? '' : 'disabled'}>
          <option value="native">${record?.document_type === 'markdown_document' ? 'Markdown' : 'DOCX'} bearbeiten/exportieren</option>
        </select>
      </label>
      <label>
        <span>Dateiname</span>
        <input name="filename" value="${escapeHtml(record ? record.filename.replace(/\.(docx|md|markdown)$/i, '') + (record.document_type === 'markdown_document' ? '-edited.md' : '-edited.docx') : '')}" ${record ? '' : 'disabled'}>
      </label>
      <div class="documents-drawer-actions">
        <button type="button" data-documents-drawer-cancel>Abbrechen</button>
        <button type="submit" ${record ? '' : 'disabled'}>Export starten</button>
      </div>
    </form>
  `;
  wireDrawerClose(state, body);
  body.querySelector('[data-documents-export-form]')?.addEventListener('submit', async (event) => {
    event.preventDefault();
    await exportSelectedDocument(state, body.querySelector('[name="filename"]')?.value || '');
    state.ctx.closeDrawers();
  });
  state.ctx.openLeftDrawer(body);
}

function wireDrawerClose(state, body) {
  body.querySelector('[data-documents-drawer-close]')?.addEventListener('click', state.ctx.closeDrawers);
  body.querySelector('[data-documents-drawer-cancel]')?.addEventListener('click', state.ctx.closeDrawers);
}

function renderWorkflowPanel(state) {
  const panel = document.createElement('form');
  panel.className = 'documents-workflow';
  panel.dataset.documentsWorkflow = 'true';
  const flow = state.workflowPanel || {};
  const isImport = flow.mode === 'import';
  const importMode = flow.importMode || 'direct';
  panel.innerHTML = `
    <div class="documents-workflow-head">
      <strong>${isImport ? 'Dokument importieren' : 'Neues Markdown-Dokument'}</strong>
      <button class="documents-column-icon" type="button" aria-label="Schließen" title="Schließen" data-documents-workflow-close>${iconSvg('close')}</button>
    </div>
    ${isImport ? `
      <label class="documents-workflow-field">
        <span>Datei</span>
        <input type="file" accept=".docx,.md,.markdown,application/vnd.openxmlformats-officedocument.wordprocessingml.document,text/markdown,text/plain" data-documents-workflow-file>
      </label>
      <label class="documents-workflow-field">
        <span>Import-Modus</span>
        <select data-documents-import-mode>
          <option value="direct" ${importMode === 'direct' ? 'selected' : ''}>1:1 übernehmen</option>
          <option value="runbook" ${importMode === 'runbook' ? 'selected' : ''}>Runbook direkt anwenden</option>
        </select>
      </label>
    ` : `
      <label class="documents-workflow-field">
        <span>Titel</span>
        <input type="text" value="${escapeHtml(flow.title || '')}" placeholder="Dokumenttitel" data-documents-new-title>
      </label>
    `}
    <label class="documents-workflow-field" data-documents-runbook-field>
      <span>Runbook</span>
      <select data-documents-workflow-runbook ${isImport && importMode === 'direct' ? 'disabled' : ''}>
        ${runbookOptions(state, flow.runbookId)}
      </select>
    </label>
    <label class="documents-workflow-field">
      <span>Tags</span>
      <input type="text" value="${escapeHtml(flow.tags || '')}" placeholder="angebot, vertrag, kunde-a" data-documents-workflow-tags>
    </label>
    <label class="documents-workflow-field">
      <span>Prompt</span>
      <textarea data-documents-workflow-prompt ${isImport && importMode === 'direct' ? 'disabled' : ''} placeholder="${isImport ? 'Optionaler Prompt für das Runbook beim Import' : 'Optionaler Startprompt für Dokument und Runbook'}">${escapeHtml(flow.prompt || '')}</textarea>
    </label>
    <button class="documents-knowledge-link" type="button" data-documents-open-knowledge>${iconSvg('knowledge')} Runbooks verwalten</button>
    <div class="documents-workflow-actions">
      <button type="button" data-documents-workflow-cancel>Abbrechen</button>
      <button type="submit">${isImport ? 'Importieren' : 'Erstellen'}</button>
    </div>
  `;
  return panel;
}

function bindWorkflowControls(state, wrap) {
  const workflow = wrap.querySelector('[data-documents-workflow]');
  if (!workflow) return;
  workflow.querySelector('[data-documents-workflow-close]')?.addEventListener('click', () => {
    state.workflowPanel = null;
    renderLeft(state);
  });
  workflow.querySelector('[data-documents-workflow-cancel]')?.addEventListener('click', () => {
    state.workflowPanel = null;
    renderLeft(state);
  });
  workflow.querySelector('[data-documents-open-knowledge]')?.addEventListener('click', () => openKnowledgeRunbooks(state));
  workflow.querySelector('[data-documents-import-mode]')?.addEventListener('change', (event) => {
    state.workflowPanel.importMode = event.currentTarget.value || 'direct';
    state.workflowPanel.prompt = workflow.querySelector('[data-documents-workflow-prompt]')?.value || '';
    state.workflowPanel.runbookId = workflow.querySelector('[data-documents-workflow-runbook]')?.value || defaultRunbookId(state);
    state.workflowPanel.tags = workflow.querySelector('[data-documents-workflow-tags]')?.value || '';
    renderLeft(state);
  });
  workflow.querySelector('[data-documents-workflow-file]')?.addEventListener('change', (event) => {
    state.workflowPanel.file = event.currentTarget.files?.[0] || null;
  });
  workflow.addEventListener('submit', async (event) => {
    event.preventDefault();
    const flow = state.workflowPanel || {};
    const prompt = workflow.querySelector('[data-documents-workflow-prompt]')?.value || '';
    const runbookId = workflow.querySelector('[data-documents-workflow-runbook]')?.value || defaultRunbookId(state);
    const tags = workflow.querySelector('[data-documents-workflow-tags]')?.value || '';
    if (flow.mode === 'import') {
      const file = flow.file || workflow.querySelector('[data-documents-workflow-file]')?.files?.[0];
      if (!file) {
        renderError(state, 'Bitte zuerst eine DOCX- oder Markdown-Datei auswählen.');
        return;
      }
      await importDocumentFile(state, file, {
        applyRunbook: flow.importMode === 'runbook',
        runbookId,
        prompt,
        tags,
        sourceAction: flow.importMode === 'runbook' ? 'import_with_runbook' : 'direct_import',
      });
    } else {
      await createMarkdownDocument(state, {
        title: workflow.querySelector('[data-documents-new-title]')?.value || flow.title,
        runbookId,
        prompt,
        tags,
      });
    }
    state.workflowPanel = null;
    renderLeft(state);
  });
}

function documentTags(record) {
  return normalizeTags(record?.tags || []);
}

function documentDescription(record) {
  return sanitizeDocumentDescription(record?.description || record?.display_cache?.description || '');
}

function sanitizeDocumentDescription(value) {
  return String(value || '').replace(/\s+/g, ' ').trim().slice(0, 280);
}

function normalizeTags(value) {
  const raw = Array.isArray(value) ? value : String(value || '').split(/[,\n;]/);
  const seen = new Set();
  const tags = [];
  for (const item of raw) {
    const tag = String(item || '').replace(/\s+/g, ' ').trim();
    if (!tag) continue;
    const key = tag.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    tags.push(tag.slice(0, 40));
    if (tags.length >= 24) break;
  }
  return tags;
}

function renderTagPills(record) {
  const tags = documentTags(record);
  if (!tags.length) return '';
  return `<div class="documents-card-tags">${tags.map((tag) => `<span>${escapeHtml(tag)}</span>`).join('')}</div>`;
}

function tagFilterOptions(state) {
  const tags = [...new Set(state.documents.flatMap((record) => documentTags(record)))].sort((a, b) => a.localeCompare(b));
  return [
    `<option value="all" ${state.tagFilter === 'all' ? 'selected' : ''}>Alle Tags</option>`,
    `<option value="untagged" ${state.tagFilter === 'untagged' ? 'selected' : ''}>Ohne Tags</option>`,
    ...tags.map((tag) => `<option value="${escapeHtml(tag)}" ${state.tagFilter === tag ? 'selected' : ''}>${escapeHtml(tag)}</option>`),
  ].join('');
}

function normalizeDocumentStatus(value, fallback = 'Draft') {
  const allowed = new Set(['Imported', 'Draft', 'Review', 'Final']);
  const status = String(value || '').trim();
  return allowed.has(status) ? status : allowed.has(fallback) ? fallback : 'Draft';
}

function documentStatusOptions(selectedStatus) {
  return ['Imported', 'Draft', 'Review', 'Final']
    .map((status) => `<option value="${status}" ${selectedStatus === status ? 'selected' : ''}>${status}</option>`)
    .join('');
}

function visibleDocuments(state) {
  const query = state.searchQuery.trim().toLowerCase();
  const status = state.statusFilter;
  const tag = state.tagFilter;
  return [...state.documents]
    .filter((record) => {
      if (status !== 'all' && record.status !== status) return false;
      const tags = documentTags(record);
      if (tag === 'untagged' && tags.length) return false;
      if (tag !== 'all' && tag !== 'untagged' && !tags.includes(tag)) return false;
      if (!query) return true;
      return [record.title, record.filename, documentDescription(record), record.status, record.document_type, record.index_text, ...tags]
        .filter(Boolean)
        .some((value) => String(value).toLowerCase().includes(query));
    })
    .sort((a, b) => {
      if (state.sortBy === 'updated_asc') return (a.updated_at_ms || 0) - (b.updated_at_ms || 0);
      if (state.sortBy === 'title_asc') return String(a.title || '').localeCompare(String(b.title || ''));
      if (state.sortBy === 'status') return String(a.status || '').localeCompare(String(b.status || '')) || String(a.title || '').localeCompare(String(b.title || ''));
      return (b.updated_at_ms || 0) - (a.updated_at_ms || 0);
    });
}

function renderRight(state) {
  const record = selectedRecord(state);
  const selectedRunbook = defaultRunbookId(state);
  const wrap = document.createElement('div');
  wrap.className = 'documents-runbooks';
  wrap.innerHTML = `
    <div class="documents-panel-title"><span>Runbooks</span><strong>${record ? escapeHtml(record.document_type || 'word') : 'none'}</strong></div>
    <form class="documents-runbook-form" data-documents-runbook-form>
      <select data-documents-runbook>
        ${runbookOptions(state, selectedRunbook)}
      </select>
      <textarea data-documents-prompt placeholder="Prompt für dieses Dokument"></textarea>
      <button type="submit" ${record ? '' : 'disabled'}>Runbook starten</button>
      <button class="documents-knowledge-link" type="button" data-documents-open-knowledge>${iconSvg('knowledge')} Runbooks verwalten</button>
    </form>
  `;
  wrap.querySelector('[data-documents-runbook-form]')?.addEventListener('submit', async (event) => {
    event.preventDefault();
    if (!record) return;
    const runbook = wrap.querySelector('[data-documents-runbook]')?.value || defaultRunbookId(state);
    const prompt = wrap.querySelector('[data-documents-prompt]')?.value || '';
    await dispatchDocumentRunbook(state, {
      record,
      versionId: record.current_version_id,
      runbookId: runbook,
      prompt,
      sourceAction: 'manual_runbook',
    });
  });
  wrap.querySelector('[data-documents-open-knowledge]')?.addEventListener('click', () => openKnowledgeRunbooks(state));
  state.ctx.right.replaceChildren(wrap);
}

async function dispatchDocumentRunbook(state, input) {
  const runbookId = input.runbookId || defaultRunbookId(state);
  if (!runbookId && !String(input.prompt || '').trim()) return null;
  const runbook = state.runbooks.find((item) => item.id === runbookId || item.command_type === runbookId);
  return state.ctx.commandBus.dispatch({
    module: 'documents',
    type: runbook?.command_type || runbookId || 'document.summarize',
    record_id: input.record.id,
    payload: {
      document_id: input.record.id,
      version_id: input.versionId || input.record.current_version_id,
      prompt: input.prompt || '',
      runbook_id: runbook?.id || runbookId,
      prompt_template: runbook?.prompt_template || '',
      source_action: input.sourceAction || 'document_runbook',
    },
    client_context: {
      surface: 'business-os-documents',
      filename: input.record.filename,
      document_type: input.record.document_type,
      action: input.sourceAction || 'document_runbook',
    },
  });
}

function runbookOptions(state, selectedId = '', options = {}) {
  const runbooks = state.runbooks.length
    ? state.runbooks
    : [
        { id: 'document.summarize', command_type: 'document.summarize', title: 'Zusammenfassen' },
        { id: 'document.extract-requirements', command_type: 'document.extract-requirements', title: 'Requirements extrahieren' },
        { id: 'document.review-risks', command_type: 'document.review-risks', title: 'Risiken prüfen' },
      ];
  const optionHtml = runbooks.map((runbook) => {
    const value = runbook.id || runbook.command_type;
    const label = runbook.title || runbook.command_type || value;
    return `<option value="${escapeHtml(value)}" ${value === selectedId || runbook.command_type === selectedId ? 'selected' : ''}>${escapeHtml(label)}</option>`;
  }).join('');
  return options.includeNone ? `<option value="">Kein Runbook</option>${optionHtml}` : optionHtml;
}

function defaultRunbookId(state) {
  return state.runbooks[0]?.id || state.runbooks[0]?.command_type || 'document.summarize';
}

async function openKnowledgeRunbooks(state) {
  const result = await state.ctx.commandBus.dispatch({
    module: 'ctox',
    type: 'ctox.knowledge.runbooks.manage',
    payload: {
      title: 'Document runbooks verwalten',
      instruction: 'Öffne das CTOX Knowledge-System für die Verwaltung von dokumentbezogenen Skillbooks, Runbooks und Runbook-Items. Fokus: document/docx/markdown Runbooks, die vom Business-OS Documents-Modul beim Erstellen, Importieren und manuellen Ausführen verwendet werden.',
      knowledge_scope: {
        form: 'procedural',
        cli_namespace: 'ctox knowledge skill',
        related_tables: ['knowledge_main_skills', 'knowledge_skillbooks', 'knowledge_runbooks', 'knowledge_runbook_items'],
        module_local_seed_collection: 'document_runbooks',
      },
      current_document_runbooks: state.runbooks.map((runbook) => ({
        id: runbook.id,
        command_type: runbook.command_type,
        title: runbook.title,
        prompt_template: runbook.prompt_template,
      })),
    },
    client_context: {
      module: 'documents',
      surface: 'documents-runbook-navigation',
      target: 'ctox-knowledge-system',
    },
  });
  sessionStorage.setItem('ctox.businessOs.focusTask', JSON.stringify({
    commandId: result.command_id || '',
    taskId: result.task_id || '',
    taskStatus: result.task_status || result.status || 'queued',
  }));
  location.hash = 'ctox?focus=knowledge-runbooks';
  document.querySelector('[data-module="ctox"]')?.click();
}

function renderCenter(state) {
  const host = state.ctx.host.querySelector('[data-documents-editor]');
  const record = selectedRecord(state);
  const renderSerial = (state.renderSerial || 0) + 1;
  state.renderSerial = renderSerial;
  if (state.superdocSaveTimer) {
    clearTimeout(state.superdocSaveTimer);
    state.superdocSaveTimer = null;
  }
  try {
    state.editorHandle?.destroy?.();
  } catch (error) {
    console.warn('[documents] previous editor destroy failed', error);
  }
  state.editorHandle = null;

  if (!record) {
    host.innerHTML = '<div class="documents-empty"><strong>Kein Dokument ausgewählt</strong><span>Links ein DOCX importieren oder auswählen.</span></div>';
    return;
  }
  const version = state.selectedVersion;
  if (!version) {
    host.innerHTML = '<div class="documents-loading"><strong>Lade Dokument</strong><span>Version wird gelesen.</span></div>';
    loadSelectedVersion(state)
      .then((loadedVersion) => {
        if (state.renderSerial !== renderSerial) return;
        if (loadedVersion) {
          renderCenter(state);
          return;
        }
        renderError(state, 'Zu diesem Dokument wurde keine gespeicherte Version gefunden. Bitte erneut importieren oder den Datensatz verwalten.');
      })
      .catch((error) => {
        if (state.renderSerial !== renderSerial) return;
        renderError(state, `Dokumentversion konnte nicht geladen werden: ${error?.message || error}`);
      });
    return;
  }
  if (record.document_type === 'word_document') {
    host.innerHTML = '<div class="documents-loading"><strong>Lade DOCX Editor</strong><span>SuperDoc wird initialisiert.</span></div>';
    mountSuperDocDocument(state, host, record, version, renderSerial).catch((error) => {
      if (state.renderSerial !== renderSerial) return;
      console.error('[documents] SuperDoc mount failed', error);
      renderError(state, `DOCX editor konnte nicht geladen werden: ${error?.message || error}`);
    });
    return;
  }

  host.innerHTML = '<div class="documents-loading"><strong>Lade Editor</strong><span>Dokument wird vorbereitet.</span></div>';
  ensureDocumentFormatModule(state).then((formatModule) => {
    if (state.renderSerial !== renderSerial) return;
    mountMarkdownDocument(state, host, version, formatModule);
  }).catch((error) => {
    if (state.renderSerial !== renderSerial) return;
    renderError(state, `Editor konnte nicht geladen werden: ${error?.message || error}`);
  });
}

function mountMarkdownDocument(state, host, version, formatModule) {
  host.replaceChildren();
  const wrap = document.createElement('div');
  wrap.className = 'documents-markdown-editor';
  const textarea = document.createElement('textarea');
  textarea.className = 'documents-markdown-textarea';
  textarea.value = formatModule.exportMarkdown(version.model_json);
  textarea.spellcheck = true;
  wrap.append(textarea);
  host.append(wrap);

  let saveSerial = 0;
  textarea.addEventListener('input', () => {
    const parsed = formatModule.importMarkdown(textarea.value);
    const document = parsed.document;
    const serial = ++saveSerial;
    state.dirty = true;
    version.model_json = document;
    saveDraftVersion(state, document).catch((error) => {
      if (serial === saveSerial) console.error('[documents] Markdown draft save failed', error);
    });
  });

  state.editorHandle = {
    kind: 'markdown',
    destroy() {
      host.replaceChildren();
    },
    focus() {
      textarea.focus();
    },
  };
  textarea.focus();
}

async function mountSuperDocDocument(state, host, record, version, renderSerial) {
  const bytes = await loadBlobBytes(state.ctx, version.blob_id);
  if (state.renderSerial !== renderSerial) return;
  if (!bytes) throw new Error(`Missing DOCX blob ${version.blob_id}`);
  const { SuperDoc } = await loadSuperDocModule(state);
  if (state.renderSerial !== renderSerial) return;
  host.replaceChildren();
  const frame = document.createElement('div');
  frame.className = 'documents-superdoc-frame';
  frame.dataset.toolbarVisible = String(state.docxToolbarVisible);
  const controls = document.createElement('div');
  controls.className = 'documents-superdoc-controls';
  const toolbarToggle = document.createElement('button');
  toolbarToggle.type = 'button';
  toolbarToggle.className = 'documents-superdoc-toolbar-toggle';
  toolbarToggle.dataset.documentsToolbarToggle = 'true';
  toolbarToggle.textContent = state.docxToolbarVisible ? 'Editorleiste ausblenden' : 'Editorleiste einblenden';
  toolbarToggle.setAttribute('aria-pressed', String(state.docxToolbarVisible));
  toolbarToggle.setAttribute('aria-label', toolbarToggle.textContent);
  toolbarToggle.addEventListener('click', () => {
    state.docxToolbarVisible = !state.docxToolbarVisible;
    localStorage.setItem(DOCX_TOOLBAR_VISIBILITY_KEY, String(state.docxToolbarVisible));
    frame.dataset.toolbarVisible = String(state.docxToolbarVisible);
    toolbarToggle.textContent = state.docxToolbarVisible ? 'Editorleiste ausblenden' : 'Editorleiste einblenden';
    toolbarToggle.setAttribute('aria-pressed', String(state.docxToolbarVisible));
    toolbarToggle.setAttribute('aria-label', toolbarToggle.textContent);
  });
  controls.append(toolbarToggle);
  const toolbar = document.createElement('div');
  const toolbarId = `documents_superdoc_toolbar_${record.id.replace(/[^a-zA-Z0-9_-]/g, '_')}`;
  toolbar.id = toolbarId;
  toolbar.className = 'documents-superdoc-toolbar';
  const ruler = document.createElement('div');
  const rulerId = `documents_superdoc_ruler_${record.id.replace(/[^a-zA-Z0-9_-]/g, '_')}`;
  ruler.id = rulerId;
  ruler.className = 'documents-superdoc-ruler';
  const editorHost = document.createElement('div');
  editorHost.className = 'documents-superdoc-editor';
  const mount = document.createElement('div');
  mount.className = 'documents-superdoc-mount';
  editorHost.append(mount);
  frame.append(controls, toolbar, ruler, editorHost);
  host.append(frame);
  const file = new File([bytes], record.filename || 'document.docx', { type: DOCX_MIME });
  if (state.renderSerial !== renderSerial) return;
  const superdoc = new SuperDoc({
    selector: mount,
    document: file,
    documentMode: 'editing',
    role: 'editor',
    contained: true,
    pagination: true,
    toolbar: `#${toolbarId}`,
    comments: { visible: false },
    rulers: true,
    rulerContainer: `#${rulerId}`,
    viewOptions: { layout: 'print' },
    useLayoutEngine: true,
    layoutEngineOptions: {
      virtualization: {
        enabled: false,
      },
    },
    user: { name: 'Business OS', email: 'business-os@local' },
    modules: {
      toolbar: {
        selector: toolbarId,
        toolbarGroups: ['left', 'center', 'right'],
        hideButtons: false,
        responsiveToContainer: true,
      },
      comments: false,
      collaboration: false,
      whiteboard: false,
      surfaces: { findReplace: true },
    },
    telemetry: { enabled: false },
    onEditorUpdate: async () => {
      state.dirty = true;
      state.needsFinalSave = true;
      await markRecordDraft(state, record);
      scheduleSuperDocDraftSave(state, record);
    },
    onException: (event) => {
      console.error('[documents] SuperDoc exception', event);
    },
  });
  if (state.renderSerial !== renderSerial) {
    superdoc.destroy?.();
    return;
  }
  state.editorHandle = {
    kind: 'superdoc',
    superdoc,
    destroy() {
      superdoc.destroy?.();
      host.replaceChildren();
    },
    async export() {
      return superdoc.export({ triggerDownload: false, isFinalDoc: false });
    },
    focus() {
      host.querySelector('[contenteditable="true"]')?.focus();
    },
  };
}

function scheduleSuperDocDraftSave(state, record) {
  if (state.superdocSaveTimer) clearTimeout(state.superdocSaveTimer);
  state.superdocSaveTimer = setTimeout(() => {
    state.superdocSaveTimer = null;
    flushActiveSuperDocDraft(state, record, { final: false }).catch((error) => {
      console.error('[documents] SuperDoc draft save failed', error);
    });
  }, 900);
}

async function flushActiveSuperDocDraft(state, record = selectedRecord(state), options = {}) {
  if (state.superdocSaveTimer) {
    clearTimeout(state.superdocSaveTimer);
    state.superdocSaveTimer = null;
  }
  if (state.superdocSavePromise) {
    try {
      const result = await state.superdocSavePromise;
      if (!options.force) return result;
    } catch (error) {
      if (!options.allowFailure) throw error;
      console.warn('[documents] ignored pending SuperDoc draft save failure', error);
      if (!options.force) return null;
    }
  }
  if (state.editorHandle?.kind !== 'superdoc' || !record || !state.selectedVersion || (!state.dirty && !options.force)) return null;
  const editorHandle = state.editorHandle;
  const version = state.selectedVersion;
  const versionId = version.id;
  const recordId = record.id;
  state.superdocSavePromise = (async () => {
    const exported = await editorHandle.export();
    const bytes = await exportDataToUint8(exported);
    const now = Date.now();
    const blobId = `${versionId}_draft_${now}`;
    await saveBlobChunks(state.ctx, {
      blobId,
      documentId: recordId,
      versionId,
      mimeType: DOCX_MIME,
      bytes,
    });
    const versionDoc = await state.ctx.db.raw.document_versions.findOne(versionId).exec();
    await versionDoc?.incrementalPatch({
      source_kind: 'edited_docx',
      blob_id: blobId,
      updated_at_ms: now,
    });
    const recordDoc = await state.ctx.db.raw.documents.findOne(recordId).exec();
    await recordDoc?.incrementalPatch({
      status: 'Draft',
      updated_at_ms: now,
    });
    version.blob_id = blobId;
    version.source_kind = 'edited_docx';
    version.updated_at_ms = now;
    const localRecord = state.documents.find((item) => item.id === recordId) || record;
    localRecord.status = 'Draft';
    localRecord.updated_at_ms = now;
    if (state.selectedVersion?.id === versionId) {
      state.selectedVersion.blob_id = blobId;
      state.selectedVersion.source_kind = 'edited_docx';
      state.selectedVersion.updated_at_ms = now;
      state.dirty = false;
      if (options.final !== false) state.needsFinalSave = false;
    }
    return blobId;
  })();
  try {
    return await state.superdocSavePromise;
  } catch (error) {
    if (!options.allowFailure) throw error;
    console.warn('[documents] ignored SuperDoc draft save failure', error);
    return null;
  } finally {
    state.superdocSavePromise = null;
  }
}

async function markRecordDraft(state, record) {
  const now = Date.now();
  const recordDoc = await state.ctx.db.raw.documents.findOne(record.id).exec();
  await recordDoc?.incrementalPatch({
    status: 'Draft',
    updated_at_ms: now,
  });
  record.status = 'Draft';
  record.updated_at_ms = now;
}

async function saveDraftVersion(state, document) {
  const record = selectedRecord(state);
  if (!record || !state.selectedVersion) return;
  const formatModule = await ensureDocumentFormatModule(state);
  const now = Date.now();
  const indexText = formatModule.getDocumentText(document).slice(0, 20000);
  const versionDoc = await state.ctx.db.raw.document_versions.findOne(state.selectedVersion.id).exec();
  await versionDoc?.incrementalPatch({
    model_json: document,
    updated_at_ms: now,
  });
  const recordDoc = await state.ctx.db.raw.documents.findOne(record.id).exec();
  await recordDoc?.incrementalPatch({
    status: 'Draft',
    index_text: indexText,
    updated_at_ms: now,
  });
  record.status = 'Draft';
  record.index_text = indexText;
  record.updated_at_ms = now;
}

async function exportSelectedDocument(state, requestedFilename = '') {
  const record = selectedRecord(state);
  if (!record || !state.selectedVersion) return;
  const formatModule = await ensureDocumentFormatModule(state);
  const isMarkdown = record.document_type === 'markdown_document' || record.mime_type === MARKDOWN_MIME;
  let data;
  if (isMarkdown) {
    data = formatModule.exportMarkdown(state.selectedVersion.model_json);
  } else if (state.editorHandle?.kind === 'superdoc') {
    data = await state.editorHandle.export();
  } else {
    renderError(state, 'DOCX Export benötigt den aktiven SuperDoc Editor. Bitte das Dokument erneut öffnen und danach exportieren.');
    return;
  }
  const blob = data instanceof Blob ? data : new Blob([data], { type: isMarkdown ? MARKDOWN_MIME : DOCX_MIME });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = sanitizeExportFilename(requestedFilename, isMarkdown)
    || record.filename.replace(/\.(docx|md|markdown)$/i, '') + (isMarkdown ? '-edited.md' : '-edited.docx');
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

async function saveBlobChunks(ctx, input) {
  requireDocumentPersistence(ctx);
  const base64 = uint8ToBase64(input.bytes);
  const total = Math.ceil(base64.length / CHUNK_SIZE) || 1;
  for (let idx = 0; idx < total; idx += 1) {
    await ctx.db.raw.document_blob_chunks.insert({
      id: `${input.blobId}_${idx}`,
      blob_id: input.blobId,
      document_id: input.documentId,
      version_id: input.versionId,
      idx,
      total,
      mime_type: input.mimeType,
      encoding: 'base64',
      data: base64.slice(idx * CHUNK_SIZE, (idx + 1) * CHUNK_SIZE),
      created_at_ms: Date.now(),
    });
  }
}

async function loadBlobBytes(ctx, blobId) {
  requireDocumentPersistence(ctx);
  if (!blobId) return null;
  const chunks = await ctx.db.raw.document_blob_chunks.find({
    selector: { blob_id: blobId },
    sort: [{ idx: 'asc' }],
  }).exec();
  if (!chunks.length) return null;
  const base64 = chunks.map((chunk) => chunk.toJSON().data || '').join('');
  return base64ToUint8(base64);
}

function requireDocumentPersistence(ctx) {
  const raw = ctx?.db?.raw;
  if (!raw?.documents || !raw?.document_versions || !raw?.document_blob_chunks) {
    throw new Error('CTOX document persistence is unavailable. Document bytes must be stored through CTOX collections, not local files.');
  }
}

async function ensureSeedRunbooks(ctx) {
  const collection = ctx.db?.raw?.document_runbooks;
  if (!collection) return;
  const existing = await collection.find().exec();
  if (existing.length) return;
  const now = Date.now();
  const runbooks = [
    ['document.summarize', 'Zusammenfassen', 'Fasse das ausgewählte DOCX strukturiert zusammen.'],
    ['document.extract-requirements', 'Requirements extrahieren', 'Extrahiere Anforderungen, offene Punkte und Nachweise.'],
    ['document.review-risks', 'Risiken prüfen', 'Prüfe fachliche, rechtliche und Umsetzungsrisiken im Dokument.'],
  ];
  for (const [id, title, prompt] of runbooks) {
    await collection.insert({
      id,
      document_type: 'word_document',
      title,
      description: prompt,
      command_type: id,
      prompt_template: prompt,
      created_at_ms: now,
      updated_at_ms: now,
    });
  }
}

function selectedRecord(state) {
  return state.documents.find((record) => record.id === state.selectedId) || null;
}

function withTimeout(promise, ms, message) {
  let timer;
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(message || `Zeitlimit nach ${ms} ms erreicht.`)), ms);
  });
  return Promise.race([
    Promise.resolve(promise).finally(() => clearTimeout(timer)),
    timeout,
  ]);
}

function clampNumber(value, min, max) {
  return Math.max(min, Math.min(max, value));
}

function renderError(state, message) {
  const host = state.ctx.host.querySelector('[data-documents-editor]');
  if (!host) return;
  host.innerHTML = `<div class="documents-error"><strong>Dokumentfehler</strong><span>${escapeHtml(message)}</span></div>`;
}

function isSupportedDocumentFile(file) {
  return isDocxFile(file) || isMarkdownFile(file);
}

function isDocxFile(file) {
  return /\.docx$/i.test(file.name) || file.type === DOCX_MIME;
}

function isMarkdownFile(file) {
  return /\.(md|markdown)$/i.test(file.name) || file.type === MARKDOWN_MIME || file.type === 'text/plain';
}

function iconSvg(name) {
  const paths = {
    new: 'M11 5h2v6h6v2h-6v6h-2v-6H5v-2h6V5Z',
    import: 'M11 4h2v8.2l2.9-2.9 1.4 1.4L12 16l-5.3-5.3 1.4-1.4 2.9 2.9V4Zm-5 13h2v1h8v-1h2v3H6v-3Z',
    export: 'M11 20h2v-8.2l2.9 2.9 1.4-1.4L12 8l-5.3 5.3 1.4 1.4 2.9-2.9V20ZM6 4v3h2V6h8v1h2V4H6Z',
    close: 'm6.4 5 5.6 5.6L17.6 5 19 6.4 13.4 12l5.6 5.6-1.4 1.4-5.6-5.6L6.4 19 5 17.6l5.6-5.6L5 6.4 6.4 5Z',
    knowledge: 'M5 4.5A2.5 2.5 0 0 1 7.5 2H19v16H7.5A2.5 2.5 0 0 0 5 20.5v-16Zm2.5-.5a.5.5 0 0 0-.5.5v12.55c.17-.03.34-.05.5-.05H17V4H7.5ZM8 7h7v2H8V7Zm0 4h6v2H8v-2Z',
    settings: 'M19.4 13.5c.1-.5.1-1 .1-1.5s0-1-.1-1.5l2-1.5-2-3.5-2.4 1a8 8 0 0 0-2.6-1.5L14 2h-4l-.4 2.5A8 8 0 0 0 7 6L4.6 5l-2 3.5 2 1.5A8 8 0 0 0 4.5 12c0 .5 0 1 .1 1.5l-2 1.5 2 3.5 2.4-1a8 8 0 0 0 2.6 1.5L10 22h4l.4-2.5A8 8 0 0 0 17 18l2.4 1 2-3.5-2-1.5ZM12 15.5A3.5 3.5 0 1 1 12 8a3.5 3.5 0 0 1 0 7.5Z',
  };
  return `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="${paths[name] || paths.new}"/></svg>`;
}

function titleFromFilename(filename) {
  return filename.replace(/\.(docx|md|markdown)$/i, '').trim() || 'Untitled document';
}

function sanitizeDocumentTitle(value) {
  return String(value || '').replace(/\s+/g, ' ').trim() || 'Neues Dokument';
}

function slugFilename(value) {
  const base = sanitizeDocumentTitle(value)
    .toLowerCase()
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
  return base || `document-${Date.now()}`;
}

function ensureExtension(filename, extension) {
  return filename.toLowerCase().endsWith(extension) ? filename : `${filename}${extension}`;
}

function sanitizeExportFilename(value, isMarkdown) {
  const trimmed = String(value || '').trim();
  if (!trimmed) return '';
  const extension = isMarkdown ? '.md' : '.docx';
  const withoutUnsafePath = trimmed.split(/[\\/]/).pop() || '';
  return ensureExtension(withoutUnsafePath.replace(/[\r\n\t]+/g, ' ').trim(), extension);
}

async function sha256Hex(bytes) {
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

function uint8ToBase64(bytes) {
  let binary = '';
  for (let index = 0; index < bytes.length; index += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(index, index + 0x8000));
  }
  return btoa(binary);
}

function base64ToUint8(base64) {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

async function exportDataToUint8(data) {
  if (data instanceof Uint8Array) return data;
  if (data instanceof ArrayBuffer) return new Uint8Array(data);
  if (data instanceof Blob) return new Uint8Array(await data.arrayBuffer());
  if (ArrayBuffer.isView(data)) return new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
  throw new Error('Unsupported SuperDoc export payload');
}

async function ensureStyles() {
  if (document.querySelector('link[data-documents-style]')) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = new URL('./index.css', import.meta.url).href;
  link.dataset.documentsStyle = 'true';
  document.head.append(link);
}

function ensureSuperDocStyles() {
  if (document.querySelector('link[data-superdoc-style]')) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = new URL('../../vendor/superdoc.css', import.meta.url).href;
  link.dataset.superdocStyle = 'true';
  document.head.append(link);
}

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}
