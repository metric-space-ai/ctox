import { loadModuleMessages } from '../../shared/i18n.js';

const STYLE_BUILD = '20260706-kit-tokens1';

// Module-level translator; set from locales/<lang>.json during mount.
let t = (key, fallback) => fallback ?? key;
const DEFAULT_SESSION_ID = 'browser_session_default';
const DEFAULT_TAB_ID = 'browser_tab_default';
const SYNTHETIC_SESSION_ID = 'browser_session_synthetic';
const SYNTHETIC_TAB_ID = 'browser_tab_synthetic';
const VIEWPORT = { width: 1280, height: 720 };
const VIEWER_HEARTBEAT_MS = 5000;
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
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
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
    stop: root.querySelector('[data-browser-stop]'),
    reset: root.querySelector('[data-browser-reset]'),
    back: root.querySelector('[data-browser-back]'),
    forward: root.querySelector('[data-browser-forward]'),
    reload: root.querySelector('[data-browser-reload]'),
    sendToCtox: root.querySelector('[data-browser-send-to-ctox]'),
    seed: root.querySelector('[data-browser-seed]'),
    clear: root.querySelector('[data-browser-clear]'),
    notice: root.querySelector('[data-browser-notice]'),
    form: root.querySelector('[data-browser-address-form]'),
    address: root.querySelector('[data-browser-address]'),
    statusChip: root.querySelector('[data-browser-status-chip]'),
    statusTitle: root.querySelector('[data-browser-status-title]'),
    statusMeta: root.querySelector('[data-browser-status-meta]'),
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

  const state = {
    selectedSessionId: '',
    requestedSessionId: '',
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
    lastViewerHeartbeatAt: 0,
    lastFrameSyncRecoveryAt: 0,
    addressDirty: false,
  };

  const cleanups = [];
  let mounted = true;
  const scheduleRefresh = debounce(safeLoadAndRender, 80);

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
  refs.start?.addEventListener('click', () => {
    const url = refs.address?.value || 'https://example.com';
    state.addressDirty = false;
    state.notice = 'Browser wird mit CTOX verbunden …';
    safeLoadAndRender();
    runBrowserCommand(dispatchBrowserCommand(ctx, state, 'browser.session.start', { url, new_session: true }));
  });
  refs.stop?.addEventListener('click', () => dispatchBrowserCommand(ctx, state, 'browser.session.stop').then(safeLoadAndRender));
  refs.reset?.addEventListener('click', () => dispatchBrowserCommand(ctx, state, 'browser.reset', {
    url: refs.address?.value || 'https://example.com',
  }).then(safeLoadAndRender));
  refs.back?.addEventListener('click', () => dispatchBrowserCommand(ctx, state, 'browser.back').then(safeLoadAndRender));
  refs.forward?.addEventListener('click', () => dispatchBrowserCommand(ctx, state, 'browser.forward').then(safeLoadAndRender));
  refs.reload?.addEventListener('click', () => dispatchBrowserCommand(ctx, state, 'browser.reload').then(safeLoadAndRender));
  refs.address?.addEventListener('input', () => {
    state.addressDirty = true;
  });
  refs.sendToCtox?.addEventListener('click', () => sendBrowserContextToCtox(ctx, state).then(safeLoadAndRender));
  refs.authAssist?.addEventListener('click', (event) => {
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
  refs.seed?.addEventListener('click', () => seedSyntheticFrame(ctx, refs.address?.value || 'https://example.com').then(safeLoadAndRender));
  refs.clear?.addEventListener('click', () => clearSyntheticFrames(ctx).then(safeLoadAndRender));
  refs.sessionList?.addEventListener('click', (event) => {
    const item = event.target?.closest?.('[data-browser-session-id]');
    if (!item) return;
    state.selectedSessionId = item.dataset.browserSessionId || '';
    safeLoadAndRender();
  });
  refs.form?.addEventListener('submit', (event) => {
    event.preventDefault();
    const commandType = state.latestSession?.id ? 'browser.navigate' : 'browser.session.start';
    const url = refs.address?.value || 'https://example.com';
    state.addressDirty = false;
    state.notice = 'Browser wird mit CTOX verbunden …';
    safeLoadAndRender();
    runBrowserCommand(dispatchBrowserCommand(ctx, state, commandType, {
      url,
    }));
  });
  installInputHandlers(ctx, refs, state, scheduleRefresh);
  const viewerHeartbeat = setInterval(() => {
    writeViewerActivity(ctx, state).catch((error) => console.warn('[browser] viewer heartbeat failed', error));
  }, VIEWER_HEARTBEAT_MS);
  cleanups.push(() => clearInterval(viewerHeartbeat));

  safeLoadAndRender();

  return () => {
    mounted = false;
    for (const cleanup of cleanups) {
      try { cleanup(); } catch (error) { console.error('[browser] cleanup failed', error); }
    }
    ctx.host.replaceChildren();
  };

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
        state.notice = browserCommandErrorMessage(error);
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
    const selectedSession = state.selectedSessionId ? latestSession(sessions, state.selectedSessionId) : null;
    const requestedSessionPending = Boolean(state.requestedSessionId && !selectedSession);
    const frameSessionId = selectedSession?.id || state.selectedSessionId || '';
    const frames = await readCollection(browserCollection(ctx, 'browser_frames'), {
      limit: frameSessionId ? 20 : 30,
      selector: frameSessionId ? { session_id: frameSessionId } : {},
    });
    if (!mounted) return;
    const newestFrame = latestFrame(frames);
    state.latestSession = selectedSession || latestSession(sessions, newestFrame?.session_id) || latestSession(sessions);
    if (!requestedSessionPending) {
      state.selectedSessionId = state.latestSession?.id || '';
      if (state.latestSession?.id === state.requestedSessionId) state.requestedSessionId = '';
    }
    state.latestFrame = latestFrame(frames, state.latestSession?.id);
    state.latestTab = latestTab(tabs, state.latestFrame?.tab_id || state.latestSession?.current_tab_id);
    state.latestCommand = latestBrowserCommand(commands, state.latestSession?.id || state.latestFrame?.session_id);
    state.browserCommands = latestBrowserCommands(commands, state.latestSession?.id || state.latestFrame?.session_id, 5);
    state.handoffTasks = latestBrowserHandoffTasks(handoffTasks, state.latestSession?.id || state.latestFrame?.session_id, 5);
    await reconcileCompletedStopCommand(ctx, state, commands);
    state.lastInputSeq = Math.max(
      Number(state.lastInputSeq || 0),
      ...inputs.map((event) => Number(event.seq || 0)).filter(Number.isFinite),
    );
    renderSessionList(refs, sessions, tabs, state.latestSession);
    renderSession(refs, state.latestSession, state.latestTab, state.latestFrame, state.latestCommand, state);
    renderAuthAssist(refs, state.latestSession);
    renderStatus(refs, state.latestSession, state.latestTab, state.latestFrame, state.latestCommand);
    renderDiagnostics(refs, state.latestFrame, inputs, state.latestCommand, state.browserCommands, state.handoffTasks);
    renderControls(refs, state);
    renderNotice(refs, state.notice);
    recoverFrameSyncIfNeeded(ctx, state);
    await renderFrame(refs, state.latestFrame, state);
    writeViewerActivity(ctx, state).catch((error) => console.warn('[browser] viewer activity update failed', error));
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
  const now = Date.now();
  const opensNewSession = payloadPatch.new_session === true;
  const sessionId = opensNewSession
    ? `browser_session_${now}`
    : state.latestSession?.id || DEFAULT_SESSION_ID;
  const tabId = opensNewSession
    ? `browser_tab_${now}`
    : state.latestTab?.id || state.latestSession?.current_tab_id || DEFAULT_TAB_ID;
  const commandId = `browser_cmd_${now}_${Math.random().toString(36).slice(2, 10)}`;
  const payload = {
    session_id: sessionId,
    tab_id: tabId,
    viewport_w: VIEWPORT.width,
    viewport_h: VIEWPORT.height,
    ...payloadPatch,
  };
  delete payload.new_session;
  if (payload.url) payload.url = normalizeUrl(payload.url);
  await startBrowserRuntimeSync(ctx);
  if (commandType === 'browser.session.stop') {
    await writeOptimisticBrowserSession(ctx, state, commandType, payload);
  }
  if (!ctx.commandBus?.dispatch) {
    const error = new Error('CTOX command bus is unavailable.');
    error.code = 'sync_unavailable';
    throw error;
  }
  await ctx.commandBus.dispatch({
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
  }, { until: 'accepted' });
  return { commandId, sessionId, tabId, opensNewSession };
}

function browserCommandErrorMessage(error) {
  const code = String(error?.code || '').toLowerCase();
  if (code === 'auth_required') return 'Die Browser-Anmeldung benötigt eine neue Business-OS-Autorisierung.';
  if (['native_unavailable', 'projection_delayed', 'sync_unavailable'].includes(code)) {
    return 'CTOX ist nicht mit dem Browser-Datenkanal verbunden. Der Browser wurde nicht gestartet. Bitte die Verbindung erneut aufbauen und dann erneut versuchen.';
  }
  const message = String(error?.message || '').trim();
  return message ? `Der Browser konnte nicht gestartet werden: ${message}` : 'Der Browser konnte nicht gestartet werden.';
}

async function writeOptimisticBrowserSession(ctx, state, commandType, payload) {
  if (!['browser.session.start', 'browser.navigate', 'browser.reset', 'browser.session.stop'].includes(commandType)) return;
  const now = Date.now();
  const sessionId = payload.session_id || state.latestSession?.id || DEFAULT_SESSION_ID;
  const tabId = payload.tab_id || state.latestTab?.id || state.latestSession?.current_tab_id || DEFAULT_TAB_ID;
  const url = payload.url || state.latestTab?.url || state.latestSession?.current_url || 'https://example.com';
  const title = state.latestTab?.title || state.latestSession?.title || 'Browser';
  const frameId = state.latestFrame?.id || state.latestSession?.active_frame_id || '';
  const frameSeq = Number(state.latestFrame?.seq || state.latestSession?.last_frame_seq || 0);
  const sessionsCollection = browserCollection(ctx, 'browser_sessions');
  const tabsCollection = browserCollection(ctx, 'browser_tabs');
  const existingSession = (await sessionsCollection?.findOne(sessionId).exec())?.toJSON?.() || {};
  const existingTab = (await tabsCollection?.findOne(tabId).exec())?.toJSON?.() || {};
  const isStop = commandType === 'browser.session.stop';
  const optimisticStatus = isStop ? 'stopped' : 'requested';
  const optimisticRuntimeStatus = isStop ? 'stopped' : 'pending_command';
  await Promise.all([
    upsertDoc(sessionsCollection, {
      ...existingSession,
      id: sessionId,
      owner_user_id: existingSession.owner_user_id || ctx.session?.user?.id || '',
      controller_user_id: ctx.session?.user?.id || existingSession.controller_user_id || '',
      status: optimisticStatus,
      runtime_status: optimisticRuntimeStatus,
      current_tab_id: tabId,
      current_url: url,
      title,
      viewport_w: payload.viewport_w || VIEWPORT.width,
      viewport_h: payload.viewport_h || VIEWPORT.height,
      device_scale_factor: existingSession.device_scale_factor || 1,
      frame_rate_target: existingSession.frame_rate_target || 0,
      active_frame_id: frameId,
      last_frame_seq: frameSeq,
      last_input_seq: existingSession.last_input_seq || 0,
      pending_input_count: existingSession.pending_input_count || 0,
      payload: {
        ...(existingSession.payload || {}),
        browser_stream: 'rxdb',
        last_command_type: commandType,
        last_requested_command: commandType,
      },
      created_at_ms: existingSession.created_at_ms || now,
      updated_at_ms: now,
    }),
    upsertDoc(tabsCollection, {
      ...existingTab,
      id: tabId,
      session_id: sessionId,
      title,
      url,
      status: optimisticStatus,
      loading: !isStop,
      active: true,
      can_go_back: Boolean(existingTab.can_go_back),
      can_go_forward: Boolean(existingTab.can_go_forward),
      frame_seq: frameSeq,
      last_frame_id: frameId,
      last_frame_at_ms: existingTab.last_frame_at_ms || 0,
      payload: {
        ...(existingTab.payload || {}),
        browser_stream: 'rxdb',
        last_command_type: commandType,
        last_requested_command: commandType,
      },
      created_at_ms: existingTab.created_at_ms || now,
      updated_at_ms: now,
    }),
  ]);
}

async function reconcileCompletedStopCommand(ctx, state, commands) {
  const sessionId = state.latestSession?.id || state.latestTab?.session_id || DEFAULT_SESSION_ID;
  const stopCommand = commands
    .filter((command) => {
      const type = command.command_type || command.type || '';
      if (type !== 'browser.session.stop') return false;
      if (String(command.status || '') !== 'completed') return false;
      const payloadSession = command.payload?.session_id;
      return command.record_id === sessionId || payloadSession === sessionId || (!command.record_id && !payloadSession);
    })
    .sort((a, b) => Number(b.created_at_ms || b.updated_at_ms || 0) - Number(a.created_at_ms || a.updated_at_ms || 0))[0];
  if (!stopCommand) return;
  if (state.latestSession?.status === 'stopped' && state.latestTab?.status === 'stopped') return;
  await writeOptimisticBrowserSession(ctx, state, 'browser.session.stop', {
    session_id: sessionId,
    tab_id: state.latestTab?.id || state.latestSession?.current_tab_id || DEFAULT_TAB_ID,
    viewport_w: state.latestSession?.viewport_w || VIEWPORT.width,
    viewport_h: state.latestSession?.viewport_h || VIEWPORT.height,
  });
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
  if (ctx.commandBus?.dispatch) {
    await ctx.commandBus.dispatch({
      id: commandId,
      module: 'ctox',
      type: 'ctox.browser_context.capture',
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
  } else {
    await upsertDoc(browserCollection(ctx, 'business_commands'), {
      id: commandId,
      command_id: commandId,
      module: 'ctox',
      command_type: 'ctox.browser_context.capture',
      type: 'ctox.browser_context.capture',
      record_id: session.id,
      inbound_channel: 'browser',
      status: 'pending_sync',
      payload,
      client_context: {
        source: options.webStack ? 'business-os.browser.web-stack-capture' : 'business-os.browser.context',
        module_id: 'browser',
        actor: actorContext(ctx.session),
        browser_context: browserContext,
      },
      created_at_ms: now,
      updated_at_ms: now,
    });
  }
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
    type: 'browser.capture.extract',
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
  if (ctx.commandBus?.dispatch) {
    await ctx.commandBus.dispatch(command);
  } else {
    await upsertDoc(browserCollection(ctx, 'business_commands'), {
      ...command,
      status: 'pending_sync',
    });
  }
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
  if (ctx.commandBus?.dispatch) {
    await ctx.commandBus.dispatch({
      id: commandId,
      module: 'browser',
      type: 'web_stack.auth_assist.complete',
      record_id: session.id,
      inbound_channel: 'browser',
      payload,
      client_context: {
        source: 'business-os.browser.auth-assist',
        module_id: 'browser',
        actor: actorContext(ctx.session),
      },
    });
  } else {
    await upsertDoc(browserCollection(ctx, 'business_commands'), {
      id: commandId,
      command_id: commandId,
      module: 'browser',
      command_type: 'web_stack.auth_assist.complete',
      type: 'web_stack.auth_assist.complete',
      record_id: session.id,
      inbound_channel: 'browser',
      status: 'pending_sync',
      payload,
      client_context: {
        source: 'business-os.browser.auth-assist',
        module_id: 'browser',
        actor: actorContext(ctx.session),
      },
      created_at_ms: now,
      updated_at_ms: now,
    });
  }
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
    browser_stream: 'rxdb',
    secret_value_in_rxdb: false,
  };
  const selector = String(session.payload?.credential_selector || session.payload?.selector || '').trim();
  if (selector) payload.selector = selector;
  await startCommandSync(ctx);
  if (ctx.commandBus?.dispatch) {
    await ctx.commandBus.dispatch({
      id: commandId,
      module: 'browser',
      type: 'browser.credential.fill',
      record_id: session.id,
      inbound_channel: 'browser',
      payload,
      client_context: {
        source: 'business-os.browser.credential-fill',
        module_id: 'browser',
        actor: actorContext(ctx.session),
      },
    });
  } else {
    await upsertDoc(browserCollection(ctx, 'business_commands'), {
      id: commandId,
      command_id: commandId,
      module: 'browser',
      command_type: 'browser.credential.fill',
      type: 'browser.credential.fill',
      record_id: session.id,
      inbound_channel: 'browser',
      status: 'pending_sync',
      payload,
      client_context: {
        source: 'business-os.browser.credential-fill',
        module_id: 'browser',
        actor: actorContext(ctx.session),
      },
      created_at_ms: now,
      updated_at_ms: now,
    });
  }
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
    if (event.metaKey || event.ctrlKey) return;
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
    text: type === 'keyDown' && event.key?.length === 1 ? event.key : '',
    payload: {
      repeat: Boolean(event.repeat),
      location: Number(event.location || 0),
      actor: actorContext(ctx.session),
    },
  });
}

