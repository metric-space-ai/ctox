import * as Lexical from '../../vendor/lexical.mjs';
import { loadModuleMessages } from '../../shared/i18n.js';

const ElementNode = Object.getPrototypeOf(Lexical.HeadingNode);

class CustomHTMLNode extends ElementNode {
  static getType() {
    return 'custom-html';
  }

  static clone(node) {
    return new CustomHTMLNode(node.__html, node.__tagName, node.__className, node.__key);
  }

  constructor(html = '', tagName = 'div', className = '', key) {
    super(key);
    this.__html = html;
    this.__tagName = tagName;
    this.__className = className;
  }

  createDOM(config) {
    const dom = document.createElement(this.__tagName);
    if (this.__className) dom.className = this.__className;
    dom.innerHTML = this.__html;
    return dom;
  }

  updateDOM(prevNode, dom) {
    if (prevNode.__html !== this.__html || prevNode.__tagName !== this.__tagName || prevNode.__className !== this.__className) {
      if (this.__className) dom.className = this.__className;
      else dom.removeAttribute('class');
      dom.innerHTML = this.__html;
    }
    return false;
  }

  exportDOM() {
    const element = document.createElement(this.__tagName);
    if (this.__className) element.className = this.__className;
    element.innerHTML = this.__html;
    return { element };
  }

  static importDOM() {
    return {
      div: (domNode) => {
        if (domNode.classList.contains('notes-todo-row') || domNode.classList.contains('callout')) {
          return {
            conversion: (node) => ({
              node: new CustomHTMLNode(node.innerHTML, node.tagName.toLowerCase(), node.className)
            }),
            priority: 1
          };
        }
        return null;
      },
      table: (domNode) => {
        if (domNode.classList.contains('notes-table')) {
          return {
            conversion: (node) => ({
              node: new CustomHTMLNode(node.innerHTML, 'table', node.className)
            }),
            priority: 1
          };
        }
        return null;
      },
      pre: (domNode) => {
        if (domNode.classList.contains('nn-code-block')) {
          return {
            conversion: (node) => ({
              node: new CustomHTMLNode(node.innerHTML, 'pre', node.className)
            }),
            priority: 1
          };
        }
        return null;
      }
    };
  }

  exportJSON() {
    return {
      type: 'custom-html',
      version: 1,
      html: this.__html,
      tagName: this.__tagName,
      className: this.__className,
    };
  }

  static importJSON(serializedNode) {
    return new CustomHTMLNode(
      serializedNode.html,
      serializedNode.tagName,
      serializedNode.className
    );
  }
}

const SAVE_DEBOUNCE_MS = 250;
const NOTES_RENDER_DEBOUNCE_MS = 50;
const DRAFT_NOTE_MARKER = '__notes_draft';

const labels = {
  de: {
    allNotes: 'Alle Notizen',
    notes: 'Notizen',
    newNote: 'Neue Notiz',
    untitled: 'Unbenannte Notiz',
    search: 'Durchsuche Notizen...',
    saving: 'Speichern…',
    saved: 'Gespeichert',
    words: 'Wörter',
    chars: 'Zeichen',
    deleteConfirm: 'Möchtest du diese Notiz wirklich löschen?',
    deleteToTrash: 'Notiz in den Papierkorb verschieben?',
    draftStatus: 'Entwurf - noch nicht gespeichert',
    draftSaved: 'Entwurf gespeichert',
    draftDiscarded: 'Entwurf verworfen',
    noSearchResults: 'Keine passenden Notizen',
    dataUnavailable: 'Notizen sind gerade nicht verfügbar.',
    dataUnavailableHint: 'Die Liste wird automatisch gefüllt, sobald Notizen geladen sind.',
    usingLocalCache: 'Notizen werden aus dem letzten verfügbaren Stand angezeigt.',
    seededNotes: 'Beispielnotizen wurden lokal angelegt.',
    newNotebookPrompt: 'Name für das neue Notizbuch:',
    newTagPrompt: 'Name für den neuen Tag:',
    defaultFolder: 'Notizen',
    noNotes: 'Keine Notizen vorhanden',
    readTime: 'Min. Lesezeit',
  },
  en: {
    allNotes: 'All Notes',
    notes: 'Notes',
    newNote: 'New Note',
    untitled: 'Untitled Note',
    search: 'Search notes...',
    saving: 'Saving…',
    saved: 'Saved',
    words: 'words',
    chars: 'characters',
    deleteConfirm: 'Are you sure you want to delete this note?',
    deleteToTrash: 'Move this note to trash?',
    draftStatus: 'Draft - not saved yet',
    draftSaved: 'Draft saved',
    draftDiscarded: 'Draft discarded',
    noSearchResults: 'No matching notes',
    dataUnavailable: 'Notes are unavailable right now.',
    dataUnavailableHint: 'The list fills automatically once notes are loaded.',
    usingLocalCache: 'Showing the last available notes.',
    seededNotes: 'Sample notes were created locally.',
    newNotebookPrompt: 'Name for the new notebook:',
    newTagPrompt: 'Name for the new tag:',
    defaultFolder: 'Notes',
    noNotes: 'No notes available',
    readTime: 'min read',
  }
};

const state = {
  ctx: null,
  lang: 'de',
  notes: [],
  notebooks: [],
  tags: [],
  activeCategory: 'notes', // 'notes', 'favorites', 'trash'
  activeNotebook: '',
  activeTag: '',
  activeNoteId: '',
  searchQuery: '',
  sortMode: 'updated', // 'updated', 'created', 'title'
  viewMode: 'list', // 'list', 'compact'
  appLocked: false,
  pinBuffer: '',
  activeNoteDecrypted: {}, // noteId -> passcode (string)
  activeNoteDecryptedContent: {}, // noteId -> decrypted plainText content (string)
  saveTimer: null,
  localSubscriptionCleanup: null,
  renderTimer: null,
  t: (key, fallback) => fallback ?? key,
  contextMenu: null,
  contextMenuCleanup: null,
  dataDiagnostics: { kind: 'starting', message: '' },
  toastTimer: null,
  lexicalEditor: null,
  lexicalRichTextCleanup: null,
  lexicalUpdateListenerCleanup: null,
  lastLexicalHtml: '',
  hydratingEditor: false,
  renderedNoteId: '',
};

const els = {};

function getCachePrefix() {
  return state.ctx?.module?.id === 'notizen' ? 'ctox.notizen' : 'ctox.notes';
}

function createDefaultNotes(now = Date.now()) {
  return [
    {
      id: 'notes_seed_ops_review',
      title: 'Operations Review',
      content: '<h1>Operations Review</h1><p>Prioritaeten fuer diese Woche pruefen und offene Entscheidungen im Dashboard nachhalten.</p>',
      folder: 'Notes',
      notebook: 'Operations',
      tags: 'review,team',
      is_favorite: true,
      is_trashed: false,
      is_locked: false,
      lock_passcode: '',
      updated_at_ms: now - 1000 * 60 * 18
    },
    {
      id: 'notes_seed_product_notes',
      title: 'Produktnotizen',
      content: '<h1>Produktnotizen</h1><p>Editoraktionen, Favoriten und Papierkorb vor dem naechsten QA-Lauf validieren.</p>',
      folder: 'Notes',
      notebook: 'Produkt',
      tags: 'qa,notizen',
      is_favorite: false,
      is_trashed: false,
      is_locked: false,
      lock_passcode: '',
      updated_at_ms: now - 1000 * 60 * 60 * 4
    },
    {
      id: 'notes_seed_meeting_followup',
      title: 'Meeting Follow-up',
      content: '<h1>Meeting Follow-up</h1><p>Naechste Schritte, Verantwortliche und offene Risiken fuer den Kundenworkshop sammeln.</p>',
      folder: 'Notes',
      notebook: 'Kunden',
      tags: 'meeting',
      is_favorite: false,
      is_trashed: false,
      is_locked: false,
      lock_passcode: '',
      updated_at_ms: now - 1000 * 60 * 60 * 24
    }
  ];
}

function getCollection() {
  const db = state.ctx?.db;
  return db?.collection?.('notes') || null;
}

function isBusinessOsPermissionDenied(error) {
  return error?.code === 'CTOX_BUSINESS_OS_PERMISSION_DENIED'
    || error?.name === 'BusinessOsPermissionError';
}

function canWriteNotesCollection() {
  const permissionCheck = state.ctx?.permissions?.canWriteCollection;
  return typeof permissionCheck === 'function' ? permissionCheck('notes') : true;
}

export async function mount(ctx) {
  state.ctx = ctx;
  state.lang = ctx.locale === 'en' ? 'en' : 'de';
  
  // Inject stylesheet dynamically
  const styleLink = document.createElement('link');
  styleLink.rel = 'stylesheet';
  styleLink.href = new URL('./index.css', import.meta.url).href;
  styleLink.id = state.ctx.module?.id === 'notizen' ? 'notizen-module-styles' : 'notes-module-styles';
  document.head.appendChild(styleLink);
  
  // Load dynamic locales
  const messages = await loadModuleMessages(import.meta.url, ctx.locale, labels);
  state.t = (key, fallback) => messages[key] ?? fallback ?? key;
  
  // Set up HTML structure
  ctx.host.innerHTML = await loadModuleMarkup();
  ctx.left.replaceChildren();
  ctx.right.replaceChildren();
  
  // Apply translation to static labels in mounted DOM
  applyStaticLabels(ctx.host, state.t);
  
  bindElements(ctx.host);
  normalizeInteractiveLabels(ctx.host);
  
  // Initialize Lexical Editor
  state.lexicalEditor = Lexical.createEditor({
    theme: {
      paragraph: 'notes-paragraph',
      heading: {
        h1: 'notes-h1',
        h2: 'notes-h2',
        h3: 'notes-h3',
      },
      list: {
        ul: 'notes-ul',
        ol: 'notes-ol',
        listitem: 'notes-li',
      },
      text: {
        bold: 'notes-bold',
        italic: 'notes-italic',
        underline: 'notes-underline',
        strikethrough: 'notes-strikethrough',
        code: 'notes-code',
      }
    },
    nodes: [
      Lexical.HeadingNode,
      Lexical.QuoteNode,
      Lexical.ListNode,
      Lexical.ListItemNode,
      Lexical.LinkNode,
      Lexical.AutoLinkNode,
      Lexical.CodeNode,
      CustomHTMLNode
    ]
  });

  if (els.editor) {
    state.lexicalEditor.setRootElement(els.editor);
  }
  state.lexicalRichTextCleanup = Lexical.registerRichText?.(state.lexicalEditor) || null;

  state.lexicalUpdateListenerCleanup = state.lexicalEditor.registerUpdateListener(({ editorState }) => {
    if (state.hydratingEditor) return;
    editorState.read(() => {
      const html = Lexical.$generateHtmlFromNodes(state.lexicalEditor);
      if (html !== state.lastLexicalHtml) {
        state.lastLexicalHtml = html;
        processContentInput(html);
      }
    });
  });

  wireEvents();
  
  // Setup column resizing logic
  const resizerCleanup = setupResizers(ctx.host);
  
  // Setup App Lock on startup if cached as locked
  const cachePrefix = state.ctx.module?.id === 'notizen' ? 'ctox.notizen' : 'ctox.notes';
  if (localStorage.getItem(`${cachePrefix}.appLocked`) === 'true') {
    state.appLocked = true;
    if (els.clientLockScreen) {
      els.clientLockScreen.removeAttribute('hidden');
    }
  }
  
  // Load notes and wire up database merge sync
  await loadNotesFromLocal();
  state.localSubscriptionCleanup = wireLocalRealtime();
  state.contextMenuCleanup = initNotesContextMenu(state);
  
  // Pre-select first note if available
  // Presence (advisory UX): show who else has the same note open, and
  // publish which note this user is editing. Cleared on unmount.
  state.presenceRemote = [];
  state.presenceCleanup = null;
  if (ctx.presence?.subscribe) {
    state.presenceCleanup = ctx.presence.subscribe((entries) => {
      state.presenceRemote = Array.isArray(entries) ? entries : [];
      scheduleRender();
    });
  }

  const initialNotes = getFilteredNotes();
  if (initialNotes.length > 0) {
    selectNote(initialNotes[0].id);
  } else {
    renderEditor();
  }

  return () => {
    state.presenceCleanup?.();
    state.presenceCleanup = null;
    try { state.ctx?.presence?.clear?.(); } catch {}
    if (state.saveTimer) clearTimeout(state.saveTimer);
    if (state.renderTimer) clearTimeout(state.renderTimer);
    if (state.toastTimer) clearTimeout(state.toastTimer);
    
    state.lexicalUpdateListenerCleanup?.();
    state.lexicalUpdateListenerCleanup = null;
    state.lexicalRichTextCleanup?.();
    state.lexicalRichTextCleanup = null;
    state.lexicalEditor?.setRootElement(null);
    state.lexicalEditor = null;
    state.lastLexicalHtml = '';
    state.renderedNoteId = '';

    state.localSubscriptionCleanup?.();
    state.localSubscriptionCleanup = null;
    
    state.contextMenuCleanup?.();
    state.contextMenu?.remove();
    state.contextMenu = null;
    state.ctx.host?.querySelector('.nn-action-toast')?.remove();
    
    resizerCleanup();
    unbindEvents();
    
    // Clean up stylesheet
    document.getElementById(state.ctx.module?.id === 'notizen' ? 'notizen-module-styles' : 'notes-module-styles')?.remove();
  };
}

function applyStaticLabels(root, t) {
  root.querySelectorAll('[data-t]').forEach(el => el.textContent = t(el.dataset.t));
  root.querySelectorAll('[data-t-title]').forEach(el => el.title = t(el.dataset.tTitle));
  root.querySelectorAll('[data-t-aria]').forEach(el => el.setAttribute('aria-label', t(el.dataset.tAria)));
  root.querySelectorAll('[data-t-placeholder]').forEach(el => el.placeholder = t(el.dataset.tPlaceholder));
}

