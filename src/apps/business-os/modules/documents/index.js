import { showBusinessConfirm } from '../../shared/dialogs.js?v=20260816-browser-sync-guards-v141';
import { loadModuleMessages } from '../../shared/i18n.js';
import { preserveScrollDuring } from '../../shared/stable-dom.js';
import { createCoalescedRefresh } from '../../office-engine/src/coalesced-refresh.mjs';
import { autoWirePaneGrammar } from '../../shared/pane-grammar.js';
import { createBusinessOsOfficeBridge } from '../../office-engine/src/business-os-bridge.mjs?v=20260908-office-source-upload-v1';

const DOCX_MIME = 'application/vnd.openxmlformats-officedocument.wordprocessingml.document';
const MARKDOWN_MIME = 'text/markdown';
const CTOX_DOCUMENTS_LOAD_TIMEOUT_MS = 60000;
const CTOX_DOCUMENTS_READY_TIMEOUT_MS = 120000;
const CTOX_DOCUMENTS_RECOVERY_DELAY_MS = 1200;
const CTOX_DOCUMENTS_MAX_RECOVERY_ATTEMPTS = 3;
const EDITOR_VERSION_CONTENT_READY_TIMEOUT_MS = 900;
const EDITOR_VERSION_CONTENT_READY_POLL_MS = 25;
const CHUNK_SIZE = 256000;
const DOCUMENT_BLOB_CACHE_MAX_ENTRIES = 64;
const DOCUMENT_BLOB_CACHE_MAX_BYTES = 16 * 1024 * 1024;
const DOCX_TOOLBAR_VISIBILITY_KEY = 'ctox.businessOs.documents.docxToolbarVisible';
const DOCUMENT_RENDER_DEBOUNCE_MS = 80;
const DOCUMENTS_ASSET_REVISION = new URL(import.meta.url).searchParams.get('v') || '20260904-office-shell-v2';
const MAIL_MERGE_SOURCE_NAMES = new Set([
  'campaign-mail-merge',
  'mail-merge',
  'mail_merge',
  'series-letter',
  'series_letter',
  'serienbrief',
]);
const SYSTEMATIC_REPORT_RUNBOOKS = [
  {
    id: 'research.report.auto',
    document_type: 'word_document',
    title: 'Deep Research Word-Bericht',
    description: 'CTOX wählt den passenden Report-Typ und erstellt ein belastbares Word-Dokument.',
    command_type: 'research.systematic.report.create',
    report_type: 'auto',
    prompt_template: 'Nutze den systematic-research Skill. Erstelle ein hochwertiges Word-Dokument (.docx), nicht Markdown. Führe Deep Research aus, nutze belastbare Quellen, baue sinnvolle Tabellen und Abbildungen ein und rendere das Ergebnis als DOCX.',
  },
  {
    id: 'research.report.feasibility_study',
    document_type: 'word_document',
    title: 'Machbarkeitsstudie',
    description: 'Decision-grade Machbarkeitsstudie mit Evidenz, Bewertung und DOCX-Render.',
    command_type: 'research.systematic.report.create',
    report_type: 'feasibility_study',
    prompt_template: 'Erstelle eine Machbarkeitsstudie als Word-Dokument via ctox report report_type=feasibility_study.',
  },
  {
    id: 'research.report.market_research',
    document_type: 'word_document',
    title: 'Marktanalyse',
    description: 'Marktgröße, Segmente, Treiber, Wettbewerb, Barrieren und Empfehlung.',
    command_type: 'research.systematic.report.create',
    report_type: 'market_research',
    prompt_template: 'Erstelle eine Marktanalyse als Word-Dokument via ctox report report_type=market_research.',
  },
  {
    id: 'research.report.competitive_analysis',
    document_type: 'word_document',
    title: 'Wettbewerbsanalyse',
    description: 'Wettbewerber-Set, Bewertungsachsen, Matrix, Positionierung, Lücken und Empfehlung.',
    command_type: 'research.systematic.report.create',
    report_type: 'competitive_analysis',
    prompt_template: 'Erstelle eine Wettbewerbsanalyse als Word-Dokument via ctox report report_type=competitive_analysis.',
  },
  {
    id: 'research.report.technology_screening',
    document_type: 'word_document',
    title: 'Technologie-Screening',
    description: 'Optioneninventar, Kriterien, Shortlist und nächste Schritte.',
    command_type: 'research.systematic.report.create',
    report_type: 'technology_screening',
    prompt_template: 'Erstelle ein Technologie-Screening als Word-Dokument via ctox report report_type=technology_screening.',
  },
  {
    id: 'research.report.whitepaper',
    document_type: 'word_document',
    title: 'Whitepaper',
    description: 'These, Kontext, Argumente, Gegenargumente, Implikationen und Position.',
    command_type: 'research.systematic.report.create',
    report_type: 'whitepaper',
    prompt_template: 'Erstelle ein Whitepaper als Word-Dokument via ctox report report_type=whitepaper.',
  },
  {
    id: 'research.report.literature_review',
    document_type: 'word_document',
    title: 'Stand der Technik',
    description: 'Literature Review mit Themen, Synthese, Lücken und offenen Fragen.',
    command_type: 'research.systematic.report.create',
    report_type: 'literature_review',
    prompt_template: 'Erstelle eine Literature Review als Word-Dokument via ctox report report_type=literature_review.',
  },
  {
    id: 'research.report.decision_brief',
    document_type: 'word_document',
    title: 'Entscheidungsvorlage',
    description: 'Kurzes empfehlungsorientiertes Memo mit Optionen, Kriterien und Bewertung.',
    command_type: 'research.systematic.report.create',
    report_type: 'decision_brief',
    prompt_template: 'Erstelle eine Entscheidungsvorlage als Word-Dokument via ctox report report_type=decision_brief.',
  },
  {
    id: 'research.report.project_description',
    document_type: 'word_document',
    title: 'Projektbeschreibung / Fördervorhaben',
    description: 'Unternehmens- und Projektbeschreibung für Innovations- und Fördervorhaben.',
    command_type: 'research.systematic.report.create',
    report_type: 'project_description',
    prompt_template: 'Erstelle eine Projektbeschreibung/Fördervorhabenbeschreibung als Word-Dokument via ctox report report_type=project_description.',
  },
  {
    id: 'research.report.source_review',
    document_type: 'word_document',
    title: 'Quellenreview',
    description: 'Systematische Quellen- und Datenquellenrecherche mit Taxonomie und Priorisierung.',
    command_type: 'research.systematic.report.create',
    report_type: 'source_review',
    prompt_template: 'Erstelle ein Quellenreview als Word-Dokument via ctox report report_type=source_review.',
  },
];

// Shell V2: the pane heads are static markup in index.html (reference:
// modules/tickets, modules/knowledge). Everything language-dependent in that
// markup is written here once, right after mount() injects the file — the head
// itself is never rebuilt afterwards.
function applyStaticLabels(host, t) {
  // Liste und Editor bekommen bewusst UNTERSCHIEDLICHE Lade-Texte: zwei
  // identische "Workspace wird geladen."-Karten nebeneinander wirkten wie ein
  // Render-Duplikat (Befund 31.08.2026, Geometry-Lab-Screenshot).
  for (const placeholder of host.querySelectorAll('[data-documents-list-placeholder]')) {
    const title = placeholder.querySelector('strong');
    if (title) title.textContent = t('documentsTitle', 'Dokumente');
    const copy = placeholder.querySelector('span');
    if (copy) copy.textContent = t('listLoading', 'Dokumente werden geladen.');
  }
  for (const placeholder of host.querySelectorAll('.documents-editor-shell > .ctox-empty')) {
    const title = placeholder.querySelector('strong');
    if (title) title.textContent = t('documentsTitle', 'Dokumente');
    const copy = placeholder.querySelector('span');
    if (copy) copy.textContent = t('documentPreparing', 'Dokument wird vorbereitet.');
  }
  const title = host.querySelector('[data-documents-title]');
  if (title) title.textContent = t('documentsTitle', 'Dokumente');

  const label = (selector, text) => {
    const element = host.querySelector(selector);
    if (!element) return;
    element.setAttribute('aria-label', text);
    element.setAttribute('title', text);
  };
  label('[data-documents-new-markdown]', t('createBlankDocument', 'Leeres Word-Dokument erstellen'));
  label('[data-documents-import-open]', t('importDocument', 'Dokument importieren'));
  label('[data-documents-export]', t('exportSelected', 'Ausgewähltes Dokument exportieren'));
  label('[data-documents-filter-toggle]', t('filters', 'Filter'));

  const search = host.querySelector('[data-documents-search]');
  if (search) {
    search.placeholder = t('searchPlaceholder', 'Dokument suchen...');
    search.setAttribute('aria-label', t('searchLabel', 'Dokumente suchen'));
  }
  const ariaOnly = [
    ['[data-documents-sort]', t('sortLabel', 'Dokumente sortieren')],
    ['[data-documents-type]', t('typeFilterLabel', 'Dokumenttyp filtern')],
    ['[data-documents-status]', t('statusFilterLabel', 'Dokumentstatus filtern')],
    ['[data-documents-app]', t('appFilterLabel', 'Ersteller-App filtern')],
    ['[data-documents-source]', t('sourceFilterLabel', 'Quelle filtern')],
    ['[data-documents-tag]', t('tagFilterLabel', 'Dokument-Tags filtern')],
    ['[data-documents-active-filters]', t('activeFilters', 'Aktive Filter')],
  ];
  for (const [selector, text] of ariaOnly) host.querySelector(selector)?.setAttribute('aria-label', text);

  const sortLabels = [
    ['updated_desc', t('sortByNewest', 'Zuletzt geändert')],
    ['updated_asc', t('sortByOldest', 'Älteste zuerst')],
    ['title_asc', t('sortByTitle', 'Titel A-Z')],
    ['creator_app', t('sortByCreatorApp', 'Ersteller-App')],
    ['status', t('sortByStatus', 'Status')],
  ];
  const sort = host.querySelector('[data-documents-sort]');
  for (const [value, text] of sortLabels) {
    const option = sort?.querySelector(`option[value="${value}"]`);
    if (option) option.textContent = text;
  }
  const reset = host.querySelector('.documents-filter-panel [data-documents-clear-filters]');
  if (reset) reset.textContent = t('clearFilters', 'Filter zurücksetzen');
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

  const html = await fetch(revisionedModuleAssetUrl('./index.html')).then((res) => res.text());
  ctx.host.innerHTML = html;
  // Documents is a two-pane app: file management on the left and Word in the
  // remaining workspace. The shell's generic right pane must not reserve any
  // width for this module.
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();
  const shellRightPane = ctx.right?.closest?.('[data-right-pane]') || null;
  const shellRightWasHidden = shellRightPane?.hidden;
  const shellLeftPane = ctx.left?.closest?.('[data-left-pane]') || null;
  const shellLeftWasHidden = shellLeftPane?.hidden;
  if (shellLeftPane) shellLeftPane.hidden = true;
  if (shellRightPane) shellRightPane.hidden = true;
  applyStaticLabels(ctx.host, t);
  const requestedDocumentId = documentIdFromLaunchArgs(ctx.args);
  const requestedVersionId = versionIdFromLaunchArgs(ctx.args);

  const state = {
    ctx,
    formatModule: null,
    formatModuleLoadPromise: null,
    superdocModule: null,
    ctoxDocumentsModule: null,
    officeEngine: 'ctox_documents',
    documents: [],
    runbooks: [],
    knowledgeItems: [],
    knowledgeRunbooks: [],
    knowledgeTables: [],
    selectedId: '',
    requestedSelectedId: requestedDocumentId,
    selectedVersion: null,
    requestedVersionId,
    requestedVersionDocumentId: requestedVersionId ? requestedDocumentId : '',
    mailMergeNavigation: null,
    editorHandle: null,
    editorDestroyPromise: null,
    editorMountKey: '',
    editorMountPromise: null,
    superdocSaveTimer: null,
    superdocSavePromise: null,
    renderSerial: 0,
    switchSerial: 0,
    needsFinalSave: false,
    dirty: false,
    searchQuery: '',
    typeFilter: 'all',
    statusFilter: 'all',
    appFilter: 'all',
    sourceFilter: 'all',
    tagFilter: 'all',
    sortBy: 'updated_desc',
    filtersOpen: false,
    listView: 'list',
    actionsOpen: false,
    libraryOpen: false,
    docxToolbarVisible: ctx?.storageScope?.get?.(DOCX_TOOLBAR_VISIBILITY_KEY) !== 'false',
    localSubscriptionCleanup: null,
    launchCleanup: null,
    contextMenu: null,
    contextMenuCleanup: null,
    openFileToken: null,
    openFilePromise: Promise.resolve(),
    blobByteCache: createDocumentBlobByteCache(),
    blobPrefetchController: null,
    blobPrefetchTask: null,
    disposed: false,
    t,
    lang: ctx.locale === 'en' ? 'en' : 'de',
  };

  const hasPendingEditorChanges = () => Boolean(
    state.dirty || state.needsFinalSave || state.editorHandle?.saving || state.superdocSavePromise,
  );
  const unregisterCloseGuard = ctx.windowManager?.registerCloseGuard?.(ctx.host, async () => {
    try {
      await withTimeout(
        flushActiveEditorDraft(state, undefined, { allowFailure: false, timeoutMs: 90000 }),
        90000,
        state.t('draftSaveTimeout', 'Automatische Draft-Speicherung beim Dokumentwechsel hat zu lange gedauert.'),
      );
      if (hasPendingEditorChanges()) {
        ctx.notifications?.show?.({ type: 'error', message: state.t('draftSavePending', 'Weitere Änderungen sind noch nicht gespeichert. Bitte nach dem Speichern erneut versuchen, das Dokument zu wechseln.') });
        return false;
      }
      return true;
    } catch (error) {
      ctx.notifications?.show?.({ type: 'error', message: error?.message || String(error) });
      return false;
    }
  });
  const preventUnsavedUnload = (event) => {
    if (!hasPendingEditorChanges()) return;
    event.preventDefault();
    event.returnValue = '';
  };
  window.addEventListener('beforeunload', preventUnsavedUnload);

  state.launchCleanup = wireModule(state);
  state.paneCleanup = wireDocumentPanes(state);
  state.openFileToken = ctx.eventBus?.on?.('desktop-app:open-file', (payload = {}) => {
    if (payload.appId !== 'documents') return;
    enqueueDocumentOpenFile(state, payload.args?.openFile);
  }) || null;
  state.localSubscriptionCleanup = wireLocalRealtime(state);
  // Mount the workbench immediately. Seed/query work can legitimately wait
  // for WebRTC catch-up and must not leave the window-manager promise pending
  // after the visible app is already usable.
  renderLeft(state);
  renderRight(state);
  renderCenter(state);
  Promise.resolve()
    .then(() => ensureSeedRunbooks(ctx))
    .then(() => Promise.all([refreshRunbooks(state), refreshDocuments(state), refreshOfficeEngineSettings(state)]))
    .then(async () => {
      if (state.disposed) return;
      if (ctx.args?.openFile) await enqueueDocumentOpenFile(state, ctx.args.openFile);
      if (state.disposed) return;
      renderLeft(state);
      renderRight(state);
      renderCenter(state);
    })
    .catch((error) => {
      if (!state.disposed) renderError(state, error?.message || String(error));
    });
  return () => {
    state.disposed = true;
    unregisterCloseGuard?.();
    window.removeEventListener('beforeunload', preventUnsavedUnload);
    if (state.superdocSaveTimer) clearTimeout(state.superdocSaveTimer);
    state.contextMenuCleanup?.();
    if (state.openFileToken) ctx.eventBus?.off?.('desktop-app:open-file', state.openFileToken);
    state.contextMenu?.remove();
    state.contextMenu = null;
    state.localSubscriptionCleanup?.();
    state.launchCleanup?.();
    state.paneCleanup?.();
    clearDocumentBlobByteCache(state);
    state.editorHandle?.destroy?.();
    if (shellRightPane) shellRightPane.hidden = shellRightWasHidden;
    if (shellLeftPane) shellLeftPane.hidden = shellLeftWasHidden;
  };
}

function enqueueDocumentOpenFile(state, input) {
  if (!input?.file) return state.openFilePromise;
  state.openFilePromise = state.openFilePromise
    .then(() => openDocumentFile(state, input))
    .catch((error) => {
      console.error('[documents] opening file from Files failed', error);
      renderError(state, error?.message || String(error));
      return null;
    });
  return state.openFilePromise;
}

async function openDocumentFile(state, input) {
  const file = input?.file;
  const validation = validateImportInput({ file });
  if (!validation.valid) throw new Error(state.t(validation.key, validation.message));
  const bytes = new Uint8Array(await file.arrayBuffer());
  const sourceSha = await sha256Hex(bytes);
  await refreshDocuments(state);
  const existing = documentBySourceSha(state.documents, sourceSha);
  if (existing) {
    state.selectedId = existing.id;
    state.selectedVersion = null;
    await loadSelectedVersion(state);
    renderLeft(state);
    renderRight(state);
    renderCenter(state);
    return existing;
  }
  return importDocumentFile(state, file);
}

function documentBySourceSha(records = [], sourceSha = '') {
  const expected = String(sourceSha || '').trim().toLowerCase();
  if (!expected) return null;
  return records.find((record) => String(record?.source_sha256 || '').trim().toLowerCase() === expected) || null;
}

