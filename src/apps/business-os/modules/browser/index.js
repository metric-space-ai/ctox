import { loadModuleMessages } from '../../shared/i18n.js';

const STYLE_BUILD = '20260715-browser-working-ui-v82';

// Module-level translator; set from locales/<lang>.json during mount.
let t = (key, fallback) => fallback ?? key;
const DEFAULT_SESSION_ID = 'browser_session_default';
const DEFAULT_TAB_ID = 'browser_tab_default';
const VIEWPORT = { width: 1280, height: 720 };
const FRAME_SYNC_RECOVERY_MS = 12000;
const BROWSER_SYNC_COLLECTIONS = [
  'browser_sessions',
  'browser_tabs',
  'browser_frames',
  'browser_input_events',
];

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
    refresh: root.querySelector('[data-browser-refresh]'),
    start: root.querySelector('[data-browser-start]'),
    privateMode: root.querySelector('[data-browser-private]'),
    viewport: root.querySelector('[data-browser-viewport]'),
    newTab: root.querySelector('[data-browser-new-tab]'),
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
    statusChip: root.querySelector('[data-browser-status-chip]'),
    statusTitle: root.querySelector('[data-browser-status-title]'),
    statusMeta: root.querySelector('[data-browser-status-meta]'),
    downloads: root.querySelector('[data-browser-downloads]'),
    authAssist: root.querySelector('[data-browser-auth-assist]'),
    shell: root.querySelector('[data-browser-frame-shell]'),
    canvas: root.querySelector('[data-browser-canvas]'),
    empty: root.querySelector('[data-browser-empty]'),
    frameId: root.querySelector('[data-browser-frame-id]'),
    frameSeq: root.querySelector('[data-browser-frame-seq]'),
    frameSize: root.querySelector('[data-browser-frame-size]'),
    frameTime: root.querySelector('[data-browser-frame-time]'),
    inputState: root.querySelector('[data-browser-input-state]'),
    commandState: root.querySelector('[data-browser-command-state]'),
    commandHistory: root.querySelector('[data-browser-command-history]'),
    handoffHistory: root.querySelector('[data-browser-handoff-history]'),
  };

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
    lastFrameSyncRecoveryAt: 0,
    leaseRenewInFlight: false,
    controllerLeaseId: '',
    addressDirty: false,
    requestedSessionStarts: new Set(),
  };

  const cleanups = [];
  let mounted = true;
  const scheduleRefresh = debounce(safeLoadAndRender, 80);

  const sessionSelectionToken = ctx.eventBus?.on?.('browser:select-session', (detail = {}) => {
    const sessionId = browserSessionIdFromArgs(detail);
    if (!sessionId) return;
    if (sessionId !== state.selectedSessionId) state.controllerLeaseId = '';
    state.selectedSessionId = sessionId;
    state.requestedSessionId = sessionId;
    scheduleRefresh();
    ensureRequestedBrowserSession(ctx, state, detail)
      .then(scheduleRefresh)
      .catch((error) => {
        state.notice = browserStartErrorMessage(error);
        scheduleRefresh();
      });
  });
  if (sessionSelectionToken && ctx.eventBus?.off) {
    cleanups.push(() => ctx.eventBus.off('browser:select-session', sessionSelectionToken));
  }
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
    browserCollection(ctx, 'browser_frames'),
    browserCollection(ctx, 'browser_input_events'),
    browserCollection(ctx, 'ctox_queue_tasks'),
  ]) {
    const sub = collection?.$?.subscribe?.(() => scheduleRefresh());
    if (sub?.unsubscribe) cleanups.push(() => sub.unsubscribe());
  }

  refs.refresh?.addEventListener('click', safeLoadAndRender);
  const startNewBrowserSession = (url = refs.address?.value || 'https://example.com') => {
    const now = Date.now();
    const sessionId = `${userSessionPrefix(ctx.session)}_${now}`;
    const tabId = `browser_tab_${now}`;
    const viewport = selectedViewport(refs.viewport);
    state.addressDirty = false;
    state.selectedSessionId = sessionId;
    state.requestedSessionId = sessionId;
    state.controllerLeaseId = newBrowserControllerLeaseId();
    state.notice = 'Browser wird mit CTOX verbunden …';
    safeLoadAndRender();
    const command = () => dispatchBrowserCommand(ctx, state, 'browser.session.start', {
      session_id: sessionId,
      tab_id: tabId,
      url,
      viewport_w: viewport.width,
      viewport_h: viewport.height,
      profile_mode: refs.privateMode?.checked ? 'private' : 'persistent',
      lease_id: state.controllerLeaseId,
      new_session: true,
    });
    runBrowserCommand(command());
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
    const leaseId = newBrowserControllerLeaseId();
    state.controllerLeaseId = leaseId;
    runBrowserCommand(
      dispatchBrowserCommand(ctx, state, 'browser.controller.acquire', { lease_id: leaseId })
        .catch((error) => {
          if (state.controllerLeaseId === leaseId) state.controllerLeaseId = '';
          throw error;
        }),
    );
  });
  refs.controllerRelease?.addEventListener('click', () => {
    const leaseId = state.controllerLeaseId;
    runBrowserCommand(
      dispatchBrowserCommand(ctx, state, 'browser.controller.release')
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
  refs.back?.addEventListener('click', () => dispatchBrowserCommand(ctx, state, 'browser.back').then(safeLoadAndRender));
  refs.forward?.addEventListener('click', () => dispatchBrowserCommand(ctx, state, 'browser.forward').then(safeLoadAndRender));
  refs.reload?.addEventListener('click', () => dispatchBrowserCommand(ctx, state, 'browser.reload').then(safeLoadAndRender));
  refs.address?.addEventListener('input', () => {
    state.addressDirty = true;
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
  const submitAddress = () => {
    const canNavigate = browserSurfaceCanControl(ctx, state);
    const url = refs.address?.value || 'https://example.com';
    if (!canNavigate) {
      startNewBrowserSession(url);
      return;
    }
    state.addressDirty = false;
    state.notice = 'Browser wird mit CTOX verbunden …';
    safeLoadAndRender();
    runBrowserCommand(dispatchBrowserCommand(ctx, state, 'browser.navigate', { url }));
  };
  refs.form?.addEventListener('submit', (event) => {
    event.preventDefault();
    submitAddress();
  });
  installInputHandlers(ctx, refs, state, scheduleRefresh);
  const leaseRenewTimer = globalThis.setInterval(renewControllerLeaseIfNeeded, 30_000);
  cleanups.push(() => globalThis.clearInterval(leaseRenewTimer));
  safeLoadAndRender();
  ensureRequestedBrowserSession(ctx, state, ctx.args)
    .then(scheduleRefresh)
    .catch((error) => {
      state.notice = browserStartErrorMessage(error);
      scheduleRefresh();
    });

  return () => {
    mounted = false;
    for (const cleanup of cleanups) {
      try { cleanup(); } catch (error) { console.error('[browser] cleanup failed', error); }
    }
    ctx.host.replaceChildren();
  };

  function renewControllerLeaseIfNeeded() {
    const session = state.latestSession;
    const actorId = String(ctx.session?.user?.id || ctx.session?.userId || '');
    const surface = ctx.host?.closest?.('.shell-window');
    if (!shouldRenewControllerLease(session, actorId, Date.now(), {
      documentVisible: globalThis.document?.visibilityState !== 'hidden',
      documentFocused: globalThis.document?.hasFocus?.() !== false,
      surfaceFocused: Boolean(surface?.classList.contains('is-focused')),
      renewInFlight: state.leaseRenewInFlight,
      controllerLeaseId: state.controllerLeaseId,
    })) return;
    state.leaseRenewInFlight = true;
    dispatchBrowserCommand(ctx, state, 'browser.controller.renew', {
      lease_id: state.controllerLeaseId,
    })
      .catch((error) => {
        console.warn('[browser] controller lease renewal failed', error);
      })
      .finally(() => {
        state.leaseRenewInFlight = false;
      });
  }

  function safeLoadAndRender() {
    loadAndRender().catch((error) => console.warn('[browser] refresh failed', error));
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
    const [commands, sessions, tabs, inputs, handoffTasks] = await Promise.all([
      readCollection(browserCollection(ctx, 'business_commands'), { limit: 50 }),
      readCollection(browserCollection(ctx, 'browser_sessions'), { limit: 20 }),
      readCollection(browserCollection(ctx, 'browser_tabs'), { limit: 40 }),
      readCollection(browserCollection(ctx, 'browser_input_events'), { limit: 80 }),
      readCollection(browserCollection(ctx, 'ctox_queue_tasks'), { limit: 50 }),
    ]);
    const actorId = String(ctx.session?.user?.id || ctx.session?.userId || '');
    const visibleSessions = sessions.filter((session) => session.owner_user_id === actorId);
    if (state.selectedSessionId
      && state.selectedSessionId !== state.requestedSessionId
      && !visibleSessions.some((session) => session.id === state.selectedSessionId)) {
      state.selectedSessionId = '';
    }
    const selectedSession = state.selectedSessionId ? latestSession(visibleSessions, state.selectedSessionId) : null;
    const requestedSessionPending = Boolean(state.requestedSessionId && !selectedSession);
    const frameSessionId = selectedSession?.id || state.selectedSessionId || '';
    const frames = frameSessionId
      ? await readCollection(browserCollection(ctx, 'browser_frames'), {
        limit: 20,
        selector: { session_id: frameSessionId },
      })
      : [];
    if (!mounted) return;
    const newestFrame = latestFrame(frames);
    state.latestSession = requestedSessionPending
      ? null
      : selectedSession || latestSession(visibleSessions, newestFrame?.session_id) || latestSession(visibleSessions);
    if (!requestedSessionPending) {
      state.selectedSessionId = state.latestSession?.id || '';
      if (state.latestSession?.id === state.requestedSessionId) state.requestedSessionId = '';
    }
    state.latestFrame = latestFrame(frames, state.latestSession?.id);
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
    renderSessionList(refs, visibleSessions, renderedTabs, state.latestSession);
    renderSession(refs, state.latestSession, state.latestTab, state.latestFrame, state.latestCommand, state);
    renderAuthAssist(refs, state.latestSession);
    renderStatus(refs, state.latestSession, state.latestTab, state.latestFrame, state.latestCommand);
    renderDownloads(refs, state.latestSession);
    renderDiagnostics(refs, state.latestFrame, inputs, state.latestCommand, state.browserCommands, state.handoffTasks);
    renderControls(ctx, refs, state);
    renderNotice(refs, state.notice);
    recoverFrameSyncIfNeeded(ctx, state);
    await renderFrame(refs, state.latestFrame, state);
  }
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

async function dispatchBrowserCommand(ctx, state, commandType, payloadPatch = {}) {
  const requiresController = browserCommandRequiresController(commandType, state.latestSession);
  if (requiresController && !browserSurfaceCanControl(ctx, state)) {
    throw new Error('Dieses Browser-Fenster ist nicht aktiv. Aktivieren Sie das Fenster oder übernehmen Sie die Steuerung.');
  }
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
  // The shell owns the live replication lifecycle and the command bus performs
  // its own command-collection flush. Awaiting another five-collection startup
  // here can deadlock submission while an existing peer is resyncing.
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
  const commandBus = requireCommandBus(ctx);
  await commandBus.dispatch(command, { until: 'accepted' });
  return { commandId, sessionId, tabId, opensNewSession };
}

async function ensureRequestedBrowserSession(ctx, state, args = {}) {
  const request = browserAuthRequestFromArgs(args);
  if (!request) return false;
  state.selectedSessionId = request.session_id;
  state.requestedSessionId = request.session_id;
  if (state.requestedSessionStarts.has(request.session_id)) return false;

  const existing = await browserCollection(ctx, 'browser_sessions')?.findOne(request.session_id).exec();
  if (existing) return false;

  state.requestedSessionStarts.add(request.session_id);
  try {
    state.controllerLeaseId = newBrowserControllerLeaseId();
    await dispatchBrowserCommand(ctx, state, 'browser.session.start', {
      ...request,
      lease_id: state.controllerLeaseId,
    });
    state.notice = 'Browser-Anmeldung wird geöffnet.';
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
  return {
    session_id: sessionId,
    tab_id: tabId,
    url: targetUrl,
    target_url: targetUrl,
    source_id: sourceId,
    purpose,
    allowed_domains: allowedDomains,
    capture_script: captureScript,
    secret_name: secretName,
    auth_assist_status: 'pending',
    profile_mode: 'persistent',
    secret_value_in_rxdb: false,
  };
}

function browserStartErrorMessage(error) {
  const code = String(error?.code || '').toLowerCase();
  if (code === 'auth_required') return 'Die Browser-Anmeldung benötigt eine neue Business-OS-Autorisierung.';
  if (code === 'native_unavailable') return 'Die Browser-Anmeldung konnte noch nicht mit CTOX verbunden werden. Bitte erneut versuchen.';
  if (['projection_delayed', 'sync_unavailable'].includes(code)) {
    return 'CTOX ist nicht mit dem Browser-Datenkanal verbunden. Der Browser wurde nicht gestartet. Bitte die Verbindung erneut aufbauen und dann erneut versuchen.';
  }
  const message = String(error?.message || '').trim();
  return message
    ? `Die Browser-Anmeldung konnte nicht geöffnet werden: ${message}`
    : 'Die Browser-Anmeldung konnte nicht geöffnet werden.';
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
  state.notice = 'Anmeldung wurde an CTOX uebergeben.';
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
    source_id: session.payload?.source_id || '',
    secret_scope: 'credentials',
    secret_name: secretName,
    field_role: session.payload?.credential_field_role || 'password',
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
    });
  state.notice = selector
    ? 'CTOX setzt die gespeicherten Zugangsdaten in das passende Feld ein.'
    : 'CTOX setzt die gespeicherten Zugangsdaten in das aktive Feld ein.';
}

async function startCommandSync(ctx) {
  return startBrowserRuntimeSync(ctx, ['business_commands']);
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
  await Promise.all(collections.map((collection) => ctx.sync.startCollection(collection)));
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

function installInputHandlers(ctx, refs, state, scheduleRefresh) {
  refs.canvas?.addEventListener('pointerdown', (event) => {
    refs.canvas.focus();
    refs.canvas.setPointerCapture?.(event.pointerId);
    writePointerInput(ctx, refs, state, 'mouseDown', event, { button: pointerButton(event.button) }).then(scheduleRefresh);
  });
  refs.canvas?.addEventListener('pointerup', (event) => {
    writePointerInput(ctx, refs, state, 'mouseUp', event, { button: pointerButton(event.button) }).then(scheduleRefresh);
  });
  refs.canvas?.addEventListener('pointermove', (event) => {
    const now = Date.now();
    if (now - Number(state.lastPointerMoveAt || 0) < 50) return;
    state.lastPointerMoveAt = now;
    writePointerInput(ctx, refs, state, 'mouseMove', event).then(scheduleRefresh);
  });
  refs.canvas?.addEventListener('wheel', (event) => {
    event.preventDefault();
    writePointerInput(ctx, refs, state, 'wheel', event, {
      dx: Number(event.deltaX || 0),
      dy: Number(event.deltaY || 0),
    }).then(scheduleRefresh);
  }, { passive: false });
  refs.canvas?.addEventListener('keydown', (event) => {
    event.preventDefault();
    writeKeyboardInput(ctx, state, 'keyDown', event).then(scheduleRefresh);
  });
  refs.canvas?.addEventListener('keyup', (event) => {
    if (event.metaKey || event.ctrlKey) return;
    event.preventDefault();
    writeKeyboardInput(ctx, state, 'keyUp', event).then(scheduleRefresh);
  });
}

async function writePointerInput(ctx, refs, state, type, event, extra = {}) {
  const point = canvasPoint(refs.canvas, event);
  await writeInputEvent(ctx, state, type, {
    x: point.x,
    y: point.y,
    buttons: Number(event.buttons || 0),
    modifiers: eventModifiers(event),
    payload: {
      pointer_id: event.pointerId || 0,
      pointer_type: event.pointerType || 'mouse',
      viewport_w: refs.canvas?.width || VIEWPORT.width,
      viewport_h: refs.canvas?.height || VIEWPORT.height,
      actor: actorContext(ctx.session),
    },
    ...extra,
  });
}

async function writeKeyboardInput(ctx, state, type, event) {
  await writeInputEvent(ctx, state, type, {
    key: event.key || '',
    code: event.code || '',
    modifiers: eventModifiers(event),
    text: type === 'keyDown' && event.key?.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey ? event.key : '',
    payload: {
      repeat: Boolean(event.repeat),
      location: Number(event.location || 0),
      actor: actorContext(ctx.session),
    },
  });
}

async function writeInputEvent(ctx, state, type, patch) {
  if (!browserSurfaceCanControl(ctx, state)) return;
  const session = state.latestSession;
  const frame = state.latestFrame;
  const sessionId = session?.id || frame?.session_id;
  if (!sessionId) return;
  const now = Date.now();
  const seq = Math.max(now, Number(state.lastInputSeq || 0) + 1);
  state.lastInputSeq = seq;
  const event = {
    id: `${sessionId}:input:${seq}:${type}`,
    tenant_id: browserTenantId(ctx),
    owner_user_id: session?.owner_user_id || '',
    controller_user_id: ctx.session?.user?.id || ctx.session?.userId || '',
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
  await upsertDoc(browserCollection(ctx, 'browser_input_events'), event);
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
  const scaleX = canvas.width / Math.max(1, rect.width);
  const scaleY = canvas.height / Math.max(1, rect.height);
  return {
    x: Math.max(0, Math.min(canvas.width, Math.round((event.clientX - rect.left) * scaleX))),
    y: Math.max(0, Math.min(canvas.height, Math.round((event.clientY - rect.top) * scaleY))),
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
  return 'left';
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
    <span class="browser-tab-item" data-browser-tab-id="${escapeHtml(tab.id)}" aria-current="${tab.id === activeSession?.current_tab_id ? 'true' : 'false'}">
      <span>${escapeHtml(tab.title || tab.url || 'Tab')}</span>
      <button type="button" class="ctox-icon-button" data-browser-tab-close aria-label="Tab schließen">×</button>
    </span>
  `).join('');
  refs.sessionList.innerHTML = tabMarkup;
}

function renderSession(refs, session, tab, frame, command, state) {
  if (!session) {
    refs.sessionCard.innerHTML = '<span class="browser-muted">Kein Browserfenster</span>';
    return;
  }
  const url = tab?.url || session.current_url || '';
  const commandError = commandErrorMessage(command);
  const sessionError = session.error ? `<div class="browser-error">${escapeHtml(session.error)}</div>` : '';
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
        <span class="browser-kicker">Website-Berechtigung</span>
        <strong>${escapeHtml(titleCase(permission.kind || 'permission'))}</strong>
        <small>${escapeHtml(permission.origin || '')}</small>
      </div>
      <div class="browser-auth-actions">
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
        <span class="browser-kicker">HTTP ${escapeHtml(httpAuth.scheme || 'Basic')} Authentifizierung</span>
        <strong>${escapeHtml(httpAuth.realm || httpAuth.origin || 'Geschützter Bereich')}</strong>
        <small>Zugangsdaten werden ausschließlich über eine CTOX Secret-Referenz eingesetzt.</small>
      </div>
      <div class="browser-auth-actions">
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
        <span class="browser-kicker">Passkey ${escapeHtml(webAuthn.type === 'create' ? 'registrieren' : 'verwenden')}</span>
        <strong>${escapeHtml(webAuthn.rp_id || 'Unbekannte Website')}</strong>
        <small>CTOX verwendet den verschlüsselten serverseitigen Passkey erst nach Ihrer Bestätigung.</small>
      </div>
      <div class="browser-auth-actions">
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
        <span class="browser-kicker">${escapeHtml(titleCase(dialog.type || 'dialog'))}</span>
        <strong>${escapeHtml(dialog.message || 'Die Webseite wartet auf eine Entscheidung.')}</strong>
      </div>
      <div class="browser-auth-actions">
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
  refs.authAssist.innerHTML = `
    <div>
      <span class="browser-kicker">Web Stack Anmeldung</span>
      <strong>${escapeHtml(payload.source_id || 'Anmeldung erforderlich')}</strong>
      <small>${escapeHtml(domains || payload.target_url || '')}</small>
      ${fillStatus ? `<small>${escapeHtml(authAssistStatusLabel(fillStatus, 'Zugangsdaten werden eingesetzt'))}</small>` : ''}
      ${extractStatus ? `<small>${escapeHtml(authAssistStatusLabel(extractStatus, 'Seitenauswertung laeuft'))}</small>` : ''}
    </div>
    <div class="browser-auth-actions">
      <button type="button" class="ctox-button" data-browser-credential-fill ${canFill ? '' : 'disabled'}>
        Zugangsdaten einsetzen
      </button>
      <button type="button" class="ctox-button" data-browser-auth-complete ${completed ? 'disabled' : ''}>
        ${completed ? 'Angemeldet' : 'Ich bin angemeldet'}
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
  }
  const url = tab?.url || session?.current_url || '';
  if (refs.statusTitle) {
    refs.statusTitle.textContent = browserDisplayTitle(tab, session, url) || t('noWindowOpen', 'Kein Browser-Fenster geoeffnet');
  }
  const bits = [];
  if (url) bits.push(url);
  if (frame?.seq != null) bits.push(`Frame ${frame.seq}`);
  if (session?.frame_rate_target) bits.push(`${session.frame_rate_target} fps`);
  const error = commandErrorMessage(command) || session?.error || '';
  if (error) bits.push(`Error: ${error}`);
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
      <button type="button" class="ctox-button" data-browser-download-action="release" data-browser-download-id="${escapeHtml(download.id || '')}" ${download.status === 'clean' ? '' : 'disabled'}>Freigeben</button>
      <button type="button" class="ctox-button" data-browser-download-action="rescan" data-browser-download-id="${escapeHtml(download.id || '')}" ${['infected', 'discarded', 'released'].includes(download.status) ? 'disabled' : ''}>Neu prüfen</button>
      <button type="button" class="ctox-button" data-browser-download-action="discard" data-browser-download-id="${escapeHtml(download.id || '')}" ${['discarded', 'released'].includes(download.status) ? 'disabled' : ''}>Verwerfen</button>
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
    refs.controllerAcquire.disabled = !hasSession || isStopped || !surfaceFocused || canControl;
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
  if (!frame?.data || state.drawing) {
    refs.empty.hidden = Boolean(frame?.data);
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
    refs.empty.hidden = false;
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
  if (status === 'failed' || status === 'error') return session.error || t('frameStartFailed', 'Der Browser konnte nicht gestartet werden');
  if (status === 'stopped' || status === 'closed') return t('frameClosed', 'Browser-Fenster geschlossen');
  if (status === 'requested' || status === 'starting' || status === 'pending_command') return t('frameStarting', 'Browser wird gestartet');
  return t('frameLoading', 'Browser-Inhalt wird geladen');
}

function browserUiState(session) {
  if (!session) return 'offline';
  const raw = String(session.runtime_status || session.status || '').toLowerCase();
  if (['active', 'running', 'capturing', 'synthetic'].includes(raw)) return 'ready';
  if (['requested', 'starting', 'pending_command', 'pending_sync'].includes(raw)) return 'starting';
  if (['stopped', 'closed'].includes(raw)) return 'offline';
  if (['failed', 'error'].includes(raw)) return 'error';
  return raw ? 'waiting' : 'offline';
}

function browserStatusLabel(session) {
  const state = browserUiState(session);
  if (state === 'ready') return t('statusReady', 'Bereit');
  if (state === 'starting') return t('statusStarting', 'Startet');
  if (state === 'waiting') return t('statusConnecting', 'Verbindet');
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

function browserSurfaceCanControl(ctx, state, now = Date.now()) {
  if (!browserSurfaceIsFocused(ctx)) return false;
  const session = state?.latestSession;
  const actorId = String(ctx?.session?.user?.id || ctx?.session?.userId || '');
  const expiresAt = Number(session?.controller_lease_expires_at_ms || 0);
  return Boolean(
    session?.id
      && actorId
      && session.controller_user_id === actorId
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
  if (!session?.id || !actorId || session.controller_user_id !== actorId) return false;
  if (!String(session.controller_lease_id || '').trim()) return false;
  if (session.controller_lease_id !== controllerLeaseId) return false;
  const expiresAt = Number(session.controller_lease_expires_at_ms || 0);
  if (!Number.isFinite(expiresAt) || expiresAt <= now) return false;
  return expiresAt - now <= 75_000;
}

function newBrowserControllerLeaseId() {
  return globalThis.crypto?.randomUUID?.()
    || `browser-lease-${Date.now()}-${Math.random().toString(36).slice(2, 12)}`;
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
  normalizeUrl,
  browserSessionIdFromArgs,
  formatBytes,
  titleCase,
  userSessionPrefix,
  selectedViewport,
  browserAuthRequestFromArgs,
  shouldRenewControllerLease,
  browserCommandRequiresController,
  browserSurfaceIsFocused,
  browserSurfaceCanControl,
  newBrowserControllerLeaseId,
};

async function ensureStyles() {
  const href = new URL(`./index.css?v=${STYLE_BUILD}`, import.meta.url).href;
  if ([...document.querySelectorAll('link[rel="stylesheet"]')].some((link) => link.href === href)) return;
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
