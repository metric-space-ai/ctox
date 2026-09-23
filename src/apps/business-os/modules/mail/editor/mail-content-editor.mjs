import { mountMailLogicEditor } from './logic-editor-v1.mjs';
import { createBusinessOsOfficeBridge } from '../../../office-engine/src/business-os-bridge.mjs?v=20260807-einheitliche-ladepfade-v97';

const EDITOR_MODES = Object.freeze({
  RICH_TEXT: 'rich-text',
  HTML: 'html',
});

const DEFAULT_EASY_EMAIL_MODULE = '../../../vendor/easy-email-editor/index.mjs';
const STYLE_REVISION = '20260923-mail-content-editor-v2';

const COPY = Object.freeze({
  de: Object.freeze({
    richText: 'Rich Text',
    html: 'HTML',
    editViewport: 'Bearbeiten',
    desktopViewport: 'Desktop-Vorschau',
    mobileViewport: 'Mobil-Vorschau',
    undo: 'Rückgängig',
    redo: 'Wiederholen',
    wordKicker: 'Word-Entwurf',
    noWordTitle: 'Noch kein Word-Entwurf verknüpft',
    noWordBody: 'Lege einen Entwurf im Word-Modul an oder verknüpfe ein vorhandenes Dokument.',
    createWord: 'Word-Entwurf anlegen',
    openWord: 'In Word bearbeiten',
    linkedVersion: 'Version',
    loadingHtml: 'HTML-Editor wird geladen…',
    htmlFailed: 'HTML-Editor konnte nicht geladen werden.',
    blocks: 'Bausteine',
    design: 'Design',
    source: 'Code',
    logic: 'Logik',
    closePanel: 'Panel schließen',
  }),
  en: Object.freeze({
    richText: 'Rich text',
    html: 'HTML',
    editViewport: 'Edit',
    desktopViewport: 'Desktop preview',
    mobileViewport: 'Mobile preview',
    undo: 'Undo',
    redo: 'Redo',
    wordKicker: 'Word draft',
    noWordTitle: 'No Word draft linked yet',
    noWordBody: 'Create a draft in the Word module or link an existing document.',
    createWord: 'Create Word draft',
    openWord: 'Edit in Word',
    linkedVersion: 'Version',
    loadingHtml: 'Loading HTML editor…',
    htmlFailed: 'The HTML editor could not be loaded.',
    blocks: 'Blocks',
    design: 'Design',
    source: 'Code',
    logic: 'Logic',
    closePanel: 'Close panel',
  }),
});

/**
 * Mount the content editor used by Mail groups and their child messages.
 *
 * Rich-text content intentionally remains a Documents-owned artifact. Mail
 * stores only the returned document reference; the Documents module keeps
 * ownership of DOCX bytes, versions, permissions and Office bridge commands.
 * HTML content stays behind the local Easy Email runtime passed here or loaded
 * from the managed `business-os/vendor/easy-email-editor/index.mjs` browser ESM.
 */
