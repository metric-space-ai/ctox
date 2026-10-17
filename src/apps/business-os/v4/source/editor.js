// Shell v4 – Quelltext-Modus: Editor-Fläche.
// Monaco aus der Standardbibliothek ctox:lib/monaco@0 (der Service Worker schreibt das Präfix nach der
// Lib-Bindung der Shell-Wurzel /~a/shell@<sv>~lib:monaco@<M.m>/ auf /~a/lib:monaco@<M.m>/ um, §16.39), sonst eine textarea mit Zeilennummern.
// Kein CDN, kein Nachladen von außen, kein Import aus einer App.
//
//   createEditor(host, { value, language, readonly, prefer: 'auto'|'textarea', onChange, workerUrl? }) →
//     Promise<{ kind, getValue(), setValue(text), setReadonly(on), focus(), destroy() }>

const MONACO_LIB_BASE = 'ctox:lib/monaco@0/';
// Worker gleich-ursprünglich unter der gebundenen Lib-Wurzel, nie aus data:/blob: (§16.50 Nr. 1): der SW schreibt dieses
// Literal mit derselben Umschreibung wie MONACO_LIB_BASE auf /~a/lib:monaco@<M.m>…/vs/base/worker/workerMain.js um,
// workerMain.js lädt den Rest (Editor-/Sprach-Worker) relativ dazu. Liefert der Kernel binding.workerUrl(lib, path)
// (K-1j), kommt die URL von dort (createEditor-Option workerUrl).
const MONACO_WORKER_MAIN = 'ctox:lib/monaco@0/vs/base/worker/workerMain.js';

let monacoP = null;
const MONACO_TIMEOUT_MS = 6000;

function loadMonaco(workerUrl = null) {
  if (globalThis.monaco?.editor) return Promise.resolve(globalThis.monaco);
  if (monacoP) return monacoP;
  let base;
  try { base = new URL(MONACO_LIB_BASE, location.origin).href; } catch { base = MONACO_LIB_BASE; }
  monacoP = (async () => {
    const head = await fetch(`${base}vs/loader.js`, { method: 'HEAD' }).catch(() => null);
    if (!head?.ok) throw new Error('monaco nicht lokal');
    let bound = null;
    // K-1k: Bindung der Shell liefert die Worker-URL; nicht gebunden (not_found) → Literal mit SW-Umschreibung.
    try { bound = typeof workerUrl === 'function' ? workerUrl('monaco', 'vs/base/worker/workerMain.js') : null; } catch { bound = null; }
    const worker = new URL(bound || MONACO_WORKER_MAIN, location.origin);
    if (worker.origin !== location.origin) throw new Error('monaco-Worker nicht gleich-ursprünglich');
    window.MonacoEnvironment = { getWorkerUrl: () => worker.href };
    if (!window.require?.config) {
      await new Promise((resolve, reject) => {
        const s = document.createElement('script');
        s.src = `${base}vs/loader.js`;
        s.onload = resolve;
        s.onerror = () => reject(new Error('monaco loader'));
        document.head.append(s);
      });
    }
    return Promise.race([
      new Promise((resolve, reject) => {
        window.require.config({ paths: { vs: `${base}vs` } });
        window.require(['vs/editor/editor.main'], () => resolve(window.monaco), reject);
      }),
      new Promise((_, reject) => setTimeout(() => reject(new Error('monaco timeout')), MONACO_TIMEOUT_MS)),
    ]);
  })().catch((e) => { monacoP = null; throw e; });
  return monacoP;
}

export function languageOf(path) {
  const ext = String(path || '').split('.').pop().toLowerCase();
  return { js: 'javascript', mjs: 'javascript', cjs: 'javascript', ts: 'typescript', css: 'css', html: 'html', json: 'json', md: 'markdown', svg: 'xml' }[ext] || 'plaintext';
}

function textareaEditor(host, { value, readonly, onChange }) {
  const wrap = document.createElement('div');
  wrap.className = 'v4-src-ed';
  const gutter = document.createElement('pre');
  gutter.className = 'v4-src-ed__gutter';
  gutter.setAttribute('aria-hidden', 'true');
  const ta = document.createElement('textarea');
  ta.className = 'v4-src-ed__text';
  ta.spellcheck = false;
  ta.wrap = 'off';
  ta.setAttribute('autocapitalize', 'off');
  ta.setAttribute('autocomplete', 'off');
  ta.value = value ?? '';
  ta.readOnly = !!readonly;
  let lines = 0;
  const renderGutter = () => {
    const n = ta.value.split('\n').length;
    if (n === lines) return;
    lines = n;
    gutter.textContent = Array.from({ length: n }, (_, i) => i + 1).join('\n');
  };
  ta.addEventListener('scroll', () => { gutter.scrollTop = ta.scrollTop; });
  ta.addEventListener('input', () => { renderGutter(); onChange?.(ta.value); });
  ta.addEventListener('keydown', (e) => {
    if (e.key === 'Tab' && !e.metaKey && !e.ctrlKey && !ta.readOnly) {
      e.preventDefault();
      ta.setRangeText('  ', ta.selectionStart, ta.selectionEnd, 'end');
      ta.dispatchEvent(new Event('input'));
    }
  });
  renderGutter();
  wrap.append(gutter, ta);
  host.append(wrap);
  return {
    kind: 'textarea',
    el: wrap,
    getValue: () => ta.value,
    setValue(text) { ta.value = text ?? ''; renderGutter(); },
    // wie Einfügen über alles: löst onChange aus (Tests, „Ordner…“-Upload in die offene Datei)
    replaceAll(text) { ta.value = text ?? ''; ta.dispatchEvent(new Event('input')); },
    setReadonly(on) { ta.readOnly = !!on; },
    focus: () => ta.focus(),
    destroy: () => wrap.remove(),
  };
}

export async function createEditor(host, { value = '', language = 'plaintext', readonly = false, prefer = 'auto', onChange, workerUrl = null } = {}) {
  if (prefer !== 'textarea') {
    try {
      const monaco = await loadMonaco(workerUrl);
      const el = document.createElement('div');
      el.className = 'v4-src-ed v4-src-ed--monaco';
      host.append(el);
      const dark = matchMedia('(prefers-color-scheme: dark)').matches;
      const ed = monaco.editor.create(el, {
        value, language, readOnly: readonly, automaticLayout: true, minimap: { enabled: false }, fontSize: 12,
        lineNumbersMinChars: 3, scrollBeyondLastLine: false, theme: dark ? 'vs-dark' : 'vs', renderLineHighlight: 'line',
        tabSize: 2, wordWrap: 'off', fixedOverflowWidgets: false,
      });
      let silent = false;
      ed.onDidChangeModelContent(() => { if (!silent) onChange?.(ed.getValue()); });
      return {
        kind: 'monaco',
        el,
        getValue: () => ed.getValue(),
        setValue(text) { silent = true; ed.setValue(text ?? ''); silent = false; },
        replaceAll(text) { ed.executeEdits('replace-all', [{ range: ed.getModel().getFullModelRange(), text: text ?? '' }]); },
        setReadonly(on) { ed.updateOptions({ readOnly: !!on }); },
        focus: () => ed.focus(),
        destroy() { const m = ed.getModel(); ed.dispose(); m?.dispose(); el.remove(); },
      };
    } catch { /* Rückfall textarea */ }
  }
  return textareaEditor(host, { value, readonly, onChange });
}
