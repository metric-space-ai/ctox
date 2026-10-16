// Code-Editor – Editor-Bibliothek des Quelltext-Modus (Shell v4, Strang UC4). Port-Protokoll: V4-PORT.md.
//
// Keine App mehr (DESKTOP_APPS = []), kein Datenweg: Dateien kommen als Eingabe, Änderungen gehen per onChange an den
// Aufrufer (v4/source: Entwurf → draft.publish). Keine Polls, kein registry.json, keine Befehle, kein Warten.
//
//   createCodeEditor({ root, files, onChange, readOnly, activePath, prefer, labels }) → Promise<CodeEditor>
//     files     [{ path, content, language? }]
//     onChange  (path, content, { dirty }) => void   – je Eingabe (Monaco/textarea)
//     readOnly  boolean
//     prefer    'auto' (Monaco aus ctox:lib/monaco@0, sonst textarea) | 'textarea'
//   CodeEditor = { kind, open(path), activePath(), getValue(path?), getFiles(), setFiles(files), setReadOnly(on),
//                  isDirty(path?), revert(path?), format(path?), toggleDiff(on?), focus(), snapshot(), destroy() }
//   snapshot() = { v:1, activePath, search, diffOpen, drafts: {path: content} }  – für AppMountHandle.snapshot des Aufrufers.

const MONACO_TIMEOUT_MS = 6000;
const DEFAULT_LABELS = Object.freeze({
  search: 'Dateien suchen',
  files: 'Quelltext-Dateien',
  diff: 'Diff',
  format: 'Format',
  revert: 'Verwerfen',
});

let monacoPromise = null;

