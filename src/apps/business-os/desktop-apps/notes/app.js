const STORAGE_KEY = 'ctox.businessOs.notes.draft';
const SAVE_DEBOUNCE_MS = 350;

export const manifest = {
  id: 'notes',
  title: 'Notes',
  glyph: '📝',
  defaultWidth: 520,
  defaultHeight: 400,
};

export async function mount(container, ctx) {
  ensureStyles();
  const initialText = readDraft();
  container.innerHTML = `
    <div class="app-notes">
      <textarea class="app-notes-area" data-notes-area aria-label="Notiz" placeholder="Schreib hier…"></textarea>
      <div class="app-notes-status" data-notes-status></div>
    </div>
  `;

  const textarea = container.querySelector('[data-notes-area]');
  const status = container.querySelector('[data-notes-status]');
  textarea.value = initialText;

  let saveTimer = null;

  function setStatus(text) {
    status.textContent = text;
  }

  function commitSave() {
    saveTimer = null;
    try {
      localStorage.setItem(STORAGE_KEY, textarea.value);
      setStatus(`Gespeichert · ${new Date().toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', second: '2-digit' })}`);
    } catch (error) {
      console.error('[notes] save failed:', error);
      setStatus('Speichern fehlgeschlagen');
    }
  }

  function onInput() {
    setStatus('Schreiben…');
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(commitSave, SAVE_DEBOUNCE_MS);
  }

  textarea.addEventListener('input', onInput);

  if (initialText.length) {
    setStatus(`${initialText.length} Zeichen`);
  } else {
    setStatus('Leer');
  }

  setTimeout(() => textarea.focus(), 50);

  return () => {
    if (saveTimer) {
      clearTimeout(saveTimer);
      commitSave();
    }
    textarea.removeEventListener('input', onInput);
  };
}

function readDraft() {
  try {
    return localStorage.getItem(STORAGE_KEY) || '';
  } catch {
    return '';
  }
}

function ensureStyles() {
  if (document.getElementById('app-notes-styles')) return;
  const style = document.createElement('style');
  style.id = 'app-notes-styles';
  style.textContent = `
    .app-notes { display: flex; flex-direction: column; height: 100%; min-height: 0; }
    .app-notes-area {
      flex: 1 1 auto;
      min-height: 0;
      border: 0;
      outline: 0;
      resize: none;
      padding: 14px;
      background: var(--surface);
      color: var(--text);
      font: inherit;
      font-size: 13px;
      line-height: 1.55;
    }
    .app-notes-area::placeholder { color: var(--muted); }
    .app-notes-status {
      flex: 0 0 auto;
      padding: 6px 12px;
      border-top: 1px solid var(--hairline, var(--line));
      background: var(--surface-2);
      color: var(--muted);
      font-size: 11px;
    }
  `;
  document.head.appendChild(style);
}
