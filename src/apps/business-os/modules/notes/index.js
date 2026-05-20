import { loadModuleMessages } from '../../shared/i18n.js';

const SAVE_DEBOUNCE_MS = 250;
const NOTES_RENDER_DEBOUNCE_MS = 50;

const labels = {
  de: {
    allNotes: 'Alle Notizen',
    notes: 'Notizen',
    newNote: 'Neue Notiz',
    untitled: 'Unbenannte Notiz',
    search: 'Suchen...',
    saving: 'Speichern…',
    saved: 'Gespeichert',
    words: 'Wörter',
    deleteConfirm: 'Möchtest du diese Notiz wirklich löschen?',
    newFolderPrompt: 'Name für den neuen Ordner:',
    defaultFolder: 'Notizen',
  },
  en: {
    allNotes: 'All Notes',
    notes: 'Notes',
    newNote: 'New Note',
    untitled: 'Untitled Note',
    search: 'Search...',
    saving: 'Saving…',
    saved: 'Saved',
    words: 'words',
    deleteConfirm: 'Are you sure you want to delete this note?',
    newFolderPrompt: 'Name for the new folder:',
    defaultFolder: 'Notes',
  }
};

const state = {
  ctx: null,
  lang: 'de',
  notes: [],
  folders: ['Notes'],
  activeFolder: 'All Notes',
  activeNoteId: '',
  searchQuery: '',
  activeTab: 'edit',
  saveTimer: null,
  localSubscriptionCleanup: null,
  renderTimer: null,
  t: (key, fallback) => fallback ?? key,
};

const els = {};

export async function mount(ctx) {
  state.ctx = ctx;
  state.lang = ctx.locale === 'en' ? 'en' : 'de';
  
  // Load dynamic locales
  const messages = await loadModuleMessages(import.meta.url, ctx.locale, labels);
  state.t = (key, fallback) => messages[key] ?? fallback ?? key;
  
  // Set up HTML structure
  ctx.host.innerHTML = documentTemplate();
  ctx.left.replaceChildren();
  ctx.right.replaceChildren();
  
  // Apply translation to static labels in mounted DOM
  applyStaticLabels(ctx.host, state.t);
  
  bindElements(ctx.host);
  wireEvents();
  
  // Setup column resizing logic
  const resizerCleanup = setupResizers(ctx.host);
  
  // Load initial notes and wire up RxDB subscription
  await loadNotesFromLocal();
  state.localSubscriptionCleanup = wireLocalRealtime();
  
  // Pre-select first note if available
  if (state.notes.length > 0) {
    selectNote(state.notes[0].id);
  } else {
    renderEditor();
  }
  
  return () => {
    if (state.saveTimer) clearTimeout(state.saveTimer);
    if (state.renderTimer) clearTimeout(state.renderTimer);
    
    state.localSubscriptionCleanup?.();
    state.localSubscriptionCleanup = null;
    
    resizerCleanup();
    unbindEvents();
  };
}

function applyStaticLabels(root, t) {
  root.querySelectorAll('[data-t]').forEach(el => el.textContent = t(el.dataset.t));
  root.querySelectorAll('[data-t-title]').forEach(el => el.title = t(el.dataset.tTitle));
  root.querySelectorAll('[data-t-aria]').forEach(el => el.setAttribute('aria-label', t(el.dataset.tAria)));
  root.querySelectorAll('[data-t-placeholder]').forEach(el => el.placeholder = t(el.dataset.tPlaceholder));
}

function documentTemplate() {
  return document.querySelector('main[data-notes-root]')?.outerHTML || '';
}

function bindElements(host) {
  els.root = host.querySelector('[data-notes-root]');
  els.folderList = host.querySelector('[data-folder-list]');
  els.notesList = host.querySelector('[data-notes-list]');
  els.notesCountLabel = host.querySelector('[data-notes-count-label]');
  els.search = host.querySelector('[data-search]');
  
  els.selectedFolder = host.querySelector('[data-selected-folder]');
  els.selectedTitle = host.querySelector('[data-selected-title]');
  els.editor = host.querySelector('[data-notes-editor]');
  els.preview = host.querySelector('[data-notes-preview]');
  
  els.splitEditor = host.querySelector('[data-notes-split-editor]');
  els.splitPreview = host.querySelector('[data-notes-split-preview]');
  
  els.status = host.querySelector('[data-notes-status]');
  els.words = host.querySelector('[data-notes-words]');
  
  els.tabs = host.querySelectorAll('.segmented button');
  els.deleteBtn = host.querySelector('[data-action="delete-note"]');
  els.createNoteBtn = host.querySelector('[data-action="create-note"]');
  els.createFolderBtn = host.querySelector('[data-action="create-folder"]');
}

