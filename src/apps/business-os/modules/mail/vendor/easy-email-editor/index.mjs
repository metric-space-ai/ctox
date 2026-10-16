const UPSTREAM_COMMIT = '16bb02926a20af20dc6dc473c72619f4a0b4f64b';
const MESSAGE_PARENT = 'ctox-easy-email-parent';
const MESSAGE_FRAME = 'ctox-easy-email-frame';
const THEME_TOKENS = Object.freeze([
  '--bg', '--surface', '--surface-2', '--surface-3', '--line', '--text',
  '--text-strong', '--muted', '--accent', '--accent-soft',
  '--accent-foreground', '--danger', '--warning', '--success', '--focus-ring',
]);

export const EASY_EMAIL_UPSTREAM = Object.freeze({
  repository: 'https://github.com/zalify/easy-email-editor',
  commit: UPSTREAM_COMMIT,
  version: '4.17.1',
  license: 'MIT',
});

function clone(value) {
  if (value == null) return value;
  return typeof structuredClone === 'function'
    ? structuredClone(value)
    : JSON.parse(JSON.stringify(value));
}

function createBlock(type, attributes = {}, value = {}, children = []) {
  return { type, data: { value }, attributes, children };
}

/** Creates a valid upstream Easy Email/MJML document, not a CTOX-only schema. */
export function createEmailDocument(input = {}) {
  const text = createBlock(
    'text',
    { padding: '10px 25px', align: 'left' },
    { content: input.content || '<p>Text eingeben …</p>' },
  );
  const column = createBlock(
    'column',
    { padding: '0px', border: 'none', 'vertical-align': 'top' },
    {},
    [text],
  );
  const section = createBlock(
    'section',
    {
      padding: '20px 0px',
      'background-repeat': 'repeat',
      'background-size': 'auto',
      'background-position': 'top center',
      border: 'none',
      direction: 'ltr',
      'text-align': 'center',
    },
    { noWrap: false },
    [column],
  );
  const wrapper = createBlock(
    'wrapper',
    { padding: '20px 0px', border: 'none', direction: 'ltr', 'text-align': 'center' },
    {},
    [section],
  );
  const page = createBlock(
    'page',
    { 'background-color': input.backgroundColor || '#efeeea', width: input.width || '600px' },
    {
      breakpoint: '480px',
      headAttributes: '',
      'font-size': '14px',
      'font-weight': '400',
      'line-height': '1.7',
      headStyles: [],
      fonts: [],
      responsive: true,
      'font-family': '-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
      'text-color': '#000000',
    },
    [wrapper],
  );
  return {
    subject: String(input.subject || ''),
    subTitle: String(input.subTitle || input.preview || ''),
    content: page,
  };
}

function isEmailDocument(value) {
  return Boolean(value && typeof value === 'object' && value.content?.type === 'page');
}

