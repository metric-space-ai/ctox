import { loadModuleMessages } from '../../shared/i18n.js';

const STYLE_BUILD = new URL(import.meta.url).searchParams.get('v') || 'browser-source';

// Module-level translator; set from locales/<lang>.json during mount.
let t = (key, fallback) => fallback ?? key;
const DEFAULT_SESSION_ID = 'browser_session_default';
const DEFAULT_TAB_ID = 'browser_tab_default';
const VIEWPORT = { width: 1280, height: 720 };
const FRAME_SYNC_RECOVERY_MS = 12000;
const POINTER_CLICK_INTERVAL_MS = 500;
const POINTER_CLICK_SLOP_PX = 4;
const POINTER_HOVER_THROTTLE_MS = 50;
const BROWSER_NAV_COMMANDS = {
  navigate: 'browser.navigate',
  back: 'browser.back',
  forward: 'browser.forward',
  reload: 'browser.reload',
};
const BROWSER_SYNC_COLLECTIONS = [
  'browser_sessions',
  'browser_tabs',
];
const SCRAPING_ADAPTER_COLLECTIONS = Object.freeze([
  // Core Outbound campaigns.
  'outbound_research_adapters',
  // Tenant-local customer tenant campaigns. The customer tenant module owns this collection;
  // reading only the core collection made its complete adapter list disappear.
  'outbound_lead_generation_adapters',
]);