function wireEvents() {
  els.search?.addEventListener('input', handleSearch);
  els.editor?.addEventListener('input', handleEditorInput);
  els.splitEditor?.addEventListener('input', handleSplitEditorInput);
  
  els.tabs?.forEach(btn => {
    btn.addEventListener('click', handleTabClick);
  });
  
  els.deleteBtn?.addEventListener('click', handleDeleteNote);
  els.createNoteBtn?.addEventListener('click', handleCreateNote);
  els.createFolderBtn?.addEventListener('click', handleCreateFolder);
}

function unbindEvents() {
  els.search?.removeEventListener('input', handleSearch);
  els.editor?.removeEventListener('input', handleEditorInput);
  els.splitEditor?.removeEventListener('input', handleSplitEditorInput);
  els.deleteBtn?.removeEventListener('click', handleDeleteNote);
  els.createNoteBtn?.removeEventListener('click', handleCreateNote);
  els.createFolderBtn?.removeEventListener('click', handleCreateFolder);
}

async function loadNotesFromLocal() {
  const collection = state.ctx.db?.raw?.notes;
  if (!collection) return;
  
  try {
    const docs = await collection.find({ sort: [{ updated_at_ms: 'desc' }] }).exec();
    state.notes = docs.map(d => d.toJSON()).filter(n => n.id);
    
    // Extract organic folder list from notes
    const folderSet = new Set();
    state.notes.forEach(note => {
      if (note.folder) folderSet.add(note.folder);
    });
    folderSet.add('Notes'); // Always have default folder
    state.folders = Array.from(folderSet);
    
    scheduleRender();
  } catch (error) {
    console.error('[notes] failed to load notes', error);
  }
}