async function loadModuleMarkup() {
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}

function bindElements(host) {
  els.root = host.querySelector('[data-notes-root]');
  
  // Keypads & Zero-Knowledge client Lock Screens
  els.clientLockScreen = host.querySelector('[data-client-lock-screen]');
  els.pinDots = host.querySelectorAll('.nn-pin-dot');
  els.pinPad = host.querySelector('.nn-pin-pad');
  els.lockAppBtn = host.querySelector('[data-action="lock-app"]');
  
  // Sidebar Panes & list components
  els.folderList = host.querySelector('.nn-nav-list');
  els.notebooksList = host.querySelector('[data-notebooks-list]');
  els.tagsList = host.querySelector('[data-tags-list]');
  
  // List Pane
  els.notesList = host.querySelector('[data-notes-list]');
  els.notesCountLabel = host.querySelector('[data-notes-count-label]');
  els.listKicker = host.querySelector('[data-list-kicker]');
  els.search = host.querySelector('[data-search]');
  els.filterTrigger = host.querySelector('[data-action="toggle-filter"]');
  els.filterPopover = host.querySelector('[data-filter-popover]');
  
  // Editor Meta Controls
  els.notebookSelectBtn = host.querySelector('.nn-notebook-select-btn');
  els.noteNotebookLabel = host.querySelector('[data-note-notebook-label]');
  els.notebookDropdown = host.querySelector('[data-notebook-dropdown]');
  
  els.tagsSelectBtn = host.querySelector('.nn-tags-select-btn');
  els.tagsDropdown = host.querySelector('[data-tags-dropdown]');
  
  els.starBtn = host.querySelector('[data-action="star-note"]');
  els.lockNoteBtn = host.querySelector('[data-action="lock-note"]');
  els.deleteBtn = host.querySelector('[data-action="delete-note"]');
  els.createNoteBtn = host.querySelector('[data-action="create-note"]');
  
  // Editor Lock Screen
  els.noteLockScreen = host.querySelector('[data-note-lock-screen]');
  els.notePasscodeInput = host.querySelector('[data-note-passcode-input]');
  els.decryptNoteBtn = host.querySelector('[data-action="decrypt-note"]');
  
  // Editor Paper Sheet
  els.paperSheet = host.querySelector('[data-paper-sheet-pane]');
  els.noteBadgesContainer = host.querySelector('[data-note-badges-container]');
  els.noteDate = host.querySelector('[data-note-date]');
  els.editor = host.querySelector('[data-notes-editor]');
  els.editorWorkspace = host.querySelector('.nn-editor-workspace');
  
  // Toolbar Inserters
  els.formatBtn = host.querySelector('[data-action="format-text"]');
  els.headersDropdown = host.querySelector('[data-headers-dropdown]');
  els.checklistBtn = host.querySelector('[data-action="insert-checklist"]');
  els.tableBtn = host.querySelector('[data-action="insert-table"]');
  els.codeblockBtn = host.querySelector('[data-action="insert-codeblock"]');
  els.formatCalloutsBtn = host.querySelector('[data-action="format-callouts"]');
  els.calloutsDropdown = host.querySelector('[data-callouts-dropdown]');
  els.timestampBtn = host.querySelector('[data-action="insert-timestamp"]');
  
  // Footer Stats
  els.status = host.querySelector('[data-notes-status]');
  els.readTime = host.querySelector('[data-notes-read-time]');
  els.words = host.querySelector('[data-notes-words]');
  els.chars = host.querySelector('[data-notes-chars]');
}

function wireEvents() {
  // App PIN lock pad
  els.pinPad?.addEventListener('click', handlePinPadClick);
  els.lockAppBtn?.addEventListener('click', handleLockAppClick);
  
  // Sidebar items togglers and creations
  els.root?.querySelectorAll('[data-toggle-nav]').forEach(el => {
    el.addEventListener('click', handleToggleNavClick);
  });
  els.folderList?.querySelectorAll('[data-nav-category]').forEach(el => {
    el.addEventListener('click', handleCategoryClick);
  });
  els.root?.querySelector('[data-action="create-notebook"]')?.addEventListener('click', handleCreateNotebookClick);
  els.root?.querySelector('[data-action="create-tag"]')?.addEventListener('click', handleCreateTagClick);
  
  // Search and Sort Filters
  els.search?.addEventListener('input', handleSearch);
  els.filterTrigger?.addEventListener('click', handleFilterTriggerClick);
  els.filterPopover?.querySelectorAll('[data-sort]').forEach(el => {
    el.addEventListener('click', handleSortClick);
  });
  els.filterPopover?.querySelectorAll('[data-view-mode]').forEach(el => {
    el.addEventListener('click', handleViewModeClick);
  });
  
  // Editor Meta controls clicks
  els.notebookSelectBtn?.addEventListener('click', handleNotebookSelectBtnClick);
  els.tagsSelectBtn?.addEventListener('click', handleTagsSelectBtnClick);
  
  els.starBtn?.addEventListener('click', handleStarNoteClick);
  els.lockNoteBtn?.addEventListener('click', handleLockNoteClick);
  els.deleteBtn?.addEventListener('click', handleDeleteNote);
  els.createNoteBtn?.addEventListener('click', handleCreateNote);
  
  // Decrypt locked notes click/keypress handlers
  els.decryptNoteBtn?.addEventListener('click', handleDecryptNoteClick);
  els.notePasscodeInput?.addEventListener('keydown', handleDecryptNoteKeydown);
  
  // Editor core formatting actions
  els.editor?.addEventListener('input', handleEditorInput);
  els.editor?.addEventListener('keydown', handleEditorKeydown);
  els.editor?.addEventListener('click', handleEditorClick);
  
  els.formatBtn?.addEventListener('click', handleFormatBtnClick);
  state.ctx.host?.querySelectorAll('[data-format-cmd]').forEach(btn => {
    btn.addEventListener('click', handleFormatCommandClick);
  });
  
  els.checklistBtn?.addEventListener('click', handleChecklistBtnClick);
  els.tableBtn?.addEventListener('click', handleTableBtnClick);
  els.codeblockBtn?.addEventListener('click', handleCodeblockBtnClick);
  
  els.formatCalloutsBtn?.addEventListener('click', handleFormatCalloutsClick);
  els.calloutsDropdown?.querySelectorAll('[data-callout-type]').forEach(btn => {
    btn.addEventListener('click', handleCalloutCommandClick);
  });
  
  els.timestampBtn?.addEventListener('click', handleTimestampBtnClick);
  
  // Global closes and custom circular checklist toggles
  document.addEventListener('click', handleGlobalClick);
  document.addEventListener('keydown', handleDocumentKeydown);
  els.folderList?.addEventListener('keydown', handleSidebarKeydown);
  els.editor?.addEventListener('click', handleEditorCheckboxClick);
}

function unbindEvents() {
  els.pinPad?.removeEventListener('click', handlePinPadClick);
  els.lockAppBtn?.removeEventListener('click', handleLockAppClick);
  els.folderList?.querySelectorAll('[data-nav-category]').forEach(el => {
    el.removeEventListener('click', handleCategoryClick);
  });
  els.search?.removeEventListener('input', handleSearch);
  els.filterTrigger?.removeEventListener('click', handleFilterTriggerClick);
  els.notebookSelectBtn?.removeEventListener('click', handleNotebookSelectBtnClick);
  els.tagsSelectBtn?.removeEventListener('click', handleTagsSelectBtnClick);
  els.starBtn?.removeEventListener('click', handleStarNoteClick);
  els.lockNoteBtn?.removeEventListener('click', handleLockNoteClick);
  els.deleteBtn?.removeEventListener('click', handleDeleteNote);
  els.createNoteBtn?.removeEventListener('click', handleCreateNote);
  els.decryptNoteBtn?.removeEventListener('click', handleDecryptNoteClick);
  els.notePasscodeInput?.removeEventListener('keydown', handleDecryptNoteKeydown);
  els.editor?.removeEventListener('input', handleEditorInput);
  els.editor?.removeEventListener('keydown', handleEditorKeydown);
  els.editor?.removeEventListener('click', handleEditorClick);
  els.formatBtn?.removeEventListener('click', handleFormatBtnClick);
  els.checklistBtn?.removeEventListener('click', handleChecklistBtnClick);
  els.tableBtn?.removeEventListener('click', handleTableBtnClick);
  els.codeblockBtn?.removeEventListener('click', handleCodeblockBtnClick);
  els.formatCalloutsBtn?.removeEventListener('click', handleFormatCalloutsClick);
  els.timestampBtn?.removeEventListener('click', handleTimestampBtnClick);
  
  document.removeEventListener('click', handleGlobalClick);
  document.removeEventListener('keydown', handleDocumentKeydown);
  els.folderList?.removeEventListener('keydown', handleSidebarKeydown);
  els.editor?.removeEventListener('click', handleEditorCheckboxClick);
}

function normalizeInteractiveLabels(root) {
  root.querySelectorAll('button[title]:not([aria-label])').forEach((button) => {
    button.setAttribute('aria-label', button.getAttribute('title'));
  });
  root.querySelectorAll('.notes-folder-item, [data-toggle-nav]').forEach((item) => {
    item.setAttribute('role', 'button');
    if (!item.hasAttribute('tabindex')) item.setAttribute('tabindex', '0');
  });
  if (els.search && !els.search.getAttribute('aria-label')) {
    els.search.setAttribute('aria-label', state.t('search', 'Durchsuche Notizen...'));
  }
}

// Local Cache Persistence
function saveToLocalCache() {
  const cachePrefix = getCachePrefix();
  localStorage.setItem(`${cachePrefix}.local_records`, JSON.stringify(state.notes.filter(note => !note[DRAFT_NOTE_MARKER])));
  localStorage.setItem(`${cachePrefix}.local_notebooks`, JSON.stringify(state.notebooks));
  localStorage.setItem(`${cachePrefix}.local_tags`, JSON.stringify(state.tags));
}

async function decryptLockedNotesInMemory() {
  for (const note of state.notes) {
    if (note.is_locked) {
      const passcode = state.activeNoteDecrypted[note.id];
      if (passcode) {
        try {
          const encryptedData = JSON.parse(note.content);
          const decrypted = await decryptContent(encryptedData, passcode);
          state.activeNoteDecryptedContent[note.id] = decrypted;
        } catch (e) {
          console.warn('Failed to decrypt locked note on load', e);
          delete state.activeNoteDecrypted[note.id];
          delete state.activeNoteDecryptedContent[note.id];
        }
      }
    }
  }
}

async function loadNotesFromLocal() {
  const collection = getCollection();
  const logPrefix = state.ctx.module?.id === 'notizen' ? '[notizen]' : '[notes]';
  if (!collection) {
    state.notes = [];
    state.notebooks = [];
    state.tags = [];
    state.dataDiagnostics = { kind: 'missing', message: state.t('dataUnavailable') };
    syncNotebooksAndTags();
    scheduleRender();
    return;
  }
  
  try {
    const docs = await collection.find().exec();
    const serverNotes = docs.map(d => d.toJSON()).filter(n => n.id);

    if (serverNotes.length === 0) {
      if (canWriteNotesCollection() && localStorage.getItem(`${getCachePrefix()}.defaultSeeded`) !== 'true') {
        const seedNotes = createDefaultNotes();
        state.notes = seedNotes.sort((a, b) => b.updated_at_ms - a.updated_at_ms);
        state.dataDiagnostics = { kind: 'seeded', message: state.t('seededNotes') };
        let seededCount = 0;
        for (const note of seedNotes) {
          try {
            await collection.insert(note);
            seededCount += 1;
          } catch (insertError) {
            console.warn(`${logPrefix} failed to seed sample note`, insertError);
          }
        }
        if (seededCount > 0) {
          localStorage.setItem(`${getCachePrefix()}.defaultSeeded`, 'true');
        }
      } else {
        state.notes = [];
        state.dataDiagnostics = { kind: 'ok-empty', message: '' };
      }
    } else {
      state.notes = serverNotes.sort((a, b) => b.updated_at_ms - a.updated_at_ms);
      state.dataDiagnostics = { kind: 'ok', message: '' };
    }
    
    await decryptLockedNotesInMemory();
    
    syncNotebooksAndTags();
    saveToLocalCache(); // Passive write-only mirror purely for Playwright E2E tests to read
    scheduleRender();
  } catch (error) {
    if (isBusinessOsPermissionDenied(error)) {
      console.log(`${logPrefix} data access locked`, error?.message || error);
    } else {
      console.error(`${logPrefix} failed to load notes`, error);
    }
    state.notes = [];
    state.notebooks = [];
    state.tags = [];
    state.dataDiagnostics = {
      kind: isBusinessOsPermissionDenied(error) ? 'missing' : 'error',
      message: error?.message || state.t('dataUnavailable'),
    };
    syncNotebooksAndTags();
    scheduleRender();
  }
}
function syncNotebooksAndTags() {
  const scannedNotebooks = new Set();
  const scannedTags = new Set();
  state.notes.forEach(note => {
    if (note.notebook) scannedNotebooks.add(note.notebook);
    if (note.tags) {
      (note.tags || '').split(',').map(t => t.trim()).filter(Boolean).forEach(tag => scannedTags.add(tag));
    }
  });
  
  // Restore empty placeholders from cached arrays
  state.notebooks.forEach(nb => scannedNotebooks.add(nb));
  state.tags.forEach(tg => scannedTags.add(tg));
  
  state.notebooks = Array.from(scannedNotebooks).sort();
  state.tags = Array.from(scannedTags).sort();
}

