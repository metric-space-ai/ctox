import {
  canModifyBusinessModule,
  canViewBusinessModuleSource,
} from '../../shared/permissions.js';

export const manifest = {
  id: 'code-editor',
  title: 'Source Editor',
  glyph: '⌘',
  defaultWidth: 1080,
  defaultHeight: 700,
};

let monacoPromise;

export async function mount(container, ctx) {
  ensureStyles();
  const state = {
    moduleId: ctx.args?.moduleId || '',
    moduleTitle: ctx.args?.moduleTitle || ctx.args?.moduleId || 'Module',
    modules: [],
    files: [],
    activePath: '',
    search: '',
    saving: false,
    loading: false,
    loadingModules: false,
    readonly: false,
    monaco: null,
    editor: null,
    model: null,
    usingMonaco: false,
    diffOpen: false,
    sourceDenied: false,
  };

  container.innerHTML = `
    <section class="source-editor" data-source-editor>
      <aside class="source-editor-sidebar">
        <div class="source-editor-sidebar-head">
          <strong data-source-module-title>${escapeHtml(state.moduleTitle)}</strong>
          <span data-source-module-id>${escapeHtml(state.moduleId || 'Modul auswählen')}</span>
        </div>
        <div class="source-editor-module-list" data-source-module-list aria-label="Business-OS Apps"></div>
        <label class="source-editor-search">
          <span aria-hidden="true">⌕</span>
          <input data-source-search type="search" placeholder="Dateien suchen" autocomplete="off">
        </label>
        <nav data-source-file-list aria-label="Source files"></nav>
      </aside>
      <main class="source-editor-main">
        <header class="source-editor-toolbar">
          <div class="source-editor-file-meta">
            <strong data-source-active-file>Source</strong>
            <span data-source-file-detail></span>
          </div>
          <div class="source-editor-actions">
            <button type="button" data-source-open-app aria-label="App öffnen" title="App öffnen"><span aria-hidden="true">↗</span><span>App</span></button>
            <button type="button" data-source-diff aria-label="Diff anzeigen" title="Diff anzeigen"><span aria-hidden="true">⇄</span><span>Diff</span></button>
            <button type="button" data-source-format aria-label="Datei formatieren" title="Datei formatieren"><span aria-hidden="true">{}</span><span>Format</span></button>
            <button type="button" data-source-revert aria-label="Änderungen verwerfen" title="Änderungen verwerfen"><span aria-hidden="true">↶</span><span>Revert</span></button>
            <button type="button" data-source-reload aria-label="Neu laden" title="Neu laden"><span aria-hidden="true">↻</span><span>Laden</span></button>
            <button type="button" data-source-save aria-label="Speichern" title="Speichern"><span aria-hidden="true">✓</span><span>Speichern</span></button>
          </div>
        </header>
        <div class="source-editor-workbench" data-source-workbench>
          <div class="source-editor-monaco" data-source-monaco></div>
          <div class="source-editor-fallback" data-source-fallback hidden>
            <div class="source-editor-lines" data-source-lines aria-hidden="true">1</div>
            <textarea data-source-code spellcheck="false" autocomplete="off" autocorrect="off" autocapitalize="off"></textarea>
          </div>
          <div class="source-editor-placeholder" data-source-placeholder>
            <strong>Modul auswählen</strong>
            <span>Wähle links eine Business-OS App, um ihre Source-Dateien zu laden.</span>
          </div>
          <aside class="source-editor-diff" data-source-diff-panel hidden></aside>
        </div>
        <footer class="source-editor-status" data-source-status>Lade Source...</footer>
      </main>
    </section>
  `;

  const refs = {
    moduleTitle: container.querySelector('[data-source-module-title]'),
    moduleId: container.querySelector('[data-source-module-id]'),
    moduleList: container.querySelector('[data-source-module-list]'),
    fileList: container.querySelector('[data-source-file-list]'),
    search: container.querySelector('[data-source-search]'),
    activeFile: container.querySelector('[data-source-active-file]'),
    fileDetail: container.querySelector('[data-source-file-detail]'),
    code: container.querySelector('[data-source-code]'),
    lines: container.querySelector('[data-source-lines]'),
    status: container.querySelector('[data-source-status]'),
    save: container.querySelector('[data-source-save]'),
    reload: container.querySelector('[data-source-reload]'),
    revert: container.querySelector('[data-source-revert]'),
    format: container.querySelector('[data-source-format]'),
    openApp: container.querySelector('[data-source-open-app]'),
    diff: container.querySelector('[data-source-diff]'),
    diffPanel: container.querySelector('[data-source-diff-panel]'),
    monacoHost: container.querySelector('[data-source-monaco]'),
    fallback: container.querySelector('[data-source-fallback]'),
    placeholder: container.querySelector('[data-source-placeholder]'),
  };

  refs.search.addEventListener('input', () => {
    state.search = refs.search.value.trim();
    renderFileList();
  });
  refs.code.addEventListener('input', () => {
    setActiveDirty(true);
    updateLineNumbers();
    renderStatus();
    updateActionState();
    if (state.diffOpen) renderDiff();
  });
  refs.code.addEventListener('scroll', () => {
    refs.lines.scrollTop = refs.code.scrollTop;
  });
  refs.code.addEventListener('keydown', (event) => {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 's') {
      event.preventDefault();
      saveActiveFile();
      return;
    }
    if (event.key === 'Tab') {
      event.preventDefault();
      insertAtCursor(refs.code, '  ');
    }
  });
  refs.save.addEventListener('click', saveActiveFile);
  refs.format.addEventListener('click', formatActiveFile);
  refs.revert.addEventListener('click', revertActiveFile);
  refs.reload.addEventListener('click', loadBundle);
  refs.diff.addEventListener('click', () => {
    if (!activeFile()) return;
    state.diffOpen = !state.diffOpen;
    refs.diffPanel.hidden = !state.diffOpen;
    refs.diff.classList.toggle('is-active', state.diffOpen);
    refs.diff.setAttribute('aria-pressed', state.diffOpen ? 'true' : 'false');
    renderDiff();
    state.editor?.layout?.();
  });
  refs.openApp.addEventListener('click', () => {
    if (state.moduleId) location.hash = `#${encodeURIComponent(state.moduleId)}`;
  });

  const monacoReady = initMonaco().catch((error) => {
    console.warn('[source-editor] Monaco unavailable, using textarea fallback:', error);
    refs.monacoHost.hidden = true;
    refs.fallback.hidden = false;
    setStatus('Monaco konnte nicht geladen werden. Texteditor-Fallback aktiv.', true);
  });

  await loadModuleCatalog();
  if (state.moduleId) {
    await loadBundle();
  } else if (state.sourceDenied) {
    clearEditor('Source nicht freigegeben', 'Diese App kann mit deinem Zugriff nicht geöffnet werden.');
    setStatus('Source ist fuer diese App nicht freigegeben.', true);
    updateActionState();
  } else {
    clearEditor('Modul auswählen', 'Wähle links eine Business-OS App, um ihre Source-Dateien zu laden.');
    setStatus(moduleEmptyStatus());
    updateActionState();
  }
  await monacoReady;
  if (state.files.length) openFile(state.activePath);

  async function initMonaco() {
    const monaco = await loadMonaco();
    if (!container.isConnected) return;
    state.monaco = monaco;
    state.editor = monaco.editor.create(refs.monacoHost, {
      value: '',
      language: 'javascript',
      theme: 'business-os-dark',
      automaticLayout: true,
      minimap: { enabled: false },
      fontSize: 12,
      lineHeight: 20,
      tabSize: 2,
      insertSpaces: true,
      scrollBeyondLastLine: false,
      roundedSelection: false,
      renderLineHighlight: 'gutter',
      overviewRulerBorder: false,
      padding: { top: 12, bottom: 12 },
    });
    state.usingMonaco = true;
    refs.fallback.hidden = true;
    refs.monacoHost.hidden = false;
    state.editor.onDidChangeModelContent(() => {
      setActiveDirty(true);
      renderStatus();
      updateActionState();
      if (state.diffOpen) renderDiff();
    });
    state.editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, saveActiveFile);
    if (state.activePath) setEditorValue(currentFileValue(), activeFile()?.language || 'text', state.activePath);
    renderStatus();
    updateActionState();
  }

  async function loadModuleCatalog() {
    state.loadingModules = true;
    renderModuleList();
    try {
      const modules = Array.isArray(ctx.modules) && ctx.modules.length
        ? ctx.modules
        : await fetchModuleCatalog();
      state.modules = normalizeModuleCatalog(modules, {
        session: ctx.session,
        governance: ctx.governance,
        requireSourceView: true,
      });
      const selected = state.modules.find((entry) => entry.id === state.moduleId);
      if (selected) {
        state.moduleTitle = selected.title;
      } else if (state.moduleId) {
        state.sourceDenied = true;
        state.moduleId = '';
        state.moduleTitle = 'Module';
        setStatus('Source ist fuer diese App nicht freigegeben.', true);
      }
      renderModuleHeader();
    } catch (error) {
      console.warn('[source-editor] module catalog unavailable:', error);
      state.modules = state.moduleId ? [{ id: state.moduleId, title: state.moduleTitle }] : [];
      renderModuleHeader();
      if (!state.moduleId) setStatus(`Modulkatalog nicht verfügbar: ${error?.message || error}`, true);
    } finally {
      state.loadingModules = false;
      renderModuleList();
    }
  }

  async function selectModule(module) {
    if (!module?.id || module.id === state.moduleId) return;
    persistActiveDraft();
    state.moduleId = module.id;
    state.moduleTitle = module.title || module.id;
    state.files = [];
    state.activePath = '';
    state.readonly = !canModifySelectedModule(module);
    refs.search.value = '';
    state.search = '';
    renderModuleHeader();
    renderModuleList();
    clearEditor('Lade Source...', `${state.moduleTitle} wird geladen.`);
    ctx.setTitle?.(`${state.moduleTitle} Source`);
    await loadBundle();
  }

  async function loadBundle() {
    if (!state.moduleId) {
      state.files = [];
      state.activePath = '';
      clearEditor('Modul auswählen', 'Wähle links eine Business-OS App, um ihre Source-Dateien zu laden.');
      renderFileList();
      setStatus(moduleEmptyStatus());
      updateActionState();
      return;
    }
    state.loading = true;
    const selected = selectedModule();
    if (!canViewSelectedModuleSource(selected)) {
      state.files = [];
      state.activePath = '';
      state.readonly = true;
      clearEditor('Source nicht freigegeben', `${state.moduleTitle} kann mit deinem Zugriff nicht geöffnet werden.`);
      renderFileList();
      setStatus('Source ist fuer diese App nicht freigegeben.', true);
      updateActionState();
      state.loading = false;
      return;
    }
    state.readonly = !canModifySelectedModule(selected);
    setStatus('Lade Source...');
    clearEditor('Lade Source...', `${state.moduleTitle} wird geladen.`);
    updateActionState();
    try {
      await ensureSourceReplication();
      const projection = await dispatchSourceCommand('ctox.source.load', {
        module_id: state.moduleId,
        title: `Load ${state.moduleId} source files`,
      });
      const expectedCount = Number(projection?.result?.count || 0);
      const files = await waitForSourceFiles(expectedCount);
      state.files = normalizeSourceFiles(files);
      renderFileList();
      openFile(state.activePath && state.files.some((file) => file.path === state.activePath)
        ? state.activePath
        : preferredInitialPath(state.files));
      setStatus(`${state.files.length} Source-Dateien geladen.${state.usingMonaco ? ' Monaco aktiv.' : ''}${state.readonly ? ' Read-only.' : ''}`);
    } catch (error) {
      console.error('[source-editor] load failed:', error);
      state.files = [];
      state.activePath = '';
      clearEditor('Source nicht verfügbar', `${state.moduleTitle}: ${error?.message || error}`);
      renderFileList();
      setStatus(`Source konnte nicht über RxDB/WebRTC geladen werden: ${error?.message || error}`, true);
    } finally {
      state.loading = false;
      renderFileList();
      updateActionState();
    }
  }

  function renderModuleHeader() {
    refs.moduleTitle.textContent = state.moduleTitle || 'Module';
    refs.moduleId.textContent = state.moduleId || 'Modul auswählen';
  }

  function renderModuleList() {
    refs.moduleList.innerHTML = '';
    if (state.loadingModules) {
      refs.moduleList.innerHTML = '<p class="source-editor-empty">Lade Apps...</p>';
      return;
    }
    if (!state.modules.length) {
      refs.moduleList.innerHTML = '<p class="source-editor-empty">Keine Apps im Katalog gefunden.</p>';
      return;
    }
    const list = document.createElement('div');
    list.className = 'source-editor-modules';
    for (const module of state.modules) {
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'source-editor-module';
      button.classList.toggle('is-active', module.id === state.moduleId);
      button.setAttribute('aria-pressed', module.id === state.moduleId ? 'true' : 'false');
      button.innerHTML = `
        <span>${escapeHtml(module.title || module.id)}</span>
        <small>${escapeHtml(module.id)}</small>
      `;
      button.addEventListener('click', () => selectModule(module));
      list.append(button);
    }
    refs.moduleList.append(list);
  }

  function renderFileList() {
    refs.fileList.innerHTML = '';
    if (!state.moduleId) {
      refs.fileList.innerHTML = '<p class="source-editor-empty">Erst eine App wählen.</p>';
      return;
    }
    if (state.loading) {
      refs.fileList.innerHTML = '<p class="source-editor-empty">Lade Source-Dateien...</p>';
      return;
    }
    if (!state.files.length) {
      refs.fileList.innerHTML = '<p class="source-editor-empty">Keine editierbaren Source-Dateien oder Projektion leer.</p>';
      return;
    }
    const files = filteredFiles();
    if (!files.length) {
      refs.fileList.innerHTML = '<p class="source-editor-empty">Keine Treffer für diese Suche.</p>';
      return;
    }
    for (const file of files) {
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'source-editor-file';
      button.setAttribute('aria-current', file.path === state.activePath ? 'page' : 'false');
      button.classList.toggle('is-active', file.path === state.activePath);
      button.classList.toggle('is-dirty', Boolean(file.dirty));
      button.innerHTML = `
        <span>${escapeHtml(shortName(file.path))}</span>
        <small>${escapeHtml(file.path)}</small>
      `;
      button.addEventListener('click', () => openFile(file.path));
      refs.fileList.append(button);
    }
  }

  function openFile(path) {
    persistActiveDraft();
    const file = state.files.find((entry) => entry.path === path);
    if (!file) {
      state.activePath = '';
      clearEditor('Keine Datei', 'Wähle eine Source-Datei aus der linken Liste.');
      renderFileList();
      renderDiff();
      renderStatus();
      updateActionState();
      return;
    }
    state.activePath = file.path;
    refs.activeFile.textContent = file.path;
    refs.fileDetail.textContent = fileDetail(file);
    refs.placeholder.hidden = true;
    refs.monacoHost.classList.remove('is-empty');
    refs.fallback.classList.remove('is-empty');
    setEditorValue(file.draft_content ?? file.content ?? '', file.language || 'text', file.path);
    updateLineNumbers();
    renderFileList();
    renderDiff();
    renderStatus();
    updateActionState();
    ctx.setTitle?.(`${state.moduleTitle} · ${shortName(file.path)}`);
  }

  async function saveActiveFile() {
    const file = activeFile();
    if (!file || state.saving || state.readonly) {
      updateActionState();
      return;
    }
    persistActiveDraft();
    if (!file.dirty) {
      renderStatus();
      updateActionState();
      return;
    }
    const content = file.draft_content ?? file.content ?? '';
    state.saving = true;
    updateActionState();
    setStatus(`Speichere ${file.path}...`);
    try {
      await ensureSourceReplication();
      const projection = await dispatchSourceCommand('ctox.source.save', {
        module_id: state.moduleId,
        path: file.path,
        content,
        title: `Save ${state.moduleId}/${file.path}`,
      });
      const result = projection?.result || {};
      const projected = result.source_file_id ? await waitForSourceFile(result.source_file_id) : null;
      file.content = content;
      file.draft_content = null;
      file.dirty = false;
      file.size_bytes = projected?.size_bytes || result.size_bytes || new Blob([file.content]).size;
      file.modified_at_ms = projected?.updated_at_ms || result.modified_at_ms || Date.now();
      file.sha256 = projected?.sha256 || result.sha256 || file.sha256 || '';
      file.previous_sha256 = projected?.previous_sha256 || result.previous_sha256 || file.previous_sha256 || '';
      file.snapshot_id = projected?.snapshot_id || result.snapshot_id || file.snapshot_id || '';
      refs.fileDetail.textContent = fileDetail(file);
      renderFileList();
      renderDiff();
      setStatus(`${file.path} gespeichert. Snapshot: ${result.snapshot_id || 'nicht nötig'}.`);
    } catch (error) {
      console.error('[source-editor] save failed:', error);
      setStatus(`Speichern fehlgeschlagen: ${error?.message || error}`, true);
    } finally {
      state.saving = false;
      updateActionState();
    }
  }

  function revertActiveFile() {
    const file = activeFile();
    if (!file || state.saving) return;
    file.draft_content = null;
    file.dirty = false;
    setEditorValue(file.content ?? '', file.language || 'text', file.path);
    updateLineNumbers();
    renderFileList();
    renderDiff();
    renderStatus();
    updateActionState();
  }

  function formatActiveFile() {
    const file = activeFile();
    if (!file || state.saving || state.loading) return;
    const value = getEditorValue();
    let formatted;
    try {
      formatted = formatSourceContent(value, file.language || languageFromPath(file.path), file.path);
    } catch (error) {
      setStatus(`Formatieren fehlgeschlagen: ${error?.message || error}`, true);
      return;
    }
    if (formatted === value) {
      setStatus(`${file.path} ist bereits formatiert.`);
      return;
    }
    setEditorValue(formatted, file.language || 'text', file.path);
    updateLineNumbers();
    setActiveDirty(true);
    renderDiff();
    renderStatus();
    updateActionState();
  }

  function persistActiveDraft() {
    const file = activeFile();
    if (!file) return;
    const value = getEditorValue();
    if (value !== (file.content ?? '')) {
      file.draft_content = value;
      file.dirty = true;
    } else {
      file.draft_content = null;
      file.dirty = false;
    }
  }

  function setActiveDirty(value) {
    const file = activeFile();
    if (!file) return;
    file.dirty = Boolean(value) && getEditorValue() !== (file.content ?? '');
    file.draft_content = file.dirty ? getEditorValue() : null;
    renderFileList();
  }

  function filteredFiles() {
    return filterSourceFiles(state.files, state.search);
  }

  function activeFile() {
    return state.files.find((entry) => entry.path === state.activePath);
  }

  function currentFileValue() {
    const file = activeFile();
    return file ? file.draft_content ?? file.content ?? '' : '';
  }

  function getEditorValue() {
    return state.editor ? state.editor.getValue() : refs.code.value;
  }

  function setEditorValue(value, language, path) {
    if (state.editor && state.monaco) {
      const uri = state.monaco.Uri.parse(`business-os-source:///${state.moduleId}/${path}`);
      const existing = state.monaco.editor.getModel(uri);
      const model = existing || state.monaco.editor.createModel(value, monacoLanguage(language), uri);
      if (model.getValue() !== value) model.setValue(value);
      state.monaco.editor.setModelLanguage(model, monacoLanguage(language));
      state.editor.setModel(model);
      state.model = model;
    } else {
      refs.code.value = value;
      refs.code.dataset.language = language || 'text';
    }
  }

  function updateLineNumbers() {
    const count = Math.max(1, refs.code.value.split('\n').length);
    let text = '';
    for (let index = 1; index <= count; index += 1) text += `${index}\n`;
    refs.lines.textContent = text;
  }

  function renderDiff() {
    if (!state.diffOpen) return;
    const file = activeFile();
    if (!file) {
      refs.diffPanel.innerHTML = '<p class="source-editor-empty">Keine Datei ausgewählt.</p>';
      return;
    }
    const diff = buildLineDiff(file.content ?? '', getEditorValue());
    if (!diff.rows.length) {
      refs.diffPanel.innerHTML = `
        <div class="source-editor-diff-head">
          <strong>Keine Änderungen</strong>
          <span>${escapeHtml(file.path)}</span>
        </div>
      `;
      return;
    }
    refs.diffPanel.innerHTML = `
      <div class="source-editor-diff-head">
        <strong>${diff.added} hinzugefügt · ${diff.removed} entfernt</strong>
        <span>${escapeHtml(file.path)}</span>
      </div>
      <div class="source-editor-diff-rows">
        ${diff.rows.map((row) => `
          <pre class="${row.type === 'add' ? 'is-add' : 'is-remove'}"><b>${row.type === 'add' ? '+' : '-'}</b>${escapeHtml(row.text || ' ')}</pre>
        `).join('')}
      </div>
    `;
  }

  function renderStatus() {
    const file = activeFile();
    refs.status.classList.remove('is-error');
    if (!file) {
      refs.status.textContent = state.moduleId
        ? `${state.moduleTitle}: keine Datei ausgewählt.`
        : moduleEmptyStatus();
      return;
    }
    const mode = state.usingMonaco ? 'Monaco' : 'Texteditor';
    const suffix = file.dirty ? ' · ungespeichert' : '';
    const readonly = state.readonly ? ' · read-only fallback' : '';
    refs.status.textContent = `${state.moduleId}/${file.path} · ${mode}${suffix}${readonly}`;
  }

  function setStatus(text, error = false) {
    refs.status.textContent = text;
    refs.status.classList.toggle('is-error', Boolean(error));
  }

  function clearEditor(title, detail) {
    refs.activeFile.textContent = title;
    refs.fileDetail.textContent = detail || '';
    refs.placeholder.hidden = false;
    refs.placeholder.querySelector('strong').textContent = title;
    refs.placeholder.querySelector('span').textContent = detail || '';
    refs.monacoHost.classList.add('is-empty');
    refs.fallback.classList.add('is-empty');
    setEditorValue('', 'text', 'empty.txt');
    updateLineNumbers();
  }

  function updateActionState() {
    const file = activeFile();
    persistActiveDraft();
    const actions = sourceEditorActionState({
      moduleId: state.moduleId,
      hasFile: Boolean(file),
      dirty: Boolean(file?.dirty),
      saving: state.saving,
      loading: state.loading,
      readonly: state.readonly,
    });
    refs.openApp.disabled = !actions.openApp;
    refs.diff.disabled = !actions.diff;
    refs.format.disabled = !actions.format;
    refs.revert.disabled = !actions.revert;
    refs.reload.disabled = !actions.reload;
    refs.save.disabled = !actions.save;
  }

  async function ensureSourceReplication() {
    await Promise.all([
      ctx.sync?.startCollection?.('business_module_source_files'),
      ctx.sync?.startCollection?.('business_commands'),
    ]);
  }

  async function loadSourceFilesFromRxdb() {
    const collection = ctx.db?.collection?.('business_module_source_files');
    if (!collection) return [];
    const docs = await collection.find({
      selector: { module_id: state.moduleId },
      sort: [{ path: 'asc' }],
    }).exec();
    return docs
      .map((doc) => doc.toJSON ? doc.toJSON() : doc)
      .filter((file) => !file._deleted)
      .sort((left, right) => String(left.path || '').localeCompare(String(right.path || '')));
  }

  async function dispatchSourceCommand(type, payload) {
    if (!ctx.commandBus?.dispatch) {
      throw new Error('business_commands collection is required for source edits');
    }
    const commandId = `cmd_${newId()}`;
    await ctx.commandBus.dispatch({
      id: commandId,
      module: 'ctox',
      type,
      record_id: `${state.moduleId}:${payload.path || 'source'}`,
      inbound_channel: state.moduleId,
      payload,
      client_context: {
        source: 'business-os-source-editor',
        module_id: state.moduleId,
        actor: actorContext(ctx.session),
      },
    });
    return waitForCommandProjection(commandId);
  }

  async function waitForCommandProjection(commandId, timeoutMs = 45000) {
    const collection = ctx.db?.collection?.('business_commands');
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const doc = await collection?.findOne(commandId).exec();
      const data = doc?.toJSON?.();
      if (data && data.status && data.status !== 'pending_sync') {
        return assertSourceCommandSucceeded(data);
      }
      await delay(300);
    }
    throw new Error(`Command ${commandId} wurde nicht synchronisiert.`);
  }

  async function waitForSourceFiles(expectedCount, timeoutMs = 45000) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const files = await loadSourceFilesFromRxdb();
      if (expectedCount <= 0 || files.length >= expectedCount) return files;
      await delay(300);
    }
    throw new Error('Source-Dateien wurden nicht über RxDB repliziert.');
  }

  async function waitForSourceFile(id, timeoutMs = 45000) {
    const collection = ctx.db?.collection?.('business_module_source_files');
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const doc = await collection?.findOne(id).exec();
      const data = doc?.toJSON?.();
      if (data && !data._deleted) return data;
      await delay(300);
    }
    return null;
  }

  function selectedModule() {
    return state.modules.find((entry) => entry.id === state.moduleId)
      || (state.moduleId ? { id: state.moduleId, title: state.moduleTitle } : null);
  }

  function canViewSelectedModuleSource(module) {
    return canViewBusinessModuleSource(module, {
      session: ctx.session,
      governance: ctx.governance,
    });
  }

  function canModifySelectedModule(module) {
    return canModifyBusinessModule(module, {
      session: ctx.session,
      governance: ctx.governance,
    });
  }

  return () => {
    state.editor?.dispose?.();
    state.model?.dispose?.();
    container.replaceChildren();
  };
}