function wireLocalRealtime() {
  const collection = state.ctx.db?.raw?.notes;
  if (!collection) return null;
  
  const sub = collection.$.subscribe(() => {
    loadNotesFromLocal().catch(err => {
      console.warn('[notes] failed to refresh local notes', err);
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
  renderFolders();
  renderNotesList();
  renderEditor();
}

function renderFolders() {
  if (!els.folderList) return;
  
  const allNotesActive = state.activeFolder === 'All Notes';
  
  let html = `
    <div class="notes-folder-item ${allNotesActive ? 'active' : ''}" data-folder-select="All Notes">
      ${state.t('allNotes')}
    </div>
  `;
  
  state.folders.forEach(folder => {
    const active = state.activeFolder === folder;
    html += `
      <div class="notes-folder-item ${active ? 'active' : ''}" data-folder-select="${folder}">
        ${folder === 'Notes' ? state.t('defaultFolder') : folder}
      </div>
    `;
  });
  
  els.folderList.innerHTML = html;
  
  // Bind folder selection clicks
  els.folderList.querySelectorAll('[data-folder-select]').forEach(el => {
    el.addEventListener('click', () => {
      const folder = el.getAttribute('data-folder-select');
      state.activeFolder = folder;
      scheduleRender();
    });
  });
}

function renderNotesList() {
  if (!els.notesList) return;
  
  const query = state.searchQuery.toLowerCase().trim();
  
  // Filter by folder
  let filtered = state.notes;
  if (state.activeFolder !== 'All Notes') {
    filtered = filtered.filter(n => n.folder === state.activeFolder);
  }
  
  // Filter by search query
  if (query) {
    filtered = filtered.filter(n => 
      (n.title && n.title.toLowerCase().includes(query)) ||
      (n.content && n.content.toLowerCase().includes(query))
    );
  }
  
  // Update header count label
  if (els.notesCountLabel) {
    if (state.activeFolder === 'All Notes') {
      els.notesCountLabel.textContent = state.t('allNotes');
    } else {
      els.notesCountLabel.textContent = state.activeFolder === 'Notes' ? state.t('defaultFolder') : state.activeFolder;
    }
  }
  
  if (filtered.length === 0) {
    els.notesList.innerHTML = `<div style="padding: 20px; font-size:12px; color: var(--text-muted); text-align:center;">${state.t('noNotes')}</div>`;
    return;
  }
  
  let html = '';
  filtered.forEach(note => {
    const active = note.id === state.activeNoteId;
    const dateStr = formatTimestamp(note.updated_at_ms);
    const snippet = extractSnippet(note.content, note.title);
    
    html += `
      <div class="notes-card ${active ? 'active' : ''}" data-note-id="${note.id}">
        <div class="notes-card-title">${escapeHtml(note.title || state.t('untitled'))}</div>
        <div class="notes-card-meta">
          <span class="notes-card-date">${dateStr}</span>
          <span class="notes-card-snippet">${escapeHtml(snippet)}</span>
        </div>
      </div>
    `;
  });
  
  els.notesList.innerHTML = html;
  
  // Bind note card clicks
  els.notesList.querySelectorAll('[data-note-id]').forEach(el => {
    el.addEventListener('click', () => {
      const id = el.getAttribute('data-note-id');
      selectNote(id);
    });
  });
}

function renderEditor() {
  const note = state.notes.find(n => n.id === state.activeNoteId);
  
  // Toggle delete button availability
  if (els.deleteBtn) {
    els.deleteBtn.disabled = !note;
  }
  
  // Toggle segmented buttons state
  els.tabs?.forEach(btn => {
    const tabName = btn.getAttribute('data-tab');
    btn.setAttribute('aria-pressed', tabName === state.activeTab ? 'true' : 'false');
  });
  
  // Toggle workspace views
  const panels = els.root.querySelectorAll('.notes-tab-panel');
  panels.forEach(p => {
    const panelName = p.getAttribute('data-panel');
    p.hidden = panelName !== state.activeTab;
  });
  
  if (!note) {
    if (els.selectedFolder) els.selectedFolder.textContent = '';
    if (els.selectedTitle) els.selectedTitle.textContent = state.t('untitled');
    if (els.editor) els.editor.value = '';
    if (els.splitEditor) els.splitEditor.value = '';
    if (els.preview) els.preview.innerHTML = '';
    if (els.splitPreview) els.splitPreview.innerHTML = '';
    if (els.words) els.words.textContent = `0 ${state.t('words')}`;
    return;
  }
  
  if (els.selectedFolder) els.selectedFolder.textContent = note.folder || state.t('defaultFolder');
  if (els.selectedTitle) els.selectedTitle.textContent = note.title || state.t('untitled');
  
  // Word count
  const wordCount = countWords(note.content);
  if (els.words) els.words.textContent = `${wordCount} ${state.t('words')}`;
  
  // Render content depending on active tab
  if (state.activeTab === 'edit') {
    if (els.editor && els.editor.value !== note.content) {
      els.editor.value = note.content || '';
    }
  } else if (state.activeTab === 'preview') {
    if (els.preview) {
      els.preview.innerHTML = markdownToHtml(note.content || '');
    }
  } else if (state.activeTab === 'split') {
    if (els.splitEditor && els.splitEditor.value !== note.content) {
      els.splitEditor.value = note.content || '';
    }
    if (els.splitPreview) {
      els.splitPreview.innerHTML = markdownToHtml(note.content || '');
    }
  }
}

function selectNote(id) {
  state.activeNoteId = id;
  // Reset tabs to edit mode upon selection
  state.activeTab = 'edit';
  scheduleRender();
}

function handleSearch(e) {
  state.searchQuery = e.target.value;
  scheduleRender();
}

function handleEditorInput(e) {
  processContentInput(e.target.value);
}

function handleSplitEditorInput(e) {
  processContentInput(e.target.value);
}

function processContentInput(newContent) {
  const note = state.notes.find(n => n.id === state.activeNoteId);
  if (!note) return;
  
  // Calculate new title from the first line
  let newTitle = '';
  for (const line of newContent.lines ? newContent.lines() : newContent.split('\n')) {
    const trimmed = line.trim();
    if (trimmed) {
      newTitle = trimmed.replace(/^#+\s+/, '').trim();
      break;
    }
  }
  if (!newTitle) {
    newTitle = state.t('untitled');
  }
  
  // Optimistically update local memory state for high-end instant response
  note.content = newContent;
  note.title = newTitle;
  note.updated_at_ms = Date.now();
  
  // Update header and snippet immediately in UI
  if (els.selectedTitle) els.selectedTitle.textContent = newTitle;
  const card = els.notesList?.querySelector(`[data-note-id="${note.id}"]`);
  if (card) {
    const titleEl = card.querySelector('.notes-card-title');
    const snippetEl = card.querySelector('.notes-card-snippet');
    const dateEl = card.querySelector('.notes-card-date');
    
    if (titleEl) titleEl.textContent = newTitle;
    if (snippetEl) snippetEl.textContent = extractSnippet(newContent, newTitle);
    if (dateEl) dateEl.textContent = formatTimestamp(note.updated_at_ms);
  }
  
  if (els.words) {
    els.words.textContent = `${countWords(newContent)} ${state.t('words')}`;
  }
  
  if (state.activeTab === 'split' && els.splitPreview) {
    els.splitPreview.innerHTML = markdownToHtml(newContent);
  }
  
  // Debounce the RxDB write-back to server
  if (els.status) els.status.textContent = state.t('saving');
  if (state.saveTimer) clearTimeout(state.saveTimer);
  
  state.saveTimer = setTimeout(() => {
    state.saveTimer = null;
    commitSave(note.id, newTitle, newContent).catch(err => {
      console.error('[notes] autosave failed', err);
      if (els.status) els.status.textContent = 'Save failed';
    });
  }, SAVE_DEBOUNCE_MS);
}

async function commitSave(noteId, title, content) {
  const collection = state.ctx.db?.raw?.notes;
  if (!collection) return;
  
  const doc = await collection.findOne(noteId).exec();
  if (doc) {
    await doc.patch({
      title,
      content,
      updated_at_ms: Date.now()
    });
    if (els.status) els.status.textContent = state.t('saved');
  }
}

async function handleCreateNote() {
  const collection = state.ctx.db?.raw?.notes;
  if (!collection) return;
  
  const folder = state.activeFolder === 'All Notes' ? 'Notes' : state.activeFolder;
  const newId = generateUUID();
  const title = state.t('newNote');
  const content = `# ${state.t('newNote')}\n\n`;
  
  try {
    await collection.insert({
      id: newId,
      title,
      content,
      folder,
      updated_at_ms: Date.now()
    });
    
    // Select the new note
    state.activeNoteId = newId;
    state.activeTab = 'edit';
    await loadNotesFromLocal();
  } catch (error) {
    console.error('[notes] failed to create note', error);
  }
}

async function handleDeleteNote() {
  const note = state.notes.find(n => n.id === state.activeNoteId);
  if (!note) return;
  
  const confirmMessage = state.t('deleteConfirm');
  if (!confirm(confirmMessage)) return;
  
  const collection = state.ctx.db?.raw?.notes;
  if (!collection) return;
  
  try {
    const doc = await collection.findOne(note.id).exec();
    if (doc) {
      await doc.remove();
    }
    
    state.activeNoteId = '';
    await loadNotesFromLocal();
    
    // Select another note
    if (state.notes.length > 0) {
      selectNote(state.notes[0].id);
    } else {
      scheduleRender();
    }
  } catch (error) {
    console.error('[notes] failed to delete note', error);
  }
}

async function handleCreateFolder() {
  const name = prompt(state.t('newFolderPrompt'));
  if (!name || !name.trim()) return;
  
  const folderName = name.trim();
  
  // Set active folder to new folder
  state.activeFolder = folderName;
  if (!state.folders.includes(folderName)) {
    state.folders.push(folderName);
  }
  
  // Create an initial note inside the new folder to make it persist
  const collection = state.ctx.db?.raw?.notes;
  if (collection) {
    const newId = generateUUID();
    const title = state.t('newNote');
    const content = `# ${state.t('newNote')}\n\n`;
    try {
      await collection.insert({
        id: newId,
        title,
        content,
        folder: folderName,
        updated_at_ms: Date.now()
      });
      state.activeNoteId = newId;
      state.activeTab = 'edit';
      await loadNotesFromLocal();
    } catch (e) {
      console.error(e);
    }
  }
}

function handleTabClick(e) {
  const tabName = e.target.getAttribute('data-tab');
  state.activeTab = tabName;
  scheduleRender();
}

/* Helper functions */

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

function extractSnippet(content, title) {
  if (!content) return '';
  const lines = content.split('\n');
  
  // Skip first line if it's the title
  let startIndex = 0;
  for (let i = 0; i < lines.length; i++) {
    const trimmed = lines[i].trim();
    if (trimmed) {
      const stripped = trimmed.replace(/^#+\s+/, '').trim();
      if (stripped === title) {
        startIndex = i + 1;
      }
      break;
    }
  }
  
  for (let i = startIndex; i < lines.length; i++) {
    const line = lines[i].trim();
    if (line) return line.replace(/^#+\s+/, '').slice(0, 80);
  }
  
  return '';
}

function countWords(str) {
  if (!str) return 0;
  return str.trim().split(/\s+/).filter(Boolean).length;
}

function markdownToHtml(markdown) {
  const lines = String(markdown || '').replace(/\r\n/g, '\n').split('\n');
  const html = [];
  let paragraph = [];
  let list = false;
  let code = null;
  
  const flushParagraph = () => {
    if (paragraph.length) {
      html.push(`<p>${inlineMarkdown(paragraph.join(' '))}</p>`);
      paragraph = [];
    }
  };
  const closeList = () => {
    if (list) {
      html.push('</ul>');
      list = false;
    }
  };
  
  for (const line of lines) {
    if (line.startsWith('```')) {
      flushParagraph();
      closeList();
      if (code) {
        html.push(`<pre><code>${escapeHtml(code.join('\n'))}</code></pre>`);
        code = null;
      } else {
        code = [];
      }
      continue;
    }
    
    if (code) {
      code.push(line);
      continue;
    }
    
    if (!line.trim()) {
      flushParagraph();
      closeList();
      continue;
    }
    
    const heading = /^(#{1,3})\s+(.+)$/.exec(line);
    if (heading) {
      flushParagraph();
      closeList();
      html.push(`<h${heading[1].length}>${inlineMarkdown(heading[2])}</h${heading[1].length}>`);
      continue;
    }
    
    const bullet = /^[-*]\s+(.+)$/.exec(line);
    if (bullet) {
      flushParagraph();
      if (!list) {
        html.push('<ul>');
        list = true;
      }
      html.push(`<li>${inlineMarkdown(bullet[1])}</li>`);
      continue;
    }
    
    paragraph.push(line.trim());
  }
  
  flushParagraph();
  closeList();
  return html.join('\n');
}

function inlineMarkdown(value) {
  return escapeHtml(value)
    .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
    .replace(/`(.+?)`/g, '<code>$1</code>');
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
  const leftPane = host.querySelector('.notes-left');
  const rightPane = host.querySelector('.notes-right');
  const leftResizer = host.querySelector('[data-resizer="left"]');
  const rightResizer = host.querySelector('[data-resizer="right"]');
  
  if (!leftPane || !rightPane || !leftResizer || !rightResizer) return () => {};
  
  // Load saved widths
  let leftWidth = parseInt(localStorage.getItem('ctox.notes.layout.leftWidth') || '380', 10);
  let rightWidth = parseInt(localStorage.getItem('ctox.notes.layout.rightWidth') || '240', 10);
  
  // Apply initial sizes
  const applyWidths = () => {
    leftPane.style.width = `${leftWidth}px`;
    leftPane.style.flex = `0 0 ${leftWidth}px`;
    rightPane.style.width = `${rightWidth}px`;
    rightPane.style.flex = `0 0 ${rightWidth}px`;
  };
  
  applyWidths();
  
  let activeResizer = null;
  let startX = 0;
  let startWidth = 0;
  
  const onPointerDown = (e) => {
    activeResizer = e.currentTarget.getAttribute('data-resizer');
    startX = e.clientX;
    if (activeResizer === 'left') {
      startWidth = leftWidth;
      leftResizer.classList.add('is-dragging');
    } else {
      startWidth = rightWidth;
      rightResizer.classList.add('is-dragging');
    }
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
    e.preventDefault();
  };
  
  const onPointerMove = (e) => {
    if (!activeResizer) return;
    const deltaX = e.clientX - startX;
    
    if (activeResizer === 'left') {
      const newWidth = Math.min(500, Math.max(280, startWidth + deltaX));
      leftWidth = newWidth;
    } else {
      const newWidth = Math.min(320, Math.max(180, startWidth - deltaX));
      rightWidth = newWidth;
    }
    
    applyWidths();
  };
  
  const onPointerUp = () => {
    if (!activeResizer) return;
    
    if (activeResizer === 'left') {
      leftResizer.classList.remove('is-dragging');
      localStorage.setItem('ctox.notes.layout.leftWidth', leftWidth);
    } else {
      rightResizer.classList.remove('is-dragging');
      localStorage.setItem('ctox.notes.layout.rightWidth', rightWidth);
    }
    
    activeResizer = null;
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
  };
  
  leftResizer.addEventListener('pointerdown', onPointerDown);
  rightResizer.addEventListener('pointerdown', onPointerDown);
  window.addEventListener('pointermove', onPointerMove);
  window.addEventListener('pointerup', onPointerUp);
  window.addEventListener('pointercancel', onPointerUp);
  
  return () => {
    leftResizer.removeEventListener('pointerdown', onPointerDown);
    rightResizer.removeEventListener('pointerdown', onPointerDown);
    window.removeEventListener('pointermove', onPointerMove);
    window.removeEventListener('pointerup', onPointerUp);
    window.removeEventListener('pointercancel', onPointerUp);
  };
}