export async function createMailContentEditor(options = {}) {
  const host = options.host;
  if (!isElement(host)) throw new TypeError('Mail content editor requires a host Element');
  if (!options.ctx || typeof options.ctx !== 'object') throw new TypeError('Mail content editor requires the shell-provided ctx');

  await ensureStyles(host.ownerDocument || document);

  const ctx = options.ctx;
  const locale = normalizeLocale(options.locale || ctx.locale);
  const labels = { ...COPY[locale], ...(options.labels || {}) };
  let mode = normalizeEditorMode(options.mode);
  let documentArtifact = normalizeDocumentArtifact(options.documentArtifact);
  let htmlDocument = cloneJson(options.htmlDocument ?? options.initialHtmlDocument ?? {});
  let htmlEditor = null;
  let htmlEditorPromise = null;
  let logicEditor = null;
  let dirty = false;
  let destroyed = false;
  let readOnly = options.readOnly === true;
  let viewport = 'edit';
  let drawerReturnFocus = null;
  let activeHtmlPanel = '';
  const listeners = new Map();

  const root = host.ownerDocument.createElement('section');
  root.className = 'mail-content-editor ctox-pane';
  root.dataset.mailContentEditor = 'true';

  const commandbar = host.ownerDocument.createElement('div');
  commandbar.className = 'mail-content-editor-commandbar ctox-pane-band';
  const modeSwitch = host.ownerDocument.createElement('div');
  modeSwitch.className = 'ctox-pane-tabs mail-content-editor-modes';
  modeSwitch.setAttribute('role', 'tablist');
  modeSwitch.setAttribute('aria-label', locale === 'en' ? 'Content format' : 'Inhaltsformat');

  const richTextTab = modeButton(host.ownerDocument, EDITOR_MODES.RICH_TEXT, labels.richText);
  const htmlTab = modeButton(host.ownerDocument, EDITOR_MODES.HTML, labels.html);
  modeSwitch.append(richTextTab, htmlTab);
  commandbar.append(modeSwitch);

  const viewportTools = host.ownerDocument.createElement('div');
  viewportTools.className = 'mail-content-editor-viewport-tools';
  viewportTools.setAttribute('role', 'group');
  viewportTools.setAttribute('aria-label', locale === 'en' ? 'Canvas view' : 'Canvas-Ansicht');
  const viewportIcons = { edit: 'edit', desktop: 'monitor', mobile: 'phone' };
  const viewportLabels = { edit: labels.editViewport, desktop: labels.desktopViewport, mobile: labels.mobileViewport };
  const viewportButtons = Object.fromEntries(['edit', 'desktop', 'mobile'].map((name) => {
    const button = host.ownerDocument.createElement('button');
    button.type = 'button';
    button.className = 'ctox-pane-icon mail-content-editor-viewport-button';
    button.dataset.mailEditorViewport = name;
    button.setAttribute('aria-label', viewportLabels[name]);
    button.title = viewportLabels[name];
    button.setAttribute('aria-pressed', String(name === viewport));
    setActionIcon(button, ctx, viewportIcons[name], viewportLabels[name].slice(0, 1));
    button.addEventListener('click', () => setViewport(name).catch((error) => {
      emit('error', { mode: EDITOR_MODES.HTML, error });
      setStatus(error?.message || String(error), 'error');
    }));
    viewportTools.append(button);
    return [name, button];
  }));
  commandbar.append(viewportTools);

  const historyTools = host.ownerDocument.createElement('div');
  historyTools.className = 'mail-content-editor-history-tools';
  historyTools.setAttribute('role', 'group');
  historyTools.setAttribute('aria-label', locale === 'en' ? 'History' : 'Verlauf');
  for (const [action, label] of [['undo', labels.undo], ['redo', labels.redo]]) {
    const button = host.ownerDocument.createElement('button');
    button.type = 'button';
    button.className = 'ctox-pane-icon';
    button.dataset.mailEditorHistory = action;
    button.setAttribute('aria-label', label);
    button.title = label;
    setActionIcon(button, ctx, action, action === 'undo' ? '↶' : '↷');
    button.addEventListener('click', () => runHistoryAction(action).catch((error) => {
      emit('error', { mode: EDITOR_MODES.HTML, error });
      setStatus(error?.message || String(error), 'error');
    }));
    historyTools.append(button);
  }
  commandbar.append(historyTools);

  const htmlTools = host.ownerDocument.createElement('div');
  htmlTools.className = 'mail-content-editor-html-tools';
  htmlTools.setAttribute('role', 'group');
  htmlTools.setAttribute('aria-label', locale === 'en' ? 'HTML editor panels' : 'HTML-Editor-Panels');
  const panelIcons = { blocks: 'grid', design: 'settings', logic: 'filter', source: 'file' };
  const panelButtons = Object.fromEntries(['blocks', 'design', 'logic', 'source'].map((name) => {
    const button = host.ownerDocument.createElement('button');
    button.type = 'button';
    button.className = 'ctox-pane-tab mail-content-editor-panel-button';
    button.dataset.mailEditorOpenPanel = name;
    button.setAttribute('aria-label', labels[name]);
    button.title = labels[name];
    const iconSpan = host.ownerDocument.createElement('span');
    iconSpan.className = 'mail-content-editor-panel-icon';
    iconSpan.setAttribute('aria-hidden', 'true');
    setActionIcon(iconSpan, ctx, panelIcons[name], '');
    const labelSpan = host.ownerDocument.createElement('span');
    labelSpan.textContent = labels[name];
    button.append(iconSpan, labelSpan);
    button.setAttribute('aria-expanded', 'false');
    button.addEventListener('click', () => openHtmlPanel(name).catch((error) => {
      emit('error', { mode: EDITOR_MODES.HTML, error });
      setStatus(error?.message || String(error), 'error');
    }));
    htmlTools.append(button);
    return [name, button];
  }));
  commandbar.append(htmlTools);

  const status = host.ownerDocument.createElement('span');
  status.className = 'mail-content-editor-status';
  status.setAttribute('role', 'status');
  status.setAttribute('aria-live', 'polite');
  commandbar.append(status);

  const body = host.ownerDocument.createElement('div');
  body.className = 'mail-content-editor-body ctox-pane-body';

  const wordPanel = host.ownerDocument.createElement('section');
  wordPanel.className = 'mail-content-editor-panel mail-content-editor-word';
  wordPanel.dataset.mailEditorPanel = EDITOR_MODES.RICH_TEXT;
  wordPanel.setAttribute('role', 'tabpanel');

  const htmlPanel = host.ownerDocument.createElement('section');
  htmlPanel.className = 'mail-content-editor-panel mail-content-editor-html';
  htmlPanel.dataset.mailEditorPanel = EDITOR_MODES.HTML;
  htmlPanel.setAttribute('role', 'tabpanel');

  const htmlHost = host.ownerDocument.createElement('div');
  htmlHost.className = 'mail-content-editor-html-host';
  htmlHost.dataset.mailEasyEmailHost = 'true';

  const drawerBackdrop = host.ownerDocument.createElement('button');
  drawerBackdrop.type = 'button';
  drawerBackdrop.className = 'mail-content-editor-drawer-backdrop';
  drawerBackdrop.setAttribute('aria-label', labels.closePanel);
  drawerBackdrop.hidden = true;
  drawerBackdrop.addEventListener('click', closeHtmlPanel);

  const drawer = host.ownerDocument.createElement('aside');
  drawer.className = 'mail-content-editor-drawer';
  drawer.setAttribute('role', 'dialog');
  drawer.setAttribute('aria-modal', 'true');
  drawer.setAttribute('aria-hidden', 'true');
  drawer.hidden = true;
  const drawerHeader = host.ownerDocument.createElement('header');
  drawerHeader.className = 'mail-content-editor-drawer-header ctox-pane-band';
  const drawerTitle = host.ownerDocument.createElement('strong');
  drawerTitle.dataset.mailEditorDrawerTitle = 'true';
  const drawerClose = host.ownerDocument.createElement('button');
  drawerClose.type = 'button';
  drawerClose.className = 'ctox-pane-icon';
  drawerClose.setAttribute('aria-label', labels.closePanel);
  drawerClose.title = labels.closePanel;
  setActionIcon(drawerClose, ctx, 'close', '×');
  drawerClose.addEventListener('click', closeHtmlPanel);
  drawerHeader.append(drawerTitle, drawerClose);
  const drawerBody = host.ownerDocument.createElement('div');
  drawerBody.className = 'mail-content-editor-drawer-body ctox-pane-body';
  const panelHosts = Object.fromEntries(['blocks', 'design', 'logic', 'source'].map((name) => {
    const panelHost = host.ownerDocument.createElement('div');
    panelHost.className = 'mail-content-editor-drawer-panel';
    panelHost.dataset.mailEditorDrawerPanel = name;
    panelHost.hidden = true;
    drawerBody.append(panelHost);
    return [name, panelHost];
  }));
  drawer.append(drawerHeader, drawerBody);
  htmlPanel.append(htmlHost, drawerBackdrop, drawer);
  body.append(wordPanel, htmlPanel);
  const commandHost = isElement(options.commandHost) ? options.commandHost : null;
  if (commandHost) {
    root.classList.add('has-external-commandbar');
    commandbar.classList.add('is-integrated');
    commandHost.replaceChildren(commandbar);
    root.append(body);
  } else {
    root.append(commandbar, body);
  }
  host.replaceChildren(root);

  richTextTab.addEventListener('click', () => setMode(EDITOR_MODES.RICH_TEXT));
  htmlTab.addEventListener('click', () => setMode(EDITOR_MODES.HTML));
  const handleEscape = (event) => {
    if (drawer.hidden) return;
    if (event.key === 'Escape') {
      event.preventDefault();
      closeHtmlPanel();
      return;
    }
    if (event.key === 'Tab') keepFocusInDrawer(event, drawer);
  };
  root.addEventListener('keydown', handleEscape);

  function emit(name, detail) {
    for (const listener of listeners.get(name) || []) listener(detail);
    options.onEvent?.({ name, detail });
  }

  function setStatus(message = '', kind = '') {
    status.textContent = message;
    status.dataset.kind = kind;
  }

  async function setMode(nextMode, setModeOptions = {}) {
    assertAlive(destroyed);
    const normalized = normalizeEditorMode(nextMode);
    const changed = normalized !== mode;
    mode = normalized;
    if (mode !== EDITOR_MODES.HTML) closeHtmlPanel();
    renderMode();
    if (mode === EDITOR_MODES.HTML) await ensureHtmlEditor();
    if (setModeOptions.focus !== false) focus();
    if (changed) emit('modechange', { mode });
    return mode;
  }

  function renderMode() {
    const isRichText = mode === EDITOR_MODES.RICH_TEXT;
    richTextTab.setAttribute('aria-selected', String(isRichText));
    richTextTab.setAttribute('aria-pressed', String(isRichText));
    htmlTab.setAttribute('aria-selected', String(!isRichText));
    htmlTab.setAttribute('aria-pressed', String(!isRichText));
    htmlTools.hidden = isRichText;
    viewportTools.hidden = isRichText;
    historyTools.hidden = isRichText;
    wordPanel.hidden = !isRichText;
    htmlPanel.hidden = isRichText;
  }

  async function setViewport(nextViewport) {
    assertAlive(destroyed);
    const normalized = ['edit', 'desktop', 'mobile'].includes(nextViewport) ? nextViewport : 'edit';
    viewport = normalized;
    for (const [name, button] of Object.entries(viewportButtons)) {
      button.setAttribute('aria-pressed', String(name === viewport));
    }
    if (mode !== EDITOR_MODES.HTML) await setMode(EDITOR_MODES.HTML, { focus: false });
    const editor = await ensureHtmlEditor();
    await editor.setViewport?.(viewport);
    emit('viewportchange', { viewport });
  }

  async function runHistoryAction(action) {
    assertAlive(destroyed);
    if (mode !== EDITOR_MODES.HTML) await setMode(EDITOR_MODES.HTML, { focus: false });
    const editor = await ensureHtmlEditor();
    if (action === 'undo') await editor.undo?.();
    else await editor.redo?.();
  }

  async function openHtmlPanel(name, panelOptions = {}) {
    assertAlive(destroyed);
    if (!Object.hasOwn(panelHosts, name)) throw new TypeError(`Unsupported HTML editor panel: ${name}`);
    if (mode !== EDITOR_MODES.HTML) await setMode(EDITOR_MODES.HTML, { focus: false });
    const editor = await ensureHtmlEditor();
    if (editor.ownsPanels === true && name !== 'logic') {
      if (!drawer.hidden) closeHtmlPanel();
      for (const [panelName, button] of Object.entries(panelButtons)) {
        button.setAttribute('aria-expanded', String(panelName === name));
      }
      await editor.setActivePanel?.(name);
      emit('panelchange', { panel: name, open: true, owner: 'runtime' });
      return;
    }
    const alreadyVisible = activeHtmlPanel === name && !drawer.hidden;
    if (!alreadyVisible) {
      for (const [panelName, panelHost] of Object.entries(panelHosts)) {
        const selected = panelName === name;
        panelHost.hidden = !selected;
        panelButtons[panelName].setAttribute('aria-expanded', String(selected));
      }
      drawerTitle.textContent = labels[name];
      drawerReturnFocus = host.ownerDocument.activeElement;
      drawer.hidden = false;
      drawerBackdrop.hidden = false;
      drawer.setAttribute('aria-hidden', 'false');
      htmlHost.setAttribute('inert', '');
      activeHtmlPanel = name;
      drawerClose.focus();
      emit('panelchange', { panel: name, open: true });
    }
    if (panelOptions.fromRuntime !== true) await editor.setActivePanel?.(name);
  }

  function closeHtmlPanel() {
    if (!drawer || drawer.hidden) return;
    const closedPanel = activeHtmlPanel;
    drawer.hidden = true;
    drawerBackdrop.hidden = true;
    drawer.setAttribute('aria-hidden', 'true');
    htmlHost.removeAttribute('inert');
    activeHtmlPanel = '';
    for (const [name, button] of Object.entries(panelButtons)) {
      button.setAttribute('aria-expanded', 'false');
      panelHosts[name].hidden = true;
    }
    htmlEditor?.setActivePanel?.(null);
    if (drawerReturnFocus?.isConnected) drawerReturnFocus.focus?.({ preventScroll: true });
    drawerReturnFocus = null;
    emit('panelchange', { panel: closedPanel, open: false });
  }

  function renderWordPanel() {
    wordPanel.replaceChildren();
    if (!documentArtifact) {
      const empty = host.ownerDocument.createElement('div');
      empty.className = 'ctox-empty mail-content-editor-empty';
      const title = host.ownerDocument.createElement('strong');
      title.textContent = labels.noWordTitle;
      const copy = host.ownerDocument.createElement('span');
      copy.textContent = labels.noWordBody;
      empty.append(title, copy);
      if (typeof options.onCreateWordArtifact === 'function' && !readOnly) {
        const create = host.ownerDocument.createElement('button');
        create.type = 'button';
        create.className = 'ctox-button';
        create.textContent = labels.createWord;
        create.addEventListener('click', async () => {
          create.disabled = true;
          try {
            const artifact = await options.onCreateWordArtifact();
            setDocumentArtifact(artifact);
            await openWord();
          } catch (error) {
            emit('error', { mode: EDITOR_MODES.RICH_TEXT, error });
            setStatus(error?.message || String(error), 'error');
          } finally {
            if (create.isConnected) create.disabled = false;
          }
        });
        empty.append(create);
      }
      wordPanel.append(empty);
      return;
    }

    const artifact = host.ownerDocument.createElement('article');
    artifact.className = 'mail-content-editor-artifact ctox-list-item';
    artifact.dataset.contextRecordId = documentArtifact.documentId;
    artifact.dataset.contextRecordType = 'document';
    artifact.dataset.contextLabel = documentArtifact.title || documentArtifact.documentId;

    const copy = host.ownerDocument.createElement('div');
    copy.className = 'mail-content-editor-artifact-copy';
    const kicker = host.ownerDocument.createElement('span');
    kicker.className = 'ctox-pane-kicker';
    kicker.textContent = labels.wordKicker;
    const title = host.ownerDocument.createElement('strong');
    title.textContent = documentArtifact.title || documentArtifact.documentId;
    copy.append(kicker, title);
    if (documentArtifact.versionId) {
      const version = host.ownerDocument.createElement('span');
      version.className = 'mail-content-editor-artifact-meta';
      version.textContent = `${labels.linkedVersion}: ${documentArtifact.versionId}`;
      copy.append(version);
    }

    const open = host.ownerDocument.createElement('button');
    open.type = 'button';
    open.className = 'ctox-button';
    open.textContent = labels.openWord;
    open.addEventListener('click', () => openWord().catch((error) => {
      emit('error', { mode: EDITOR_MODES.RICH_TEXT, error });
      setStatus(error?.message || String(error), 'error');
    }));
    artifact.append(copy, open);
    wordPanel.append(artifact);
  }

  async function ensureHtmlEditor() {
    assertAlive(destroyed);
    if (htmlEditor) return htmlEditor;
    if (htmlEditorPromise) return htmlEditorPromise;
    setStatus(labels.loadingHtml, 'loading');
    const pending = (async () => {
      const runtime = await loadEasyEmailRuntime(options);
      const createEditor = validateEasyEmailRuntime(runtime);
      const editor = await createEditor({
        host: htmlHost,
        document: cloneJson(htmlDocument),
        locale,
        theme: currentShellTheme(),
        readOnly,
        mergeTags: cloneJson(options.mergeTags || {}),
        panelHosts: { logic: panelHosts.logic },
        requestPanel: (name) => openHtmlPanel(name, { fromRuntime: true }),
        logicBridge: { managedExternally: true },
        onChange(change = {}) {
          const normalized = normalizeHtmlChange(change, htmlDocument);
          htmlDocument = normalized.document;
          dirty = true;
          emit('change', { mode: EDITOR_MODES.HTML, ...normalized });
        },
      });
      htmlEditor = validateEasyEmailHandle(editor);
      await htmlEditor.setViewport?.(viewport);
      // `htmlDocument` is the adapter's ordered source mirror. Logic edits are
      // serialized against it so rapid field changes cannot read an older
      // React frame snapshot between set-document handshakes.
      logicEditor = mountMailLogicEditor({
        host: panelHosts.logic,
        locale,
        readOnly,
        mergeTags: cloneJson(options.mergeTags || {}),
        getActionIcon: (name) => ctx?.getActionIcon?.(name) || '',
        getDocument: () => cloneJson(htmlDocument),
        setDocument: async (documentValue) => {
          htmlDocument = cloneJson(documentValue);
          await htmlEditor.setDocument(cloneJson(documentValue));
        },
        getSelectedBlockId: () => htmlEditor.getSelectedBlockId?.() || '',
        onSelectionChange: (listener) => htmlEditor.onSelectionChange?.(listener),
        onTestDataChange: (testData) => htmlEditor.setMergeTags?.(testData),
        onPreviewChange: (preview) => htmlEditor.setLogicPreview?.(preview),
        onChange: (detail) => {
          dirty = true;
          emit('change', { mode: EDITOR_MODES.HTML, source: 'logic', ...detail });
        },
      });
      if (destroyed) {
        logicEditor?.destroy?.();
        logicEditor = null;
        await htmlEditor.destroy();
        htmlEditor = null;
        throw new Error('Mail content editor was destroyed while the HTML editor loaded');
      }
      setStatus('', '');
      emit('ready', { mode: EDITOR_MODES.HTML });
      return htmlEditor;
    })();
    htmlEditorPromise = pending;
    try {
      return await pending;
    } catch (error) {
      htmlHost.replaceChildren();
      const failure = host.ownerDocument.createElement('div');
      failure.className = 'ctox-empty';
      const title = host.ownerDocument.createElement('strong');
      title.textContent = labels.htmlFailed;
      const detail = host.ownerDocument.createElement('span');
      detail.textContent = error?.message || String(error);
      failure.append(title, detail);
      htmlHost.append(failure);
      setStatus(labels.htmlFailed, 'error');
      emit('error', { mode: EDITOR_MODES.HTML, error });
      throw error;
    } finally {
      if (htmlEditorPromise === pending) htmlEditorPromise = null;
    }
  }

  async function openWord() {
    assertAlive(destroyed);
    if (!documentArtifact) throw new Error(labels.noWordTitle);
    const result = await openDocumentsArtifact(ctx, documentArtifact);
    emit('open', { mode: EDITOR_MODES.RICH_TEXT, artifact: documentArtifact, result });
    return result;
  }

  function setDocumentArtifact(value) {
    assertAlive(destroyed);
    documentArtifact = normalizeDocumentArtifact(value);
    renderWordPanel();
    dirty = true;
    emit('change', { mode: EDITOR_MODES.RICH_TEXT, artifact: documentArtifact });
    return documentArtifact;
  }

  async function setHtmlDocument(value) {
    assertAlive(destroyed);
    htmlDocument = cloneJson(value ?? {});
    if (htmlEditor) await htmlEditor.setDocument(cloneJson(htmlDocument));
    await logicEditor?.reload?.();
    dirty = true;
    emit('change', { mode: EDITOR_MODES.HTML, document: cloneJson(htmlDocument) });
  }

  async function serialize() {
    assertAlive(destroyed);
    if (mode === EDITOR_MODES.RICH_TEXT) {
      const compiled = documentArtifact
        ? await (options.wordCompiler || createBusinessOsOfficeBridge(ctx, 'document')).freezeEmailContent({
          recordId: documentArtifact.documentId,
          versionId: documentArtifact.versionId,
        })
        : null;
      return {
        mode,
        format: 'documents-artifact',
        documentArtifact: documentArtifact ? { ...documentArtifact } : null,
        compiledHtml: String(compiled?.html || ''),
        compiledText: String(compiled?.text || ''),
        compilerId: String(compiled?.artifact?.compiler_id || ''),
        sourceSha256: String(compiled?.artifact?.source_sha256 || ''),
        compiledHtmlRef: compiled?.artifact?.html_blob_id ? {
          storage: 'document_blob_chunks',
          blob_id: compiled.artifact.html_blob_id,
          media_type: 'text/html; charset=utf-8',
          sha256: compiled.artifact.html_sha256,
        } : null,
        compiledTextRef: compiled?.artifact?.text_blob_id ? {
          storage: 'document_blob_chunks',
          blob_id: compiled.artifact.text_blob_id,
          media_type: 'text/plain; charset=utf-8',
          sha256: compiled.artifact.text_sha256,
        } : null,
        compiledAssets: cloneJson(compiled?.artifact?.assets || []),
        diagnostics: cloneJson(compiled?.artifact?.diagnostics || []),
      };
    }
    const editor = await ensureHtmlEditor();
    await logicEditor?.flush?.();
    const documentValue = cloneJson(htmlDocument);
    htmlDocument = documentValue;
    const [html, mjml] = await Promise.all([
      editor.getHtml(),
      editor.getMjml?.() || '',
    ]);
    const text = emailHtmlToText(html);
    return {
      mode,
      format: 'easy-email-mjml',
      htmlDocument: documentValue,
      html: String(html || ''),
      mjml: String(mjml || ''),
      text,
      compiledHtml: String(html || ''),
      compiledText: text,
      compilerId: 'easy-email-editor@4.17.1+ctox-mjml-browser',
    };
  }

  function focus() {
    if (destroyed) return;
    if (mode === EDITOR_MODES.HTML) {
      htmlEditor?.focus?.();
      return;
    }
    wordPanel.querySelector('button')?.focus?.();
  }

  async function setReadOnly(nextReadOnly) {
    assertAlive(destroyed);
    readOnly = nextReadOnly === true;
    if (htmlEditor?.setReadOnly) await htmlEditor.setReadOnly(readOnly);
    logicEditor?.setReadOnly?.(readOnly);
    renderWordPanel();
  }

  renderWordPanel();
  renderMode();
  if (mode === EDITOR_MODES.HTML) await ensureHtmlEditor();

  return Object.freeze({
    get mode() { return mode; },
    get dirty() { return dirty; },
    get documentArtifact() { return documentArtifact ? { ...documentArtifact } : null; },
    setMode,
    setDocumentArtifact,
    setHtmlDocument,
    openWord,
    serialize,
    focus,
    setReadOnly,
    openHtmlPanel,
    closeHtmlPanel,
    markClean() { dirty = false; },
    on(name, listener) {
      if (typeof listener !== 'function') throw new TypeError('Mail content editor listener must be a function');
      const set = listeners.get(String(name)) || new Set();
      set.add(listener);
      listeners.set(String(name), set);
      return () => set.delete(listener);
    },
    async destroy() {
      if (destroyed) return;
      destroyed = true;
      const pending = htmlEditorPromise;
      if (pending) await pending.catch(() => null);
      await logicEditor?.flush?.();
      logicEditor?.destroy?.();
      logicEditor = null;
      await htmlEditor?.destroy?.();
      htmlEditor = null;
      listeners.clear();
      root.removeEventListener('keydown', handleEscape);
      commandbar.remove();
      root.remove();
    },
  });
}