export async function mount(ctx) {
  await ensureStyles();
  const messages = await loadModuleMessages(import.meta.url, ctx.locale).catch(() => ({}));
  t = (key, fallback) => messages[key] ?? fallback ?? key;
  const moduleUrl = new URL(import.meta.url);
  const templateUrl = new URL('./index.html', moduleUrl);
  templateUrl.search = moduleUrl.search;
  templateUrl.searchParams.set('fragment', STYLE_BUILD);
  const html = await fetch(templateUrl, { cache: 'no-store' }).then((res) => res.text());
  ctx.host.innerHTML = html;

  const root = ctx.host.querySelector('[data-browser-root]');
  if (!root) throw new Error('browser: root element missing after fragment mount');
  applyTranslations(root);

  const refs = {
    root,
    sessionCard: root.querySelector('[data-browser-session-card]'),
    sessionList: root.querySelector('[data-browser-session-list]'),
    sessionsPane: root.querySelector('.browser-sessions'),
    viewToggle: root.querySelector('[data-browser-view-toggle]'),
    sessions: root.querySelector('[data-browser-sessions]'),
    sessionsEmpty: root.querySelector('[data-browser-sessions-empty]'),
    sessionsImport: root.querySelector('[data-action="import"]'),
    sessionsExport: root.querySelector('[data-action="export"]'),
    start: root.querySelector('[data-browser-start]'),
    toggleAdvanced: root.querySelector('[data-browser-toggle-advanced]'),
    advanced: root.querySelector('[data-browser-advanced]'),
    privateMode: root.querySelector('[data-browser-private]'),
    viewport: root.querySelector('[data-browser-viewport]'),
    newTab: root.querySelector('[data-browser-new-tab]'),
    contextMenu: root.querySelector('[data-browser-context-menu]'),
    tabstrip: root.querySelector('[data-browser-tabstrip]'),
    adapterCount: root.querySelector('[data-pg-count="adapters"]'),
    upload: root.querySelector('[data-browser-upload]'),
    controllerAcquire: root.querySelector('[data-browser-controller-acquire]'),
    controllerRelease: root.querySelector('[data-browser-controller-release]'),
    observerGrant: root.querySelector('[data-browser-observer-grant]'),
    observerRevoke: root.querySelector('[data-browser-observer-revoke]'),
    clipboardCopy: root.querySelector('[data-browser-clipboard-copy]'),
    clipboardPaste: root.querySelector('[data-browser-clipboard-paste]'),
    clipboardClear: root.querySelector('[data-browser-clipboard-clear]'),
    stop: root.querySelector('[data-browser-stop]'),
    back: root.querySelector('[data-browser-back]'),
    forward: root.querySelector('[data-browser-forward]'),
    reload: root.querySelector('[data-browser-reload]'),
    sendToCtox: root.querySelector('[data-browser-send-to-ctox]'),
    notice: root.querySelector('[data-browser-notice]'),
    form: root.querySelector('[data-browser-address-form]'),
    go: root.querySelector('[data-browser-go]'),
    address: root.querySelector('[data-browser-address]'),
    addressClear: null,
    statusChip: root.querySelector('[data-browser-status-chip]'),
    statusTitle: root.querySelector('[data-browser-status-title]'),
    statusMeta: root.querySelector('[data-browser-status-meta]'),
    downloads: root.querySelector('[data-browser-downloads]'),
    authAssist: root.querySelector('[data-browser-auth-assist]'),
    shell: root.querySelector('[data-browser-frame-shell]'),
    canvas: root.querySelector('[data-browser-canvas]'),
    railToggle: root.querySelector('[data-browser-rail-toggle]'),
    preciseToggle: root.querySelector('[data-browser-precise]'),
    preciseLayer: root.querySelector('[data-browser-precise-layer]'),
    preciseMark: root.querySelector('[data-browser-precise-mark]'),
    precisePanel: root.querySelector('[data-browser-precise-panel]'),
    preciseText: root.querySelector('[data-browser-precise-text]'),
    preciseHint: root.querySelector('[data-browser-precise-hint]'),
    empty: root.querySelector('[data-browser-empty]'),
    viewSwitch: root.querySelectorAll('[data-browser-view]'),
    scriptPanel: root.querySelector('[data-browser-script-panel]'),
    scriptCode: root.querySelector('[data-browser-script-code]'),
    scriptPath: root.querySelector('[data-browser-script-path]'),
    automationOverlay: root.querySelector('[data-browser-automation-overlay]'),
    automationTitle: root.querySelector('[data-browser-automation-title]'),
    automationStatus: root.querySelector('[data-browser-automation-status]'),
    automationSource: root.querySelector('[data-browser-automation-source]'),
    automationCode: root.querySelector('[data-browser-automation-code]'),
    frameId: root.querySelector('[data-browser-frame-id]'),
    frameSeq: root.querySelector('[data-browser-frame-seq]'),
    frameSize: root.querySelector('[data-browser-frame-size]'),
    frameTime: root.querySelector('[data-browser-frame-time]'),
    inputState: root.querySelector('[data-browser-input-state]'),
    commandState: root.querySelector('[data-browser-command-state]'),
    commandHistory: root.querySelector('[data-browser-command-history]'),
    handoffHistory: root.querySelector('[data-browser-handoff-history]'),
  };
  refs.addressClear = installAddressClearControl(refs.form, refs.address, refs.go);

  const requestedSessionId = browserSessionIdFromArgs(ctx.args);
  const state = {
    selectedSessionId: requestedSessionId,
    requestedSessionId,
    latestFrame: null,
    latestSession: null,
    latestTab: null,
    latestCommand: null,
    browserCommands: [],
    handoffTasks: [],
    notice: '',
    drawing: false,
    lastInputSeq: 0,
    lastPointerMoveAt: 0,
    lastPointerClick: null,
    pointerIsDown: false,
    lastFrameSyncRecoveryAt: 0,
    leaseRenewInFlight: false,
    controllerLeaseId: '',
    addressDirty: false,
    requestedSessionStarts: new Set(),
    // LEFT grammar column (SHELL-wired data-pg-*). The module only mirrors the
    // shell state to filter/label the sessions list; it wires no chrome itself.
    leftView: { search: '', view: 'cards', band: 'all', filters: {} },
    // Imported sessions are a read-only local overlay for review (browser
    // sessions are a read-only projection; the app never persists them).
    importedSessions: [],
    directLiveEnabled: typeof ctx.sync?.requestNative === 'function',
    directLiveFailures: 0,
    // Letzte ok:false-Begruendung des Live-Kanals; steuert Einmal-Logging und
    // das Aufraeumen der Statuszeile, sobald wieder Bilder kommen.
    lastLiveRejection: '',
    directInputFailures: 0,
    directInputQueue: [],
    directFrameSeq: 0,
    latestDirectFrame: null,
    // Invalidates a live-frame request that started before a navigation or a
    // session switch. Without this generation an old, slow screenshot can
    // arrive after the new page and paint the previous URL and pixels back
    // over an already completed navigation.
    directNavigationEpoch: 0,
  };

  const cleanups = [];
  let mounted = true;
  let refreshInFlight = null;
  let refreshQueued = false;
  const scheduleRefresh = debounce(safeLoadAndRender, 80);
  // Die Tab-Leiste wird ausserhalb dieses Scopes gebaut, braucht nach einer
  // Tab-Aktion aber denselben Auffrischer wie jeder andere Knopf.
  state.refresh = safeLoadAndRender;
  const requestedStartTimers = new Set();

  function openRequestedBrowserSession(args, attempt = 0) {
    if (!browserSessionIdFromArgs(args)) return;
    state.notice = 'Browser-Anmeldung wird geöffnet.';
    scheduleRefresh();
    ensureRequestedBrowserSession(ctx, state, args)
      .then((started) => {
        if (!started) state.notice = '';
        scheduleRefresh();
      })
      .catch(async (error) => {
        const requestedSessionId = browserSessionIdFromArgs(args);
        const session = requestedSessionId
          ? await browserCollection(ctx, 'browser_sessions')?.findOne(requestedSessionId).exec().catch(() => null)
          : null;
        const sessionData = session?.toJSON?.() || session;
        // The durable command can be processed and the browser can already be
        // ready while its terminal receipt is still delayed on the return
        // replication path. The materialized browser session is authoritative
        // here: do not replace a successful start with a false error or queue
        // duplicate retries merely because the acknowledgement timed out.
        if (requestedSessionId && !browserSessionNeedsStart(sessionData)) {
          state.requestedSessionStarts.delete(requestedSessionId);
          state.notice = '';
          scheduleRefresh();
          return;
        }
        const errorCode = String(error?.code || '').toLowerCase();
        if (['peer_connect_timeout', 'projection_delayed'].includes(errorCode)) {
          // The command bus has already persisted the start request. This code
          // describes only the delayed return path, not a rejected browser
          // start. The session projection/live status will surface a genuine
          // runtime failure if one occurs later.
          state.requestedSessionStarts.delete(requestedSessionId);
          state.notice = '';
          scheduleRefresh();
          return;
        }
        state.notice = browserStartErrorMessage(error);
        scheduleRefresh();
        if (!browserStartErrorIsRetryable(error) || attempt >= 3 || !mounted) return;
        const timer = globalThis.setTimeout(() => {
          requestedStartTimers.delete(timer);
          openRequestedBrowserSession(args, attempt + 1);
        }, Math.min(8_000, 1_000 * (2 ** attempt)));
        requestedStartTimers.add(timer);
      });
  }

  function selectRequestedBrowserSession(detail = {}) {
    const sessionId = browserSessionIdFromArgs(detail);
    if (!sessionId) return;
    if (sessionId !== state.selectedSessionId) state.controllerLeaseId = '';
    if (sessionId !== state.selectedSessionId) state.directNavigationEpoch += 1;
    state.selectedSessionId = sessionId;
    state.requestedSessionId = sessionId;
    scheduleRefresh();
    openRequestedBrowserSession(detail);
  }

  const sessionSelectionToken = ctx.eventBus?.on?.('browser:select-session', selectRequestedBrowserSession);
  if (sessionSelectionToken && ctx.eventBus?.off) {
    cleanups.push(() => ctx.eventBus.off('browser:select-session', sessionSelectionToken));
  }
  // `openDesktopApp('browser', { args })` focuses an already open Browser and
  // delivers the new launch request as this DOM event. The Browser previously
  // consumed only the initial `ctx.args`; a later auth handoff therefore kept
  // showing whichever ended session happened to be selected and never sent
  // `browser.session.start`. Consume every launch so protected-source buttons
  // work whether the Browser was closed, open, or pre-opened as immediate
  // feedback while the server prepares the authoritative session arguments.
  const handleDesktopAppLaunch = (event) => {
    const detail = event?.detail || {};
    if (detail.appId && detail.appId !== 'browser') return;
    selectRequestedBrowserSession(detail.args || {});
  };
  ctx.host?.addEventListener?.('ctox-business-os-app-launch', handleDesktopAppLaunch);
  cleanups.push(() => ctx.host?.removeEventListener?.('ctox-business-os-app-launch', handleDesktopAppLaunch));
  const handleFocusRefresh = () => {
    scheduleRefresh();
    renewControllerLeaseIfNeeded();
  };
  const focusRefreshToken = ctx.eventBus?.on?.('window:focused', handleFocusRefresh);
  if (focusRefreshToken && ctx.eventBus?.off) {
    cleanups.push(() => ctx.eventBus.off('window:focused', focusRefreshToken));
  }
  globalThis.addEventListener?.('focus', handleFocusRefresh);
  globalThis.addEventListener?.('blur', scheduleRefresh);
  globalThis.document?.addEventListener?.('visibilitychange', handleFocusRefresh);
  cleanups.push(() => {
    globalThis.removeEventListener?.('focus', handleFocusRefresh);
    globalThis.removeEventListener?.('blur', scheduleRefresh);
    globalThis.document?.removeEventListener?.('visibilitychange', handleFocusRefresh);
  });

  for (const collectionName of ['business_commands', ...BROWSER_SYNC_COLLECTIONS, 'ctox_queue_tasks']) {
    ctx.sync?.startCollection?.(collectionName)
      ?.catch?.((error) => console.warn(`[browser] ${collectionName} sync start failed`, error));
  }

  for (const collection of [
    browserCollection(ctx, 'business_commands'),
    browserCollection(ctx, 'browser_sessions'),
    browserCollection(ctx, 'browser_tabs'),
    browserCollection(ctx, 'ctox_queue_tasks'),
  ]) {
    const sub = collection?.$?.subscribe?.(() => scheduleRefresh());
    if (sub?.unsubscribe) cleanups.push(() => sub.unsubscribe());
  }

  // LEFT column grammar is SHELL-wired (search / shard-list toggle / filter
  // tray / reset / active-dot / counted band). The module re-renders the
  // sessions list on the bubbling change event; no manual refresh button —
  // the sessions list is reactive (collection subscriptions above).
  const onLeftGrammarChange = (event) => {
    const detail = event?.detail || refs.sessionsPane?.__ctoxPaneGrammar?.state?.() || {};
    state.leftView = {
      search: String(detail.search || '').trim().toLowerCase(),
      view: detail.view === 'list' ? 'list' : 'cards',
      band: detail.band || 'all',
      filters: detail.filters || {},
    };
    // Der Umschalter ist eine AKTION: die Shell stempelt aria-pressed auf jeden
    // [data-pg-view]-Knopf und emittiert danach dieses Ereignis — hier faellt
    // das Attribut wieder weg, und Symbol/Beschriftung folgen der Ansicht.
    syncViewToggle(refs, state.leftView.view);
    renderLeftRail(ctx, refs, state);
    renderTabstrip(ctx, refs, state);
  };
  root.addEventListener('ctox-pane-grammar-change', onLeftGrammarChange);
  cleanups.push(() => root.removeEventListener('ctox-pane-grammar-change', onLeftGrammarChange));
  // Die Adapter-Lease endet mit dem Modul, sonst haelt sie die bedarfs-
  // geladene Sammlung dauerhaft offen.
  cleanups.push(() => {
    for (const lease of state.adapterLeases || []) lease?.release?.();
    state.adapterLeases = [];
  });
  refs.sessionsImport?.addEventListener('click', () => importBrowserSessions(ctx, state, refs));
  refs.sessionsExport?.addEventListener('click', () => exportBrowserSessions(state, refs));
  // EIN Umschalter Karten/Liste (Betreiber-Direktive 31.08.2026). Der Knopf
  // dreht data-pg-view auf sich selbst; die Shell-Grammatik liest daraus
  // weiterhin die AKTUELLE Ansicht (viewButtons[0].dataset.pgView) und meldet
  // sie ueber ctox-pane-grammar-change zurueck. mount() laeuft vor
  // autoWirePaneGrammar, dieser Listener also vor dem der Shell — sie sieht
  // beim Emittieren schon den gedrehten Wert. Das eigene Rendern hier haelt den
  // Umschalter auch dann funktionsfaehig, wenn die Shell nicht verdrahtet ist.
  refs.viewToggle?.addEventListener('click', () => {
    const next = refs.viewToggle.dataset.pgView === 'list' ? 'cards' : 'list';
    syncViewToggle(refs, next);
    state.leftView = { ...state.leftView, view: next };
    renderLeftRail(ctx, refs, state);
  });
  syncViewToggle(refs, state.leftView.view);
  refs.toggleAdvanced?.addEventListener('click', () => {
    if (!refs.advanced) return;
    const hidden = refs.advanced.classList.toggle('is-advanced-hidden');
    refs.toggleAdvanced.setAttribute('aria-pressed', hidden ? 'false' : 'true');
  });
  const startNewBrowserSession = (url = refs.address?.value || 'https://example.com') => {
    const now = Date.now();
    // Jeder Aufruf vergibt eine NEUE Sitzungskennung und startet einen eigenen
    // Chrome. Ohne Sperre wird aus zwei Klicks zweimal alles.
    // Am 16.08.2026 auf der Kundeninstanz gemessen: sieben Startbefehle in zwei
    // Minuten, sieben Sitzungen, 21 Chrome-Prozesse, 34 Eintraege in der Liste.
    // Mitverursacht durch die Reparatur vom 15.08.: seitdem fuehrt "Los" bei
    // einer toten Sitzung in den Start-Zweig — richtig, aber eben oefter.
    const grund = startSperrgrund(state, now);
    if (grund) {
      console.info('[browser] Start uebersprungen:', grund);
      state.notice = grund;
      safeLoadAndRender();
      return;
    }
    state.startPendingSince = now;
    const sessionId = `${userSessionPrefix(ctx.session)}_${now}`;
    const tabId = `browser_tab_${now}`;
    const viewport = selectedViewport(refs.viewport);
    console.info(`[browser] session start clicked session_id=${sessionId} tab_id=${tabId}`);
    state.addressDirty = false;
    state.directNavigationEpoch += 1;
    state.selectedSessionId = sessionId;
    state.requestedSessionId = sessionId;
    state.latestSession = null;
    state.latestTab = null;
    state.latestFrame = null;
    state.latestDirectFrame = null;
    state.controllerLeaseId = newBrowserControllerLeaseId();
    state.directLiveEnabled = typeof ctx.sync?.requestNative === 'function';
    state.directLiveFailures = 0;
    state.lastLiveRejection = '';
    state.directInputFailures = 0;
    state.directInputQueue.length = 0;
    state.notice = 'Browser wird mit CTOX verbunden …';
    safeLoadAndRender();
    const startPayload = {
      session_id: sessionId,
      tab_id: tabId,
      url,
      viewport_w: viewport.width,
      viewport_h: viewport.height,
      profile_mode: refs.privateMode?.checked ? 'private' : 'persistent',
      lease_id: state.controllerLeaseId,
      new_session: true,
    };
    const command = () => dispatchBrowserCommand(ctx, state, 'browser.session.start', startPayload);
    const directStart = async () => {
      if (typeof ctx.sync?.requestNative !== 'function') return command();
      try {
        // requestNative respektiert sein timeoutMs nicht zuverlaessig: auf
        // managed production tenant am 20.08.2026 gemessen blieb das Promise ewig offen,
        // der Rueckfall auf den dauerhaften Befehlsweg wurde nie erreicht,
        // finally lief nie, die Start-Sperre blieb gesetzt — der Los-Knopf war
        // dauerhaft tot, ohne Meldung. Ein eigenes Zeitlimit erzwingt nach
        // 10 s den catch-Zweig und damit den funktionierenden Befehlsweg.
        const direktantwort = ctx.sync.requestNative('ctox.browser.live.v1', {
          op: 'session.start',
          ...startPayload,
        }, {
          collection: 'business_commands',
          requiredCapability: 'ctox-browser-live-v1',
          timeoutMs: 30_000,
        });
        const zeitlimit = new Promise((unused, reject) => setTimeout(() => {
          const fehler = new Error('Direktkanal antwortet nicht (Zeitlimit nach 10 s).');
          fehler.code = 'native_unavailable';
          reject(fehler);
        }, 10_000));
        const response = await Promise.race([direktantwort, zeitlimit]);
        // Eine Antwort ohne Inhalt ist keine Antwort. Ohne diese Pruefung wurde
        // `undefined` als erfolgreicher Start gewertet und die Oberflaeche
        // meldete eine Sitzung, die es nie gab.
        if (!response || typeof response !== 'object') {
          const fehler = new Error('Direktkanal lieferte keine Sitzungsantwort.');
          fehler.code = 'native_unavailable';
          throw fehler;
        }
        const startedAt = Date.now();
        state.latestSession = {
          id: sessionId,
          owner_user_id: browserActorIds(ctx.session)[0] || '',
          controller_user_id: browserActorIds(ctx.session)[0] || '',
          controller_lease_id: state.controllerLeaseId,
          controller_lease_expires_at_ms: Number(response?.lease_expires_at_ms || startedAt + 120_000),
          status: 'active',
          runtime_status: 'active',
          viewport_w: viewport.width,
          viewport_h: viewport.height,
          current_tab_id: tabId,
          current_url: response?.nav?.url || url,
          title: response?.nav?.title || 'Browser',
        };
        state.latestTab = {
          id: tabId,
          session_id: sessionId,
          url: response?.nav?.url || url,
          title: response?.nav?.title || 'Browser',
          can_go_back: response?.nav?.can_go_back === true,
          can_go_forward: response?.nav?.can_go_forward === true,
        };
        return { opensNewSession: true, sessionId };
      } catch (error) {
        console.warn('[browser] direct session start unavailable; using durable command', error);
        return command();
      }
    };
    // Sperre loesen, sobald das Ergebnis da ist — Erfolg wie Fehlschlag.
    // Bleibt sie stehen, wartet der Nutzer bis START_SPERRE_MS ablaeuft.
    runBrowserCommand(directStart().finally(() => { state.startPendingSince = 0; }));
  };
  refs.start?.addEventListener('click', () => startNewBrowserSession());
  refs.stop?.addEventListener('click', () => dispatchBrowserCommand(ctx, state, 'browser.session.stop').then(safeLoadAndRender));
  refs.newTab?.addEventListener('click', () => {
    const now = Date.now();
    dispatchBrowserCommand(ctx, state, 'browser.tab.open', {
      tab_id: `browser_tab_${now}`,
      url: refs.address?.value || 'https://example.com',
    }).then(safeLoadAndRender);
  });
  refs.upload?.addEventListener('click', () => {
    const fileId = globalThis.prompt('CTOX Datei-ID für den Upload');
    if (!fileId) return;
    dispatchBrowserCommand(ctx, state, 'browser.upload.select', { file_id: fileId.trim() }).then(safeLoadAndRender);
  });
  refs.controllerAcquire?.addEventListener('click', () => {
    if (state.leaseReacquireInFlight) return;
    state.leaseReacquireInFlight = true;
    state.lastReacquireAtMs = Date.now();
    const leaseId = newBrowserControllerLeaseId();
    state.controllerLeaseId = leaseId;
    runBrowserCommand(
      requestBrowserControllerLease(ctx, state, 'controller.acquire', leaseId)
        .catch((error) => {
          if (state.controllerLeaseId === leaseId) state.controllerLeaseId = '';
          throw error;
        })
        .finally(() => {
          state.leaseReacquireInFlight = false;
        }),
    );
  });
  refs.controllerRelease?.addEventListener('click', () => {
    const leaseId = state.controllerLeaseId;
    runBrowserCommand(
      requestBrowserControllerLease(ctx, state, 'controller.release', leaseId)
        .then((result) => {
          if (state.controllerLeaseId === leaseId) state.controllerLeaseId = '';
          return result;
        }),
    );
  });
  refs.observerGrant?.addEventListener('click', () => {
    const userId = globalThis.prompt('Benutzer-ID des Beobachters');
    if (!userId) return;
    dispatchBrowserCommand(ctx, state, 'browser.observer.grant', { user_id: userId.trim() }).then(safeLoadAndRender);
  });
  refs.observerRevoke?.addEventListener('click', () => {
    const userId = globalThis.prompt('Benutzer-ID des zu entfernenden Beobachters');
    if (!userId) return;
    dispatchBrowserCommand(ctx, state, 'browser.observer.revoke', { user_id: userId.trim() }).then(safeLoadAndRender);
  });
  refs.clipboardCopy?.addEventListener('click', () => dispatchBrowserCommand(ctx, state, 'browser.clipboard.copy', { confirmed: true }).then(safeLoadAndRender));
  refs.clipboardPaste?.addEventListener('click', () => dispatchBrowserCommand(ctx, state, 'browser.clipboard.paste', { confirmed: true }).then(safeLoadAndRender));
  refs.clipboardClear?.addEventListener('click', () => dispatchBrowserCommand(ctx, state, 'browser.clipboard.clear', { confirmed: true }).then(safeLoadAndRender));
  refs.back?.addEventListener('click', () => runBrowserCommand(submitBrowserNav(ctx, state, refs, 'back')));
  refs.forward?.addEventListener('click', () => runBrowserCommand(submitBrowserNav(ctx, state, refs, 'forward')));
  refs.reload?.addEventListener('click', () => runBrowserCommand(submitBrowserNav(ctx, state, refs, 'reload')));
  refs.address?.addEventListener('input', () => {
    state.addressDirty = true;
  });
  refs.address?.addEventListener('focus', (event) => {
    event.currentTarget?.select?.();
  });
  refs.addressClear?.addEventListener('click', () => {
    if (!refs.address) return;
    refs.address.value = '';
    state.addressDirty = true;
    refs.address.focus();
  });
  refs.sendToCtox?.addEventListener('click', () => sendBrowserContextToCtox(ctx, state).then(safeLoadAndRender));
  refs.authAssist?.addEventListener('click', (event) => {
    const permissionButton = event.target?.closest?.('[data-browser-permission-response]');
    if (permissionButton) {
      dispatchBrowserCommand(ctx, state, 'browser.permission.respond', {
        accept: permissionButton.dataset.browserPermissionResponse === 'accept',
        confirmed: true,
      }).then(safeLoadAndRender);
      return;
    }
    const httpAuthButton = event.target?.closest?.('[data-browser-http-auth-response]');
    if (httpAuthButton) {
      const accept = httpAuthButton.dataset.browserHttpAuthResponse === 'accept';
      const secretName = accept ? globalThis.prompt('CTOX Secret-Referenz für HTTP-Auth') : '';
      if (accept && !secretName) return;
      dispatchBrowserCommand(ctx, state, 'browser.http_auth.respond', {
        accept,
        confirmed: true,
        secret_name: secretName?.trim?.() || '',
      }).then(safeLoadAndRender);
      return;
    }
    const webAuthnButton = event.target?.closest?.('[data-browser-webauthn-response]');
    if (webAuthnButton) {
      dispatchBrowserCommand(ctx, state, 'browser.webauthn.respond', {
        accept: webAuthnButton.dataset.browserWebauthnResponse === 'accept',
        confirmed: true,
      }).then(safeLoadAndRender);
      return;
    }
    const dialogButton = event.target?.closest?.('[data-browser-dialog-response]');
    if (dialogButton) {
      const accept = dialogButton.dataset.browserDialogResponse === 'accept';
      const dialog = state.latestSession?.payload?.pending_dialog;
      const value = accept && dialog?.type === 'prompt'
        ? globalThis.prompt(dialog.message || 'Eingabe', dialog.default_value || '')
        : undefined;
      dispatchBrowserCommand(ctx, state, 'browser.dialog.respond', { accept, value }).then(safeLoadAndRender);
      return;
    }
    const fillButton = event.target?.closest?.('[data-browser-credential-fill]');
    if (fillButton) {
      fillWebStackCredential(ctx, state).then(safeLoadAndRender);
      return;
    }
    const completeButton = event.target?.closest?.('[data-browser-auth-complete]');
    if (completeButton) {
      completeWebStackAuthAssist(ctx, state).then(safeLoadAndRender);
      return;
    }
    const captureButton = event.target?.closest?.('[data-browser-web-stack-capture]');
    if (captureButton) sendBrowserContextToCtox(ctx, state, { webStack: true }).then(safeLoadAndRender);
    const extractButton = event.target?.closest?.('[data-browser-web-stack-extract]');
    if (extractButton) extractWebStackFields(ctx, state).then(safeLoadAndRender);
  });
  refs.downloads?.addEventListener('click', (event) => {
    const action = event.target?.closest?.('[data-browser-download-action]');
    if (!action) return;
    dispatchBrowserCommand(ctx, state, `browser.download.${action.dataset.browserDownloadAction}`, {
      download_id: action.dataset.browserDownloadId || '',
    }).then(safeLoadAndRender);
  });
  refs.sessionList?.addEventListener('click', (event) => {
    const tabItem = event.target?.closest?.('[data-browser-tab-id]');
    if (tabItem) {
      const tabId = tabItem.dataset.browserTabId || '';
      const commandType = event.target?.closest?.('[data-browser-tab-close]')
        ? 'browser.tab.close'
        : 'browser.tab.activate';
      dispatchBrowserCommand(ctx, state, commandType, { tab_id: tabId }).then(safeLoadAndRender);
      return;
    }
    const item = event.target?.closest?.('[data-browser-session-id]');
    if (!item) return;
    state.selectedSessionId = item.dataset.browserSessionId || '';
    safeLoadAndRender();
  });
  // LEFT sessions selector. Selecting a session is an in-place is-selected flip
  // across existing rows — it must NOT rebuild the list (a rebuild would clamp
  // the well's scrollTop to 0 and yank the operator to the top). Only the data
  // that drives the MAIN canvas changes, so the reactive load re-renders the
  // work surface while the signature guard leaves the list markup untouched.
  refs.sessions?.addEventListener('click', (event) => {
    const item = event.target?.closest?.('[data-browser-session-id]');
    if (!item) return;
    const sessionId = item.dataset.browserSessionId || '';
    if (sessionId === state.selectedSessionId) return;
    if (sessionId !== state.selectedSessionId) state.controllerLeaseId = '';
    state.directNavigationEpoch += 1;
    state.selectedSessionId = sessionId;
    state.requestedSessionId = '';
    state.directLiveEnabled = typeof ctx.sync?.requestNative === 'function';
    state.directLiveFailures = 0;
    state.lastLiveRejection = '';
    state.directInputFailures = 0;
    state.directInputQueue.length = 0;
    // Bind the work surface synchronously to the clicked row.  Waiting for a
    // second asynchronous collection read leaves latestSession pointing at
    // the previously selected (often disconnected) session.  During that
    // window the controller button and the direct input pump otherwise send
    // acquire/input requests to the old session even though the new row is
    // already painted as selected.
    const selectedSession = latestSession(state.visibleSessions || [], sessionId);
    state.latestSession = selectedSession;
    state.latestFrame = null;
    state.latestDirectFrame = null;
    state.latestTab = null;
    markActiveSession(refs, sessionId);
    safeLoadAndRender();
  });
  const submitAddress = () => {
    // Am 15.08.2026 auf der Kundeninstanz gemessen, und es war die Ursache:
    // Die Entscheidung "navigieren oder neu starten" haing allein an der PACHT,
    // nie an der LEBENDIGKEIT der Sitzung. Zustand der Sitzung des Nutzers:
    //   status = disconnected, Pacht gueltig, Steuerung = der Nutzer selbst.
    // Damit war canNavigate wahr, der Start-Zweig unerreichbar, und jede
    // eingegebene Adresse ging als browser.navigate an eine tote Sitzung.
    // Beleg: letzter Sitzungsstart aus der Oberflaeche 13.08. 12:51, danach in
    // 48 h nur noch 259 x browser.controller.acquire — eine gueltige Pacht auf
    // einer Leiche, die meine eigene Wiederbeschaffung frisch hielt.
    // Eine tote Sitzung MUSS in den Start-Zweig fuehren, sonst gibt es keinen
    // Weg zurueck.
    const url = refs.address?.value || 'https://example.com';
    if (browserAddressAction(state.latestSession, browserSurfaceCanControl(ctx, state)) === 'start') {
      startNewBrowserSession(url);
      return;
    }
    state.addressDirty = false;
    state.notice = 'Browser wird mit CTOX verbunden …';
    safeLoadAndRender();
    runBrowserCommand(submitBrowserNav(ctx, state, refs, 'navigate', { url }));
  };
  refs.form?.addEventListener('submit', (event) => {
    event.preventDefault();
    // OHNE dieses catch stirbt der Klick lautlos. Am 14.08.2026 gemessen: der
    // Nutzer drueckt "Los", nichts passiert, keine Meldung, kein Befehl in der
    // Datenbank. Der Rueckweg war dabei nachweislich intakt — ein serverseitig
    // eingespeister Startbefehl lieferte in 30 s eine aktive Sitzung mit
    // Bildstrom. Wirft irgendetwas in dieser Kette, sieht es fuer den Nutzer
    // aus wie ein toter Knopf.
    try {
      submitAddress();
    } catch (error) {
      console.error('[browser] Adresse absenden fehlgeschlagen', error);
      state.notice = `Der Vorgang konnte nicht gestartet werden: ${error?.message || error}`;
      safeLoadAndRender();
    }
  });
  // Some embedded/webview browsers do not synthesize a form submit for Enter
  // in the address input (this was reproducible in the production UI).  Keep
  // the keyboard path explicit so address navigation is not mouse-only.
  refs.address?.addEventListener('keydown', (event) => {
    if (event.key !== 'Enter' || event.isComposing) return;
    event.preventDefault();
    try {
      submitAddress();
    } catch (error) {
      console.error('[browser] Adresse per Eingabetaste absenden fehlgeschlagen', error);
      state.notice = `Der Vorgang konnte nicht gestartet werden: ${error?.message || error}`;
      safeLoadAndRender();
    }
  });
  installRailToggle(refs);
  installPreciseInput(ctx, refs, state);
  installInputHandlers(ctx, refs, state, scheduleRefresh);
  installCanvasPaste(ctx, refs, state);
  installViewSwitch(ctx, refs, state);
  cleanups.push(startDirectBrowserLive(ctx, refs, state, () => mounted, scheduleRefresh));
  const leaseRenewTimer = globalThis.setInterval(renewControllerLeaseIfNeeded, 30_000);
  cleanups.push(() => globalThis.clearInterval(leaseRenewTimer));
  safeLoadAndRender();
  openRequestedBrowserSession(ctx.args);

  return () => {
    mounted = false;
    for (const timer of requestedStartTimers) globalThis.clearTimeout(timer);
    requestedStartTimers.clear();
    for (const cleanup of cleanups) {
      try { cleanup(); } catch (error) { console.error('[browser] cleanup failed', error); }
    }
    ctx.host.replaceChildren();
  };

  function renewControllerLeaseIfNeeded() {
    // Auch hier wiedererkennen, nicht nur beim Neuzeichnen: Nach einem
    // Zurueckholen laeuft die Liste nicht zwangslaeufig neu durch, und ohne
    // die eigene Kennung schlaegt die Erneuerung unten still fehl — die Pacht
    // altert dann bis zum Ablauf aus, das Bild friert ein, und erst der
    // naechste Rueckholzyklus bringt es zurueck. Auf der Kundeninstanz
    // gemessen: 89s -> 72s -> 54s -> 36s -> 25s ohne eine einzige Erneuerung,
    // obwohl Sichtbarkeit, Fokus und Fensterfokus alle gesetzt waren.
    erkenneEigenePachtWieder(ctx, state);
    const session = state.latestSession;
    const actorIds = browserActorIds(ctx.session);
    const surface = ctx.host?.closest?.('.shell-window');
    if (!shouldRenewControllerLease(session, actorIds, Date.now(), {
      documentVisible: globalThis.document?.visibilityState !== 'hidden',
      documentFocused: globalThis.document?.hasFocus?.() !== false,
      surfaceFocused: Boolean(surface?.classList.contains('is-focused')),
      renewInFlight: state.leaseRenewInFlight,
      controllerLeaseId: state.controllerLeaseId,
    })) {
      // Abgelaufen? Dann nicht erneuern, sondern NEU HOLEN. Ohne diesen Zweig
      // bleibt die Sitzung fuer immer unbedienbar, waehrend sie "Bereit" meldet.
      versucheSteuerungZurueckzuholen();
      return;
    }
    state.leaseRenewInFlight = true;
    requestBrowserControllerLease(ctx, state, 'controller.renew', state.controllerLeaseId)
      .catch((error) => {
        console.warn('[browser] controller lease renewal failed', error);
      })
      .finally(() => {
        state.leaseRenewInFlight = false;
      });
  }

  // writeInputEvent liegt ausserhalb dieses Bereichs und kennt nur ctx/state.
  // Ueber diesen Haken kann ein blockierter Klick die Steuerung anfordern.
  state.steuerungZurueckholen = (optionen) => versucheSteuerungZurueckzuholen(optionen);

  function versucheSteuerungZurueckzuholen(optionen = {}) {
    const session = state.latestSession;
    if (!shouldReacquireControllerLease(session, browserActorIds(ctx.session), Date.now(), {
      reacquireInFlight: state.leaseReacquireInFlight,
      lastReacquireAtMs: state.lastReacquireAtMs,
      uebernahmeDurchInteraktion: Boolean(optionen.uebernahmeDurchInteraktion),
      currentLeaseId: state.controllerLeaseId,
    })) return;
    state.leaseReacquireInFlight = true;
    state.lastReacquireAtMs = Date.now();
    const leaseId = newBrowserControllerLeaseId();
    state.controllerLeaseId = leaseId;
    console.info('[browser] Steuerung abgelaufen — hole sie zurück', { session: session?.id });
    requestBrowserControllerLease(ctx, state, 'controller.acquire', leaseId)
      .then(() => {
        setzeEingabeHinweis(ctx, state, '');
        safeLoadAndRender();
      })
      .catch((error) => {
        if (state.controllerLeaseId === leaseId) state.controllerLeaseId = '';
        console.warn('[browser] Steuerung konnte nicht zurückgeholt werden', error);
        setzeEingabeHinweis(ctx, state, 'Steuerung abgelaufen und konnte nicht zurückgeholt werden.');
      })
      .finally(() => {
        state.leaseReacquireInFlight = false;
      });
  }

  function safeLoadAndRender() {
    if (refreshInFlight) {
      refreshQueued = true;
      return refreshInFlight;
    }
    refreshInFlight = loadAndRender()
      .catch((error) => console.warn('[browser] refresh failed', error))
      .finally(() => {
        refreshInFlight = null;
        if (!refreshQueued || !mounted) return;
        refreshQueued = false;
        safeLoadAndRender();
      });
    return refreshInFlight;
  }

  function runBrowserCommand(promise) {
    return promise
      .then((result) => {
        if (result?.opensNewSession && result.sessionId) {
          state.selectedSessionId = result.sessionId;
          state.requestedSessionId = result.sessionId;
        }
        state.notice = '';
        safeLoadAndRender();
      })
      .catch((error) => {
        state.notice = browserStartErrorMessage(error);
        console.warn('[browser] command failed', error);
        safeLoadAndRender();
      });
  }

  async function loadAndRender() {
    if (!mounted) return;
    // The normal live path carries screenshots and input acknowledgements in
    // the authenticated request/response on browser_sessions. Reading the two
    // legacy collections here would implicitly start their RxDB bridges and
    // recreate the frame hot loop even though no replicated frame is needed.
    const actorIds = browserActorIds(ctx.session);
    // Keep the first interactive paint entirely off the general demand-query
    // channel. That channel may still be hydrating other Business OS modules;
    // the dedicated browser-live channel returns the small owner-scoped
    // session window without waiting behind those transfers.
    const directSessions = state.directLiveEnabled
      ? readDirectBrowserSessions(ctx).catch((error) => {
        console.warn('[browser] direct session list unavailable; using bounded demand query', error);
        return readCollection(browserCollection(ctx, 'browser_sessions'), {
          limit: 50,
          selector: { owner_user_id: { $in: actorIds } },
        });
      })
      : null;
    const replicatedInputs = state.directLiveEnabled
      ? Promise.resolve([])
      : readCollection(browserCollection(ctx, 'browser_input_events'), { limit: 80 });
    const [commands, sessions, requestedSession, initialTabs, inputs, handoffTasks] = await Promise.all([
      state.directLiveEnabled
        ? Promise.resolve([])
        : readCollection(browserCollection(ctx, 'business_commands'), { limit: 50 }),
      directSessions || readCollection(browserCollection(ctx, 'browser_sessions'), {
        limit: 50,
        selector: { owner_user_id: { $in: actorIds } },
      }),
      readDocument(
        browserCollection(ctx, 'browser_sessions'),
        state.requestedSessionId || state.selectedSessionId,
      ),
      state.directLiveEnabled
        ? Promise.resolve([])
        : readCollection(browserCollection(ctx, 'browser_tabs'), { limit: 40 }),
      replicatedInputs,
      state.directLiveEnabled
        ? Promise.resolve([])
        : readCollection(browserCollection(ctx, 'ctox_queue_tasks'), { limit: 50 }),
    ]);
    const directRequestedSession = state.directLiveEnabled && (state.requestedSessionId || state.selectedSessionId)
      ? requestedSession
        || sessions.find((session) => session.id === (state.requestedSessionId || state.selectedSessionId))
        || null
      : requestedSession;
    // The direct live start response is already authoritative enough to make
    // the new session selectable. Do not hide it while its durable projection
    // is still crossing the demand-query channel.
    const visibleSessions = mergeRequestedSession(
      mergeRequestedSession(sessions, directRequestedSession),
      state.latestSession,
    )
      .filter((session) => actorIds.includes(String(session.owner_user_id || '')));
    if (state.selectedSessionId
      && state.selectedSessionId !== state.requestedSessionId
      && !visibleSessions.some((session) => session.id === state.selectedSessionId)) {
      state.selectedSessionId = '';
    }
    const selectedSession = state.selectedSessionId ? latestSession(visibleSessions, state.selectedSessionId) : null;
    const requestedTab = await readDocument(
      browserCollection(ctx, 'browser_tabs'),
      state.directLiveEnabled
        ? ''
        : selectedSession?.current_tab_id || directRequestedSession?.current_tab_id || '',
    );
    const tabs = mergeRequestedDocument(initialTabs, requestedTab);
    const requestedSessionPending = Boolean(state.requestedSessionId && !selectedSession);
    const frameSessionId = selectedSession?.id || state.selectedSessionId || '';
    const frames = !state.directLiveEnabled && frameSessionId
      ? await readCollection(browserCollection(ctx, 'browser_frames'), {
        limit: 20,
        selector: { session_id: frameSessionId },
      })
      : [];
    if (!mounted) return;
    const newestFrame = latestFrame(frames);
    const directOptimisticSession = state.latestSession?.id === state.requestedSessionId
      && state.latestDirectFrame?.session_id === state.requestedSessionId
      ? state.latestSession
      : null;
    state.latestSession = requestedSessionPending
      ? directOptimisticSession
      : selectedSession || latestSession(visibleSessions, newestFrame?.session_id) || latestSession(visibleSessions);
    if (!requestedSessionPending) {
      state.selectedSessionId = state.latestSession?.id || '';
      erkenneEigenePachtWieder(ctx, state);
      if (state.latestSession?.id === state.requestedSessionId
        && browserSessionIsLive(state.latestSession)) {
        state.requestedSessionId = '';
        if (state.notice === 'Browser-Anmeldung wird geöffnet.') state.notice = '';
      }
    }
    const replicatedFrame = latestFrame(frames, state.latestSession?.id);
    state.latestFrame = state.latestDirectFrame?.session_id === state.latestSession?.id
      ? state.latestDirectFrame
      : replicatedFrame;
    state.latestTab = latestTab(tabs, state.latestFrame?.tab_id || state.latestSession?.current_tab_id);
    state.latestCommand = latestBrowserCommand(commands, state.latestSession?.id || state.latestFrame?.session_id);
    applyLatestNavigationResult(state, commands);
    state.browserCommands = latestBrowserCommands(commands, state.latestSession?.id || state.latestFrame?.session_id, 5);
    state.handoffTasks = latestBrowserHandoffTasks(handoffTasks, state.latestSession?.id || state.latestFrame?.session_id, 5);
    state.lastInputSeq = Math.max(
      Number(state.lastInputSeq || 0),
      ...inputs.map((event) => Number(event.seq || 0)).filter(Number.isFinite),
    );
    const renderedTabs = state.latestTab?.id
      ? tabs.map((tab) => tab.id === state.latestTab.id ? { ...tab, ...state.latestTab } : tab)
      : tabs;
    state.visibleSessions = visibleSessions;
    state.tabs = tabs;
    // Left = persistent sessions selector; syncs from the shell grammar state
    // (may not be wired yet on the first paint — that is null-guarded).
    const grammar = refs.sessionsPane?.__ctoxPaneGrammar?.state?.();
    if (grammar) {
      state.leftView = {
        search: String(grammar.search || '').trim().toLowerCase(),
        view: grammar.view === 'list' ? 'list' : 'cards',
        band: grammar.band || 'all',
        filters: grammar.filters || {},
      };
    }
    renderLeftRail(ctx, refs, state);
    renderTabstrip(ctx, refs, state);
    // Auto-reveal: the remote work surface is meaningful once a session is
    // selected (visible = hasSelection && !userCollapsed). No session -> the
    // canvas shows its empty state instead of stale chrome.
    refs.root.classList.toggle('is-session-active', browserWorkbenchVisible(Boolean(state.latestSession?.id)));
    renderSessionList(refs, visibleSessions, renderedTabs, state.latestSession);
    renderSession(refs, state.latestSession, state.latestTab, state.latestFrame, state.latestCommand, state);
    renderAuthAssist(refs, state.latestSession);
    renderAutomationOverlay(refs, state.latestSession);
    renderStatus(refs, state.latestSession, state.latestTab, state.latestFrame, state.latestCommand);
    renderDownloads(refs, state.latestSession);
    renderDiagnostics(refs, state.latestFrame, inputs, state.latestCommand, state.browserCommands, state.handoffTasks);
    renderControls(ctx, refs, state);
    renderNotice(refs, state.notice);
    recoverFrameSyncIfNeeded(ctx, state);
    await renderFrame(refs, state.latestFrame, state);
  }
}

