// CTOX stealth_init.js — JS-property evasions injected via addInitScript.
//
// Algorithms and shapes are ported (under MIT, Copyright berstend & contributors)
// from the open-source evasions in
// https://github.com/berstend/puppeteer-extra/tree/master/packages/puppeteer-extra-plugin-stealth.
// This is *userland* — Patchright already handles the structural CDP leaks
// (Runtime.enable, Console.enable, sourceURL). What remains in 2026 is the
// JS-property surface: navigator/window/WebGL/permissions/iframes/etc.
//
// Loaded by both runners via `addInitScript({ path: 'stealth_init.js' })`
// — runs once on every new document, before any page script.
//
// Self-contained IIFE so we can be served as a plain script.

(() => {
  'use strict';

  // ──────────────────────────────────────────────────────────────────────
  // toString-patching helpers — hide our overrides from .toString() checks
  // ──────────────────────────────────────────────────────────────────────
  const originalToString = Function.prototype.toString;
  const patchedFns = new WeakMap();

  function asNative(name, fn) {
    const nativeStr = `function ${name}() { [native code] }`;
    patchedFns.set(fn, nativeStr);
    return fn;
  }

  Function.prototype.toString = new Proxy(originalToString, {
    apply(target, thisArg, args) {
      if (patchedFns.has(thisArg)) return patchedFns.get(thisArg);
      return Reflect.apply(target, thisArg, args);
    },
  });
  patchedFns.set(Function.prototype.toString, 'function toString() { [native code] }');

  function defineGetter(obj, prop, getter, name) {
    try {
      const wrapped = asNative(`get ${prop}`, getter);
      Object.defineProperty(obj, prop, { get: wrapped, configurable: true });
    } catch {}
  }

  // ──────────────────────────────────────────────────────────────────────
  // navigator.webdriver — fpscanner WEBDRIVER probes `'webdriver' in
  // navigator`, not the value. Real Chrome (no --enable-automation) has
  // no webdriver property at all; just returning undefined keeps the
  // property in place. Delete it entirely; fall back to undefined getter
  // if the descriptor turns out to be non-configurable.
  // ──────────────────────────────────────────────────────────────────────
  try {
    delete Navigator.prototype.webdriver;
  } catch {}
  try {
    delete navigator.webdriver;
  } catch {}
  if ('webdriver' in navigator) {
    try {
      Object.defineProperty(Navigator.prototype, 'webdriver', {
        get: asNative('get webdriver', () => undefined),
        set: () => {},
        configurable: true,
      });
    } catch {}
  }

  // ──────────────────────────────────────────────────────────────────────
  // window.chrome — runtime/csi/loadTimes/app shaped like real Chrome
  // ──────────────────────────────────────────────────────────────────────
  if (!window.chrome || !window.chrome.runtime) {
    const makeChrome = () => {
      const csi = asNative('csi', () => ({
        startE: Date.now(),
        onloadT: Date.now(),
        pageT: performance.now(),
        tran: 15,
      }));
      const loadTimes = asNative('loadTimes', () => ({
        commitLoadTime: Date.now() / 1000 - 1,
        connectionInfo: 'http/1.1',
        finishDocumentLoadTime: Date.now() / 1000,
        finishLoadTime: Date.now() / 1000,
        firstPaintAfterLoadTime: 0,
        firstPaintTime: Date.now() / 1000,
        navigationType: 'Other',
        npnNegotiatedProtocol: 'unknown',
        requestTime: Date.now() / 1000 - 1,
        startLoadTime: Date.now() / 1000 - 1,
        wasAlternateProtocolAvailable: false,
        wasFetchedViaSpdy: false,
        wasNpnNegotiated: false,
      }));
      const runtime = {
        OnInstalledReason: {
          CHROME_UPDATE: 'chrome_update',
          INSTALL: 'install',
          SHARED_MODULE_UPDATE: 'shared_module_update',
          UPDATE: 'update',
        },
        OnRestartRequiredReason: {
          APP_UPDATE: 'app_update',
          OS_UPDATE: 'os_update',
          PERIODIC: 'periodic',
        },
        PlatformArch: { ARM: 'arm', ARM64: 'arm64', MIPS: 'mips', MIPS64: 'mips64', X86_32: 'x86-32', X86_64: 'x86-64' },
        PlatformNaclArch: { ARM: 'arm', MIPS: 'mips', MIPS64: 'mips64', X86_32: 'x86-32', X86_64: 'x86-64' },
        PlatformOs: { ANDROID: 'android', CROS: 'cros', LINUX: 'linux', MAC: 'mac', OPENBSD: 'openbsd', WIN: 'win' },
        RequestUpdateCheckStatus: { NO_UPDATE: 'no_update', THROTTLED: 'throttled', UPDATE_AVAILABLE: 'update_available' },
      };
      Object.defineProperty(runtime, 'id', { get: asNative('get id', () => undefined), configurable: true });
      return { app: { isInstalled: false, InstallState: { DISABLED: 'disabled', INSTALLED: 'installed', NOT_INSTALLED: 'not_installed' }, RunningState: { CANNOT_RUN: 'cannot_run', READY_TO_RUN: 'ready_to_run', RUNNING: 'running' } }, csi, loadTimes, runtime };
    };
    try {
      Object.defineProperty(window, 'chrome', { value: makeChrome(), configurable: true, writable: true });
    } catch {}
  }

  // ──────────────────────────────────────────────────────────────────────
  // navigator.plugins / navigator.mimeTypes — realistic Chrome 2024+ PDF set
  // (Native Client was removed in Chrome 102, do NOT include it)
  // ──────────────────────────────────────────────────────────────────────
  try {
    const pluginData = [
      { name: 'PDF Viewer', filename: 'internal-pdf-viewer', description: 'Portable Document Format' },
      { name: 'Chrome PDF Viewer', filename: 'internal-pdf-viewer', description: 'Portable Document Format' },
      { name: 'Chromium PDF Viewer', filename: 'internal-pdf-viewer', description: 'Portable Document Format' },
      { name: 'Microsoft Edge PDF Viewer', filename: 'internal-pdf-viewer', description: 'Portable Document Format' },
      { name: 'WebKit built-in PDF', filename: 'internal-pdf-viewer', description: 'Portable Document Format' },
    ];
    const mimeData = [
      { type: 'application/pdf', suffixes: 'pdf', description: 'Portable Document Format' },
      { type: 'text/pdf', suffixes: 'pdf', description: 'Portable Document Format' },
    ];

    const fakeMimeTypes = mimeData.map((m) => {
      const mt = Object.create(MimeType.prototype);
      Object.defineProperty(mt, 'type', { get: () => m.type });
      Object.defineProperty(mt, 'suffixes', { get: () => m.suffixes });
      Object.defineProperty(mt, 'description', { get: () => m.description });
      return mt;
    });
    const fakePlugins = pluginData.map((p) => {
      const plugin = Object.create(Plugin.prototype);
      Object.defineProperty(plugin, 'name', { get: () => p.name });
      Object.defineProperty(plugin, 'filename', { get: () => p.filename });
      Object.defineProperty(plugin, 'description', { get: () => p.description });
      Object.defineProperty(plugin, 'length', { get: () => fakeMimeTypes.length });
      plugin.item = asNative('item', (i) => fakeMimeTypes[i] || null);
      plugin.namedItem = asNative('namedItem', (n) => fakeMimeTypes.find((m) => m.type === n) || null);
      for (let i = 0; i < fakeMimeTypes.length; i += 1) plugin[i] = fakeMimeTypes[i];
      return plugin;
    });
    // Link mime types back to their enabledPlugin (PDF viewer is plugins[0])
    fakeMimeTypes.forEach((mt) => {
      Object.defineProperty(mt, 'enabledPlugin', { get: () => fakePlugins[0] });
    });

    const pluginArray = Object.create(PluginArray.prototype);
    Object.defineProperty(pluginArray, 'length', { get: () => fakePlugins.length });
    // Real Chrome truncates index to uint32 (i >>> 0). incolumitas overflowTest
    // probes item(2**32) and expects it to equal plugins[0].
    pluginArray.item = asNative('item', (i) => fakePlugins[i >>> 0] || null);
    pluginArray.namedItem = asNative('namedItem', (n) => fakePlugins.find((p) => p.name === n) || null);
    pluginArray.refresh = asNative('refresh', () => {});
    for (let i = 0; i < fakePlugins.length; i += 1) pluginArray[i] = fakePlugins[i];
    pluginArray[Symbol.iterator] = function* () { for (const p of fakePlugins) yield p; };

    const mimeTypeArray = Object.create(MimeTypeArray.prototype);
    Object.defineProperty(mimeTypeArray, 'length', { get: () => fakeMimeTypes.length });
    mimeTypeArray.item = asNative('item', (i) => fakeMimeTypes[i >>> 0] || null);
    mimeTypeArray.namedItem = asNative('namedItem', (n) => fakeMimeTypes.find((m) => m.type === n) || null);
    for (let i = 0; i < fakeMimeTypes.length; i += 1) mimeTypeArray[i] = fakeMimeTypes[i];
    mimeTypeArray[Symbol.iterator] = function* () { for (const m of fakeMimeTypes) yield m; };

    Object.defineProperty(Navigator.prototype, 'plugins', {
      get: asNative('get plugins', () => pluginArray),
      configurable: true,
    });
    Object.defineProperty(Navigator.prototype, 'mimeTypes', {
      get: asNative('get mimeTypes', () => mimeTypeArray),
      configurable: true,
    });
  } catch {}

  // ──────────────────────────────────────────────────────────────────────
  // Notification.permission — headless reports 'denied' by default; real
  // Chrome on a fresh profile reports 'default' (= prompt user on demand)
  // ──────────────────────────────────────────────────────────────────────
  try {
    if (typeof Notification !== 'undefined' && Notification.permission === 'denied') {
      Object.defineProperty(Notification, 'permission', {
        get: asNative('get permission', () => 'default'),
        configurable: true,
      });
    }
  } catch {}

  // ──────────────────────────────────────────────────────────────────────
  // navigator.permissions.query — notifications consistency
  // Real Chrome returns prompt for notifications when headed, but a
  // pure headless context sometimes leaks "denied" for notifications
  // while still returning "default" for Notification.permission.
  // ──────────────────────────────────────────────────────────────────────
  try {
    const originalQuery = navigator.permissions && navigator.permissions.query;
    if (originalQuery) {
      const patched = asNative('query', function (params) {
        if (params && params.name === 'notifications') {
          return Promise.resolve({ state: Notification.permission === 'default' ? 'prompt' : Notification.permission, onchange: null });
        }
        return originalQuery.apply(navigator.permissions, [params]);
      });
      navigator.permissions.query = patched;
    }
  } catch {}

  // ──────────────────────────────────────────────────────────────────────
  // document.hasFocus / window blur — headless reports false; real focus
  // is the default state. Patches are mild and only affect read tells.
  // ──────────────────────────────────────────────────────────────────────
  try {
    const origHasFocus = document.hasFocus;
    document.hasFocus = asNative('hasFocus', function () {
      try {
        return origHasFocus.call(document) || true;
      } catch { return true; }
    });
  } catch {}

  // ──────────────────────────────────────────────────────────────────────
  // navigator.vibrate — must exist on Chrome (function, returns true/false)
  // ──────────────────────────────────────────────────────────────────────
  try {
    if (typeof navigator.vibrate !== 'function') {
      Object.defineProperty(Navigator.prototype, 'vibrate', {
        value: asNative('vibrate', function () { return true; }),
        configurable: true,
        writable: true,
      });
    }
  } catch {}

  // ──────────────────────────────────────────────────────────────────────
  // WebGL — UNMASKED_VENDOR_WEBGL / UNMASKED_RENDERER_WEBGL per platform
  // Picks a vendor/renderer pair that matches a plausible host. We use
  // a deterministic mapping based on navigator.platform.
  // ──────────────────────────────────────────────────────────────────────
  try {
    const platform = (navigator.platform || '').toLowerCase();
    let vendor = 'Google Inc. (Intel)';
    let renderer = 'ANGLE (Intel, Intel(R) UHD Graphics, OpenGL 4.1)';
    if (platform.includes('mac')) {
      vendor = 'Google Inc. (Apple)';
      renderer = 'ANGLE (Apple, Apple M1, OpenGL 4.1)';
    } else if (platform.includes('win')) {
      vendor = 'Google Inc. (NVIDIA)';
      renderer = 'ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 Direct3D11 vs_5_0 ps_5_0, D3D11)';
    }
    const patchGetParameter = (Proto) => {
      if (typeof Proto === 'undefined') return;
      const original = Proto.prototype.getParameter;
      const wrapped = asNative('getParameter', function (param) {
        // UNMASKED_VENDOR_WEBGL / UNMASKED_RENDERER_WEBGL constants
        if (param === 37445) return vendor;
        if (param === 37446) return renderer;
        return original.apply(this, arguments);
      });
      Proto.prototype.getParameter = wrapped;
    };
    patchGetParameter(WebGLRenderingContext);
    if (typeof WebGL2RenderingContext !== 'undefined') patchGetParameter(WebGL2RenderingContext);
  } catch {}

  // ──────────────────────────────────────────────────────────────────────
  // iframe.contentWindow — propagate chrome/navigator overrides into iframes
  // Anti-bots often probe an about:blank iframe to bypass top-level patches.
  // ──────────────────────────────────────────────────────────────────────
  try {
    const elementDescriptor = Object.getOwnPropertyDescriptor(HTMLIFrameElement.prototype, 'contentWindow');
    if (elementDescriptor && elementDescriptor.get) {
      const original = elementDescriptor.get;
      const wrapped = asNative('get contentWindow', function () {
        const win = original.apply(this);
        try {
          if (win && !win.chrome) Object.defineProperty(win, 'chrome', { value: window.chrome, configurable: true });
        } catch {}
        return win;
      });
      Object.defineProperty(HTMLIFrameElement.prototype, 'contentWindow', {
        get: wrapped,
        configurable: true,
      });
    }
  } catch {}

  // ──────────────────────────────────────────────────────────────────────
  // navigator.hardwareConcurrency — 1 is a headless tell; clamp to >= 4
  // ──────────────────────────────────────────────────────────────────────
  try {
    if ((navigator.hardwareConcurrency || 0) < 4) {
      Object.defineProperty(Navigator.prototype, 'hardwareConcurrency', {
        get: asNative('get hardwareConcurrency', () => 8),
        configurable: true,
      });
    }
  } catch {}

  // ──────────────────────────────────────────────────────────────────────
  // navigator.languages — fall back to ['en-US', 'en'] if empty/unset
  // (Playwright already sets locale; this only fixes the empty-array tell)
  // ──────────────────────────────────────────────────────────────────────
  try {
    if (!navigator.languages || navigator.languages.length === 0) {
      Object.defineProperty(Navigator.prototype, 'languages', {
        get: asNative('get languages', () => ['en-US', 'en']),
        configurable: true,
      });
    }
  } catch {}

  // ──────────────────────────────────────────────────────────────────────
  // outerHeight / outerWidth — match the inner dimensions in headless
  // (headless reports 0/0 by default which is a strong tell)
  // ──────────────────────────────────────────────────────────────────────
  try {
    if (window.outerHeight === 0 || window.outerWidth === 0) {
      Object.defineProperty(window, 'outerHeight', {
        get: asNative('get outerHeight', () => window.innerHeight + 85),
        configurable: true,
      });
      Object.defineProperty(window, 'outerWidth', {
        get: asNative('get outerWidth', () => window.innerWidth),
        configurable: true,
      });
    }
  } catch {}

  // ──────────────────────────────────────────────────────────────────────
  // navigator.connection — headless commonly reports rtt=0 (a strong tell
  // since headed Chrome rounds to nearest 25ms with privacy budget, and
  // 0 means "unmeasured"). We override unconditionally because the value
  // may flip to 0 after init-time depending on background measurements.
  // ──────────────────────────────────────────────────────────────────────
  try {
    const fakeConn = {
      rtt: 50,
      downlink: 10,
      effectiveType: '4g',
      saveData: false,
      type: 'wifi',
      onchange: null,
      addEventListener: asNative('addEventListener', () => {}),
      removeEventListener: asNative('removeEventListener', () => {}),
      dispatchEvent: asNative('dispatchEvent', () => true),
    };
    Object.defineProperty(Navigator.prototype, 'connection', {
      get: asNative('get connection', () => fakeConn),
      configurable: true,
    });
  } catch {}

  // ──────────────────────────────────────────────────────────────────────
  // overflow / scrollLeft tells — headless reports inconsistent values on
  // elements with overflow:hidden. Force consistent zero-clamping so the
  // incolumitas overflowTest does not flag it.
  // ──────────────────────────────────────────────────────────────────────
  try {
    const originalScrollLeftDesc = Object.getOwnPropertyDescriptor(Element.prototype, 'scrollLeft');
    if (originalScrollLeftDesc && originalScrollLeftDesc.get && originalScrollLeftDesc.set) {
      const origGet = originalScrollLeftDesc.get;
      const origSet = originalScrollLeftDesc.set;
      Object.defineProperty(Element.prototype, 'scrollLeft', {
        get: asNative('get scrollLeft', function () { return origGet.call(this); }),
        set: asNative('set scrollLeft', function (value) {
          // Clamp to int >= 0 like real Chrome
          const v = Math.max(0, Math.floor(Number(value) || 0));
          return origSet.call(this, v);
        }),
        configurable: true,
      });
    }
  } catch {}

  // ──────────────────────────────────────────────────────────────────────
  // navigator.userAgentData — JS-side counterpart to Sec-CH-UA headers.
  // Headless Chromium reports brands like "HeadlessChrome" here; replace
  // with a vanilla Chrome 146 brand triple, mobile=false, platform derived
  // from navigator.platform. The accompanying HTTP headers are set on the
  // context via extraHTTPHeaders in the runner so JS and HTTP align.
  // ──────────────────────────────────────────────────────────────────────
  try {
    const platRaw = (navigator.platform || '').toLowerCase();
    let uaPlatform = 'Linux';
    if (platRaw.includes('mac')) uaPlatform = 'macOS';
    else if (platRaw.includes('win')) uaPlatform = 'Windows';
    const brands = [
      { brand: 'Chromium', version: '146' },
      { brand: 'Google Chrome', version: '146' },
      { brand: 'Not.A/Brand', version: '24' },
    ];
    const fullVersionList = [
      { brand: 'Chromium', version: '146.0.7680.177' },
      { brand: 'Google Chrome', version: '146.0.7680.177' },
      { brand: 'Not.A/Brand', version: '24.0.0.0' },
    ];
    const uad = {
      brands,
      mobile: false,
      platform: uaPlatform,
      getHighEntropyValues: asNative('getHighEntropyValues', function (hints) {
        const data = {
          architecture: uaPlatform === 'macOS' ? 'arm' : 'x86',
          bitness: '64',
          brands,
          fullVersionList,
          mobile: false,
          model: '',
          platform: uaPlatform,
          platformVersion: uaPlatform === 'macOS' ? '14.7.0' : (uaPlatform === 'Windows' ? '15.0.0' : '6.5.0'),
          uaFullVersion: '146.0.7680.177',
          wow64: false,
          formFactors: ['Desktop'],
        };
        const out = {};
        if (Array.isArray(hints)) {
          for (const h of hints) if (h in data) out[h] = data[h];
        }
        // brands/mobile/platform are always present per spec.
        out.brands = brands;
        out.mobile = false;
        out.platform = uaPlatform;
        return Promise.resolve(out);
      }),
      toJSON: asNative('toJSON', function () {
        return { brands, mobile: false, platform: uaPlatform };
      }),
    };
    Object.defineProperty(Navigator.prototype, 'userAgentData', {
      get: asNative('get userAgentData', () => uad),
      configurable: true,
    });
  } catch {}
})();