export async function createCodeEditor({ root, files = [], onChange, readOnly = false, activePath = '', prefer = 'auto', labels = {}, restored = null } = {}) {
  if (!root) throw new Error('createCodeEditor: root fehlt');
  const L = { ...DEFAULT_LABELS, ...labels };
  const snap = restored && typeof restored === 'object' ? restored : {};
  const state = {
    files: normalizeSourceFiles(files),
    activePath: '',
    search: typeof snap.search === 'string' ? snap.search : '',
    readOnly: Boolean(readOnly),
    diffOpen: Boolean(snap.diffOpen),
    destroyed: false,
  };
  for (const [path, content] of Object.entries(snap.drafts || {})) {
    const file = state.files.find((f) => f.path === path);
    if (file && typeof content === 'string') { file.draft_content = content; file.dirty = content !== file.content; }
  }

  // Hülle trägt den CSS-Scope der Bibliothek ([data-app="code-editor"], scope-css); Stylesheet liegt im Fenster.
  const el = document.createElement('div');
  el.dataset.app = 'code-editor';
  el.className = 'ctox-code-editor-scope';
  el.style.cssText = 'height:100%;min-height:0;position:relative;container-type:inline-size';
  el.innerHTML = `
    <link rel="stylesheet" href="${new URL('./code-editor.css', import.meta.url).href}">
    <section class="ctox-code-editor">
    <aside class="ctox-code-editor__side">
      <label class="ctox-code-editor__search"><span aria-hidden="true">⌕</span><input type="search" data-ce-search autocomplete="off"></label>
      <nav class="ctox-code-editor__files" data-ce-files></nav>
    </aside>
    <main class="ctox-code-editor__main">
      <header class="ctox-code-editor__bar">
        <strong data-ce-active></strong><span data-ce-detail></span>
        <span class="ctox-code-editor__spacer"></span>
        <button type="button" data-ce-diff></button>
        <button type="button" data-ce-format></button>
        <button type="button" data-ce-revert></button>
      </header>
      <div class="ctox-code-editor__work">
        <div class="ctox-code-editor__host" data-ce-host></div>
        <aside class="ctox-code-editor__diff" data-ce-diff-panel hidden></aside>
      </div>
    </main>
    </section>`;
  root.append(el);
  const refs = {
    search: el.querySelector('[data-ce-search]'),
    files: el.querySelector('[data-ce-files]'),
    active: el.querySelector('[data-ce-active]'),
    detail: el.querySelector('[data-ce-detail]'),
    diff: el.querySelector('[data-ce-diff]'),
    format: el.querySelector('[data-ce-format]'),
    revert: el.querySelector('[data-ce-revert]'),
    host: el.querySelector('[data-ce-host]'),
    diffPanel: el.querySelector('[data-ce-diff-panel]'),
  };
  refs.search.placeholder = L.search;
  refs.files.setAttribute('aria-label', L.files);
  refs.diff.textContent = L.diff;
  refs.format.textContent = L.format;
  refs.revert.textContent = L.revert;
  refs.search.value = state.search;

  const surface = await createSurface(refs.host, { prefer, readOnly: state.readOnly, onInput: handleInput });
  let silent = false;

  refs.search.addEventListener('input', () => { state.search = refs.search.value; renderFiles(); });
  refs.diff.addEventListener('click', () => api.toggleDiff());
  refs.format.addEventListener('click', () => { try { api.format(); } catch { /* ungültiges JSON: Inhalt bleibt */ } });
  refs.revert.addEventListener('click', () => api.revert());

  function current() { return state.files.find((f) => f.path === state.activePath) || null; }
  function valueOf(file) { return file ? (file.draft_content ?? file.content ?? '') : ''; }

  function handleInput(value) {
    if (silent || state.destroyed) return;
    const file = current();
    if (!file) return;
    file.draft_content = value;
    file.dirty = value !== (file.content ?? '');
    renderFiles();
    renderBar();
    if (state.diffOpen) renderDiff();
    onChange?.(file.path, value, { dirty: file.dirty });
  }

  function setSurfaceValue(text, language) {
    silent = true;
    try { surface.setValue(text, language); } finally { silent = false; }
  }

  function renderFiles() {
    const rows = filterSourceFiles(state.files, state.search);
    refs.files.replaceChildren(...rows.map((file) => {
      const b = document.createElement('button');
      b.type = 'button';
      b.className = 'ctox-code-editor__file';
      b.dataset.path = file.path;
      b.classList.toggle('is-active', file.path === state.activePath);
      b.classList.toggle('is-dirty', Boolean(file.dirty));
      b.setAttribute('aria-current', file.path === state.activePath ? 'true' : 'false');
      b.textContent = file.path;
      b.addEventListener('click', () => api.open(file.path));
      return b;
    }));
  }

  function renderBar() {
    const file = current();
    refs.active.textContent = file ? shortName(file.path) : '';
    refs.detail.textContent = file ? `${file.language}${file.dirty ? ' · ●' : ''}` : '';
    const state2 = sourceEditorActionState({ hasFile: Boolean(file), dirty: Boolean(file?.dirty), readOnly: state.readOnly });
    refs.format.disabled = !state2.canFormat;
    refs.revert.disabled = !state2.canRevert;
    refs.diff.disabled = !state2.canDiff;
    refs.diff.setAttribute('aria-pressed', state.diffOpen ? 'true' : 'false');
  }

  function renderDiff() {
    const file = current();
    refs.diffPanel.hidden = !state.diffOpen || !file;
    if (refs.diffPanel.hidden) { refs.diffPanel.replaceChildren(); return; }
    const diff = buildLineDiff(file.content ?? '', valueOf(file));
    const head = document.createElement('div');
    head.className = 'ctox-code-editor__diff-head';
    head.textContent = `+${diff.added} −${diff.removed}`;
    const pre = document.createElement('pre');
    for (const row of diff.rows) {
      const line = document.createElement('div');
      line.className = `is-${row.type}`;
      line.textContent = `${row.type === 'add' ? '+' : '−'} ${row.text}`;
      pre.append(line);
    }
    refs.diffPanel.replaceChildren(head, pre);
  }

  const api = {
    kind: surface.kind,
    open(path) {
      const file = state.files.find((f) => f.path === path);
      if (!file) return false;
      state.activePath = file.path;
      setSurfaceValue(valueOf(file), file.language);
      renderFiles(); renderBar(); renderDiff();
      return true;
    },
    activePath: () => state.activePath,
    getValue(path = state.activePath) { return valueOf(state.files.find((f) => f.path === path)); },
    getFiles() { return state.files.map((f) => ({ path: f.path, content: valueOf(f), language: f.language, dirty: Boolean(f.dirty) })); },
    setFiles(next) {
      const drafts = new Map(state.files.filter((f) => f.dirty).map((f) => [f.path, f.draft_content]));
      state.files = normalizeSourceFiles(next);
      for (const file of state.files) if (drafts.has(file.path)) { file.draft_content = drafts.get(file.path); file.dirty = file.draft_content !== file.content; }
      if (!api.open(state.activePath)) api.open(preferredInitialPath(state.files));
      if (!state.files.length) { state.activePath = ''; setSurfaceValue('', 'plaintext'); renderFiles(); renderBar(); renderDiff(); }
    },
    setReadOnly(on) { state.readOnly = Boolean(on); surface.setReadOnly(state.readOnly); renderBar(); },
    isDirty(path) { return path ? Boolean(state.files.find((f) => f.path === path)?.dirty) : state.files.some((f) => f.dirty); },
    revert(path = state.activePath) {
      const file = state.files.find((f) => f.path === path);
      if (!file || !file.dirty) return;
      file.draft_content = null; file.dirty = false;
      if (path === state.activePath) setSurfaceValue(file.content ?? '', file.language);
      renderFiles(); renderBar(); renderDiff();
      onChange?.(file.path, file.content ?? '', { dirty: false });
    },
    format(path = state.activePath) {
      const file = state.files.find((f) => f.path === path);
      if (!file || state.readOnly) return;
      const next = formatSourceContent(valueOf(file), file.language, file.path);
      if (path === state.activePath) surface.setValue(next, file.language); else handleInput(next);
      if (path === state.activePath && surface.kind === 'textarea') handleInput(next);
    },
    toggleDiff(on = !state.diffOpen) { state.diffOpen = Boolean(on); renderBar(); renderDiff(); },
    focus: () => surface.focus(),
    snapshot() {
      const drafts = {};
      for (const f of state.files) if (f.dirty) drafts[f.path] = f.draft_content;
      return { v: 1, activePath: state.activePath, search: state.search, diffOpen: state.diffOpen, drafts };
    },
    destroy() { state.destroyed = true; surface.destroy(); el.remove(); },
  };

  const startPath = [snap.activePath, activePath].find((p) => p && state.files.some((f) => f.path === p)) || preferredInitialPath(state.files);
  if (!api.open(startPath)) { renderFiles(); renderBar(); }
  return api;
}