async function readDirectBrowserSessions(ctx) {
  if (typeof ctx.sync?.requestNative !== 'function') return [];
  const response = await ctx.sync.requestNative('ctox.browser.live.v1', {
    op: 'session.list',
  }, {
    collection: 'business_commands',
    requiredCapability: 'ctox-browser-live-v1',
    timeoutMs: 5_000,
  });
  return Array.isArray(response?.sessions)
    ? response.sessions.filter((session) => session && session._deleted !== true)
    : [];
}

function recoverFrameSyncIfNeeded(ctx, state) {
  if (!ctx.sync) return;
  if (state.latestFrame?.data) return;
  const status = String(state.latestSession?.runtime_status || state.latestSession?.status || '').toLowerCase();
  const commandStatus = String(state.latestCommand?.status || state.latestCommand?.task_status || '').toLowerCase();
  const browserIsExpected = ['active', 'running', 'requested', 'pending_command', 'starting'].includes(status)
    || ['pending_sync', 'accepted', 'completed'].includes(commandStatus);
  if (!browserIsExpected) return;
  const now = Date.now();
  if (now - Number(state.lastFrameSyncRecoveryAt || 0) < FRAME_SYNC_RECOVERY_MS) return;
  state.lastFrameSyncRecoveryAt = now;
  for (const collectionName of BROWSER_SYNC_COLLECTIONS) {
    ctx.sync.restartCollection?.(collectionName)
      ?.catch?.((error) => console.warn(`[browser] ${collectionName} sync restart failed`, error));
  }
}

async function submitBrowserNav(ctx, state, refs, action, extra = {}) {
  const commandType = BROWSER_NAV_COMMANDS[action];
  if (!commandType) throw new Error('Unknown browser navigation action: ' + action);
  const payload = action === 'navigate'
    ? { url: extra.url || refs.address?.value || 'https://example.com' }
    : {};
  if (state.directLiveEnabled
    && typeof ctx.sync?.requestNative === 'function'
    && state.latestSession?.id
    && state.controllerLeaseId) {
    const sessionId = state.latestSession.id;
    const navigationEpoch = Number(state.directNavigationEpoch || 0) + 1;
    state.directNavigationEpoch = navigationEpoch;
    const response = await ctx.sync.requestNative('ctox.browser.live.v1', {
      op: action,
      session_id: sessionId,
      lease_id: state.controllerLeaseId,
      ...payload,
    }, {
      collection: 'business_commands',
      requiredCapability: 'ctox-browser-live-v1',
      timeoutMs: 35_000,
    });
    if (response?.nav && directResponseBelongsToSurface(state, sessionId, navigationEpoch)) {
      applyDirectNavigationState(state, response.nav);
    }
    return response;
  }
  return dispatchBrowserCommand(ctx, state, commandType, payload);
}

// SPUR auf dem Befehlsweg. Am 13.08.2026 blieb der Sitzungsstart aus, ohne dass
// irgendwo etwas erschien: kein Chrome-Prozess, keine Protokollzeile im Dienst,
// alle Sitzungen 'disconnected' OHNE Fehlereintrag. Drei Reparaturen (Pacht,
// Startmerkliste, Meldungstrennung) haben das Symptom nicht aufgeloest.
// Statt einen vierten Fix zu raten: messen, wo die Kette reisst.
// Ein Vorgang, der nichts protokolliert, ist von einem Vorgang, der nie
// stattfindet, nicht zu unterscheiden.
function spurBefehl(schritt, commandType, extra = {}) {
  try {
    console.info(`[browser][SPUR] ${schritt} type=${commandType} ${JSON.stringify(extra)}`);
  } catch { console.info(`[browser][SPUR] ${schritt} type=${commandType}`); }
}

async function requestBrowserControllerLease(ctx, state, operation, leaseId) {
  const commandType = `browser.${operation}`;
  const sessionId = state.latestSession?.id;
  if (!sessionId || !leaseId) {
    throw new Error('Browser-Sitzung und Steuerpacht werden benötigt.');
  }
  if (typeof ctx.sync?.requestNative !== 'function') {
    return dispatchBrowserCommand(ctx, state, commandType, { lease_id: leaseId });
  }
  try {
    const response = await ctx.sync.requestNative('ctox.browser.live.v1', {
      op: operation,
      session_id: sessionId,
      lease_id: leaseId,
    }, {
      collection: 'business_commands',
      requiredCapability: 'ctox-browser-live-v1',
      // A lease operation writes the session projection, and on a large store
      // that write is not a 5s affair: measured 5.6s on a customer instance
      // with a 947 MB database, so EVERY reacquisition failed with "exceeded
      // 5000ms" and the surface reported "Steuerung abgelaufen und konnte
      // nicht zurückgeholt werden" while the channel was healthy. This runs on
      // a 30s background timer, so a longer deadline costs nothing; a frame
      // request keeps its own, much tighter budget.
      timeoutMs: 15_000,
    });
    const released = operation === 'controller.release';
    const actorId = browserActorIds(ctx.session)[0] || '';
    state.latestSession = {
      ...(state.latestSession || {}),
      controller_user_id: released ? '' : actorId,
      controller_lease_id: released ? '' : String(response?.lease_id || leaseId),
      controller_lease_expires_at_ms: released
        ? 0
        : Number(response?.lease_expires_at_ms || Date.now() + 120_000),
    };
    if (!released) state.controllerLeaseId = String(response?.lease_id || leaseId);
    state.directLiveEnabled = true;
    setzeEingabeHinweis(ctx, state, '');
    return response;
  } catch (error) {
    console.warn('[browser] Direkte Steuerpacht nicht verfügbar; nutze bestätigten Befehlsweg.', error);
    const result = await dispatchBrowserCommand(ctx, state, commandType, { lease_id: leaseId });
    const released = operation === 'controller.release';
    const actorId = browserActorIds(ctx.session)[0] || '';
    state.latestSession = {
      ...(state.latestSession || {}),
      controller_user_id: released ? '' : actorId,
      controller_lease_id: released ? '' : leaseId,
      controller_lease_expires_at_ms: released ? 0 : Date.now() + 120_000,
    };
    return result;
  }
}

async function dispatchBrowserCommand(ctx, state, commandType, payloadPatch = {}) {
  spurBefehl('1-aufgerufen', commandType, {
    sitzung: state.latestSession?.id || '-',
    status: state.latestSession?.status || '-',
    neueSitzung: payloadPatch.new_session === true,
  });
  const requiresController = browserCommandRequiresController(commandType, state.latestSession);
  if (requiresController && !browserSurfaceCanControl(ctx, state)) {
    spurBefehl('2-ABBRUCH-keine-steuerung', commandType, {
      grund: eingabeSperrgrund(ctx, state) || '(unbekannt)',
    });
    throw new Error('Dieses Browser-Fenster ist nicht aktiv. Aktivieren Sie das Fenster oder übernehmen Sie die Steuerung.');
  }
  spurBefehl('2-steuerung-ok', commandType, { brauchtSteuerung: requiresController });
  const now = Date.now();
  const opensNewSession = payloadPatch.new_session === true;
  const requestedSessionId = browserSessionIdFromArgs(payloadPatch);
  const sessionId = requestedSessionId || (opensNewSession
    ? `${userSessionPrefix(ctx.session)}_${now}`
    : state.latestSession?.id || `${userSessionPrefix(ctx.session)}_default`);
  const tabId = String(payloadPatch.tab_id || (opensNewSession
    ? `browser_tab_${now}`
    : state.latestTab?.id || state.latestSession?.current_tab_id || DEFAULT_TAB_ID));
  const commandId = `browser_cmd_${now}_${Math.random().toString(36).slice(2, 10)}`;
  const payload = {
    session_id: sessionId,
    tab_id: tabId,
    viewport_w: VIEWPORT.width,
    viewport_h: VIEWPORT.height,
    ...payloadPatch,
  };
  if (requiresController) payload.lease_id = state.controllerLeaseId;
  delete payload.new_session;
  if (payload.url) payload.url = normalizeUrl(payload.url);
  // Browser commands must not race the command collection during a freshly
  // mounted window. Only await the command path here; awaiting all browser
  // projections can deadlock while an existing peer is resyncing.
  const command = {
    id: commandId,
    command_id: commandId,
    module: 'browser',
    command_type: commandType,
    type: commandType,
    record_id: sessionId,
    inbound_channel: 'browser',
    status: 'pending_sync',
    payload,
    client_context: {
      source: 'business-os.browser.runtime',
      module_id: 'browser',
      actor: actorContext(ctx.session),
      handled_by: 'native-rxdb-peer',
    },
    created_at_ms: now,
    updated_at_ms: now,
    sync_queue_tasks: false,
  };
  spurBefehl('3-befehl-gebaut', commandType, { commandId, sessionId, tabId });
  const commandBus = requireCommandBus(ctx);
  spurBefehl('4-befehlsbus-da', commandType, { busDa: !!commandBus });
  const waitsForRuntime = [
    'browser.session.start',
    'browser.navigate',
    'browser.reload',
    'browser.back',
    'browser.forward',
    'browser.reset',
  ].includes(commandType);
  // The command bus owns collection readiness and the confirmed push. A
  // second startCollection() preflight here can restart a degraded bridge
  // while another Browser command awaits its receipt, cancelling that write.
  console.info(`[browser] submitting command command_type=${commandType} command_id=${commandId} session_id=${sessionId}`);
  try {
    // Compatibility contract for non-runtime commands:
    // commandBus.dispatch(command, { until: 'accepted' })
    // The command bus owns collection readiness, capability acquisition and
    // the confirmed RxDB push. A separate startCollection() preflight here
    // used to abort before dispatch whenever the mounted window's bridge was
    // stale, so the native service never saw the session-start command.
    await commandBus.dispatch(command, {
      until: waitsForRuntime ? 'terminal' : 'accepted',
      ...(waitsForRuntime ? { timeoutMs: 150_000 } : {}),
    });
  } catch (error) {
    console.error(
      `[browser] command submission failed command_type=${commandType} command_id=${commandId} session_id=${sessionId}`,
      error,
    );
    throw error;
  }
  if (waitsForRuntime) await refreshBrowserProjections(ctx);
  return { commandId, sessionId, tabId, opensNewSession };
}

async function ensureRequestedBrowserSession(ctx, state, args = {}) {
  const request = browserAuthRequestFromArgs(args);
  if (!request) return false;
  state.selectedSessionId = request.session_id;
  state.requestedSessionId = request.session_id;
  // Die Merkliste verhindert, dass derselbe Start doppelt losgeschickt wird,
  // solange er noch laeuft. Sie darf aber nicht dauerhaft merken: eine Sitzung,
  // die einmal erfolgreich startete und spaeter wegbricht (Dienstneustart,
  // Prozessende), blieb bis 13.08.2026 fuer immer in der Liste — der Eintrag
  // wurde nur im FEHLERfall entfernt. Danach wurde jeder weitere Startversuch
  // stumm verworfen.
  // Auf der Kundeninstanz gemessen: 34 Sitzungen, davon 0 aktiv, kein
  // Chrome-Prozess, keine Protokollzeile in 20 Minuten — und weder der
  // Start-Knopf noch das Plus-Symbol bewirkten etwas. Es lief vorher; nach
  // einem Dienstneustart nie wieder.
  // Deshalb zuerst den Zustand lesen, dann die Merkliste bewerten: was nicht
  // mehr laeuft, ist auch nicht mehr "in Arbeit".
  const existing = await browserCollection(ctx, 'browser_sessions')?.findOne(request.session_id).exec();
  const existingData = existing?.toJSON?.() || existing;
  if (!browserSessionNeedsStart(existingData)) {
    // Laeuft bereits — Merkliste aufraeumen, damit ein spaeteres Wegbrechen
    // wieder startbar ist.
    state.requestedSessionStarts.delete(request.session_id);
    return false;
  }
  if (state.requestedSessionStarts.has(request.session_id)) {
    console.info('[browser] Start bereits angefordert, warte auf Ergebnis', request.session_id);
    return false;
  }

  state.requestedSessionStarts.add(request.session_id);
  try {
    state.controllerLeaseId = newBrowserControllerLeaseId();
    await dispatchBrowserCommand(ctx, state, 'browser.session.start', {
      ...request,
      lease_id: state.controllerLeaseId,
    });
    return true;
  } catch (error) {
    state.requestedSessionStarts.delete(request.session_id);
    throw error;
  }
}

function browserAuthRequestFromArgs(value) {
  const sessionId = browserSessionIdFromArgs(value);
  const purpose = String(value?.purpose || '').trim();
  const targetUrl = String(value?.target_url || value?.targetUrl || '').trim();
  if (!sessionId || purpose !== 'web_stack_auth' || !targetUrl) return null;
  const tabId = String(value?.tab_id || value?.tabId || `browser_tab_${sessionId}`).trim();
  const sourceId = String(value?.source_id || value?.sourceId || '').trim();
  const allowedDomains = Array.isArray(value?.allowed_domains)
    ? value.allowed_domains.map((entry) => String(entry || '').trim()).filter(Boolean)
    : [];
  const captureScript = String(value?.capture_script || value?.captureScript || '').trim();
  const secretName = String(value?.secret_name || value?.required_secret_name || '').trim();
  const verifySelector = String(value?.verify_selector || value?.verifySelector || '').trim();
  const authAssistCommandId = String(value?.auth_assist_command_id || value?.command_id || '').trim();
  const authAssistTaskId = String(value?.auth_assist_task_id || value?.execution_task_id || '').trim();
  const requestingTaskId = String(value?.requesting_task_id || '').trim();
  const instruction = String(value?.instruction || '').trim();
  return {
    session_id: sessionId,
    tab_id: tabId,
    url: targetUrl,
    target_url: targetUrl,
    source_id: sourceId,
    purpose,
    allowed_domains: allowedDomains,
    capture_script: captureScript,
    verify_selector: verifySelector,
    secret_name: secretName,
    auth_assist_command_id: authAssistCommandId,
    auth_assist_task_id: authAssistTaskId,
    requesting_task_id: requestingTaskId,
    instruction,
    auth_assist_status: 'pending',
    profile_mode: 'persistent',
    secret_value_in_rxdb: false,
  };
}

function browserStartErrorMessage(error) {
  const code = String(error?.code || '').toLowerCase();
  if (code === 'auth_required') return 'Die Browser-Anmeldung benötigt eine neue Business-OS-Autorisierung.';
  if (code === 'native_unavailable') return 'Die Browser-Anmeldung konnte noch nicht mit CTOX verbunden werden. Bitte erneut versuchen.';
  // Zwei voellig verschiedene Fehler trugen bis 13.08.2026 dieselbe Meldung.
  // sync_unavailable heisst "keine Verbindung". projection_delayed heisst
  // "Verbindung steht, die Antwort kam nur nicht rechtzeitig zurueck".
  // Auf der Kundeninstanz gemessen: alle fuenf Browser-Collections
  // 'connected' — und trotzdem stand da "CTOX ist nicht mit dem
  // Browser-Datenkanal verbunden". Die Meldung behauptete das Gegenteil des
  // gemessenen Zustands und schickte den Nutzer auf die falsche Faehrte
  // ("Verbindung erneut aufbauen"), waehrend in Wahrheit nur die Antwort
  // ausstand. Eine Zeitueberschreitung ist kein Ausfall.
  if (code === 'sync_unavailable') {
    return 'CTOX ist nicht mit dem Browser-Datenkanal verbunden. Der Browser wurde nicht gestartet. Bitte die Verbindung erneut aufbauen und dann erneut versuchen.';
  }
  if (code === 'projection_delayed') {
    return 'CTOX hat den Befehl angenommen, aber noch nicht bestätigt. Die Verbindung steht — der Vorgang läuft möglicherweise noch. Bitte einen Moment warten und die Ansicht neu laden, bevor Sie es erneut versuchen.';
  }
  const message = String(error?.message || '').trim();
  return message
    ? `Die Browser-Anmeldung konnte nicht geöffnet werden: ${message}`
    : 'Die Browser-Anmeldung konnte nicht geöffnet werden.';
}

function browserSessionError(session) {
  const direct = String(session?.last_error || session?.error || '').trim();
  if (direct) return direct;
  const status = String(session?.runtime_status || session?.status || '').toLowerCase();
  if (['active', 'running', 'capturing'].includes(status)) return '';
  return String(session?.payload?.last_error || session?.payload?.error || '').trim();
}

function browserSessionIsLive(session) {
  const status = String(session?.runtime_status || session?.status || '').toLowerCase();
  return status === 'active';
}

// Warum ein Start uebersprungen wird — im Klartext, nicht als Schweigen.
// Leerer Rueckgabewert heisst: starten ist erlaubt.
//
// Ein Start ist teuer: neue Sitzungskennung, neuer Chrome, neuer Eintrag in der
// Liste. Zwei Klicks duerfen daraus nicht zwei Browser machen.
// Die Sperre laeuft nach START_SPERRE_MS ab, damit ein haengengebliebener Start
// den Nutzer nicht dauerhaft aussperrt — genau der Fehler, den die
// Startmerkliste am 14.08.2026 hatte.
const START_SPERRE_MS = 8000;

function startSperrgrund(state, jetzt = Date.now()) {
  const seit = Number(state?.startPendingSince || 0);
  if (Number.isFinite(seit) && seit > 0 && jetzt - seit < START_SPERRE_MS) {
    return 'Eine Sitzung wird bereits gestartet …';
  }
  // Laeuft die angeforderte Sitzung schon oder faehrt sie gerade hoch, ist ein
  // zweiter Start reine Verschwendung.
  const angefordert = String(state?.requestedSessionId || '');
  const sitzung = state?.latestSession;
  if (angefordert && sitzung?.id === angefordert && !browserSessionNeedsStart(sitzung)) {
    return 'Diese Sitzung läuft bereits.';
  }
  return '';
}

// Was die Adressleiste tun muss: navigieren oder eine neue Sitzung starten.
// Beide Bedingungen sind noetig. Die Steuerung allein genuegt nicht — eine
// gueltige Pacht auf einer toten Sitzung ist genau die Falle, in der die
// Kundeninstanz vom 13.08. bis 15.08.2026 sass.
function browserAddressAction(session, canControl) {
  return browserSessionIsLive(session) && canControl ? 'navigate' : 'start';
}

function browserSessionNeedsStart(session) {
  if (!session?.id) return true;
  const status = String(session.runtime_status || session.status || '').toLowerCase();
  return !['active', 'starting'].includes(status);
}

function browserStartErrorIsRetryable(error) {
  const code = String(error?.code || '').toLowerCase();
  return ['projection_delayed', 'sync_unavailable', 'native_unavailable'].includes(code);
}

function userSessionPrefix(session) {
  const raw = String(session?.user?.id || session?.userId || 'browser-user');
  const safe = raw.toLowerCase().replace(/[^a-z0-9_-]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 64);
  return `browser_session_${safe || 'user'}`;
}

function selectedViewport(select) {
  const [width, height] = String(select?.value || '1280x720').split('x').map(Number);
  return {
    width: Number.isFinite(width) ? Math.max(320, Math.min(3840, width)) : VIEWPORT.width,
    height: Number.isFinite(height) ? Math.max(240, Math.min(2160, height)) : VIEWPORT.height,
  };
}

async function sendBrowserContextToCtox(ctx, state, options = {}) {
  const session = state.latestSession;
  const tab = state.latestTab;
  const frame = state.latestFrame;
  if (!session?.id) {
    state.notice = 'Kein Browser-Fenster zum Senden geoeffnet.';
    return;
  }
  const payloadMetadata = session.payload || {};
  const now = Date.now();
  const url = tab?.url || session.current_url || '';
  const title = browserDisplayTitle(tab, session, url);
  const sourceId = payloadMetadata.source_id || '';
  const captureScript = payloadMetadata.capture_script || '';
  const verifySelector = payloadMetadata.verify_selector || '';
  const purpose = payloadMetadata.purpose || '';
  const sourceModule = options.webStack ? 'web_stack' : 'browser';
  const browserContext = {
    session_id: session.id,
    tab_id: tab?.id || session.current_tab_id || '',
    url,
    title,
    status: session.runtime_status || session.status || '',
    purpose,
    source_id: sourceId,
    capture_script: captureScript,
    verify_selector: verifySelector,
    frame_id: frame?.id || '',
    frame_seq: frame?.seq || 0,
    frame_captured_at_ms: frame?.captured_at_ms || 0,
    frame_expires_at_ms: frame?.expires_at_ms || 0,
    frame_mime_type: frame?.mime_type || '',
    frame_width: frame?.width || 0,
    frame_height: frame?.height || 0,
    frame_size_bytes: frame?.size_bytes || 0,
    frame_hash: frame?.frame_hash || '',
    frame_data_in_payload: false,
  };
  const commandId = `browser_context_${now}_${Math.random().toString(36).slice(2, 10)}`;
  const payload = {
    title: options.webStack && sourceId ? `Web Stack Browser: ${sourceId}` : `Browser: ${title}`,
    instruction: url
      ? `Use this browser context from ${url}. The screenshot is available as the referenced browser frame record.`
      : 'Use this browser context. The screenshot is available as the referenced browser frame record.',
    source_module: sourceModule,
    required_skills: options.webStack ? ['browser-context', 'web-stack', 'ctox'] : ['browser-context', 'ctox'],
    browser_context: browserContext,
    source_id: sourceId,
    capture_script: captureScript,
    secret_value_in_payload: false,
  };
  await startCommandSync(ctx);
  await requireCommandBus(ctx).dispatch({
      id: commandId,
      module: 'ctox',
      command_type: 'ctox.browser_context.capture',
      record_id: session.id,
      inbound_channel: 'browser',
      payload,
      client_context: {
        source: options.webStack ? 'business-os.browser.web-stack-capture' : 'business-os.browser.context',
        module_id: 'browser',
        actor: actorContext(ctx.session),
        browser_context: browserContext,
      },
    });
  const target = options.webStack ? 'Web Stack' : 'CTOX';
  state.notice = frame?.id
    ? `Browser-Kontext wurde an ${target} uebergeben.`
    : `Browser-Kontext wurde an ${target} uebergeben. Sobald die Seite geladen ist, wird auch die aktuelle Ansicht referenziert.`;
}

async function extractWebStackFields(ctx, state) {
  const session = state.latestSession;
  const tab = state.latestTab;
  const frame = state.latestFrame;
  const payload = session?.payload || {};
  if (!session?.id || !payload.capture_script) {
    state.notice = 'Keine Web-Stack-Uebergabe fuer dieses Browser-Fenster verfuegbar.';
    return;
  }
  const now = Date.now();
  const browserContext = {
    session_id: session.id,
    tab_id: tab?.id || session.current_tab_id || '',
    url: tab?.url || session.current_url || '',
    title: browserDisplayTitle(tab, session, tab?.url || session.current_url || ''),
    status: session.runtime_status || session.status || '',
    purpose: payload.purpose || '',
    source_id: payload.source_id || '',
    capture_script: payload.capture_script || '',
    verify_selector: payload.verify_selector || '',
    frame_id: frame?.id || '',
    frame_seq: frame?.seq || 0,
    frame_captured_at_ms: frame?.captured_at_ms || 0,
    frame_expires_at_ms: frame?.expires_at_ms || 0,
    frame_mime_type: frame?.mime_type || '',
    frame_width: frame?.width || 0,
    frame_height: frame?.height || 0,
    frame_size_bytes: frame?.size_bytes || 0,
    frame_hash: frame?.frame_hash || '',
    frame_data_in_payload: false,
  };
  const artifact = {
    kind: 'browser_context',
    schema_version: 1,
    stream: 'rxdb',
    source_module: 'web_stack',
    source_id: payload.source_id || '',
    capture_script: payload.capture_script || '',
    browser_context: browserContext,
    sensitivity: 'browser_context_reference',
    secret_value_in_payload: false,
    frame_data_in_payload: false,
  };
  const commandId = `browser_extract_${now}_${Math.random().toString(36).slice(2, 10)}`;
  const command = {
    id: commandId,
    command_id: commandId,
    module: 'ctox',
    command_type: 'browser.capture.extract',
    record_id: session.id,
    inbound_channel: 'browser',
    payload: {
      session_id: session.id,
      source_id: payload.source_id || '',
      capture_script: payload.capture_script || '',
      frame_id: frame?.id || '',
      browser_context_artifact: artifact,
      secret_value_in_payload: false,
      frame_data_in_payload: false,
    },
    client_context: {
      source: 'business-os.browser.web-stack-extract',
      module_id: 'browser',
      actor: actorContext(ctx.session),
    },
    created_at_ms: now,
    updated_at_ms: now,
  };
  await startCommandSync(ctx);
  await requireCommandBus(ctx).dispatch(command);
  state.notice = 'CTOX liest die Seite fuer den Web Stack aus.';
}