async function loadDocumentFormatModule() {
  return import('../../vendor/document-format.mjs?v=20260904-office-shell-v2');
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

async function loadCtoxDocumentsModule(state) {
  if (!state.ctoxDocumentsModule) {
    state.ctoxDocumentsModule = await import('../../vendor/ctox-office/ctox-office-document.mjs?v=20260904-office-shell-v2');
  }
  return state.ctoxDocumentsModule;
}

async function refreshOfficeEngineSettings(state) {
  const collection = state.ctx.db?.collection?.('ctox_runtime_settings');
  if (!collection) return state.officeEngine;
  const settings = await collection.findOne('runtime-settings').exec();
  const projected = settings?.toJSON?.() || settings;
  state.officeEngine = officeEngineFromSettings(projected);
  return state.officeEngine;
}

function officeEngineFromSettings(settings) {
  return settings?.office?.documents_engine === 'legacy' ? 'legacy' : 'ctox_documents';
}

function wireModule(state) {
  const refreshLeft = () => renderLeft(state);
  const handleAppLaunch = (event) => {
    if (event?.detail?.appId && event.detail.appId !== state.ctx.module?.id) return;
    const documentId = documentIdFromLaunchArgs(event?.detail?.args);
    const versionId = versionIdFromLaunchArgs(event?.detail?.args);
    if (!documentId) return;
    if (state.documents.some((record) => record.id === documentId)) {
      switchSelectedDocument(state, documentId, { versionId }).catch((error) => {
        console.error('[documents] requested document could not be opened', error);
      });
      return;
    }
    state.requestedSelectedId = documentId;
    state.requestedVersionId = versionId;
    state.requestedVersionDocumentId = versionId ? documentId : '';
    state.selectedVersion = null;
    refreshDocumentsFromLocal(state).catch((error) => {
      console.error('[documents] requested document could not be opened', error);
    });
  };
  state.ctx.host.addEventListener('documents:refresh-left', refreshLeft);
  state.ctx.host.addEventListener('ctox-business-os-app-launch', handleAppLaunch);
  return () => {
    state.ctx.host.removeEventListener('documents:refresh-left', refreshLeft);
    state.ctx.host.removeEventListener('ctox-business-os-app-launch', handleAppLaunch);
  };
}

function wireDocumentPanes(state) {
  const root = state.ctx.host.querySelector('[data-documents-module]');
  const backdrop = root?.querySelector('[data-documents-drawer-backdrop]');
  const closeDrawers = () => {
    state.actionsOpen = false;
    state.libraryOpen = false;
    renderPaneVisibility(state);
    renderDocumentStrip(state);
  };
  const handleKeydown = (event) => {
    if (event.key !== 'Escape' || (!state.actionsOpen && !state.libraryOpen)) return;
    event.preventDefault();
    closeDrawers();
  };
  const updateResponsiveMode = (width = root?.getBoundingClientRect?.().width || 0) => {
    if (!root) return;
    root.classList.toggle('is-compact', width <= 768);
    root.classList.toggle('is-narrow', width <= 620);
    root.classList.toggle('is-phone', width <= 440);
    root.classList.toggle('is-actions-overlay', width < 1616);
    if (width > 1024 && state.libraryOpen) {
      state.libraryOpen = false;
      renderPaneVisibility(state);
    }
  };
  const resizeObserver = typeof ResizeObserver === 'function' && root
    ? new ResizeObserver((entries) => updateResponsiveMode(entries[0]?.contentRect?.width))
    : null;
  const handleWindowResize = () => updateResponsiveMode();
  backdrop?.addEventListener('click', closeDrawers);
  root?.addEventListener('keydown', handleKeydown);
  resizeObserver?.observe(root);
  window.addEventListener('resize', handleWindowResize);
  updateResponsiveMode();
  renderPaneVisibility(state);
  return () => {
    backdrop?.removeEventListener('click', closeDrawers);
    root?.removeEventListener('keydown', handleKeydown);
    resizeObserver?.disconnect();
    window.removeEventListener('resize', handleWindowResize);
  };
}

function renderPaneVisibility(state) {
  const root = state.ctx.host.querySelector('[data-documents-module]');
  const actions = root?.querySelector('[data-documents-actions-drawer]');
  const actionsResizer = root?.querySelector('[data-documents-actions-resizer]');
  const backdrop = root?.querySelector('[data-documents-drawer-backdrop]');
  const anyDrawerOpen = Boolean(state.actionsOpen || state.libraryOpen);
  root?.classList.toggle('is-actions-open', Boolean(state.actionsOpen));
  root?.classList.toggle('is-library-open', Boolean(state.libraryOpen));
  if (actions) {
    actions.hidden = !state.actionsOpen;
    actions.setAttribute('aria-hidden', String(!state.actionsOpen));
  }
  if (actionsResizer) actionsResizer.hidden = !state.actionsOpen;
  if (backdrop) backdrop.hidden = !anyDrawerOpen;
}

function documentIdFromLaunchArgs(args) {
  return String(args?.record || args?.documentId || '').trim();
}

function versionIdFromLaunchArgs(args) {
  return String(args?.version || args?.versionId || '').trim();
}

function initDocumentsContextMenu(state) {
  state.contextMenu?.remove();
  const menu = document.createElement('div');
  menu.className = 'ctox-context-menu documents-context-menu';
  menu.hidden = true;
  // Nothing the app draws may leave the app window: the menu is positioned
  // inside the module root, never on document.body.
  const root = state.ctx.host.querySelector('[data-documents-module]') || state.ctx.host;
  root.append(menu);
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

  window.addEventListener('click', handleOutsideClick, { capture: true });
  window.addEventListener('keydown', handleEscape);

  return () => {
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
          <strong>${escapeHtml(state.t('chatToCtox', 'CTOX beauftragen'))}</strong>
          <span>${escapeHtml(documentContextSummary(context))}</span>
        </div>
        <button type="button" data-documents-context-close aria-label="${escapeHtml(state.t('close', 'Schließen'))}">×</button>
      </header>
      <div class="documents-context-mode" role="radiogroup" aria-label="${escapeHtml(state.t('chatActionLabel', 'CTOX Aufgabe'))}">
        <label><input type="radio" name="contextMode" value="data" checked /> ${escapeHtml(state.t('chatWorkDataLabel', 'Mit Daten arbeiten'))}</label>
        <label><input type="radio" name="contextMode" value="ask" /> ${escapeHtml(state.t('chatAnswerLabel', 'Frage beantworten'))}</label>
        ${canModifyApp ? `<label><input type="radio" name="contextMode" value="app" /> ${escapeHtml(state.t('chatModifyAppLabel', 'App modifizieren'))}</label>` : ''}
      </div>
      <textarea data-documents-context-message placeholder="${escapeHtml(state.t('chatPlaceholder', 'Was soll CTOX hier tun oder prüfen?'))}"></textarea>
      <footer>
        <span data-documents-context-status></span>
        <button type="submit">${escapeHtml(state.t('send', 'Senden'))}</button>
      </footer>
    </form>
  `;
  state.contextMenu.hidden = false;
  state.contextMenu.style.left = '0px';
  state.contextMenu.style.top = '0px';
  // Positioned inside the module root, so the click point has to be converted
  // from viewport coordinates into that root's own box and clamped to it.
  const rect = state.contextMenu.getBoundingClientRect();
  const rootRect = state.contextMenu.parentElement.getBoundingClientRect();
  const maxLeft = Math.max(8, rootRect.width - rect.width - 8);
  const maxTop = Math.max(8, rootRect.height - rect.height - 8);
  state.contextMenu.style.left = `${clampNumber(x - rootRect.left, 8, maxLeft)}px`;
  state.contextMenu.style.top = `${clampNumber(y - rootRect.top, 8, maxTop)}px`;

  const form = state.contextMenu.querySelector('[data-documents-context-chat-form]');
  const textarea = state.contextMenu.querySelector('[data-documents-context-message]');
  state.contextMenu.querySelector('[data-documents-context-close]')?.addEventListener('click', () => hideDocumentsContextMenu(state));
  form?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const mode = new FormData(form).get('contextMode') || 'data';
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
    .join(' · ') || 'Dokumente';
}

async function dispatchDocumentsContextChat(state, context, message, mode = 'data') {
  const trimmed = String(message || '').trim();
  const status = state.contextMenu?.querySelector('[data-documents-context-status]');
  if (!trimmed) {
    if (status) status.textContent = state.t('chatMissingMessage', 'Nachricht fehlt.');
    return;
  }

  const safeMode = mode === 'app' && canModifyDocumentsApp(state) ? 'app' : (mode === 'ask' ? 'ask' : 'data');
  const record = state.documents.find((item) => item.id === context.record_id) || selectedRecord(state);
  const runbookId = defaultRunbookId(state);
  const runbook = state.runbooks.find((item) => item.id === runbookId || item.command_type === runbookId) || null;
  if (!document.querySelector('[data-ctox-chat-root]')) {
    if (status) status.textContent = state.t('chatNotReady', 'Chat ist noch nicht bereit.');
    return;
  }
  const openBusinessChat = state.ctx?.openBusinessChat || state.ctx?.businessChat?.open;
  if (typeof openBusinessChat !== 'function') {
    if (status) status.textContent = state.t('chatNotReady', 'Chat ist noch nicht bereit.');
    return;
  }
  if (status) status.textContent = state.t('chatOpening', 'Chat wird geöffnet...');
  const titlePrefix = safeMode === 'app'
    ? state.t('chatModifyAppTitle', 'Dokumente-App anpassen')
    : safeMode === 'ask'
      ? state.t('chatAnswerLabel', 'Frage beantworten')
      : state.t('chatWorkDataTitle', 'Mit Dokumenten arbeiten');
  const title = `${titlePrefix} · ${context.label || record?.title || context.column || 'Dokumente'}`;
  const instruction = safeMode === 'app'
    ? state.t('chatModifyAppInstruction', `Passe die Dokumente-App anhand dieser Admin-Anweisung an. Kontext nur als UI-Bezug verwenden, Dokumentdaten selbst nicht als primäres Ziel verändern.\n\n{0}`, trimmed)
    : safeMode === 'ask'
      ? `Beantworte die folgende Frage ausschließlich lesend. Nutze nur vorhandene Daten und Kontext; führe keine Änderungen an Daten, Records, Dateien oder der App aus. Antworte knapp und direkt.\n\n${trimmed}`
      : trimmed;
  openBusinessChat({
    text: trimmed,
    draft: trimmed,
    module: 'documents',
    source_module: 'documents',
    source_title: 'Dokumente',
    action: 'context-chat',
    reuseActive: false,
    command_type: safeMode === 'app' ? 'ctox.business_os.app.modify' : 'business_os.chat.task',
    record_id: safeMode === 'app' ? 'documents' : (record?.id || context.record_id || 'documents'),
    title,
    command_title: title,
    instruction,
    mode: safeMode,
    target: safeMode === 'app' ? 'app' : (safeMode === 'ask' ? 'read' : 'data'),
    payload: {
      title,
      instruction,
      prompt: trimmed,
      user_message: trimmed,
      mode: safeMode,
      target: safeMode === 'app' ? 'app' : (safeMode === 'ask' ? 'read' : 'data'),
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
  });
  hideDocumentsContextMenu(state);
}

function wireLocalRealtime(state) {
  const collections = ['documents', 'document_versions', 'document_runbooks', 'document_blob_chunks', 'knowledge_items', 'knowledge_runbooks', 'knowledge_tables'];
  let disposed = false;
  const refresh = createCoalescedRefresh({
    delayMs: DOCUMENT_RENDER_DEBOUNCE_MS,
    refresh: (changed, isActive) => refreshDocumentsFromLocal(state, changed, isActive),
    onError: (error) => {
      console.warn('[documents] local realtime render failed', error);
    },
  });
  const subscriptions = collections
    .map((collectionName) => documentCollection(state.ctx, collectionName)?.$?.subscribe?.(() => {
      if (!disposed) refresh.notify(collectionName);
    }) || null)
    .filter(Boolean);
  return () => {
    disposed = true;
    refresh.dispose();
    for (const sub of subscriptions) {
      try { sub.unsubscribe?.(); } catch {}
    }
  };
}

function documentCollection(ctx, collectionName) {
  return ctx?.db?.collection?.(collectionName) || null;
}

async function refreshDocumentsFromLocal(state, changed = null, isActive = () => true) {
  if (!isActive()) return;
  const all = changed === null;
  await Promise.all([
    (all || changed.has('document_runbooks')) ? refreshRunbooks(state) : null,
    (all || ['knowledge_items', 'knowledge_runbooks', 'knowledge_tables'].some((name) => changed.has(name)))
      ? refreshKnowledge(state, changed) : null,
    (all || changed.has('documents')) ? refreshDocuments(state) : null,
  ]);
  if (!isActive()) return;
  let selectedVersionLoaded = false;
  if (all || ['documents', 'document_versions', 'document_blob_chunks'].some((name) => changed.has(name))) {
    const expectedSelectedVersionId = state.requestedVersionDocumentId === state.selectedId
      ? state.requestedVersionId
      : selectedRecord(state)?.current_version_id;
    if (state.selectedId && !ctoxDocumentsDraftProtected(state)
        && state.selectedVersion?.id !== expectedSelectedVersionId) {
      selectedVersionLoaded = Boolean(await loadSelectedVersion(state).catch(() => null));
    }
  }
  if (!isActive()) return;
  renderLeft(state);
  renderRight(state);
  renderDocumentStrip(state);
  if (selectedVersionLoaded) renderCenter(state);
}

async function refreshDocuments(state) {
  const collection = documentCollection(state.ctx, 'documents');
  const rawDocuments = collection
    ? await collection.find({ sort: [{ updated_at_ms: 'desc' }] }).exec()
    : [];
  state.documents = rawDocuments
    .map((doc) => normalizeDocumentRecord(typeof doc.toJSON === 'function' ? doc.toJSON() : doc))
    .filter(isActiveDocumentRecord);

  if (state.requestedSelectedId
      && state.documents.some((record) => record.id === state.requestedSelectedId)) {
    state.selectedId = state.requestedSelectedId;
    state.requestedSelectedId = '';
    state.selectedVersion = null;
    if (state.requestedVersionDocumentId !== state.selectedId) {
      state.requestedVersionId = '';
      state.requestedVersionDocumentId = '';
    }
    state.mailMergeNavigation = null;
  }

  if (state.selectedId && !state.documents.some((record) => record.id === state.selectedId)) {
    state.selectedId = state.documents[0]?.id || '';
    state.selectedVersion = null;
    state.requestedVersionId = '';
    state.requestedVersionDocumentId = '';
    state.mailMergeNavigation = null;
  }
  if (!state.selectedId && state.documents[0]) state.selectedId = state.documents[0].id;
}

async function refreshRunbooks(state) {
  const collection = documentCollection(state.ctx, 'document_runbooks');
  const storedRunbooks = collection
    ? (await collection.find({ sort: [{ title: 'asc' }] }).exec()).map((doc) => doc.toJSON())
    : [];
  state.runbooks = mergeDocumentRunbooks(storedRunbooks);
}

async function refreshKnowledge(state, changed = null) {
  const read = async (name) => {
    const collection = documentCollection(state.ctx, name);
    if (!collection) return [];
    const docs = await collection.find({ sort: [{ updated_at_ms: 'desc' }] }).exec();
    return docs.map((doc) => normalizeKnowledgeRecord(typeof doc.toJSON === 'function' ? doc.toJSON() : doc));
  };
  const collections = [
    ['knowledge_items', 'knowledgeItems'],
    ['knowledge_runbooks', 'knowledgeRunbooks'],
    ['knowledge_tables', 'knowledgeTables'],
  ].filter(([name]) => changed === null || changed.has(name));
  const results = await Promise.all(collections.map(([name]) => (
    name === 'knowledge_tables' ? read(name).then(mergeKnowledgeTableReferences) : read(name)
  )));
  collections.forEach(([, field], index) => {
    state[field] = results[index];
  });
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

function nextBlankDocumentTitle(state) {
  const base = state.t('untitledDocument', 'Neues Dokument');
  const titles = new Set(state.documents.map((record) => String(record.title || '').trim().toLocaleLowerCase()));
  if (!titles.has(base.toLocaleLowerCase())) return base;
  let suffix = 2;
  while (titles.has(`${base} ${suffix}`.toLocaleLowerCase())) suffix += 1;
  return `${base} ${suffix}`;
}

async function createBlankWordDocument(state) {
  if (state.creatingBlankDocument) return null;
  state.creatingBlankDocument = true;
  try {
    const title = nextBlankDocumentTitle(state);
    const formatModule = await ensureDocumentFormatModule(state);
    if (typeof formatModule.createBlankDocx !== 'function') {
      throw new Error(state.t('blankDocumentUnavailable', 'Die Word-Dokumentvorlage ist nicht verfügbar.'));
    }
    const bytes = await formatModule.createBlankDocx();
    const file = new File([bytes], ensureExtension(slugFilename(title), '.docx'), { type: DOCX_MIME });
    const record = await importDocumentFile(state, file, {
      sourceKind: 'created_blank',
      status: 'Draft',
      title,
    });
    state.ctx.notifications?.show?.({ type: 'success', message: state.t('blankDocumentCreated', 'Leeres Word-Dokument erstellt.') });
    return record;
  } catch (error) {
    console.error('[documents] blank Word document creation failed', error);
    state.ctx.notifications?.show?.({ type: 'error', message: `${state.t('documentCreateFailed', 'Dokument konnte nicht erstellt werden:')} ${error?.message || error}` });
    return null;
  } finally {
    state.creatingBlankDocument = false;
  }
}

async function importDocumentFile(state, file, workflow = {}) {
  requireDocumentPersistence(state.ctx);
  if (!isSupportedDocumentFile(file)) {
    renderError(state, 'Nur .docx, .md, .markdown oder .txt Dateien werden akzeptiert.');
    return null;
  }
  const isMarkdown = isMarkdownFile(file) || isTextFile(file);
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
  const sourceKind = String(workflow.sourceKind || '').trim()
    || (isTextFile(file) ? 'imported_text' : isMarkdown ? 'imported_markdown' : 'imported_docx');
  const status = String(workflow.status || '').trim() || 'Imported';
  const title = sanitizeDocumentTitle(workflow.title || titleFromFilename(file.name));

  await saveBlobChunks(state.ctx, {
    blobId,
    documentId,
    versionId,
    mimeType,
    bytes,
  });

  await documentCollection(state.ctx, 'document_versions').insert({
    id: versionId,
    document_id: documentId,
    version: 1,
    source_kind: sourceKind,
    blob_id: blobId,
    model_json: parsed.document,
    diagnostics: parsed.diagnostics,
    created_at_ms: now,
    updated_at_ms: now,
  });

  await documentCollection(state.ctx, 'documents').insert({
    id: documentId,
    title,
    filename: file.name,
    description: '',
    mime_type: mimeType,
    status,
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
    title,
    filename: file.name,
    description: '',
    mime_type: mimeType,
    status,
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
  if (!record?.id) {
    state.selectedVersion = null;
    return null;
  }
  const handle = state.editorHandle;
  const activity = handle?.activity;
  const previousVersion = state.selectedVersion;
  const canApply = () => state.selectedId === record.id
    && state.editorHandle === handle && handle?.activity === activity
    && state.selectedVersion === previousVersion && !ctoxDocumentsDraftProtected(state);
  if (!canApply()) return null;
  const replication = await startDocumentVersionReplication(state.ctx);
  const requestedVersionId = state.requestedVersionId && state.requestedVersionDocumentId === record.id
    ? state.requestedVersionId
    : '';
  const targetVersionId = requestedVersionId || record.current_version_id;
  const readLocalVersion = async (timeoutMs = 4500) => {
    // A missing named head/history entry is a synchronization gap, not
    // permission to open another version or move the authoritative head.
    if (targetVersionId) {
      return await withDocumentVersionTimeout(
        documentCollection(state.ctx, 'document_versions').findOne(targetVersionId).exec(),
        timeoutMs,
        `Version ${targetVersionId} konnte nicht geladen werden.`,
      );
    }
    const fallback = await withDocumentVersionTimeout(
      documentCollection(state.ctx, 'document_versions').find({
        selector: { document_id: record.id },
        sort: [{ updated_at_ms: 'desc' }],
        limit: 1,
      }).exec(),
      timeoutMs,
      `Keine Versionen für ${record.id} gefunden.`,
    );
    return fallback[0] || null;
  };
  const doc = await resolveLocalFirst(
    readLocalVersion,
    () => awaitDocumentVersionReplication(replication, state.ctx),
  );
  if (!canApply()) return null;
  state.selectedVersion = doc?.toJSON() || null;
  state.dirty = false;
  await refreshMailMergeNavigation(state);
  return state.selectedVersion;
}

async function startDocumentVersionReplication(ctx) {
  if (typeof ctx?.sync?.startCollection !== 'function') return null;
  return ctx.sync.startCollection('document_versions');
}

async function awaitDocumentVersionReplication(bridge, ctx) {
  if (typeof ctx?.sync?.startCollection === 'function') {
    bridge = await withTimeout(
      ctx.sync.startCollection('document_versions', { forceDirect: true }),
      60000,
      'Dokumentversionen konnten nicht rechtzeitig synchronisiert werden.',
    ) || bridge;
  }
  if (bridge?.ready) {
    bridge = await withTimeout(
      typeof bridge.ready === 'function' ? bridge.ready() : bridge.ready,
      60000,
      'Dokumentversionen konnten nicht rechtzeitig synchronisiert werden.',
    ) || bridge;
  }
  const replication = bridge?.state || bridge;
  if (typeof replication?.waitForOpenPeerId === 'function') {
    await withTimeout(
      replication.waitForOpenPeerId(60000),
      60000,
      'Dokumentversionen konnten nicht vollständig synchronisiert werden.',
    );
  }
}

async function withDocumentVersionTimeout(promise, ms, message) {
  let timer;
  try {
    return await Promise.race([
      promise,
      new Promise((_, reject) => {
        timer = setTimeout(() => reject(Object.assign(new Error(message), {
          code: 'document_version_read_timeout',
        })), ms);
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

async function resolveLocalFirst(readLocal, awaitReplication) {
  try {
    const localValue = await readLocal(4500);
    if (localValue) return localValue;
  } catch (error) {
    if (!isTransientDocumentReadError(error)) throw error;
  }
  await awaitReplication();
  return readLocal(60000);
}

function isTransientDocumentReadError(error) {
  if (error?.code === 'document_version_read_timeout') return true;
  const message = String(error?.message || error || '').toLowerCase();
  return message.includes('webrtc replication cancelled')
    || message.includes('query_cancelled')
    || message.includes('replication-cancel')
    || message.includes('database connection is closing')
    || (message.includes('idbdatabase') && message.includes('closing'));
}

async function refreshMailMergeNavigation(state) {
  const group = selectedDocumentGroup(state);
  if (!group?.is_mail_merge) {
    state.mailMergeNavigation = null;
    return null;
  }
  let entries;
  const typedMailMergeRecord = group.records
    .filter((record) => ['mail_merge', 'series_letter'].includes(record.document_type))
    .sort((left, right) => (
      Number(right.id === state.selectedId) - Number(left.id === state.selectedId)
      || Number(right.updated_at_ms || 0) - Number(left.updated_at_ms || 0)
      || right.id.localeCompare(left.id)
    ))[0] || null;
  if (typedMailMergeRecord) {
    const collection = documentCollection(state.ctx, 'document_versions');
    const docs = collection
      ? await collection.find({ selector: { document_id: typedMailMergeRecord.id } }).exec()
      : [];
    entries = docs
      .map((doc) => typeof doc?.toJSON === 'function' ? doc.toJSON() : doc)
      .filter((version) => (
        version?.id
        && (version.source_kind === 'mail_merge_recipient'
          || plainObject(version.mail_merge_recipient))
      ))
      .sort((left, right) => (
        Number(left.mail_merge_recipient?.index ?? left.version ?? 0)
        - Number(right.mail_merge_recipient?.index ?? right.version ?? 0)
      ))
      .map((version, index) => ({
        documentId: typedMailMergeRecord.id,
        versionId: version.id,
        blobId: version.blob_id || '',
        recipientId: firstText(version.mail_merge_recipient?.id, version.provenance?.recipient_id, version.id),
        label: firstText(version.mail_merge_recipient?.label, version.provenance?.recipient_label, `Empfänger ${index + 1}`),
        index,
      }));
  } else {
    entries = uniqueMailMergeRecipientRecords(group.records, group.title).map((record, index) => ({
      documentId: record.id,
      versionId: record.current_version_id,
      recipientId: firstText(
        record.provenance?.selectionmember_id,
        record.provenance?.recipient_id,
        record.id,
      ),
      label: recipientLabelFromRecord(record, group.title),
      index,
    }));
  }
  if (!entries.length) {
    state.mailMergeNavigation = null;
    return null;
  }
  const activeIndex = Math.max(0, entries.findIndex((entry) => (
    entry.documentId === state.selectedId
    && (!state.selectedVersion?.id || entry.versionId === state.selectedVersion.id)
  )));
  state.mailMergeNavigation = {
    groupId: group.group_id,
    title: group.title,
    entries,
    activeIndex,
  };
  return state.mailMergeNavigation;
}

function renderDocumentStrip(state) {
  const host = state.ctx.host.querySelector('[data-documents-document-strip]');
  if (!host) return;
  const record = selectedRecord(state);
  const navigation = state.mailMergeNavigation;
  const active = navigation?.entries?.[navigation.activeIndex] || null;
  const hasMailMerge = Boolean(active && navigation.entries.length);
  const recipientLoading = state.mailMergeRecipientLoading;
  const isActiveRecipientLoading = Boolean(
    active
    && recipientLoading?.documentId === active.documentId
    && recipientLoading?.versionId === active.versionId,
  );
  const recipientPosition = isActiveRecipientLoading
    ? state.lang === 'en'
      ? `Loading recipient ${navigation.activeIndex + 1} of ${navigation.entries.length} …`
      : `Empfänger ${navigation.activeIndex + 1} von ${navigation.entries.length} wird geladen …`
    : `${navigation?.activeIndex + 1} ${state.t('of', 'von')} ${navigation?.entries?.length}`;
  host.innerHTML = `
    <div class="documents-strip-leading">
      <button class="ctox-pane-icon documents-library-toggle" type="button" data-documents-library-toggle aria-label="${escapeHtml(state.t('showDocuments', 'Dokumentliste öffnen'))}" title="${escapeHtml(state.t('showDocuments', 'Dokumentliste öffnen'))}" aria-expanded="${String(state.libraryOpen)}">${actionIcon(state, 'folder')}</button>
      ${hasMailMerge ? `
        <span class="documents-series-label">${actionIcon(state, 'copy')}<strong>${escapeHtml(state.t('seriesLetter', 'Serienbrief'))}</strong></span>
        <div class="documents-recipient-navigator" data-documents-recipient-navigator>
          <button class="ctox-pane-icon" type="button" data-documents-recipient-previous aria-label="${escapeHtml(state.t('previousRecipient', 'Vorheriger Empfänger'))}" title="${escapeHtml(state.t('previousRecipient', 'Vorheriger Empfänger'))}" ${navigation.activeIndex <= 0 ? 'disabled aria-disabled="true"' : ''}>${actionIcon(state, 'chevronLeft')}</button>
          <span class="documents-recipient-position" ${isActiveRecipientLoading ? 'role="status" aria-live="polite"' : ''}>${escapeHtml(recipientPosition)}</span>
          <label class="documents-recipient-search">
            <span class="sr-only">${escapeHtml(state.t('searchRecipient', 'Empfänger suchen'))}</span>
            <input type="search" data-documents-recipient-search list="documents-recipient-options" value="${escapeHtml(active.label)}" aria-label="${escapeHtml(state.t('searchRecipient', 'Empfänger suchen'))}" autocomplete="off">
            <datalist id="documents-recipient-options">${navigation.entries.map((entry) => `<option value="${escapeHtml(entry.label)}"></option>`).join('')}</datalist>
          </label>
          <button class="ctox-pane-icon" type="button" data-documents-recipient-next aria-label="${escapeHtml(state.t('nextRecipient', 'Nächster Empfänger'))}" title="${escapeHtml(state.t('nextRecipient', 'Nächster Empfänger'))}" ${navigation.activeIndex >= navigation.entries.length - 1 ? 'disabled aria-disabled="true"' : ''}>${actionIcon(state, 'chevronRight')}</button>
        </div>
      ` : `<div class="documents-current-document"><strong>${escapeHtml(record?.title || state.t('noDocumentSelected', 'Kein Dokument ausgewählt.'))}</strong>${record ? `<span>${escapeHtml(documentTypeLabel(state, record.document_type))}</span>` : ''}</div>`}
    </div>
  `;
  host.querySelector('[data-documents-library-toggle]')?.addEventListener('click', () => {
    state.libraryOpen = !state.libraryOpen;
    if (state.libraryOpen) state.actionsOpen = false;
    renderPaneVisibility(state);
    renderDocumentStrip(state);
  });
  const navigateRecipient = (requestedIndex, trigger) => {
    trigger?.setAttribute('aria-busy', 'true');
    trigger?.setAttribute('disabled', '');
    selectMailMergeRecipient(state, requestedIndex, navigation)
      .catch((error) => {
        console.error('[documents] mail merge recipient could not be opened', error);
        renderError(
          state,
          `${state.t('documentLoadFailed', 'Dokument konnte nicht geladen werden:')} ${error?.message || error}`,
        );
      })
      .finally(() => {
        if (!trigger?.isConnected) return;
        trigger.removeAttribute('aria-busy');
        trigger.removeAttribute('disabled');
      });
  };
  host.querySelector('[data-documents-recipient-previous]')?.addEventListener('click', (event) => {
    navigateRecipient(navigation.activeIndex - 1, event.currentTarget);
  });
  host.querySelector('[data-documents-recipient-next]')?.addEventListener('click', (event) => {
    navigateRecipient(navigation.activeIndex + 1, event.currentTarget);
  });
  const search = host.querySelector('[data-documents-recipient-search]');
  syncMailMergeRecipientSearch(search, active);
  const chooseSearchResult = () => {
    const index = findMailMergeRecipientIndex(navigation.entries, search?.value);
    if (index >= 0 && index !== navigation.activeIndex) {
      navigateRecipient(index, search);
    }
  };
  search?.addEventListener('change', chooseSearchResult);
  search?.addEventListener('keydown', (event) => {
    if (event.key !== 'Enter') return;
    event.preventDefault();
    chooseSearchResult();
  });
}

async function selectMailMergeRecipient(state, requestedIndex, renderedNavigation = null) {
  const selection = resolveMailMergeRecipientSelection(
    state.mailMergeNavigation,
    renderedNavigation,
    requestedIndex,
  );
  if (!selection) return;
  await switchSelectedDocument(
    state,
    selection.entry.documentId,
    { versionId: selection.entry.versionId },
  );
}

function resolveMailMergeRecipientSelection(currentNavigation, renderedNavigation, requestedIndex) {
  const navigation = renderedNavigation?.entries?.length
    ? renderedNavigation
    : currentNavigation?.entries?.length
      ? currentNavigation
      : null;
  if (!navigation) return null;
  const index = clampNumber(Number(requestedIndex) || 0, 0, navigation.entries.length - 1);
  return { navigation, index, entry: navigation.entries[index] };
}

function findMailMergeRecipientIndex(entries = [], query = '') {
  const expected = normalizeSearchText(query);
  if (!expected) return -1;
  const exact = entries.findIndex((entry) => normalizeSearchText(entry.label) === expected);
  if (exact >= 0) return exact;
  const prefix = entries.findIndex((entry) => normalizeSearchText(entry.label).startsWith(expected));
  if (prefix >= 0) return prefix;
  return entries.findIndex((entry) => normalizeSearchText(entry.label).includes(expected));
}

function syncMailMergeRecipientSearch(search, activeRecipient) {
  if (!search || !activeRecipient) return;
  const label = String(activeRecipient.label || '');
  if (search.value !== label) search.value = label;
}

function beginMailMergeRecipientLoading(state, documentId, versionId) {
  const navigation = state.mailMergeNavigation;
  const activeIndex = navigation?.entries?.findIndex((entry) => (
    entry.documentId === documentId && entry.versionId === versionId
  ));
  if (!navigation || activeIndex < 0) return false;
  state.mailMergeNavigation = { ...navigation, activeIndex };
  state.mailMergeRecipientLoading = { documentId, versionId };
  return true;
}

function clearMailMergeRecipientLoading(state, documentId, versionId) {
  const loading = state.mailMergeRecipientLoading;
  if (!loading) return;
  if (documentId && loading.documentId !== documentId) return;
  if (versionId && loading.versionId !== versionId) return;
  state.mailMergeRecipientLoading = null;
}

function documentListSignature(records = []) {
  // Selection is NOT part of the list signature — selection is an in-place
  // aria-current flip (applyDocumentListSelection). Including it would rebuild
  // the well on every click and clamp scrollTop to 0.
  return JSON.stringify(records.map((record) => ([
    record.id,
    record.title || '',
    record.filename || '',
    record.status || '',
    record.document_type || '',
    record.is_mail_merge ? 1 : 0,
    record.recipient_count || 0,
    (record.tags || []).join(','),
    (record.source_labels || []).join(','),
    documentDescription(record) || '',
  ])));
}

function applyDocumentListSelection(state) {
  const list = state.ctx?.host?.querySelector('[data-documents-list]');
  if (!list) return;
  const groups = visibleDocuments(state);
  for (const card of list.querySelectorAll('.documents-card')) {
    const mainId = card.querySelector('[data-document-id]')?.dataset?.documentId || '';
    const group = groups.find((entry) => entry.id === mainId);
    const isCurrent = Boolean(group?.record_ids?.includes(state.selectedId));
    card.setAttribute('aria-current', String(isCurrent));
  }
}

// Shell V2: the explorer head is static markup in index.html. This only fills
// it — icons, data-derived option lists, filter state and the list body. It no
// longer builds a head, so the head geometry is decidable from index.html and
// the search input keeps focus and caret across every data tick.
function renderLeft(state) {
  const explorer = state.ctx.host.querySelector('[data-documents-explorer]');
  if (!explorer) return;
  const visible = visibleDocuments(state);
  const activeFilterCount = documentFilterCount(state);

  if (explorer.dataset.documentsHeadBound !== 'true') {
    explorer.dataset.documentsHeadBound = 'true';
    // The static SVGs in index.html are the offline fallback; the shell icon
    // set (ctx.getActionIcon) wins as soon as the module is mounted.
    replaceHeadIcon(state, explorer, '[data-documents-new-markdown]', 'add');
    replaceHeadIcon(state, explorer, '[data-documents-import-open]', 'upload');
    replaceHeadIcon(state, explorer, '[data-documents-export]', 'export');
    replaceHeadIcon(state, explorer, '[data-documents-filter-toggle]', 'filter');
    bindLeftControls(state, explorer);
  }

  const exportButton = explorer.querySelector('[data-documents-export]');
  if (exportButton) {
    const canExport = canExportDocument(state);
    exportButton.disabled = !canExport;
    exportButton.setAttribute('aria-disabled', String(!canExport));
  }

  const sort = explorer.querySelector('[data-documents-sort]');
  if (sort) sort.value = state.sortBy || 'updated_desc';
  const optionLists = [
    ['[data-documents-type]', documentTypeFilterOptions],
    ['[data-documents-status]', documentStatusFilterOptions],
    ['[data-documents-app]', documentAppFilterOptions],
    ['[data-documents-source]', documentSourceFilterOptions],
    ['[data-documents-tag]', tagFilterOptions],
  ];
  for (const [selector, build] of optionLists) {
    const select = explorer.querySelector(selector);
    if (!select) continue;
    const markup = build(state);
    if (select.dataset.ctoxOptionSig === markup) continue;
    select.dataset.ctoxOptionSig = markup;
    select.innerHTML = markup;
  }

  const filterToggle = explorer.querySelector('[data-documents-filter-toggle]');
  filterToggle?.classList.toggle('is-active', activeFilterCount > 0);
  const filterCount = explorer.querySelector('[data-documents-filter-count]');
  if (filterCount) {
    filterCount.textContent = activeFilterCount ? String(activeFilterCount) : '';
    filterCount.hidden = !activeFilterCount;
  }
  const filterReset = explorer.querySelector('.documents-filter-panel [data-documents-clear-filters]');
  if (filterReset) {
    filterReset.disabled = !activeFilterCount;
    filterReset.setAttribute('aria-disabled', String(!activeFilterCount));
  }

  const activeFilters = explorer.querySelector('[data-documents-active-filters]');
  if (activeFilters) {
    const markup = renderActiveDocumentFilters(state);
    if (activeFilters.dataset.ctoxRenderSig !== markup) {
      activeFilters.dataset.ctoxRenderSig = markup;
      activeFilters.innerHTML = markup;
      activeFilters.hidden = !markup;
      activeFilters.querySelector('[data-documents-clear-filters]')
        ?.addEventListener('click', () => clearDocumentFilters(state));
    }
  }

  const search = explorer.querySelector('[data-documents-search]');
  if (search && document.activeElement !== search && search.value !== (state.searchQuery || '')) {
    search.value = state.searchQuery || '';
  }

  const list = explorer.querySelector('[data-documents-list]');
  if (list) {
    explorer.dataset.officeView = state.listView || 'list';
    populateDocumentList(state, list, visible);
    applyDocumentListSelection(state);
  }
  autoWirePaneGrammar(state.ctx.host);
  explorer.__ctoxPaneGrammar?.refreshDot();
  explorer.__ctoxPaneGrammar?.setFooter(`${visible.length} ${state.lang === 'en' ? 'files' : 'Dateien'} · ${activeFilterCount ? (state.lang === 'en' ? 'Filtered' : 'Gefiltert') : (state.lang === 'en' ? 'All' : 'Alle')}`);
  renderPaneVisibility(state);
}

// Swaps the static fallback glyph of a head button for the shell icon while
// leaving the rest of the button (e.g. the filter count badge) untouched.
function replaceHeadIcon(state, scope, selector, name) {
  const svg = scope.querySelector(`${selector} svg`);
  if (!svg) return;
  const template = document.createElement('template');
  template.innerHTML = actionIcon(state, name).trim();
  const replacement = template.content.firstElementChild;
  if (replacement) svg.replaceWith(replacement);
}

function clearDocumentFilters(state, { resetSort = false } = {}) {
  state.searchQuery = '';
  state.typeFilter = 'all';
  state.statusFilter = 'all';
  state.appFilter = 'all';
  state.sourceFilter = 'all';
  state.tagFilter = 'all';
  if (resetSort) state.sortBy = 'updated_desc';
  renderLeft(state);
}

function populateDocumentList(state, list, records = visibleDocuments(state)) {
  const signature = documentListSignature(records);
  if (list.dataset.ctoxRenderSig === signature) {
    applyDocumentListSelection(state);
    return;
  }
  preserveScrollDuring(list, () => {
    list.replaceChildren();
    for (const record of records) {
      const card = document.createElement('article');
      card.className = 'documents-card';
      card.dataset.contextModule = 'documents';
      card.dataset.contextRecordType = 'document';
      card.dataset.contextRecordId = record.id;
      card.dataset.contextLabel = record.title || record.filename || record.id;
      card.dataset.documentsColumn = 'documents';
      card.setAttribute('aria-current', String(record.record_ids.includes(state.selectedId)));
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'documents-card-main';
      button.dataset.documentId = record.id;
      button.innerHTML = `
      <strong>${escapeHtml(record.title)}</strong>
      <small>${escapeHtml(documentTypeLabel(state, record.document_type))} · ${record.is_mail_merge ? `${escapeHtml(String(record.recipient_count))} ${escapeHtml(state.t('recipients', 'Empfänger'))}` : escapeHtml(new Date(record.updated_at_ms).toLocaleDateString(state.lang === 'en' ? 'en-GB' : 'de-DE'))}</small>
    `;
      button.title = record.filename || record.title;
      button.addEventListener('click', () => {
        const documentId = record.record_ids.includes(state.selectedId) ? state.selectedId : record.id;
        switchSelectedDocument(state, documentId).catch((error) => {
          console.error('[documents] document switch failed', error);
          renderError(state, `${state.t('documentSwitchFailed', 'Dokumentwechsel fehlgeschlagen:')} ${error?.message || error}`);
        });
      });
      const manage = document.createElement('button');
      manage.type = 'button';
      manage.className = 'documents-card-manage';
      manage.dataset.documentManage = record.id;
      manage.title = `${escapeHtml(record.title)} ${escapeHtml(state.t('manageDocument', 'verwalten'))}`;
      manage.setAttribute('aria-label', `${escapeHtml(record.title)} ${escapeHtml(state.t('manageDocument', 'verwalten'))}`);
      manage.innerHTML = actionIcon(state, 'settings');
      manage.addEventListener('click', () => openManageDocumentDrawer(state, selectedRecordInGroup(state, record)));
      card.append(button, manage);
      list.append(card);
    }
    if (!records.length) {
      const empty = document.createElement('div');
      // Shell V2 §7: empty states sit on the shared .ctox-empty step and carry
      // exactly one filled primary action.
      empty.className = 'ctox-empty documents-empty';
      empty.innerHTML = state.documents.length
        ? `
        <strong>${escapeHtml(state.t('noMatches', 'Keine Treffer'))}</strong>
        <span>${escapeHtml(state.t('adjustSearchFilter', 'Suche oder Filter anpassen.'))}</span>
        <div class="documents-empty-actions">
          <button class="ctox-button is-primary" type="button" data-documents-clear-filters>${escapeHtml(state.t('clearFilters', 'Filter zurücksetzen'))}</button>
        </div>
      `
        : `
        <strong>${escapeHtml(state.t('noDocuments', 'Keine Dokumente'))}</strong>
        <span>${escapeHtml(state.t('importPrompt', 'Ein leeres Word-Dokument erstellen oder DOCX beziehungsweise Markdown importieren.'))}</span>
        <div class="documents-empty-actions">
          <button class="ctox-button is-primary" type="button" data-documents-empty-new>${actionIcon(state, 'add')} ${escapeHtml(state.t('createWordDocument', 'Leeres Word-Dokument'))}</button>
          <button class="ctox-button" type="button" data-documents-empty-import>${actionIcon(state, 'upload')} ${escapeHtml(state.t('importDocument', 'Dokument importieren'))}</button>
        </div>
      `;
      empty.querySelector('[data-documents-empty-import]')?.addEventListener('click', () => openImportDrawer(state));
      empty.querySelector('[data-documents-empty-new]')?.addEventListener('click', () => { void createBlankWordDocument(state); });
      empty.querySelector('[data-documents-clear-filters]')?.addEventListener('click', () => {
        clearDocumentFilters(state, { resetSort: true });
      });
      list.append(empty);
    }
  });
  list.dataset.ctoxRenderSig = signature;
  applyDocumentListSelection(state);
}

async function switchSelectedDocument(state, documentId, options = {}, lifecycle = {}) {
  if (!documentId) return;
  const versionId = String(options.versionId || '').trim();
  const sameDocument = documentId === state.selectedId;
  const flushDraft = lifecycle.flushActiveEditorDraft || flushActiveEditorDraft;
  const destroyEditor = lifecycle.destroyActiveEditor || destroyActiveEditor;
  const loadVersion = lifecycle.loadSelectedVersion || loadSelectedVersion;
  const renderSelection = lifecycle.renderSelection || ((currentState) => {
    // Selection flips in place on the existing list. Only the right strip /
    // pane chrome need a cheap refresh — never rebuild the scrollable left well.
    applyDocumentListSelection(currentState);
    renderRight(currentState);
    renderDocumentStrip(currentState);
    renderPaneVisibility(currentState);
  });
  const renderEditor = lifecycle.renderCenter || renderCenter;
  const renderLoadError = lifecycle.renderError || renderError;
  const replaceEditorVersion = lifecycle.replaceActiveEditorVersion || replaceActiveEditorVersion;
  const waitForEditorContent = lifecycle.waitForActiveEditorVersionContent || waitForActiveEditorVersionContent;
  if (
    sameDocument
    && state.selectedVersion
    && (!versionId || versionId === state.selectedVersion.id)
  ) {
    if (state.editorHandle) {
      state.editorHandle.focus?.();
      scheduleMailMergeBlobPrefetch(state);
    } else {
      renderEditor(state);
    }
    return;
  }
  const switchSerial = (state.switchSerial || 0) + 1;
  state.switchSerial = switchSerial;
  const previousRecord = selectedRecord(state);
  const canReplaceEditorVersion = sameDocument && canReplaceActiveEditorVersion(state);
  if (!canReplaceEditorVersion || state.dirty || state.superdocSavePromise || state.editorHandle?.saving) {
    try {
      await withTimeout(
        flushDraft(state, previousRecord, { allowFailure: false }),
        90000,
        state.t('draftSaveTimeout', 'Automatische Draft-Speicherung beim Dokumentwechsel hat zu lange gedauert.'),
      );
    } catch (error) {
      if (state.switchSerial !== switchSerial) return;
      state.ctx.notifications?.show?.({ type: 'error', message: error.message });
      return;
    }
    if (state.switchSerial !== switchSerial) return;
    if (state.dirty || state.editorHandle?.saving) {
      state.ctx.notifications?.show?.({ type: 'error', message: state.t('draftSavePending', 'Weitere Änderungen sind noch nicht gespeichert. Bitte nach dem Speichern erneut versuchen, das Dokument zu wechseln.') });
      return;
    }
  }
  if (state.switchSerial !== switchSerial) return;

  if (canReplaceEditorVersion) {
    const previousVersion = state.selectedVersion;
    const previousRequestedVersionId = state.requestedVersionId;
    const previousRequestedVersionDocumentId = state.requestedVersionDocumentId;
    const previousNavigation = state.mailMergeNavigation;
    const previousLoading = state.mailMergeRecipientLoading;
    const previousDirty = state.dirty;
    state.requestedVersionId = versionId;
    state.requestedVersionDocumentId = versionId ? documentId : '';
    if (beginMailMergeRecipientLoading(state, documentId, versionId)) {
      state.libraryOpen = false;
      renderSelection(state);
    }

    let nextVersion;
    try {
      nextVersion = await loadVersion(state);
      if (state.switchSerial !== switchSerial) return;
      if (!nextVersion) throw new Error(state.t('noSavedVersionFound', 'Keine gespeicherte Dokumentversion gefunden.'));
    } catch (error) {
      if (state.switchSerial !== switchSerial) return;
      state.selectedVersion = previousVersion;
      state.requestedVersionId = previousRequestedVersionId;
      state.requestedVersionDocumentId = previousRequestedVersionDocumentId;
      state.mailMergeNavigation = previousNavigation;
      state.mailMergeRecipientLoading = previousLoading;
      state.dirty = previousDirty;
      renderSelection(state);
      throw error;
    }

    try {
      await replaceEditorVersion(state, previousRecord, nextVersion);
      if (state.switchSerial !== switchSerial) return;
      clearMailMergeRecipientLoading(state, documentId, versionId);
      state.libraryOpen = false;
      renderSelection(state);
      scheduleMailMergeBlobPrefetch(state);
      return;
    } catch (replacementError) {
      if (state.switchSerial !== switchSerial) return;
      console.warn('[documents] recipient version did not become ready; rebuilding editor', replacementError);
      state.renderSerial = (state.renderSerial || 0) + 1;
      try {
        await destroyEditor(state);
        if (state.switchSerial !== switchSerial) return;
        state.libraryOpen = false;
        renderSelection(state);
        await renderEditor(state);
        if (state.switchSerial !== switchSerial) return;
        await waitForEditorContent(state.editorHandle, previousRecord, nextVersion);
        if (state.switchSerial !== switchSerial) return;
        clearMailMergeRecipientLoading(state, documentId, versionId);
        renderSelection(state);
        scheduleMailMergeBlobPrefetch(state);
        return;
      } catch (rebuildError) {
        if (state.switchSerial !== switchSerial) return;
        clearMailMergeRecipientLoading(state, documentId, versionId);
        renderSelection(state);
        renderLoadError(
          state,
          `${state.t('documentLoadFailed', 'Dokument konnte nicht geladen werden:')} ${rebuildError?.message || rebuildError}`,
        );
        return;
      }
    }
  }

  state.renderSerial = (state.renderSerial || 0) + 1;
  await destroyEditor(state);
  if (state.switchSerial !== switchSerial) return;
  state.selectedId = documentId;
  state.requestedVersionId = versionId;
  state.requestedVersionDocumentId = versionId ? documentId : '';
  state.selectedVersion = null;
  state.mailMergeNavigation = null;
  state.mailMergeRecipientLoading = null;
  state.libraryOpen = false;
  bindDocumentBlobByteCache(state, documentId);
  renderSelection(state);
  const host = state.ctx.host.querySelector('[data-documents-editor]');
  if (host) host.innerHTML = `<div class="ctox-empty documents-loading"><strong>${escapeHtml(state.t('loadingDocument', 'Lade Dokument'))}</strong><span>${escapeHtml(state.t('documentSwitchRunning', 'Dokumentwechsel läuft.'))}</span></div>`;
  try {
    await loadVersion(state);
  } catch (error) {
    if (state.switchSerial !== switchSerial) return;
    state.selectedVersion = null;
    renderSelection(state);
    renderLoadError(state, `${state.t('documentLoadFailed', 'Dokument konnte nicht geladen werden:')} ${error?.message || error}`);
    return;
  }
  if (state.switchSerial !== switchSerial) return;
  renderSelection(state);
  renderEditor(state);
}

function bindLeftControls(state, wrap) {
  wrap.querySelector('[data-documents-import-open]')?.addEventListener('click', () => {
    openImportDrawer(state);
  });
  wrap.querySelector('[data-documents-new-markdown]')?.addEventListener('click', () => {
    void createBlankWordDocument(state);
  });
  wrap.querySelector('[data-documents-export]')?.addEventListener('click', () => openExportDrawer(state));
  wrap.addEventListener('ctox-pane-grammar-change', ({ detail }) => {
    state.searchQuery = detail.search;
    state.sortBy = detail.filters.sort;
    for (const name of ['type', 'status', 'app', 'source', 'tag']) state[`${name}Filter`] = detail.filters[name];
    state.listView = detail.view;
    const list = wrap.querySelector('[data-documents-list]');
    if (list) list.scrollTop = 0;
    renderLeft(state);
  });
}

function openManageDocumentDrawer(state, record) {
  const body = document.createElement('div');
  body.className = 'drawer-body documents-action-drawer';
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <h2 id="documents-manage-dialog-title">${escapeHtml(state.t('manageDocumentTitle', 'Dokument verwalten'))}</h2>
        <p>${escapeHtml(record.filename)} · ${escapeHtml(record.document_type === 'markdown_document' ? 'Markdown' : 'DOCX')}</p>
      </div>
      <button class="icon-button" type="button" data-documents-drawer-close aria-label="${escapeHtml(state.t('close', 'Schließen'))}">×</button>
    </header>
    <form class="documents-drawer-form" data-documents-manage-form>
      <label>
        <span>${escapeHtml(state.t('title', 'Titel'))}</span>
        <input name="title" value="${escapeHtml(record.title)}" placeholder="${escapeHtml(state.t('title', 'Titel'))}">
      </label>
      <label>
        <span>${escapeHtml(state.t('status', 'Status'))}</span>
        <select name="status">
          ${documentStatusOptions(record.status)}
        </select>
      </label>
      <label>
        <span>${escapeHtml(state.t('description', 'Beschreibung'))}</span>
        <textarea name="description" placeholder="${escapeHtml(state.t('description', 'Beschreibung'))}">${escapeHtml(documentDescription(record))}</textarea>
      </label>
      <label>
        <span>${escapeHtml(state.t('tags', 'Tags'))}</span>
        <input name="tags" value="${escapeHtml(documentTags(record).join(', '))}" placeholder="angebot, vertrag, kunde-a">
      </label>
      <div class="documents-drawer-actions documents-drawer-actions-three">
        <button type="button" data-documents-drawer-cancel>${escapeHtml(state.t('cancel', 'Abbrechen'))}</button>
        <button class="documents-danger-button" type="button" data-documents-delete>${escapeHtml(state.t('delete', 'Dokument löschen'))}</button>
        <button type="submit">${escapeHtml(state.t('save', 'Speichern'))}</button>
      </div>
    </form>
  `;
  wireDrawerClose(state, body, { labelledBy: 'documents-manage-dialog-title' });
  body.querySelector('[data-documents-delete]')?.addEventListener('click', async () => {
    const confirmed = await showBusinessConfirm(state.t('deleteConfirmMessage', 'Dokument "{0}" löschen?', record.title), {
      title: state.t('deleteConfirmTitle', 'Dokument löschen'),
      confirmLabel: state.t('deleteLabel', 'Löschen'),
    });
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
    await flushActiveEditorDraft(state, target, { allowFailure: true }).catch((error) => {
      console.warn('[documents] continuing delete after draft save failed', error);
    });
  }

  const doc = await documentCollection(state.ctx, 'documents').findOne(documentId).exec();
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
  const doc = await documentCollection(state.ctx, 'documents').findOne(documentId).exec();
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
        <h2 id="documents-new-dialog-title">${escapeHtml(state.t('newDocumentTitle', 'Neues Dokument'))}</h2>
        <p>${escapeHtml(state.t('newDocumentDescription', 'CTOX erstellt ein Word-Dokument per Research-/Report-Runbook.'))}</p>
      </div>
      <button class="icon-button" type="button" data-documents-drawer-close aria-label="${escapeHtml(state.t('close', 'Schließen'))}">×</button>
    </header>
    <form class="documents-drawer-form" data-documents-new-form novalidate>
      <label>
        <span>${escapeHtml(state.t('title', 'Titel'))}</span>
        <input name="title" value="Research-${new Date().toISOString().slice(0, 10)}" placeholder="${escapeHtml(state.t('title', 'Titel'))}" required>
      </label>
      <label>
        <span>${escapeHtml(state.t('documentType', 'Dokumenttyp'))}</span>
        <select name="runbook">${runbookOptions(state, 'research.report.auto')}</select>
      </label>
      <label>
        <span>${escapeHtml(state.t('knowledgeBasis', 'Knowledge-Basis'))}</span>
        <select name="knowledge">${knowledgeOptions(state, 'auto')}</select>
        <small>${escapeHtml(state.t('knowledgeBasisHint', 'CTOX nutzt den Skill als Wissens-Hub und liest die verknüpften Originalquellen für Nachweise.'))}</small>
      </label>
      <label>
        <span>${escapeHtml(state.t('tags', 'Tags'))}</span>
        <input name="tags" placeholder="angebot, vertrag, kunde-a">
      </label>
      <label>
        <span>${escapeHtml(state.t('prompt', 'Prompt'))}</span>
        <textarea name="prompt" required placeholder="${escapeHtml(state.t('newDocumentPromptPlaceholder', 'Was soll CTOX recherchieren und als Word-Dokument ausarbeiten?'))}"></textarea>
      </label>
      <button class="documents-knowledge-link" type="button" data-documents-open-knowledge>${actionIcon(state, 'knowledge')} ${escapeHtml(state.t('openKnowledge', 'CTOX Knowledge öffnen'))}</button>
      <p class="documents-form-status" role="status" data-documents-form-status></p>
      <div class="documents-drawer-actions">
        <button type="button" data-documents-drawer-cancel>${escapeHtml(state.t('cancel', 'Abbrechen'))}</button>
        <button type="submit" disabled aria-disabled="true">${escapeHtml(state.t('createWordDocument', 'Word-Dokument erstellen'))}</button>
      </div>
    </form>
  `;
  wireDrawerClose(state, body, { labelledBy: 'documents-new-dialog-title' });
  body.querySelector('[data-documents-open-knowledge]')?.addEventListener('click', () => openKnowledgeRunbooks(state));
  const newForm = body.querySelector('[data-documents-new-form]');
  updateNewDocumentSubmitState(state, newForm);
  newForm?.addEventListener('input', () => updateNewDocumentSubmitState(state, newForm));
  newForm?.addEventListener('change', () => updateNewDocumentSubmitState(state, newForm));
  newForm?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const formElement = event.currentTarget;
    if (!updateNewDocumentSubmitState(state, formElement)) return;
    const submit = formElement.querySelector('button[type="submit"]');
    const form = new FormData(event.currentTarget);
    try {
      if (submit) {
        submit.disabled = true;
        submit.textContent = state.t('taskCreating', 'Task wird angelegt...');
      }
      await dispatchNewDocumentReport(state, {
        title: form.get('title')?.toString() || '',
        runbookId: form.get('runbook')?.toString() || '',
        knowledgeId: form.get('knowledge')?.toString() || 'auto',
        prompt: form.get('prompt')?.toString() || '',
        tags: form.get('tags')?.toString() || '',
      });
      state.ctx.closeDrawers();
    } catch (error) {
      renderError(state, `${state.t('taskCreationFailed', 'CTOX konnte den Dokument-Task nicht anlegen:')} ${error?.message || error}`);
      if (submit) {
        submit.disabled = false;
        submit.textContent = state.t('createWordDocument', 'Word-Dokument erstellen');
      }
    }
  });
  state.ctx.openLeftDrawer(body);
}

function openImportDrawer(state) {
  const body = document.createElement('div');
  body.className = 'drawer-body documents-action-drawer';
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <h2 id="documents-import-dialog-title">${escapeHtml(state.t('importDocumentTitle', 'Dokument importieren'))}</h2>
        <p>${escapeHtml(state.t('importDocumentDescription', 'Datei auswählen, Importmodus festlegen und optional direkt ein Runbook anwenden.'))}</p>
      </div>
      <button class="icon-button" type="button" data-documents-drawer-close aria-label="${escapeHtml(state.t('close', 'Schließen'))}">×</button>
    </header>
    <form class="documents-drawer-form" data-documents-import-form novalidate>
      <label>
        <span>${escapeHtml(state.t('file', 'Datei'))}</span>
        <input type="file" name="file" required data-documents-import-file accept=".docx,.md,.markdown,application/vnd.openxmlformats-officedocument.wordprocessingml.document,text/markdown,text/plain">
      </label>
      <label>
        <span>${escapeHtml(state.t('importMode', 'Import-Modus'))}</span>
        <select name="importMode" data-documents-import-mode>
          <option value="direct">${escapeHtml(state.t('importModeDirect', '1:1 übernehmen'))}</option>
          <option value="runbook">${escapeHtml(state.t('importModeRunbook', 'Runbook direkt anwenden'))}</option>
        </select>
      </label>
      <label>
        <span>${escapeHtml(state.t('runbook', 'Runbook'))}</span>
        <select name="runbook" data-documents-runbook-select disabled>${runbookOptions(state, defaultRunbookId(state))}</select>
      </label>
      <label>
        <span>${escapeHtml(state.t('tags', 'Tags'))}</span>
        <input name="tags" placeholder="angebot, vertrag, kunde-a">
      </label>
      <label>
        <span>${escapeHtml(state.t('prompt', 'Prompt'))}</span>
        <textarea name="prompt" data-documents-runbook-prompt disabled placeholder="${escapeHtml(state.t('runbookPromptPlaceholder', 'Optionaler Prompt für das Runbook beim Import'))}"></textarea>
      </label>
      <button class="documents-knowledge-link" type="button" data-documents-open-knowledge>${actionIcon(state, 'knowledge')} ${escapeHtml(state.t('openKnowledge', 'CTOX Knowledge öffnen'))}</button>
      <p class="documents-form-status" role="status" data-documents-form-status></p>
      <div class="documents-drawer-actions">
        <button type="button" data-documents-drawer-cancel>${escapeHtml(state.t('cancel', 'Abbrechen'))}</button>
        <button type="submit" disabled aria-disabled="true">${escapeHtml(state.t('import', 'Importieren'))}</button>
      </div>
    </form>
  `;
  wireDrawerClose(state, body, { labelledBy: 'documents-import-dialog-title' });
  body.querySelector('[data-documents-open-knowledge]')?.addEventListener('click', () => openKnowledgeRunbooks(state));
  const importForm = body.querySelector('[data-documents-import-form]');
  updateImportSubmitState(state, importForm);
  body.querySelector('[data-documents-import-mode]')?.addEventListener('change', (event) => {
    const enabled = event.currentTarget.value === 'runbook';
    body.querySelector('[data-documents-runbook-select]').disabled = !enabled;
    body.querySelector('[data-documents-runbook-prompt]').disabled = !enabled;
    updateImportSubmitState(state, importForm);
  });
  importForm?.addEventListener('change', () => updateImportSubmitState(state, importForm));
  importForm?.addEventListener('input', () => updateImportSubmitState(state, importForm));
  importForm?.addEventListener('submit', async (event) => {
    event.preventDefault();
    if (!updateImportSubmitState(state, event.currentTarget)) return;
    const form = new FormData(event.currentTarget);
    const file = form.get('file');
    if (!(file instanceof File) || !file.name) {
      renderError(state, state.t('chooseFileFirstError', 'Bitte zuerst im Import-Dialog eine DOCX- oder Markdown-Datei auswählen.'));
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
  const activeFilename = state.selectedVersion?.filename || record?.filename || '';
  const canExport = canExportDocument(state);
  const body = document.createElement('div');
  body.className = 'drawer-body documents-action-drawer';
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <h2 id="documents-export-dialog-title">${escapeHtml(state.t('exportDocumentTitle', 'Dokument exportieren'))}</h2>
        <p>${record ? escapeHtml(record.title) : escapeHtml(state.t('noDocumentSelected', 'Kein Dokument ausgewählt.'))}</p>
      </div>
      <button class="icon-button" type="button" data-documents-drawer-close aria-label="${escapeHtml(state.t('close', 'Schließen'))}">×</button>
    </header>
    <form class="documents-drawer-form" data-documents-export-form>
      <label>
        <span>${escapeHtml(state.t('format', 'Format'))}</span>
        <select name="format" ${canExport ? '' : 'disabled'}>
          <option value="native">${record?.document_type === 'markdown_document' ? 'Markdown' : 'DOCX'} ${escapeHtml(state.t('export', 'Export starten'))}</option>
        </select>
      </label>
      <label>
        <span>${escapeHtml(state.t('filename', 'Dateiname'))}</span>
        <input name="filename" value="${escapeHtml(record ? activeFilename.replace(/\.(docx|md|markdown)$/i, '') + (record.document_type === 'markdown_document' ? '-edited.md' : '-edited.docx') : '')}" ${canExport ? '' : 'disabled'}>
      </label>
      <div class="documents-drawer-actions">
        <button type="button" data-documents-drawer-cancel>${escapeHtml(state.t('cancel', 'Abbrechen'))}</button>
        <button type="submit" ${canExport ? '' : 'disabled aria-disabled="true"'}>${escapeHtml(state.t('export', 'Export starten'))}</button>
      </div>
    </form>
  `;
  wireDrawerClose(state, body, { labelledBy: 'documents-export-dialog-title' });
  body.querySelector('[data-documents-export-form]')?.addEventListener('submit', async (event) => {
    event.preventDefault();
    if (!canExportDocument(state)) return;
    await exportSelectedDocument(state, body.querySelector('[name="filename"]')?.value || '');
    state.ctx.closeDrawers();
  });
  state.ctx.openLeftDrawer(body);
}

function wireDrawerClose(state, body, options = {}) {
  body.setAttribute('role', 'dialog');
  body.setAttribute('aria-modal', 'true');
  body.tabIndex = -1;
  if (options.labelledBy) body.setAttribute('aria-labelledby', options.labelledBy);
  const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  let cleaned = false;
  const observer = new MutationObserver(() => {
    if (!body.isConnected) cleanup();
  });
  const cleanup = () => {
    if (cleaned) return;
    cleaned = true;
    observer.disconnect();
    window.removeEventListener('keydown', onKeyDown, true);
    if (previousFocus?.isConnected) previousFocus.focus({ preventScroll: true });
  };
  const close = () => {
    cleanup();
    state.ctx.closeDrawers();
  };
  function onKeyDown(event) {
    if (!body.isConnected) {
      cleanup();
      return;
    }
    if (event.key !== 'Escape') return;
    event.preventDefault();
    close();
  }
  body.querySelector('[data-documents-drawer-close]')?.addEventListener('click', close);
  body.querySelector('[data-documents-drawer-cancel]')?.addEventListener('click', close);
  observer.observe(document.body, { childList: true, subtree: true });
  window.addEventListener('keydown', onKeyDown, true);
  requestAnimationFrame(() => {
    if (!body.isConnected) return;
    focusFirstDialogControl(body);
  });
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
      <strong>${isImport ? escapeHtml(state.t('importDocumentTitle', 'Dokument importieren')) : escapeHtml(state.t('newDocumentTitle', 'Neues Dokument'))}</strong>
      <button class="ctox-pane-icon" type="button" aria-label="${escapeHtml(state.t('close', 'Schließen'))}" title="${escapeHtml(state.t('close', 'Schließen'))}" data-documents-workflow-close>${actionIcon(state, 'close')}</button>
    </div>
    ${isImport ? `
      <label class="documents-workflow-field">
        <span>${escapeHtml(state.t('file', 'Datei'))}</span>
        <input type="file" accept=".docx,.md,.markdown,application/vnd.openxmlformats-officedocument.wordprocessingml.document,text/markdown,text/plain" data-documents-workflow-file>
      </label>
      <label class="documents-workflow-field">
        <span>${escapeHtml(state.t('importMode', 'Import-Modus'))}</span>
        <select data-documents-import-mode>
          <option value="direct" ${importMode === 'direct' ? 'selected' : ''}>${escapeHtml(state.t('importModeDirect', '1:1 übernehmen'))}</option>
          <option value="runbook" ${importMode === 'runbook' ? 'selected' : ''}>${escapeHtml(state.t('importModeRunbook', 'Runbook direkt anwenden'))}</option>
        </select>
      </label>
    ` : `
      <label class="documents-workflow-field">
        <span>${escapeHtml(state.t('title', 'Titel'))}</span>
        <input type="text" value="${escapeHtml(flow.title || '')}" placeholder="${escapeHtml(state.t('title', 'Titel'))}" data-documents-new-title>
      </label>
      <label class="documents-workflow-field">
        <span>${escapeHtml(state.t('knowledgeBasis', 'Knowledge-Basis'))}</span>
        <select data-documents-workflow-knowledge>${knowledgeOptions(state, flow.knowledgeId || 'auto')}</select>
      </label>
    `}
    <label class="documents-workflow-field" data-documents-runbook-field>
      <span>${isImport ? escapeHtml(state.t('runbook', 'Runbook')) : escapeHtml(state.t('documentType', 'Dokumenttyp'))}</span>
      <select data-documents-workflow-runbook ${isImport && importMode === 'direct' ? 'disabled' : ''}>
        ${runbookOptions(state, flow.runbookId)}
      </select>
    </label>
    <label class="documents-workflow-field">
      <span>${escapeHtml(state.t('tags', 'Tags'))}</span>
      <input type="text" value="${escapeHtml(flow.tags || '')}" placeholder="angebot, vertrag, kunde-a" data-documents-workflow-tags>
    </label>
    <label class="documents-workflow-field">
      <span>${escapeHtml(state.t('prompt', 'Prompt'))}</span>
      <textarea data-documents-workflow-prompt ${isImport && importMode === 'direct' ? 'disabled' : ''} placeholder="${isImport ? escapeHtml(state.t('runbookPromptPlaceholder', 'Optionaler Prompt für das Runbook beim Import')) : escapeHtml(state.t('newDocumentPromptPlaceholder', 'Was soll CTOX recherchieren und als Word-Dokument ausarbeiten?'))}">${escapeHtml(flow.prompt || '')}</textarea>
    </label>
    <button class="documents-knowledge-link" type="button" data-documents-open-knowledge>${actionIcon(state, 'knowledge')} ${escapeHtml(state.t('openKnowledge', 'Knowledge öffnen'))}</button>
    <div class="documents-workflow-actions">
      <button type="button" data-documents-workflow-cancel>${escapeHtml(state.t('cancel', 'Abbrechen'))}</button>
      <button type="submit">${isImport ? escapeHtml(state.t('import', 'Importieren')) : escapeHtml(state.t('createWordDocument', 'Word-Dokument erstellen'))}</button>
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
    state.workflowPanel.knowledgeId = workflow.querySelector('[data-documents-workflow-knowledge]')?.value || 'auto';
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
    const knowledgeId = workflow.querySelector('[data-documents-workflow-knowledge]')?.value || 'auto';
    try {
      if (flow.mode === 'import') {
        const file = flow.file || workflow.querySelector('[data-documents-workflow-file]')?.files?.[0];
        if (!file) {
          renderError(state, state.t('chooseFileFirstWorkflowError', 'Bitte zuerst eine DOCX- oder Markdown-Datei auswählen.'));
          return;
        }
        await importDocumentFile(state, file, {
          applyRunbook: flow.importMode === 'runbook',
          runbookId,
          knowledgeId,
          prompt,
          tags,
          sourceAction: flow.importMode === 'runbook' ? 'import_with_runbook' : 'direct_import',
        });
      } else {
        await dispatchNewDocumentReport(state, {
          title: workflow.querySelector('[data-documents-new-title]')?.value || flow.title,
          runbookId,
          knowledgeId,
          prompt,
          tags,
        });
      }
      state.workflowPanel = null;
      renderLeft(state);
    } catch (error) {
      renderError(state, `${state.t('taskCreationFailed', 'CTOX konnte den Dokument-Task nicht anlegen:')} ${error?.message || error}`);
    }
  });
}

function normalizeDocumentRecord(record = {}) {
  const title = String(record.title || record.filename || record.id || '').trim();
  const filename = String(record.filename || (title ? ensureExtension(slugFilename(title), record.document_type === 'markdown_document' ? '.md' : '.docx') : '')).trim();
  const provenance = plainObject(record.provenance)
    ? record.provenance
    : plainObject(record.display_cache?.provenance)
      ? record.display_cache.provenance
      : {};
  return {
    ...record,
    id: String(record.id || '').trim(),
    title: title || stateLessTitleFallback(record),
    filename: filename || 'document.docx',
    status: String(record.status || 'Draft').trim() || 'Draft',
    document_type: record.document_type || (isMarkdownFilename(filename) ? 'markdown_document' : 'word_document'),
    current_version_id: String(record.current_version_id || ''),
    index_text: String(record.index_text || ''),
    tags: normalizeTags(record.tags || []),
    provenance,
    template_ref: plainObject(record.template_ref) ? record.template_ref : null,
    mail_merge: plainObject(record.mail_merge) ? record.mail_merge : null,
    series_letter: plainObject(record.series_letter) ? record.series_letter : null,
    updated_at_ms: Number(record.updated_at_ms || record.created_at_ms || 0),
  };
}

function normalizeKnowledgeRecord(record = {}) {
  const payload = record.payload && typeof record.payload === 'object' && !Array.isArray(record.payload)
    ? record.payload
    : {};
  return {
    ...payload,
    ...record,
    id: String(record.id || payload.id || '').trim(),
    kind: String(record.kind || payload.kind || '').trim().toLowerCase(),
    title: String(record.title || payload.title || record.id || '').trim(),
    summary: String(record.summary || record.description || payload.summary || payload.description || '').trim(),
    source_path: String(record.source_path || payload.source_path || '').trim(),
    domain: String(record.domain || record.knowledge_domain || payload.domain || payload.knowledge_domain || '').trim(),
    updated_at_ms: Number(record.updated_at_ms ?? payload.updated_at_ms ?? 0),
    payload,
  };
}

function firstArray(...values) {
  return values.find(Array.isArray) || [];
}

function tableRows(table, payload) {
  return firstArray(
    payload.rows,
    payload.records,
    payload.data,
    payload.dataframe?.rows,
    payload.dataframe?.records,
    payload.dataframe?.data,
    table.rows,
    table.records,
    table.data,
    table.dataframe?.rows,
    table.dataframe?.records,
    table.dataframe?.data,
  );
}

function nonNegativeInteger(value) {
  const number = Number(value);
  return Number.isInteger(number) && number >= 0 ? number : null;
}

function declaredTableNumber(table, payload, names) {
  for (const name of names) {
    if (table[name] !== undefined && table[name] !== null && table[name] !== '') {
      return { value: Number(table[name]), present: true };
    }
    if (payload[name] !== undefined && payload[name] !== null && payload[name] !== '') {
      return { value: Number(payload[name]), present: true };
    }
  }
  return { value: null, present: false };
}

function mergeKnowledgeTableReferences(tables = []) {
  const groups = new Map();
  for (const table of Array.isArray(tables) ? tables : []) {
    if (!table || typeof table !== 'object') continue;
    const payload = table.payload && typeof table.payload === 'object' && !Array.isArray(table.payload)
      ? table.payload
      : table;
    const logicalId = String(
      payload.logical_table_id
      || table.logical_table_id
      || payload.id
      || table.id
      || '',
    ).trim();
    if (!logicalId) continue;
    const chunkIndexValue = declaredTableNumber(table, payload, ['chunk_index', 'chunkIndex']);
    const chunkIndex = chunkIndexValue.present ? nonNegativeInteger(chunkIndexValue.value) : 0;
    const entry = {
      table,
      payload,
      id: String(table.id || payload.id || logicalId).trim(),
      rows: tableRows(table, payload),
      chunkIndex,
      chunkCount: declaredTableNumber(table, payload, ['chunk_count', 'chunkCount']),
      rowOffset: declaredTableNumber(table, payload, ['chunk_row_offset', 'chunkRowOffset', 'row_offset', 'rowOffset']),
      rowCount: declaredTableNumber(table, payload, ['chunk_row_count', 'chunkRowCount']),
      totalRows: declaredTableNumber(table, payload, ['total_rows', 'totalRows', 'row_count', 'rowCount']),
      projectedRows: declaredTableNumber(table, payload, ['projected_row_count', 'projectedRowCount']),
      rowsComplete: table.rows_complete ?? payload.rows_complete,
    };
    if (!groups.has(logicalId)) groups.set(logicalId, []);
    groups.get(logicalId).push(entry);
  }

  return [...groups.entries()].map(([logicalId, parts]) => {
    const sorted = [...parts].sort((left, right) => (
      (left.chunkIndex ?? Number.MAX_SAFE_INTEGER) - (right.chunkIndex ?? Number.MAX_SAFE_INTEGER)
      || left.id.localeCompare(right.id)
    ));
    const first = sorted[0];
    const errors = [];
    const chunkCounts = [...new Set(sorted
      .map((part) => part.chunkCount.present ? part.chunkCount.value : null)
      .filter((value) => value !== null))];
    const declaredChunkCount = chunkCounts[0] ?? Math.max(sorted.length, 1);
    if (!Number.isInteger(declaredChunkCount) || declaredChunkCount < 1) {
      errors.push('invalid_chunk_count');
    }
    if (chunkCounts.length > 1 || sorted.some((part) => (
      part.chunkCount.present && part.chunkCount.value !== declaredChunkCount
    ))) {
      errors.push('inconsistent_chunk_count');
    }

    const indexSet = new Set();
    for (const part of sorted) {
      if (part.chunkIndex === null) {
        errors.push('invalid_chunk_index');
        continue;
      }
      if (part.chunkIndex >= declaredChunkCount) errors.push('chunk_index_out_of_range');
      if (indexSet.has(part.chunkIndex)) errors.push('duplicate_chunk_index');
      indexSet.add(part.chunkIndex);
    }
    for (let index = 0; index < declaredChunkCount; index += 1) {
      if (!indexSet.has(index)) errors.push(`missing_chunk_index:${index}`);
    }

    const hasExplicitOffset = sorted.some((part) => part.rowOffset.present);
    const hasMissingOffset = sorted.some((part) => !part.rowOffset.present);
    if (hasExplicitOffset && hasMissingOffset && sorted.length > 1) errors.push('missing_chunk_offset');

    const rows = [];
    const lineage = [];
    let derivedOffset = 0;
    let offsetBase = null;
    for (const part of sorted) {
      const rowCount = part.rows.length;
      const declaredRowCount = part.rowCount.present ? part.rowCount.value : null;
      if (declaredRowCount !== null && (!Number.isInteger(declaredRowCount) || declaredRowCount !== rowCount)) {
        errors.push('inconsistent_chunk_row_count');
      }
      const offset = part.rowOffset.present ? nonNegativeInteger(part.rowOffset.value) : derivedOffset;
      if (offset === null) {
        errors.push('invalid_chunk_offset');
      } else {
        if (offsetBase === null) offsetBase = offset;
        if (offset !== derivedOffset && (!hasExplicitOffset || part.rowOffset.present)) {
          errors.push('inconsistent_chunk_offset');
        }
      }
      rows.push(...part.rows);
      lineage.push({
        id: part.id,
        logical_table_id: logicalId,
        chunk_index: part.chunkIndex,
        chunk_count: declaredChunkCount,
        row_offset: offset,
        chunk_row_offset: offset,
        row_count: rowCount,
        chunk_row_count: rowCount,
        total_rows: part.totalRows.value,
        projected_row_count: part.projectedRows.value,
        rows_complete: part.rowsComplete !== false,
      });
      derivedOffset += rowCount;
      if (part.rowsComplete === false || part.rowsComplete === 'false') errors.push('source_rows_incomplete');
    }
    if (offsetBase !== null && offsetBase !== 0) errors.push('non_zero_initial_chunk_offset');

    const totalRows = consistentDeclaredValue(sorted, 'totalRows', errors, 'inconsistent_total_rows');
    const projectedRows = consistentDeclaredValue(sorted, 'projectedRows', errors, 'inconsistent_projected_row_count');
    if (projectedRows !== null && projectedRows !== rows.length) errors.push('projected_row_count_mismatch');
    else if (projectedRows === null && totalRows !== null && totalRows !== rows.length) errors.push('total_rows_mismatch');
    if (totalRows !== null && projectedRows !== null && totalRows !== projectedRows) {
      errors.push('total_rows_mismatch');
    }

    const uniqueErrors = [...new Set(errors)];
    const complete = uniqueErrors.length === 0;
    const chunkIds = lineage.map((entry) => entry.id);
    const mergedPayload = {
      ...first.payload,
      id: logicalId,
      logical_table_id: logicalId,
      chunk_index: 0,
      chunk_count: declaredChunkCount,
      chunk_row_offset: 0,
      chunk_row_count: rows.length,
      row_count: totalRows ?? first.payload.row_count ?? rows.length,
      projected_row_count: projectedRows ?? rows.length,
      rows_complete: complete,
      chunk_status: complete ? 'complete' : 'incomplete',
      chunk_validation_errors: uniqueErrors,
      chunk_ids: chunkIds,
      chunk_lineage: lineage,
      lineage,
      rows,
    };
    return normalizeKnowledgeRecord({
      ...first.table,
      ...mergedPayload,
      id: logicalId,
      logical_table_id: logicalId,
      chunk_status: complete ? 'complete' : 'incomplete',
      chunk_validation_errors: uniqueErrors,
      chunk_ids: chunkIds,
      chunk_lineage: lineage,
      lineage,
      payload: mergedPayload,
    });
  });
}

function consistentDeclaredValue(parts, field, errors, errorCode) {
  const values = [...new Set(parts
    .map((part) => part[field].present ? part[field].value : null)
    .filter((value) => value !== null))];
  if (values.length > 1) errors.push(errorCode);
  return values[0] ?? null;
}

function knowledgeCandidates(state) {
  const active = (state.knowledgeItems || []).filter((item) => item.id && item.is_deleted !== true && item._deleted !== true);
  const preferred = active.filter((item) => ['skillbook', 'skill'].includes(item.kind));
  const fallback = preferred.length ? preferred : active.filter((item) => item.kind !== 'dataframe');
  const tables = (state.knowledgeTables || [])
    .filter((table) => table.id && table.is_deleted !== true && table._deleted !== true)
    .map((table) => ({
      ...table,
      kind: 'dataframe',
      selection_type: 'table',
      is_procedural_skill: false,
    }));
  const ids = new Set(fallback.map((item) => item.id));
  return [...fallback, ...tables.filter((table) => !ids.has(table.id))];
}

function knowledgeSearchTokens(value) {
  return new Set(String(value || '')
    .toLowerCase()
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .split(/[^a-z0-9]+/)
    .filter((token) => token.length > 2));
}

function scoreKnowledgeCandidate(candidate, query) {
  const queryTokens = knowledgeSearchTokens(query);
  if (!queryTokens.size) return 0;
  const candidateTokens = knowledgeSearchTokens(`${candidate.title} ${candidate.summary} ${candidate.domain} ${candidate.source_path}`);
  let score = 0;
  for (const token of queryTokens) if (candidateTokens.has(token)) score += 1;
  if (candidate.kind === 'skillbook') score += 0.25;
  return score;
}

function resolveKnowledgeContext(state, requestedId = '', query = '') {
  const candidates = knowledgeCandidates(state);
  const manual = requestedId && requestedId !== 'auto'
    ? candidates.find((candidate) => candidate.id === requestedId)
    : null;
  const selected = manual || [...candidates]
    .sort((left, right) => scoreKnowledgeCandidate(right, query) - scoreKnowledgeCandidate(left, query)
      || right.updated_at_ms - left.updated_at_ms)[0] || null;
  if (!selected) return null;
  const domain = selected.domain;
  const relatedTables = (state.knowledgeTables || []).filter((table) => (
    table.id === selected.id
    || (domain && [table.domain, table.payload?.domain, table.knowledge_domain, table.payload?.knowledge_domain].includes(domain))
  ));
  const linkedRunbookIds = new Set([
    ...(Array.isArray(selected.linked_runbook_ids) ? selected.linked_runbook_ids : []),
    ...(Array.isArray(selected.payload?.linked_runbook_ids) ? selected.payload.linked_runbook_ids : []),
  ].map(String));
  const relatedRunbooks = state.knowledgeRunbooks.filter((runbook) => (
    linkedRunbookIds.has(runbook.id)
    || (domain && [runbook.domain, runbook.payload?.domain, runbook.knowledge_domain, runbook.payload?.knowledge_domain].includes(domain))
  ));
  const sourceReferences = [selected.source_references, selected.payload?.source_references, selected.sources, selected.payload?.sources]
    .find(Array.isArray) || [];
  return {
    selection_mode: manual ? 'manual' : 'auto',
    id: selected.id,
    kind: selected.kind || 'skill',
    selection_type: selected.kind === 'dataframe' ? 'table' : 'skill',
    is_procedural_skill: ['skill', 'skillbook'].includes(selected.kind),
    title: selected.title,
    summary: selected.summary,
    domain,
    source_path: selected.source_path,
    updated_at_ms: selected.updated_at_ms,
    linked_runbook_ids: relatedRunbooks.map((runbook) => runbook.id),
    table_ids: relatedTables.map((table) => table.id),
    table_lineage: relatedTables.map((table) => ({
      id: table.id,
      chunk_status: table.chunk_status || table.payload?.chunk_status || 'complete',
      rows_complete: table.rows_complete ?? table.payload?.rows_complete ?? true,
      chunk_ids: table.chunk_ids || table.payload?.chunk_ids || [table.id],
      chunk_lineage: table.chunk_lineage || table.payload?.chunk_lineage || table.lineage || table.payload?.lineage || [],
      projected_row_count: table.projected_row_count ?? table.payload?.projected_row_count ?? null,
    })),
    source_references: sourceReferences.slice(0, 200),
  };
}

function knowledgeContextInstruction(knowledgeContext) {
  if (!knowledgeContext) {
    return 'Waehle automatisch den fachlich passendsten aktuellen Skill oder Skillbook aus CTOX Knowledge.';
  }
  if (knowledgeContext.is_procedural_skill) {
    return `Verwende als Knowledge-Hub ${knowledgeContext.kind} "${knowledgeContext.title}" (ID ${knowledgeContext.id}, Domain ${knowledgeContext.domain || 'ohne Domain'}). Lies den aktuellen Skill-Inhalt und seine verknuepften Ressourcen/Tabellen.`;
  }
  return `Verwende die ausgewählte Knowledge-Tabelle "${knowledgeContext.title}" (ID ${knowledgeContext.id}, Domain ${knowledgeContext.domain || 'ohne Domain'}) als Datenbasis. Diese Auswahl ist eine Tabelle und kein prozeduraler Skill; beachte ihren Vollständigkeitsstatus und ihre Tabellen-Lineage.`;
}

function knowledgeOptions(state, selectedId = 'auto') {
  const candidates = knowledgeCandidates(state);
  const automatic = `<option value="auto" ${!selectedId || selectedId === 'auto' ? 'selected' : ''}>${escapeHtml(state.t('knowledgeAuto', 'Automatisch passend wählen'))}</option>`;
  return automatic + candidates.map((item) => (
    `<option value="${escapeHtml(item.id)}" ${item.id === selectedId ? 'selected' : ''}>${item.selection_type === 'table' ? `${escapeHtml(state.t('knowledgeTable', 'Tabelle'))}: ` : ''}${escapeHtml(item.title)}${item.domain ? ` · ${escapeHtml(item.domain)}` : ''}</option>`
  )).join('');
}

function documentKnowledgeLink(record) {
  return (Array.isArray(record?.linked_records) ? record.linked_records : [])
    .find((link) => link?.type === 'knowledge' || link?.collection === 'knowledge_items') || null;
}

function isDocumentKnowledgeStale(state, record) {
  const link = documentKnowledgeLink(record);
  if (!link?.id) return false;
  const current = state.knowledgeItems.find((item) => item.id === link.id);
  return Boolean(current && Number(current.updated_at_ms || 0) > Number(link.updated_at_ms || 0));
}

function stateLessTitleFallback(record = {}) {
  return String(record.id || '').trim() || 'Neues Dokument';
}

function isActiveDocumentRecord(record = {}) {
  return Boolean(record.id) && record.is_deleted !== true;
}

function canExportDocument(state) {
  const record = selectedRecord(state);
  return Boolean(record?.id && (state.selectedVersion?.id || record.current_version_id));
}

function readNewDocumentInput(form) {
  const formData = new FormData(form);
  return {
    title: formData.get('title')?.toString() || '',
    runbookId: formData.get('runbook')?.toString() || '',
    knowledgeId: formData.get('knowledge')?.toString() || 'auto',
    prompt: formData.get('prompt')?.toString() || '',
    tags: formData.get('tags')?.toString() || '',
  };
}

function validateNewDocumentInput(input = {}) {
  const title = String(input.title || '').trim();
  const prompt = String(input.prompt || '').trim();
  const runbookId = String(input.runbookId || '').trim();
  if (!title) return { valid: false, key: 'validationTitleRequired', message: 'Titel fehlt.' };
  if (!runbookId) return { valid: false, key: 'validationRunbookRequired', message: 'Runbook fehlt.' };
  if (!prompt) return { valid: false, key: 'validationPromptRequired', message: 'Prompt erforderlich.' };
  return { valid: true, message: '' };
}

function updateNewDocumentSubmitState(state, form) {
  if (!form) return false;
  const validation = validateNewDocumentInput(readNewDocumentInput(form));
  const message = validation.valid ? '' : state.t(validation.key, validation.message);
  setFormValidationState(form, validation.valid, message);
  return validation.valid;
}

function readImportInput(form) {
  const formData = new FormData(form);
  const file = formData.get('file');
  return {
    file,
    importMode: formData.get('importMode')?.toString() || 'direct',
    runbookId: formData.get('runbook')?.toString() || '',
  };
}

function validateImportInput(input = {}) {
  const file = input.file;
  if (!(file instanceof File) || !file.name) {
    return { valid: false, key: 'validationFileRequired', message: 'Datei erforderlich.' };
  }
  if (!isSupportedDocumentFile(file)) {
    return { valid: false, key: 'validationUnsupportedFile', message: 'Nur DOCX, Markdown oder Text.' };
  }
  if (input.importMode === 'runbook' && !String(input.runbookId || '').trim()) {
    return { valid: false, key: 'validationRunbookRequired', message: 'Runbook fehlt.' };
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
  const status = form.querySelector('[data-documents-form-status]');
  if (submit) {
    submit.disabled = !isValid;
    submit.setAttribute('aria-disabled', String(!isValid));
  }
  if (status) {
    status.textContent = isValid ? '' : message;
    status.hidden = isValid || !message;
  }
}

function focusFirstDialogControl(body) {
  const target = body.querySelector('[autofocus], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), button:not([disabled])');
  if (target instanceof HTMLElement) {
    target.focus({ preventScroll: true });
    return;
  }
  body.focus({ preventScroll: true });
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
    `<option value="all" ${state.tagFilter === 'all' ? 'selected' : ''}>${escapeHtml(state.t('allTags', 'Alle Tags'))}</option>`,
    `<option value="untagged" ${state.tagFilter === 'untagged' ? 'selected' : ''}>${escapeHtml(state.t('untagged', 'Ohne Tags'))}</option>`,
    ...tags.map((tag) => `<option value="${escapeHtml(tag)}" ${state.tagFilter === tag ? 'selected' : ''}>${escapeHtml(tag)}</option>`),
  ].join('');
}

function documentTypeFilterOptions(state) {
  const types = [...new Set(groupDocumentRecords(state.documents).map((record) => record.document_type))]
    .filter(Boolean)
    .sort((left, right) => documentTypeLabel(state, left).localeCompare(documentTypeLabel(state, right), state.lang));
  return [
    `<option value="all" ${state.typeFilter === 'all' ? 'selected' : ''}>${escapeHtml(state.t('allDocumentTypes', 'Alle Typen'))}</option>`,
    ...types.map((type) => `<option value="${escapeHtml(type)}" ${state.typeFilter === type ? 'selected' : ''}>${escapeHtml(documentTypeLabel(state, type))}</option>`),
  ].join('');
}

function documentStatusFilterOptions(state) {
  const statuses = [...new Set(state.documents.map((record) => record.status).filter(Boolean))]
    .sort((left, right) => left.localeCompare(right, state.lang));
  return [
    `<option value="all" ${state.statusFilter === 'all' ? 'selected' : ''}>${escapeHtml(state.t('allStatuses', 'Alle Status'))}</option>`,
    ...statuses.map((status) => `<option value="${escapeHtml(status)}" ${state.statusFilter === status ? 'selected' : ''}>${escapeHtml(status)}</option>`),
  ].join('');
}

function documentSourceFilterOptions(state) {
  const identities = uniqueBy(state.documents.map(documentSourceIdentity), ({ source_key: key }) => key)
    .sort((left, right) => left.label.localeCompare(right.label, state.lang));
  return [
    `<option value="all" ${state.sourceFilter === 'all' ? 'selected' : ''}>${escapeHtml(state.t('allSources', 'Alle Quellen'))}</option>`,
    ...identities.map(({ source_key: key, source_label: label }) => `<option value="${escapeHtml(key)}" ${state.sourceFilter === key ? 'selected' : ''}>${escapeHtml(label)}</option>`),
  ].join('');
}

function documentAppFilterOptions(state) {
  const identities = uniqueBy(state.documents.map(documentSourceIdentity), ({ app_key: key }) => key)
    .sort((left, right) => left.app_label.localeCompare(right.app_label, state.lang));
  return [
    `<option value="all" ${state.appFilter === 'all' ? 'selected' : ''}>${escapeHtml(state.t('allApps', 'Alle Ersteller-Apps'))}</option>`,
    ...identities.map(({ app_key: key, app_label: label }) => `<option value="${escapeHtml(key)}" ${state.appFilter === key ? 'selected' : ''}>${escapeHtml(label)}</option>`),
  ].join('');
}

function documentTypeLabel(state, type) {
  const labels = {
    mail_merge: state.t('seriesLetter', 'Serienbrief'),
    series_letter: state.t('seriesLetter', 'Serienbrief'),
    word_document: 'Word',
    markdown_document: 'Markdown',
  };
  return labels[type] || humanizeIdentifier(type || state.t('unknownDocumentType', 'Dokument'));
}

function documentFilterCount(state) {
  return ['typeFilter', 'statusFilter', 'appFilter', 'sourceFilter', 'tagFilter']
    .filter((key) => state[key] && state[key] !== 'all').length;
}

function renderActiveDocumentFilters(state) {
  const labels = [];
  if (state.typeFilter !== 'all') labels.push(documentTypeLabel(state, state.typeFilter));
  if (state.statusFilter !== 'all') labels.push(state.statusFilter);
  if (state.appFilter !== 'all') {
    const app = state.documents.map(documentSourceIdentity)
      .find(({ app_key: key }) => key === state.appFilter);
    if (app) labels.push(app.app_label);
  }
  if (state.sourceFilter !== 'all') {
    const source = state.documents.map(documentSourceIdentity)
      .find(({ source_key: key }) => key === state.sourceFilter);
    if (source) labels.push(source.source_label);
  }
  if (state.tagFilter !== 'all') labels.push(state.tagFilter === 'untagged' ? state.t('untagged', 'Ohne Tags') : state.tagFilter);
  if (!labels.length) return '';
  // Inner markup only — the container is static in index.html so it stays part
  // of the pane head instead of being rebuilt with it.
  return `${labels.map((label) => `<span>${escapeHtml(label)}</span>`).join('')}
    <button type="button" data-documents-clear-filters aria-label="${escapeHtml(state.t('clearFilters', 'Filter zurücksetzen'))}" title="${escapeHtml(state.t('clearFilters', 'Filter zurücksetzen'))}">${actionIcon(state, 'close')}</button>`;
}

function normalizeDocumentStatus(value, fallback = 'Draft') {
  const allowed = new Set(['Imported', 'Created', 'CreatedWithWarnings', 'Draft', 'Review', 'Final']);
  const status = String(value || '').trim();
  return allowed.has(status) ? status : allowed.has(fallback) ? fallback : 'Draft';
}

function documentStatusOptions(selectedStatus) {
  return ['Imported', 'Created', 'CreatedWithWarnings', 'Draft', 'Review', 'Final']
    .map((status) => `<option value="${status}" ${selectedStatus === status ? 'selected' : ''}>${status}</option>`)
    .join('');
}

function visibleDocuments(state) {
  const query = normalizeSearchText(state.searchQuery);
  const status = state.statusFilter || 'all';
  const type = state.typeFilter || 'all';
  const app = state.appFilter || 'all';
  const source = state.sourceFilter || 'all';
  const tag = state.tagFilter || 'all';
  return groupDocumentRecords(state.documents)
    .filter((entry) => {
      if (status !== 'all' && !entry.statuses.includes(status)) return false;
      if (type !== 'all' && entry.document_type !== type) return false;
      if (app !== 'all' && !entry.app_keys.includes(app)) return false;
      if (source !== 'all' && !entry.source_keys.includes(source)) return false;
      if (tag === 'untagged' && entry.tags.length) return false;
      if (tag !== 'all' && tag !== 'untagged' && !entry.tags.includes(tag)) return false;
      if (!query) return true;
      return normalizeSearchText(entry.search_text).includes(query);
    })
    .sort((left, right) => {
      if (state.sortBy === 'updated_asc') return left.updated_at_ms - right.updated_at_ms;
      if (state.sortBy === 'title_asc') return left.title.localeCompare(right.title, 'de');
      if (state.sortBy === 'creator_app') {
        return (left.source_labels[0] || '').localeCompare(right.source_labels[0] || '', 'de')
          || left.title.localeCompare(right.title, 'de');
      }
      if (state.sortBy === 'status') {
        return left.status.localeCompare(right.status, 'de') || left.title.localeCompare(right.title, 'de');
      }
      return right.updated_at_ms - left.updated_at_ms;
    });
}

function groupDocumentRecords(records = []) {
  const groups = new Map();
  for (const record of records.filter(isActiveDocumentRecord)) {
    const bundle = mailMergeBundleDescriptor(record);
    const key = bundle?.key || `document:${record.id}`;
    if (!groups.has(key)) groups.set(key, { key, bundle, records: [] });
    groups.get(key).records.push(record);
  }
  return [...groups.values()].map(finalizeDocumentGroup);
}

function finalizeDocumentGroup(group) {
  const records = [...group.records].sort((left, right) => (
    recipientLabelFromRecord(left).localeCompare(recipientLabelFromRecord(right), 'de')
    || left.id.localeCompare(right.id)
  ));
  const primary = [...records].sort((left, right) => (
    Number(right.updated_at_ms || 0) - Number(left.updated_at_ms || 0)
    || right.id.localeCompare(left.id)
  ))[0];
  const isMailMerge = Boolean(group.bundle);
  const statuses = [...new Set(records.map((record) => record.status).filter(Boolean))];
  const tags = [...new Set(records.flatMap(documentTags))];
  const sourceIdentities = uniqueBy(
    records.map(documentSourceIdentity),
    (identity) => identity.key,
  );
  const title = isMailMerge ? mailMergeGroupTitle(records, group.bundle) : primary.title;
  const recipientCount = isMailMerge
    ? Math.max(
      uniqueMailMergeRecipientRecords(records).length,
      ...records.map((record) => Number(
        record.mail_merge?.recipient_count || record.series_letter?.recipient_count || 0,
      )),
    )
    : 0;
  const searchValues = records.flatMap((record) => [
    record.title,
    record.filename,
    record.index_text,
    documentDescription(record),
    record.status,
    record.document_type,
    recipientLabelFromRecord(record),
    ...documentTags(record),
    ...Object.values(record.provenance || {}),
  ]);
  return {
    ...primary,
    group_id: group.key,
    bundle_key: group.bundle?.key || '',
    is_mail_merge: isMailMerge,
    records,
    record_ids: records.map((record) => record.id),
    title,
    filename: isMailMerge ? `${title}.docx` : primary.filename,
    document_type: isMailMerge ? 'mail_merge' : primary.document_type,
    status: statuses.length === 1 ? statuses[0] : statuses.join(', '),
    statuses,
    tags,
    app_keys: [...new Set(sourceIdentities.map(({ app_key: key }) => key))],
    source_keys: [...new Set(sourceIdentities.map(({ source_key: key }) => key))],
    source_labels: sourceIdentities.map(({ label }) => label),
    recipient_count: recipientCount,
    updated_at_ms: Math.max(...records.map((record) => Number(record.updated_at_ms || 0)), 0),
    search_text: searchValues.filter((value) => value != null).join(' '),
  };
}

function mailMergeBundleDescriptor(record = {}) {
  const mailMerge = plainObject(record.mail_merge) ? record.mail_merge : null;
  const seriesLetter = plainObject(record.series_letter) ? record.series_letter : null;
  const explicit = mailMerge || seriesLetter;
  const provenance = plainObject(record.provenance) ? record.provenance : {};
  const source = String(provenance.source || '').trim().toLowerCase();
  const explicitBundleId = firstText(
    explicit?.bundle_id,
    explicit?.id,
    provenance.bundle_id,
    provenance.mail_merge_id,
    provenance.series_letter_id,
  );
  const exportRunId = firstText(provenance.export_run_id, provenance.exportRunId);
  const appId = firstText(provenance.app_id, provenance.appId);
  const campaignId = firstText(
    provenance.selection_id,
    provenance.campaign_id,
    provenance.campaignId,
  );
  const templateId = firstText(
    record.template_ref?.template_id,
    record.template_ref?.id,
    provenance.template_id,
  );
  const templateVersion = firstText(record.template_ref?.version, provenance.template_version, '1');
  const stableCampaignBundle = MAIL_MERGE_SOURCE_NAMES.has(source) && campaignId && templateId
    ? `${appId || 'unknown'}:${source}:${campaignId}:${templateId}:${templateVersion}`
    : '';
  const isTypedBundle = record.document_type === 'mail_merge'
    || record.document_type === 'series_letter'
    || Boolean(explicit);
  if (isTypedBundle) {
    const bundleId = firstText(
      explicitBundleId,
      stableCampaignBundle,
      exportRunId,
      record.idempotency_key,
      record.id,
    );
    return bundleId ? {
      key: `mail_merge:${bundleId}`,
      kind: record.document_type === 'series_letter' ? 'series_letter' : 'mail_merge',
      id: bundleId,
      explicit: true,
    } : null;
  }
  if (!MAIL_MERGE_SOURCE_NAMES.has(source)) return null;
  if (explicitBundleId) {
    return {
      key: `mail_merge:${explicitBundleId}`,
      kind: 'mail_merge',
      id: explicitBundleId,
      explicit: false,
    };
  }
  if (!campaignId || !templateId) return null;
  return {
    key: `mail_merge:${stableCampaignBundle}`,
    kind: 'mail_merge',
    id: `${campaignId}:${templateId}:${templateVersion}`,
    explicit: false,
  };
}

function mailMergeGroupTitle(records, bundle) {
  const configured = records.map((record) => firstText(
    record.mail_merge?.title,
    record.series_letter?.title,
    record.provenance?.bundle_title,
    record.provenance?.campaign_name,
    record.display_cache?.mail_merge_title,
  )).find(Boolean);
  if (configured) return configured;
  if (bundle?.explicit && records.length === 1) return records[0].title || 'Serienbrief';
  const common = longestCommonPrefix(records.map((record) => record.title || record.filename))
    .replace(/[\s\-–—:|]+$/g, '')
    .trim();
  return common.length >= 3 ? common : records[0]?.title || 'Serienbrief';
}

function longestCommonPrefix(values = []) {
  const texts = values.map((value) => String(value || '')).filter(Boolean);
  if (!texts.length) return '';
  let prefix = texts[0];
  for (const text of texts.slice(1)) {
    while (prefix && !text.toLocaleLowerCase('de').startsWith(prefix.toLocaleLowerCase('de'))) {
      prefix = prefix.slice(0, -1);
    }
    if (!prefix) break;
  }
  return prefix;
}

function recipientLabelFromRecord(record, groupTitle = '') {
  const configured = firstText(
    record.mail_merge_recipient?.label,
    record.provenance?.recipient_label,
    record.provenance?.recipient_name,
    record.display_cache?.recipient_label,
  );
  if (configured) return configured;
  let title = String(record.title || record.filename || '').replace(/\.docx$/i, '').trim();
  if (groupTitle && title.toLocaleLowerCase('de').startsWith(groupTitle.toLocaleLowerCase('de'))) {
    title = title.slice(groupTitle.length).replace(/^[\s\-–—:|]+/, '');
  }
  const parts = title.split(/\s+-\s+/).filter(Boolean);
  if (groupTitle && parts.length > 1) parts.pop();
  return parts.join(' - ') || title || record.id;
}

function mailMergeRecipientKey(record, groupTitle = '') {
  return firstText(
    record.mail_merge_recipient?.id,
    record.provenance?.selectionmember_id,
    record.provenance?.recipient_id,
    record.provenance?.person_id,
    normalizeSearchText(recipientLabelFromRecord(record, groupTitle)),
    record.id,
  );
}

function uniqueMailMergeRecipientRecords(records = [], groupTitle = '') {
  const byRecipient = new Map();
  for (const record of records) {
    const key = mailMergeRecipientKey(record, groupTitle);
    const current = byRecipient.get(key);
    if (!current || Number(record.updated_at_ms || 0) >= Number(current.updated_at_ms || 0)) {
      byRecipient.set(key, record);
    }
  }
  return [...byRecipient.values()].sort((left, right) => (
    recipientLabelFromRecord(left, groupTitle).localeCompare(recipientLabelFromRecord(right, groupTitle), 'de')
    || left.id.localeCompare(right.id)
  ));
}

function documentSourceIdentity(record = {}) {
  const provenance = plainObject(record.provenance) ? record.provenance : {};
  const appId = firstText(provenance.app_id, provenance.appId);
  const source = firstText(provenance.source);
  const appKey = appId || 'manual';
  const sourceKey = source || 'manual';
  const appLabel = appId ? humanizeIdentifier(appId) : 'Manuell';
  const sourceLabel = source ? humanizeIdentifier(source) : 'Import';
  return {
    key: `${appKey}::${sourceKey}`,
    app_key: appKey,
    source_key: sourceKey,
    app_id: appId,
    source,
    app_label: appLabel,
    source_label: sourceLabel,
    label: `${appLabel} · ${sourceLabel}`,
  };
}

function humanizeIdentifier(value) {
  return String(value || '')
    .replace(/[_-]+/g, ' ')
    .replace(/\b\w/g, (letter) => letter.toLocaleUpperCase('de'))
    .trim();
}

function selectedDocumentGroup(state) {
  return groupDocumentRecords(state.documents)
    .find((group) => group.record_ids.includes(state.selectedId)) || null;
}

function normalizeSearchText(value) {
  return String(value || '')
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .toLocaleLowerCase('de')
    .replace(/\s+/g, ' ')
    .trim();
}

function uniqueBy(values, keyFn) {
  const seen = new Set();
  return values.filter((value) => {
    const key = keyFn(value);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function firstText(...values) {
  return values.map((value) => String(value ?? '').trim()).find(Boolean) || '';
}

function plainObject(value) {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function renderRight(state) {
  const slot = state.ctx.host.querySelector('[data-documents-actions-content]');
  if (!slot) return;
  const record = selectedRecord(state);
  const selectedRunbook = defaultRunbookId(state);
  const knowledgeLink = documentKnowledgeLink(record);
  const knowledgeStale = isDocumentKnowledgeStale(state, record);
  const canRunbook = Boolean(record?.id && selectedRunbook && (record.current_version_id || state.selectedVersion?.id));
  const wrap = document.createElement('div');
  wrap.className = 'documents-runbooks';
  wrap.innerHTML = `
    <header class="ctox-pane-header ctox-pane-band documents-runbook-header">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">${escapeHtml(state.t('runbookKicker', 'Aktionen'))}</span>
          <h2 class="ctox-pane-title">${escapeHtml(state.t('runbooksTitle', 'Runbooks'))}</h2>
        </div>
        <div class="ctox-pane-actions">
          <strong class="ctox-badge documents-runbook-state">${record ? escapeHtml(record.document_type || 'word') : escapeHtml(state.t('none', 'keine'))}</strong>
          <button class="ctox-pane-icon" type="button" data-documents-actions-close aria-label="${escapeHtml(state.t('closeActions', 'Aktionen schließen'))}" title="${escapeHtml(state.t('closeActions', 'Aktionen schließen'))}">${actionIcon(state, 'close')}</button>
        </div>
      </div>
    </header>
    <form class="documents-runbook-form" data-documents-runbook-form>
      <label class="documents-runbook-field">
        <span>${escapeHtml(state.t('knowledgeBasis', 'Knowledge-Basis'))}</span>
        <select data-documents-knowledge ${record ? '' : 'disabled'}>
          ${knowledgeOptions(state, knowledgeLink?.id || 'auto')}
        </select>
      </label>
      ${knowledgeLink ? `<div class="documents-knowledge-status${knowledgeStale ? ' is-stale' : ''}"><strong>${escapeHtml(knowledgeLink.title || knowledgeLink.id)}</strong><span>${knowledgeStale ? escapeHtml(state.t('knowledgeNewer', 'Knowledge wurde aktualisiert. Dokument kann neu abgeleitet werden.')) : escapeHtml(state.t('knowledgeCurrent', 'Dokument ist mit diesem Knowledge-Stand verknüpft.'))}</span></div>` : ''}
      <select data-documents-runbook ${record ? '' : 'disabled'}>
        ${runbookOptions(state, selectedRunbook)}
      </select>
      <textarea data-documents-prompt ${record ? '' : 'disabled'} placeholder="${escapeHtml(state.t('promptPlaceholder', 'Prompt für dieses Dokument'))}"></textarea>
      <button type="submit" ${canRunbook ? '' : 'disabled aria-disabled="true"'}>${escapeHtml(state.t('runbookStart', 'Runbook starten'))}</button>
      ${record && knowledgeLink ? `<button type="button" data-documents-refresh-from-knowledge>${escapeHtml(knowledgeStale ? state.t('refreshDocumentKnowledge', 'Mit aktuellem Knowledge aktualisieren') : state.t('rebuildDocumentKnowledge', 'Aus Knowledge neu ableiten'))}</button>` : ''}
      <button class="documents-knowledge-link" type="button" data-documents-open-knowledge>${actionIcon(state, 'knowledge')} ${escapeHtml(state.t('openKnowledge', 'Knowledge öffnen'))}</button>
    </form>
  `;
  wrap.querySelector('[data-documents-runbook-form]')?.addEventListener('submit', async (event) => {
    event.preventDefault();
    if (!canRunbook) return;
    const runbook = wrap.querySelector('[data-documents-runbook]')?.value || defaultRunbookId(state);
    const knowledgeId = wrap.querySelector('[data-documents-knowledge]')?.value || knowledgeLink?.id || 'auto';
    const prompt = wrap.querySelector('[data-documents-prompt]')?.value || '';
    await dispatchDocumentRunbook(state, {
      record,
      versionId: state.selectedVersion?.id || record.current_version_id,
      runbookId: runbook,
      knowledgeId,
      prompt,
      sourceAction: 'manual_runbook',
    });
  });
  wrap.querySelector('[data-documents-refresh-from-knowledge]')?.addEventListener('click', async () => {
    const knowledgeId = wrap.querySelector('[data-documents-knowledge]')?.value || knowledgeLink?.id || 'auto';
    await dispatchNewDocumentReport(state, {
      title: record.title,
      targetDocumentId: record.id,
      runbookId: selectedRunbook,
      knowledgeId,
      prompt: `Aktualisiere das bestehende Dokument "${record.title}" aus dem aktuellsten Stand des verknuepften Knowledge. Bewahre den Dokumentzweck, aktualisiere Fakten, Tabellen, Ableitungen und Quellen und kennzeichne wesentliche Aenderungen nachvollziehbar.`,
      tags: record.tags,
    });
  });
  wrap.querySelector('[data-documents-open-knowledge]')?.addEventListener('click', () => openKnowledgeRunbooks(state));
  wrap.querySelector('[data-documents-actions-close]')?.addEventListener('click', () => {
    state.actionsOpen = false;
    renderPaneVisibility(state);
    renderDocumentStrip(state);
  });
  slot.replaceChildren(wrap);
  renderPaneVisibility(state);
}

async function dispatchDocumentRunbook(state, input) {
  const runbookId = input.runbookId || defaultRunbookId(state);
  if (!runbookId && !String(input.prompt || '').trim()) return null;
  const runbook = state.runbooks.find((item) => item.id === runbookId || item.command_type === runbookId);
  const knowledgeContext = resolveKnowledgeContext(state, input.knowledgeId, `${input.record.title} ${input.prompt || ''}`);
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
      knowledge_context: knowledgeContext,
    },
    client_context: {
      surface: 'business-os-documents',
      filename: input.record.filename,
      document_type: input.record.document_type,
      action: input.sourceAction || 'document_runbook',
      knowledge_context_id: knowledgeContext?.id || '',
      knowledge_domain: knowledgeContext?.domain || '',
    },
  });
}

async function dispatchNewDocumentReport(state, input = {}) {
  const prompt = String(input.prompt || '').trim();
  if (!prompt) {
    throw new Error('Prompt fehlt. CTOX braucht einen Auftrag für das Word-Dokument.');
  }
  const title = sanitizeDocumentTitle(input.title || 'Research Document');
  const runbookId = input.runbookId || 'research.report.auto';
  const runbook = state.runbooks.find((item) => item.id === runbookId || item.command_type === runbookId)
    || SYSTEMATIC_REPORT_RUNBOOKS[0];
  const reportType = runbook.report_type || (runbook.id || '').replace(/^research\.report\./, '') || 'auto';
  const knowledgeContext = resolveKnowledgeContext(state, input.knowledgeId, `${title} ${prompt}`);
  const filename = ensureExtension(slugFilename(title), '.docx');
  const outputPath = `runtime/business-os/documents/generated/${filename}`;
  const commandId = `cmd_${crypto.randomUUID()}`;
  const instruction = [
    `Erstelle das Word-Dokument "${title}".`,
    `Nutzerauftrag: ${prompt}`,
    '',
    'Nutze den systematic-research Skill, die CTOX Report-Pipeline und den doc/Documents Word-Produktionsskill.',
    knowledgeContextInstruction(knowledgeContext),
    'Der Skill strukturiert die Ableitung, ist aber keine Primaerquelle. Lies fuer faktische Aussagen die darin referenzierten Originalquellen und zitiere diese, nicht den Skill als Ersatzquelle.',
    'Halte die Knowledge-Lookup-Pflicht aus dem systematic-research Skill ein und verwende immer den neuesten verfuegbaren Knowledge-Stand.',
    reportType && reportType !== 'auto'
      ? `Verwende report_type_id=${reportType}.`
      : 'Wähle den passenden report_type_id aus den CTOX Report-Blueprints.',
    'Erzeuge ein solides .docx-Dokument mit sauberer Struktur, Quellen/Evidenz, Tabellen und sinnvollen Abbildungen/Diagrammen, wo sie fachlich tragen.',
    'Wende den Documents-Skill-Workflow an: Design-Preset, echte Word-Styles, echte Listen/Tabellengeometrie, DOCX-Render/QA soweit die lokale Runtime es erlaubt.',
    'Erzeuge kein Markdown als Endartefakt. Das finale Artefakt muss ein Word-Dokument sein.',
    `Speichere das finale DOCX im Workspace unter ${outputPath}.`,
    'Der Business-OS Writeback registriert genau diese DOCX-Datei danach automatisch im Documents-Modul.',
  ].join('\n');
  const command = {
    id: commandId,
    module: 'documents',
    type: runbook.command_type || 'research.systematic.report.create',
    record_id: input.targetDocumentId || '',
    inbound_channel: 'business_os.documents',
    payload: {
      title,
      instruction,
      prompt,
      report_type_id: reportType,
      selected_runbook_id: runbook.id || runbookId,
      selected_runbook: runbook,
      desired_format: 'docx',
      output_filename: filename,
      output_path: outputPath,
      required_skills: ['systematic-research', 'knowledge', 'doc'],
      knowledge_context: knowledgeContext,
      tags: normalizeTags(input.tags),
      thread_key: 'business-os/documents',
      required_artifacts: [outputPath],
      document_quality_contract: {
        use_documents_skill: true,
        final_artifact_format: 'docx',
        require_real_word_styles: true,
        require_tables_and_figures_when_useful: true,
        require_render_or_structural_qa: true,
      },
      writeback_contract: {
        module: 'documents',
        collection: 'documents',
        desired_format: 'docx',
        document_type: 'word_document',
        title,
        filename,
        output_path: outputPath,
        document_id: input.targetDocumentId || '',
        preserve_knowledge_lineage: true,
      },
    },
    client_context: {
      module: 'documents',
      surface: 'documents-new-document',
      action: 'create_word_document',
      source_module: 'documents',
      inbound_channel: 'business_os.documents',
      selected_runbook_id: runbook.id || runbookId,
      report_type_id: reportType,
      document_type: 'word_document',
      filename,
      output_path: outputPath,
      knowledge_context_id: knowledgeContext?.id || '',
      knowledge_domain: knowledgeContext?.domain || '',
      knowledge_selection_mode: knowledgeContext?.selection_mode || 'auto',
    },
  };
  const result = await state.ctx.commandBus.dispatch(command);
  rememberDocumentTask(result, {
    title,
    filename,
    runbookId: runbook.id || runbookId,
    reportType,
  });
  return result;
}

function rememberDocumentTask(result, meta = {}) {
  const trackingId = result?.task_id || result?.command_id || '';
  if (!trackingId) return;
  try {
    sessionStorage.setItem('ctox.businessOs.focusTask', JSON.stringify({
      taskId: result.task_id || trackingId,
      commandId: result.command_id || '',
      module: 'documents',
      source: 'documents-new-document',
      title: meta.title || '',
      filename: meta.filename || '',
      runbookId: meta.runbookId || '',
      reportType: meta.reportType || '',
      createdAt: new Date().toISOString(),
    }));
  } catch (_) {
    // Ignore unavailable session storage.
  }
}

function runbookOptions(state, selectedId = '', options = {}) {
  const runbooks = state.runbooks.length ? state.runbooks : mergeDocumentRunbooks([]);
  const optionHtml = runbooks.map((runbook) => {
    const value = runbook.id || runbook.command_type;
    const label = state.t(`runbooks.${value}.title`, runbook.title || runbook.command_type || value);
    return `<option value="${escapeHtml(value)}" ${value === selectedId || runbook.command_type === selectedId ? 'selected' : ''}>${escapeHtml(label)}</option>`;
  }).join('');
  return options.includeNone ? `<option value="">${escapeHtml(state.t('noRunbook', 'Kein Runbook'))}</option>${optionHtml}` : optionHtml;
}

function defaultRunbookId(state) {
  return state.runbooks[0]?.id || state.runbooks[0]?.command_type || 'research.report.auto';
}

async function openKnowledgeRunbooks(state) {
  const result = await state.ctx.commandBus.dispatch({
    module: 'ctox',
    command_type: 'ctox.knowledge.runbooks.manage',
    payload: {
      title: state.t('manageDocumentRunbooksTitle', 'Document runbooks verwalten'),
      instruction: state.t('manageDocumentRunbooksInstruction', 'Öffne das CTOX Knowledge-System für die Verwaltung von dokumentbezogenen Skillbooks, Runbooks und Runbook-Items. Fokus: document/docx/markdown Runbooks, die vom Business-OS Documents-Modul beim Erstellen, Importieren und manuellen Ausführen verwendet werden.'),
      knowledge_scope: {
        form: 'procedural',
        cli_namespace: 'ctox knowledge skill',
        related_tables: ['knowledge_main_skills', 'knowledge_skillbooks', 'knowledge_runbooks', 'knowledge_runbook_items'],
        module_local_seed_collection: 'document_runbooks',
      },
      current_document_runbooks: state.runbooks.map((runbook) => ({
        id: runbook.id,
        command_type: runbook.command_type,
        title: state.t(`runbooks.${runbook.id}.title`, runbook.title || runbook.command_type),
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

async function renderCenter(state) {
  const host = state.ctx.host.querySelector('[data-documents-editor]');
  const record = selectedRecord(state);
  renderDocumentStrip(state);
  if (ctoxDocumentsDraftProtected(state)) return state.editorHandle;
  const version = state.selectedVersion;
  const renderKey = editorRenderKey(record, version, state.officeEngine);
  const reusableTarget = currentEditorTarget(state, renderKey);
  if (reusableTarget) return reusableTarget;
  const renderSerial = (state.renderSerial || 0) + 1;
  state.renderSerial = renderSerial;
  if (state.superdocSaveTimer) {
    clearTimeout(state.superdocSaveTimer);
    state.superdocSaveTimer = null;
  }
  await destroyActiveEditor(state);
  if (state.renderSerial !== renderSerial) return;

  if (!record) {
    state.mailMergeNavigation = null;
    renderDocumentStrip(state);
    host.innerHTML = `<div class="ctox-empty documents-empty"><strong>${escapeHtml(state.t('noDocumentSelected', 'Kein Dokument ausgewählt.'))}</strong><span>${escapeHtml(state.t('noDocumentSelectedPrompt', 'Links ein DOCX importieren oder auswählen.'))}</span></div>`;
    return;
  }
  if (!version) {
    host.innerHTML = `<div class="ctox-empty documents-loading"><strong>${escapeHtml(state.t('loadingDocument', 'Lade Dokument'))}</strong><span>${escapeHtml(state.t('versionLoading', 'Version wird gelesen.'))}</span></div>`;
    loadSelectedVersion(state)
      .then((loadedVersion) => {
        if (state.renderSerial !== renderSerial) return;
        if (loadedVersion) {
          renderCenter(state);
          return;
        }
        renderError(state, state.t('noSavedVersionFound', 'Zu diesem Dokument wurde keine gespeicherte Version gefunden. Bitte erneut importieren oder den Datensatz verwalten.'));
      })
      .catch((error) => {
        if (state.renderSerial !== renderSerial) return;
        renderError(state, `${state.t('loadVersionFailed', 'Dokumentversion konnte nicht geladen werden:')} ${error?.message || error}`);
      });
    return;
  }
  if (!state.mailMergeNavigation && selectedDocumentGroup(state)?.is_mail_merge) {
    await refreshMailMergeNavigation(state);
    if (state.renderSerial !== renderSerial) return;
    renderDocumentStrip(state);
  }
  if (isDocxDocumentRecord(record)) {
    const useCtoxDocuments = state.officeEngine === 'ctox_documents';
    host.innerHTML = `<div class="ctox-empty documents-loading"><strong>${escapeHtml(state.t('loadingDocxEditor', 'Lade Word-Editor'))}</strong><span>${escapeHtml(useCtoxDocuments ? state.t('documentEditorInitializing', 'Dokumenteditor wird initialisiert.') : state.t('superdocInitializing', 'SuperDoc wird initialisiert.'))}</span></div>`;
    const mountEditor = useCtoxDocuments ? mountCtoxDocuments : mountSuperDocDocument;
    const mountPromise = mountWordEditor(state, host, record, version, renderSerial, renderKey, mountEditor, useCtoxDocuments);
    return trackEditorMount(state, renderKey, mountPromise);
  }

  host.innerHTML = `<div class="ctox-empty documents-loading"><strong>${escapeHtml(state.t('loadingEditor', 'Lade Editor'))}</strong><span>${escapeHtml(state.t('documentPreparing', 'Dokument wird vorbereitet.'))}</span></div>`;
  ensureDocumentFormatModule(state).then((formatModule) => {
    if (state.renderSerial !== renderSerial) return;
    mountMarkdownDocument(state, host, version, formatModule, renderKey);
  }).catch((error) => {
    if (state.renderSerial !== renderSerial) return;
    renderError(state, `${state.t('editorLoadFailed', 'Editor konnte nicht geladen werden:')} ${error?.message || error}`);
  });
}

function editorRenderKey(record, version, officeEngine = '') {
  if (!record?.id || !version?.id) return '';
  return `${record.id}:${version.id}:${String(officeEngine || 'default')}`;
}

function currentEditorTarget(state, renderKey) {
  if (!renderKey) return null;
  if (state.editorHandle?.renderKey === renderKey) return Promise.resolve(state.editorHandle);
  if (state.editorMountKey === renderKey && state.editorMountPromise) return state.editorMountPromise;
  return null;
}

function isCurrentEditorRender(state, renderSerial) {
  return !state.disposed && state.renderSerial === renderSerial;
}

function trackEditorMount(state, renderKey, mountPromise) {
  const trackedPromise = Promise.resolve(mountPromise);
  state.editorMountKey = renderKey;
  state.editorMountPromise = trackedPromise;
  const clearTrackedMount = () => {
    if (state.editorMountPromise !== trackedPromise) return;
    state.editorMountKey = '';
    state.editorMountPromise = null;
  };
  trackedPromise.then(clearTrackedMount, clearTrackedMount);
  return trackedPromise;
}

async function mountWordEditor(state, host, record, version, renderSerial, renderKey, mountEditor, useCtoxDocuments) {
  try {
    await initializeOfficeEditorWithRecovery({
      initialize: () => mountEditor(state, host, record, version, renderSerial, renderKey),
      isCurrent: () => isCurrentEditorRender(state, renderSerial),
      shouldRetry: (error) => useCtoxDocuments && isTransientOfficeStartupError(error),
      onRetry: async () => {
        host.innerHTML = `<div class="ctox-empty documents-loading"><strong>${escapeHtml(state.t('loadingDocxEditor', 'Lade DOCX Editor'))}</strong><span>${escapeHtml(state.t('officeEditorRetry', 'Verbindung wird wiederhergestellt. Dokument wird erneut geöffnet.'))}</span></div>`;
      },
      wait: delay,
      retryDelayMs: CTOX_DOCUMENTS_RECOVERY_DELAY_MS,
      maxRecoveryAttempts: CTOX_DOCUMENTS_MAX_RECOVERY_ATTEMPTS,
    });
  } catch (error) {
    if (state.renderSerial !== renderSerial) return;
    console.error(`[documents] ${useCtoxDocuments ? 'CTOX Documents' : 'SuperDoc'} mount failed`, error);
    renderError(state, `${state.t('docxEditorLoadFailed', 'DOCX editor konnte nicht geladen werden:')} ${error?.message || error}`);
  }
}

function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function initializeOfficeEditorWithRecovery(options = {}) {
  const initialize = options.initialize;
  if (typeof initialize !== 'function') throw new TypeError('Office editor initialization is required');
  const isCurrent = typeof options.isCurrent === 'function' ? options.isCurrent : () => true;
  const shouldRetry = typeof options.shouldRetry === 'function' ? options.shouldRetry : () => false;
  const onRetry = typeof options.onRetry === 'function' ? options.onRetry : async () => {};
  const wait = typeof options.wait === 'function' ? options.wait : delay;
  const retryDelayMs = Math.max(0, Number(options.retryDelayMs) || 0);
  const maxRecoveryAttempts = Math.max(0, Number(options.maxRecoveryAttempts) || 0);
  let recoveryAttempts = 0;

  while (isCurrent()) {
    try {
      await initialize();
      return { recovered: recoveryAttempts > 0, recoveryAttempts };
    } catch (error) {
      if (!isCurrent()) return { cancelled: true, recovered: false, recoveryAttempts };
      if (recoveryAttempts >= maxRecoveryAttempts || !shouldRetry(error)) throw error;
      recoveryAttempts += 1;
      await onRetry(error, recoveryAttempts);
      if (retryDelayMs > 0) await wait(retryDelayMs);
    }
  }
  return { cancelled: true, recovered: false, recoveryAttempts };
}

function isTransientOfficeStartupError(error) {
  const message = String(error?.message || error || '').toLowerCase();
  const code = String(error?.code || '').toLowerCase();
  const method = String(error?.details?.method || error?.method || '').toLowerCase();
  const retryableRpcMethod = ['bridge.loadversion', 'editor.open', 'editor.ready'].includes(method);
  return message.includes('iframe load timed out')
    || message.includes('editor.ready')
    || message.includes('app-ready timed out')
    || message.includes('fork sdk load timed out')
    || message.includes('webrtc replication cancelled')
    || message.includes('replication-cancel')
    || message.includes('query_cancelled')
    || message.includes('office rpc timed out: bridge.loadversion')
    || message.includes('office rpc timed out: editor.open')
    || (code === 'rpc_timeout' && retryableRpcMethod)
    || code === 'rpc_closed';
}

function canReplaceActiveEditorVersion(state) {
  return typeof state.editorHandle?.openVersion === 'function';
}

async function replaceActiveEditorVersion(state, record, version, options = {}) {
  const editorHandle = state.editorHandle;
  if (!record?.id || !version?.id || !canReplaceActiveEditorVersion(state)) {
    throw new Error('Der aktive Office-Editor kann diese Dokumentversion nicht direkt öffnen.');
  }
  const openResult = await editorHandle.openVersion({ recordId: record.id, versionId: version.id, version });
  await waitForActiveEditorVersionContent(editorHandle, record, version, { ...options, openResult });
  editorHandle.renderKey = editorRenderKey(record, version, state.officeEngine);
  editorHandle.focus?.();
}

async function waitForActiveEditorVersionContent(editorHandle, record, version, options = {}) {
  const timeoutMs = Math.max(0, Number(options.timeoutMs ?? EDITOR_VERSION_CONTENT_READY_TIMEOUT_MS));
  const pollMs = Math.max(0, Number(options.pollIntervalMs ?? EDITOR_VERSION_CONTENT_READY_POLL_MS));
  const deadline = Date.now() + timeoutMs;
  let inspection = options.openResult || null;
  let inspectionError = null;

  while (true) {
    if (isEditorVersionContentReady(inspection, record, version)) return inspection;
    if (typeof editorHandle?.inspect === 'function') {
      try {
        inspection = await withTimeout(
          editorHandle.inspect(),
          Math.max(1, deadline - Date.now()),
          `Office-Editor-Inspektion für Version ${version?.id || ''} hat zu lange gedauert.`,
        );
        inspectionError = null;
      } catch (error) {
        inspectionError = error;
      }
      if (isEditorVersionContentReady(inspection, record, version)) return inspection;
    }
    if (Date.now() >= deadline) break;
    await delay(Math.min(pollMs || 1, Math.max(1, deadline - Date.now())));
  }

  const error = new Error(
    inspectionError?.message
      || `Dokumentversion ${version?.id || ''} wurde vom Office-Editor nicht darstellbar geöffnet.`,
  );
  error.code = 'editor_content_not_ready';
  error.details = { inspection };
  throw error;
}

function isEditorVersionContentReady(inspection, record, version) {
  if (!inspection || typeof inspection !== 'object') return false;
  const inspectedRecordId = String(inspection.record_id || inspection.recordId || '');
  const inspectedVersionId = String(inspection.version_id || inspection.versionId || '');
  if (record?.id && inspectedRecordId !== String(record.id)) return false;
  if (version?.id && inspectedVersionId !== String(version.id)) return false;
  return inspection.document_ready === true
    || inspection.documentReady === true
    || inspection.content_ready === true
    || inspection.contentReady === true
    || inspection.open === true;
}

async function destroyActiveEditor(state) {
  const editorHandle = state.editorHandle;
  state.editorHandle = null;
  const previousDestroy = state.editorDestroyPromise || Promise.resolve();
  if (!editorHandle) {
    await previousDestroy.catch((error) => {
      console.warn('[documents] previous editor destroy failed', error);
    });
    return;
  }

  const destroyPromise = previousDestroy
    .catch((error) => {
      console.warn('[documents] previous editor destroy failed', error);
    })
    .then(() => editorHandle.destroy?.());
  state.editorDestroyPromise = destroyPromise;
  try {
    await destroyPromise;
  } catch (error) {
    console.warn('[documents] previous editor destroy failed', error);
  } finally {
    if (state.editorDestroyPromise === destroyPromise) state.editorDestroyPromise = null;
  }
}

function mountMarkdownDocument(state, host, version, formatModule, renderKey = '') {
  host.replaceChildren();
  const wrap = document.createElement('div');
  wrap.className = 'documents-markdown-editor';
  const toolbar = document.createElement('div');
  toolbar.className = 'documents-markdown-toolbar';
  const editButton = document.createElement('button');
  editButton.type = 'button';
  editButton.className = 'documents-markdown-toggle';
  editButton.textContent = state.t('edit', 'Bearbeiten');
  toolbar.append(editButton);
  const preview = document.createElement('article');
  preview.className = 'documents-markdown-preview';
  const textarea = document.createElement('textarea');
  textarea.className = 'documents-markdown-textarea';
  textarea.value = formatModule.exportMarkdown(version.model_json);
  textarea.spellcheck = true;
  textarea.hidden = true;
  preview.innerHTML = renderMarkdownPreview(textarea.value);
  wrap.append(toolbar, preview, textarea);
  host.append(wrap);

  let saveSerial = 0;
  let editing = false;
  const setEditing = (nextEditing) => {
    editing = Boolean(nextEditing);
    textarea.hidden = !editing;
    preview.hidden = editing;
    editButton.textContent = editing ? state.t('preview', 'Vorschau') : state.t('edit', 'Bearbeiten');
    editButton.setAttribute('aria-pressed', String(editing));
    if (editing) textarea.focus();
  };
  editButton.addEventListener('click', () => {
    if (editing) {
      preview.innerHTML = renderMarkdownPreview(textarea.value);
      setEditing(false);
      return;
    }
    setEditing(true);
  });
  textarea.addEventListener('input', () => {
    const parsed = formatModule.importMarkdown(textarea.value);
    const document = parsed.document;
    const serial = ++saveSerial;
    state.dirty = true;
    version.model_json = document;
    preview.innerHTML = renderMarkdownPreview(textarea.value);
    saveDraftVersion(state, document).catch((error) => {
      if (serial === saveSerial) console.error('[documents] Markdown draft save failed', error);
    });
  });

  state.editorHandle = {
    kind: 'markdown',
    renderKey,
    destroy() {
      host.replaceChildren();
    },
    focus() {
      if (editing) textarea.focus();
      else preview.focus?.();
    },
  };
  setEditing(false);
}

function renderMarkdownPreview(markdown) {
  const lines = String(markdown || '').replace(/\r\n/g, '\n').split('\n');
  const html = [];
  let paragraph = [];
  let listOpen = false;

  const flushParagraph = () => {
    if (!paragraph.length) return;
    html.push(`<p>${renderInlineMarkdown(paragraph.join(' '))}</p>`);
    paragraph = [];
  };
  const closeList = () => {
    if (!listOpen) return;
    html.push('</ul>');
    listOpen = false;
  };

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed) {
      flushParagraph();
      closeList();
      continue;
    }
    const heading = /^(#{1,4})\s+(.+)$/.exec(trimmed);
    if (heading) {
      flushParagraph();
      closeList();
      const level = Math.min(heading[1].length, 4);
      html.push(`<h${level}>${renderInlineMarkdown(heading[2])}</h${level}>`);
      continue;
    }
    const bullet = /^[-*]\s+(.+)$/.exec(trimmed);
    if (bullet) {
      flushParagraph();
      if (!listOpen) {
        html.push('<ul>');
        listOpen = true;
      }
      html.push(`<li>${renderInlineMarkdown(bullet[1])}</li>`);
      continue;
    }
    paragraph.push(trimmed);
  }

  flushParagraph();
  closeList();
  return html.join('') || '<p class="documents-markdown-empty">Leeres Dokument</p>';
}

function renderInlineMarkdown(value) {
  return escapeHtml(value)
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/\*([^*]+)\*/g, '<em>$1</em>');
}

function ctoxDocumentsDraftProtected(state) {
  const handle = state.editorHandle;
  return Boolean(handle?.kind === 'ctox-documents'
    && handle.recordId === state.selectedId
    && (state.dirty || handle.saving || state.superdocSavePromise));
}

async function mountCtoxDocuments(state, host, record, version, renderSerial, renderKey) {
  const { createCtoxDocumentsEditor } = await loadCtoxDocumentsModule(state);
  if (!isCurrentEditorRender(state, renderSerial)) return;
  host.replaceChildren();
  const mount = document.createElement('div');
  mount.className = 'documents-superdoc-frame documents-ctox-documents-frame';
  host.append(mount);
  const permissions = ctoxDocumentsPermissions(state.ctx);
  const editor = await createCtoxDocumentsEditor({
    host: mount,
    bridge: createBusinessOsOfficeBridge(state.ctx, 'document'),
    locale: state.lang,
    theme: document.documentElement.dataset.theme || 'system',
    permissions,
    loadTimeoutMs: CTOX_DOCUMENTS_LOAD_TIMEOUT_MS,
    readyTimeoutMs: CTOX_DOCUMENTS_READY_TIMEOUT_MS,
  });
  if (!isCurrentEditorRender(state, renderSerial)) {
    await editor.destroy();
    return;
  }
  let handle;
  const isActive = () => handle && state.editorHandle === handle
    && state.selectedId === record.id && isCurrentEditorRender(state, renderSerial);
  const removeDirtyListener = editor.on('dirty', () => {
    if (!isActive()) return;
    markCtoxDocumentsDraft(state, record, handle);
  });
  const removeSavingListener = editor.on('saving', () => {
    if (!isActive()) return;
    handle.saving = true;
    handle.activity += 1;
    handle.savingActivity = handle.activity;
  });
  const removeSavedListener = editor.on('saved', ({ versionId, dirty } = {}) => {
    if (!isActive() || !versionId) return;
    handle.saving = false;
    handle.activity += 1;
    record.current_version_id = versionId;
    const currentRecord = state.documents.find((item) => item.id === record.id);
    if (currentRecord) currentRecord.current_version_id = versionId;
    state.selectedVersion = { ...state.selectedVersion, id: versionId, version_id: versionId };
    handle.renderKey = editorRenderKey(record, state.selectedVersion, state.officeEngine);
    state.dirty = Boolean(dirty);
    state.needsFinalSave = state.dirty;
    if (state.dirty) scheduleCtoxDocumentsDraftSave(state, record);
  });
  const removeErrorListener = editor.on('error', (payload) => {
    if (!isActive()) return;
    handle.saving = false;
    state.dirty = true;
    state.needsFinalSave = true;
    if (state.superdocSaveTimer) clearTimeout(state.superdocSaveTimer);
    state.superdocSaveTimer = null;
    const error = payload?.error || payload;
    state.ctx.notifications?.show?.({
      type: 'error', message: error?.message || String(error), time: 0,
      action: { label: state.t('close', 'Schließen'), callback: () => {} },
    });
  });
  const cleanupCallbacks = [removeDirtyListener, removeSavingListener, removeSavedListener, removeErrorListener];
  await openOfficeEditorInstance(
    editor,
    { recordId: record.id, versionId: version.id },
    cleanupCallbacks,
  );
  if (!isCurrentEditorRender(state, renderSerial)) {
    for (const cleanup of cleanupCallbacks) cleanup();
    await editor.destroy();
    return;
  }
  state.editorHandle = handle = {
    kind: 'ctox-documents',
    recordId: record.id,
    activity: 0,
    saving: false,
    renderKey,
    editor,
    async destroy() {
      for (const cleanup of cleanupCallbacks) cleanup();
      await editor.destroy();
      host.replaceChildren();
    },
    openVersion: (request) => editor.open(request),
    save: (options) => editor.save(options),
    export: async () => (await editor.export({ format: 'docx' })).bytes,
    focus: () => editor.focus(),
    inspect: () => editor.inspect(),
  };
  scheduleMailMergeBlobPrefetch(state);
}

async function openOfficeEditorInstance(editor, request, cleanupCallbacks = []) {
  try {
    await editor.open(request);
  } catch (error) {
    for (const cleanup of cleanupCallbacks) {
      try {
        cleanup?.();
      } catch (cleanupError) {
        console.warn('[documents] failed Office editor listener cleanup after open error', cleanupError);
      }
    }
    try {
      await editor.destroy();
    } catch (destroyError) {
      console.warn('[documents] failed Office editor cleanup after open error', destroyError);
    }
    throw error;
  }
}

function ctoxDocumentsPermissions(ctx) {
  const canWrite = ctx.permissions?.canWriteCollection?.('documents') !== false
    && ctx.permissions?.canWriteCollection?.('document_versions') !== false
    && ctx.permissions?.canWriteCollection?.('document_blob_chunks') !== false;
  return { read: true, write: canWrite, export: true, comment: canWrite, review: canWrite };
}

function markCtoxDocumentsDraft(state, record, handle) {
  handle.activity += 1;
  state.dirty = true;
  state.needsFinalSave = true;
  // The native commit persists Draft and advances the version atomically.
  // A browser status patch would replicate the entire possibly stale record
  // and can send its old current_version_id after a newer commit succeeds.
  record.status = 'Draft';
  record.updated_at_ms = Date.now();
  scheduleCtoxDocumentsDraftSave(state, record);
}

function scheduleCtoxDocumentsDraftSave(state, record) {
  if (state.superdocSaveTimer) clearTimeout(state.superdocSaveTimer);
  state.superdocSaveTimer = setTimeout(() => {
    state.superdocSaveTimer = null;
    flushActiveCtoxDocumentsDraft(state, record).catch((error) => {
      console.error('[documents] CTOX Documents draft save failed', error);
    });
  }, 900);
}

async function flushActiveCtoxDocumentsDraft(state, record = selectedRecord(state), options = {}) {
  const handle = state.editorHandle;
  const selectedId = state.selectedId;
  if (handle?.kind !== 'ctox-documents' || !record || handle.recordId !== record.id
    || (!state.dirty && !options.force)) return null;
  if (state.superdocSavePromise) return state.superdocSavePromise;
  const isActive = () => state.editorHandle === handle && state.selectedId === selectedId
    && selectedId === handle.recordId;
  const savePromise = Promise.resolve().then(() => handle.save({ reason: options.final === false ? 'autosave' : 'final' }));
  state.superdocSavePromise = savePromise;
  let saved = false;
  try {
    const result = await savePromise;
    saved = true;
    return result;
  } catch (error) {
    if (isActive()) {
      handle.saving = false;
      state.dirty = true;
      state.needsFinalSave = true;
      if (state.superdocSaveTimer) clearTimeout(state.superdocSaveTimer);
      state.superdocSaveTimer = null;
    }
    if (!options.allowFailure) throw error;
    console.warn('[documents] ignored CTOX Documents draft save failure', error);
    return null;
  } finally {
    if (state.superdocSavePromise === savePromise) {
      state.superdocSavePromise = null;
      if (saved && isActive() && state.dirty) scheduleCtoxDocumentsDraftSave(state, record);
    }
  }
}

function flushActiveEditorDraft(state, record = selectedRecord(state), options = {}) {
  if (state.editorHandle?.kind === 'ctox-documents') {
    return flushActiveCtoxDocumentsDraft(state, record, options);
  }
  return flushActiveSuperDocDraft(state, record, options);
}

async function mountSuperDocDocument(state, host, record, version, renderSerial, renderKey) {
  const bytes = await loadBlobBytes(
    state.ctx,
    version.blob_id,
    bindDocumentBlobByteCache(state, record.id),
  );
  if (!isCurrentEditorRender(state, renderSerial)) return;
  if (!bytes) throw new Error(`Missing DOCX blob ${version.blob_id}`);
  const { SuperDoc } = await loadSuperDocModule(state);
  if (!isCurrentEditorRender(state, renderSerial)) return;
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
  toolbarToggle.textContent = state.docxToolbarVisible ? state.t('hideEditorToolbar', 'Editorleiste ausblenden') : state.t('showEditorToolbar', 'Editorleiste einblenden');
  toolbarToggle.setAttribute('aria-pressed', String(state.docxToolbarVisible));
  toolbarToggle.setAttribute('aria-label', toolbarToggle.textContent);
  toolbarToggle.addEventListener('click', () => {
    state.docxToolbarVisible = !state.docxToolbarVisible;
    try { state.ctx?.storageScope?.set?.(DOCX_TOOLBAR_VISIBILITY_KEY, String(state.docxToolbarVisible)); } catch {}
    frame.dataset.toolbarVisible = String(state.docxToolbarVisible);
    toolbarToggle.textContent = state.docxToolbarVisible ? state.t('hideEditorToolbar', 'Editorleiste ausblenden') : state.t('showEditorToolbar', 'Editorleiste einblenden');
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
  const file = new File([bytes], version.filename || record.filename || 'document.docx', { type: DOCX_MIME });
  if (!isCurrentEditorRender(state, renderSerial)) return;
  let replacingVersion = false;
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
      if (replacingVersion) return;
      state.dirty = true;
      state.needsFinalSave = true;
      await markRecordDraft(state, record);
      scheduleSuperDocDraftSave(state, record);
    },
    onException: (event) => {
      console.error('[documents] SuperDoc exception', event);
    },
  });
  if (!isCurrentEditorRender(state, renderSerial)) {
    superdoc.destroy?.();
    return;
  }
  const replaceFile = superdoc.activeEditor?.replaceFile;
  state.editorHandle = {
    kind: 'superdoc',
    renderKey,
    superdoc,
    ...(typeof replaceFile === 'function' ? {
      async openVersion({ version: nextVersion }) {
        const nextBytes = await loadBlobBytes(
          state.ctx,
          nextVersion?.blob_id,
          bindDocumentBlobByteCache(state, record.id),
        );
        if (!nextBytes) throw new Error(`Missing DOCX blob ${nextVersion?.blob_id || ''}`);
        const nextFile = new File(
          [nextBytes],
          nextVersion.filename || record.filename || 'document.docx',
          { type: DOCX_MIME },
        );
        replacingVersion = true;
        try {
          await replaceFile.call(superdoc.activeEditor, nextFile);
          state.dirty = false;
          return {
            record_id: record.id,
            version_id: nextVersion.id,
            content_ready: true,
          };
        } finally {
          replacingVersion = false;
        }
      },
    } : {}),
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
    inspect() {
      return {
        record_id: record.id,
        version_id: state.selectedVersion?.id || version.id,
        content_ready: true,
      };
    },
  };
  scheduleMailMergeBlobPrefetch(state);
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
    const versionDoc = await documentCollection(state.ctx, 'document_versions').findOne(versionId).exec();
    const previousBlobId = versionDoc?.get('blob_id') || '';
    await versionDoc?.incrementalPatch({
      source_kind: 'edited_docx',
      blob_id: blobId,
      updated_at_ms: now,
    });
    // The version now points at the new blob; reclaim the superseded draft
    // blob's chunks so per-keystroke autosaves don't accumulate unbounded full-
    // document copies that replicate over WebRTC (the original imported blob is
    // preserved).
    if (isReclaimableDraftBlob(previousBlobId, blobId)) {
      await deleteBlobChunks(state.ctx, previousBlobId).catch((error) => {
        console.warn('[documents] failed to reclaim superseded draft blob', error);
      });
    }
    const recordDoc = await documentCollection(state.ctx, 'documents').findOne(recordId).exec();
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
  const recordDoc = await documentCollection(state.ctx, 'documents').findOne(record.id).exec();
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
  const versionDoc = await documentCollection(state.ctx, 'document_versions').findOne(state.selectedVersion.id).exec();
  await versionDoc?.incrementalPatch({
    model_json: document,
    updated_at_ms: now,
  });
  const recordDoc = await documentCollection(state.ctx, 'documents').findOne(record.id).exec();
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
  if (!record) return;
  if (!state.selectedVersion && record.current_version_id) {
    await loadSelectedVersion(state);
  }
  if (!state.selectedVersion) return;
  const formatModule = await ensureDocumentFormatModule(state);
  const isMarkdown = record.document_type === 'markdown_document' || record.mime_type === MARKDOWN_MIME;
  if (!isMarkdown && !isDocxDocumentRecord(record)) {
    renderError(state, state.t('unsupportedDocumentExport', 'Dieser Dokumenttyp kann nicht als DOCX exportiert werden.'));
    return;
  }
  let data;
  if (isMarkdown) {
    data = formatModule.exportMarkdown(state.selectedVersion.model_json);
  } else if (state.editorHandle?.kind === 'superdoc' || state.editorHandle?.kind === 'ctox-documents') {
    data = await state.editorHandle.export();
  } else {
    renderError(state, state.t('docxExportSuperDocRequired', 'DOCX Export benötigt einen aktiven Office Editor. Bitte das Dokument erneut öffnen und danach exportieren.'));
    return;
  }
  const blob = data instanceof Blob ? data : new Blob([data], { type: isMarkdown ? MARKDOWN_MIME : DOCX_MIME });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  const selectedFilename = state.selectedVersion?.filename || record.filename;
  a.download = sanitizeExportFilename(requestedFilename, isMarkdown)
    || selectedFilename.replace(/\.(docx|md|markdown)$/i, '') + (isMarkdown ? '-edited.md' : '-edited.docx');
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

async function saveBlobChunks(ctx, input) {
  requireDocumentPersistence(ctx);
  await createBusinessOsOfficeBridge(ctx, 'document').stageSourceBlob({
    recordId: input.documentId,
    versionId: input.versionId,
    blobId: input.blobId,
    mimeType: input.mimeType,
    bytes: input.bytes,
  });
}

async function writeCollectionDocuments(collection, docs) {
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
    await collection.insert(doc);
  }
}

// True when a version's previous blob is a superseded draft blob that can be
// garbage-collected after the version is repointed to a new one. Draft blobs are
// timestamped per autosave; the original imported source blob (no `_draft_`
// marker) and the freshly written blob are never reclaimed.
function isReclaimableDraftBlob(previousBlobId, currentBlobId) {
  return Boolean(previousBlobId)
    && previousBlobId !== currentBlobId
    && String(previousBlobId).includes('_draft_');
}

async function deleteBlobChunks(ctx, blobId) {
  requireDocumentPersistence(ctx);
  if (!blobId) return;
  const chunks = await documentCollection(ctx, 'document_blob_chunks')
    .find({ selector: { blob_id: blobId } })
    .exec();
  await Promise.all(chunks.map((chunk) => chunk.remove()));
}

function createDocumentBlobByteCache(options = {}) {
  return {
    documentId: '',
    entries: new Map(),
    totalBytes: 0,
    maxEntries: Math.max(1, Number(options.maxEntries) || DOCUMENT_BLOB_CACHE_MAX_ENTRIES),
    maxBytes: Math.max(1, Number(options.maxBytes) || DOCUMENT_BLOB_CACHE_MAX_BYTES),
  };
}

function bindDocumentBlobByteCache(state, documentId) {
  if (!state.blobByteCache) state.blobByteCache = createDocumentBlobByteCache();
  const normalizedDocumentId = String(documentId || '');
  if (state.blobByteCache.documentId === normalizedDocumentId) return state.blobByteCache;
  cancelMailMergeBlobPrefetch(state);
  state.blobByteCache.entries.clear();
  state.blobByteCache.totalBytes = 0;
  state.blobByteCache.documentId = normalizedDocumentId;
  return state.blobByteCache;
}

function clearDocumentBlobByteCache(state) {
  cancelMailMergeBlobPrefetch(state);
  if (!state.blobByteCache) return;
  state.blobByteCache.entries.clear();
  state.blobByteCache.totalBytes = 0;
  state.blobByteCache.documentId = '';
}

async function loadBlobBytes(ctx, blobId, cache = null) {
  requireDocumentPersistence(ctx);
  if (!blobId) return null;
  const cached = cache?.entries?.get(blobId);
  if (cached) {
    cache.entries.delete(blobId);
    cache.entries.set(blobId, cached);
    return cached.promise;
  }
  const entry = { promise: null, bytes: null, size: 0 };
  entry.promise = (async () => {
    const chunks = await documentCollection(ctx, 'document_blob_chunks').find({
      selector: { blob_id: blobId },
      sort: [{ idx: 'asc' }],
    }).exec();
    if (!chunks.length) {
      if (cache?.entries?.get(blobId) === entry) cache.entries.delete(blobId);
      return null;
    }
    const base64 = chunks.map((chunk) => chunk.toJSON().data || '').join('');
    const bytes = base64ToUint8(base64);
    if (cache?.entries?.get(blobId) === entry) {
      entry.bytes = bytes;
      entry.size = bytes.byteLength;
      cache.totalBytes += entry.size;
      trimDocumentBlobByteCache(cache);
    }
    return bytes;
  })().catch((error) => {
    if (cache?.entries?.get(blobId) === entry) cache.entries.delete(blobId);
    throw error;
  });
  if (cache?.entries) cache.entries.set(blobId, entry);
  return entry.promise;
}

function trimDocumentBlobByteCache(cache) {
  while (cache.entries.size > cache.maxEntries || cache.totalBytes > cache.maxBytes) {
    const oldestResolved = Array.from(cache.entries).find(([, entry]) => entry.bytes);
    if (!oldestResolved) return;
    const [blobId, entry] = oldestResolved;
    cache.entries.delete(blobId);
    cache.totalBytes = Math.max(0, cache.totalBytes - entry.size);
  }
}

function cancelMailMergeBlobPrefetch(state) {
  state.blobPrefetchController?.abort?.();
  state.blobPrefetchController = null;
  state.blobPrefetchTask = null;
}

function scheduleMailMergeBlobPrefetch(state) {
  const record = selectedRecord(state);
  const navigation = state.mailMergeNavigation;
  const blobIds = Array.from(new Set((navigation?.entries || [])
    .filter((entry) => entry.documentId === record?.id && entry.blobId)
    .map((entry) => entry.blobId)))
    .filter((blobId) => blobId !== state.selectedVersion?.blob_id);
  cancelMailMergeBlobPrefetch(state);
  if (!record?.id || !blobIds.length || state.disposed) return null;
  const cache = bindDocumentBlobByteCache(state, record.id);
  const controller = new AbortController();
  state.blobPrefetchController = controller;
  const task = (async () => {
    for (const blobId of blobIds) {
      await waitForDocumentBlobPrefetchSlot(controller.signal);
      if (controller.signal.aborted || state.disposed || state.selectedId !== record.id) return;
      await loadBlobBytes(state.ctx, blobId, cache).catch((error) => {
        if (!controller.signal.aborted) {
          console.warn('[documents] mail merge recipient blob prefetch failed', error);
        }
      });
    }
  })().finally(() => {
    if (state.blobPrefetchTask === task) {
      state.blobPrefetchController = null;
      state.blobPrefetchTask = null;
    }
  });
  state.blobPrefetchTask = task;
  return task;
}

function waitForDocumentBlobPrefetchSlot(signal) {
  if (signal?.aborted) return Promise.resolve();
  return new Promise((resolve) => {
    let settled = false;
    const finish = () => {
      if (settled) return;
      settled = true;
      signal?.removeEventListener?.('abort', finish);
      resolve();
    };
    signal?.addEventListener?.('abort', finish, { once: true });
    if (typeof globalThis.requestIdleCallback === 'function') {
      globalThis.requestIdleCallback(finish, { timeout: 750 });
    } else {
      setTimeout(finish, 50);
    }
  });
}

function requireDocumentPersistence(ctx) {
  if (!documentCollection(ctx, 'documents')
    || !documentCollection(ctx, 'document_versions')
    || !documentCollection(ctx, 'document_blob_chunks')) {
    throw new Error('CTOX document persistence is unavailable. Document bytes must be stored through CTOX collections, not local files.');
  }
}

async function ensureSeedRunbooks(ctx) {
  const collection = documentCollection(ctx, 'document_runbooks');
  if (!collection) return;
  const existing = await collection.find().exec();
  const now = Date.now();
  const existingIds = new Set(existing.map((doc) => doc.toJSON().id));
  const runbooks = mergeDocumentRunbooks([
    { id: 'document.summarize', document_type: 'word_document', title: 'Zusammenfassen', command_type: 'document.summarize', prompt_template: 'Fasse das ausgewählte DOCX strukturiert zusammen.' },
    { id: 'document.extract-requirements', document_type: 'word_document', title: 'Requirements extrahieren', command_type: 'document.extract-requirements', prompt_template: 'Extrahiere Anforderungen, offene Punkte und Nachweise.' },
    { id: 'document.review-risks', document_type: 'word_document', title: 'Risiken prüfen', command_type: 'document.review-risks', prompt_template: 'Prüfe fachliche, rechtliche und Umsetzungsrisiken im Dokument.' },
  ]);
  for (const runbook of runbooks) {
    if (existingIds.has(runbook.id)) continue;
    await collection.insert({
      ...runbook,
      description: runbook.description || runbook.prompt_template || '',
      created_at_ms: now,
      updated_at_ms: now,
    });
  }
}

function mergeDocumentRunbooks(runbooks = []) {
  const byId = new Map();
  [...SYSTEMATIC_REPORT_RUNBOOKS, ...runbooks].forEach((runbook) => {
    const id = runbook.id || runbook.command_type;
    if (!id) return;
    byId.set(id, {
      ...runbook,
      id,
      document_type: runbook.document_type || 'word_document',
      title: runbook.title || id,
      command_type: runbook.command_type || id,
      prompt_template: runbook.prompt_template || runbook.description || '',
    });
  });
  return Array.from(byId.values()).sort((left, right) => {
    const leftReport = String(left.id || '').startsWith('research.report.');
    const rightReport = String(right.id || '').startsWith('research.report.');
    if (left.id === 'research.report.auto') return -1;
    if (right.id === 'research.report.auto') return 1;
    if (leftReport !== rightReport) return leftReport ? -1 : 1;
    return String(left.title || '').localeCompare(String(right.title || ''), 'de');
  });
}

function selectedRecord(state) {
  return state.documents.find((record) => record.id === state.selectedId) || null;
}

function selectedRecordInGroup(state, group) {
  return group.records.find((record) => record.id === state.selectedId) || group.records[0] || null;
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
  host.innerHTML = `<div class="ctox-empty documents-error"><strong>${escapeHtml(state.t('documentError', 'Dokumentfehler'))}</strong><span>${escapeHtml(message)}</span></div>`;
}

function isSupportedDocumentFile(file) {
  return isDocxFile(file) || isMarkdownFile(file) || isTextFile(file);
}

function isDocxDocumentRecord(record = {}) {
  return ['word_document', 'mail_merge', 'series_letter'].includes(record.document_type)
    || record.mime_type === DOCX_MIME
    || /\.docx$/i.test(String(record.filename || ''));
}

function isDocxFile(file) {
  return /\.docx$/i.test(file.name) || file.type === DOCX_MIME;
}

function isMarkdownFile(file) {
  const hasMarkdownExtension = /\.(md|markdown)$/i.test(file.name);
  return file.type === MARKDOWN_MIME || (hasMarkdownExtension && (!file.type || file.type === 'text/plain'));
}

function isTextFile(file) {
  return /\.txt$/i.test(file.name) && (!file.type || file.type === 'text/plain');
}

function isMarkdownFilename(filename) {
  return /\.(md|markdown)$/i.test(String(filename || ''));
}

// Module-only glyphs with no shared/icons.js equivalent, drawn in the same
// stroke style as actionIconPaths (fill: none, currentColor, 1.8 stroke).
const DOCUMENTS_LOCAL_ICON_PATHS = Object.freeze({
  knowledge: 'M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2ZM4 19.5A2.5 2.5 0 0 1 6.5 17H20',
});

// Standard action glyphs (shared/icons.js actionIconPaths) — used only when
// the module runs without ctx.getActionIcon; the normal path is the shell
// helper handed in through mount(ctx).
const DOCUMENTS_FALLBACK_ACTION_ICON_PATHS = Object.freeze({
  add: 'M12 5v14M5 12h14',
  upload: 'M12 15V4M12 4 8 8M12 4l4 4M5 19h14',
  export: 'M12 3v11M12 3 8 7M12 3l4 4M5 12v7h14v-7',
  close: 'M6 6l12 12M18 6L6 18',
  filter: 'M4 6h16M7 12h10M10 18h4',
  copy: 'M9 9h10v11H9zM5 15V4h10',
  folder: 'M4 6h5l2 2h9v11H4V6Z',
  chevronLeft: 'M15 6l-6 6 6 6',
  chevronRight: 'M9 6l6 6-6 6',
  settings: 'M12 8.5a3.5 3.5 0 1 1 0 7 3.5 3.5 0 0 1 0-7ZM12 3v2.2M12 18.8V21M21 12h-2.2M5.2 12H3M18.4 5.6l-1.6 1.6M7.2 16.8l-1.6 1.6M18.4 18.4l-1.6-1.6M7.2 7.2 5.6 5.6',
  more: 'M6 12h.01M12 12h.01M18 12h.01',
});

function strokeIconSvg(name, path, size = 16, strokeWidth = 1.8) {
  return `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="${strokeWidth}" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" class="ctox-action-icon ctox-action-${name}"><path d="${path}"></path></svg>`;
}

function actionIcon(state, name, size = 16, strokeWidth = 1.8) {
  if (DOCUMENTS_LOCAL_ICON_PATHS[name]) {
    return strokeIconSvg(name, DOCUMENTS_LOCAL_ICON_PATHS[name], size, strokeWidth);
  }
  const fromCtx = state?.ctx?.getActionIcon?.(name, size, strokeWidth);
  if (typeof fromCtx === 'string' && fromCtx) return fromCtx;
  return strokeIconSvg(name, DOCUMENTS_FALLBACK_ACTION_ICON_PATHS[name] || DOCUMENTS_FALLBACK_ACTION_ICON_PATHS.more, size, strokeWidth);
}

function titleFromFilename(filename) {
  return filename.replace(/\.(docx|md|markdown|txt)$/i, '').trim() || 'Untitled document';
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
  link.href = revisionedModuleAssetUrl('./index.css').href;
  link.dataset.documentsStyle = 'true';
  document.head.append(link);
}

function revisionedModuleAssetUrl(relativePath) {
  const url = new URL(relativePath, import.meta.url);
  url.searchParams.set('v', DOCUMENTS_ASSET_REVISION);
  return url;
}

function ensureSuperDocStyles() {
  if (document.querySelector('link[data-superdoc-style]')) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = new URL('../../vendor/superdoc.css', import.meta.url).href;
  link.dataset.superdocStyle = 'true';
  document.head.append(link);
}

export const __documentsTestHooks = {
  loadSelectedVersion,
  markCtoxDocumentsDraft,
  wireLocalRealtime,
  refreshDocumentsFromLocal,
  refreshKnowledge,
  withDocumentVersionTimeout,
  awaitDocumentVersionReplication,
  documentKnowledgeLink,
  documentBySourceSha,
  isDocumentKnowledgeStale,
  isActiveDocumentRecord,
  knowledgeCandidates,
  mergeKnowledgeTableReferences,
  normalizeDocumentRecord,
  normalizeKnowledgeRecord,
  resolveKnowledgeContext,
  groupDocumentRecords,
  mailMergeBundleDescriptor,
  refreshMailMergeNavigation,
  findMailMergeRecipientIndex,
  resolveMailMergeRecipientSelection,
  switchSelectedDocument,
  canReplaceActiveEditorVersion,
  replaceActiveEditorVersion,
  waitForActiveEditorVersionContent,
  isEditorVersionContentReady,
  syncMailMergeRecipientSearch,
  createDocumentBlobByteCache,
  bindDocumentBlobByteCache,
  clearDocumentBlobByteCache,
  loadBlobBytes,
  documentSourceIdentity,
  documentFilterCount,
  isDocxDocumentRecord,
  knowledgeContextInstruction,
  validateImportInput,
  validateNewDocumentInput,
  visibleDocuments,
  isReclaimableDraftBlob,
  officeEngineFromSettings,
  ctoxDocumentsPermissions,
  saveBlobChunks,
  documentIdFromLaunchArgs,
  versionIdFromLaunchArgs,
  editorRenderKey,
  currentEditorTarget,
  isCurrentEditorRender,
  trackEditorMount,
  destroyActiveEditor,
  initializeOfficeEditorWithRecovery,
  isTransientOfficeStartupError,
  resolveLocalFirst,
  openOfficeEditorInstance,
};

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}