async function createSurface(host, { prefer, readOnly, onInput }) {
  if (prefer !== 'textarea') {
    try {
      const monaco = await Promise.race([
        loadMonaco(),
        new Promise((_, reject) => setTimeout(() => reject(new Error('monaco timeout')), MONACO_TIMEOUT_MS)),
      ]);
      const el = document.createElement('div');
      el.className = 'ctox-code-editor__monaco';
      host.append(el);
      const dark = matchMedia('(prefers-color-scheme: dark)').matches;
      const ed = monaco.editor.create(el, {
        value: '', language: 'plaintext', readOnly, automaticLayout: true, minimap: { enabled: false }, fontSize: 12,
        scrollBeyondLastLine: false, theme: dark ? 'business-os-dark' : 'vs', tabSize: 2,
      });
      ed.onDidChangeModelContent(() => onInput(ed.getValue()));
      return {
        kind: 'monaco',
        setValue(text, language) {
          const model = ed.getModel();
          ed.setValue(text ?? '');
          if (model && language) monaco.editor.setModelLanguage(model, monacoLanguage(language));
        },
        setReadOnly(on) { ed.updateOptions({ readOnly: on }); },
        focus: () => ed.focus(),
        destroy() { const m = ed.getModel(); ed.dispose(); m?.dispose(); el.remove(); },
      };
    } catch { /* Rückfall textarea */ }
  }
  const wrap = document.createElement('div');
  wrap.className = 'ctox-code-editor__fallback';
  const gutter = document.createElement('pre');
  gutter.className = 'ctox-code-editor__lines';
  gutter.setAttribute('aria-hidden', 'true');
  const ta = document.createElement('textarea');
  ta.className = 'ctox-code-editor__text';
  ta.spellcheck = false;
  ta.wrap = 'off';
  ta.setAttribute('autocapitalize', 'off');
  ta.setAttribute('autocomplete', 'off');
  ta.readOnly = readOnly;
  let lines = 0;
  const renderGutter = () => {
    const n = ta.value.split('\n').length;
    if (n === lines) return;
    lines = n;
    gutter.textContent = Array.from({ length: n }, (_, i) => i + 1).join('\n');
  };
  ta.addEventListener('scroll', () => { gutter.scrollTop = ta.scrollTop; });
  ta.addEventListener('input', () => { renderGutter(); onInput(ta.value); });
  ta.addEventListener('keydown', (event) => {
    if (event.key === 'Tab' && !event.metaKey && !event.ctrlKey && !ta.readOnly) {
      event.preventDefault();
      ta.setRangeText('  ', ta.selectionStart, ta.selectionEnd, 'end');
      ta.dispatchEvent(new Event('input'));
    }
  });
  wrap.append(gutter, ta);
  host.append(wrap);
  renderGutter();
  return {
    kind: 'textarea',
    setValue(text) { ta.value = text ?? ''; renderGutter(); },
    setReadOnly(on) { ta.readOnly = on; },
    focus: () => ta.focus(),
    destroy: () => wrap.remove(),
  };
}

