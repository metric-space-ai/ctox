const STYLE_BUILD = '20260525-rxdb-browser1';
const DEFAULT_SESSION_ID = 'browser_session_default';
const DEFAULT_TAB_ID = 'browser_tab_default';
const SYNTHETIC_SESSION_ID = 'browser_session_synthetic';
const SYNTHETIC_TAB_ID = 'browser_tab_synthetic';
const VIEWPORT = { width: 1280, height: 720 };
const VIEWER_HEARTBEAT_MS = 5000;

export async function mount(ctx) {
  await ensureStyles();
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  ctx.host.innerHTML = html;

  const root = ctx.host.querySelector('[data-browser-root]');
  if (!root) throw new Error('browser: root element missing after fragment mount');

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
  };

  const cleanups = [];
  const scheduleRefresh = debounce(loadAndRender, 80);

  for (const collectionName of ['business_commands', 'browser_sessions', 'browser_tabs', 'browser_frames', 'browser_input_events', 'ctox_queue_tasks']) {
    try {
      await ctx.sync?.startCollection?.(collectionName);
    } catch (error) {
      console.warn(`[browser] ${collectionName} sync start failed`, error);
    }
  }

  for (const collection of [
    ctx.db?.raw?.business_commands,
    ctx.db?.raw?.browser_sessions,
    ctx.db?.raw?.browser_tabs,
    ctx.db?.raw?.browser_frames,
    ctx.db?.raw?.browser_input_events,
    ctx.db?.raw?.ctox_queue_tasks,
  ]) {
    const sub = collection?.$?.subscribe?.(() => scheduleRefresh());
    if (sub?.unsubscribe) cleanups.push(() => sub.unsubscribe());
  }

  refs.refresh?.addEventListener('click', loadAndRender);
  refs.start?.addEventListener('click', () => dispatchBrowserCommand(ctx, state, 'browser.session.start', {
    url: refs.address?.value || 'https://example.com',
  }).then(loadAndRender));
  refs.stop?.addEventListener('click', () => dispatchBrowserCommand(ctx, state, 'browser.session.stop').then(loadAndRender));
  refs.reset?.addEventListener('click', () => dispatchBrowserCommand(ctx, state, 'browser.reset', {
    url: refs.address?.value || 'https://example.com',
  }).then(loadAndRender));
  refs.back?.addEventListener('click', () => dispatchBrowserCommand(ctx, state, 'browser.back').then(loadAndRender));
  refs.forward?.addEventListener('click', () => dispatchBrowserCommand(ctx, state, 'browser.forward').then(loadAndRender));
  refs.reload?.addEventListener('click', () => dispatchBrowserCommand(ctx, state, 'browser.reload').then(loadAndRender));
  refs.sendToCtox?.addEventListener('click', () => sendBrowserContextToCtox(ctx, state).then(loadAndRender));
  refs.authAssist?.addEventListener('click', (event) => {
    const fillButton = event.target?.closest?.('[data-browser-credential-fill]');
    if (fillButton) {
      fillWebStackCredential(ctx, state).then(loadAndRender);
      return;
    }
    const completeButton = event.target?.closest?.('[data-browser-auth-complete]');
    if (completeButton) {
      completeWebStackAuthAssist(ctx, state).then(loadAndRender);
      return;
    }
    const captureButton = event.target?.closest?.('[data-browser-web-stack-capture]');
    if (captureButton) sendBrowserContextToCtox(ctx, state, { webStack: true }).then(loadAndRender);
    const extractButton = event.target?.closest?.('[data-browser-web-stack-extract]');
    if (extractButton) extractWebStackFields(ctx, state).then(loadAndRender);
  });
  refs.seed?.addEventListener('click', () => seedSyntheticFrame(ctx, refs.address?.value || 'https://example.com').then(loadAndRender));
  refs.clear?.addEventListener('click', () => clearSyntheticFrames(ctx).then(loadAndRender));
  refs.sessionList?.addEventListener('click', (event) => {
    const item = event.target?.closest?.('[data-browser-session-id]');
    if (!item) return;
    state.selectedSessionId = item.dataset.browserSessionId || '';
    loadAndRender();
  });
  refs.form?.addEventListener('submit', (event) => {
    event.preventDefault();
    const commandType = state.latestSession?.id ? 'browser.navigate' : 'browser.session.start';
    dispatchBrowserCommand(ctx, state, commandType, {
      url: refs.address?.value || 'https://example.com',
    }).then(loadAndRender);
  });
  installInputHandlers(ctx, refs, state, scheduleRefresh);
  const viewerHeartbeat = setInterval(() => {
    writeViewerActivity(ctx, state).catch((error) => console.warn('[browser] viewer heartbeat failed', error));
  }, VIEWER_HEARTBEAT_MS);
  cleanups.push(() => clearInterval(viewerHeartbeat));

  await loadAndRender();

  return () => {
    for (const cleanup of cleanups) {
      try { cleanup(); } catch (error) { console.error('[browser] cleanup failed', error); }
    }
    ctx.host.replaceChildren();
  };

  async function loadAndRender() {
    const [commands, sessions, tabs, frames, inputs, handoffTasks] = await Promise.all([
      readCollection(ctx.db?.raw?.business_commands),
      readCollection(ctx.db?.raw?.browser_sessions),
      readCollection(ctx.db?.raw?.browser_tabs),
      readCollection(ctx.db?.raw?.browser_frames),
      readCollection(ctx.db?.raw?.browser_input_events),
      readCollection(ctx.db?.raw?.ctox_queue_tasks),
    ]);
    const selectedSession = state.selectedSessionId ? latestSession(sessions, state.selectedSessionId) : null;
    const newestFrame = latestFrame(frames);
    state.latestSession = selectedSession || latestSession(sessions, newestFrame?.session_id) || latestSession(sessions);
    state.selectedSessionId = state.latestSession?.id || '';
    state.latestFrame = latestFrame(frames, state.latestSession?.id);
    state.latestTab = latestTab(tabs, state.latestFrame?.tab_id || state.latestSession?.current_tab_id);
    state.latestCommand = latestBrowserCommand(commands, state.latestSession?.id || state.latestFrame?.session_id);
    state.browserCommands = latestBrowserCommands(commands, state.latestSession?.id || state.latestFrame?.session_id, 5);
    state.handoffTasks = latestBrowserHandoffTasks(handoffTasks, state.latestSession?.id || state.latestFrame?.session_id, 5);
    state.lastInputSeq = Math.max(
      Number(state.lastInputSeq || 0),
      ...inputs.map((event) => Number(event.seq || 0)).filter(Number.isFinite),
    );
    renderSessionList(refs, sessions, tabs, state.latestSession);
    renderSession(refs, state.latestSession, state.latestTab, state.latestFrame, state.latestCommand);
    renderAuthAssist(refs, state.latestSession);
    renderStatus(refs, state.latestSession, state.latestTab, state.latestFrame, state.latestCommand);
    renderDiagnostics(refs, state.latestFrame, inputs, state.latestCommand, state.browserCommands, state.handoffTasks);
    renderControls(refs, state);
    renderNotice(refs, state.notice);
    await renderFrame(refs, state.latestFrame, state);
    writeViewerActivity(ctx, state).catch((error) => console.warn('[browser] viewer activity update failed', error));
  }
}