function wireLocalRealtime() {
  const collection = getCollection();
  if (!collection) return null;
  
  const logPrefix = state.ctx.module?.id === 'notizen' ? '[notizen]' : '[notes]';
  const sub = collection.$.subscribe(() => {
    loadNotesFromLocal().catch(err => {
      console.warn(`${logPrefix} failed to refresh notes`, err);
    });
  });
  
  return () => {
    sub?.unsubscribe();
  };
}

function scheduleRender() {
  if (state.renderTimer) return;
  state.renderTimer = setTimeout(() => {
    state.renderTimer = null;
    renderAll();
  }, NOTES_RENDER_DEBOUNCE_MS);
}

function renderAll() {
  renderSidebar();
  renderNotesList();
  renderEditor();
}

function renderSidebar() {
  // Bind category selection styles
  els.folderList?.querySelectorAll('[data-nav-category]').forEach(el => {
    const category = el.getAttribute('data-nav-category');
    const active = state.activeCategory === category && !state.activeNotebook && !state.activeTag;
    el.classList.toggle('active', active);
    
    // Set Count Badge
    const countEl = el.querySelector('.notes-folder-count');
    if (countEl) {
      if (category === 'notes') {
        countEl.textContent = state.notes.filter(n => !n.is_trashed).length;
      } else if (category === 'favorites') {
        countEl.textContent = state.notes.filter(n => n.is_favorite && !n.is_trashed).length;
      } else if (category === 'trash') {
        countEl.textContent = state.notes.filter(n => n.is_trashed).length;
      }
    }
  });
  
  // Render Notebooks sublist
  if (els.notebooksList) {
    let html = '';
    state.notebooks.forEach(nb => {
      const active = state.activeNotebook === nb && !state.activeCategory && !state.activeTag;
      const count = state.notes.filter(n => n.notebook === nb && !n.is_trashed).length;
      html += `
        <div class="notes-folder-item ${active ? 'active' : ''}" data-nav-notebook="${escapeHtml(nb)}">
          <div class="notes-folder-item-left">
            <svg class="notes-folder-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"></path>
              <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z"></path>
            </svg>
            <span class="notes-folder-name">${escapeHtml(nb)}</span>
          </div>
          <span class="notes-folder-count">${count}</span>
        </div>
      `;
    });
    els.notebooksList.innerHTML = html;
    normalizeInteractiveLabels(els.notebooksList);
    
    els.notebooksList.querySelectorAll('[data-nav-notebook]').forEach(el => {
      el.addEventListener('click', () => {
        const nb = el.getAttribute('data-nav-notebook');
        state.activeCategory = '';
        state.activeTag = '';
        state.activeNotebook = nb;
        
        const filtered = getFilteredNotes();
        if (filtered.length > 0) selectNote(filtered[0].id);
        else selectNote('');
        scheduleRender();
      });
    });
  }
  
  // Render Tags sublist
  if (els.tagsList) {
    let html = '';
    state.tags.forEach(tg => {
      const active = state.activeTag === tg && !state.activeCategory && !state.activeNotebook;
      const count = state.notes.filter(n => {
        return (n.tags || '').split(',').map(x => x.trim()).includes(tg) && !n.is_trashed;
      }).length;
      html += `
        <div class="notes-folder-item ${active ? 'active' : ''}" data-nav-tag="${escapeHtml(tg)}">
          <div class="notes-folder-item-left">
            ${state.ctx?.getActionIcon?.('tag') || ''}
            <span class="notes-folder-name">${escapeHtml(tg)}</span>
          </div>
          <span class="notes-folder-count">${count}</span>
        </div>
      `;
    });
    els.tagsList.innerHTML = html;
    normalizeInteractiveLabels(els.tagsList);
    
    els.tagsList.querySelectorAll('[data-nav-tag]').forEach(el => {
      el.addEventListener('click', () => {
        const tg = el.getAttribute('data-nav-tag');
        state.activeCategory = '';
        state.activeNotebook = '';
        state.activeTag = tg;
        
        const filtered = getFilteredNotes();
        if (filtered.length > 0) selectNote(filtered[0].id);
        else selectNote('');
        scheduleRender();
      });
    });
  }
}

function getFilteredNotes({ includeSearch = true } = {}) {
  let list = state.notes.slice();
  
  if (state.activeCategory === 'favorites') {
    list = list.filter(n => n.is_favorite && !n.is_trashed);
  } else if (state.activeCategory === 'trash') {
    list = list.filter(n => n.is_trashed);
  } else if (state.activeCategory === 'notes') {
    list = list.filter(n => !n.is_trashed);
  } else if (state.activeNotebook) {
    list = list.filter(n => n.notebook === state.activeNotebook && !n.is_trashed);
  } else if (state.activeTag) {
    list = list.filter(n => (n.tags || '').split(',').map(x => x.trim()).includes(state.activeTag) && !n.is_trashed);
  }
  
  // Search query filter
  const query = state.searchQuery.toLowerCase().trim();
  if (includeSearch && query) {
    list = list.filter(n => {
      let textToSearch = n.content || '';
      if (n.is_locked) {
        textToSearch = state.activeNoteDecryptedContent[n.id] || '';
      }
      const titleMatch = (n.title || '').toLowerCase().includes(query);
      const textMatch = getPlainText(textToSearch).toLowerCase().includes(query);
      return titleMatch || textMatch;
    });
  }
  // Sort
  if (state.sortMode === 'title') {
    list.sort((a, b) => (a.title || '').localeCompare(b.title || ''));
  } else if (state.sortMode === 'created') {
    list.sort((a, b) => a.updated_at_ms - b.updated_at_ms);
  } else {
    list.sort((a, b) => b.updated_at_ms - a.updated_at_ms);
  }
  
  return list;
}

function buildNotesEmptyState({ totalNotes, scopedNotes, hasSearch, activeLabel, diagnostics, t }) {
  const translate = typeof t === 'function' ? t : (_key, fallback) => fallback;
  const diagnosticKind = diagnostics?.kind || 'ok';
  if (diagnosticKind === 'missing' || diagnosticKind === 'error') {
    return {
      kind: 'unavailable',
      title: translate('dataUnavailable', 'Notes are unavailable right now.'),
      body: translate('dataUnavailableHint', 'The list fills automatically once notes are loaded.')
    };
  }
  if (hasSearch) {
    return {
      kind: 'no-results',
      title: translate('noSearchResults', 'No matching notes'),
      body: translate('clearSearchHint', 'Suche loeschen oder Suchbegriff anpassen.')
    };
  }
  if (totalNotes > 0 && scopedNotes === 0) {
    return {
      kind: 'empty-scope',
      title: translate('noNotes', 'No notes'),
      body: activeLabel ? `${activeLabel}: ${translate('noNotes', 'No notes')}` : translate('noNotes', 'No notes')
    };
  }
  if (diagnosticKind === 'local-cache' || diagnosticKind === 'seeded') {
    return {
      kind: diagnosticKind,
      title: translate('noNotes', 'No notes'),
      body: diagnostics?.message || ''
    };
  }
  return {
    kind: 'empty',
    title: translate('noNotes', 'No notes'),
    body: ''
  };
}

function getActiveListLabel() {
  if (state.activeNotebook) return state.activeNotebook;
  if (state.activeTag) return `#${state.activeTag}`;
  if (state.activeCategory === 'favorites') return 'Favoriten';
  if (state.activeCategory === 'trash') return 'Papierkorb';
  return state.t('allNotes');
}

function renderNotesList() {
  if (!els.notesList) return;
  
  const list = getFilteredNotes();
  
  // Update header titles and list count label kicker
  if (els.listKicker) {
    if (state.activeNotebook) {
      els.listKicker.textContent = 'Notizbuch';
      els.notesCountLabel.textContent = state.activeNotebook;
    } else if (state.activeTag) {
      els.listKicker.textContent = 'Tag';
      els.notesCountLabel.textContent = '#' + state.activeTag;
    } else {
      els.listKicker.textContent = 'Notesnook';
      if (state.activeCategory === 'favorites') {
        els.notesCountLabel.textContent = 'Favoriten';
      } else if (state.activeCategory === 'trash') {
        els.notesCountLabel.textContent = 'Papierkorb';
      } else {
        els.notesCountLabel.textContent = state.t('allNotes');
      }
    }
  }
  
  if (list.length === 0) {
    const scopedNotes = getFilteredNotes({ includeSearch: false }).length;
    const emptyState = buildNotesEmptyState({
      totalNotes: state.notes.length,
      scopedNotes,
      hasSearch: !!state.searchQuery.trim(),
      activeLabel: getActiveListLabel(),
      diagnostics: state.dataDiagnostics,
      t: state.t
    });
    els.notesList.innerHTML = `
      <div class="ctox-empty nn-empty-state-${escapeHtml(emptyState.kind)}">
        <strong>${escapeHtml(emptyState.title)}</strong>
        ${emptyState.body ? `<span>${escapeHtml(emptyState.body)}</span>` : ''}
      </div>
    `;
    return;
  }

  // Segment into Buckets
  const todayBucket = [];
  const yesterdayBucket = [];
  const olderBucket = [];

  const todayDate = new Date();
  const yesterdayDate = new Date();
  yesterdayDate.setDate(yesterdayDate.getDate() - 1);

  const todayStr = todayDate.toDateString();
  const yesterdayStr = yesterdayDate.toDateString();

  list.forEach(note => {
    const d = new Date(note.updated_at_ms);
    const dStr = d.toDateString();
    if (dStr === todayStr) {
      todayBucket.push(note);
    } else if (dStr === yesterdayStr) {
      yesterdayBucket.push(note);
    } else {
      olderBucket.push(note);
    }
  });

  let html = '';

  function renderCard(note) {
    const active = note.id === state.activeNoteId;
    const dateStr = formatTimestamp(note.updated_at_ms);
    let contentForSnippet = note.content;
    if (note.is_locked) {
      contentForSnippet = state.activeNoteDecryptedContent[note.id] || '🔒 Verschlüsselte Notiz';
    }
    const snippet = extractSnippet(contentForSnippet, note.title);
    
    let badgesHtml = '';
    if (note.notebook) {
      badgesHtml += `<span class="nn-card-badge notebook-badge">📁 ${escapeHtml(note.notebook)}</span>`;
    }
    if (note.tags) {
      (note.tags || '').split(',').map(x => x.trim()).filter(Boolean).forEach(tg => {
        badgesHtml += `<span class="nn-card-badge tag-badge">#${escapeHtml(tg)}</span>`;
      });
    }
    
    return `
      <div class="notes-card ${active ? 'active' : ''}" data-note-id="${note.id}">
        <div class="nn-card-row">
          <div class="notes-card-title">${escapeHtml(note.title || state.t('untitled'))}</div>
          <div class="nn-card-icons">
            ${note.is_favorite ? '<svg class="nn-card-icon starred" viewBox="0 0 24 24" aria-hidden="true"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon></svg>' : ''}
            ${note.is_locked ? '<svg class="nn-card-icon locked" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"></rect><path d="M7 11V7a5 5 0 0 1 10 0v4"></path></svg>' : ''}
          </div>
        </div>
        <div class="notes-card-meta">
          <span class="notes-card-date">${dateStr}</span>
          <span class="notes-card-snippet">${escapeHtml(snippet)}</span>
        </div>
        ${badgesHtml ? `<div class="nn-card-badges">${badgesHtml}</div>` : ''}
      </div>
    `;
  }

  const lang = state.lang;
  if (todayBucket.length > 0) {
    html += `<div class="notes-group-header bo-notes-group-today bo-notes-group-today-notes">${lang === 'en' ? 'Today' : 'Heute'}</div>`;
    todayBucket.forEach(n => html += renderCard(n));
  }
  if (yesterdayBucket.length > 0) {
    html += `<div class="notes-group-header bo-notes-group-yesterday bo-notes-group-yesterday-notes">${lang === 'en' ? 'Yesterday' : 'Gestern'}</div>`;
    yesterdayBucket.forEach(n => html += renderCard(n));
  }
  if (olderBucket.length > 0) {
    html += `<div class="notes-group-header bo-notes-group-older bo-notes-group-older-notes">${lang === 'en' ? 'Older' : 'Ältere'}</div>`;
    olderBucket.forEach(n => html += renderCard(n));
  }

  els.notesList.innerHTML = html;
  
  // Bind note card clicks
  els.notesList.querySelectorAll('[data-note-id]').forEach(el => {
    el.setAttribute('role', 'button');
    el.setAttribute('tabindex', '0');
    el.setAttribute('aria-label', el.querySelector('.notes-card-title')?.textContent?.trim() || state.t('notes', 'Notizen'));
    el.addEventListener('click', () => {
      const id = el.getAttribute('data-note-id');
      selectNote(id);
    });
    el.addEventListener('keydown', (event) => {
      if (event.key !== 'Enter' && event.key !== ' ') return;
      event.preventDefault();
      const id = el.getAttribute('data-note-id');
      selectNote(id);
    });
  });
}

