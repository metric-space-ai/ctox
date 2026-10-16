// Shell v4 – Workjet-Hostbrücke (Port von V2 mobile-host.js, Pin e35a116ae). Eigentum Strang D.
//
// Der Workjet-Mobil-Host (apps/mobile …/WorkjetBusinessOsModule.swift|kt) setzt vor jedem Shell-Skript
// <html data-workjet-mobile-host="true">, schickt Befehle als CustomEvent 'workjet-business-os-host-command' und
// empfängt Antworten über globalThis.workjetBusinessOsPostMessage (Protokoll workjet.business-os-shell.v1).
// Die Shell legt dafür globalThis.workjetHostBridge.postSessionTransferEvent an (Sitzungsereignisse, session.js).
//
// Rumpf wörtlich V2 mobile-host.js:1-265 mit diesen Ersetzungen (Begründung in WJB-D.md):
//   - Modul-Seiteneffekt → installWorkjetHostBridge({target, doc, shell, catalog}) mit uninstall() (ein Aufruf je
//     startShell über index.js; ein Shell-Tausch ersetzt die alte Brücke).
//   - `globalThis` → `target`, `document` → `doc`, `location`/`history` → `target.location`/`target.history`.
//   - Fenster → App: V2 liest data-owner-id `module:<app>`; der v4-Rahmen (shared/window-manager.js, gleiche Klassen
//     .shell-window / .is-focused) setzt data-owner-id `v4app:<app>#<n>` und data-window-app `<app>`
//     (v4/frame/index.js openApp). Fenster eines gerade abgelösten Rahmens ([data-v4-retiring]) zählen nicht.
//   - App öffnen: V2 setzt location.hash (V2-Router). v4 hat keinen Hash-Router: `shell().navigate(appId)`
//     (v4/frame/index.js shellHandle) bzw. für Home `shell().showDesktop()` – RAHMEN-HAKEN, siehe WJB-D.md.
//   - Katalog: V2 lädt die statische Altdatei per fetch (in v4 verboten, Wächter ref-legacy-catalog-file). v4 bekommt
//     ihn über `catalog()` (KERN-/RAHMEN-HAKEN: mobilePresentation/desktopOnly je App, siehe WJB-D.md). Fehlt er, gilt
//     der V2-Fehlerpfad: shell.error 'catalog-unavailable', retryable.
const PROTOCOL = 'workjet.business-os-shell.v1';
const CATALOG_TYPE = 'workjet.business-os-mobile-apps.v1';
const MAX_MESSAGE_BYTES = 65_536;
const SAFE_ID = /^[a-z0-9][a-z0-9._-]{0,127}$/;
const V4_OWNER = /^v4app:([^#]+)(?:#\d+)?$/;

/**
 * @param {object} [o]
 * @param {object} [o.target]   globalThis (Globals, Ereignisse, location, history)
 * @param {Document} [o.doc]
 * @param {() => ({navigate?: (appId: string) => boolean, showDesktop?: () => void} | null)} [o.shell]  Rahmen-Griff
 * @param {() => Promise<object>} [o.catalog]  Mobil-Katalog {type, revision, apps:[{id,title,mobilePresentation,desktopOnly}]}
 * @param {(cb: () => void) => (() => void)} [o.catalogChanges]  v4: meldet neue App-Listen; der Katalog wird dann neu
 *   gebildet und bei geänderter Revision als catalog.replace nachgeschoben (V2 lud die statische Datei nur einmal).
 * @param {typeof MutationObserver} [o.MutationObserverImpl]
 * @returns {{installed: boolean, uninstall(): void}}
 */
export function installWorkjetHostBridge({
  target = globalThis, doc = globalThis.document, shell = null, catalog: loadCatalog = null, catalogChanges = null,
  MutationObserverImpl = globalThis.MutationObserver,
} = {}) {
  // ref: mobile-host.js:6
  if (doc?.documentElement?.dataset?.workjetMobileHost !== 'true') return { installed: false, uninstall() {} };
  const shellHandle = () => (typeof shell === 'function' ? shell() : shell) || null;
  let catalog = null;
  let activeAppId = null;
  let disposed = false;

  // ref: mobile-host.js:10-14
  const post = (message) => {
    if (disposed) return;
    const raw = JSON.stringify({ protocol: PROTOCOL, ...message });
    if (new TextEncoder().encode(raw).byteLength > MAX_MESSAGE_BYTES) return;
    target.workjetBusinessOsPostMessage?.(JSON.parse(raw));
  };

  // ref: mobile-host.js:16-38
  const boundedSessionTransferEvent = (event) => {
    if (!event || typeof event !== 'object' || event.type !== 'workjet.session.transfer') return null;
    const text = (value, maxLength) => {
      if (typeof value !== 'string') return null;
      const normalized = value.trim();
      if (!normalized || [...normalized].length > maxLength
        || /[\u0000-\u001f\u007f]/u.test(normalized)) return null;
      return normalized;
    };
    const integer = (value) => Number.isInteger(value) && value >= 0 ? value : null;
    const bounded = {
      type: 'workjet.session.transfer',
      transferId: text(event.transferId, 160),
      sessionId: text(event.sessionId, 160),
      state: text(event.state, 64),
      fenceEpoch: integer(event.fenceEpoch),
      sourceComputerId: text(event.sourceComputerId, 256),
      targetComputerId: text(event.targetComputerId, 256),
      deadlineAtMs: integer(event.deadlineAtMs),
      updatedAtMs: integer(event.updatedAtMs),
    };
    if (Object.values(bounded).some((value) => value === null)) return null;
    return Object.freeze(bounded);
  };

  // ref: mobile-host.js:40-50
  const existingWorkjetHostBridge = target.workjetHostBridge;
  const hostBridge = Object.freeze({
    ...(existingWorkjetHostBridge && typeof existingWorkjetHostBridge === 'object'
      ? existingWorkjetHostBridge
      : {}),
    postSessionTransferEvent(event) {
      const bounded = boundedSessionTransferEvent(event);
      if (bounded) post({ type: 'session.transfer.event', event: bounded });
    },
  });
  target.workjetHostBridge = hostBridge;

  // ref: mobile-host.js:52
  const descriptor = (appId) => catalog?.apps?.find((app) => app.id === appId) || null;

  // ref: mobile-host.js:54-58 (+ v4-Fensterkennung, siehe Kopf)
  const appIdForWindow = (element) => {
    const ownerId = String(element?.dataset?.ownerId || '');
    const appId = String(element?.dataset?.windowApp || V4_OWNER.exec(ownerId)?.[1] || ownerId.replace(/^(?:module|desktop-app):/, ''));
    return SAFE_ID.test(appId) ? appId : null;
  };
  // v4: nur Fenster des laufenden Rahmens (ein abgelöster Rahmen steht kurz daneben, v4/frame/index.js stopShell).
  const shellWindows = (selector = '.shell-window') => [...doc.querySelectorAll(selector)]
    .filter((element) => !element.closest?.('[data-v4-retiring]'));

  // ref: mobile-host.js:60-66
  const applyActiveAppMetadata = (appId) => {
    activeAppId = appId;
    doc.body.dataset.workjetMobileApp = appId;
    const presentation = descriptor(appId)?.mobilePresentation;
    if (presentation) doc.body.dataset.workjetMobilePresentation = presentation;
    else delete doc.body.dataset.workjetMobilePresentation;
  };

  // ref: mobile-host.js:68-89
  const markActiveWindow = () => {
    if (disposed) return;
    shellWindows('.shell-window[data-workjet-mobile-active]').forEach((element) => {
      delete element.dataset.workjetMobileActive;
    });
    // A focused window is authoritative only while an app is already active.
    // When Home is active, a previously focused window may still exist in the
    // shared window manager. Promoting that stale focus here used to replace
    // the freshly opened Home desk with Threads immediately.
    // v4 (WJB-M-2): der Fenstermanager lässt is-focused auf minimierten Fenstern stehen; gültig ist nur das oberste
    // sichtbare fokussierte Fenster (V2 hatte genau eins).
    const focused = shellWindows('.shell-window.is-focused').filter((el) => el.style?.display !== 'none')
      .sort((x, y) => (Number(x.style?.zIndex) || 0) - (Number(y.style?.zIndex) || 0)).at(-1) || null;
    const focusedAppId = appIdForWindow(focused);
    if (activeAppId !== 'desktop' && focusedAppId && focusedAppId !== activeAppId) {
      applyActiveAppMetadata(focusedAppId);
      publishState('active');
    }
    if (!activeAppId) return;
    const active = shellWindows().find((element) => appIdForWindow(element) === activeAppId);
    if (active) active.dataset.workjetMobileActive = 'true';
  };

  // ref: mobile-host.js:91-103
  const publishState = (state = 'active') => {
    if (!activeAppId) return;
    const app = descriptor(activeAppId);
    post({
      type: 'app.state',
      appId: activeAppId,
      title: app?.title || (activeAppId === 'desktop' ? 'Business OS' : activeAppId),
      canGoBack: false,
      state,
      actions: [],
    });
  };

  // ref: mobile-host.js:105-134 (location.hash → Rahmen-Griff, siehe Kopf)
  const openApp = (appId) => {
    if (!SAFE_ID.test(appId)) return;
    if (appId === 'desktop') {
      applyActiveAppMetadata(appId);
      shellHandle()?.showDesktop?.();
      // v4 (WJB-M-1): die Aktiv-Marke des zuletzt offenen Fensters sofort lösen (V2: hashchange → Beobachter).
      queueMicrotask(markActiveWindow);
      publishState('active');
      return;
    }
    const app = descriptor(appId);
    if (!app || app.desktopOnly || !app.mobilePresentation) {
      post({ type: 'shell.error', code: 'mobile-presentation-unavailable', retryable: false });
      return;
    }
    const handle = shellHandle();
    if (typeof handle?.navigate !== 'function') {
      // v4: ohne Rahmen-Griff kann die Shell keine App öffnen; gleiche Meldung wie für eine nicht mobile App.
      post({ type: 'shell.error', code: 'mobile-presentation-unavailable', retryable: false });
      return;
    }
    applyActiveAppMetadata(appId);
    post({
      type: 'app.state',
      appId,
      title: app.title,
      canGoBack: false,
      state: 'opening',
      actions: [],
    });
    handle.navigate(appId);
    queueMicrotask(markActiveWindow);
    setTimeout(() => {
      markActiveWindow();
      publishState('active');
    }, 0);
  };

  // ref: mobile-host.js:136-137
  const allowedKeys = (value, keys) =>
    value && typeof value === 'object' && Object.keys(value).every((key) => keys.includes(key));

  // ref: mobile-host.js:139-221
  const receive = (event) => {
    const command = event.detail;
    if (!command || command.protocol !== PROTOCOL || typeof command.type !== 'string') return;
    if (JSON.stringify(command).length > MAX_MESSAGE_BYTES) return;
    if (command.type === 'host.configure' && allowedKeys(command, ['protocol', 'type', 'platform', 'windowClass', 'colorScheme', 'reducedMotion', 'locale'])) {
      doc.documentElement.dataset.workjetWindowClass = command.windowClass;
      doc.documentElement.dataset.workjetPlatform = command.platform;
      return;
    }
    if (command.type === 'catalog.request' && allowedKeys(command, ['protocol', 'type'])) {
      if (catalog) post({ type: 'catalog.replace', catalog });
      return;
    }
    if (command.type === 'app.open' && allowedKeys(command, ['protocol', 'type', 'appId'])) {
      openApp(command.appId);
      return;
    }
    if (['app.close', 'app.suspend', 'app.resume'].includes(command.type) && allowedKeys(command, ['protocol', 'type', 'appId'])) {
      if (command.appId !== activeAppId) return;
      doc.documentElement.dataset.workjetLifecycle = command.type.split('.')[1];
      publishState(command.type === 'app.close' ? 'closed' : command.type === 'app.suspend' ? 'suspended' : 'active');
      if (command.type === 'app.close') {
        activeAppId = null;
        delete doc.body.dataset.workjetMobileApp;
        delete doc.body.dataset.workjetMobilePresentation;
        markActiveWindow();
      }
      return;
    }
    if (command.type === 'navigation.back' && allowedKeys(command, ['protocol', 'type'])) {
      target.history?.back?.();
      return;
    }
    if (command.type === 'action.invoke' && allowedKeys(command, ['protocol', 'type', 'appId', 'actionId'])) {
      if (command.appId !== activeAppId || !SAFE_ID.test(command.actionId)) return;
      target.dispatchEvent(new CustomEvent('workjet-business-os-action', {
        detail: { appId: command.appId, actionId: command.actionId },
      }));
      return;
    }
    if (command.type === 'device.control' && allowedKeys(command, ['protocol', 'type', 'requestId', 'request'])) {
      if (!SAFE_ID.test(command.requestId) || typeof target.workjetBusinessOsDeviceControl !== 'function') return;
      Promise.resolve(target.workjetBusinessOsDeviceControl(command.request))
        .then((result) => post({ type: 'device.control.result', requestId: command.requestId, result }))
        .catch((error) => post({
          type: 'device.control.result',
          requestId: command.requestId,
          error: {
            code: String(error?.code || 'device-control-failed').slice(0, 128),
            message: String(error?.message || 'Device control failed.').slice(0, 512),
          },
        }));
      return;
    }
    if (command.type === 'project.control' && allowedKeys(command, ['protocol', 'type', 'requestId', 'request'])) {
      if (!SAFE_ID.test(command.requestId) || typeof target.workjetProjectControl !== 'function') return;
      Promise.resolve(target.workjetProjectControl(command.request))
        .then((result) => post({ type: 'project.control.result', requestId: command.requestId, result }))
        .catch((error) => post({
          type: 'project.control.result',
          requestId: command.requestId,
          error: {
            code: String(error?.code || 'project-control-failed').slice(0, 128),
            message: String(error?.message || 'Project control failed.').slice(0, 512),
          },
        }));
    }
    if (command.type === 'session.control' && allowedKeys(command, ['protocol', 'type', 'requestId', 'request'])) {
      if (!SAFE_ID.test(command.requestId) || typeof target.workjetSessionControl !== 'function') return;
      Promise.resolve(target.workjetSessionControl(command.request))
        .then((result) => post({ type: 'session.control.result', requestId: command.requestId, result }))
        .catch((error) => post({
          type: 'session.control.result',
          requestId: command.requestId,
          error: {
            code: String(error?.code || 'session-control-failed').slice(0, 128),
            message: String(error?.message || 'Session control failed.').slice(0, 512),
          },
        }));
    }
  };

  // ref: mobile-host.js:223-251
  target.addEventListener('workjet-business-os-host-command', receive);
  // Desktop icons open apps through the canonical CTOX launcher rather than
  // through the host bridge. Capture the actual user activation so Home can
  // remain stable in the presence of stale window-manager focus while icon
  // clicks still promote the selected app to the responsive fullscreen view.
  const onClick = (event) => {
    if (activeAppId !== 'desktop') return;
    const icon = event.target?.closest?.('.desktop-icon[data-target]');
    const appId = String(icon?.dataset?.target || '');
    if (!SAFE_ID.test(appId)) return;
    applyActiveAppMetadata(appId);
    queueMicrotask(() => {
      markActiveWindow();
      publishState('active');
    });
  };
  const onKeydown = (event) => {
    if (activeAppId !== 'desktop' || !['Enter', ' '].includes(event.key)) return;
    const icon = event.target?.closest?.('.desktop-icon[data-target]');
    const appId = String(icon?.dataset?.target || '');
    if (!SAFE_ID.test(appId)) return;
    applyActiveAppMetadata(appId);
    queueMicrotask(() => {
      markActiveWindow();
      publishState('active');
    });
  };
  doc.addEventListener('click', onClick, true);
  doc.addEventListener('keydown', onKeydown, true);
  const observer = typeof MutationObserverImpl === 'function' ? new MutationObserverImpl(markActiveWindow) : null;
  observer?.observe(doc.documentElement, {
    childList: true,
    subtree: true,
  });

  // ref: mobile-host.js:253-264 (fetch der Altdatei → catalog(), siehe Kopf)
  Promise.resolve()
    .then(() => {
      if (typeof loadCatalog !== 'function') throw new Error('catalog unavailable'); // v4-allow: code (V2-Fehlertext, intern: der catch meldet nur shell.error)
      return loadCatalog();
    })
    .then((value) => {
      if (value?.type !== CATALOG_TYPE || !Array.isArray(value.apps) || value.apps.length > 256) {
        throw new Error('catalog invalid');
      }
      if (value.apps.some((app) => app?.id === 'desktop')) throw new Error('catalog contains native home');
      catalog = value;
      post({ type: 'shell.ready', revision: String(value.revision || 'unknown').slice(0, 256) });
      post({ type: 'catalog.replace', catalog });
    })
    .catch(() => post({ type: 'shell.error', code: 'catalog-unavailable', retryable: true }));

  // v4: Katalog folgt der App-Liste (neue/entfernte Apps, layout aus dem Manifest nachgeladen).
  const offCatalog = typeof catalogChanges === 'function' && typeof loadCatalog === 'function' ? catalogChanges(() => {
    if (disposed || !catalog) return; // vor dem ersten Katalog gilt der Startpfad oben
    Promise.resolve().then(() => loadCatalog()).then((value) => {
      if (disposed || value?.type !== CATALOG_TYPE || !Array.isArray(value.apps) || value.apps.length > 256) return;
      if (value.apps.some((app) => app?.id === 'desktop') || value.revision === catalog?.revision) return;
      catalog = value;
      post({ type: 'catalog.replace', catalog });
    }).catch(() => {});
  }) : null;

  return {
    installed: true,
    uninstall() {
      disposed = true;
      if (typeof offCatalog === 'function') offCatalog();
      target.removeEventListener('workjet-business-os-host-command', receive);
      doc.removeEventListener('click', onClick, true);
      doc.removeEventListener('keydown', onKeydown, true);
      observer?.disconnect();
      if (target.workjetHostBridge === hostBridge) {
        if (existingWorkjetHostBridge === undefined) delete target.workjetHostBridge;
        else target.workjetHostBridge = existingWorkjetHostBridge;
      }
    },
  };
}