async function fetchModuleCatalog() {
  const response = await fetch('./modules/registry.json', { cache: 'no-store' });
  if (!response.ok) throw new Error(`registry.json ${response.status}`);
  const payload = await response.json();
  return payload.modules || payload;
}

export function normalizeModuleCatalog(modules, options = {}) {
  const rows = Array.isArray(modules) ? modules : [];
  const seen = new Set();
  return rows
    .filter((module) => module && typeof module.id === 'string' && module.id.trim())
    .filter((module) => module.editable !== false && module.hidden !== true)
    .map((module) => ({
      id: module.id.trim(),
      title: module.title || module.name || module.id.trim(),
      source: module.source || '',
    }))
    .filter((module) => {
      if (seen.has(module.id)) return false;
      seen.add(module.id);
      return true;
    })
    .filter((module) => !options.requireSourceView || canViewBusinessModuleSource(module, {
      session: options.session,
      governance: options.governance,
    }))
    .sort((left, right) => left.title.localeCompare(right.title, undefined, { sensitivity: 'base' }));
}

export function sourceCommandErrorMessage(projection) {
  const status = String(projection?.status || '');
  const result = projection?.result && typeof projection.result === 'object' ? projection.result : {};
  const decision = result.policy_decision && typeof result.policy_decision === 'object'
    ? result.policy_decision
    : null;
  if (status !== 'failed' && decision?.allowed !== false) return '';
  return String(
    decision?.display_reason
      || decision?.reason_code
      || result.error
      || projection?.error
      || 'Source command was denied'
  );
}