function instanceId() {
  if (globalThis.crypto?.randomUUID) return globalThis.crypto.randomUUID();
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

/**
 * Mount the genuine bundled Easy Email React editor in a local same-origin
 * iframe. All persistence remains owned by the caller.
 */
export async function createEasyEmailEditor(options = {}) {
  const host = options.host;
  if (!host?.ownerDocument || typeof host.replaceChildren !== 'function') {
    throw new TypeError('createEasyEmailEditor requires a DOM host');
  }

  const doc = host.ownerDocument;
  const win = doc.defaultView || globalThis.window;
  const id = instanceId();
  const origin = win.location.origin;
  const selectionListeners = new Set();
  const pending = new Map();
  let requestSequence = 0;
  let destroyed = false;
  let ready = false;
  let initialized = false;
  let currentDocument = isEmailDocument(options.document)
    ? clone(options.document)
    : createEmailDocument();
  let selectedBlockId = 'content';
  let mergeTags = clone(options.mergeTags || {});

  function themePayload() {
    const rootStyle = win.getComputedStyle(doc.documentElement);
    const hostStyle = win.getComputedStyle(host);
    const tokens = Object.fromEntries(THEME_TOKENS.map((name) => [
      name,
      hostStyle.getPropertyValue(name).trim() || rootStyle.getPropertyValue(name).trim(),
    ]).filter(([, value]) => value));
    return {
      theme: doc.documentElement?.dataset?.theme || options.theme || 'system',
      tokens,
    };
  }

  const shell = doc.createElement('div');
  shell.className = 'ctox-easy-email-upstream-frame';
  Object.assign(shell.style, {
    position: 'absolute',
    inset: '0',
    minWidth: '0',
    minHeight: '0',
    overflow: 'hidden',
    background: 'var(--bg)',
  });
  const frame = doc.createElement('iframe');
  frame.title = 'Easy Email HTML-Editor';
  frame.setAttribute('referrerpolicy', 'no-referrer');
  Object.assign(frame.style, {
    display: 'block',
    width: '100%',
    height: '100%',
    border: '0',
    background: 'var(--bg)',
  });
  const frameUrl = new URL('./bundle/frame.html', import.meta.url);
  frameUrl.searchParams.set('instance', id);
  frameUrl.searchParams.set('v', UPSTREAM_COMMIT.slice(0, 12));
  frame.src = frameUrl.href;
  shell.append(frame);
  host.replaceChildren(shell);

  let resolveReady;
  let rejectReady;
  const readyPromise = new Promise((resolve, reject) => {
    resolveReady = resolve;
    rejectReady = reject;
  });
  const readyTimer = win.setTimeout(() => {
    rejectReady(new Error('Easy Email upstream bundle did not become ready'));
  }, 30000);

  function post(type, payload, requestId) {
    if (destroyed) throw new Error('Easy Email editor is destroyed');
    frame.contentWindow?.postMessage(
      { source: MESSAGE_PARENT, instanceId: id, type, payload, requestId },
      origin,
    );
  }

  function request(type, payload) {
    return readyPromise.then(() => new Promise((resolve, reject) => {
      const requestId = `${id}-${++requestSequence}`;
      const timer = win.setTimeout(() => {
        pending.delete(requestId);
        reject(new Error(`Easy Email bridge request timed out: ${type}`));
      }, 30000);
      pending.set(requestId, {
        resolve(value) {
          win.clearTimeout(timer);
          resolve(value);
        },
        reject(error) {
          win.clearTimeout(timer);
          reject(error);
        },
      });
      post(type, payload, requestId);
    }));
  }

  function onMessage(event) {
    if (event.source !== frame.contentWindow || event.origin !== origin) return;
    const message = event.data;
    if (message?.source !== MESSAGE_FRAME || message.instanceId !== id) return;
    if (message.type === 'ready') {
      if (message.payload?.upstreamCommit !== UPSTREAM_COMMIT) {
        rejectReady(new Error('Easy Email bundle pin does not match its bridge'));
        return;
      }
      ready = true;
      win.clearTimeout(readyTimer);
      post('init', { document: currentDocument, mergeTags, ...themePayload() });
      resolveReady();
      return;
    }
    if (message.type === 'change' && message.payload?.document) {
      // The frame renders its internal default once before the ready handshake.
      // It must never overwrite a caller-supplied initial document.
      if (!initialized) return;
      currentDocument = clone(message.payload.document);
      options.onChange?.({ document: clone(currentDocument) });
      return;
    }
    if (message.type === 'initialized') {
      initialized = true;
      return;
    }
    if (message.type === 'selection') {
      selectedBlockId = String(message.payload?.blockId || 'content');
      for (const listener of selectionListeners) listener({ blockId: selectedBlockId });
      return;
    }
    if (message.type === 'response' || message.type === 'error') {
      const entry = pending.get(message.requestId);
      if (!entry) return;
      pending.delete(message.requestId);
      if (message.type === 'error') entry.reject(new Error(message.payload?.message || 'Easy Email bridge error'));
      else entry.resolve(message.payload);
    }
  }

  function onFrameError() {
    rejectReady(new Error('Easy Email upstream frame failed to load'));
  }

  win.addEventListener('message', onMessage);
  frame.addEventListener('error', onFrameError, { once: true });
  const themeObserver = new MutationObserver(() => {
    if (ready) post('set-theme', themePayload());
  });
  themeObserver.observe(doc.documentElement, { attributes: true, attributeFilter: ['class', 'data-theme', 'style'] });
  await readyPromise;

  return Object.freeze({
    element: shell,
    iframe: frame,
    ownsPanels: true,
    getSelectedBlockId: () => selectedBlockId,
    onSelectionChange(listener) {
      if (typeof listener !== 'function') throw new TypeError('Selection listener must be a function');
      selectionListeners.add(listener);
      listener({ blockId: selectedBlockId });
      return () => selectionListeners.delete(listener);
    },
    async getDocument() {
      const result = await request('get-document');
      currentDocument = clone(result.document);
      return clone(currentDocument);
    },
    async setDocument(next) {
      currentDocument = isEmailDocument(next) ? clone(next) : createEmailDocument();
      const result = await request('set-document', { document: currentDocument });
      currentDocument = clone(result.document);
    },
    async getHtml() {
      const result = await request('get-html');
      return result.html;
    },
    async getMjml() {
      const result = await request('get-html');
      return result.mjml;
    },
    async setMergeTags(next) {
      mergeTags = clone(next || {});
      await request('set-merge-tags', mergeTags);
    },
    async setLogicPreview(preview) {
      await request('set-logic-preview', preview?.blockId ? clone(preview) : null);
    },
    async setActivePanel(name) {
      // The parent owns the Logic drawer; the bridge still receives the panel
      // state so contextual canvas chrome is suppressed consistently.
      const panel = ['blocks', 'design', 'source', 'logic'].includes(name) ? name : null;
      await readyPromise;
      post('set-panel', { name: panel || null });
    },
    async setViewport(name) {
      const viewport = ['edit', 'desktop', 'mobile'].includes(name) ? name : 'edit';
      await readyPromise;
      post('set-viewport', { name: viewport });
    },
    async undo() {
      await request('history', { action: 'undo' });
    },
    async redo() {
      await request('history', { action: 'redo' });
    },
    async select(blockId) {
      await readyPromise;
      post('set-selection', { blockId });
    },
    focus() {
      frame.focus();
      if (ready) post('focus');
    },
    async destroy() {
      if (destroyed) return;
      destroyed = true;
      win.clearTimeout(readyTimer);
      win.removeEventListener('message', onMessage);
      frame.removeEventListener('error', onFrameError);
      themeObserver.disconnect();
      for (const entry of pending.values()) entry.reject(new Error('Easy Email editor was destroyed'));
      pending.clear();
      selectionListeners.clear();
      shell.remove();
    },
  });
}

export default { createEasyEmailEditor, createEmailDocument, EASY_EMAIL_UPSTREAM };