function loadMonaco() {
  if (globalThis.monaco?.editor) {
    defineBusinessTheme(globalThis.monaco);
    return Promise.resolve(globalThis.monaco);
  }
  if (monacoPromise) return monacoPromise;
  monacoPromise = (async () => {
    const monacoBaseUrl = resolveMonacoBaseUrl();
    const loaderUrl = `${monacoBaseUrl}vs/loader.js`;
    globalThis.MonacoEnvironment = {
      getWorkerUrl() {
        const worker = `self.MonacoEnvironment = { baseUrl: '${monacoBaseUrl}' }; importScripts('${monacoBaseUrl}vs/base/worker/workerMain.js');`;
        return `data:text/javascript;charset=utf-8,${encodeURIComponent(worker)}`;
      },
    };
    if (!globalThis.require?.config) {
      await verifyJavaScriptAsset(loaderUrl);
      await new Promise((resolve, reject) => {
        const script = document.createElement('script');
        script.src = loaderUrl;
        script.onload = resolve;
        script.onerror = () => reject(new Error(`failed to load ${loaderUrl}`));
        document.head.append(script);
      });
    }
    return new Promise((resolve, reject) => {
      globalThis.require.config({ paths: { vs: `${monacoBaseUrl}vs` } });
      globalThis.require(['vs/editor/editor.main'], () => { defineBusinessTheme(globalThis.monaco); resolve(globalThis.monaco); }, reject);
    });
  })().catch((error) => { monacoPromise = null; throw error; });
  return monacoPromise;
}

/** Monaco ist Standardbibliothek der Shell (v4/tools/isolation/standard-libs.json). Der Service Worker schreibt das
 *  Präfix auf /~a/shell@<ver>/lib/monaco@0/ um; ohne ihn bleibt es unauflösbar und der Editor nimmt die textarea. */
export const MONACO_LIB_BASE = 'ctox:lib/monaco@0/';

/** Absolute Basis-URL von Monaco. */
export function resolveMonacoBaseUrl(base = MONACO_LIB_BASE, origin = globalThis.location?.origin || '') {
  try {
    return new URL(base, origin || undefined).href;
  } catch (_error) {
    return base;
  }
}

export function isJavaScriptMime(contentType) {
  const mime = String(contentType || '').split(';')[0].trim().toLowerCase();
  return !mime
    || ['application/javascript', 'text/javascript', 'application/ecmascript', 'text/ecmascript', 'application/x-javascript', 'application/octet-stream'].includes(mime);
}