async function dispatchBrowserCommand(ctx, state, commandType, payloadPatch = {}) {
  const now = Date.now();
  const sessionId = state.latestSession?.id || DEFAULT_SESSION_ID;
  const tabId = state.latestTab?.id || state.latestSession?.current_tab_id || DEFAULT_TAB_ID;
  const commandId = `browser_cmd_${now}_${Math.random().toString(36).slice(2, 10)}`;
  const payload = {
    session_id: sessionId,
    tab_id: tabId,
    viewport_w: VIEWPORT.width,
    viewport_h: VIEWPORT.height,
    ...payloadPatch,
  };
  if (payload.url) payload.url = normalizeUrl(payload.url);
  await startCommandSync(ctx);
  if (ctx.commandBus?.dispatch) {
    await ctx.commandBus.dispatch({
      id: commandId,
      module: 'browser',
      type: commandType,
      record_id: sessionId,
      inbound_channel: 'browser',
      payload,
      client_context: {
        source: 'business-os.browser',
        module_id: 'browser',
        actor: actorContext(ctx.session),
      },
    });
  } else {
    await upsertDoc(ctx.db?.raw?.business_commands, {
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
        source: 'business-os.browser',
        module_id: 'browser',
        actor: actorContext(ctx.session),
      },
      created_at_ms: now,
      updated_at_ms: now,
    });
  }
}