function assertSourceCommandSucceeded(projection) {
  const message = sourceCommandErrorMessage(projection);
  if (message) throw new Error(message);
  return projection;
}

export function normalizeSourceFiles(files) {
  return (Array.isArray(files) ? files : [])
    .filter((file) => file && !file._deleted && !file.is_deleted && typeof file.path === 'string' && file.path.trim())
    .map((file) => ({
      ...file,
      path: file.path.trim(),
      language: file.language || languageFromPath(file.path),
      draft_content: null,
      dirty: false,
    }))
    .sort((left, right) => left.path.localeCompare(right.path, undefined, { sensitivity: 'base' }));
}

export function filterSourceFiles(files, query) {
  const needle = String(query || '').trim().toLowerCase();
  if (!needle) return files;
  return files.filter((file) => `${file.path} ${file.language || ''}`.toLowerCase().includes(needle));
}

export function sourceEditorActionState(input) {
  const hasModule = Boolean(input.moduleId);
  const hasFile = Boolean(input.hasFile);
  const busy = Boolean(input.loading || input.saving);
  const dirty = Boolean(input.dirty);
  return {
    openApp: hasModule && !busy,
    diff: hasFile && !busy,
    format: hasFile && !busy && !Boolean(input.readonly),
    revert: hasFile && dirty && !busy,
    reload: hasModule && !Boolean(input.saving),
    save: hasFile && dirty && !busy && !Boolean(input.readonly),
  };
}