function renderEditor() {
  const note = state.notes.find(n => n.id === state.activeNoteId);

  // Publish presence for the open note (the editor is live, so "editing").
  // Only collection + record id go on the wire — never titles or content, so
  // locked notes stay non-revealing. The registry dedups unchanged sets.
  try {
    state.ctx?.presence?.set(note ? [{ collection: 'notes', recordId: note.id, mode: 'editing' }] : []);
  } catch {}

  // Clean up any dynamic restore banner
  els.editorWorkspace?.querySelector('.nn-restore-banner')?.remove();
  els.editorWorkspace?.querySelector('.nn-draft-banner')?.remove();
  
  updateActionAvailability(note);
  
  if (!note) {
    state.renderedNoteId = '';
    if (els.noteNotebookLabel) els.noteNotebookLabel.textContent = 'Kein Notizbuch';
    if (els.noteBadgesContainer) els.noteBadgesContainer.innerHTML = '';
    if (els.noteDate) els.noteDate.textContent = '';
    if (els.editor) els.editor.innerHTML = '';
    if (els.words) els.words.textContent = `0 ${state.t('words')}`;
    if (els.chars) els.chars.textContent = `0 ${state.t('chars')}`;
    els.starBtn?.classList.remove('active');
    els.lockNoteBtn?.classList.remove('active');
    els.noteLockScreen?.setAttribute('hidden', '');
    setToolbarDisabled(true, 'Keine Notiz ausgewählt');
    return;
  }
  
  // Update Star & Lock buttons active styling
  els.starBtn?.classList.toggle('active', !!note.is_favorite);
  els.lockNoteBtn?.classList.toggle('active', !!note.is_locked);
  
  // Update Notebook selector labels
  els.noteNotebookLabel.textContent = note.notebook || 'Kein Notizbuch';
  
  // Update badges container
  let badgesHtml = '';
  if (note.notebook) {
    badgesHtml += `<span class="nn-card-badge notebook-badge">📁 ${escapeHtml(note.notebook)}</span>`;
  }
  if (note.tags) {
    (note.tags || '').split(',').map(x => x.trim()).filter(Boolean).forEach(tg => {
      badgesHtml += `<span class="nn-card-badge tag-badge">#${escapeHtml(tg)}</span>`;
    });
  }
  // Presence hint: other users with this note open right now.
  const ownActorId = state.ctx?.actor?.id || '';
  const presencePeers = (state.presenceRemote || []).filter((entry) => entry
    && entry.collection === 'notes'
    && entry.recordId === note.id
    && entry.actorId
    && entry.actorId !== ownActorId);
  if (presencePeers.length) {
    const names = [...new Set(presencePeers.map((entry) => entry.actorName || entry.actorId))].join(', ');
    badgesHtml += `<span class="nn-card-badge presence-badge">✎ ${escapeHtml(names)} ${escapeHtml(state.t('presenceEditing', 'bearbeitet gerade'))}</span>`;
  }
  if (els.noteBadgesContainer) {
    els.noteBadgesContainer.innerHTML = badgesHtml;
  }
  
  // Update dates
  if (els.noteDate) els.noteDate.textContent = formatFullTimestamp(note.updated_at_ms, state.lang);
  
  // Show Restore banner if trashed
  if (note.is_trashed && els.editorWorkspace) {
    const banner = document.createElement('div');
    banner.className = 'nn-restore-banner';
    banner.style.cssText = 'background: var(--accent-soft); border-bottom: 1px solid var(--line); padding: 8px 16px; display: flex; align-items: center; justify-content: space-between; font-size: 12.5px; color: var(--accent); margin-bottom: 10px; border-radius: 4px;';
    banner.innerHTML = `
      <span>⚠️ Diese Notiz ist im Papierkorb.</span>
      <button type="button" data-action="restore-note" style="background: var(--accent); color: var(--surface); border: 0; padding: 4px 8px; border-radius: 4px; cursor: pointer; font-size: 11px; font-weight: 600;">Wiederherstellen</button>
    `;
    els.editorWorkspace.prepend(banner);
    banner.querySelector('[data-action="restore-note"]')?.addEventListener('click', handleRestoreNoteClick);
  }

  if (note[DRAFT_NOTE_MARKER] && els.editorWorkspace) {
    const banner = document.createElement('div');
    banner.className = 'nn-draft-banner';
    banner.innerHTML = `
      <span>${escapeHtml(state.t('draftStatus'))}</span>
      <div class="nn-banner-actions">
        <button type="button" data-action="save-draft-note">${escapeHtml(state.t('save', 'Speichern'))}</button>
        <button type="button" data-action="discard-draft-note">${escapeHtml(state.t('discard', 'Verwerfen'))}</button>
      </div>
    `;
    els.editorWorkspace.prepend(banner);
    banner.querySelector('[data-action="save-draft-note"]')?.addEventListener('click', handleSaveDraftNoteClick);
    banner.querySelector('[data-action="discard-draft-note"]')?.addEventListener('click', handleDiscardDraftNoteClick);
  }
  
  // Check Zero-Knowledge locked state
  if (note.is_locked && !state.activeNoteDecrypted[note.id]) {
    els.noteLockScreen?.removeAttribute('hidden');
    if (els.editor) els.editor.innerHTML = '';
    if (els.words) els.words.textContent = `0 ${state.t('words')}`;
    if (els.chars) els.chars.textContent = `0 ${state.t('chars')}`;
    setToolbarDisabled(true, 'Notiz ist gesperrt');
  } else {
    els.noteLockScreen?.setAttribute('hidden', '');
    setToolbarDisabled(false);
    
    let contentToDisplay = normalizeStoredContent(note.content || '');
    if (note.is_locked) {
      contentToDisplay = normalizeStoredContent(state.activeNoteDecryptedContent[note.id] || '');
    }
    
    const shouldSelectEditorEnd = state.renderedNoteId !== note.id;
    if (state.lastLexicalHtml !== contentToDisplay || shouldSelectEditorEnd) {
      state.lastLexicalHtml = contentToDisplay;
      if (state.lexicalEditor) {
        state.hydratingEditor = true;
        state.lexicalEditor.update(() => {
          const parser = new DOMParser();
          const dom = parser.parseFromString(contentToDisplay, 'text/html');
          const nodes = Lexical.$generateNodesFromDOM(state.lexicalEditor, dom);
          const root = Lexical.$getRoot();
          root.clear();
          root.append(...nodes);
          if (shouldSelectEditorEnd) {
            selectEditableEnd(root);
          }
        });
        state.hydratingEditor = false;
      }
    }
    state.renderedNoteId = note.id;
    
    // Stats calculation
    const plainText = getPlainText(contentToDisplay);
    const wordCount = countWords(plainText);
    const charCount = plainText.length;
    const readMin = Math.max(1, Math.ceil(wordCount / 200));
    
    if (els.words) els.words.textContent = `${wordCount} ${state.t('words')}`;
    if (els.chars) els.chars.textContent = `${charCount} ${state.t('chars')}`;
    if (els.readTime) els.readTime.textContent = `${readMin} ${state.t('readTime')}`;
  }
}

function updateActionAvailability(note) {
  const hasNote = !!note;
  if (els.deleteBtn) {
    els.deleteBtn.disabled = !hasNote;
    els.deleteBtn.title = note?.[DRAFT_NOTE_MARKER] ? state.t('discard', 'Verwerfen') : state.t('deleteNote', 'Notiz loeschen');
  }
  if (els.starBtn) {
    els.starBtn.disabled = !hasNote || !!note?.[DRAFT_NOTE_MARKER] || !!note?.is_locked;
    els.starBtn.title = note?.[DRAFT_NOTE_MARKER]
      ? state.t('draftSaveFirst', 'Entwurf zuerst speichern')
      : 'Favorit umschalten';
  }
  if (els.lockNoteBtn) {
    els.lockNoteBtn.disabled = !hasNote || !!note?.[DRAFT_NOTE_MARKER];
    els.lockNoteBtn.title = note?.[DRAFT_NOTE_MARKER]
      ? state.t('draftSaveFirst', 'Entwurf zuerst speichern')
      : 'Notiz verschluesseln';
  }
  [els.notebookSelectBtn, els.tagsSelectBtn].forEach(btn => {
    if (btn) btn.disabled = !hasNote;
  });
}

function setToolbarDisabled(disabled, reason = '') {
  state.ctx?.host?.querySelectorAll('.nn-editor-toolbar button').forEach(btn => {
    btn.disabled = disabled;
    btn.setAttribute('aria-disabled', disabled ? 'true' : 'false');
    if (reason) btn.setAttribute('data-disabled-reason', reason);
    else btn.removeAttribute('data-disabled-reason');
  });
  if (els.editor) {
    els.editor.setAttribute('contenteditable', disabled ? 'false' : 'true');
  }
}

function selectNote(id) {
  state.activeNoteId = id;
  scheduleRender();
}

function handleSearch(e) {
  state.searchQuery = e.target.value;
  scheduleRender();
}

function handleEditorInput(e) {
  // Lexical handles input automatically via its registerUpdateListener
}

function lockedPlaceholderTitle(t = state.t) {
  return t('lockedNoteTitle', 'Gesperrte Notiz');
}