async function completeWebStackAuthAssist(ctx, state) {
  const session = state.latestSession;
  if (!session?.id) {
    state.notice = 'Kein Web-Stack-Browserfenster geoeffnet.';
    return;
  }
  if (session.payload?.purpose !== 'web_stack_auth') {
    state.notice = 'Dieses Browserfenster gehoert nicht zu einer Web-Stack-Anmeldung.';
    return;
  }
  const now = Date.now();
  const commandId = `web_stack_auth_complete_${now}_${Math.random().toString(36).slice(2, 10)}`;
  const payload = {
    session_id: session.id,
    tab_id: state.latestTab?.id || session.current_tab_id || '',
    source_id: session.payload?.source_id || '',
    secret_name: session.payload?.secret_name || '',
    lease_id: state.controllerLeaseId,
    confirmed: true,
    auth_assist_command_id: session.payload?.auth_assist_command_id || '',
    auth_assist_task_id: session.payload?.auth_assist_task_id || '',
    requesting_task_id: session.payload?.requesting_task_id || '',
    completed_at_ms: now,
    browser_stream: 'rxdb',
    secret_value_in_rxdb: false,
  };
  await startCommandSync(ctx);
  await requireCommandBus(ctx).dispatch({
      id: commandId,
      module: 'browser',
      command_type: 'web_stack.auth_assist.complete',
      record_id: session.id,
      inbound_channel: 'browser',
      payload,
      client_context: {
        source: 'business-os.browser.auth-assist',
        module_id: 'browser',
        actor: actorContext(ctx.session),
      },
    });
  state.notice = 'Anmeldung bestätigt. CTOX setzt denselben Recherchekontext fort.';
}

async function fillWebStackCredential(ctx, state) {
  const session = state.latestSession;
  if (!session?.id) {
    state.notice = 'Kein Web-Stack-Browserfenster geoeffnet.';
    return;
  }
  const secretName = session.payload?.secret_name || '';
  if (!secretName) {
    state.notice = 'Fuer diese Quelle ist kein Zugang im CTOX Secret Store hinterlegt.';
    return;
  }
  const now = Date.now();
  const commandId = `browser_credential_fill_${now}_${Math.random().toString(36).slice(2, 10)}`;
  const payload = {
    session_id: session.id,
    tab_id: state.latestTab?.id || session.current_tab_id || '',
    lease_id: state.controllerLeaseId,
    source_id: session.payload?.source_id || '',
    secret_scope: 'credentials',
    secret_name: secretName,
    field_role: session.payload?.credential_field_role || 'both',
    confirmed: true,
    browser_stream: 'rxdb',
    secret_value_in_rxdb: false,
  };
  const selector = String(session.payload?.credential_selector || session.payload?.selector || '').trim();
  if (selector) payload.selector = selector;
  await startCommandSync(ctx);
  await requireCommandBus(ctx).dispatch({
      id: commandId,
      module: 'browser',
      command_type: 'browser.credential.fill',
      record_id: session.id,
      inbound_channel: 'browser',
      payload,
      client_context: {
        source: 'business-os.browser.credential-fill',
        module_id: 'browser',
        actor: actorContext(ctx.session),
      },
    }, { until: 'terminal', timeoutMs: 150_000 });
  await refreshBrowserProjections(ctx);
  state.notice = 'Zugangsdaten wurden eingesetzt.';
}

async function startCommandSync(ctx) {
  return startBrowserRuntimeSync(ctx, ['business_commands']);
}

async function refreshBrowserProjections(ctx) {
  if (!ctx.sync?.restartCollection) return;
  await Promise.all(
    BROWSER_SYNC_COLLECTIONS
      .map((collection) => ctx.sync.restartCollection(collection, { forceDirect: true })),
  );
}

async function startBrowserRuntimeSync(ctx, collections = [
  'business_commands',
  'browser_sessions',
  'browser_tabs',
  'browser_frames',
  'browser_input_events',
]) {
  if (!ctx.sync?.startCollection) {
    const error = new Error('CTOX Browser-Datenkanal ist nicht verfügbar.');
    error.code = 'sync_unavailable';
    throw error;
  }
  // startCollection kann nach einem Dienst-Neustart ewig haengen: die Bruecke
  // wartet auf ein Ready-Ereignis, das nie kommt. Gemessen am 20.08.2026 auf
  // managed production tenant: jeder Sitzungsstart blieb VOR dem Dispatch stumm stehen —
  // kein Fehler, kein Protokoll, kein Chromium. Der Befehlsbus prueft seine
  // Bereitschaft selbst; nach 8 s wird deshalb abgebrochen statt gewartet,
  // und der Aufrufer faehrt ueber seinen Fehlerpfad fort.
  const zeitlimit = new Promise((unused, reject) => setTimeout(() => {
    const error = new Error('Browser-Datenkanal antwortet nicht (Zeitlimit nach 8 s).');
    error.code = 'sync_unavailable';
    reject(error);
  }, 8_000));
  await Promise.race([
    Promise.all(collections.map((collection) => ctx.sync.startCollection(collection))),
    zeitlimit,
  ]);
}

function actorContext(session) {
  const user = session?.user || {};
  return {
    user_id: user.id || session?.userId || '',
    display_name: user.display_name || user.name || '',
    role: user.role || '',
  };
}

function requireCommandBus(ctx) {
  if (!ctx?.commandBus?.dispatch) {
    throw new Error('CTOX command bus is unavailable. The action was not submitted.');
  }
  return ctx.commandBus;
}

// custom: Tab-Leiste der fernen Sitzung.
//
// Der Runner fuehrt seit jeher mehrere Tabs (tab_open/tab_activate/tab_close),
// sichtbar waren sie in der Oberflaeche aber nur als Zaehler "0 Tabs" -- und
// der Knopf "Neuer Tab" schrieb einen Befehl, den niemand ausfuehrte.
//
// Bei einem einzigen Tab bleibt die Leiste verborgen: eine Leiste mit genau
// einem Eintrag kostet Hoehe und sagt nichts.
function tabsDerSitzung(state) {
  // Zuerst die Live-Liste des Runners: sie beschreibt, was der ferne Browser
  // WIRKLICH offen hat, und braucht keine Replikation.
  if (Array.isArray(state.liveTabs) && state.liveTabs.length) {
    return state.liveTabs
      .map((tab) => ({
        id: String(tab.id || tab.tab_id || ''),
        title: tab.title || '',
        url: tab.url || '',
        // Der Runner setzt `active` aus dem tatsaechlich gewaehlten Page-Objekt
        // -- im Gegensatz zur Projektion, die jeden angefassten Tab als aktiv
        // schreibt.
        liveAktiv: tab.active === true,
      }))
      .filter((tab) => tab.id);
  }
  const sessionId = state.latestSession?.id;
  if (!sessionId) return [];
  return (Array.isArray(state.tabs) ? state.tabs : [])
    .filter((tab) => tab.session_id === sessionId && tab.status !== 'closed')
    .sort((a, b) => Number(a.position || 0) - Number(b.position || 0)
      || String(a.id).localeCompare(String(b.id)));
}

// Der Frame-Takt liefert die Tab-Liste des Runners mit, laeuft aber nur bei
// Fokus UND gehaltener Pacht. Ohne gezielten Abruf bliebe die Leiste deshalb
// leer, bis jemand zufaellig in die Buehne klickt -- gemessen genau so: der
// Tab war offen, der Befehl "completed", die Leiste unsichtbar.
async function holeLiveZustand(ctx, state) {
  const sessionId = state.latestSession?.id;
  if (!sessionId || !state.controllerLeaseId) return;
  if (typeof ctx.sync?.requestNative !== 'function') return;
  try {
    const antwort = await ctx.sync.requestNative('ctox.browser.live.v1', {
      session_id: sessionId,
      lease_id: state.controllerLeaseId,
      events: [],
      frame_after_ms: 0,
    }, {
      collection: 'business_commands',
      requiredCapability: 'ctox-browser-live-v1',
      timeoutMs: 10_000,
    });
    if (antwort?.nav) applyDirectNavigationState(state, antwort.nav);
  } catch (error) {
    console.warn('[browser] Live-Zustand konnte nicht geholt werden', error);
  }
}

// Ein Tab-Befehl endet wie jede andere Aktion: auffrischen, und ein
// Fehlschlag landet im Hinweisband statt in einer stillen Konsolenzeile --
// "keine Steuerung" ist der haeufigste Fall und muss erklaert werden.
function tabBefehl(ctx, state, commandType, tabId) {
  return dispatchBrowserCommand(ctx, state, commandType, { tab_id: tabId })
    .then(() => holeLiveZustand(ctx, state))
    .then(() => state.refresh?.())
    .catch((error) => {
      state.notice = `Der Vorgang konnte nicht gestartet werden: ${error?.message || error}`;
      state.refresh?.();
    });
}

function renderTabstrip(ctx, refs, state) {
  const leiste = refs.tabstrip;
  if (!leiste) return;
  // Einmal pro Sitzung nachfassen, falls der Takt noch nicht laeuft.
  if (!Array.isArray(state.liveTabs)
    && state.latestSession?.id
    && state.controllerLeaseId
    && state.liveTabsAbrufFuer !== state.latestSession.id) {
    state.liveTabsAbrufFuer = state.latestSession.id;
    holeLiveZustand(ctx, state).then(() => renderTabstrip(ctx, refs, state));
  }
  const tabs = tabsDerSitzung(state);
  if (tabs.length < 2) {
    leiste.hidden = true;
    leiste.replaceChildren();
    return;
  }
  // Welcher Tab aktiv ist, sagt die Sitzung -- NICHT das Feld `active` am Tab:
  // upsert_browser_tab schreibt dort hart `true` fuer jeden angefassten Tab und
  // setzt die uebrigen nicht zurueck, sodass jeder je benutzte Tab als aktiv
  // gilt. `current_tab_id` wird dagegen bei jedem Befehl mitgefuehrt.
  let aktiverTab = String(tabs.find((tab) => tab.liveAktiv)?.id
    || state.liveActiveTabId
    || state.latestSession?.current_tab_id
    || '');
  // Nach dem Schliessen zeigt current_tab_id noch auf den geschlossenen Tab --
  // der Runner ist dann laengst auf einen anderen gewechselt. Ohne diesen
  // Rueckfall waere kurzzeitig gar kein Tab hervorgehoben.
  if (!tabs.some((tab) => tab.id === aktiverTab)) aktiverTab = tabs[0].id;
  // Nur neu bauen, wenn sich wirklich etwas geaendert hat -- sonst verliert
  // ein Klick waehrend eines Renderdurchlaufs sein Ziel.
  const signatur = tabs.map((tab) => `${tab.id}:${tab.id === aktiverTab ? 1 : 0}:${tab.title || tab.url || ''}`).join('|');
  if (leiste.dataset.signatur === signatur) return;
  leiste.dataset.signatur = signatur;
  leiste.hidden = false;
  leiste.replaceChildren(...tabs.map((tab) => {
    const istAktiv = tab.id === aktiverTab;
    const eintrag = document.createElement('div');
    eintrag.className = 'browser-tab' + (istAktiv ? ' is-active' : '');
    const knopf = document.createElement('button');
    knopf.type = 'button';
    knopf.className = 'browser-tab-title';
    knopf.setAttribute('role', 'tab');
    knopf.setAttribute('aria-selected', istAktiv ? 'true' : 'false');
    const beschriftung = String(tab.title || tab.url || tab.id).trim();
    knopf.textContent = beschriftung;
    knopf.title = String(tab.url || beschriftung);
    knopf.addEventListener('click', () => {
      if (istAktiv) return;
      tabBefehl(ctx, state, 'browser.tab.activate', tab.id);
    });
    const schliessen = document.createElement('button');
    schliessen.type = 'button';
    schliessen.className = 'browser-tab-close';
    schliessen.setAttribute('aria-label', tBrowser(ctx, 'tabClose', 'Tab schließen'));
    schliessen.textContent = '\u00d7';
    schliessen.addEventListener('click', (event) => {
      event.stopPropagation();
      tabBefehl(ctx, state, 'browser.tab.close', tab.id);
    });
    eintrag.append(knopf, schliessen);
    return eintrag;
  }));
}

// custom: Eigenes Kontextmenue ueber der Buehne.
//
// Ohne preventDefault oeffnet der AEUSSERE Browser sein eigenes Menue ueber
// dem Remote-Bild. Dessen Eintraege ("Zurueck", "Neu laden") wirken dann auf
// die Shell statt auf die ferne Seite -- also genau falsch herum.
//
// Die Eintraege loesen die vorhandenen Knoepfe aus, statt die Befehle noch
// einmal zu bauen: Steuerungspflicht, Sperren und Fehlermeldungen existieren
// damit weiterhin nur an einer Stelle, und ein gesperrter Knopf graut den
// Menueeintrag automatisch mit aus.
function installContextMenu(refs) {
  const menu = refs.contextMenu;
  if (!menu || !refs.canvas) return;
  const ziele = {
    back: () => refs.back,
    forward: () => refs.forward,
    reload: () => refs.reload,
    copy: () => refs.clipboardCopy,
    paste: () => refs.clipboardPaste,
  };
  const schliessen = () => {
    menu.hidden = true;
  };
  refs.canvas.addEventListener('contextmenu', (event) => {
    event.preventDefault();
    const shell = menu.offsetParent || refs.canvas.parentElement;
    const box = shell?.getBoundingClientRect();
    if (!box) return;
    for (const eintrag of menu.querySelectorAll('[data-browser-context-action]')) {
      const knopf = ziele[eintrag.dataset.browserContextAction]?.();
      eintrag.disabled = !knopf || knopf.disabled;
    }
    menu.hidden = false;
    // Innerhalb der Buehne halten, damit das Menue am rechten oder unteren
    // Rand nicht aus dem Fenster laeuft.
    const breite = menu.offsetWidth || 160;
    const hoehe = menu.offsetHeight || 200;
    const x = Math.min(Math.max(0, event.clientX - box.left), Math.max(0, box.width - breite));
    const y = Math.min(Math.max(0, event.clientY - box.top), Math.max(0, box.height - hoehe));
    menu.style.left = `${Math.round(x)}px`;
    menu.style.top = `${Math.round(y)}px`;
  });
  menu.addEventListener('click', (event) => {
    const eintrag = event.target.closest?.('[data-browser-context-action]');
    if (!eintrag || eintrag.disabled) return;
    schliessen();
    ziele[eintrag.dataset.browserContextAction]?.()?.click();
  });
  // Jeder Weg aus dem Menue heraus schliesst es -- auch der Klick auf die
  // Buehne, der sonst als Eingabe an die ferne Seite ginge.
  refs.canvas.addEventListener('pointerdown', schliessen);
  // Escape muss das Menue schliessen, egal wo der Fokus gerade steht -- beim
  // Oeffnen per Maus liegt er nicht im Menue, und ein Menue, das offen bleibt,
  // verdeckt die Buehne.
  const aufEscape = (event) => {
    if (event.key !== 'Escape' || menu.hidden) return;
    event.stopPropagation();
    schliessen();
    refs.canvas.focus();
  };
  menu.addEventListener('keydown', aufEscape);
  document.addEventListener('keydown', aufEscape, true);
}

// Sitzungsleiste einklappbar: auf schmalen Fenstern und im Freischaltmodus
// braucht die Buehne jede Spalte. Zustand ueberlebt den Reload.
function installRailToggle(refs) {
  const root = refs.railToggle?.closest('[data-browser-root]');
  if (!root || !refs.railToggle) return;
  let gespeichert = '';
  try { gespeichert = window.localStorage.getItem('ctox-browser-rail') || ''; } catch {}
  const anwenden = (zu) => {
    root.classList.toggle('is-rail-collapsed', zu);
    refs.railToggle.setAttribute('aria-pressed', zu ? 'true' : 'false');
    try { window.localStorage.setItem('ctox-browser-rail', zu ? 'zu' : 'offen'); } catch {}
  };
  if (gespeichert === 'zu') anwenden(true);
  refs.railToggle.addEventListener('click', () => {
    anwenden(!root.classList.contains('is-rail-collapsed'));
  });
}

// Praezise Eingabe: Klickpunkt waehlen, lokal tippen/pasten, am Punkt
// ausfuehren. Umgeht die Latenz des fernen Eingabekanals fuer Formulare.
function installPreciseInput(ctx, refs, state) {
  if (!refs.preciseToggle || !refs.preciseLayer || !refs.shell || !refs.canvas) return;
  const root = refs.preciseToggle.closest('[data-browser-root]');
  const setHint = (text) => { if (refs.preciseHint) refs.preciseHint.textContent = text || ''; };
  const layerZu = () => {
    refs.preciseLayer.hidden = true;
    setHint('');
  };
  const modusSetzen = (an) => {
    state.preciseEnabled = an;
    refs.preciseToggle.setAttribute('aria-pressed', an ? 'true' : 'false');
    root?.classList.toggle('is-precise', an);
    if (!an) layerZu();
  };
  refs.preciseToggle.addEventListener('click', () => modusSetzen(!state.preciseEnabled));
  // Capture-Phase auf dem Buehnen-Container: faengt den Klick VOR den
  // normalen Canvas-Handlern ab, solange der Modus aktiv ist.
  refs.shell.addEventListener('pointerdown', (event) => {
    if (!state.preciseEnabled) return;
    if (refs.precisePanel?.contains(event.target)) return;
    if (event.target !== refs.canvas) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    const punkt = canvasPoint(refs.canvas, event);
    state.precisePoint = punkt;
    const shellBox = refs.shell.getBoundingClientRect();
    const lokalX = event.clientX - shellBox.left;
    const lokalY = event.clientY - shellBox.top;
    refs.preciseLayer.hidden = false;
    if (refs.preciseMark) {
      refs.preciseMark.style.left = `${Math.round(lokalX)}px`;
      refs.preciseMark.style.top = `${Math.round(lokalY)}px`;
    }
    if (refs.precisePanel) {
      const panelBreite = 340;
      const links = Math.max(8, Math.min(lokalX + 14, Math.max(8, shellBox.width - panelBreite - 8)));
      const oben = Math.max(8, Math.min(lokalY + 14, Math.max(8, shellBox.height - 170)));
      refs.precisePanel.style.left = `${Math.round(links)}px`;
      refs.precisePanel.style.top = `${Math.round(oben)}px`;
    }
    setHint(`Punkt ${punkt.x}×${punkt.y} gewählt`);
    refs.preciseText?.focus();
  }, true);
  const fernerKlick = async (button, anzahl = 1) => {
    const punkt = state.precisePoint;
    if (!punkt) return;
    for (let i = 0; i < anzahl; i += 1) {
      await writeInputEvent(ctx, state, 'click', preciseClickPatch(ctx, refs, punkt, button));
    }
  };
  refs.precisePanel?.addEventListener('click', async (event) => {
    const knopf = event.target.closest?.('[data-browser-precise-act]');
    if (!knopf) return;
    const akt = knopf.dataset.browserPreciseAct;
    if (akt === 'close') { layerZu(); return; }
    try {
      if (akt === 'click') { await fernerKlick('left'); setHint('Klick gesendet'); }
      if (akt === 'dblclick') { await fernerKlick('left', 2); setHint('Doppelklick gesendet'); }
      if (akt === 'rightclick') { await fernerKlick('right'); setHint('Rechtsklick gesendet'); }
      if (akt === 'type' || akt === 'type-enter') {
        const text = String(refs.preciseText?.value || '');
        await fernerKlick('left');
        await new Promise((r) => setTimeout(r, 160));
        if (text) await writeInputEvent(ctx, state, 'keyDown', preciseTextPatch(ctx, text));
        if (akt === 'type-enter') {
          await new Promise((r) => setTimeout(r, 120));
          await writeInputEvent(ctx, state, 'keyDown', preciseKeyPatch(ctx, 'Enter'));
        }
        setHint(text ? 'Text am Punkt eingefügt' : 'Klick gesendet (kein Text eingegeben)');
      }
    } catch (error) {
      setHint(`Eingabe fehlgeschlagen: ${error?.message || error}`);
    }
  });
  refs.preciseText?.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') { layerZu(); refs.canvas?.focus(); }
    event.stopPropagation();
  });
}

function preciseClickPatch(ctx, refs, punkt, button) {
  return {
    x: punkt.x, y: punkt.y, detail: 1, clickCount: 1,
    button, buttons: button === 'right' ? 2 : 1, dx: 0, dy: 0,
    key: '', code: '', modifiers: [], text: '',
    payload: {
      pointer_id: 0, pointer_type: 'precise', click_count: 1,
      viewport_w: refs.canvas?.width || VIEWPORT.width,
      viewport_h: refs.canvas?.height || VIEWPORT.height,
      actor: actorContext(ctx.session),
    },
  };
}

function preciseTextPatch(ctx, text) {
  return {
    x: 0, y: 0, detail: 0, clickCount: 0, button: 'none', buttons: 0,
    dx: 0, dy: 0, key: '', code: '', modifiers: [], text,
    payload: { repeat: false, location: 0, actor: actorContext(ctx.session) },
  };
}

function preciseKeyPatch(ctx, key) {
  return {
    x: 0, y: 0, detail: 0, clickCount: 0, button: 'none', buttons: 0,
    dx: 0, dy: 0, key, code: key, modifiers: [], text: '',
    payload: { repeat: false, location: 0, actor: actorContext(ctx.session) },
  };
}

// Einfuegen aus der Zwischenablage direkt auf der Buehne: das paste-Ereignis
// traegt den Text lokal; er geht als EIN keyboard.type an die ferne Seite.
function installCanvasPaste(ctx, refs, state) {
  refs.canvas?.addEventListener('paste', (event) => {
    const text = event.clipboardData?.getData('text/plain') || '';
    if (!text) return;
    event.preventDefault();
    writeInputEvent(ctx, state, 'keyDown', preciseTextPatch(ctx, text));
  });
}