async function writeInputEvent(ctx, state, type, patch) {
  const session = state.latestSession;
  const frame = state.latestFrame;
  const sessionId = session?.id || frame?.session_id;
  if (!sessionId) return;
  const now = Date.now();
  const seq = Math.max(now, Number(state.lastInputSeq || 0) + 1);
  state.lastInputSeq = seq;
  const event = {
    id: `${sessionId}:input:${seq}:${type}`,
    session_id: sessionId,
    tab_id: state.latestTab?.id || frame?.tab_id || '',
    seq,
    type,
    status: 'pending',
    created_at_ms: now,
    updated_at_ms: now,
    ...patch,
  };
  await upsertDoc(browserCollection(ctx, 'browser_input_events'), event);
  if (session) {
    const pendingInputCount = await countPendingInputEvents(ctx, session.id);
    await patchDoc(browserCollection(ctx, 'browser_sessions'), session.id, {
      last_input_seq: Math.max(seq, Number(session.last_input_seq || 0)),
      pending_input_count: pendingInputCount,
      updated_at_ms: now,
    });
    await writeViewerActivity(ctx, state, { force: true, atMs: now });
  }
}

async function countPendingInputEvents(ctx, sessionId) {
  const collection = browserCollection(ctx, 'browser_input_events');
  if (!collection?.find || !sessionId) return 0;
  const docs = await collection.find().exec();
  return (docs || [])
    .map((doc) => doc?.toJSON?.() || doc)
    .filter((event) => event?.session_id === sessionId && event?.status === 'pending')
    .length;
}