export function normalizeEditorMode(value) {
  const mode = String(value || EDITOR_MODES.RICH_TEXT).trim().toLowerCase();
  if (mode === EDITOR_MODES.RICH_TEXT || mode === 'rich' || mode === 'word') return EDITOR_MODES.RICH_TEXT;
  if (mode === EDITOR_MODES.HTML || mode === 'easy-email') return EDITOR_MODES.HTML;
  throw new TypeError(`Unsupported mail editor mode: ${value}`);
}

export function normalizeDocumentArtifact(value) {
  if (value == null || value === '') return null;
  if (typeof value !== 'object' || Array.isArray(value)) throw new TypeError('Word artifact must be an object');
  const documentId = String(value.documentId || value.document_id || value.recordId || value.record_id || '').trim();
  if (!documentId) throw new TypeError('Word artifact requires documentId');
  const versionId = String(value.versionId || value.version_id || '').trim();
  const title = String(value.title || value.label || '').trim();
  return Object.freeze({
    documentId,
    versionId,
    title,
    deepLink: buildDocumentsDeepLink({ documentId, versionId }),
  });
}

export function buildDocumentsDeepLink(value = {}) {
  const documentId = String(value.documentId || value.document_id || '').trim();
  if (!documentId) return '#documents';
  const params = new URLSearchParams();
  // `record`/`version` are consumed by the current Documents module. The
  // canonical `record_id` alias keeps the reference useful to Threads and the
  // shell record-focus contract as it converges.
  params.set('record', documentId);
  params.set('record_id', documentId);
  const versionId = String(value.versionId || value.version_id || '').trim();
  if (versionId) {
    params.set('version', versionId);
    params.set('version_id', versionId);
  }
  return `#documents?${params.toString()}`;
}