function installInputHandlers(ctx, refs, state, scheduleRefresh) {
  const afterInput = () => {
    if (!state.directLiveEnabled) scheduleRefresh();
  };
  refs.canvas?.addEventListener('pointerdown', (event) => {
    event.preventDefault();
    refs.canvas.focus();
    refs.canvas.setPointerCapture?.(event.pointerId);
    state.pointerIsDown = true;
    writePointerInput(ctx, refs, state, 'mouseDown', event).then(afterInput);
  });
  refs.canvas?.addEventListener('pointerup', (event) => {
    event.preventDefault();
    state.pointerIsDown = false;
    writePointerInput(ctx, refs, state, 'mouseUp', event).then(afterInput);
  });
  refs.canvas?.addEventListener('pointercancel', (event) => {
    state.pointerIsDown = false;
    writePointerInput(ctx, refs, state, 'mouseUp', event).then(afterInput);
  });
  refs.canvas?.addEventListener('pointermove', (event) => {
    const now = Date.now();
    const dragging = state.pointerIsDown || Number(event.buttons || 0) > 0;
    if (!dragging && now - Number(state.lastPointerMoveAt || 0) < POINTER_HOVER_THROTTLE_MS) return;
    state.lastPointerMoveAt = now;
    writePointerInput(ctx, refs, state, 'mouseMove', event).then(afterInput);
  });
  refs.canvas?.addEventListener('wheel', (event) => {
    event.preventDefault();
    writePointerInput(ctx, refs, state, 'wheel', event).then(afterInput);
  }, { passive: false });
  refs.canvas?.addEventListener('keydown', (event) => {
    // Cmd/Ctrl+V muss durch: preventDefault hier wuergte das paste-Ereignis
    // ab, bevor es entstand — deshalb kam ⌘V nie an. installCanvasPaste
    // schickt den Zwischenablage-Text dann als Ganzes.
    if ((event.metaKey || event.ctrlKey) && String(event.key).toLowerCase() === 'v') return;
    event.preventDefault();
    writeKeyboardInput(ctx, state, 'keyDown', event).then(afterInput);
  });
  refs.canvas?.addEventListener('keyup', (event) => {
    event.preventDefault();
    writeKeyboardInput(ctx, state, 'keyUp', event).then(afterInput);
  });
  installContextMenu(refs);
}

async function writePointerInput(ctx, refs, state, type, event, extra = {}) {
  const point = canvasPoint(refs.canvas, event);
  const clickCount = type === 'wheel' ? 0 : pointerClickCount(state, type, event, point);
  const buttons = pointerButtons(event, type);
  const patch = {
    x: point.x,
    y: point.y,
    detail: clickCount,
    clickCount,
    button: pointerButton(event.button),
    buttons,
    dx: type === 'wheel' ? Number(event.deltaX || 0) : 0,
    dy: type === 'wheel' ? Number(event.deltaY || 0) : 0,
    key: '',
    code: '',
    modifiers: eventModifiers(event),
    text: '',
    payload: {
      pointer_id: event.pointerId || 0,
      pointer_type: event.pointerType || 'mouse',
      click_count: clickCount,
      viewport_w: refs.canvas?.width || VIEWPORT.width,
      viewport_h: refs.canvas?.height || VIEWPORT.height,
      actor: actorContext(ctx.session),
    },
    ...extra,
  };
  await writeInputEvent(ctx, state, type, patch);
}

async function writeKeyboardInput(ctx, state, type, event) {
  const key = event.key || '';
  const code = event.code || '';
  const modifiers = eventModifiers(event);
  await writeInputEvent(ctx, state, type, {
    x: 0,
    y: 0,
    detail: 0,
    clickCount: 0,
    button: 'none',
    buttons: 0,
    dx: 0,
    dy: 0,
    key,
    code,
    modifiers,
    text: keyboardText(type, event),
    payload: {
      repeat: Boolean(event.repeat),
      location: Number(event.location || 0),
      actor: actorContext(ctx.session),
    },
  });
}

async function writeInputEvent(ctx, state, type, patch) {
  // Beide Abbruchgruende waren bis 13.08.2026 STUMM: kein Protokoll, keine
  // Meldung, kein Hinweis in der Oberflaeche. Auf der Kundeninstanz gemessen:
  // null Eingabe-Ereignisse in zehn Minuten aktiver Bedienung, waehrend beide
  // Sitzungen "active" meldeten und die Oberflaeche "Bereit" zeigte. Der
  // Nutzer sah einen laufenden Browser, den niemand bedienen durfte — und
  // nichts sagte ihm warum. Ein Abbruch ohne Grund ist von einem Absturz
  // nicht zu unterscheiden.
  if (!browserSurfaceCanControl(ctx, state)) {
    meldeEingabeGesperrt(ctx, state, type);
    // Ein Klick IN dieses Fenster ist die ausdrueckliche Ansage, hier steuern
    // zu wollen. Haelt ein totes Fenster desselben Nutzers noch die Pacht,
    // wird sie jetzt uebernommen — sonst bliebe die Sitzung unbedienbar, bis
    // die Pacht von allein ablaeuft.
    if (type === 'mouseDown' || type === 'keyDown') {
      try { state.steuerungZurueckholen?.({ uebernahmeDurchInteraktion: true }); } catch {}
    }
    return;
  }
  const session = state.latestSession;
  const frame = state.latestFrame;
  const sessionId = session?.id || frame?.session_id;
  if (!sessionId) {
    // Faellt der Bildstrom aus, gibt es kein latestFrame. Ohne latestSession
    // bricht es dann hier ab — der Bildstromdefekt blockiert die Eingabe mit.
    console.warn('[browser] Eingabe verworfen: keine Sitzung zuordenbar', { type });
    setzeEingabeHinweis(ctx, state, 'Keine Sitzung zugeordnet. Bitte die Sitzung neu starten.');
    return;
  }
  const now = Date.now();
  const seq = Math.max(now, Number(state.lastInputSeq || 0) + 1);
  state.lastInputSeq = seq;
  const event = {
    id: `${sessionId}:input:${seq}:${type}`,
    tenant_id: browserTenantId(ctx),
    owner_user_id: session?.owner_user_id || '',
    controller_user_id: session?.controller_user_id || browserActorIds(ctx.session)[0] || '',
    session_id: sessionId,
    tab_id: state.latestTab?.id || frame?.tab_id || '',
    seq,
    client_seq: seq,
    frame_seq: Number(frame?.seq || session?.last_frame_seq || 0),
    lease_id: state.controllerLeaseId || '',
    ack_status: 'pending',
    type,
    status: 'pending',
    created_at_ms: now,
    updated_at_ms: now,
    ...patch,
  };
  console.info('[browser] input sent', browserInputTrace(event));
  if (state.directLiveEnabled && typeof ctx.sync?.requestNative === 'function') {
    if (event.type === 'mouseMove' && state.directInputQueue.length > 0) {
      const last = state.directInputQueue[state.directInputQueue.length - 1];
      if (last?.type === 'mouseMove' && !state.pointerIsDown) state.directInputQueue.pop();
    }
    state.directInputQueue.push(event);
    if (state.directInputStatusElement) {
      state.directInputStatusElement.dataset.browserInputSentSeq = String(seq);
    }
    if (state.directInputQueue.length > 128) {
      state.directInputQueue.splice(0, state.directInputQueue.length - 128);
    }
    state.scheduleDirectInputFlush?.();
    return;
  }
  await upsertDoc(browserCollection(ctx, 'browser_input_events'), event);
}

function startDirectBrowserLive(ctx, refs, state, isMounted, scheduleRefresh) {
  let stopped = false;
  let timer = null;
  let inputTimer = null;
  let inputFlushInFlight = false;
  const schedule = (delay) => {
    if (stopped) return;
    timer = globalThis.setTimeout(pump, delay);
  };
  const scheduleInput = (delay = 0) => {
    if (stopped || inputFlushInFlight || inputTimer) return;
    inputTimer = globalThis.setTimeout(() => {
      inputTimer = null;
      flushInput();
    }, delay);
  };
  const flushInput = async () => {
    if (stopped || inputFlushInFlight || !isMounted()) return;
    const directSessionId = state.latestSession?.id || state.requestedSessionId || '';
    if (!state.directLiveEnabled
      || !directSessionId
      || !state.controllerLeaseId
      || globalThis.document?.visibilityState === 'hidden'
      || !browserSurfaceIsFocused(ctx)) {
      if (state.directInputQueue.length) scheduleInput(150);
      return;
    }
    const pending = state.directInputQueue
      .filter((event) => event?.session_id === directSessionId)
      .slice(0, 64);
    if (!pending.length) return;
    inputFlushInFlight = true;
    try {
      const response = await ctx.sync.requestNative('ctox.browser.live.v1', {
        op: 'input',
        session_id: directSessionId,
        lease_id: state.controllerLeaseId,
        events: pending,
      }, {
        collection: 'business_commands',
        requiredCapability: 'ctox-browser-live-v1',
        timeoutMs: 3_000,
      });
      const sentIds = new Set(pending.map((event) => event.id));
      for (let index = state.directInputQueue.length - 1; index >= 0; index -= 1) {
        if (sentIds.has(state.directInputQueue[index]?.id)) state.directInputQueue.splice(index, 1);
      }
      state.directInputFailures = 0;
      const acknowledgedSeq = Math.max(...pending.map((event) => Number(event.seq || 0)));
      const acknowledgedAt = Date.now();
      if (state.directInputStatusElement) {
        state.directInputStatusElement.dataset.browserInputAckSeq = String(acknowledgedSeq);
        state.directInputStatusElement.dataset.browserInputAckMs = String(Math.max(
          0,
          acknowledgedAt - Math.max(...pending.map((event) => Number(event.created_at_ms || acknowledgedAt))),
        ));
        state.directInputStatusElement.dataset.browserRemoteFocus = String(
          response?.input?.tag || '',
        );
        state.directInputStatusElement.dataset.browserRemoteEditable = String(
          response?.input?.editable === true,
        );
        state.directInputStatusElement.dataset.browserRemoteValueLength = Number.isFinite(
          response?.input?.valueLength,
        ) ? String(response.input.valueLength) : '';
      }
      console.info('[browser] direct input applied', {
        session_id: directSessionId,
        events: pending.length,
        applied: Number(response?.applied || 0),
        acknowledged_seq: acknowledgedSeq,
        remote_input: response?.input || null,
      });
      if (state.notice === 'Browser-Eingabe wird wieder verbunden …') {
        state.notice = '';
        renderNotice(refs, '');
      }
      const failed = Array.isArray(response?.results)
        ? response.results.filter((result) => result?.ok === false)
        : [];
      if (failed.length) {
        state.notice = failed[0]?.error || 'Eine Browser-Eingabe ist fehlgeschlagen.';
        renderNotice(refs, state.notice);
      }
    } catch (error) {
      state.directInputFailures += 1;
      if (state.directInputFailures === 2 || state.directInputFailures % 10 === 0) {
        console.warn('[browser] direct WebRTC input path reconnecting', error);
        state.notice = 'Browser-Eingabe wird wieder verbunden …';
        renderNotice(refs, state.notice);
      }
    } finally {
      inputFlushInFlight = false;
      if (state.directInputQueue.length) scheduleInput(state.directInputFailures ? 150 : 0);
    }
  };
  state.directInputStatusElement = refs.canvas;
  state.scheduleDirectInputFlush = scheduleInput;
  const pump = async () => {
    if (stopped || !isMounted()) return;
    const directSessionId = state.latestSession?.id || state.requestedSessionId || '';
    if (!state.directLiveEnabled
      || !directSessionId
      || !state.controllerLeaseId
      || globalThis.document?.visibilityState === 'hidden'
      || !browserSurfaceIsFocused(ctx)) {
      schedule(150);
      return;
    }
    const directNavigationEpoch = Number(state.directNavigationEpoch || 0);
    try {
      const response = await ctx.sync.requestNative('ctox.browser.live.v1', {
        session_id: directSessionId,
        lease_id: state.controllerLeaseId,
        events: [],
        frame_after_ms: Number(state.latestDirectFrame?.captured_at_ms || 0),
      }, {
        collection: 'business_commands',
        requiredCapability: 'ctox-browser-live-v1',
        timeoutMs: 5_000,
      });
      if (!directResponseBelongsToSurface(state, directSessionId, directNavigationEpoch)) {
        schedule(0);
        return;
      }
      // Eine Antwort mit ok:false ist KEIN Nicht-Ereignis. Am 31.08.2026 lief
      // jeder Bildabruf in "unknown op live" (Runner aus einem workjet-Pin
      // ohne die live-Operation), und diese Schleife hat die Fehlerantwort
      // wortlos verschluckt: kein Zaehler, keine Meldung, die Buehne blieb
      // fuer immer auf "Browser-Inhalt wird geladen". Der Fehlertext gehoert
      // dorthin, wo der Nutzer hinsieht — auf die Buehne und in die Statuszeile.
      if (response && response.ok === false) {
        state.directLiveFailures += 1;
        const grund = String(response.error || 'Der Browser-Dienst hat den Bildabruf abgelehnt.');
        if (state.lastLiveRejection !== grund) {
          console.warn('[browser] direct live request rejected', { session_id: directSessionId, error: grund });
        }
        state.lastLiveRejection = grund;
        state.notice = grund;
        renderNotice(refs, state.notice);
        if (refs.empty && state.ansicht !== 'script' && !state.latestDirectFrame) {
          refs.empty.hidden = false;
          refs.empty.textContent = grund;
        }
        schedule(1_000);
        return;
      }
      const recoveredDirectLive = state.directLiveFailures > 0;
      state.directLiveFailures = 0;
      if (state.lastLiveRejection) {
        if (state.notice === state.lastLiveRejection) {
          state.notice = '';
          renderNotice(refs, '');
        }
        state.lastLiveRejection = '';
      }
      if (recoveredDirectLive
        && state.notice === 'Direkter Browser-Datenkanal wird wieder verbunden …') {
        state.notice = '';
        renderNotice(refs, '');
      }
      const failed = Array.isArray(response?.results)
        ? response.results.filter((result) => result?.ok === false)
        : [];
      if (failed.length) {
        state.notice = failed[0]?.error || 'Eine Browser-Eingabe ist fehlgeschlagen.';
        renderNotice(refs, state.notice);
      }
      const screenshot = response?.screenshot;
      if (screenshot?.base64) {
        state.directFrameSeq += 1;
        const now = Date.now();
        const capturedAtMs = Number(screenshot.capturedAtMs || now);
        if (!state.latestSession) {
          state.latestSession = {
            id: directSessionId,
            owner_user_id: browserActorIds(ctx.session)[0] || '',
            controller_user_id: browserActorIds(ctx.session)[0] || '',
            controller_lease_id: state.controllerLeaseId,
            controller_lease_expires_at_ms: now + 120_000,
            status: 'active',
            runtime_status: 'active',
            viewport_w: VIEWPORT.width,
            viewport_h: VIEWPORT.height,
            current_tab_id: state.latestTab?.id || '',
          };
        }
        state.latestDirectFrame = {
          id: `browser_live_${directSessionId}_${state.directFrameSeq}`,
          session_id: directSessionId,
          tab_id: state.latestTab?.id || state.latestSession.current_tab_id || '',
          seq: state.directFrameSeq,
          mime_type: screenshot.mimeType || 'image/jpeg',
          data: screenshot.base64,
          width: Number(state.latestSession.viewport_w || VIEWPORT.width),
          height: Number(state.latestSession.viewport_h || VIEWPORT.height),
          captured_at_ms: capturedAtMs,
          updated_at_ms: now,
          expires_at_ms: now + 10_000,
        };
        state.latestFrame = state.latestDirectFrame;
        if (response.nav) {
          applyDirectNavigationState(state, response.nav);
        }
        renderStatus(refs, state.latestSession, state.latestTab, state.latestFrame, state.latestCommand);
        await renderFrame(refs, state.latestFrame, state);
      }
    } catch (error) {
      const waitingForNewSession = Boolean(
        state.requestedSessionId
        && !state.latestSession
        && Number(state.startPendingSince || 0) > 0
        && Date.now() - Number(state.startPendingSince || 0) < 150_000,
      );
      if (waitingForNewSession) {
        state.directLiveFailures = 0;
        schedule(500);
        return;
      }
      state.directLiveFailures += 1;
      if (state.directLiveFailures === 3 || state.directLiveFailures % 20 === 0) {
        // Frames and input are intentionally never persisted as an automatic
        // fallback. That path wrote a fresh screenshot into SQLite/IndexedDB
        // on every tick and could consume the whole application. Keep the
        // bounded input queue and let the shared WebRTC peer reconnect.
        console.warn('[browser] direct WebRTC live path reconnecting', error);
        state.notice = 'Direkter Browser-Datenkanal wird wieder verbunden …';
        renderNotice(refs, state.notice);
      }
    }
    schedule(state.directLiveFailures ? 500 : 100);
  };
  schedule(0);
  return () => {
    stopped = true;
    if (timer) globalThis.clearTimeout(timer);
    if (inputTimer) globalThis.clearTimeout(inputTimer);
    state.scheduleDirectInputFlush = null;
    state.directInputStatusElement = null;
  };
}

function browserInputTrace(event) {
  const clickCount = Number(event.clickCount || event.detail || event.payload?.click_count || 0);
  return {
    session_id: event.session_id || '',
    tab_id: event.tab_id || '',
    seq: Number(event.seq || 0),
    type: event.type || '',
    x: Number(event.x || 0),
    y: Number(event.y || 0),
    detail: clickCount,
    clickCount,
    button: event.button || '',
    buttons: Number(event.buttons || 0),
    dx: Number(event.dx || 0),
    dy: Number(event.dy || 0),
    modifiers: Array.isArray(event.modifiers) ? event.modifiers : [],
    key: event.key || '',
    code: event.code || '',
    text: event.text || '',
  };
}

function installAddressClearControl(form, address, go) {
  if (!form || !address) return null;
  const existing = form.querySelector?.('[data-browser-address-clear]');
  if (existing) return existing;
  const button = globalThis.document?.createElement?.('button');
  if (!button) return null;
  button.type = 'button';
  button.className = 'ctox-icon-button';
  button.dataset.browserAddressClear = '';
  button.setAttribute('aria-label', 'Adresse löschen');
  button.title = 'Adresse löschen';
  button.textContent = '×';
  form.insertBefore(button, go || null);
  return button;
}

function browserTenantId(ctx) {
  return String(
    ctx?.sync?.config?.instance_id
      || ctx?.sync?.config?.instanceId
      || ctx?.config?.instance_id
      || ctx?.config?.instanceId
      || '',
  ).trim();
}

function canvasPoint(canvas, event) {
  const rect = canvas.getBoundingClientRect();
  const sourceWidth = Math.max(1, Number(canvas.width || VIEWPORT.width));
  const sourceHeight = Math.max(1, Number(canvas.height || VIEWPORT.height));
  const fitScale = Math.min(
    Math.max(1, rect.width) / sourceWidth,
    Math.max(1, rect.height) / sourceHeight,
  );
  const renderedWidth = sourceWidth * fitScale;
  const renderedHeight = sourceHeight * fitScale;
  const contentLeft = rect.left + (rect.width - renderedWidth) / 2;
  const contentTop = rect.top + (rect.height - renderedHeight) / 2;
  return {
    x: Math.max(0, Math.min(sourceWidth, Math.round((event.clientX - contentLeft) / fitScale))),
    y: Math.max(0, Math.min(sourceHeight, Math.round((event.clientY - contentTop) / fitScale))),
  };
}

function directResponseBelongsToSurface(state, sessionId, navigationEpoch) {
  const activeSessionId = state.latestSession?.id || state.requestedSessionId || '';
  return activeSessionId === sessionId
    && Number(state.directNavigationEpoch || 0) === Number(navigationEpoch || 0);
}

function applyDirectNavigationState(state, nav) {
  const currentUrl = nav?.url || state.latestSession?.current_url || '';
  // An empty document title is meaningful. Retaining the previous title makes
  // a successfully navigated page look stale (for example httpbingo displayed
  // "Example Domain"). Use the new host as the honest fallback instead.
  const title = String(nav?.title || '').trim() || browserUrlLabel(currentUrl);
  state.latestTab = { ...(state.latestTab || {}), ...nav, title };
  // Der Runner liefert seine Tab-Liste in JEDER Antwort mit (navState()).
  // Sie ist der verlaesslichere Weg als die replizierte Sammlung
  // `browser_tabs`: die stand auf dieser Instanz auf 0, waehrend der Runner
  // laengst zwei Tabs fuehrte. Beide Quellen bleiben erhalten, die Live-
  // Antwort hat Vorrang.
  if (Array.isArray(nav?.tabs)) {
    state.liveTabs = nav.tabs;
    state.liveActiveTabId = String(nav.active_tab_id || '');
  }
  state.latestSession = {
    ...(state.latestSession || {}),
    current_url: currentUrl,
    title,
    updated_at_ms: Date.now(),
  };
}

function eventModifiers(event) {
  return [
    event.altKey ? 'Alt' : '',
    event.ctrlKey ? 'Control' : '',
    event.metaKey ? 'Meta' : '',
    event.shiftKey ? 'Shift' : '',
  ].filter(Boolean);
}

function pointerButton(button) {
  if (button === 1) return 'middle';
  if (button === 2) return 'right';
  if (button === -1 || button == null) return 'none';
  return 'left';
}

function pointerButtons(event, type) {
  const reported = Number(event.buttons || 0);
  if (type === 'mouseDown' && reported === 0) {
    if (event.button === 1) return 4;
    if (event.button === 2) return 2;
    return 1;
  }
  if (type === 'mouseUp') return reported;
  return reported;
}

function pointerClickCount(state, type, event, point) {
  const fromDom = Number(event.detail || 0);
  if (fromDom > 0) {
    if (type === 'mouseDown' || type === 'mouseUp') {
      state.lastPointerClick = {
        count: fromDom,
        x: point.x,
        y: point.y,
        at: Date.now(),
        button: Number(event.button || 0),
      };
    }
    return fromDom;
  }
  if (type === 'mouseMove') return Number(state.lastPointerClick?.count || 0);
  if (type !== 'mouseDown' && type !== 'mouseUp') return 0;

  const now = Date.now();
  const previous = state.lastPointerClick;
  const sameButton = previous && Number(previous.button || 0) === Number(event.button || 0);
  const closeEnough = previous
    && Math.abs(previous.x - point.x) <= POINTER_CLICK_SLOP_PX
    && Math.abs(previous.y - point.y) <= POINTER_CLICK_SLOP_PX
    && now - Number(previous.at || 0) <= POINTER_CLICK_INTERVAL_MS;
  const nextCount = type === 'mouseDown'
    ? (sameButton && closeEnough ? Number(previous.count || 0) + 1 : 1)
    : (sameButton && closeEnough ? Number(previous.count || 1) : 1);
  state.lastPointerClick = {
    count: nextCount,
    x: point.x,
    y: point.y,
    at: now,
    button: Number(event.button || 0),
  };
  return nextCount;
}

function keyboardText(type, event) {
  if (type !== 'keyDown') return '';
  const key = event.key || '';
  if (!key || key.length !== 1) return '';
  // Option (macOS) und AltGr (Windows, meldet ctrl+alt) KOMPONIEREN Zeichen:
  // @ ist auf deutschem Mac-Layout Option+L. Solche Tasten muessen als TEXT
  // zur fernen Seite, nicht als Kombination — "Alt+@" drueckt dort nichts.
  const altGr = event.ctrlKey && event.altKey;
  if ((event.ctrlKey || event.metaKey) && !altGr) return '';
  return key;
}

