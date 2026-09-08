import { OfficeRpcPeer } from './rpc.mjs';
import { readShellAppearance, observeShellAppearance } from './shell-appearance.mjs';

const VALID_KINDS = new Set(['document', 'spreadsheet']);
const OFFICE_OPERATION_TIMEOUT_MS = 120000;
const OFFICE_STARTUP_TIMEOUT_MS = 120000;

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
  const frameUrl = new URL('./frame.html', import.meta.url);
  frameUrl.searchParams.set('kind', kind);
  const assetRevision = new URL(import.meta.url).searchParams.get('v');
  if (assetRevision) frameUrl.searchParams.set('v', assetRevision);
  frame.srcdoc = capsuleFrameDocument(frameUrl, assetRevision);
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
  const readyTimeoutMs = normalizeStartupTimeout(options.readyTimeoutMs);
  frame.contentWindow.postMessage({
    type: 'ctox-office-connect',
    kind,
    productName,
    locale: options.locale === 'en' ? 'en' : 'de',
    theme: currentShellTheme(options.theme),
    appearance: readShellAppearance(options.host, options.theme),
    permissions: normalizePermissions(options.permissions),
    launchArgs: sanitizeLaunchArgs(options.launchArgs, readyTimeoutMs),
  }, location.origin, [channel.port2]);

  try {
    await rpc.call('editor.ready', null, { timeoutMs: readyTimeoutMs });
  } catch (error) {
    offEvent();
    rpc.close();
    frame.remove();
    throw error;
  }

  let destroyed = false;
  const stopAppearanceObserver = observeShellAppearance(options.host, options.theme, (appearance) => {
    if (!destroyed) rpc.call('editor.setAppearance', appearance).catch(() => {});
  });
  return Object.freeze({
    kind,
    open: (request) => rpc.call('editor.open', request, { timeoutMs: OFFICE_OPERATION_TIMEOUT_MS }),
    save: (request = {}) => rpc.call('editor.save', request, { timeoutMs: OFFICE_OPERATION_TIMEOUT_MS }),
    export: (request = {}) => rpc.call('editor.export', request, { timeoutMs: OFFICE_OPERATION_TIMEOUT_MS }),
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
      stopAppearanceObserver();
      try { await rpc.call('editor.destroy', null, { timeoutMs: 3000 }); } catch {}
      offEvent();
      rpc.close();
      listeners.clear();
      frame.remove();
    },
  });
}

function capsuleFrameDocument(frameUrl, assetRevision) {
  const assetRoot = new URL('.', frameUrl);
  const revision = assetRevision ? `?v=${encodeURIComponent(assetRevision)}` : '';
  return `<!doctype html>
<html lang="de">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width,initial-scale=1">
    <title>CTOX Editor</title>
    <link rel="stylesheet" href="${assetRoot.href}frame.css${revision}">
  </head>
  <body>
    <main id="ctox-office-frame-root" aria-live="polite">
      <div class="ctox-office-frame-loading">CTOX Editor wird initialisiert …</div>
    </main>
    <script type="module" src="${assetRoot.href}frame-runtime.mjs${revision}"></script>
  </body>
</html>`;
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

function sanitizeLaunchArgs(args = {}, readyTimeoutMs = OFFICE_STARTUP_TIMEOUT_MS) {
  return {
    runtimeModule: typeof args.runtimeModule === 'string' ? args.runtimeModule : '',
    testMode: args.testMode === true,
    recordId: typeof args.recordId === 'string' ? args.recordId : '',
    versionId: typeof args.versionId === 'string' ? args.versionId : '',
    appReadyTimeoutMs: Math.max(10000, readyTimeoutMs - 5000),
  };
}

function normalizeStartupTimeout(value) {
  const timeoutMs = Number(value);
  return Number.isFinite(timeoutMs) && timeoutMs >= 10000
    ? Math.min(timeoutMs, 300000)
    : OFFICE_STARTUP_TIMEOUT_MS;
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