async function sendBrowserContextToCtox(ctx, state, options = {}) {
  const session = state.latestSession;
  const tab = state.latestTab;
  const frame = state.latestFrame;
  if (!session?.id) {
    state.notice = 'No browser session to send.';
    return;
  }
  const payloadMetadata = session.payload || {};
  const now = Date.now();
  const url = tab?.url || session.current_url || '';
  const title = tab?.title || session.title || 'Remote Browser';
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
    title: options.webStack && sourceId ? `Web Stack Browser: ${sourceId}` : `Remote Browser: ${title}`,
    instruction: url
      ? `Use this Remote Browser context from ${url}. The screenshot is available as the referenced browser_frames record.`
      : 'Use this Remote Browser context. The screenshot is available as the referenced browser_frames record.',
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
    await upsertDoc(ctx.db?.raw?.business_commands, {
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
    ? `Sent browser context with frame ${frame.seq || frame.id} to ${target}.`
    : `Sent browser context to ${target} without a current frame.`;
}

async function extractWebStackFields(ctx, state) {
  const session = state.latestSession;
  const tab = state.latestTab;
  const frame = state.latestFrame;
  const payload = session?.payload || {};
  if (!session?.id || !payload.capture_script) {
    state.notice = 'No Web Stack capture script available.';
    return;
  }
  const now = Date.now();
  const browserContext = {
    session_id: session.id,
    tab_id: tab?.id || session.current_tab_id || '',
    url: tab?.url || session.current_url || '',
    title: tab?.title || session.title || 'Remote Browser',
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
    await upsertDoc(ctx.db?.raw?.business_commands, {
      ...command,
      status: 'pending_sync',
    });
  }
  state.notice = `Requested Web Stack extract via ${payload.capture_script}.`;
}

async function completeWebStackAuthAssist(ctx, state) {
  const session = state.latestSession;
  if (!session?.id) {
    state.notice = 'No Web Stack browser session to complete.';
    return;
  }
  if (session.payload?.purpose !== 'web_stack_auth') {
    state.notice = 'This browser session is not a Web Stack login session.';
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
    await upsertDoc(ctx.db?.raw?.business_commands, {
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
  state.notice = 'Web Stack login completion queued.';
}

async function fillWebStackCredential(ctx, state) {
  const session = state.latestSession;
  if (!session?.id) {
    state.notice = 'No Web Stack browser session to fill.';
    return;
  }
  const secretName = session.payload?.secret_name || '';
  if (!secretName) {
    state.notice = 'This Web Stack session has no credential reference.';
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
    await upsertDoc(ctx.db?.raw?.business_commands, {
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
    ? 'Credential fill queued for the source-specific browser field.'
    : 'Credential fill queued for the focused browser field.';
}

async function startCommandSync(ctx) {
  try {
    await ctx.sync?.startCollection?.('business_commands');
  } catch (error) {
    console.warn('[browser] business_commands sync start failed', error);
  }
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
  await upsertDoc(ctx.db?.raw?.browser_input_events, event);
  if (session) {
    await patchDoc(ctx.db?.raw?.browser_sessions, session.id, {
      last_input_seq: seq,
      pending_input_count: Number(session.pending_input_count || 0) + 1,
      updated_at_ms: now,
    });
    await writeViewerActivity(ctx, state, { force: true, atMs: now });
  }
}

async function writeViewerActivity(ctx, state, options = {}) {
  const session = state.latestSession;
  if (!session?.id || session.id === SYNTHETIC_SESSION_ID) return;
  const now = Number(options.atMs || Date.now());
  if (!options.force && now - Number(state.lastViewerHeartbeatAt || 0) < VIEWER_HEARTBEAT_MS) return;
  state.lastViewerHeartbeatAt = now;
  const collection = ctx.db?.raw?.browser_sessions;
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

async function readCollection(collection) {
  if (!collection?.find) return [];
  const docs = await collection.find().exec();
  return docs
    .map((doc) => doc?.toJSON?.() || doc)
    .filter((doc) => doc && doc._deleted !== true);
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
    const status = session.runtime_status || session.status || 'unknown';
    return `
      <button type="button" class="browser-session-item" data-browser-session-id="${escapeHtml(session.id)}" aria-current="${session.id === activeSession?.id ? 'true' : 'false'}">
        <strong>${escapeHtml(tab?.title || session.title || 'Remote Browser')}</strong>
        <span class="browser-pill">${escapeHtml(status)}</span>
        <span>${escapeHtml(url)}</span>
        <span>${escapeHtml(formatTime(session.updated_at_ms))}</span>
      </button>
    `;
  }).join('');
}

function renderSession(refs, session, tab, frame, command) {
  if (!session) {
    refs.sessionCard.innerHTML = '<span class="browser-muted">No session</span>';
    return;
  }
  const url = tab?.url || session.current_url || '';
  const commandError = commandErrorMessage(command);
  const sessionError = session.error ? `<div class="browser-error">${escapeHtml(session.error)}</div>` : '';
  const commandLine = command
    ? `<div class="browser-muted">Command ${escapeHtml(command.command_type || command.type || 'browser')} - ${escapeHtml(command.status || 'pending')}</div>`
    : '';
  const policy = session.payload?.control_policy || {};
  const owner = session.owner_user_id || policy.owner_user_id || '-';
  const controller = session.controller_user_id || policy.controller_user_id || '-';
  if (refs.address && url) refs.address.value = url;
  refs.sessionCard.innerHTML = `
    <strong>${escapeHtml(tab?.title || session.title || 'Remote Browser')}</strong>
    <div class="browser-muted">${escapeHtml(url || 'about:blank')}</div>
    <div class="browser-meta-grid">
      <span>Status</span><span>${escapeHtml(session.runtime_status || session.status || 'unknown')}</span>
      <span>Owner</span><span>${escapeHtml(owner)}</span>
      <span>Control</span><span>${escapeHtml(controller)}</span>
      <span>Frame</span><span>${escapeHtml(frame ? `${frame.width}x${frame.height}` : 'No frame')}</span>
    </div>
    ${commandLine}
    ${sessionError}
    ${commandError ? `<div class="browser-error">${escapeHtml(commandError)}</div>` : ''}
  `;
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
      <span class="browser-kicker">Web Stack</span>
      <strong>${escapeHtml(payload.source_id || 'Auth Assist')}</strong>
      <small>${escapeHtml(domains || payload.target_url || '')}</small>
      ${payload.capture_script ? `<small>Capture: ${escapeHtml(payload.capture_script)}</small>` : ''}
      ${fillStatus ? `<small>Credential fill: ${escapeHtml(fillStatus)}</small>` : ''}
      ${extractStatus ? `<small>Extract: ${escapeHtml(extractStatus)}</small>` : ''}
    </div>
    <div class="browser-auth-actions">
      <button type="button" data-browser-credential-fill ${canFill ? '' : 'disabled'}>
        Fill credential
      </button>
      <button type="button" data-browser-auth-complete ${completed ? 'disabled' : ''}>
        ${completed ? 'Login saved' : 'Login done'}
      </button>
      <button type="button" data-browser-web-stack-capture ${canCapture ? '' : 'disabled'}>
        Capture for Web Stack
      </button>
      <button type="button" data-browser-web-stack-extract ${canExtract ? '' : 'disabled'}>
        Extract fields
      </button>
    </div>
  `;
}

function renderStatus(refs, session, tab, frame, command) {
  const state = String(session?.runtime_status || session?.status || 'offline').toLowerCase();
  if (refs.statusChip) {
    refs.statusChip.textContent = session ? titleCase(state) : 'Offline';
    refs.statusChip.dataset.state = state;
  }
  const url = tab?.url || session?.current_url || '';
  if (refs.statusTitle) {
    refs.statusTitle.textContent = tab?.title || session?.title || url || 'No remote browser session';
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
    refs.empty.textContent = 'Frame error';
  } finally {
    state.drawing = false;
  }
}

function frameEmptyText(state) {
  const session = state.latestSession;
  const commandError = commandErrorMessage(state.latestCommand);
  if (commandError) return commandError;
  if (!session) return 'Start a remote browser session';
  const status = String(session.runtime_status || session.status || '').toLowerCase();
  if (status === 'failed' || status === 'error') return session.error || 'Remote browser failed';
  if (status === 'stopped' || status === 'closed') return 'Remote browser stopped';
  if (status === 'requested' || status === 'starting') return 'Waiting for CTOX to open Chromium';
  return 'Waiting for the next RxDB frame';
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
  if (status === 'failed' || error) return String(error || 'Command failed');
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
    title: 'Synthetic Remote Browser',
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
    title: 'Synthetic Remote Browser',
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
    upsertDoc(ctx.db?.raw?.browser_sessions, session),
    upsertDoc(ctx.db?.raw?.browser_tabs, tab),
    upsertDoc(ctx.db?.raw?.browser_frames, frame),
  ]);
}

async function clearSyntheticFrames(ctx) {
  const docs = await ctx.db?.raw?.browser_frames?.find().exec();
  for (const doc of docs || []) {
    const json = doc?.toJSON?.() || {};
    if (json.session_id === SYNTHETIC_SESSION_ID) {
      await doc.remove();
    }
  }
}

async function upsertDoc(collection, doc) {
  if (!collection) throw new Error('Browser collection is not registered');
  if (typeof collection.upsert === 'function') {
    await collection.upsert(doc);
    return;
  }
  const existing = await collection.findOne(doc.id).exec();
  if (existing) {
    await existing.incrementalPatch(doc);
  } else {
    await collection.insert(doc);
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
  ctx.fillText('CTOX Remote Browser', 72, 140);
  ctx.font = '26px system-ui, -apple-system, BlinkMacSystemFont, sans-serif';
  ctx.fillStyle = 'rgba(255,255,255,0.74)';
  ctx.fillText(url, 72, 196);
  ctx.font = '20px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace';
  ctx.fillText(`RxDB frame seq ${seq}`, 72, 254);
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
