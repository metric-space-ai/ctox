import { OfficeRpcPeer } from './rpc.mjs';

const VALID_KINDS = new Set(['document', 'spreadsheet']);

export async function createCtoxOfficeEditor(options = {}) {
  const kind = String(options.kind || '');
  if (!VALID_KINDS.has(kind)) throw new TypeError(`Unsupported CTOX editor kind: ${kind}`);
  const productName = kind === 'document' ? 'CTOX Documents' : 'CTOX Spreadsheets';
  if (!(options.host instanceof Element)) throw new TypeError(`${productName} requires a host Element`);
  const bridge = validateBridge(options.bridge, productName);
  const frame = document.createElement('iframe');
  frame.className = `ctox-office-capsule ctox-office-capsule--${kind}`;
  frame.title = `${productName} Editor`;
  frame.dataset.ctoxOfficeKind = kind;
  frame.style.cssText = 'display:block;width:100%;height:100%;border:0;background:transparent';
  frame.setAttribute('referrerpolicy', 'no-referrer');
  frame.src = new URL(`./frame.html?kind=${encodeURIComponent(kind)}`, import.meta.url).href;
  options.host.replaceChildren(frame);

  await waitForFrameLoad(frame, options.loadTimeoutMs);
  const channel = new MessageChannel();
  const rpc = new OfficeRpcPeer(channel.port1, {
    'bridge.loadVersion': (request) => bridge.loadVersion(request),
    'bridge.prepare': (request) => bridge.prepare(request),
    'bridge.commit': (request) => bridge.commit(request),
    'bridge.export': (request) => bridge.export(request),
    'bridge.reportIntegrityError': (request) => bridge.reportIntegrityError?.(request),
  });
  const listeners = new Map();
  const offEvent = rpc.on('editor.event', ({ name, detail } = {}) => {
    for (const listener of listeners.get(name) || []) listener(detail);
  });
  frame.contentWindow.postMessage({
    type: 'ctox-office-connect',
    kind,
    productName,
    locale: options.locale === 'en' ? 'en' : 'de',
    theme: options.theme || 'system',
    permissions: normalizePermissions(options.permissions),
    launchArgs: sanitizeLaunchArgs(options.launchArgs),
  }, location.origin, [channel.port2]);

  try {
    await rpc.call('editor.ready', null, { timeoutMs: options.readyTimeoutMs || 30000 });
  } catch (error) {
    offEvent();
    rpc.close();
    frame.remove();
    throw error;
  }

  let destroyed = false;
  const themeObserver = new MutationObserver(() => {
    if (!destroyed) rpc.call('editor.setTheme', currentShellTheme(options.theme)).catch(() => {});
  });
  themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
  return Object.freeze({
    kind,
    open: (request) => rpc.call('editor.open', request),
    save: (request = {}) => rpc.call('editor.save', request),
    export: (request = {}) => rpc.call('editor.export', request),
    focus: () => rpc.call('editor.focus'),
    setPermissions: (permissions) => rpc.call('editor.setPermissions', normalizePermissions(permissions)),
    inspect: () => rpc.call('editor.inspect'),
    on(name, listener) {
      if (typeof listener !== 'function') throw new TypeError('Office event listener must be a function');
      const set = listeners.get(name) || new Set();
      set.add(listener);
      listeners.set(name, set);
      return () => set.delete(listener);
    },
    async destroy() {
      if (destroyed) return;
      destroyed = true;
      themeObserver.disconnect();
      try { await rpc.call('editor.destroy', null, { timeoutMs: 3000 }); } catch {}
      offEvent();
      rpc.close();
      listeners.clear();
      frame.remove();
    },
  });
}

function currentShellTheme(fallback = 'system') {
  const shellTheme = document.documentElement.dataset.theme;
  if (shellTheme === 'dark' || shellTheme === 'light') return shellTheme;
  return fallback === 'dark' || fallback === 'light' ? fallback : 'system';
}

function validateBridge(bridge, productName) {
  if (!bridge || typeof bridge !== 'object') throw new TypeError(`${productName} requires a bridge`);
  for (const method of ['loadVersion', 'prepare', 'commit', 'export']) {
    if (typeof bridge[method] !== 'function') throw new TypeError(`${productName} bridge is missing ${method}()`);
  }
  return bridge;
}

function normalizePermissions(permissions = {}) {
  return Object.freeze({
    read: permissions.read !== false,
    write: permissions.write !== false,
    export: permissions.export !== false,
    comment: permissions.comment !== false,
    review: permissions.review !== false,
  });
}

function sanitizeLaunchArgs(args = {}) {
  return {
    runtimeModule: typeof args.runtimeModule === 'string' ? args.runtimeModule : '',
    testMode: args.testMode === true,
    recordId: typeof args.recordId === 'string' ? args.recordId : '',
    versionId: typeof args.versionId === 'string' ? args.versionId : '',
  };
}

function waitForFrameLoad(frame, timeoutValue) {
  const timeoutMs = Number.isFinite(Number(timeoutValue)) ? Number(timeoutValue) : 15000;
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error('CTOX product iframe load timed out')), timeoutMs);
    const cleanup = () => {
      clearTimeout(timeout);
      frame.removeEventListener('load', onLoad);
      frame.removeEventListener('error', onError);
    };
    const onLoad = () => { cleanup(); resolve(); };
    const onError = () => { cleanup(); reject(new Error('CTOX product iframe failed to load')); };
    frame.addEventListener('load', onLoad, { once: true });
    frame.addEventListener('error', onError, { once: true });
  });
}