function deriveTitleFromPlainText(plainText, t = state.t) {
  const lines = String(plainText || '').split('\n').map(l => l.trim()).filter(Boolean);
  const title = lines.length > 0 ? lines[0].replace(/^#+\s+/, '').trim() : '';
  return title || t('untitled', 'Unbenannte Notiz');
}

// Builds the metadata fields persisted to the synced collection. This is the
// single chokepoint that enforces the zero-knowledge invariants for locked
// notes: the cleartext passcode is never persisted (it lives only in-memory in
// state.activeNoteDecrypted), and the title is replaced with a non-revealing
// placeholder so the synced/replicated store never carries the first line of a
// locked note's decrypted body.
function buildNotePersistPayload(note, t = state.t) {
  const isLocked = !!note.is_locked;
  return {
    title: isLocked ? lockedPlaceholderTitle(t) : (note.title || ''),
    notebook: note.notebook || '',
    tags: note.tags || '',
    is_favorite: !!note.is_favorite,
    is_trashed: !!note.is_trashed,
    is_locked: isLocked,
    lock_passcode: ''
  };
}

function processContentInput(newHtml) {
  const note = state.notes.find(n => n.id === state.activeNoteId);
  if (!note) return;

  const plainText = getPlainText(newHtml);

  let newTitle;
  if (note.is_locked) {
    state.activeNoteDecryptedContent[note.id] = newHtml;
    // Zero-knowledge: never derive a cleartext title from a locked note's
    // decrypted body — the title is persisted in cleartext (see commitSave).
    newTitle = lockedPlaceholderTitle();
  } else {
    note.content = newHtml;
    newTitle = deriveTitleFromPlainText(plainText);
  }

  note.title = newTitle;
  note.updated_at_ms = Date.now();
  if (!note[DRAFT_NOTE_MARKER]) {
    saveToLocalCache();
  }
  // Instant UI reflection
  const card = els.notesList?.querySelector(`[data-note-id="${note.id}"]`);
  if (card) {
    const titleEl = card.querySelector('.notes-card-title');
    const snippetEl = card.querySelector('.notes-card-snippet');
    const dateEl = card.querySelector('.notes-card-date');
    if (titleEl) titleEl.textContent = newTitle;
    if (snippetEl) snippetEl.textContent = extractSnippet(newHtml, newTitle);
    if (dateEl) dateEl.textContent = formatTimestamp(note.updated_at_ms);
  }
  
  // Stats
  const wordCount = countWords(plainText);
  const charCount = plainText.length;
  const readMin = Math.max(1, Math.ceil(wordCount / 200));
  
  if (els.words) els.words.textContent = `${wordCount} ${state.t('words')}`;
  if (els.chars) els.chars.textContent = `${charCount} ${state.t('chars')}`;
  if (els.readTime) els.readTime.textContent = `${readMin} ${state.t('readTime')}`;
  
  // Syncing display
  if (els.status) {
    if (note[DRAFT_NOTE_MARKER]) {
      els.status.textContent = state.t('draftStatus');
    } else {
      els.status.innerHTML = `
        <svg class="nn-sync-icon pulse" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M17.5 19A3.5 3.5 0 0 0 21 15.5c0-2.79-2.54-4.5-5-4.5-.42-1.04-1.21-1.92-2.18-2.5A6 6 0 0 0 2 13.5c0 2.2 1.4 3.9 3.5 4.5"></path></svg>
        <span>${state.t('saving')}</span>
      `;
    }
  }
  if (state.saveTimer) clearTimeout(state.saveTimer);
  if (note[DRAFT_NOTE_MARKER]) return;
  
  state.saveTimer = setTimeout(() => {
    state.saveTimer = null;
    commitSave(note).catch(err => {
      const logPrefix = state.ctx.module?.id === 'notizen' ? '[notizen]' : '[notes]';
      console.error(`${logPrefix} autosave failed`, err);
      if (els.status) els.status.textContent = 'Save failed';
    });
  }, SAVE_DEBOUNCE_MS);
}

function normalizeStoredContent(content) {
  const value = String(content || '');
  const trimmed = value.trim();
  if (!trimmed) return '';
  if (trimmed.startsWith('<') || /<\/?[a-z][\s\S]*>/i.test(trimmed)) {
    return value;
  }
  const lines = value.replace(/\r\n?/g, '\n').split('\n');
  return lines.map((line) => {
    const trimmedLine = line.trim();
    if (!trimmedLine) return '<p><br></p>';
    const heading = trimmedLine.match(/^(#{1,3})\s+(.+)$/);
    if (heading) {
      const level = heading[1].length;
      return `<h${level}>${escapeHtml(heading[2])}</h${level}>`;
    }
    return `<p>${escapeHtml(line)}</p>`;
  }).join('');
}
async function commitSave(note) {
  if (!note || note[DRAFT_NOTE_MARKER]) return;
  const collection = getCollection();
  if (!collection) return;
  
  try {
    const doc = await collection.findOne(note.id).exec();
    if (doc) {
      let contentToSave = note.content;
      if (note.is_locked) {
        const passcode = state.activeNoteDecrypted[note.id];
        const decrypted = state.activeNoteDecryptedContent[note.id] || '';
        if (passcode) {
          const encrypted = await encryptContent(decrypted, passcode);
          contentToSave = JSON.stringify(encrypted);
          note.content = contentToSave; // update state note content to match
        }
      }
      
      await doc.patch({
        ...buildNotePersistPayload(note),
        content: contentToSave,
        updated_at_ms: Date.now()
      });
      
      // Update local storage mirror too so it matches
      saveToLocalCache();
      
      if (els.status) {
        els.status.innerHTML = `
          <svg class="nn-sync-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M17.5 19A3.5 3.5 0 0 0 21 15.5c0-2.79-2.54-4.5-5-4.5-.42-1.04-1.21-1.92-2.18-2.5A6 6 0 0 0 2 13.5c0 2.2 1.4 3.9 3.5 4.5"></path></svg>
          <span>${state.t('saved')}</span>
        `;
      }
    }
  } catch (err) {
    console.warn('Background save failed', err);
  }
}
async function handleCreateNote() {
  const existingDraft = state.notes.find(note => note[DRAFT_NOTE_MARKER]);
  if (existingDraft) {
    state.activeNoteId = existingDraft.id;
    scheduleRender();
    window.setTimeout(() => focusEditorAtEnd(), NOTES_RENDER_DEBOUNCE_MS + 20);
    return;
  }

  const folder = 'Notes';
  const newId = generateUUID();
  const title = state.t('newNote');
  const content = `<h1>${title}</h1><p><br></p>`;
  
  const newNote = {
    id: newId,
    title,
    content,
    folder,
    notebook: state.activeNotebook || '',
    tags: state.activeTag || '',
    is_favorite: state.activeCategory === 'favorites',
    is_trashed: false,
    is_locked: false,
    lock_passcode: '',
    updated_at_ms: Date.now(),
    [DRAFT_NOTE_MARKER]: true
  };
  
  state.notes.unshift(newNote);
  syncNotebooksAndTags();
  
  state.activeNoteId = newId;
  scheduleRender();
  window.setTimeout(() => focusEditorAtEnd(), NOTES_RENDER_DEBOUNCE_MS + 20);
}

async function handleSaveDraftNoteClick() {
  const note = state.notes.find(n => n.id === state.activeNoteId);
  if (!note || !note[DRAFT_NOTE_MARKER]) return;

  delete note[DRAFT_NOTE_MARKER];
  note.updated_at_ms = Date.now();

  const collection = getCollection();
  if (collection) {
    try {
      await collection.insert({ ...note });
    } catch (error) {
      const logPrefix = state.ctx.module?.id === 'notizen' ? '[notizen]' : '[notes]';
      console.warn(`${logPrefix} background draft save failed`, error);
      const doc = await collection.findOne(note.id).exec().catch(() => null);
      if (doc) await doc.patch(note);
    }
  }

  syncNotebooksAndTags();
  saveToLocalCache();
  showActionToast(state.t('draftSaved'));
  scheduleRender();
}

function handleDiscardDraftNoteClick() {
  const note = state.notes.find(n => n.id === state.activeNoteId);
  if (!note || !note[DRAFT_NOTE_MARKER]) return;

  state.notes = state.notes.filter(n => n.id !== note.id);
  state.activeNoteId = getFilteredNotes()[0]?.id || '';
  syncNotebooksAndTags();
  showActionToast(state.t('draftDiscarded'));
  scheduleRender();
}

async function handleDeleteNote() {
  const note = state.notes.find(n => n.id === state.activeNoteId);
  if (!note) return;

  if (note[DRAFT_NOTE_MARKER]) {
    handleDiscardDraftNoteClick();
    return;
  }
  
  if (note.is_trashed) {
    const confirmMessage = state.t('deleteConfirm');
    if (!confirm(confirmMessage)) return;
    
    state.notes = state.notes.filter(n => n.id !== note.id);
    saveToLocalCache();
    
    const oldId = note.id;
    state.activeNoteId = '';
    
    const list = getFilteredNotes();
    if (list.length > 0) state.activeNoteId = list[0].id;
    scheduleRender();
    
    const collection = getCollection();
    if (collection) {
      try {
        const doc = await collection.findOne(oldId).exec();
        if (doc) await doc.remove();
      } catch (error) {
        const logPrefix = state.ctx.module?.id === 'notizen' ? '[notizen]' : '[notes]';
        console.warn(`${logPrefix} failed to remove note`, error);
      }
    }
  } else {
    const confirmMessage = state.t('deleteToTrash');
    if (!confirm(confirmMessage)) return;

    note.is_trashed = true;
    note.updated_at_ms = Date.now();
    saveToLocalCache();
    
    state.activeNoteId = '';
    const list = getFilteredNotes();
    if (list.length > 0) state.activeNoteId = list[0].id;
    scheduleRender();
    
    const collection = getCollection();
    if (collection) {
      try {
        const doc = await collection.findOne(note.id).exec();
        if (doc) await doc.patch({ is_trashed: true, updated_at_ms: Date.now() });
      } catch (error) {
        const logPrefix = state.ctx.module?.id === 'notizen' ? '[notizen]' : '[notes]';
        console.warn(`${logPrefix} background trash failed`, error);
      }
    }

    showActionToast('Notiz in den Papierkorb verschoben.', 'Rueckgaengig', async () => {
      note.is_trashed = false;
      note.updated_at_ms = Date.now();
      state.activeNoteId = note.id;
      saveToLocalCache();
      scheduleRender();
      const undoCollection = getCollection();
      if (undoCollection) {
        const doc = await undoCollection.findOne(note.id).exec().catch(() => null);
        if (doc) await doc.patch({ is_trashed: false, updated_at_ms: Date.now() });
      }
    });
  }
}

async function handleRestoreNoteClick() {
  const note = state.notes.find(n => n.id === state.activeNoteId);
  if (!note) return;
  
  note.is_trashed = false;
  note.updated_at_ms = Date.now();
  saveToLocalCache();
  
  scheduleRender();
  
  const collection = getCollection();
  if (collection) {
    try {
      const doc = await collection.findOne(note.id).exec();
      if (doc) await doc.patch({ is_trashed: false, updated_at_ms: Date.now() });
    } catch (error) {
      const logPrefix = state.ctx.module?.id === 'notizen' ? '[notizen]' : '[notes]';
      console.warn(`${logPrefix} background restore failed`, error);
    }
  }
}

// Sidebars & Expander navigations
function handleToggleNavClick(e) {
  const header = e.currentTarget;
  const list = header.nextElementSibling;
  const arrow = header.querySelector('.nn-nav-arrow');
  if (list) {
    const isHidden = list.style.display === 'none';
    list.style.display = isHidden ? 'flex' : 'none';
    if (arrow) {
      arrow.classList.toggle('active', isHidden);
    }
  }
}

async function handleCreateNotebookClick(e) {
  e.stopPropagation();
  const name = prompt(state.t('newNotebookPrompt'));
  if (!name || !name.trim()) return;
  
  const nb = name.trim();
  if (!state.notebooks.includes(nb)) {
    state.notebooks.push(nb);
    syncNotebooksAndTags();
    saveToLocalCache();
  }
  
  state.activeCategory = '';
  state.activeTag = '';
  state.activeNotebook = nb;
  await handleCreateNote();
}

async function handleCreateTagClick(e) {
  e.stopPropagation();
  const name = prompt(state.t('newTagPrompt'));
  if (!name || !name.trim()) return;
  
  const tg = name.trim();
  if (!state.tags.includes(tg)) {
    state.tags.push(tg);
    syncNotebooksAndTags();
    saveToLocalCache();
  }
  
  state.activeCategory = '';
  state.activeNotebook = '';
  state.activeTag = tg;
  await handleCreateNote();
}

function handleCategoryClick(e) {
  const cat = e.currentTarget.getAttribute('data-nav-category');
  state.activeCategory = cat;
  state.activeNotebook = '';
  state.activeTag = '';
  
  const list = getFilteredNotes();
  if (list.length > 0) selectNote(list[0].id);
  else selectNote('');
  scheduleRender();
}

// Keypads and Zero-Knowledge Lock overlays
function handlePinPadClick(e) {
  const btn = e.target.closest('button');
  if (!btn) return;
  
  if (btn.hasAttribute('data-pin')) {
    const digit = btn.getAttribute('data-pin');
    if (state.pinBuffer.length < 4) {
      state.pinBuffer += digit;
      els.pinDots?.forEach((dot, index) => {
        dot.classList.toggle('filled', index < state.pinBuffer.length);
      });
      
      if (state.pinBuffer.length === 4) {
        setTimeout(submitPin, 200);
      }
    }
  } else if (btn.getAttribute('data-action') === 'pin-clear') {
    state.pinBuffer = '';
    els.pinDots?.forEach(dot => dot.classList.remove('filled', 'error'));
  } else if (btn.getAttribute('data-action') === 'pin-ok') {
    submitPin();
  }
}

function submitPin() {
  const cachePrefix = state.ctx.module?.id === 'notizen' ? 'ctox.notizen' : 'ctox.notes';
  if (state.pinBuffer === '1234') {
    state.appLocked = false;
    localStorage.setItem(`${cachePrefix}.appLocked`, 'false');
    els.clientLockScreen?.setAttribute('hidden', '');
    state.pinBuffer = '';
    els.pinDots?.forEach(dot => dot.classList.remove('filled', 'error'));
  } else {
    els.pinDots?.forEach(dot => {
      dot.classList.add('error');
    });
    state.pinBuffer = '';
    setTimeout(() => {
      els.pinDots?.forEach(dot => dot.classList.remove('filled', 'error'));
    }, 600);
  }
}

function handleLockAppClick() {
  const cachePrefix = state.ctx.module?.id === 'notizen' ? 'ctox.notizen' : 'ctox.notes';
  state.appLocked = true;
  localStorage.setItem(`${cachePrefix}.appLocked`, 'true');
  state.pinBuffer = '';
  els.pinDots?.forEach(dot => dot.classList.remove('filled', 'error'));
  els.clientLockScreen?.removeAttribute('hidden');
}
// Locked Note Decrypter
async function handleDecryptNoteClick() {
  const note = state.notes.find(n => n.id === state.activeNoteId);
  if (!note) return;
  
  const pw = els.notePasscodeInput?.value || '';
  try {
    const encryptedData = JSON.parse(note.content);
    const decrypted = await decryptContent(encryptedData, pw);
    
    state.activeNoteDecrypted[note.id] = pw;
    state.activeNoteDecryptedContent[note.id] = decrypted;
    if (els.notePasscodeInput) els.notePasscodeInput.value = '';
    renderEditor();
  } catch (error) {
    console.error('Decryption failed', error);
    if (els.notePasscodeInput) {
      els.notePasscodeInput.style.borderColor = 'var(--danger)';
      els.notePasscodeInput.style.boxShadow = '0 0 0 3px color-mix(in srgb, var(--danger) 20%, transparent)';
      setTimeout(() => {
        els.notePasscodeInput.style.borderColor = '';
        els.notePasscodeInput.style.boxShadow = '';
      }, 800);
    }
  }
}

function handleDecryptNoteKeydown(e) {
  if (e.key === 'Enter') {
    handleDecryptNoteClick();
  }
}

// Editor actions & Meta dropdown bindings
function handleStarNoteClick() {
  const note = state.notes.find(n => n.id === state.activeNoteId);
  if (!note) return;
  if (note[DRAFT_NOTE_MARKER] || note.is_locked) {
    showActionToast(state.t('draftSaveFirst', 'Entwurf zuerst speichern'));
    return;
  }
  
  const previousValue = !!note.is_favorite;
  note.is_favorite = !note.is_favorite;
  note.updated_at_ms = Date.now();
  saveToLocalCache();
  
  scheduleRender();
  commitSave(note);
  showActionToast(
    note.is_favorite ? 'Als Favorit markiert.' : 'Favorit entfernt.',
    'Rueckgaengig',
    async () => {
      note.is_favorite = previousValue;
      note.updated_at_ms = Date.now();
      saveToLocalCache();
      scheduleRender();
      await commitSave(note);
    }
  );
}

async function handleLockNoteClick() {
  const note = state.notes.find(n => n.id === state.activeNoteId);
  if (!note) return;
  if (note[DRAFT_NOTE_MARKER]) {
    showActionToast(state.t('draftSaveFirst', 'Entwurf zuerst speichern'));
    return;
  }
  
  if (note.is_locked) {
    if (!state.activeNoteDecrypted[note.id]) {
      showActionToast('Notiz zuerst entsperren, dann Verschluesselung entfernen.');
      return;
    }
    if (!confirm('Verschluesselung fuer diese Notiz entfernen?')) return;
    let plainText = note.content;
    const passcode = state.activeNoteDecrypted[note.id];
    if (passcode) {
      plainText = state.activeNoteDecryptedContent[note.id] || note.content;
    }
    
    note.is_locked = false;
    note.lock_passcode = '';
    note.content = plainText;
    // Note is now cleartext again — restore a content-derived title.
    note.title = deriveTitleFromPlainText(getPlainText(plainText));
    delete state.activeNoteDecrypted[note.id];
    delete state.activeNoteDecryptedContent[note.id];

    saveToLocalCache();
    scheduleRender();
    await commitSave(note);
  } else {
    const entered = prompt('Gebe ein Passwort für diese Notiz ein (Standard: 1234):');
    if (entered === null) return;
    const pw = entered.trim() || '1234';
    const plainText = note.content;
    try {
      const encrypted = await encryptContent(plainText, pw);
      note.is_locked = true;
      // Zero-knowledge: the passcode is kept only in-memory (state below); it
      // is never written onto the note object, so it cannot reach localStorage
      // or the synced collection.
      note.lock_passcode = '';
      note.content = JSON.stringify(encrypted);
      // Replace the cleartext title with a non-revealing placeholder.
      note.title = lockedPlaceholderTitle();

      state.activeNoteDecrypted[note.id] = pw;
      state.activeNoteDecryptedContent[note.id] = plainText;
      
      saveToLocalCache();
      scheduleRender();
      await commitSave(note);
    } catch (err) {
      console.error('Encryption failed', err);
      alert('Verschlüsselung fehlgeschlagen!');
    }
  }
}
// Notebook select dropdown triggers
function handleNotebookSelectBtnClick(e) {
  e.stopPropagation();
  const wasHidden = els.notebookDropdown.hidden;
  closeAllDropdowns();
  els.notebookDropdown.hidden = !wasHidden;
  
  if (!els.notebookDropdown.hidden) {
    const note = state.notes.find(n => n.id === state.activeNoteId);
    let html = `
      <button type="button" class="nn-dropdown-item ${!note?.notebook ? 'active' : ''}" data-select-notebook="">
        <span>Kein Notizbuch</span>
      </button>
      <div class="nn-popover-divider"></div>
    `;
    state.notebooks.forEach(nb => {
      const active = note?.notebook === nb;
      html += `
        <button type="button" class="nn-dropdown-item ${active ? 'active' : ''}" data-select-notebook="${escapeHtml(nb)}">
          <span>📁 ${escapeHtml(nb)}</span>
        </button>
      `;
    });
    els.notebookDropdown.innerHTML = html;
    
    els.notebookDropdown.querySelectorAll('[data-select-notebook]').forEach(btn => {
      btn.addEventListener('click', () => {
        if (!note) return;
        const selected = btn.getAttribute('data-select-notebook');
        note.notebook = selected;
        note.updated_at_ms = Date.now();
        saveToLocalCache();
        scheduleRender();
        commitSave(note);
        closeAllDropdowns();
      });
    });
  }
}

// Tags select dropdown triggers
function handleTagsSelectBtnClick(e) {
  e.stopPropagation();
  const wasHidden = els.tagsDropdown.hidden;
  closeAllDropdowns();
  els.tagsDropdown.hidden = !wasHidden;
  
  if (!els.tagsDropdown.hidden) {
    const note = state.notes.find(n => n.id === state.activeNoteId);
    const assignedTags = (note?.tags || '').split(',').map(x => x.trim()).filter(Boolean);
    
    if (state.tags.length === 0) {
      els.tagsDropdown.innerHTML = `
        <div style="padding: 10px 14px; font-size:11.5px; color: var(--nn-text-muted);">
          Keine Tags erstellt
        </div>
      `;
      return;
    }
    
    let html = '';
    state.tags.forEach(tg => {
      const isChecked = assignedTags.includes(tg);
      html += `
        <label class="nn-dropdown-item" style="cursor:pointer; display:flex; align-items:center; gap:8px;">
          <input type="checkbox" data-note-tag-check="${escapeHtml(tg)}" ${isChecked ? 'checked' : ''} style="accent-color: var(--nn-accent);" />
          <span>🏷️ ${escapeHtml(tg)}</span>
        </label>
      `;
    });
    els.tagsDropdown.innerHTML = html;
    
    els.tagsDropdown.querySelectorAll('[data-note-tag-check]').forEach(cb => {
      cb.addEventListener('change', () => {
        if (!note) return;
        const tg = cb.getAttribute('data-note-tag-check');
        const assigned = (note.tags || '').split(',').map(x => x.trim()).filter(Boolean);
        
        let newAssigned;
        if (cb.checked) {
          newAssigned = Array.from(new Set([...assigned, tg]));
        } else {
          newAssigned = assigned.filter(x => x !== tg);
        }
        
        note.tags = newAssigned.join(',');
        note.updated_at_ms = Date.now();
        saveToLocalCache();
        scheduleRender();
        commitSave(note);
      });
    });
  }
}

// Popover sort & filters
function handleFilterTriggerClick(e) {
  e.stopPropagation();
  const wasHidden = els.filterPopover.hidden;
  closeAllDropdowns();
  els.filterPopover.hidden = !wasHidden;
}

function handleSortClick(e) {
  state.sortMode = e.currentTarget.getAttribute('data-sort');
  els.filterPopover.querySelectorAll('[data-sort]').forEach(el => {
    el.classList.toggle('active', el.getAttribute('data-sort') === state.sortMode);
  });
  scheduleRender();
}

function handleViewModeClick(e) {
  state.viewMode = e.currentTarget.getAttribute('data-view-mode');
  els.filterPopover.querySelectorAll('[data-view-mode]').forEach(el => {
    el.classList.toggle('active', el.getAttribute('data-view-mode') === state.viewMode);
  });
  els.notesList?.classList.toggle('nn-compact-view', state.viewMode === 'compact');
  closeAllDropdowns();
}

// Rich Text format dropdown
function handleFormatBtnClick(e) {
  e.stopPropagation();
  const wasHidden = els.headersDropdown.hidden;
  closeAllDropdowns();
  els.headersDropdown.hidden = !wasHidden;
}

function syncEditorFromDom() {
  if (state.lexicalEditor && els.editor) {
    const html = els.editor.innerHTML;
    state.lastLexicalHtml = html;
    state.lexicalEditor.update(() => {
      const parser = new DOMParser();
      const dom = parser.parseFromString(html, 'text/html');
      const nodes = Lexical.$generateNodesFromDOM(state.lexicalEditor, dom);
      const root = Lexical.$getRoot();
      root.clear();
      root.append(...nodes);
    });
    processContentInput(html);
  }
}

function handleFormatCommandClick(e) {
  e.stopPropagation();
  const btn = e.currentTarget;
  const cmd = btn.getAttribute('data-format-cmd');
  const val = btn.getAttribute('data-val') || null;
  
  if (state.lexicalEditor) {
    els.editor?.focus();
    if (cmd === 'bold' || cmd === 'italic' || cmd === 'underline' || cmd === 'strikeThrough') {
      const type = cmd === 'strikeThrough' ? 'strikethrough' : cmd.toLowerCase();
      state.lexicalEditor.dispatchCommand(Lexical.FORMAT_TEXT_COMMAND, type);
    } else if (cmd === 'formatBlock') {
      state.lexicalEditor.update(() => {
        const selection = Lexical.$getSelection();
        if (Lexical.$isRangeSelection(selection)) {
          if (val === 'h1' || val === 'h2' || val === 'h3') {
            Lexical.$setBlocksType(selection, () => Lexical.$createHeadingNode(val));
          } else if (val === 'p') {
            Lexical.$setBlocksType(selection, () => Lexical.$createParagraphNode());
          }
        }
      });
    } else if (cmd === 'insertHorizontalRule') {
      state.lexicalEditor.update(() => {
        const selection = Lexical.$getSelection();
        if (Lexical.$isRangeSelection(selection)) {
          const hr = document.createElement('hr');
          const nodes = Lexical.$generateNodesFromDOM(state.lexicalEditor, hr);
          selection.insertNodes(nodes);
        }
      });
    } else if (cmd === 'justifyLeft' || cmd === 'justifyCenter') {
      const type = cmd === 'justifyLeft' ? 'left' : 'center';
      state.lexicalEditor.dispatchCommand(Lexical.FORMAT_ELEMENT_COMMAND, type);
    } else if (cmd === 'insertHTML') {
      state.lexicalEditor.update(() => {
        const selection = Lexical.$getSelection();
        if (Lexical.$isRangeSelection(selection)) {
          const temp = document.createElement('div');
          temp.innerHTML = val;
          const nodes = Lexical.$generateNodesFromDOM(state.lexicalEditor, temp);
          selection.insertNodes(nodes);
        }
      });
    } else if (cmd === 'hiliteColor') {
      state.lexicalEditor.update(() => {
        const selection = Lexical.$getSelection();
        if (Lexical.$isRangeSelection(selection)) {
          Lexical.$patchStyleText(selection, { 'background-color': val });
        }
      });
    } else {
      // Fallback
      document.execCommand(cmd, false, val);
      syncEditorFromDom();
    }
  }
  closeAllDropdowns();
}

// Inserters
function handleChecklistBtnClick() {
  if (state.lexicalEditor) {
    state.lexicalEditor.update(() => {
      let selection = Lexical.$getSelection();
      const node = new CustomHTMLNode(
        `<input type="checkbox" class="notes-todo-checkbox" contenteditable="false"><span class="notes-todo-text">Checklisteneintrag</span>`,
        'div',
        'notes-todo-row'
      );
      
      try {
        const anchorNode = selection?.anchor?.getNode();
        if (Lexical.$isRangeSelection(selection) && anchorNode && anchorNode.getParent() !== null) {
          selection.insertNodes([node]);
          const p = Lexical.$createParagraphNode();
          selection.insertNodes([p]);
        } else {
          Lexical.$getRoot().append(node);
          const p = Lexical.$createParagraphNode();
          Lexical.$getRoot().append(p);
        }
      } catch (err) {
        console.warn('Checklist insert fallback triggered:', err);
        Lexical.$getRoot().append(node);
        const p = Lexical.$createParagraphNode();
        Lexical.$getRoot().append(p);
      }
    });
  }
}

function handleTableBtnClick() {
  if (state.lexicalEditor) {
    state.lexicalEditor.update(() => {
      let selection = Lexical.$getSelection();
      const node = new CustomHTMLNode(
        `<tbody>
          <tr>
            <td contenteditable="true">Spalte 1</td>
            <td contenteditable="true">Spalte 2</td>
          </tr>
          <tr>
            <td contenteditable="true">Inhalt 1</td>
            <td contenteditable="true">Inhalt 2</td>
          </tr>
        </tbody>`,
        'table',
        'notes-table'
      );
      
      try {
        const anchorNode = selection?.anchor?.getNode();
        if (Lexical.$isRangeSelection(selection) && anchorNode && anchorNode.getParent() !== null) {
          selection.insertNodes([node]);
          const p = Lexical.$createParagraphNode();
          selection.insertNodes([p]);
        } else {
          Lexical.$getRoot().append(node);
          const p = Lexical.$createParagraphNode();
          Lexical.$getRoot().append(p);
        }
      } catch (err) {
        console.warn('Table insert fallback triggered:', err);
        Lexical.$getRoot().append(node);
        const p = Lexical.$createParagraphNode();
        Lexical.$getRoot().append(p);
      }
    });
  }
}

function handleCodeblockBtnClick() {
  if (state.lexicalEditor) {
    state.lexicalEditor.update(() => {
      let selection = Lexical.$getSelection();
      const node = new CustomHTMLNode(
        `<code>// Monospace Code Here...</code>`,
        'pre',
        'nn-code-block'
      );
      
      try {
        const anchorNode = selection?.anchor?.getNode();
        if (Lexical.$isRangeSelection(selection) && anchorNode && anchorNode.getParent() !== null) {
          selection.insertNodes([node]);
        } else {
          Lexical.$getRoot().append(node);
        }
      } catch (err) {
        console.warn('Codeblock insert fallback triggered:', err);
        Lexical.$getRoot().append(node);
      }
    });
  }
}

function handleFormatCalloutsClick(e) {
  e.stopPropagation();
  const wasHidden = els.calloutsDropdown.hidden;
  closeAllDropdowns();
  els.calloutsDropdown.hidden = !wasHidden;
}

function handleCalloutCommandClick(e) {
  const type = e.currentTarget.getAttribute('data-callout-type');
  let emoji = '💡', label = 'INFO';
  if (type === 'warning') { emoji = '⚠️'; label = 'WARNUNG'; }
  else if (type === 'tip') { emoji = '❇️'; label = 'TIPP'; }
  else if (type === 'danger') { emoji = '🚨'; label = 'GEFAHR'; }
  
  if (state.lexicalEditor) {
    state.lexicalEditor.update(() => {
      let selection = Lexical.$getSelection();
      const node = new CustomHTMLNode(
        `<span class="callout-icon">${emoji}</span>
        <div class="callout-content" contenteditable="true">
          <strong>${label}</strong><br>Schreibe hier...
        </div>`,
        'div',
        `callout callout-${type}`
      );
      
      try {
        const anchorNode = selection?.anchor?.getNode();
        if (Lexical.$isRangeSelection(selection) && anchorNode && anchorNode.getParent() !== null) {
          selection.insertNodes([node]);
        } else {
          Lexical.$getRoot().append(node);
        }
      } catch (err) {
        console.warn('Callout insert fallback triggered:', err);
        Lexical.$getRoot().append(node);
      }
    });
  }
  closeAllDropdowns();
}

function handleTimestampBtnClick() {
  if (state.lexicalEditor) {
    state.lexicalEditor.update(() => {
      let selection = Lexical.$getSelection();
      const timeStr = new Date().toLocaleString();
      const textNode = Lexical.$createTextNode(timeStr);
      
      try {
        const anchorNode = selection?.anchor?.getNode();
        if (Lexical.$isRangeSelection(selection) && anchorNode && anchorNode.getParent() !== null) {
          selection.insertNodes([textNode]);
        } else {
          const root = Lexical.$getRoot();
          const lastChild = root.getLastChild();
          if (lastChild && lastChild.getType() === 'paragraph') {
            lastChild.append(textNode);
          } else {
            const p = Lexical.$createParagraphNode();
            p.append(textNode);
            root.append(p);
          }
        }
      } catch (err) {
        console.warn('Timestamp insert fallback triggered:', err);
        const root = Lexical.$getRoot();
        const lastChild = root.getLastChild();
        if (lastChild && lastChild.getType() === 'paragraph') {
          lastChild.append(textNode);
        } else {
          const p = Lexical.$createParagraphNode();
          p.append(textNode);
          root.append(p);
        }
      }
    });
  }
}

function handleGlobalClick(e) {
  if (e.target.closest('.nn-meta-select-wrap') || 
      e.target.closest('.nn-format-wrapper') || 
      e.target.closest('.nn-filter-trigger') || 
      e.target.closest('[data-action="toggle-filter"]')) {
    return;
  }
  closeAllDropdowns();
}

function handleDocumentKeydown(e) {
  if (e.key === 'Escape') {
    closeAllDropdowns();
  }
}

function handleSidebarKeydown(e) {
  if (e.key !== 'Enter' && e.key !== ' ') return;
  const target = e.target?.closest?.('.notes-folder-item, [data-toggle-nav]');
  if (!target || !els.folderList?.contains(target)) return;
  e.preventDefault();
  target.click();
}

function closeAllDropdowns() {
  if (els.notebookDropdown) els.notebookDropdown.hidden = true;
  if (els.tagsDropdown) els.tagsDropdown.hidden = true;
  if (els.filterPopover) els.filterPopover.hidden = true;
  if (els.headersDropdown) els.headersDropdown.hidden = true;
  if (els.calloutsDropdown) els.calloutsDropdown.hidden = true;
}

function showActionToast(message, actionLabel = '', onAction = null) {
  if (!els.root) return;
  if (state.toastTimer) clearTimeout(state.toastTimer);
  els.root.querySelector('.nn-action-toast')?.remove();

  const toast = document.createElement('div');
  toast.className = 'nn-action-toast';
  const text = document.createElement('span');
  text.textContent = message;
  toast.append(text);

  if (actionLabel && typeof onAction === 'function') {
    const action = document.createElement('button');
    action.type = 'button';
    action.textContent = actionLabel;
    action.addEventListener('click', async () => {
      try {
        await onAction();
      } finally {
        toast.remove();
      }
    }, { once: true });
    toast.append(action);
  }

  els.root.append(toast);
  state.toastTimer = setTimeout(() => {
    state.toastTimer = null;
    toast.remove();
  }, 6500);
}

function handleEditorCheckboxClick(e) {
  if (e.target.classList.contains('notes-todo-checkbox')) {
    const cb = e.target;
    if (cb.checked) {
      cb.setAttribute('checked', 'checked');
    } else {
      cb.removeAttribute('checked');
    }
    syncEditorFromDom();
  }
}

function handleEditorClick() {
  window.setTimeout(() => {
    const selection = window.getSelection();
    if (!selection || String(selection) || !state.activeNoteId) return;
    focusEditorAtEnd();
  }, 0);
}

function focusEditorAtEnd() {
  if (!state.lexicalEditor || !els.editor) return;
  state.lexicalEditor.update(() => {
    selectEditableEnd(Lexical.$getRoot());
  });
  state.lexicalEditor.getRootElement()?.focus({ preventScroll: true });
}

function selectEditableEnd(root) {
  const lastChild = root.getLastChild?.();
  if (lastChild?.selectEnd) {
    lastChild.selectEnd();
    return;
  }
  root.selectEnd();
}

function handleEditorKeydown(e) {
  if (!els.editor) return;
  
  const selection = window.getSelection();
  if (!selection.rangeCount) return;
  
  const todoRow = getActiveTodoRow();
  const range = selection.getRangeAt(0);
  
  if (todoRow) {
    const textNode = todoRow.querySelector('.notes-todo-text');
    if (!textNode) return;
    
    if (e.key === 'Enter') {
      e.preventDefault();
      const isEmpty = !textNode.textContent.trim();
      if (isEmpty) {
        // Convert to standard paragraph
        const p = document.createElement('p');
        p.innerHTML = '<br>';
        todoRow.parentNode.replaceChild(p, todoRow);
        
        const newRange = document.createRange();
        newRange.selectNodeContents(p);
        newRange.collapse(true);
        selection.removeAllRanges();
        selection.addRange(newRange);
      } else {
        // Create a new range from cursor to end of textNode
        const endRange = document.createRange();
        endRange.setStart(range.endContainer, range.endOffset);
        endRange.setEndAfter(textNode.lastChild || textNode);
        
        // Extract trailing content
        const trailingFragment = endRange.extractContents();
        
        // Create new todo row
        const newRow = document.createElement('div');
        newRow.className = 'notes-todo-row';
        
        const currentIndent = Array.from(todoRow.classList).find(c => c.startsWith('indent-'));
        if (currentIndent) {
          newRow.classList.add(currentIndent);
        }
        
        const checkbox = document.createElement('input');
        checkbox.type = 'checkbox';
        checkbox.className = 'notes-todo-checkbox';
        checkbox.setAttribute('contenteditable', 'false');
        newRow.appendChild(checkbox);
        
        const newText = document.createElement('span');
        newText.className = 'notes-todo-text';
        newText.appendChild(trailingFragment);
        
        if (!newText.textContent.trim()) {
          newText.innerHTML = '<br>';
        }
        newRow.appendChild(newText);
        
        todoRow.parentNode.insertBefore(newRow, todoRow.nextSibling);
        
        const newRange = document.createRange();
        newRange.selectNodeContents(newText);
        newRange.collapse(true);
        selection.removeAllRanges();
        selection.addRange(newRange);
      }
      syncEditorFromDom();
    } else if (e.key === 'Tab') {
      e.preventDefault();
      const currentIndent = Array.from(todoRow.classList).find(c => c.startsWith('indent-'));
      if (e.shiftKey) {
        // Outdent
        if (currentIndent === 'indent-3') {
          todoRow.classList.remove('indent-3');
          todoRow.classList.add('indent-2');
        } else if (currentIndent === 'indent-2') {
          todoRow.classList.remove('indent-2');
          todoRow.classList.add('indent-1');
        } else if (currentIndent === 'indent-1') {
          todoRow.classList.remove('indent-1');
        }
      } else {
        // Indent
        if (!currentIndent) {
          todoRow.classList.add('indent-1');
        } else if (currentIndent === 'indent-1') {
          todoRow.classList.remove('indent-1');
          todoRow.classList.add('indent-2');
        } else if (currentIndent === 'indent-2') {
          todoRow.classList.remove('indent-2');
          todoRow.classList.add('indent-3');
        }
      }
      syncEditorFromDom();
    } else if (e.key === 'Backspace') {
      const isAtStart = isAtStartOfNode(textNode, range);
      if (isAtStart) {
        e.preventDefault();
        const currentIndent = Array.from(todoRow.classList).find(c => c.startsWith('indent-'));
        if (currentIndent) {
          // Outdent
          if (currentIndent === 'indent-3') {
            todoRow.classList.remove('indent-3');
            todoRow.classList.add('indent-2');
          } else if (currentIndent === 'indent-2') {
            todoRow.classList.remove('indent-2');
            todoRow.classList.add('indent-1');
          } else if (currentIndent === 'indent-1') {
            todoRow.classList.remove('indent-1');
          }
        } else {
          // Convert to standard paragraph
          const p = document.createElement('p');
          p.innerHTML = textNode.innerHTML || '<br>';
          todoRow.parentNode.replaceChild(p, todoRow);
          
          const newRange = document.createRange();
          newRange.selectNodeContents(p);
          newRange.collapse(true);
          selection.removeAllRanges();
          selection.addRange(newRange);
        }
        syncEditorFromDom();
      }
    }
  } else {
    // Check if we are inside a heading block to split or start new standard paragraph on Enter
    const activeBlock = getActiveBlockElement();
    if (activeBlock && /^(H[1-6])$/i.test(activeBlock.nodeName)) {
      if (e.key === 'Enter') {
        e.preventDefault();
        const isAtEnd = isAtEndOfNode(activeBlock, range);
        if (isAtEnd) {
          const p = document.createElement('p');
          p.innerHTML = '<br>';
          activeBlock.parentNode.insertBefore(p, activeBlock.nextSibling);
          
          const newRange = document.createRange();
          newRange.selectNodeContents(p);
          newRange.collapse(true);
          selection.removeAllRanges();
          selection.addRange(newRange);
        } else {
          // Split block
          const postRange = document.createRange();
          postRange.setStart(range.endContainer, range.endOffset);
          postRange.setEndAfter(activeBlock.lastChild || activeBlock);
          const trailingFragment = postRange.extractContents();
          
          const p = document.createElement('p');
          p.appendChild(trailingFragment);
          if (!p.textContent.trim()) {
            p.innerHTML = '<br>';
          }
          activeBlock.parentNode.insertBefore(p, activeBlock.nextSibling);
          
          const newRange = document.createRange();
          newRange.selectNodeContents(p);
          newRange.collapse(true);
          selection.removeAllRanges();
          selection.addRange(newRange);
        }
        syncEditorFromDom();
      }
    }
  }
}

function getActiveTodoRow() {
  const selection = window.getSelection();
  if (!selection.rangeCount) return null;
  let node = selection.anchorNode;
  while (node && node !== els.editor) {
    if (node.nodeType === Node.ELEMENT_NODE && node.classList.contains('notes-todo-row')) {
      return node;
    }
    node = node.parentNode;
  }
  return null;
}

function getActiveBlockElement() {
  const selection = window.getSelection();
  if (!selection.rangeCount) return null;
  let node = selection.anchorNode;
  while (node && node !== els.editor) {
    if (node.nodeType === Node.ELEMENT_NODE && /^(H[1-6]|P|DIV|BLOCKQUOTE)$/i.test(node.nodeName)) {
      return node;
    }
    node = node.parentNode;
  }
  return null;
}

function isAtStartOfNode(node, selectionRange) {
  try {
    const preRange = document.createRange();
    preRange.setStart(node, 0);
    preRange.setEnd(selectionRange.startContainer, selectionRange.startOffset);
    return preRange.toString().length === 0;
  } catch (err) {
    return false;
  }
}

function isAtEndOfNode(node, selectionRange) {
  try {
    const postRange = document.createRange();
    postRange.setStart(selectionRange.endContainer, selectionRange.endOffset);
    postRange.setEnd(node, node.childNodes.length);
    return postRange.toString().length === 0;
  } catch (err) {
    return false;
  }
}
async function deriveKey(passcode, salt) {
  const encoder = new TextEncoder();
  const baseKey = await window.crypto.subtle.importKey(
    'raw',
    encoder.encode(passcode),
    'PBKDF2',
    false,
    ['deriveKey']
  );
  
  return window.crypto.subtle.deriveKey(
    {
      name: 'PBKDF2',
      salt: salt,
      iterations: 100000,
      hash: 'SHA-256'
    },
    baseKey,
    { name: 'AES-GCM', length: 256 },
    false,
    ['encrypt', 'decrypt']
  );
}

async function encryptContent(content, passcode) {
  const encoder = new TextEncoder();
  const salt = window.crypto.getRandomValues(new Uint8Array(16));
  const iv = window.crypto.getRandomValues(new Uint8Array(12));
  
  const key = await deriveKey(passcode, salt);
  const encrypted = await window.crypto.subtle.encrypt(
    { name: 'AES-GCM', iv },
    key,
    encoder.encode(content)
  );
  
  return {
    cipherText: btoa(String.fromCharCode(...new Uint8Array(encrypted))),
    salt: btoa(String.fromCharCode(...salt)),
    iv: btoa(String.fromCharCode(...iv))
  };
}

async function decryptContent(encryptedData, passcode) {
  try {
    const salt = new Uint8Array(atob(encryptedData.salt).split('').map(c => c.charCodeAt(0)));
    const iv = new Uint8Array(atob(encryptedData.iv).split('').map(c => c.charCodeAt(0)));
    const cipher = new Uint8Array(atob(encryptedData.cipherText).split('').map(c => c.charCodeAt(0)));
    
    const key = await deriveKey(passcode, salt);
    const decrypted = await window.crypto.subtle.decrypt(
      { name: 'AES-GCM', iv },
      key,
      cipher
    );
    
    return new TextDecoder().decode(decrypted);
  } catch (e) {
    console.error('Decryption failed', e);
    throw new Error('Ungültiges Passwort oder beschädigte Notiz');
  }
}

/* Helpers */

function generateUUID() {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
    const r = Math.random() * 16 | 0;
    const v = c === 'x' ? r : (r & 0x3 | 0x8);
    return v.toString(16);
  });
}

function formatTimestamp(ms) {
  if (!ms) return '';
  const date = new Date(ms);
  const now = new Date();
  
  if (date.toDateString() === now.toDateString()) {
    return date.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
  }
  
  const timeDiff = now.getTime() - date.getTime();
  if (timeDiff < 7 * 24 * 60 * 60 * 1000) {
    return date.toLocaleDateString(undefined, { weekday: 'long' });
  }
  
  return date.toLocaleDateString(undefined, { day: '2-digit', month: '2-digit', year: '2-digit' });
}

function formatFullTimestamp(ms, lang) {
  if (!ms) return '';
  const date = new Date(ms);
  const options = { day: 'numeric', month: 'long', year: 'numeric' };
  const dateStr = date.toLocaleDateString(lang === 'en' ? 'en-US' : 'de-DE', options);
  const timeStr = date.toLocaleTimeString(lang === 'en' ? 'en-US' : 'de-DE', { hour: '2-digit', minute: '2-digit' });
  if (lang === 'en') {
    return `${dateStr} at ${timeStr}`;
  } else {
    return `${dateStr} um ${timeStr}`;
  }
}

function getPlainText(html) {
  if (!html) return '';
  let processed = html
    .replace(/<\/p>/gi, '\n')
    .replace(/<\/div>/gi, '\n')
    .replace(/<\/li>/gi, '\n')
    .replace(/<\/h[1-6]>/gi, '\n')
    .replace(/<\/td>/gi, '\n')
    .replace(/<br\s*\/?>/gi, '\n');
  const tmp = document.createElement('div');
  tmp.innerHTML = processed;
  return tmp.textContent || tmp.innerText || '';
}

function extractSnippet(html, title) {
  if (html && html.trim().startsWith('{') && html.trim().endsWith('}')) {
    return '🔒 Verschlüsselte Notiz';
  }
  const plainText = getPlainText(html);
  if (!plainText) return '';
  const lines = plainText.split('\n').map(l => l.trim()).filter(Boolean);
  
  let startIndex = 0;
  for (let i = 0; i < lines.length; i++) {
    if (lines[i] === title) {
      startIndex = i + 1;
      break;
    }
  }
  
  for (let i = startIndex; i < lines.length; i++) {
    if (lines[i]) return lines[i].slice(0, 80);
  }
  
  return '';
}

function countWords(str) {
  if (!str) return 0;
  return str.trim().split(/\s+/).filter(Boolean).length;
}

function escapeHtml(value) {
  return String(value ?? '').replace(/[&<>"']/g, (char) => ({
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#39;',
  })[char]);
}