function moduleEmptyStatus() {
  return 'Kein Modul ausgewählt. Links eine Business-OS App auswählen.';
}

function loadMonaco() {
  if (window.monaco?.editor) {
    defineBusinessTheme(window.monaco);
    return Promise.resolve(window.monaco);
  }
  if (monacoPromise) return monacoPromise;
  monacoPromise = new Promise(async (resolve, reject) => {
    const monacoBaseUrl = resolveMonacoBaseUrl(import.meta.url, window.location?.origin);
    const loaderUrl = `${monacoBaseUrl}vs/loader.js`;
    window.MonacoEnvironment = {
      getWorkerUrl() {
        const baseUrl = monacoBaseUrl;
        const worker = `
          self.MonacoEnvironment = { baseUrl: '${baseUrl}' };
          importScripts('${baseUrl}vs/base/worker/workerMain.js');
        `;
        return `data:text/javascript;charset=utf-8,${encodeURIComponent(worker)}`;
      },
    };
    const ready = () => {
      window.require.config({ paths: { vs: `${monacoBaseUrl}vs` } });
      window.require(['vs/editor/editor.main'], () => {
        defineBusinessTheme(window.monaco);
        resolve(window.monaco);
      }, reject);
    };
    if (window.require?.config) {
      ready();
      return;
    }
    try {
      await verifyJavaScriptAsset(loaderUrl);
    } catch (error) {
      reject(error);
      return;
    }
    const script = document.createElement('script');
    script.src = loaderUrl;
    script.onload = ready;
    script.onerror = () => reject(new Error(`failed to load ${loaderUrl}`));
    document.head.append(script);
  }).catch((error) => {
    monacoPromise = null;
    throw error;
  });
  return monacoPromise;
}