function browserInputPayload(event) {
  return {
    type: event.type || '',
    x: Number(event.x || 0),
    y: Number(event.y || 0),
    detail: Number(event.detail || event.clickCount || 0),
    clickCount: Number(event.clickCount || event.detail || 0),
    button: event.button || 'left',
    buttons: Number(event.buttons || 0),
    dx: Number(event.dx || 0),
    dy: Number(event.dy || 0),
    key: event.key || '',
    code: event.code || '',
    modifiers: Array.isArray(event.modifiers) ? event.modifiers : [],
    text: event.text || '',
  };
}

async function readCollection(collection, options = {}) {
  if (!collection?.find) return [];
  const limit = Number.isFinite(options.limit) ? options.limit : 100;
  const selector = options.selector || {};
  const sort = options.sort || [{ updated_at_ms: 'desc' }];
  const docs = await collection.find({ selector, sort, limit }).exec();
  return docs
    .map((doc) => doc?.toJSON?.() || doc)
    .filter((doc) => doc && doc._deleted !== true);
}

async function readDocument(collection, id) {
  if (!collection?.findOne || !id) return null;
  const doc = await collection.findOne(id).exec();
  const data = doc?.toJSON?.() || doc;
  return data && data._deleted !== true ? data : null;
}

function mergeRequestedSession(sessions, requestedSession) {
  return mergeRequestedDocument(sessions, requestedSession);
}

function mergeRequestedDocument(documents, requestedDocument) {
  if (!requestedDocument?.id) return documents;
  return [
    requestedDocument,
    ...documents.filter((document) => document.id !== requestedDocument.id),
  ];
}

function browserCollection(ctx, name) {
  return ctx?.db?.collection?.(name) || null;
}

function latestFrame(frames, sessionId = '') {
  return frames
    .filter((frame) => frame.data && (!sessionId || frame.session_id === sessionId) && Number(frame.expires_at_ms || 0) > Date.now())
    .sort((a, b) => Number(b.seq || 0) - Number(a.seq || 0) || Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0))[0] || null;
}

function latestSession(sessions, sessionId) {
  const candidates = sessionId
    ? sessions.filter((session) => session.id === sessionId)
    : sessions;
  return candidates.sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0))[0] || null;
}

function latestTab(tabs, tabId) {
  const candidates = tabId
    ? tabs.filter((tab) => tab.id === tabId)
    : tabs;
  return candidates.sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0))[0] || null;
}

function latestBrowserCommand(commands, sessionId) {
  return latestBrowserCommands(commands, sessionId, 1)[0] || null;
}

function applyLatestNavigationResult(state, commands) {
  const sessionId = state.latestSession?.id || state.latestFrame?.session_id;
  if (!sessionId) return;
  const command = commands
    .filter((candidate) => {
      const type = candidate.command_type || candidate.type || '';
      const candidateSessionId = candidate.payload?.session_id || candidate.record_id || '';
      return candidateSessionId === sessionId
        && ['browser.session.start', 'browser.navigate', 'browser.reload', 'browser.back', 'browser.forward', 'browser.reset'].includes(type)
        && candidate.status === 'completed'
        && candidate.result?.url;
    })
    .sort((a, b) => Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0))[0];
  if (!command) return;
  const url = String(command.result.url || '');
  const title = String(command.result.title || state.latestTab?.title || state.latestSession?.title || 'Browser');
  state.latestSession = { ...state.latestSession, current_url: url, title };
  state.latestTab = { ...(state.latestTab || {}), url, title };
}

function latestBrowserCommands(commands, sessionId, limit = 5) {
  return commands
    .filter((command) => {
      const type = command.command_type || command.type || '';
      if (!type.startsWith('browser.')) return false;
      if (!sessionId) return true;
      const payloadSession = command.payload?.session_id;
      return command.record_id === sessionId || payloadSession === sessionId;
    })
    .sort((a, b) => Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0))
    .slice(0, limit);
}

function latestBrowserHandoffTasks(tasks, sessionId, limit = 5) {
  return tasks
    .filter((task) => {
      if (task.command_type !== 'ctox.browser_context.capture') return false;
      if (task.inbound_channel !== 'browser') return false;
      if (!sessionId) return true;
      return String(task.prompt || '').includes(sessionId) || String(task.command_id || '').includes(sessionId);
    })
    .sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0))
    .slice(0, limit);
}

function renderSessionList(refs, sessions, tabs, activeSession) {
  if (!refs.sessionList) return;
  const sorted = [...sessions].sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0));
  if (!sorted.length) {
    refs.sessionList.innerHTML = '';
    return;
  }
  const activeTabs = tabs
    .filter((tab) => tab.session_id === activeSession?.id && tab.status !== 'closed')
    .sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0));
  const tabMarkup = activeTabs.map((tab) => `
    <span class="ctox-pane-tab browser-tab" data-browser-tab-id="${escapeHtml(tab.id)}" aria-selected="${tab.id === activeSession?.current_tab_id ? 'true' : 'false'}">
      <span class="browser-tab-title">${escapeHtml(tab.title || tab.url || 'Tab')}</span>
      <button type="button" class="ctox-icon-button ctox-icon-button--sm" data-browser-tab-close aria-label="Tab schließen">×</button>
    </span>
  `).join('');
  refs.sessionList.innerHTML = tabMarkup;
}

// ----- LEFT sessions selector (canonical grammar column) -----

// Owned sessions plus the read-only import overlay (imported entries that are
// not already a real owned session). Imported sessions are marked and never
// persisted — browser sessions are a read-only projection.
// custom: Scraping-Adapter im Browser-Modul sichtbar machen.
//
// Die Adapter der Recherche steuern Browser-Laeufe. Core-Outbound und lokale
// Tenant-Apps besitzen getrennte Sammlungen; der Browser zeigt beide in einer
// Betriebssicht. Die Sammlungen sind bedarfsgeladen, deshalb Lease statt
// startCollection (Muster wie im App-Store-Modul).
async function ladeScrapingAdapter(ctx, state) {
  if (state.adapterLadedauer) return state.adapterLadedauer;
  state.adapterLadedauer = (async () => {
    const rows = [];
    const errors = [];
    state.adapterLeases ||= [];
    for (const collectionName of SCRAPING_ADAPTER_COLLECTIONS) {
      try {
        if (typeof ctx.sync?.leaseCollection === 'function') {
          const lease = await ctx.sync.leaseCollection(collectionName, `browser:scraping-adapters:${collectionName}`);
          if (lease) state.adapterLeases.push(lease);
        }
        const collectionRows = await readCollection(browserCollection(ctx, collectionName), {
          limit: 200,
          sort: [{ updated_at_ms: 'desc' }],
        });
        rows.push(...collectionRows.map((row) => ({ ...row, adapter_collection: collectionName })));
      } catch (error) {
        errors.push(`${collectionName}: ${String(error?.message || error)}`);
      }
    }
    const deduplicated = new Map();
    for (const row of rows) {
      if (!row || row.is_deleted === true) continue;
      deduplicated.set(`${row.adapter_collection}:${row.id || row.source_id}`, row);
    }
    state.adapters = [...deduplicated.values()];
    state.adapterFehler = errors.length === SCRAPING_ADAPTER_COLLECTIONS.length
      ? errors.join(' | ')
      : '';
    state.adapterLadedauer = null;
  })();
  return state.adapterLadedauer;
}

function adapterZustand(adapter) {
  const auth = String(adapter.auth_status || '').toLowerCase();
  const status = String(adapter.status || adapter.last_test?.status || '').toLowerCase();
  if (adapter.enabled === false) return { klasse: 'is-off', text: t('adapterOff', 'Deaktiviert') };
  if (auth.includes('missing') || auth.includes('required') || status.includes('auth')) {
    return { klasse: 'is-error', text: t('adapterAuthMissing', 'Zugang fehlt') };
  }
  if (status.includes('unreachable') || status.includes('fail') || adapter.last_error) {
    return { klasse: 'is-warn', text: t('adapterUnreachable', 'Letzte Prüfung fehlgeschlagen') };
  }
  return { klasse: 'is-ok', text: t('adapterReady', 'Bereit') };
}

// Eine Schiene, zwei Inhalte: Sitzungen oder Scraping-Adapter, je nach Band.
function renderLeftRail(ctx, refs, state) {
  if (state.leftView?.band === 'adapters') {
    renderAdapterRail(ctx, refs, state);
    return;
  }
  renderSessions(refs, sessionRenderList(state), state.latestSession, state.leftView, sessionTabCounts(state.tabs), ctx);
}

function renderAdapterRail(ctx, refs, state) {
  const rail = refs.sessions;
  if (!rail) return;
  if (refs.adapterCount) refs.adapterCount.textContent = String((state.adapters || []).length || '');
  if (!Array.isArray(state.adapters)) {
    rail.replaceChildren();
    const laden = document.createElement('div');
    laden.className = 'ctox-empty';
    laden.textContent = t('adaptersLoading', 'Adapter werden geladen …');
    rail.appendChild(laden);
    ladeScrapingAdapter(ctx, state).then(() => {
      if (state.leftView?.band === 'adapters') renderAdapterRail(ctx, refs, state);
    });
    return;
  }
  rail.replaceChildren();
  if (state.adapterFehler) {
    const fehler = document.createElement('div');
    fehler.className = 'ctox-empty';
    fehler.textContent = t('adaptersFailed', 'Adapter konnten nicht geladen werden: ') + state.adapterFehler;
    rail.appendChild(fehler);
    return;
  }
  if (!state.adapters.length) {
    const leer = document.createElement('div');
    leer.className = 'ctox-empty';
    leer.textContent = t('adaptersEmpty', 'Noch keine Scraping-Adapter. Sie entstehen in der Outbound-App unter „Quellen & Zugänge“.');
    rail.appendChild(leer);
    return;
  }
  const suche = String(state.leftView?.search || '');
  for (const adapter of state.adapters) {
    const label = String(adapter.label || adapter.source_id || adapter.id || '');
    if (suche && !label.toLowerCase().includes(suche)) continue;
    const zustand = adapterZustand(adapter);
    const karte = document.createElement('div');
    karte.className = 'browser-adapter-card';
    const kopf = document.createElement('div');
    kopf.className = 'browser-adapter-head';
    const punkt = document.createElement('span');
    punkt.className = `browser-adapter-dot ${zustand.klasse}`;
    const name = document.createElement('strong');
    name.textContent = label;
    kopf.append(punkt, name);
    const meta = document.createElement('div');
    meta.className = 'browser-adapter-meta';
    const getestet = Number(adapter.last_test?.at_ms || adapter.updated_at_ms || 0);
    const latenz = Number(adapter.latency_ms || adapter.last_test?.latency_ms || 0);
    meta.textContent = [
      String(adapter.source_id || ''),
      zustand.text,
      getestet ? new Date(getestet).toLocaleString() : '',
      latenz ? `${latenz} ms` : '',
    ].filter(Boolean).join(' · ');
    karte.append(kopf, meta);
    if (adapter.last_error) {
      const fehlerzeile = document.createElement('div');
      fehlerzeile.className = 'browser-adapter-error';
      fehlerzeile.textContent = String(adapter.last_error).slice(0, 160);
      karte.appendChild(fehlerzeile);
    }
    rail.appendChild(karte);
  }
  const hinweis = document.createElement('div');
  hinweis.className = 'browser-adapter-hint';
  hinweis.textContent = t('adaptersManagedHint', 'Verwaltet in der Outbound-App („Quellen & Zugänge“).');
  rail.appendChild(hinweis);
}

function sessionRenderList(state) {
  const owned = Array.isArray(state?.visibleSessions) ? state.visibleSessions : [];
  const ownedIds = new Set(owned.map((session) => session.id));
  const imported = (Array.isArray(state?.importedSessions) ? state.importedSessions : [])
    .filter((session) => session?.id && !ownedIds.has(session.id));
  return [...owned, ...imported];
}

function sessionTabCounts(tabs) {
  const counts = {};
  for (const tab of Array.isArray(tabs) ? tabs : []) {
    if (tab.status === 'closed') continue;
    counts[tab.session_id] = Number(counts[tab.session_id] || 0) + 1;
  }
  return counts;
}

// Rebuild the sessions well ONLY when the rendered data changed (signature
// guard). Selection is applied in place via markActiveSession so a re-render
// never clamps the well scroll or moves the operator.
function renderSessions(refs, sessions, activeSession, view = {}, tabCounts = {}, ctx = null) {
  if (!refs.sessions) return;
  const all = Array.isArray(sessions) ? sessions : [];
  const filtered = filterSessionsForView(all, view);
  const listView = view?.view === 'list' ? 'list' : 'cards';
  refs.sessions.classList.toggle('is-list', listView === 'list');

  const signature = sessionListSignature(filtered, listView, tabCounts);
  if (refs.sessions.dataset.sig !== signature) {
    refs.sessions.dataset.sig = signature;
    refs.sessions.innerHTML = filtered
      .map((session) => sessionShardMarkup(session, tabCounts[session.id] || 0, ctx, listView))
      .join('');
  }
  if (refs.sessionsEmpty) refs.sessionsEmpty.hidden = filtered.length > 0;
  markActiveSession(refs, activeSession?.id || '');

  const counts = browserSessionViewCounts(all, view);
  const pg = refs.sessionsPane?.__ctoxPaneGrammar;
  if (pg?.setCounts) pg.setCounts(counts);
  else writeSessionCounts(refs, counts);
  const noun = filtered.length === 1 ? 'Sitzung' : 'Sitzungen';
  const footer = `${filtered.length} ${noun} · ${bandLabel(view?.band)}`;
  if (pg?.setFooter) pg.setFooter(footer);
  else writeSessionFooter(refs, footer);
}

// In-place selection flip across existing rows — NEVER a rebuild.
function markActiveSession(refs, sessionId) {
  for (const node of refs.sessions?.querySelectorAll('[data-browser-session-id]') || []) {
    const active = node.dataset.browserSessionId === sessionId;
    node.classList.toggle('is-selected', active);
    node.classList.toggle('is-active', active);
    node.setAttribute('aria-selected', active ? 'true' : 'false');
  }
}

// Karten und Liste sind zwei verschiedene Dichten, keine zwei Innenabstaende
// (Betreiber-Direktive 31.08.2026):
//   KARTEN = fetter Titel + Betriebs-Meta + Detailzeile (drei Zeilen),
//   LISTE  = genau eine Zeile, Titel links und EIN Kurz-Meta rechts.
// Beide Formen leben von Feldern, die die App ohnehin geladen hat.
function sessionShardMarkup(session, tabCount, ctx = null, listView = 'cards') {
  const url = session.current_url || session.payload?.target_url || '';
  const title = browserDisplayTitle(null, session, url);
  const importedClass = session.__imported ? ' browser-session--imported' : '';
  // Das Symbol traegt den Zustand, nicht nur der Text: bei 34 Sitzungen findet
  // man "Eingriff noetig" sonst nicht, ohne jede Zeile zu lesen.
  const z = browserSitzungZustand(session, ctx);
  const open = `
    <div class="ctox-list-item browser-session${importedClass} ${z.klasse}" role="option" aria-selected="false" tabindex="0"
      data-browser-session-id="${escapeHtml(session.id)}"
      data-browser-zustand="${escapeHtml(z.klasse)}"
      data-context-record-id="${escapeHtml(session.id)}"
      data-context-record-type="browser_session"
      data-context-label="${escapeHtml(title)}">
      <span class="browser-session-zustand" title="${escapeHtml(z.text)}" aria-label="${escapeHtml(z.text)}">${z.symbol}</span>
      <span class="browser-session-title">${escapeHtml(title)}</span>`;
  if (listView === 'list') {
    const brief = browserSessionListMeta(session, tabCount, ctx);
    return `${open}
      <span class="browser-session-brief" title="${escapeHtml(brief)}">${escapeHtml(brief)}</span>
    </div>`;
  }
  const meta = browserSessionShardMeta(session, tabCount, ctx);
  const detail = browserSessionShardDetail(session, url);
  return `${open}
      <span class="browser-session-meta" title="${escapeHtml(meta)}">${escapeHtml(meta)}</span>
      <span class="browser-session-detail" title="${escapeHtml(detail)}">${escapeHtml(detail)}</span>
    </div>`;
}

// WER STEUERT DIESE SITZUNG, und muss jemand eingreifen?
//
// Bis 13.08.2026 stand in jeder Zeile nur "Persoenlich · Bereit · 1 Tab". Bei
// 34 Sitzungen war die Liste damit wertlos: man sah nicht, ob eine Sitzung dem
// Nutzer gehoert oder von der Recherche gefahren wird, und vor allem nicht, wo
// jemand eingreifen muss. Eine Automatik, die auf eine Anmeldung oder eine
// Bot-Pruefung wartet, lief still in ihre Zeitueberschreitung — der Klick, der
// gereicht haette, wurde nie angefordert.
//
// Die vier Zustaende kommen ohne neue Felder aus; sie stehen alle schon im
// Sitzungsdatensatz.
const SITZUNG_ZUSTAENDE = Object.freeze({
  eingriff: { symbol: '⚠', klasse: 'is-eingriff', text: 'Eingriff nötig' },
  nutzer: { symbol: '👤', klasse: 'is-nutzer', text: 'Sie steuern' },
  automatik: { symbol: '⚙', klasse: 'is-automatik', text: 'Automatik' },
  ruhend: { symbol: '○', klasse: 'is-ruhend', text: 'Ruhend' },
});

function browserSitzungZustand(session, ctx, jetzt = Date.now()) {
  if (!session?.id) return SITZUNG_ZUSTAENDE.ruhend;
  // 1. Eingriff zuerst — er ist der einzige Zustand, der jemanden BRAUCHT.
  const fehler = String(browserSessionError(session) || '').toLowerCase();
  const titel = String(session.title || '').toLowerCase();
  const wartetAufMensch = /anmeld|login|sign in|captcha|verify|bot|zustimm|consent|just a moment/
    .test(`${fehler} ${titel}`);
  const lauft = ['active', 'running', 'ready', 'capturing'].includes(
    String(session.runtime_status || session.status || '').toLowerCase());
  if (lauft && wartetAufMensch) return SITZUNG_ZUSTAENDE.eingriff;
  if (!lauft) return SITZUNG_ZUSTAENDE.ruhend;

  // 2. Steuert der angemeldete Nutzer selbst — mit GUELTIGER Pacht?
  const meine = browserActorIds(ctx?.session) || [];
  const steuernder = String(session.controller_user_id || '');
  const pachtBis = Number(session.controller_lease_expires_at_ms || 0);
  if (steuernder && meine.includes(steuernder) && pachtBis > jetzt) {
    return SITZUNG_ZUSTAENDE.nutzer;
  }
  // 3. Sonst faehrt sie jemand anderes — Recherche, Adapter, anderer Nutzer.
  //    Diese Sitzungen NICHT versehentlich uebernehmen: ein Zugriff bricht
  //    einen laufenden Rechercheschritt ab.
  return SITZUNG_ZUSTAENDE.automatik;
}

function browserSessionShardMeta(session, tabCount = 0, ctx = null) {
  const status = browserStatusLabel(session);
  const profile = (session.profile_mode || session.payload?.profile_mode) === 'private' ? 'Privat' : 'Persönlich';
  const count = Number(tabCount || 0);
  const zustand = browserSitzungZustand(session, ctx);
  const parts = [zustand.text, profile, status, `${count} ${count === 1 ? 'Tab' : 'Tabs'}`];
  const error = browserSessionError(session);
  if (error) parts.push(error);
  if (session.__imported) parts.push('Import');
  return parts.join(' · ');
}

// Dritte Kartenzeile: wo die Sitzung steht und wann sie sich zuletzt gemeldet
// hat. Reine Anzeige aus bereits geladenen Feldern — kein neuer Datenpfad.
// `updated_at_ms` steht in sessionListSignature, die Zeile veraltet also nicht.
function browserSessionShardDetail(session, url = '') {
  const target = url || session?.current_url || session?.payload?.target_url || '';
  const parts = [];
  if (target) parts.push(browserUrlLabel(target));
  const seen = Number(session?.updated_at_ms || session?.created_at_ms || 0);
  if (seen > 0) {
    parts.push(new Date(seen).toLocaleString(undefined, {
      day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit',
    }));
  }
  if (session?.__imported) parts.push(t('shardImported', 'Import'));
  return parts.join(' · ');
}

// Listenzeile: EIN Kurz-Meta rechts. Der Betriebszustand ist das, wonach in
// einer dichten Liste gesucht wird — Profil, Fehlertext und Adresse stehen in
// der Kartenansicht und im Titel-Tooltip.
function browserSessionListMeta(session, tabCount = 0, ctx = null) {
  const zustand = browserSitzungZustand(session, ctx);
  const count = Number(tabCount || 0);
  return count > 0 ? `${zustand.text} · ${count}` : zustand.text;
}

// EIN Umschalter Karten/Liste. `data-pg-view` traegt die AKTUELLE Ansicht (die
// Shell-Grammatik liest sie dort), Symbol und Beschriftung benennen die ZIEL-
// Ansicht. aria-pressed entfaellt: der Knopf ist eine Aktion, kein Zustand —
// die Shell stempelt es auf [data-pg-view], hier faellt es wieder weg.
function syncViewToggle(refs, view) {
  const button = refs?.viewToggle;
  if (!button) return;
  const current = view === 'list' ? 'list' : 'cards';
  button.dataset.pgView = current;
  const label = current === 'list'
    ? t('viewShowCards', 'Als Karten anzeigen')
    : t('viewShowList', 'Als Liste anzeigen');
  button.setAttribute('aria-label', label);
  button.title = label;
  button.removeAttribute('aria-pressed');
}

function browserSessionBand(session) {
  // "Aktiv" means a native runtime has confirmed a live Chromium-backed
  // session. Starting, blocked and disconnected rows do not count as active.
  return browserUiState(session) === 'ready' ? 'active' : 'closed';
}

function browserSessionMatchesBand(session, band) {
  if (!band || band === 'all') return true;
  return browserSessionBand(session) === band;
}

function filterSessionsForView(sessions, view = {}) {
  const search = String(view.search || '').trim().toLowerCase();
  const profile = view.filters?.profile && view.filters.profile !== 'all' ? view.filters.profile : '';
  const band = view.band || 'all';
  return (Array.isArray(sessions) ? sessions : []).filter((session) => {
    if (!browserSessionMatchesBand(session, band)) return false;
    if (profile) {
      const mode = session.profile_mode || session.payload?.profile_mode || 'persistent';
      if (mode !== profile) return false;
    }
    if (search) {
      const hay = `${session.id || ''} ${session.title || ''} ${session.current_url || ''} ${session.payload?.target_url || ''}`.toLowerCase();
      if (!hay.includes(search)) return false;
    }
    return true;
  });
}

// Band counts ignore the band selection (the band IS the selector) but honor
// search + profile filters; zeros are rendered, never hidden.
function browserSessionViewCounts(sessions, view = {}) {
  const base = filterSessionsForView(sessions, { ...view, band: 'all' });
  return {
    all: base.length,
    active: base.filter((session) => browserSessionBand(session) === 'active').length,
    closed: base.filter((session) => browserSessionBand(session) === 'closed').length,
  };
}