function setupResizers(host) {
  const leftResizer = host.querySelector('[data-resizer="left"]');
  const rightResizer = host.querySelector('[data-resizer="right"]');
  const containerEl = host.querySelector('[data-notes-root]') || host;
  const leftPane = host.querySelector('.notes-sidebar-pane');
  const listPane = host.querySelector('.notes-list-pane');
  
  const cleanups = [];
  const cachePrefix = getCachePrefix();

  const attachLocalResizer = ({ handle, pane, cssVar, storageKey, minWidth, maxWidth }) => {
    if (!handle || !pane) return null;

    let startX = 0;
    let startWidth = 0;
    let raf = 0;

    const clamp = (width) => Math.max(minWidth, Math.min(maxWidth, width));
    const setWidth = (width) => {
      const next = clamp(width);
      containerEl.style.setProperty(cssVar, `${next}px`);
      handle.setAttribute('aria-valuenow', String(Math.round(next)));
      handle.setAttribute('aria-valuetext', `${Math.round(next)} px`);
      localStorage.setItem(storageKey, String(Math.round(next)));
      return next;
    };
    const currentWidth = () => {
      const raw = window.getComputedStyle(containerEl).getPropertyValue(cssVar);
      const parsed = parseFloat(raw);
      return clamp(Number.isFinite(parsed) ? parsed : pane.getBoundingClientRect().width);
    };
    const onPointerMove = (event) => {
      if (raf) cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => {
        raf = 0;
        setWidth(startWidth + event.clientX - startX);
      });
    };
    const onPointerUp = () => {
      if (raf) cancelAnimationFrame(raf);
      raf = 0;
      document.body.classList.remove('is-resizing');
      handle.classList.remove('is-active');
      window.removeEventListener('pointermove', onPointerMove);
      window.removeEventListener('pointerup', onPointerUp);
      window.removeEventListener('pointercancel', onPointerUp);
    };
    const onPointerDown = (event) => {
      event.preventDefault();
      startX = event.clientX;
      startWidth = currentWidth();
      document.body.classList.add('is-resizing');
      handle.classList.add('is-active');
      window.addEventListener('pointermove', onPointerMove);
      window.addEventListener('pointerup', onPointerUp);
      window.addEventListener('pointercancel', onPointerUp);
    };
    const onKeyDown = (event) => {
      const delta = event.key === 'ArrowLeft' ? -24 : event.key === 'ArrowRight' ? 24 : 0;
      if (event.key === 'Home') {
        event.preventDefault();
        setWidth(minWidth);
      } else if (event.key === 'End') {
        event.preventDefault();
        setWidth(maxWidth);
      } else if (delta) {
        event.preventDefault();
        setWidth(currentWidth() + delta);
      }
    };

    handle.style.touchAction = 'none';
    handle.tabIndex = 0;
    handle.setAttribute('aria-valuemin', String(minWidth));
    handle.setAttribute('aria-valuemax', String(maxWidth));
    setWidth(currentWidth());
    handle.addEventListener('pointerdown', onPointerDown);
    handle.addEventListener('keydown', onKeyDown);

    return () => {
      handle.removeEventListener('pointerdown', onPointerDown);
      handle.removeEventListener('keydown', onKeyDown);
      onPointerUp();
    };
  };
  
  const leftWidth = localStorage.getItem(`${cachePrefix}.layout.leftWidth`) || '240';
  const rightWidth = localStorage.getItem(`${cachePrefix}.layout.rightWidth`) || '300';
  containerEl.style.setProperty('--notes-left-width', `${leftWidth}px`);
  containerEl.style.setProperty('--notes-right-width', `${rightWidth}px`);
  const leftCleanup = attachLocalResizer({
    handle: leftResizer,
    pane: leftPane,
    cssVar: '--notes-left-width',
    storageKey: `${cachePrefix}.layout.leftWidth`,
    minWidth: 180,
    maxWidth: 380
  });
  const rightCleanup = attachLocalResizer({
    handle: rightResizer,
    pane: listPane,
    cssVar: '--notes-right-width',
    storageKey: `${cachePrefix}.layout.rightWidth`,
    minWidth: 240,
    maxWidth: 480
  });
  if (leftCleanup) cleanups.push(leftCleanup);
  if (rightCleanup) cleanups.push(rightCleanup);
  
  return () => {
    cleanups.forEach(c => c());
  };
}

