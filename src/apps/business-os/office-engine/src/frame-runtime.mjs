import { OfficeRpcPeer } from './rpc.mjs';

const root = document.querySelector('#ctox-office-frame-root');
const OFFICE_OPERATION_TIMEOUT_MS = 120000;
let connected = false;

window.addEventListener('message', async (event) => {
  if (connected || event.origin !== location.origin || event.data?.type !== 'ctox-office-connect') return;
  const [port] = event.ports || [];
  if (!port) return;
  connected = true;
  const config = event.data;
  const productName = config.productName || (config.kind === 'spreadsheet' ? 'CTOX Spreadsheets' : 'CTOX Documents');
  document.title = productName;
  const loading = root.querySelector('.ctox-office-frame-loading');
  if (loading) loading.textContent = `${productName} wird initialisiert …`;
  document.documentElement.lang = config.locale === 'en' ? 'en' : 'de';
  let runtime = null;
  const peer = new OfficeRpcPeer(port, {
    'editor.ready': async () => {
      runtime ||= await loadRuntime(config, peer);
      return runtime.inspect();
    },
    'editor.open': async (request) => {
      runtime ||= await loadRuntime(config, peer);
      return runtime.open(request);
    },
    'editor.save': (request) => requireRuntime(runtime).save(request),
    'editor.export': (request) => requireRuntime(runtime).export(request),
    'editor.focus': () => requireRuntime(runtime).focus(),
    'editor.setPermissions': (permissions) => requireRuntime(runtime).setPermissions(permissions),
    'editor.setTheme': (theme) => requireRuntime(runtime).setTheme(theme),
    'editor.inspect': () => requireRuntime(runtime).inspect(),
    'editor.destroy': async () => {
      await runtime?.destroy?.();
      runtime = null;
      root.replaceChildren();
      return { destroyed: true };
    },
  });
});

async function loadRuntime(config, peer) {
  const runtimeName = config.kind === 'spreadsheet' ? 'ctox-spreadsheets' : 'ctox-documents';
  const defaultModule = new URL(`./runtime/${runtimeName}.mjs`, import.meta.url).href;
  const moduleUrl = config.launchArgs?.runtimeModule
    ? new URL(config.launchArgs.runtimeModule, location.href).href
    : defaultModule;
  try {
    const module = await import(moduleUrl);
    if (typeof module.createOfficeFrameRuntime !== 'function') {
      throw new Error(`Office runtime ${moduleUrl} has no createOfficeFrameRuntime export`);
    }
    const runtime = await module.createOfficeFrameRuntime({
      root,
      kind: config.kind,
      locale: config.locale,
      theme: config.theme,
      permissions: config.permissions,
      launchArgs: config.launchArgs,
      bridge: {
        loadVersion: (request) => peer.call('bridge.loadVersion', request, { timeoutMs: OFFICE_OPERATION_TIMEOUT_MS }),
        prepare: (request) => peer.call('bridge.prepare', request, { timeoutMs: OFFICE_OPERATION_TIMEOUT_MS }),
        commit: (request, transfer = []) => peer.call('bridge.commit', request, { transfer, timeoutMs: OFFICE_OPERATION_TIMEOUT_MS }),
        export: (request) => peer.call('bridge.export', request, { timeoutMs: OFFICE_OPERATION_TIMEOUT_MS }),
        reportIntegrityError: (request) => peer.call('bridge.reportIntegrityError', request),
      },
      emit: (name, detail) => peer.emit('editor.event', { name, detail }),
    });
    assertRuntime(runtime);
    return runtime;
  } catch (error) {
    root.innerHTML = `<div class="ctox-office-frame-error"><div><strong>${productName} nicht verfügbar</strong><p>${escapeHtml(error?.message || error)}</p></div></div>`;
    throw error;
  }
}

function assertRuntime(runtime) {
  for (const method of ['open', 'save', 'export', 'focus', 'setPermissions', 'setTheme', 'inspect', 'destroy']) {
    if (typeof runtime?.[method] !== 'function') throw new Error(`Office frame runtime is missing ${method}()`);
  }
}

function requireRuntime(runtime) {
  if (!runtime) throw new Error('Office frame runtime is not initialized');
  return runtime;
}

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}