export async function openDocumentsArtifact(ctx, value) {
  const artifact = normalizeDocumentArtifact(value);
  if (!artifact) throw new TypeError('A Word artifact is required');
  if (typeof ctx?.openDesktopApp !== 'function') {
    const error = new Error('Business OS document launcher is unavailable');
    error.code = 'documents_launcher_unavailable';
    throw error;
  }
  return ctx.openDesktopApp('documents', {
    args: {
      record: artifact.documentId,
      record_id: artifact.documentId,
      documentId: artifact.documentId,
      ...(artifact.versionId ? {
        version: artifact.versionId,
        version_id: artifact.versionId,
        versionId: artifact.versionId,
      } : {}),
    },
  });
}

export function validateEasyEmailRuntime(runtime) {
  const factory = runtime?.createEasyEmailEditor || runtime?.default?.createEasyEmailEditor;
  if (typeof factory !== 'function') {
    throw new TypeError('Local Easy Email runtime must export createEasyEmailEditor(options)');
  }
  return factory;
}

export function validateEasyEmailHandle(editor) {
  if (!editor || typeof editor !== 'object') throw new TypeError('Easy Email runtime returned no editor handle');
  for (const method of [
    'getDocument',
    'getHtml',
    'setDocument',
    'getSelectedBlockId',
    'onSelectionChange',
    'setMergeTags',
    'focus',
    'destroy',
  ]) {
    if (typeof editor[method] !== 'function') throw new TypeError(`Easy Email editor handle is missing ${method}()`);
  }
  return editor;
}