function initNotesContextMenu(state) {
  state.contextMenu?.remove();
  const menu = document.createElement('div');
  menu.className = 'ctox-context-menu notes-context-menu';
  menu.hidden = true;
  document.body.append(menu);
  state.contextMenu = menu;

  const handleContextMenu = (event) => {
    if (state.ctx.module?.id !== (state.ctx.module?.id === 'notizen' ? 'notizen' : 'notes')) return;
    const context = noteCommandContextFromElement(state, event.target);
    event.preventDefault();
    event.stopPropagation();
    renderNotesContextMenu(state, context, event.clientX, event.clientY);
  };
  const handleOutsideClick = (event) => {
    if (state.contextMenu?.contains(event.target)) return;
    hideNotesContextMenu(state);
  };
  const handleEscape = (event) => {
    if (event.key === 'Escape') hideNotesContextMenu(state);
  };

  state.ctx.host.addEventListener('contextmenu', handleContextMenu);
  window.addEventListener('click', handleOutsideClick, { capture: true });
  window.addEventListener('keydown', handleEscape);

  return () => {
    state.ctx.host.removeEventListener('contextmenu', handleContextMenu);
    window.removeEventListener('click', handleOutsideClick, { capture: true });
    window.removeEventListener('keydown', handleEscape);
    hideNotesContextMenu(state);
  };
}