export function resolveMonacoBaseUrl(moduleUrl, origin = '') {
  try {
    const url = new URL('../../vendor/monaco/', moduleUrl);
    return url.href;
  } catch (_error) {
    const base = origin || globalThis.location?.origin || '';
    return base ? new URL('/business-os/vendor/monaco/', base).href : '/business-os/vendor/monaco/';
  }
}

export function isJavaScriptMime(contentType) {
  const mime = String(contentType || '').split(';')[0].trim().toLowerCase();
  return !mime
    || mime === 'application/javascript'
    || mime === 'text/javascript'
    || mime === 'application/ecmascript'
    || mime === 'text/ecmascript'
    || mime === 'application/x-javascript'
    || mime === 'application/octet-stream';
}

async function verifyJavaScriptAsset(url) {
  const response = await fetch(url, { cache: 'force-cache' });
  const contentType = response.headers.get('content-type') || '';
  if (!response.ok) throw new Error(`Monaco loader ${response.status}: ${url}`);
  if (!isJavaScriptMime(contentType)) {
    throw new Error(`Monaco loader MIME ${contentType || 'unbekannt'}: ${url}`);
  }
}

function defineBusinessTheme(monaco) {
  if (!monaco?.editor || monaco.__businessOsThemeDefined) return;
  monaco.editor.defineTheme('business-os-dark', {
    base: 'vs-dark',
    inherit: true,
    rules: [],
    colors: {
      'editor.background': '#0b1115',
      'editor.foreground': '#d7dee7',
      'editorLineNumber.foreground': '#61707f',
      'editorLineNumber.activeForeground': '#9fb0c2',
      'editorCursor.foreground': '#69c8b7',
      'editor.selectionBackground': '#234f57',
      'editor.lineHighlightBackground': '#121a20',
      'editorGutter.background': '#0b1115',
    },
  });
  monaco.__businessOsThemeDefined = true;
}