async function loadEasyEmailRuntime(options) {
  if (options.easyEmailRuntime) return options.easyEmailRuntime;
  if (typeof options.loadEasyEmailRuntime === 'function') return options.loadEasyEmailRuntime();
  const url = String(options.easyEmailModuleUrl || DEFAULT_EASY_EMAIL_MODULE);
  return import(new URL(url, import.meta.url).href);
}

function normalizeHtmlChange(change, fallbackDocument) {
  if (change && typeof change === 'object' && Object.hasOwn(change, 'document')) {
    return {
      document: cloneJson(change.document ?? {}),
      html: typeof change.html === 'string' ? change.html : undefined,
      text: typeof change.text === 'string' ? change.text : undefined,
    };
  }
  return { document: cloneJson(change ?? fallbackDocument ?? {}) };
}

function modeButton(doc, value, label) {
  const button = doc.createElement('button');
  button.type = 'button';
  button.className = 'ctox-pane-tab mail-content-editor-tab';
  button.dataset.mailEditorMode = value;
  button.setAttribute('role', 'tab');
  button.textContent = label;
  return button;
}

function setActionIcon(button, ctx, name, fallback) {
  const icon = ctx?.getActionIcon?.(name);
  if (typeof icon === 'string' && icon.trim()) button.innerHTML = icon;
  else button.textContent = fallback;
}