// Selection-independent signature: a pure selection change (not part of the
// signature) leaves it identical, so renderSessions skips the rebuild.
function sessionListSignature(filteredSessions, listView, tabCounts = {}) {
  const rows = (Array.isArray(filteredSessions) ? filteredSessions : []).map((session) => [
    session.id || '',
    browserUiState(session),
    String(session.title || session.current_url || ''),
    browserSessionError(session),
    session.profile_mode || session.payload?.profile_mode || '',
    Number(tabCounts[session.id] || 0),
    Number(session.updated_at_ms || 0),
    session.__imported ? 'i' : '',
    // Der Zustand haengt an Steuerndem und Pacht — ohne diese beiden
    // Werte in der Signatur bliebe das Symbol stehen, wenn eine Pacht
    // ablaeuft oder jemand anderes uebernimmt.
    String(session.controller_user_id || ''),
    Number(session.controller_lease_expires_at_ms || 0) > Date.now() ? 'p' : '-',
  ].join(':'));
  return `${listView}::${rows.join('|')}`;
}

// Auto-reveal model (design-guide "Progressive Disclosure"): the remote work
// surface is the browser's "detail" — revealed once a session is selected and
// not user-collapsed. Mirrors the outbound/conversations idiom.
function browserWorkbenchVisible(hasSession, userCollapsed = false) {
  return Boolean(hasSession) && !userCollapsed;
}

function bandLabel(band) {
  if (band === 'active') return 'Aktiv';
  if (band === 'closed') return 'Beendet';
  return 'Alle';
}

function writeSessionCounts(refs, counts) {
  for (const [key, value] of Object.entries(counts || {})) {
    const node = refs.sessionsPane?.querySelector(`[data-pg-count="${key}"]`);
    if (node) node.textContent = ` (${value})`;
  }
}

function writeSessionFooter(refs, text) {
  const node = refs.sessionsPane?.querySelector('[data-pg-footer]');
  if (node) node.textContent = text || '';
}

// Export/import are honest and small: export writes owned sessions as JSON via a
// Blob; import overlays sessions for local review only (never persisted).
function buildBrowserSessionsExport(sessions, nowMs = Date.now()) {
  return {
    kind: 'browser_sessions',
    schema_version: 1,
    exported_at_ms: Number(nowMs) || 0,
    sessions: (Array.isArray(sessions) ? sessions : []).map((session) => ({
      id: session.id || '',
      title: browserDisplayTitle(null, session, session.current_url || ''),
      url: session.current_url || session.payload?.target_url || '',
      status: session.runtime_status || session.status || '',
      profile_mode: session.profile_mode || session.payload?.profile_mode || 'persistent',
      owner_user_id: session.owner_user_id || '',
      controller_user_id: session.controller_user_id || '',
      updated_at_ms: Number(session.updated_at_ms || 0),
    })),
  };
}

function parseBrowserSessionsImport(parsed) {
  const rows = Array.isArray(parsed)
    ? parsed
    : Array.isArray(parsed?.sessions) ? parsed.sessions : [];
  return rows
    .map((row) => {
      const id = String(row?.id || '').trim();
      if (!id) return null;
      const status = String(row.status || 'imported');
      return {
        id,
        title: String(row.title || ''),
        current_url: String(row.url || row.current_url || ''),
        status,
        runtime_status: status,
        profile_mode: String(row.profile_mode || 'persistent'),
        updated_at_ms: Number(row.updated_at_ms || 0),
        __imported: true,
      };
    })
    .filter(Boolean);
}

function downloadBrowserJson(payload, filename, root) {
  let url = '';
  try {
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
    url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = filename;
    anchor.rel = 'noopener';
    (root || document.body)?.appendChild?.(anchor);
    anchor.click();
    anchor.remove?.();
  } catch (error) {
    console.error('[browser] session export failed', error);
  } finally {
    if (url) setTimeout(() => { try { URL.revokeObjectURL(url); } catch {} }, 4000);
  }
}

function exportBrowserSessions(state, refs) {
  const rows = sessionRenderList(state).filter((session) => !session.__imported);
  downloadBrowserJson(buildBrowserSessionsExport(rows, Date.now()), 'browser-sessions.json', refs?.root);
}

function importBrowserSessions(ctx, state, refs) {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = 'application/json,.json';
  input.addEventListener('change', async () => {
    const file = input.files && input.files[0];
    if (!file) return;
    let parsed;
    try {
      parsed = JSON.parse(await file.text());
    } catch {
      ctx.notifications?.show?.({ type: 'error', title: 'Browser', message: 'Ungültige JSON-Datei.' });
      return;
    }
    const imported = parseBrowserSessionsImport(parsed);
    if (!imported.length) {
      ctx.notifications?.show?.({ type: 'warning', title: 'Browser', message: 'Keine Sitzungen in der Datei.' });
      return;
    }
    state.importedSessions = imported;
    renderLeftRail(ctx, refs, state);
    renderTabstrip(ctx, refs, state);
    ctx.notifications?.show?.({ type: 'info', title: 'Browser', message: `${imported.length} Sitzungen geladen (nur lokal).` });
  });
  input.click();
}

function renderSession(refs, session, tab, frame, command, state) {
  if (!session) {
    refs.sessionCard.innerHTML = '<span class="browser-muted">Kein Browserfenster</span>';
    return;
  }
  const url = tab?.url || session.current_url || '';
  const commandError = commandErrorMessage(command);
  const runtimeError = browserSessionError(session);
  const sessionError = runtimeError
    ? `<div class="browser-error"><strong>Browser-Laufzeit:</strong> ${escapeHtml(runtimeError)}</div>`
    : '';
  const commandLine = command
    ? `<div class="browser-muted">Letzte Browseraktion: ${escapeHtml(browserActionLabel(command.command_type || command.type || 'browser'))}</div>`
    : '';
  const policy = session.payload?.control_policy || {};
  const owner = session.owner_user_id || policy.owner_user_id || '-';
  const controller = session.controller_user_id || policy.controller_user_id || '-';
  const leaseRemaining = Math.max(0, Number(session.controller_lease_expires_at_ms || 0) - Date.now());
  const profileMode = session.profile_mode || session.payload?.profile_mode || 'persistent';
  const status = browserStatusLabel(session);
  if (refs.address && url && !state?.addressDirty && document.activeElement !== refs.address) refs.address.value = url;
  refs.sessionCard.innerHTML = `
    <strong>${escapeHtml(browserDisplayTitle(tab, session, url))}</strong>
    <div class="browser-muted">${escapeHtml(url || 'about:blank')}</div>
    <div class="browser-meta-grid">
      <span>Status</span><span>${escapeHtml(status)}</span>
      <span>Owner</span><span>${escapeHtml(owner)}</span>
      <span>Control</span><span>${escapeHtml(controller)}</span>
      <span>Lease</span><span>${leaseRemaining ? `${Math.ceil(leaseRemaining / 60000)} min` : 'inaktiv'}</span>
      <span>Beobachter</span><span>${Array.isArray(session.allowed_observer_user_ids) ? session.allowed_observer_user_ids.length : 0}</span>
      <span>Profil</span><span>${profileMode === 'private' ? 'Privat – wird gelöscht' : 'Persönlich – persistent'}</span>
      <span>Frame</span><span>${escapeHtml(frame ? `${frame.width}x${frame.height}` : 'Kein Bild')}</span>
    </div>
    ${commandLine}
    ${sessionError}
    ${commandError ? `<div class="browser-error">${escapeHtml(commandError)}</div>` : ''}
  `;
}

function browserActionLabel(commandType) {
  const type = String(commandType || '');
  if (type === 'browser.session.start') return 'Fenster geoeffnet';
  if (type === 'browser.navigate') return 'Navigation';
  if (type === 'browser.reload') return 'Neu laden';
  if (type === 'browser.back') return 'Zurueck';
  if (type === 'browser.forward') return 'Vor';
  if (type === 'browser.reset') return 'Zurueckgesetzt';
  if (type === 'browser.session.stop') return 'Fenster geschlossen';
  return 'Aktualisiert';
}

function renderAuthAssist(refs, session) {
  if (!refs.authAssist) return;
  const payload = session?.payload || {};
  const permission = payload.pending_permission;
  if (permission && typeof permission === 'object') {
    refs.authAssist.hidden = false;
    refs.authAssist.innerHTML = `
      <div>
        <span class="ctox-pane-kicker">Website-Berechtigung</span>
        <strong>${escapeHtml(titleCase(permission.kind || 'permission'))}</strong>
        <small>${escapeHtml(permission.origin || '')}</small>
      </div>
      <div class="ctox-pane-tools">
        <button type="button" class="ctox-button" data-browser-permission-response="dismiss">Blockieren</button>
        <button type="button" class="ctox-button" data-browser-permission-response="accept">Einmal erlauben</button>
      </div>`;
    return;
  }
  const httpAuth = payload.pending_http_auth;
  if (httpAuth && typeof httpAuth === 'object') {
    refs.authAssist.hidden = false;
    refs.authAssist.innerHTML = `
      <div>
        <span class="ctox-pane-kicker">HTTP ${escapeHtml(httpAuth.scheme || 'Basic')} Authentifizierung</span>
        <strong>${escapeHtml(httpAuth.realm || httpAuth.origin || 'Geschützter Bereich')}</strong>
        <small>Zugangsdaten werden ausschließlich über eine CTOX Secret-Referenz eingesetzt.</small>
      </div>
      <div class="ctox-pane-tools">
        <button type="button" class="ctox-button" data-browser-http-auth-response="dismiss">Abbrechen</button>
        <button type="button" class="ctox-button" data-browser-http-auth-response="accept">Secret verwenden</button>
      </div>`;
    return;
  }
  const webAuthn = payload.pending_webauthn;
  if (webAuthn && typeof webAuthn === 'object') {
    refs.authAssist.hidden = false;
    refs.authAssist.innerHTML = `
      <div>
        <span class="ctox-pane-kicker">Passkey ${escapeHtml(webAuthn.type === 'create' ? 'registrieren' : 'verwenden')}</span>
        <strong>${escapeHtml(webAuthn.rp_id || 'Unbekannte Website')}</strong>
        <small>CTOX verwendet den verschlüsselten serverseitigen Passkey erst nach Ihrer Bestätigung.</small>
      </div>
      <div class="ctox-pane-tools">
        <button type="button" class="ctox-button" data-browser-webauthn-response="dismiss">Ablehnen</button>
        <button type="button" class="ctox-button" data-browser-webauthn-response="accept">Bestätigen</button>
      </div>`;
    return;
  }
  const dialog = payload.pending_dialog;
  if (dialog && typeof dialog === 'object') {
    refs.authAssist.hidden = false;
    refs.authAssist.innerHTML = `
      <div>
        <span class="ctox-pane-kicker">${escapeHtml(titleCase(dialog.type || 'dialog'))}</span>
        <strong>${escapeHtml(dialog.message || 'Die Webseite wartet auf eine Entscheidung.')}</strong>
      </div>
      <div class="ctox-pane-tools">
        <button type="button" class="ctox-button" data-browser-dialog-response="dismiss">Abbrechen</button>
        <button type="button" class="ctox-button" data-browser-dialog-response="accept">Bestätigen</button>
      </div>`;
    return;
  }
  const isAuthAssist = payload.purpose === 'web_stack_auth';
  refs.authAssist.hidden = !isAuthAssist;
  if (!isAuthAssist) {
    refs.authAssist.innerHTML = '';
    return;
  }
  const completed = payload.auth_assist_status === 'completed' || payload.authenticated === true;
  const fillStatus = payload.credential_fill_status || '';
  const extractStatus = payload.capture_extract_status || '';
  const canFill = Boolean(payload.secret_name) && !completed;
  const canCapture = Boolean(completed && payload.capture_script);
  const canExtract = Boolean(completed && payload.capture_script);
  const domains = Array.isArray(payload.allowed_domains) ? payload.allowed_domains.join(', ') : '';
  const instruction = String(payload.instruction || '').trim()
    || 'Melden Sie sich auf der geöffneten Seite an und lösen Sie gegebenenfalls MFA oder Captcha. Bestätigen Sie erst danach die Fortsetzung.';
  refs.authAssist.innerHTML = `
    <div class="browser-auth-summary">
      <span class="ctox-pane-kicker">Web Stack Anmeldung</span>
      <strong>${escapeHtml(payload.source_id || 'Anmeldung erforderlich')}</strong>
      <small class="browser-auth-instruction" title="${escapeHtml(instruction)}">${escapeHtml(domains || payload.target_url || '')} · ${escapeHtml(instruction)}</small>
      ${fillStatus ? `<small>${escapeHtml(authAssistStatusLabel(fillStatus, 'Zugangsdaten werden eingesetzt'))}</small>` : ''}
      ${extractStatus ? `<small>${escapeHtml(authAssistStatusLabel(extractStatus, 'Seitenauswertung laeuft'))}</small>` : ''}
    </div>
    <div class="ctox-pane-tools">
      <button type="button" class="ctox-button" data-browser-credential-fill ${canFill ? '' : 'disabled'}>
        Zugangsdaten einsetzen
      </button>
      <button type="button" class="ctox-button" data-browser-auth-complete ${completed ? 'disabled' : ''}>
        ${completed ? 'Recherche wird fortgesetzt' : 'Erledigt – Recherche fortsetzen'}
      </button>
      <button type="button" class="ctox-button" data-browser-web-stack-capture ${canCapture ? '' : 'disabled'}>
        An CTOX uebergeben
      </button>
      <button type="button" class="ctox-button" data-browser-web-stack-extract ${canExtract ? '' : 'disabled'}>
        Seite auslesen
      </button>
    </div>
  `;
}

function renderAutomationOverlay(refs, session) {
  if (!refs.automationOverlay) return;
  const payload = session?.payload || {};
  const captureScript = String(payload.capture_script || '').trim();
  const isWebStack = payload.purpose === 'web_stack_auth' || Boolean(captureScript);
  refs.automationOverlay.hidden = !isWebStack;
  if (!isWebStack) {
    if (refs.automationCode) refs.automationCode.textContent = '';
    return;
  }
  const source = String(payload.source_id || payload.target_url || session?.current_url || 'Web Stack');
  const extractStatus = String(payload.capture_extract_status || '').toLowerCase();
  const authenticated = payload.auth_assist_status === 'completed' || payload.authenticated === true;
  const status = extractStatus
    ? authAssistStatusLabel(extractStatus, 'Seitenauswertung läuft')
    : authenticated
      ? 'Bereit zum Auslesen'
      : 'Wartet auf Anmeldung';
  if (refs.automationTitle) refs.automationTitle.textContent = 'Playwright-Scraper';
  if (refs.automationStatus) refs.automationStatus.textContent = status;
  if (refs.automationSource) refs.automationSource.textContent = source;
  if (refs.automationCode) {
    refs.automationCode.textContent = captureScript || 'Noch kein Capture-Code für diese Quelle hinterlegt.';
  }
}

function authAssistStatusLabel(status, fallback) {
  const normalized = String(status || '').toLowerCase();
  if (['completed', 'done', 'ok', 'success'].includes(normalized)) return 'Abgeschlossen';
  if (['pending', 'pending_sync', 'accepted', 'running'].includes(normalized)) return fallback;
  if (['failed', 'error'].includes(normalized)) return 'Aktion fehlgeschlagen';
  return fallback;
}

function renderStatus(refs, session, tab, frame, command) {
  const state = browserUiState(session);
  if (refs.statusChip) {
    refs.statusChip.textContent = session ? browserStatusLabel(session) : t('statusDisconnected', 'Nicht verbunden');
    refs.statusChip.dataset.state = state;
    refs.statusChip.classList.toggle('is-success', state === 'ready');
    refs.statusChip.classList.toggle('is-warning', ['starting', 'waiting', 'blocked'].includes(state));
    refs.statusChip.classList.toggle('is-danger', state === 'error');
  }
  const url = tab?.url || session?.current_url || '';
  if (refs.statusTitle) {
    refs.statusTitle.textContent = browserDisplayTitle(tab, session, url) || t('noWindowOpen', 'Kein Browser-Fenster geoeffnet');
  }
  const bits = [];
  if (url) bits.push(url);
  if (frame?.seq != null) bits.push(`Frame ${frame.seq}`);
  if (session?.frame_rate_target) bits.push(`${session.frame_rate_target} fps`);
  const error = commandErrorMessage(command) || browserSessionError(session);
  if (error) bits.push(`Grund: ${error}`);
  if (refs.statusMeta) refs.statusMeta.textContent = bits.join(' - ') || '-';
}

function renderDownloads(refs, session) {
  if (!refs.downloads) return;
  const downloads = Array.isArray(session?.payload?.downloads) ? session.payload.downloads : [];
  refs.downloads.hidden = downloads.length === 0;
  refs.downloads.innerHTML = downloads.map((download) => `
    <span class="browser-download-item">
      <strong>${escapeHtml(download.filename || 'Download')}</strong>
      · ${escapeHtml(download.status || 'Unbekannt')}
      · ${escapeHtml(formatBytes(download.size_bytes || 0))}
      <button type="button" class="ctox-button ctox-button--sm" data-browser-download-action="release" data-browser-download-id="${escapeHtml(download.id || '')}" ${download.status === 'clean' ? '' : 'disabled'}>Freigeben</button>
      <button type="button" class="ctox-button ctox-button--sm" data-browser-download-action="rescan" data-browser-download-id="${escapeHtml(download.id || '')}" ${['infected', 'discarded', 'released'].includes(download.status) ? 'disabled' : ''}>Neu prüfen</button>
      <button type="button" class="ctox-button ctox-button--sm" data-browser-download-action="discard" data-browser-download-id="${escapeHtml(download.id || '')}" ${['discarded', 'released'].includes(download.status) ? 'disabled' : ''}>Verwerfen</button>
    </span>
  `).join('');
}

function renderControls(ctx, refs, state) {
  const hasSession = Boolean(state.latestSession?.id);
  const isStopped = ['stopped', 'closed'].includes(String(state.latestSession?.status || state.latestSession?.runtime_status || '').toLowerCase());
  const surfaceFocused = browserSurfaceIsFocused(ctx);
  const canControl = browserSurfaceCanControl(ctx, state);
  for (const button of [refs.go, refs.stop, refs.reload, refs.back, refs.forward, refs.sendToCtox, refs.upload, refs.newTab, refs.clipboardCopy, refs.clipboardPaste, refs.clipboardClear]) {
    if (!button) continue;
    button.disabled = !hasSession || isStopped || !canControl;
  }
  // The address bar is also the recovery path for a stale or disconnected
  // session. Keep it operable; submit starts a fresh leased session when the
  // current surface cannot safely navigate the existing one.
  if (refs.go) refs.go.disabled = false;
  if (refs.controllerAcquire) {
    // NICHT am Fokus aufhaengen. Ein Klick AUF diesen Knopf ist die
    // Interaktion, die die Uebernahme rechtfertigt - ein deaktivierter Knopf
    // kann diesen Klick nie empfangen. Meldet die Huelle kein `is-focused`
    // (gemessen am 03.09.2026 auf THESEN: visibilityState=hidden,
    // hasFocus()=false bei offenem Fenster), war die Sitzung dauerhaft
    // unbedienbar UND der einzige Ausweg gesperrt.
    refs.controllerAcquire.disabled = !hasSession || isStopped || canControl;
  }
  if (refs.controllerRelease) {
    refs.controllerRelease.disabled = !hasSession || isStopped || !canControl;
  }
  for (const button of [refs.observerGrant, refs.observerRevoke]) {
    if (!button) continue;
    button.disabled = !hasSession || isStopped || !surfaceFocused;
  }
  refs.canvas?.setAttribute('aria-disabled', canControl ? 'false' : 'true');
}

function renderNotice(refs, notice) {
  if (!refs.notice) return;
  refs.notice.hidden = !notice;
  refs.notice.textContent = notice || '';
}

async function renderFrame(refs, frame, state) {
  // Der Buehnen-Platzhalter liegt im selben Container wie die Skriptansicht.
  // Ohne diese Abfrage schien "Browser-Inhalt wird geladen" mitten in den
  // angezeigten Quelltext hinein, sobald neu gerendert wurde.
  const skriptSichtbar = state.ansicht === 'script';
  if (!frame?.data || state.drawing) {
    refs.empty.hidden = skriptSichtbar || Boolean(frame?.data);
    if (!frame?.data) refs.empty.textContent = frameEmptyText(state);
    return;
  }
  state.drawing = true;
  try {
    const img = new Image();
    await new Promise((resolve, reject) => {
      img.onload = resolve;
      img.onerror = reject;
      img.src = `data:${frame.mime_type || 'image/png'};base64,${frame.data}`;
    });
    refs.canvas.width = Number(frame.width || VIEWPORT.width);
    refs.canvas.height = Number(frame.height || VIEWPORT.height);
    const ctx = refs.canvas.getContext('2d');
    ctx.clearRect(0, 0, refs.canvas.width, refs.canvas.height);
    ctx.drawImage(img, 0, 0, refs.canvas.width, refs.canvas.height);
    refs.empty.hidden = true;
  } catch (error) {
    console.error('[browser] frame render failed', error);
    refs.empty.hidden = skriptSichtbar;
    refs.empty.textContent = t('frameRenderFailed', 'Die Seite konnte nicht angezeigt werden.');
  } finally {
    state.drawing = false;
  }
}

function frameEmptyText(state) {
  const session = state.latestSession;
  const commandError = commandErrorMessage(state.latestCommand);
  if (commandError) return commandError;
  if (!session) return t('frameOpenNew', 'Oeffne ein neues Browser-Fenster');
  const status = String(session.runtime_status || session.status || '').toLowerCase();
  const runtimeError = browserSessionError(session);
  if (['failed', 'error', 'blocked'].includes(status)) {
    return runtimeError || t('frameStartFailed', 'Der Browser konnte nicht gestartet werden');
  }
  if (['disconnected', 'offline'].includes(status)) {
    return runtimeError || 'Kein laufender Browser-Prozess. Starten Sie die Sitzung erneut.';
  }
  if (status === 'stopped' || status === 'closed') return t('frameClosed', 'Browser-Fenster geschlossen');
  // Eine abgelaufene Steuerpacht sieht aus wie ein haengendes Bild: der
  // Bildtakt fragt ohne Pacht gar nicht erst, und die Buehne meldete
  // "Browser-Inhalt wird geladen", waehrend die Sitzung "Bereit" zeigte.
  // Am 03.09.2026 auf THESEN gemessen: Pacht seit 4,7 Stunden abgelaufen.
  const pachtEndeMs = Number(session.controller_lease_expires_at_ms || 0);
  const pachtTot = !Number.isFinite(pachtEndeMs) || pachtEndeMs <= Date.now();
  if (['active', 'running', 'capturing'].includes(status) && pachtTot) {
    return 'Steuerung abgelaufen. Klicken Sie auf „Steuerung übernehmen“, um Bild und Eingabe zurückzuholen.';
  }
  if (status === 'requested' || status === 'starting' || status === 'pending_command') return t('frameStarting', 'Browser wird gestartet');
  return t('frameLoading', 'Browser-Inhalt wird geladen');
}

function browserUiState(session) {
  if (!session) return 'offline';
  const raw = String(session.runtime_status || session.status || '').toLowerCase();
  if (['active', 'running', 'capturing'].includes(raw)) return 'ready';
  if (['requested', 'starting', 'pending_command', 'pending_sync'].includes(raw)) return 'starting';
  if (raw === 'blocked') return 'blocked';
  if (['stopped', 'closed', 'disconnected', 'offline'].includes(raw)) return 'offline';
  if (['failed', 'error'].includes(raw)) return 'error';
  return raw ? 'waiting' : 'offline';
}