async function verifyJavaScriptAsset(url) {
  const response = await fetch(url, { method: 'HEAD' });
  const contentType = response.headers.get('content-type') || '';
  if (!response.ok) throw new Error(`Monaco loader ${response.status}: ${url}`);
  if (!isJavaScriptMime(contentType)) throw new Error(`Monaco loader MIME ${contentType || 'unbekannt'}: ${url}`);
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

export function normalizeSourceFiles(files) {
  return (Array.isArray(files) ? files : [])
    .filter((file) => file && !file._deleted && !file.is_deleted && typeof file.path === 'string' && file.path.trim())
    .map((file) => ({
      ...file,
      path: file.path.trim(),
      content: typeof file.content === 'string' ? file.content : '',
      language: file.language || languageFromPath(file.path.trim()),
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

export function sourceModelPath(moduleId, filePath) {
  const moduleSegment = String(moduleId || 'unselected').trim().replace(/^\/+|\/+$/g, '') || 'unselected';
  const fileSegment = String(filePath || 'empty.txt').trim().replace(/\\/g, '/').replace(/^\/+/, '') || 'empty.txt';
  return `/${moduleSegment}/${fileSegment}`.replace(/\/{2,}/g, '/');
}

/** Knopfzustände der Editorleiste (Speichern/Commit/History/Agent gehören dem Aufrufer, v4/source). */
export function sourceEditorActionState({ hasFile = false, dirty = false, readOnly = false } = {}) {
  return {
    canFormat: hasFile && !readOnly,
    canRevert: hasFile && dirty && !readOnly,
    canDiff: hasFile,
  };
}

export function buildLineDiff(beforeRaw, afterRaw) {
  const before = String(beforeRaw ?? '').split('\n');
  const after = String(afterRaw ?? '').split('\n');
  let start = 0;
  while (start < before.length && start < after.length && before[start] === after[start]) start += 1;
  let beforeEnd = before.length - 1;
  let afterEnd = after.length - 1;
  while (beforeEnd >= start && afterEnd >= start && before[beforeEnd] === after[afterEnd]) { beforeEnd -= 1; afterEnd -= 1; }
  const rows = [];
  for (let index = start; index <= beforeEnd; index += 1) rows.push({ type: 'remove', text: before[index] });
  for (let index = start; index <= afterEnd; index += 1) rows.push({ type: 'add', text: after[index] });
  return { rows: rows.slice(0, 400), added: Math.max(0, afterEnd - start + 1), removed: Math.max(0, beforeEnd - start + 1) };
}

export function formatSourceContent(value, language, path = '') {
  const source = String(value ?? '');
  const normalizedLanguage = language || languageFromPath(path);
  if (normalizedLanguage === 'json') return `${JSON.stringify(JSON.parse(source), null, 2)}\n`;
  return source
    .split('\n')
    .map((line) => line.replace(/[ \t]+$/g, ''))
    .join('\n')
    .replace(/\n+$/g, '\n')
    .replace(/[^\n]$/g, (char) => `${char}\n`);
}

function preferredInitialPath(files) {
  return files.find((file) => file.path === 'index.js')?.path
    || files.find((file) => file.path === 'module.json')?.path
    || files[0]?.path
    || '';
}

function shortName(path) {
  return String(path || '').split('/').pop() || path || '';
}

function monacoLanguage(language) {
  if (language === 'text') return 'plaintext';
  return language || 'plaintext';
}

export function languageFromPath(path) {
  const ext = String(path || '').split('.').pop()?.toLowerCase();
  if (ext === 'js' || ext === 'mjs' || ext === 'cjs') return 'javascript';
  if (ext === 'ts' || ext === 'tsx') return 'typescript';
  if (ext === 'css') return 'css';
  if (ext === 'html') return 'html';
  if (ext === 'json') return 'json';
  if (ext === 'md') return 'markdown';
  return 'text';
}