function actorContext(session) {
  const user = session?.user || {};
  return {
    id: user.id || '',
    display_name: user.display_name || user.name || user.id || '',
    role: user.role || 'user',
    is_admin: Boolean(user.is_admin),
  };
}

function newId() {
  return globalThis.crypto?.randomUUID?.() || `${Date.now()}_${Math.random().toString(36).slice(2)}`;
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function preferredInitialPath(files) {
  return files.find((file) => file.path === 'index.js')?.path
    || files.find((file) => file.path === 'module.json')?.path
    || files[0]?.path
    || '';
}

function insertAtCursor(textarea, value) {
  const start = textarea.selectionStart;
  const end = textarea.selectionEnd;
  textarea.setRangeText(value, start, end, 'end');
  textarea.dispatchEvent(new Event('input', { bubbles: true }));
}

function buildLineDiff(beforeRaw, afterRaw) {
  const before = String(beforeRaw ?? '').split('\n');
  const after = String(afterRaw ?? '').split('\n');
  let start = 0;
  while (start < before.length && start < after.length && before[start] === after[start]) start += 1;
  let beforeEnd = before.length - 1;
  let afterEnd = after.length - 1;
  while (beforeEnd >= start && afterEnd >= start && before[beforeEnd] === after[afterEnd]) {
    beforeEnd -= 1;
    afterEnd -= 1;
  }
  const rows = [];
  for (let index = start; index <= beforeEnd; index += 1) rows.push({ type: 'remove', text: before[index] });
  for (let index = start; index <= afterEnd; index += 1) rows.push({ type: 'add', text: after[index] });
  return {
    rows: rows.slice(0, 400),
    added: Math.max(0, afterEnd - start + 1),
    removed: Math.max(0, beforeEnd - start + 1),
  };
}

function shortName(path) {
  return String(path || '').split('/').pop() || path || 'Source';
}

function fileDetail(file) {
  const hash = file.sha256 ? ` · ${file.sha256.slice(0, 10)}` : '';
  return `${file.language || 'text'} · ${formatBytes(file.size_bytes || 0)}${hash}`;
}

export function formatSourceContent(value, language, path = '') {
  const source = String(value ?? '');
  const normalizedLanguage = language || languageFromPath(path);
  if (normalizedLanguage === 'json') {
    return `${JSON.stringify(JSON.parse(source), null, 2)}\n`;
  }
  return source
    .split('\n')
    .map((line) => line.replace(/[ \t]+$/g, ''))
    .join('\n')
    .replace(/\n+$/g, '\n')
    .replace(/[^\n]$/g, (char) => `${char}\n`);
}

function monacoLanguage(language) {
  if (language === 'text') return 'plaintext';
  if (language === 'markdown') return 'markdown';
  return language || 'plaintext';
}

function languageFromPath(path) {
  const ext = String(path || '').split('.').pop()?.toLowerCase();
  if (ext === 'js' || ext === 'mjs' || ext === 'cjs') return 'javascript';
  if (ext === 'ts' || ext === 'tsx') return 'typescript';
  if (ext === 'css') return 'css';
  if (ext === 'html') return 'html';
  if (ext === 'json') return 'json';
  if (ext === 'md') return 'markdown';
  return 'text';
}

function formatBytes(value) {
  const bytes = Number(value || 0);
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function ensureStyles() {
  if (document.getElementById('source-editor-styles')) return;
  const style = document.createElement('style');
  style.id = 'source-editor-styles';
  style.textContent = `
    .source-editor {
      display: grid;
      grid-template-columns: 248px minmax(0, 1fr);
      height: 100%;
      min-height: 0;
      background: var(--surface);
      color: var(--text);
      font: 12px/1.35 ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }
    .source-editor-sidebar {
      display: grid;
      grid-template-rows: auto minmax(96px, 24%) auto minmax(0, 1fr);
      min-width: 0;
      border-right: 1px solid var(--hairline, var(--line));
      background: color-mix(in srgb, var(--surface-2) 62%, var(--surface));
    }
    .source-editor-sidebar-head {
      display: grid;
      gap: 2px;
      padding: 12px;
      border-bottom: 1px solid var(--hairline, var(--line));
    }
    .source-editor-sidebar-head strong,
    .source-editor-sidebar-head span {
      min-width: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .source-editor-sidebar-head strong { font-size: 13px; }
    .source-editor-sidebar-head span { color: var(--muted); font-size: 11px; }
    .source-editor-module-list {
      min-height: 0;
      overflow: auto;
      padding: 8px;
      border-bottom: 1px solid var(--hairline, var(--line));
    }
    .source-editor-modules {
      display: grid;
      gap: 5px;
    }
    .source-editor-module {
      display: grid;
      gap: 1px;
      width: 100%;
      min-height: 36px;
      border: 1px solid transparent;
      border-radius: 7px;
      background: transparent;
      color: var(--text);
      padding: 6px 8px;
      text-align: left;
    }
    .source-editor-module:hover {
      background: color-mix(in srgb, var(--surface) 70%, transparent);
    }
    .source-editor-module.is-active {
      border-color: color-mix(in srgb, var(--accent) 34%, var(--line));
      background: color-mix(in srgb, var(--accent) 12%, var(--surface));
    }
    .source-editor-module span,
    .source-editor-module small {
      min-width: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .source-editor-module span { font-weight: 760; }
    .source-editor-module small { color: var(--muted); }
    .source-editor-search {
      display: grid;
      grid-template-columns: auto minmax(0, 1fr);
      align-items: center;
      gap: 6px;
      padding: 8px;
      border-bottom: 1px solid var(--hairline, var(--line));
      color: var(--muted);
    }
    .source-editor-search input {
      width: 100%;
      min-width: 0;
      min-height: 28px;
      border: 1px solid var(--hairline, var(--line));
      border-radius: 7px;
      background: color-mix(in srgb, var(--surface) 76%, var(--surface-2));
      color: var(--text);
      padding: 0 8px;
      outline: none;
    }
    .source-editor-search input:focus {
      border-color: color-mix(in srgb, var(--accent) 50%, var(--line));
    }
    .source-editor-sidebar nav {
      min-height: 0;
      overflow: auto;
      padding: 8px;
    }
    .source-editor-file {
      position: relative;
      display: grid;
      gap: 2px;
      width: 100%;
      min-height: 40px;
      border: 1px solid transparent;
      border-radius: 7px;
      background: transparent;
      color: var(--text);
      padding: 7px 8px;
      text-align: left;
    }
    .source-editor-file:hover {
      background: color-mix(in srgb, var(--surface) 70%, transparent);
    }
    .source-editor-file.is-active {
      border-color: color-mix(in srgb, var(--accent) 34%, var(--line));
      background: color-mix(in srgb, var(--accent) 12%, var(--surface));
    }
    .source-editor-file.is-dirty::after {
      content: "";
      position: absolute;
      top: 9px;
      right: 8px;
      width: 6px;
      height: 6px;
      border-radius: 50%;
      background: var(--accent);
    }
    .source-editor-file span,
    .source-editor-file small {
      min-width: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      padding-right: 12px;
    }
    .source-editor-file span { font-weight: 760; }
    .source-editor-file small { color: var(--muted); }
    .source-editor-main {
      display: grid;
      grid-template-rows: 46px minmax(0, 1fr) 28px;
      min-width: 0;
      min-height: 0;
      background: color-mix(in srgb, var(--bg) 42%, var(--surface));
    }
    .source-editor-toolbar {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      padding: 8px 10px 8px 12px;
      border-bottom: 1px solid var(--hairline, var(--line));
      background: color-mix(in srgb, var(--surface) 90%, var(--surface-2));
    }
    .source-editor-file-meta {
      display: grid;
      gap: 1px;
      min-width: 0;
    }
    .source-editor-file-meta strong,
    .source-editor-file-meta span {
      min-width: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .source-editor-file-meta strong { font-size: 12px; }
    .source-editor-file-meta span { color: var(--muted); font-size: 11px; }
    .source-editor-actions {
      display: inline-flex;
      gap: 6px;
      flex: 0 0 auto;
    }
    .source-editor-actions button {
      display: inline-flex;
      align-items: center;
      gap: 5px;
      min-height: 28px;
      border: 1px solid var(--hairline, var(--line));
      border-radius: 7px;
      background: color-mix(in srgb, var(--surface) 78%, var(--surface-2));
      color: var(--text);
      padding: 0 10px;
      font-weight: 730;
    }
    .source-editor-actions button:hover,
    .source-editor-actions button.is-active {
      border-color: color-mix(in srgb, var(--accent) 34%, var(--line));
      color: var(--accent);
    }
    .source-editor-actions button:disabled {
      opacity: 0.45;
      cursor: not-allowed;
      color: var(--muted);
      border-color: var(--hairline, var(--line));
    }
    .source-editor-actions button[data-source-save] {
      color: var(--accent);
      border-color: color-mix(in srgb, var(--accent) 34%, var(--line));
    }
    .source-editor-workbench {
      position: relative;
      display: grid;
      grid-template-columns: minmax(0, 1fr) minmax(260px, 34%);
      min-height: 0;
      background: #0b1115;
    }
    .source-editor-diff[hidden] {
      display: none;
    }
    .source-editor-workbench:has(.source-editor-diff[hidden]) {
      grid-template-columns: minmax(0, 1fr);
    }
    .source-editor-monaco {
      min-width: 0;
      min-height: 0;
      height: 100%;
    }
    .source-editor-monaco.is-empty {
      opacity: 0;
      pointer-events: none;
    }
    .source-editor-monaco[hidden],
    .source-editor-fallback[hidden] {
      display: none !important;
    }
    .source-editor-fallback {
      display: grid;
      grid-template-columns: 58px minmax(0, 1fr);
      min-height: 0;
      height: 100%;
    }
    .source-editor-fallback.is-empty {
      visibility: hidden;
      pointer-events: none;
    }
    .source-editor-placeholder {
      position: absolute;
      left: 50%;
      top: 50%;
      transform: translate(-50%, -50%);
      display: grid;
      gap: 6px;
      max-width: min(460px, calc(100% - 48px));
      padding: 18px 20px;
      border: 1px solid color-mix(in srgb, var(--line) 78%, transparent);
      border-radius: 8px;
      background: color-mix(in srgb, var(--surface) 84%, var(--bg));
      color: var(--text);
      text-align: center;
      z-index: 1;
    }
    .source-editor-placeholder[hidden] {
      display: none;
    }
    .source-editor-placeholder strong {
      font-size: 13px;
    }
    .source-editor-placeholder span {
      color: var(--muted);
      font-size: 12px;
    }
    .source-editor-lines,
    .source-editor textarea {
      margin: 0;
      border: 0;
      font: 12px/1.55 ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace;
      tab-size: 2;
    }
    .source-editor-lines {
      overflow: hidden;
      padding: 12px 10px 12px 0;
      border-right: 1px solid color-mix(in srgb, var(--line) 68%, transparent);
      color: var(--muted);
      text-align: right;
      white-space: pre;
      user-select: none;
    }
    .source-editor textarea {
      width: 100%;
      min-width: 0;
      height: 100%;
      min-height: 0;
      resize: none;
      outline: 0;
      background: transparent;
      color: var(--text);
      padding: 12px 14px;
      white-space: pre;
      overflow: auto;
    }
    .source-editor-diff {
      min-width: 0;
      min-height: 0;
      overflow: auto;
      border-left: 1px solid var(--hairline, var(--line));
      background: color-mix(in srgb, var(--surface) 72%, var(--bg));
    }
    .source-editor-diff-head {
      position: sticky;
      top: 0;
      display: grid;
      gap: 2px;
      padding: 10px 12px;
      border-bottom: 1px solid var(--hairline, var(--line));
      background: color-mix(in srgb, var(--surface) 92%, var(--bg));
      z-index: 1;
    }
    .source-editor-diff-head strong {
      font-size: 12px;
    }
    .source-editor-diff-head span {
      min-width: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      color: var(--muted);
      font-size: 11px;
    }
    .source-editor-diff-rows {
      padding: 8px 0;
    }
    .source-editor-diff pre {
      margin: 0;
      padding: 3px 10px;
      overflow: hidden;
      text-overflow: ellipsis;
      font: 11px/1.45 ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace;
      white-space: pre;
    }
    .source-editor-diff pre b {
      display: inline-block;
      width: 16px;
      font-weight: 800;
    }
    .source-editor-diff pre.is-add {
      background: color-mix(in srgb, #1a7f4f 18%, transparent);
      color: #bcebd1;
    }
    .source-editor-diff pre.is-remove {
      background: color-mix(in srgb, #9b3030 18%, transparent);
      color: #f1c2c2;
    }
    .source-editor-status {
      display: flex;
      align-items: center;
      min-width: 0;
      border-top: 1px solid var(--hairline, var(--line));
      color: var(--muted);
      padding: 0 12px;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .source-editor-status.is-error {
      color: var(--danger);
    }
    .source-editor-empty {
      margin: 0;
      padding: 10px 8px;
      color: var(--muted);
    }
    @media (max-width: 860px) {
      .source-editor { grid-template-columns: 190px minmax(0, 1fr); }
      .source-editor-actions button { padding: 0 8px; }
      .source-editor-workbench { grid-template-columns: minmax(0, 1fr); }
      .source-editor-diff { display: none; }
    }
    @media (max-width: 560px) {
      .source-editor {
        grid-template-columns: minmax(0, 1fr);
        grid-template-rows: minmax(190px, 38%) minmax(0, 1fr);
      }
      .source-editor-sidebar {
        grid-template-rows: auto minmax(84px, 1fr) auto minmax(72px, 1fr);
        border-right: 0;
        border-bottom: 1px solid var(--hairline, var(--line));
      }
      .source-editor-main {
        grid-template-rows: auto minmax(0, 1fr) 32px;
      }
      .source-editor-toolbar {
        align-items: flex-start;
        flex-wrap: wrap;
        min-height: 0;
      }
      .source-editor-actions {
        width: 100%;
        overflow-x: auto;
        padding-bottom: 2px;
      }
      .source-editor-actions button {
        flex: 0 0 34px;
        justify-content: center;
        padding: 0;
      }
      .source-editor-actions button span:not([aria-hidden="true"]) {
        display: none;
      }
      .source-editor-placeholder {
        max-width: calc(100% - 24px);
        padding: 14px;
      }
      .source-editor-fallback {
        grid-template-columns: 44px minmax(0, 1fr);
      }
      .source-editor-lines {
        padding-right: 8px;
      }
    }
  `;
  document.head.append(style);
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