function hideNotesContextMenu(state) {
  if (state.contextMenu) state.contextMenu.hidden = true;
}

function canModifyNotesApp(state) {
  if (typeof state.ctx.canModifyModule === 'function' && state.ctx.canModifyModule()) return true;
  const user = state.ctx.session?.user || {};
  const role = String(user.role || (user.is_admin ? 'admin' : 'user')).trim().toLowerCase().replace(/^business_os_/, '');
  return ['admin', 'chef'].includes(role);
}

function noteCommandContextFromElement(state, target) {
  const element = target?.nodeType === Node.ELEMENT_NODE ? target : target?.parentElement;
  const activeNote = state.notes.find((item) => item.id === state.activeNoteId) || null;
  const bodyText = activeNote?.content || '';
  const bodySnippet = bodyText.slice(0, 500);

  return {
    module: state.ctx.module?.id || 'notizen',
    column: state.ctx.left?.contains?.(element) ? 'folders' : (els.notesList?.contains?.(element) ? 'list' : 'editor'),
    record_type: activeNote ? 'notes' : 'module',
    record_id: activeNote?.id || '',
    label: activeNote?.title || '',
    body_snippet: bodySnippet,
    selected_text: String(window.getSelection?.()?.toString?.() || '').trim().slice(0, 1000),
    clicked_text: String(element?.innerText || element?.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 500),
  };
}

function renderNotesContextMenu(state, context, x, y) {
  const canModifyApp = canModifyNotesApp(state);
  const titleLabel = state.ctx.module?.id === 'notizen' ? 'Notizen' : 'Notes';
  state.contextMenu.innerHTML = `
    <form class="notes-context-chat" data-notes-context-chat-form>
      <header>
        <div>
          <strong>${escapeHtml(state.t('chatToCtox', 'Chat to CTOX'))}</strong>
          <span>${escapeHtml(context.label || titleLabel)}</span>
        </div>
        <button type="button" data-notes-context-close aria-label="${escapeHtml(state.t('close', 'Schließen'))}">×</button>
      </header>
      <div class="ctox-context-mode" role="radiogroup" aria-label="${escapeHtml(state.t('chatActionLabel', 'CTOX Aufgabe'))}">
        <label><input type="radio" name="contextMode" value="data" checked /> ${escapeHtml(state.t('chatWorkDataLabel', 'Mit Daten arbeiten'))}</label>
        <label><input type="radio" name="contextMode" value="ask" /> ${escapeHtml(state.t('chatAnswerLabel', 'Frage beantworten'))}</label>
        ${canModifyApp ? `<label><input type="radio" name="contextMode" value="app" /> ${escapeHtml(state.t('chatModifyAppLabel', 'App modifizieren'))}</label>` : ''}
      </div>
      <textarea data-notes-context-message placeholder="${escapeHtml(state.t('chatPlaceholder', 'Was soll CTOX hier tun oder prüfen?'))}"></textarea>
      <footer>
        <span data-notes-context-status></span>
        <button type="submit">${escapeHtml(state.t('send', 'Senden'))}</button>
      </footer>
    </form>
  `;
  state.contextMenu.hidden = false;
  state.contextMenu.style.left = '0px';
  state.contextMenu.style.top = '0px';
  const rect = state.contextMenu.getBoundingClientRect();
  const clampNumber = (val, min, max) => Math.min(max, Math.max(min, val));
  const maxLeft = Math.max(8, window.innerWidth - rect.width - 8);
  const maxTop = Math.max(8, window.innerHeight - rect.height - 8);
  state.contextMenu.style.left = `${clampNumber(x, 8, maxLeft)}px`;
  state.contextMenu.style.top = `${clampNumber(y, 8, maxTop)}px`;

  const form = state.contextMenu.querySelector('[data-notes-context-chat-form]');
  const textarea = state.contextMenu.querySelector('[data-notes-context-message]');
  state.contextMenu.querySelector('[data-notes-context-close]')?.addEventListener('click', () => hideNotesContextMenu(state));
  form?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const mode = new FormData(form).get('contextMode') || 'data';
    await dispatchNotesContextChat(state, context, textarea?.value || '', mode);
  });
  requestAnimationFrame(() => textarea?.focus());
}

async function dispatchNotesContextChat(state, context, message, mode = 'data') {
  const trimmed = String(message || '').trim();
  const status = state.contextMenu?.querySelector('[data-notes-context-status]');
  if (!trimmed) {
    if (status) status.textContent = state.t('chatMissingMessage', 'Nachricht fehlt.');
    return;
  }

  const activeModuleId = state.ctx.module?.id || 'notizen';
  const safeMode = mode === 'app' && canModifyNotesApp(state) ? 'app' : (mode === 'ask' ? 'ask' : 'data');
  const activeNote = state.notes.find((item) => item.id === state.activeNoteId) || null;
  if (!document.querySelector('[data-ctox-chat-root]')) {
    if (status) status.textContent = state.t('chatNotReady', 'Chat ist noch nicht bereit.');
    return;
  }
  if (status) status.textContent = state.t('chatOpening', 'Oeffne Chat...');
  
  const titlePrefix = safeMode === 'app'
    ? (activeModuleId === 'notizen' ? 'Notizen App modifizieren' : 'Notes App modifizieren')
    : safeMode === 'ask'
      ? state.t('chatAnswerLabel', 'Frage beantworten')
      : (activeModuleId === 'notizen' ? 'Notiz bearbeiten' : 'Note edit');
  const title = `${titlePrefix} · ${context.label || (activeModuleId === 'notizen' ? 'Notizen' : 'Notes')}`;
  const instruction = safeMode === 'app'
    ? `Modifiziere die ${activeModuleId === 'notizen' ? 'Notizen' : 'Notes'}-App anhand dieser Admin-Anweisung. Kontext nur als UI-Bezug verwenden, Notizdaten selbst nicht als primäres Ziel verändern.\n\n${trimmed}`
    : safeMode === 'ask'
      ? `Beantworte die folgende Frage ausschließlich lesend. Nutze nur vorhandene Daten und Kontext; führe keine Änderungen an Daten, Records, Dateien oder der App aus. Antworte knapp und direkt.\n\n${trimmed}`
      : trimmed;

  window.dispatchEvent(new CustomEvent('ctox-business-os-chat-submit', {
    detail: {
      text: trimmed,
      module: activeModuleId,
      source_title: activeModuleId === 'notizen' ? 'Notizen' : 'Notes',
      command_type: safeMode === 'app' ? 'ctox.business_os.app.modify' : 'business_os.chat.task',
      record_id: safeMode === 'app' ? activeModuleId : (activeNote?.id || activeModuleId),
      title,
      instruction,
      payload: {
        title,
        instruction,
        prompt: trimmed,
        user_message: trimmed,
        mode: safeMode,
        target: safeMode === 'app' ? 'app' : (safeMode === 'ask' ? 'read' : 'data'),
        selected_note: activeNote,
        context,
        thread_key: `business-os/${activeModuleId}`,
      },
      client_context: {
        action: 'context-chat',
        mode: safeMode,
        column: context.column,
        record_type: context.record_type,
        note_id: activeNote?.id || '',
        note_title: activeNote?.title || '',
      },
    },
  }));
  hideNotesContextMenu(state);
}

function noteActionAvailability(note) {
  if (!note) {
    return {
      canDelete: false,
      canFavorite: false,
      canLock: false,
      deleteMode: 'disabled',
      lockMode: 'disabled'
    };
  }
  if (note[DRAFT_NOTE_MARKER]) {
    return {
      canDelete: true,
      canFavorite: false,
      canLock: false,
      deleteMode: 'discard-draft',
      lockMode: 'save-draft-first'
    };
  }
  return {
    canDelete: true,
    canFavorite: !note.is_locked,
    canLock: !note.is_locked || !!note.decrypted,
    deleteMode: note.is_trashed ? 'confirm-permanent-delete' : 'confirm-trash-with-undo',
    lockMode: note.is_locked ? 'unlock-requires-decryption' : 'prompt-password'
  };
}

export const __notesTestHooks = {
  DRAFT_NOTE_MARKER,
  buildNotesEmptyState,
  createDefaultNotes,
  getCachePrefix,
  isBusinessOsPermissionDenied,
  noteActionAvailability,
  buildNotePersistPayload,
  deriveTitleFromPlainText,
  lockedPlaceholderTitle
};