function keepFocusInDrawer(event, drawer) {
  const focusable = [...drawer.querySelectorAll([
    'button:not([disabled])',
    'input:not([disabled])',
    'select:not([disabled])',
    'textarea:not([disabled])',
    'a[href]',
    '[tabindex]:not([tabindex="-1"])',
  ].join(','))].filter((element) => !element.hidden && element.getAttribute('aria-hidden') !== 'true');
  if (!focusable.length) {
    event.preventDefault();
    drawer.focus?.();
    return;
  }
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  const active = drawer.ownerDocument.activeElement;
  if (event.shiftKey && (active === first || !drawer.contains(active))) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && active === last) {
    event.preventDefault();
    first.focus();
  }
}

function normalizeLocale(value) {
  return String(value || 'de').toLowerCase().startsWith('en') ? 'en' : 'de';
}

function currentShellTheme() {
  const value = document.documentElement?.dataset?.theme;
  return value === 'light' || value === 'dark' ? value : 'system';
}

function cloneJson(value) {
  if (value == null) return value;
  return typeof structuredClone === 'function'
    ? structuredClone(value)
    : JSON.parse(JSON.stringify(value));
}

function emailHtmlToText(value) {
  return String(value || '')
    .replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, '')
    .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, '')
    .replace(/<br\s*\/?>/gi, '\n')
    .replace(/<\/(?:p|div|h[1-6]|li|tr)>/gi, '\n')
    .replace(/<li\b[^>]*>/gi, '- ')
    .replace(/<[^>]+>/g, ' ')
    .replace(/&nbsp;/gi, ' ')
    .replace(/&amp;/gi, '&')
    .replace(/&lt;/gi, '<')
    .replace(/&gt;/gi, '>')
    .replace(/&quot;/gi, '"')
    .replace(/&#39;/gi, "'")
    .replace(/[ \t]+/g, ' ')
    .replace(/ *\n */g, '\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

function isElement(value) {
  return typeof Element === 'undefined'
    ? Boolean(value && value.nodeType === 1 && typeof value.replaceChildren === 'function')
    : value instanceof Element;
}

function assertAlive(destroyed) {
  if (destroyed) throw new Error('Mail content editor is destroyed');
}

async function ensureStyles(doc) {
  if (doc.querySelector('link[data-mail-content-editor-style]')) return;
  const link = doc.createElement('link');
  link.rel = 'stylesheet';
  link.dataset.mailContentEditorStyle = STYLE_REVISION;
  const url = new URL('./editor.css', import.meta.url);
  url.searchParams.set('v', STYLE_REVISION);
  link.href = url.href;
  (doc.head || doc.documentElement).append(link);
}

export const MAIL_EDITOR_MODES = EDITOR_MODES;