function browserStatusLabel(session) {
  const state = browserUiState(session);
  if (state === 'ready') return t('statusReady', 'Bereit');
  if (state === 'starting') return t('statusStarting', 'Startet');
  if (state === 'waiting') return t('statusConnecting', 'Verbindet');
  if (state === 'blocked') return 'Aktion erforderlich';
  if (state === 'error') return t('statusError', 'Fehler');
  return t('statusDisconnected', 'Nicht verbunden');
}

function browserDisplayTitle(tab, session, url = '') {
  const raw = String(tab?.title || session?.title || '').trim();
  if (!raw || /^remote browser$/i.test(raw)) {
    return url ? browserUrlLabel(url) : 'Browser';
  }
  return raw;
}

function browserUrlLabel(url) {
  try {
    const parsed = new URL(url);
    return parsed.hostname || 'Browser';
  } catch {
    return 'Browser';
  }
}

function renderDiagnostics(refs, frame, inputs, command, commands = [], handoffTasks = []) {
  refs.frameId.textContent = frame?.id || '-';
  refs.frameSeq.textContent = frame?.seq == null ? '-' : String(frame.seq);
  refs.frameSize.textContent = frame?.size_bytes ? formatBytes(frame.size_bytes) : '-';
  refs.frameTime.textContent = frame?.captured_at_ms ? formatTime(frame.captured_at_ms) : '-';
  const pending = inputs.filter((event) => event.status === 'pending').length;
  const consumed = inputs.filter((event) => event.status === 'consumed').length;
  refs.inputState.textContent = `${pending} pending / ${consumed} consumed`;
  refs.commandState.textContent = command ? commandSummary(command) : '-';
  if (refs.commandHistory) {
    refs.commandHistory.innerHTML = commands.length
      ? commands.map((item) => `
          <div class="browser-command-row">
            <strong>${escapeHtml(item.command_type || item.type || 'browser')}</strong>
            <span>${escapeHtml(item.status || 'pending')}</span>
            <span>${escapeHtml(formatTime(item.updated_at_ms || item.created_at_ms))}</span>
            <span>${escapeHtml(commandErrorMessage(item) || item.record_id || '')}</span>
          </div>
        `).join('')
      : '<div class="browser-command-row"><span>No browser commands</span></div>';
  }
  if (refs.handoffHistory) {
    refs.handoffHistory.innerHTML = handoffTasks.length
      ? handoffTasks.map((item) => `
          <div class="browser-command-row">
            <strong>${escapeHtml(item.title || 'Browser context')}</strong>
            <span>${escapeHtml(item.status || item.route_status || 'queued')}</span>
            <span>${escapeHtml(formatTime(item.updated_at_ms))}</span>
            <span>${escapeHtml(item.id || item.command_id || '')}</span>
          </div>
        `).join('')
      : '<div class="browser-command-row"><span>No CTOX handoffs</span></div>';
  }
}

function commandSummary(command) {
  const type = command.command_type || command.type || 'browser';
  const status = command.status || 'pending';
  const error = commandErrorMessage(command);
  if (error) return `${type} failed: ${error}`;
  return `${type} ${status}`;
}

function commandErrorMessage(command) {
  if (!command) return '';
  const status = String(command.status || '').toLowerCase();
  const error = command.error || command.result?.error || command.payload?.error || '';
  if (status === 'failed' || error) return String(error || 'Die Browser-Aktion ist fehlgeschlagen.');
  return '';
}

async function upsertDoc(collection, doc) {
  if (!collection) throw new Error('Browser collection is not registered');
  const next = { ...doc };
  delete next._rev;
  delete next._meta;
  const existing = await collection.findOne(next.id).exec();
  if (existing?.incrementalPatch) {
    await existing.incrementalPatch(next);
    return;
  }
  if (existing) {
    await existing.patch(next);
  } else if (typeof collection.upsert === 'function') {
    await collection.upsert(next);
  } else {
    await collection.insert(next);
  }
}

async function patchDoc(collection, id, patch) {
  if (!collection || !id) return;
  const existing = await collection.findOne(id).exec();
  if (existing?.incrementalPatch) {
    await existing.incrementalPatch(patch);
  }
}

function normalizeUrl(value) {
  const trimmed = String(value || '').trim();
  if (!trimmed) return 'https://example.com';
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(trimmed)) return trimmed;
  return `https://${trimmed}`;
}

function browserSessionIdFromArgs(value) {
  const sessionId = String(value?.session_id || value?.sessionId || '').trim();
  return /^browser_session_[a-z0-9_-]+$/i.test(sessionId) ? sessionId : '';
}

function browserSurfaceIsFocused(ctx) {
  if (globalThis.document?.visibilityState === 'hidden') return false;
  if (globalThis.document?.hasFocus?.() === false) return false;
  const surface = ctx?.host?.closest?.('.shell-window');
  return Boolean(surface?.classList.contains('is-focused'));
}

// Warum die Bedienung gesperrt ist — im Klartext, nicht als Schweigen.
// Die Reihenfolge folgt der Pruefung in browserSurfaceCanControl.
function eingabeSperrgrund(ctx, state, now = Date.now()) {
  if (!browserSurfaceIsFocused(ctx)) return 'Das Browserfenster hat nicht den Fokus.';
  const session = state?.latestSession;
  if (!session?.id) return 'Keine aktive Sitzung.';
  const actorIds = browserActorIds(ctx?.session);
  if (!actorIds.length) return 'Kein angemeldeter Nutzer.';
  if (!actorIds.includes(String(session.controller_user_id || ''))) {
    return 'Die Sitzung wird gerade von jemand anderem gesteuert.';
  }
  const leaseId = String(session.controller_lease_id || '').trim();
  if (!leaseId) return 'Keine Steuerungsberechtigung für diese Sitzung.';
  if (session.controller_lease_id !== state.controllerLeaseId) {
    return 'Die Steuerung wurde an ein anderes Fenster übergeben.';
  }
  const expiresAt = Number(session.controller_lease_expires_at_ms || 0);
  if (!Number.isFinite(expiresAt) || expiresAt <= now) {
    return 'Die Steuerung ist abgelaufen.';
  }
  return '';
}

function setzeEingabeHinweis(ctx, state, text) {
  if (!state) return;
  if (state.eingabeHinweis === text) return;
  state.eingabeHinweis = text;
  // eingabeHinweis wurde gesetzt und NIRGENDS gerendert — es gab nur die zwei
  // Zuweisungen in dieser Funktion. Der Nutzer sah deshalb exakt nichts: kein
  // Klick wirkte, keine Meldung erschien. Am 19.08.2026 auf managed production tenant
  // gemessen: jede Eingabe endete in "Die Steuerung wurde an ein anderes
  // Fenster uebergeben", sichtbar allein in der Browserkonsole. Der Grund
  // gehoert dorthin, wo der Nutzer hinsieht — in die Statuszeile.
  if (text) state.notice = text;
  else if (state.notice === state.letzteSperrmeldung) state.notice = '';
  try { renderBrowserSurface?.(ctx, state); } catch {}
}

// Gedrosselt: bei gehaltener Maus entstehen sonst hunderte gleiche Zeilen.
function meldeEingabeGesperrt(ctx, state, type) {
  const grund = eingabeSperrgrund(ctx, state) || 'Bedienung gesperrt.';
  const jetzt = Date.now();
  if (state.letzteSperrmeldung !== grund || jetzt - (state.letzteSperrmeldungMs || 0) > 5000) {
    console.warn('[browser] Eingabe gesperrt', { grund, type });
    state.letzteSperrmeldung = grund;
    state.letzteSperrmeldungMs = jetzt;
  }
  setzeEingabeHinweis(ctx, state, grund);
}

function browserSurfaceCanControl(ctx, state, now = Date.now()) {
  if (!browserSurfaceIsFocused(ctx)) return false;
  const session = state?.latestSession;
  const actorIds = browserActorIds(ctx?.session);
  const expiresAt = Number(session?.controller_lease_expires_at_ms || 0);
  return Boolean(
    session?.id
      && actorIds.length
      && actorIds.includes(String(session.controller_user_id || ''))
      && String(session.controller_lease_id || '').trim()
      && session.controller_lease_id === state.controllerLeaseId
      && Number.isFinite(expiresAt)
      && expiresAt > now
  );
}

function browserCommandRequiresController(commandType, session) {
  if (!session?.id) return false;
  return ![
    'browser.session.start',
    'browser.controller.acquire',
    'browser.observer.grant',
    'browser.observer.revoke',
  ].includes(commandType);
}

function shouldRenewControllerLease(session, actorId, now = Date.now(), options = {}) {
  const {
    documentVisible = true,
    documentFocused = true,
    surfaceFocused = true,
    renewInFlight = false,
    controllerLeaseId = '',
  } = options;
  if (!documentVisible || !documentFocused || !surfaceFocused || renewInFlight) return false;
  const actorIds = Array.isArray(actorId)
    ? actorId.map((value) => String(value || '')).filter(Boolean)
    : [String(actorId || '')].filter(Boolean);
  if (!session?.id || !actorIds.includes(String(session.controller_user_id || ''))) return false;
  if (!String(session.controller_lease_id || '').trim()) return false;
  if (session.controller_lease_id !== controllerLeaseId) return false;
  const expiresAt = Number(session.controller_lease_expires_at_ms || 0);
  // Eine ABGELAUFENE Pacht wird bewusst NICHT erneuert — der Server weist die
  // Erneuerung ab, und alle 30 s ein Fehlschlag waere eine Endlosschleife.
  // Der Wächter darunter haelt das fest. Fuer diesen Fall ist NEU HOLEN
  // zustaendig, nicht Erneuern: siehe shouldReacquireControllerLease.
  if (!Number.isFinite(expiresAt) || expiresAt <= now) return false;
  return expiresAt - now <= 75_000;
}

// Abgelaufene Pacht: neu HOLEN statt zu erneuern.
//
// Bis 13.08.2026 gab es diesen Weg nicht. Die Erneuerung schloss den
// abgelaufenen Zustand zu Recht aus, aber niemand holte die Pacht danach
// zurueck — der Zustand, der die Reparatur am dringendsten braucht, hatte
// keine. Zusammen mit der Bedingung "nur bei sichtbarem, fokussiertem Fenster"
// ergab das eine Falle ohne Ausgang: Fenster wegklicken, Pacht laeuft
// planmaessig ab, zurueckkommen — und sie wird nie wieder angefasst.
// Auf der Kundeninstanz gemessen: beide Sitzungen "active", beide Pachten
// abgelaufen, null Eingabe-Ereignisse in zehn Minuten aktiver Bedienung.
//
// Anders als die Erneuerung braucht das Neuholen KEIN fokussiertes Fenster —
// sonst bliebe genau die Falle bestehen. Es verlangt aber, dass die Sitzung
// uns gehoert, und es laeuft nur, wenn nicht schon ein Versuch unterwegs ist.
// Jeder stille Abbruch hier kostete Stunden Fehlersuche: die Sitzung meldete
// "Bereit", nichts geschah, und keine Zeile sagte warum. Der Grund wird jetzt
// EINMAL je Wechsel protokolliert (nicht bei jedem 30-s-Takt).
function merkeReacquireGrund(session, grund) {
  try {
    const schluessel = `${session?.id || '-'}|${grund}`;
    if (globalThis.__ctoxBrowserReacquireGrund === schluessel) return;
    globalThis.__ctoxBrowserReacquireGrund = schluessel;
    console.info(`[browser] Steuerung wird NICHT zurueckgeholt: ${grund}`, { session: session?.id || '' });
  } catch { /* Diagnose darf nie stoeren */ }
}

function shouldReacquireControllerLease(session, actorId, now = Date.now(), options = {}) {
  const { reacquireInFlight = false, lastReacquireAtMs = 0 } = options;
  if (reacquireInFlight) return false;
  // Nach einem Fehlschlag nicht sofort wieder — sonst entsteht genau die
  // Endlosschleife, die der Wächter der Erneuerung verhindert.
  if (now - Number(lastReacquireAtMs || 0) < 10_000) return false;
  const actorIds = Array.isArray(actorId)
    ? actorId.map((value) => String(value || '')).filter(Boolean)
    : [String(actorId || '')].filter(Boolean);
  if (!session?.id || !actorIds.length) {
    merkeReacquireGrund(session, session?.id ? 'kein angemeldeter Nutzer' : 'keine Sitzung');
    return false;
  }
  // Fremd gesteuerte Sitzungen nicht an uns reissen.
  const controller = String(session.controller_user_id || '');
  if (controller && !actorIds.includes(controller)) {
    merkeReacquireGrund(session, `Sitzung gehoert ${controller}`);
    return false;
  }
  // Eine tote Sitzung braucht einen START, keine Pacht. Ohne diese Zeile hielt
  // die Wiederbeschaffung genau den Zustand fest, den sie heilen sollte: am
  // 15.08.2026 gemessen 259 erfolgreiche acquire in 48 h auf einer Sitzung mit
  // status=disconnected. Die stets gueltige Pacht liess submitAddress den
  // Navigations-Zweig waehlen und machte den Start-Zweig unerreichbar.
  if (browserSessionNeedsStart(session)) {
    merkeReacquireGrund(session, 'Sitzung braucht einen Start, keine Pacht');
    return false;
  }
  const expiresAt = Number(session.controller_lease_expires_at_ms || 0);
  if (!Number.isFinite(expiresAt) || expiresAt <= now) return true;
  // Gleicher Nutzer, andere Pacht: ein anderes Fenster haelt die Steuerung —
  // meist ein totes, etwa der Tab von vor einem Dienstneustart. Bisher wurde
  // nur bei ABGELAUFENER Pacht zurueckgeholt, also sperrte sich das lebende
  // Fenster selbst aus, solange die tote Pacht noch gueltig war. Die Uebernahme
  // laeuft nur auf ausdrueckliche Interaktion (ein Klick IN dieses Fenster),
  // nie auf einem Zeitgeber — sonst reissen sich zwei offene Tabs die Steuerung
  // im Wechsel gegenseitig weg. Fremde Nutzer sind oben bereits ausgeschlossen.
  if (!options.uebernahmeDurchInteraktion) return false;
  const eigene = String(options.currentLeaseId || '').trim();
  const fremde = String(session.controller_lease_id || '').trim();
  return Boolean(fremde) && fremde !== eigene;
}

// Nach einem Neuladen ist state.controllerLeaseId leer, waehrend die Sitzung
// uns weiterhin als Steuernden mit gueltiger Pacht fuehrt. Die Erneuerung
// verlangt aber Gleichheit beider Kennungen, also erkennt die App ihre eigene
// Pacht nicht wieder und erneuert sie nie. Sie muss stattdessen jedes Mal den
// Ablauf abwarten und zurueckholen — und in genau diesem Fenster lehnt der
// Server jeden Bildabruf ab ("browser live controller lease is missing or
// expired"), weshalb die Flaeche leer bleibt. Auf der Kundeninstanz gemessen:
// eigene=- fremd=d52e78cc rest=-2s, ueber Minuten unveraendert, waehrend die
// Sitzung serverseitig lief.
//
// Nur die EIGENE, noch gueltige Pacht wird wiedererkannt. Eine abgelaufene
// bleibt Sache von shouldReacquireControllerLease, und eine fremde Pacht --
// auch die eines anderen Fensters desselben Nutzers -- bleibt der Uebernahme
// durch ausdrueckliche Interaktion vorbehalten.
function erkenneEigenePachtWieder(ctx, state) {
  if (state.controllerLeaseId) return;
  const session = state.latestSession;
  const leaseId = String(session?.controller_lease_id || '').trim();
  if (!leaseId) return;
  const actorIds = browserActorIds(ctx.session);
  if (!actorIds.includes(String(session.controller_user_id || ''))) return;
  const expiresAt = Number(session.controller_lease_expires_at_ms || 0);
  if (!Number.isFinite(expiresAt) || expiresAt <= Date.now()) return;
  state.controllerLeaseId = leaseId;
}

// Umschalter zwischen Live-Ansicht und dem Automatisierungsskript dieser
// Sitzung. Das Skript wird pro Sitzung erzeugt und liegt neben dem Profil --
// es ist damit die einzige verlaessliche Antwort auf "was steuert diese
// Scraping-Sitzung gerade". Der Quellbaum kann davon abweichen: genau diese
// Abweichung kostete am 20.08.2026 einen Tag, weil der ausgelieferte Runner
// die Bildoperation gar nicht kannte, der Checkout aber schon.
//
// Bewusst nur lesend. Der Runner hat die Datei beim Start geladen; sie hier
// zu aendern wuerde nichts an der laufenden Sitzung aendern und nur den
// Eindruck erwecken, es taete es.
function installViewSwitch(ctx, refs, state) {
  const knoepfe = [...(refs.viewSwitch || [])];
  if (!knoepfe.length || !refs.scriptPanel) return;
  const zeige = async (ansicht) => {
    state.ansicht = ansicht;
    for (const k of knoepfe) {
      const aktiv = k.dataset.browserView === ansicht;
      k.classList.toggle('is-active', aktiv);
      k.setAttribute('aria-selected', aktiv ? 'true' : 'false');
    }
    const skript = ansicht === 'script';
    refs.scriptPanel.hidden = !skript;
    if (refs.canvas) refs.canvas.style.visibility = skript ? 'hidden' : '';
    if (refs.empty && skript) refs.empty.hidden = true;
    if (!skript || !refs.scriptCode) return;
    const sessionId = state.latestSession?.id;
    if (!sessionId) {
      refs.scriptCode.textContent = tBrowser(ctx, 'scriptNoSession', 'Keine laufende Sitzung — es gibt kein Skript zu zeigen.');
      return;
    }
    // Der Live-Kanal verlangt die Steuerungspacht fuer JEDE Operation, auch
    // fuer diese rein lesende. Ohne Vorabpruefung bekaeme der Betrachter hier
    // "controller lease is missing or expired" zu lesen -- fuer einen Klick auf
    // "Skript" eine Meldung, die niemand mit der Ursache verbindet.
    const pacht = state.controllerLeaseId || state.latestSession?.controller_lease_id || '';
    if (!pacht) {
      refs.scriptCode.textContent = tBrowser(ctx, 'scriptNeedsControl', 'Nur der Steuernde kann das Skript lesen — übernimm zuerst die Steuerung.');
      return;
    }
    refs.scriptCode.textContent = tBrowser(ctx, 'scriptLoading', 'Skript wird geladen …');
    try {
      const antwort = await ctx.sync.requestNative('ctox.browser.live.v1', {
        op: 'script',
        session_id: sessionId,
        lease_id: pacht,
      }, {
        collection: 'business_commands',
        requiredCapability: 'ctox-browser-live-v1',
        timeoutMs: 15_000,
      });
      refs.scriptCode.textContent = String(antwort?.script || '');
      if (refs.scriptPath) {
        const bytes = Number(antwort?.bytes || 0);
        refs.scriptPath.textContent = antwort?.path
          ? `${antwort.path} · ${Math.round(bytes / 1024)} KB`
          : '';
      }
    } catch (error) {
      refs.scriptCode.textContent = `${tBrowser(ctx, 'scriptFailed', 'Skript konnte nicht gelesen werden')}: ${error?.message || error}`;
    }
  };
  for (const k of knoepfe) {
    k.addEventListener('click', () => { zeige(k.dataset.browserView || 'live'); });
  }
  zeige('live');
}

function tBrowser(ctx, schluessel, standard) {
  const t = ctx?.i18n?.t;
  return typeof t === 'function' ? t(schluessel, standard) : standard;
}

function newBrowserControllerLeaseId() {
  return globalThis.crypto?.randomUUID?.()
    || `browser-lease-${Date.now()}-${Math.random().toString(36).slice(2, 12)}`;
}

function browserActorIds(session) {
  const user = session?.user && typeof session.user === 'object' ? session.user : {};
  return [...new Set([
    user.id,
    user.email,
    user.login,
    session?.userId,
  ].map((value) => String(value || '').trim()).filter(Boolean))];
}

function debounce(fn, delayMs) {
  let timer = null;
  return (...args) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), delayMs);
  };
}

function formatTime(ms) {
  try {
    return new Date(Number(ms)).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  } catch (_) {
    return '-';
  }
}

function formatBytes(bytes) {
  const value = Number(bytes || 0);
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
}

function titleCase(value) {
  const text = String(value || '').replace(/[_-]+/g, ' ');
  if (!text) return '';
  return text.charAt(0).toUpperCase() + text.slice(1);
}

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

export const __browserTestHooks = {
  SCRAPING_ADAPTER_COLLECTIONS,
  normalizeUrl,
  browserSessionIdFromArgs,
  formatBytes,
  titleCase,
  userSessionPrefix,
  selectedViewport,
  browserAuthRequestFromArgs,
  shouldRenewControllerLease,
  shouldReacquireControllerLease,
  browserSitzungZustand,
  eingabeSperrgrund,
  browserCommandRequiresController,
  browserSurfaceIsFocused,
  browserSurfaceCanControl,
  browserActorIds,
  mergeRequestedSession,
  mergeRequestedDocument,
  browserSessionError,
  browserSessionIsLive,
  browserAddressAction,
  startSperrgrund,
  browserSessionNeedsStart,
  browserStartErrorIsRetryable,
  newBrowserControllerLeaseId,
  filterSessionsForView,
  browserSessionViewCounts,
  browserSessionBand,
  browserUiState,
  browserStatusLabel,
  frameEmptyText,
  sessionListSignature,
  browserWorkbenchVisible,
  browserSessionShardMeta,
  browserSessionShardDetail,
  browserSessionListMeta,
  sessionShardMarkup,
  syncViewToggle,
  buildBrowserSessionsExport,
  parseBrowserSessionsImport,
  sessionRenderList,
  sessionTabCounts,
  browserInputTrace,
  browserInputPayload,
  eventModifiers,
  pointerButton,
  pointerButtons,
  pointerClickCount,
  keyboardText,
  canvasPoint,
  directResponseBelongsToSurface,
  applyDirectNavigationState,
  submitBrowserNav,
  writePointerInput,
  writeKeyboardInput,
  writeInputEvent,
  installInputHandlers,
};

async function ensureStyles() {
  const href = new URL(`./index.css?v=${STYLE_BUILD}`, import.meta.url).href;
  const moduleLinks = [...document.querySelectorAll('link[rel="stylesheet"]')].filter((link) => {
    try { return new URL(link.href).pathname.endsWith('/modules/browser/index.css'); } catch { return false; }
  });
  if (moduleLinks.some((link) => link.href === href)) {
    moduleLinks.filter((link) => link.href !== href).forEach((link) => link.remove());
    return;
  }
  moduleLinks.forEach((link) => link.remove());
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  document.head.appendChild(link);
}

// Translate static markup: data-t (textContent) and data-t-aria (aria-label).
// German markup text is the fallback when a key is missing.
function applyTranslations(root) {
  root.querySelectorAll('[data-t]').forEach((el) => {
    el.textContent = t(el.dataset.t, el.textContent.trim());
  });
  root.querySelectorAll('[data-t-aria]').forEach((el) => {
    el.setAttribute('aria-label', t(el.dataset.tAria, el.getAttribute('aria-label') || ''));
  });
}