async function writeViewerActivity(ctx, state, options = {}) {
  return;
  const session = state.latestSession;
  if (!session?.id || session.id === SYNTHETIC_SESSION_ID) return;
  if (session.status !== 'active' && session.runtime_status !== 'active') return;
  const now = Number(options.atMs || Date.now());
  if (!options.force && now - Number(state.lastViewerHeartbeatAt || 0) < VIEWER_HEARTBEAT_MS) return;
  state.lastViewerHeartbeatAt = now;
  const collection = browserCollection(ctx, 'browser_sessions');
  if (!collection?.findOne) return;
  const existing = await collection.findOne(session.id).exec();
  if (!existing?.incrementalPatch) return;
  const current = existing.toJSON?.() || session;
  const payload = {
    ...(current.payload || {}),
    viewer_source: 'business-os.browser',
    viewer_user_id: ctx.session?.user?.id || ctx.session?.userId || '',
    last_viewer_at_ms: now,
  };
  await existing.incrementalPatch({
    payload,
    updated_at_ms: Math.max(now, Number(current.updated_at_ms || 0)),
  });
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
  refs.sessionList.innerHTML = sorted.slice(0, 8).map((session) => {
    const tab = latestTab(tabs.filter((candidate) => candidate.session_id === session.id), session.current_tab_id);
    const url = tab?.url || session.current_url || 'about:blank';
    const status = browserStatusLabel(session);
    const uiState = browserUiState(session);
    return `
      <button type="button" class="browser-session-item" data-browser-session-id="${escapeHtml(session.id)}" aria-current="${session.id === activeSession?.id ? 'true' : 'false'}">
        <strong>${escapeHtml(browserDisplayTitle(tab, session, url))}</strong>
        <span class="browser-pill" data-state="${escapeHtml(uiState)}">${escapeHtml(status)}</span>
        <span>${escapeHtml(url)}</span>
        <span>${escapeHtml(formatTime(session.updated_at_ms))}</span>
      </button>
    `;
  }).join('');
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
  const status = browserStatusLabel(session);
  if (refs.address && url && !state?.addressDirty && document.activeElement !== refs.address) refs.address.value = url;
  refs.sessionCard.innerHTML = `
    <strong>${escapeHtml(browserDisplayTitle(tab, session, url))}</strong>
    <div class="browser-muted">${escapeHtml(url || 'about:blank')}</div>
    <div class="browser-meta-grid">
      <span>Status</span><span>${escapeHtml(status)}</span>
      <span>Owner</span><span>${escapeHtml(owner)}</span>
      <span>Control</span><span>${escapeHtml(controller)}</span>
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

function renderControls(refs, state) {
  const hasSession = Boolean(state.latestSession?.id);
  const isStopped = ['stopped', 'closed'].includes(String(state.latestSession?.status || state.latestSession?.runtime_status || '').toLowerCase());
  for (const button of [refs.stop, refs.reset, refs.reload, refs.back, refs.forward, refs.sendToCtox]) {
    if (!button) continue;
    button.disabled = !hasSession || isStopped;
  }
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

async function seedSyntheticFrame(ctx, requestedUrl) {
  const now = Date.now();
  const url = normalizeUrl(requestedUrl);
  const seq = now;
  const image = await createSyntheticFrame(url, seq);
  const session = {
    id: SYNTHETIC_SESSION_ID,
    owner_user_id: ctx.session?.user?.id || '',
    controller_user_id: ctx.session?.user?.id || '',
    status: 'synthetic',
    runtime_status: 'not_started',
    current_tab_id: SYNTHETIC_TAB_ID,
    current_url: url,
    title: 'Browser Vorschau',
    viewport_w: VIEWPORT.width,
    viewport_h: VIEWPORT.height,
    device_scale_factor: 1,
    frame_rate_target: 0,
    active_frame_id: `browser_frame_synthetic_${seq}`,
    last_frame_seq: seq,
    last_input_seq: 0,
    pending_input_count: 0,
    payload: { source: 'business-os.browser.synthetic' },
    created_at_ms: now,
    updated_at_ms: now,
  };
  const tab = {
    id: SYNTHETIC_TAB_ID,
    session_id: SYNTHETIC_SESSION_ID,
    title: 'Browser Vorschau',
    url,
    status: 'synthetic',
    loading: false,
    active: true,
    can_go_back: false,
    can_go_forward: false,
    frame_seq: seq,
    last_frame_id: session.active_frame_id,
    last_frame_at_ms: now,
    payload: { source: 'business-os.browser.synthetic' },
    created_at_ms: now,
    updated_at_ms: now,
  };
  const frame = {
    id: session.active_frame_id,
    session_id: SYNTHETIC_SESSION_ID,
    tab_id: SYNTHETIC_TAB_ID,
    seq,
    mime_type: image.mimeType,
    encoding: 'base64',
    data: image.base64,
    width: VIEWPORT.width,
    height: VIEWPORT.height,
    viewport_w: VIEWPORT.width,
    viewport_h: VIEWPORT.height,
    quality: 100,
    size_bytes: image.sizeBytes,
    frame_hash: await sha256Hex(image.base64),
    captured_at_ms: now,
    expires_at_ms: now + 5 * 60 * 1000,
    updated_at_ms: now,
  };
  await Promise.all([
    upsertDoc(browserCollection(ctx, 'browser_sessions'), session),
    upsertDoc(browserCollection(ctx, 'browser_tabs'), tab),
    upsertDoc(browserCollection(ctx, 'browser_frames'), frame),
  ]);
}

async function clearSyntheticFrames(ctx) {
  const docs = await browserCollection(ctx, 'browser_frames')?.find().exec();
  for (const doc of docs || []) {
    const json = doc?.toJSON?.() || {};
    if (json.session_id === SYNTHETIC_SESSION_ID) {
      await doc.remove();
    }
  }
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

async function createSyntheticFrame(url, seq) {
  const canvas = document.createElement('canvas');
  canvas.width = VIEWPORT.width;
  canvas.height = VIEWPORT.height;
  const ctx = canvas.getContext('2d');
  const gradient = ctx.createLinearGradient(0, 0, canvas.width, canvas.height);
  gradient.addColorStop(0, '#082f49');
  gradient.addColorStop(0.55, '#0f172a');
  gradient.addColorStop(1, '#14532d');
  ctx.fillStyle = gradient;
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.strokeStyle = 'rgba(255,255,255,0.12)';
  for (let x = 0; x <= canvas.width; x += 80) {
    ctx.beginPath();
    ctx.moveTo(x, 0);
    ctx.lineTo(x, canvas.height);
    ctx.stroke();
  }
  for (let y = 0; y <= canvas.height; y += 80) {
    ctx.beginPath();
    ctx.moveTo(0, y);
    ctx.lineTo(canvas.width, y);
    ctx.stroke();
  }
  ctx.fillStyle = 'rgba(255,255,255,0.92)';
  ctx.font = '700 52px system-ui, -apple-system, BlinkMacSystemFont, sans-serif';
  ctx.fillText('CTOX Browser', 72, 140);
  ctx.font = '26px system-ui, -apple-system, BlinkMacSystemFont, sans-serif';
  ctx.fillStyle = 'rgba(255,255,255,0.74)';
  ctx.fillText(url, 72, 196);
  ctx.font = '20px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace';
  ctx.fillText(`Vorschau ${seq}`, 72, 254);
  ctx.strokeStyle = 'rgba(34,197,94,0.75)';
  ctx.lineWidth = 4;
  ctx.strokeRect(72, 314, 1136, 260);
  ctx.fillStyle = 'rgba(14,165,233,0.22)';
  ctx.fillRect(92, 334, 1096, 220);
  const dataUrl = canvas.toDataURL('image/png');
  const base64 = dataUrl.split(',')[1] || '';
  return {
    base64,
    mimeType: 'image/png',
    sizeBytes: Math.ceil((base64.length * 3) / 4),
  };
}

async function sha256Hex(text) {
  if (!globalThis.crypto?.subtle) return '';
  const bytes = new TextEncoder().encode(text);
  const hash = await globalThis.crypto.subtle.digest('SHA-256', bytes);
  return [...new Uint8Array(hash)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

function normalizeUrl(value) {
  const trimmed = String(value || '').trim();
  if (!trimmed) return 'https://example.com';
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(trimmed)) return trimmed;
  return `https://${trimmed}`;
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
