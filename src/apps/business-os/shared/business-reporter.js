const REPORTER_STYLE_ID = 'ctox-business-reporter-style';

let reporterState = null;
let fabButton = null;
let bugActor = null;

let eggState = {
  state: 'sleeping', // 'sleeping' | 'awakening' | 'crawling' | 'scurrying'
  x: 0,
  y: 0,
  angle: 0,
  animationFrameId: null,
  currentTarget: null,
  pauseUntil: 0,
  wakeUpStartTime: 0,
  scurryStartTime: 0,
  scurryStartPos: null,
  scurryStartAngle: 0,
  lastTime: 0,
};
let idleTimeout = null;
const IDLE_TIME = 300000; // 5 minutes of inactivity

function interpolateAngle(current, target, step) {
  let diff = (target - current) % 360;
  if (diff < -180) diff += 360;
  if (diff > 180) diff -= 360;
  return current + diff * step;
}

function getNextTarget() {
  const margin = 60;
  const tx = margin + Math.random() * (window.innerWidth - 2 * margin - 44);
  const ty = margin + Math.random() * (window.innerHeight - 2 * margin - 44);
  return { x: tx, y: ty };
}

function startEasterEgg() {
  if (!fabButton) return;
  if (window.innerWidth < 1024) {
    idleTimeout = setTimeout(startEasterEgg, IDLE_TIME);
    return;
  }
  if (reporterState && (reporterState.modal || reporterState.markupMode !== 'idle')) {
    idleTimeout = setTimeout(startEasterEgg, IDLE_TIME);
    return;
  }

  if (!bugActor) {
    bugActor = document.createElement('div');
    bugActor.className = 'ctox-bug-actor';
    bugActor.innerHTML = bugIconSvg();
    bugActor.addEventListener('click', () => {
      openReporterDialog(reporterState);
      stopEasterEggInstantly();
    });
    document.body.append(bugActor);
  }

  const rect = fabButton.getBoundingClientRect();
  eggState.state = 'awakening';
  eggState.x = rect.left;
  eggState.y = rect.top;
  eggState.angle = 0;
  eggState.wakeUpStartTime = performance.now();
  eggState.lastTime = performance.now();

  fabButton.classList.add('bug-crawled-away');
  const innerSvg = fabButton.querySelector('svg');
  if (innerSvg) {
    innerSvg.style.opacity = '0';
    innerSvg.style.visibility = 'hidden';
    innerSvg.style.display = 'none';
  }

  bugActor.style.display = 'inline-flex';
  bugActor.style.left = `${eggState.x}px`;
  bugActor.style.top = `${eggState.y}px`;
  bugActor.style.transform = 'rotate(0deg)';

  if (eggState.animationFrameId) {
    cancelAnimationFrame(eggState.animationFrameId);
  }
  eggState.animationFrameId = requestAnimationFrame(animLoop);
}

function scurryBack() {
  if (eggState.state === 'scurrying' || eggState.state === 'sleeping') return;
  eggState.state = 'scurrying';
  eggState.scurryStartTime = performance.now();
  eggState.scurryStartPos = { x: eggState.x, y: eggState.y };
  eggState.scurryStartAngle = eggState.angle;
  eggState.lastTime = performance.now();
}

function stopEasterEggInstantly() {
  if (eggState.animationFrameId) {
    cancelAnimationFrame(eggState.animationFrameId);
    eggState.animationFrameId = null;
  }
  if (idleTimeout) {
    clearTimeout(idleTimeout);
  }

  eggState.state = 'sleeping';

  if (fabButton) {
    fabButton.classList.remove('bug-crawled-away');
    const innerSvg = fabButton.querySelector('svg');
    if (innerSvg) {
      innerSvg.style.opacity = '';
      innerSvg.style.visibility = '';
      innerSvg.style.display = '';
    }
  }

  if (bugActor) {
    bugActor.style.display = 'none';
    bugActor.style.left = '';
    bugActor.style.top = '';
    bugActor.style.transform = '';
  }

  eggState.currentTarget = null;
  eggState.angle = 0;
  eggState.lastTime = 0;

  idleTimeout = setTimeout(startEasterEgg, IDLE_TIME);
}

function animLoop(timestamp) {
  if (eggState.state === 'sleeping' || !fabButton || !bugActor) return;

  if (eggState.state === 'awakening') {
    const elapsed = timestamp - eggState.wakeUpStartTime;
    if (elapsed < 600) {
      const wiggleAngle = Math.sin(timestamp * 0.05) * 15;
      bugActor.style.transform = `rotate(${wiggleAngle}deg)`;
      eggState.animationFrameId = requestAnimationFrame(animLoop);
      return;
    } else {
      eggState.state = 'crawling';
      eggState.currentTarget = getNextTarget();
      eggState.pauseUntil = 0;
      eggState.lastTime = timestamp;
    }
  }

  if (eggState.state === 'crawling') {
    if (timestamp < eggState.pauseUntil) {
      const lookAngle = eggState.angle + Math.sin(timestamp * 0.01) * 8;
      bugActor.style.transform = `rotate(${lookAngle}deg)`;
      eggState.animationFrameId = requestAnimationFrame(animLoop);
      return;
    }

    const target = eggState.currentTarget;
    if (!target) {
      eggState.currentTarget = getNextTarget();
      eggState.animationFrameId = requestAnimationFrame(animLoop);
      return;
    }

    const dx = target.x - eggState.x;
    const dy = target.y - eggState.y;
    const distance = Math.hypot(dx, dy);

    if (distance < 10) {
      eggState.x = target.x;
      eggState.y = target.y;
      eggState.pauseUntil = timestamp + 1000 + Math.random() * 1500;
      eggState.currentTarget = getNextTarget();
      eggState.lastTime = timestamp;
    } else {
      const speed = 75; // px per second
      if (!eggState.lastTime) eggState.lastTime = timestamp;
      const dt = (timestamp - eggState.lastTime) / 1000;
      eggState.lastTime = timestamp;

      const step = speed * Math.min(dt, 0.1);
      const moveX = (dx / distance) * step;
      const moveY = (dy / distance) * step;

      eggState.x += moveX;
      eggState.y += moveY;

      bugActor.style.left = `${eggState.x}px`;
      bugActor.style.top = `${eggState.y}px`;

      const targetAngleRad = Math.atan2(dy, dx);
      const targetAngleDeg = (targetAngleRad * 180 / Math.PI) + 90;

      eggState.angle = interpolateAngle(eggState.angle, targetAngleDeg, 0.08);

      const walkingWiggle = Math.sin(timestamp * 0.02) * 10;
      bugActor.style.transform = `rotate(${eggState.angle + walkingWiggle}deg)`;
    }

    eggState.animationFrameId = requestAnimationFrame(animLoop);
    return;
  }

  if (eggState.state === 'scurrying') {
    const elapsed = timestamp - eggState.scurryStartTime;
    const duration = 350; // ms

    let homeX = window.innerWidth - 62;
    let homeY = window.innerHeight - 62;
    if (fabButton) {
      const rect = fabButton.getBoundingClientRect();
      homeX = rect.left;
      homeY = rect.top;
    }

    const t = Math.min(elapsed / duration, 1);
    const easeOutCubic = 1 - Math.pow(1 - t, 3);

    eggState.x = eggState.scurryStartPos.x + (homeX - eggState.scurryStartPos.x) * easeOutCubic;
    eggState.y = eggState.scurryStartPos.y + (homeY - eggState.scurryStartPos.y) * easeOutCubic;

    bugActor.style.left = `${eggState.x}px`;
    bugActor.style.top = `${eggState.y}px`;

    const dx = homeX - eggState.scurryStartPos.x;
    const dy = homeY - eggState.scurryStartPos.y;
    const homeAngleRad = Math.atan2(dy, dx);
    const homeAngleDeg = (homeAngleRad * 180 / Math.PI) + 90;

    eggState.angle = interpolateAngle(eggState.scurryStartAngle, homeAngleDeg, t * 2);
    const panicWiggle = Math.sin(timestamp * 0.08) * 18;
    bugActor.style.transform = `rotate(${eggState.angle + panicWiggle}deg)`;

    if (t >= 1) {
      eggState.state = 'sleeping';

      if (fabButton) {
        fabButton.classList.remove('bug-crawled-away');
        const innerSvg = fabButton.querySelector('svg');
        if (innerSvg) {
          innerSvg.style.opacity = '';
          innerSvg.style.visibility = '';
          innerSvg.style.display = '';
        }
      }

      if (bugActor) {
        bugActor.style.display = 'none';
        bugActor.style.left = '';
        bugActor.style.top = '';
        bugActor.style.transform = '';
      }

      eggState.currentTarget = null;
      eggState.angle = 0;
      eggState.lastTime = 0;

      idleTimeout = setTimeout(startEasterEgg, IDLE_TIME);
    } else {
      eggState.animationFrameId = requestAnimationFrame(animLoop);
    }
    return;
  }
}

export function initBusinessReporter({
  session,
  getActiveModule,
  commandBus,
  db = null,
  sync = null,
}) {
  if (!session?.authenticated || document.querySelector('[data-ctox-reporter]')) return;
  installReporterStyles();
  reporterState = {
    session,
    getActiveModule,
    commandBus,
    db,
    sync,
    modal: null,
    overlay: null,
    attachment: null,
    markupMode: 'idle',
    selectionOrigin: null,
    selectionRect: null,
    strokes: [],
    activeStroke: null,
    savingMarkup: false,
  };

  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'ctox-report-fab';
  button.dataset.ctoxReporter = 'true';
  button.setAttribute('aria-label', 'Bug oder Feature an CTOX melden');
  button.title = 'Bug oder Feature an CTOX melden';
  button.innerHTML = bugIconSvg();
  button.addEventListener('click', () => openReporterDialog(reporterState));
  document.body.append(button);

  fabButton = button;

  const handleActivity = (event) => {
    if (event.target.closest('.ctox-report-fab') || event.target.closest('.ctox-bug-actor')) {
      if (eggState.state !== 'sleeping') {
        stopEasterEggInstantly();
      }
      return;
    }
    resetIdleTimer();
  };

  function resetIdleTimer() {
    if (idleTimeout) {
      clearTimeout(idleTimeout);
      idleTimeout = null;
    }
    if (eggState.state === 'awakening' || eggState.state === 'crawling') {
      scurryBack();
    } else if (eggState.state === 'sleeping') {
      idleTimeout = setTimeout(startEasterEgg, IDLE_TIME);
    }
  }

  window.addEventListener('mousemove', handleActivity, { passive: true });
  window.addEventListener('mousedown', handleActivity, { passive: true });
  window.addEventListener('keydown', handleActivity, { passive: true });
  window.addEventListener('scroll', handleActivity, { passive: true });
  window.addEventListener('touchstart', handleActivity, { passive: true });
  window.addEventListener('pointermove', handleActivity, { passive: true });

  idleTimeout = setTimeout(startEasterEgg, IDLE_TIME);
}

function openReporterDialog(state) {
  const module = state.getActiveModule?.() || { id: 'ctox', title: 'CTOX' };
  const backdrop = document.createElement('div');
  backdrop.className = 'ctox-report-backdrop';
  backdrop.innerHTML = `
    <form class="ctox-report-dialog" data-report-form>
      <header>
        <div>
          <strong>Bug oder Feature an CTOX</strong>
          <span>${escapeHtml(module.title || module.id || 'Business OS')}</span>
        </div>
        <button type="button" class="ctox-report-close" data-close aria-label="Schließen">x</button>
      </header>
      <div class="ctox-report-grid">
        <label>
          <span>Typ</span>
          <select name="kind">
            <option value="bug">Bug</option>
            <option value="feature">Feature-Wunsch</option>
          </select>
        </label>
        <label>
          <span>Priorität</span>
          <select name="severity">
            <option value="medium">Mittel</option>
            <option value="high">Hoch</option>
            <option value="low">Niedrig</option>
          </select>
        </label>
      </div>
      <label>
        <span>Titel</span>
        <input name="title" required placeholder="Kurz beschreiben" />
      </label>
      <label>
        <span>Beschreibung</span>
        <textarea name="summary" rows="5" placeholder="Was ist passiert oder was wird gebraucht?"></textarea>
      </label>
      <label>
        <span>Erwartung</span>
        <textarea name="expected" rows="3" placeholder="Was sollte CTOX tun oder prüfen?"></textarea>
      </label>
      <div class="ctox-report-actions">
        <button type="button" class="ctox-report-secondary" data-markup>${screenIconSvg()}<span>Screenshot + Kritzeln</span></button>
        <button type="button" class="ctox-report-secondary" data-open-reports>Bugs & Features öffnen</button>
      </div>
      <div class="ctox-report-attachment" data-attachment hidden>
        <div>
          <span data-attachment-label></span>
          <button type="button" data-remove-attachment>Entfernen</button>
        </div>
        <img alt="Report Screenshot" data-attachment-img />
      </div>
      <footer>
        <span data-status></span>
        <button type="submit">An CTOX senden</button>
      </footer>
    </form>
  `;
  state.modal = backdrop;
  state.attachment = null;
  backdrop.querySelector('[data-close]')?.addEventListener('click', () => closeReporterDialog(state));
  backdrop.querySelector('[data-open-reports]')?.addEventListener('click', () => {
    closeReporterDialog(state);
    location.hash = '#reports';
  });
  backdrop.querySelector('[data-remove-attachment]')?.addEventListener('click', () => {
    state.attachment = null;
    syncAttachmentPreview(state);
  });
  backdrop.querySelector('[data-markup]')?.addEventListener('click', () => startMarkup(state));
  backdrop.addEventListener('click', (event) => {
    if (event.target === backdrop) closeReporterDialog(state);
  });
  backdrop.querySelector('[data-report-form]')?.addEventListener('submit', async (event) => {
    event.preventDefault();
    await submitReport(state, module, event.currentTarget);
  });
  document.body.append(backdrop);
  backdrop.querySelector('input[name="title"]')?.focus();
}

function closeReporterDialog(state) {
  if (state.markupMode !== 'idle') cancelMarkup(state);
  state.modal?.remove();
  state.modal = null;
}

async function submitReport(state, module, form) {
  const status = form.querySelector('[data-status]');
  const submit = form.querySelector('button[type="submit"]');
  const data = new FormData(form);
  const now = Date.now();
  const title = data.get('title')?.toString().trim() || 'Business OS report';
  const summary = data.get('summary')?.toString().trim() || '';
  const expected = data.get('expected')?.toString().trim() || '';
  const kind = data.get('kind')?.toString() || 'bug';
  const severity = data.get('severity')?.toString() || 'medium';
  const clientContext = {
    source: 'business-os-reporter',
    module_id: module.id || '',
    url: location.href,
    app_version: document.documentElement.dataset.appVersion || '',
    viewport: {
      width: innerWidth,
      height: innerHeight,
      scrollX: scrollX,
      scrollY: scrollY,
      devicePixelRatio: devicePixelRatio || 1,
    },
    user_agent: navigator.userAgent,
    created_at: new Date(now).toISOString(),
    attachment: state.attachment ? {
      rect: state.attachment.rect,
      strokes: state.attachment.strokes,
      capture_mode: state.attachment.captureMode,
      captured_at: state.attachment.capturedAt,
      mime: state.attachment.compositeDataUrl?.match(/^data:([^;]+)/)?.[1] || 'image/png',
      data_url: state.attachment.compositeDataUrl,
    } : null,
  };
  submit.disabled = true;
  status.textContent = 'Sende...';
  try {
    const result = await dispatchBusinessReport({
      commandBus: state.commandBus,
      session: state.session,
      module,
      kind,
      severity,
      title,
      summary,
      expected,
      clientContext,
      now,
    });
    await upsertLocalReport(state, {
      result,
      module,
      kind,
      severity,
      title,
      summary,
      expected,
      clientContext,
      now,
    });
    window.dispatchEvent(new CustomEvent('ctox-business-os-reports-updated', {
      detail: { reportId: result.report_id || '', moduleId: module.id || '' },
    }));
    status.textContent = 'Als CTOX Task angelegt.';
    setTimeout(() => closeReporterDialog(state), 700);
  } catch (error) {
    submit.disabled = false;
    status.textContent = error.message || String(error);
  }
}

export async function dispatchBusinessReport({
  commandBus,
  session,
  module,
  kind = 'bug',
  severity = 'medium',
  title = 'Business OS report',
  summary = '',
  expected = '',
  clientContext = {},
  now = Date.now(),
}) {
  if (!commandBus?.dispatch) {
    throw new Error('business_commands collection is required for reports');
  }
  const reportId = `report_${newId()}`;
  const commandId = `cmd_${newId()}`;
  const moduleId = module?.id || clientContext?.module_id || 'ctox';
  const actor = session?.user ? {
    id: session.user.id || '',
    display_name: session.user.display_name || session.user.name || session.user.id || '',
    role: session.user.role || 'user',
    is_admin: Boolean(session.user.is_admin),
  } : null;
  await commandBus.dispatch({
    id: commandId,
    module: 'ctox',
    type: `ctox.report.${kind || 'bug'}`,
    record_id: reportId,
    inbound_channel: moduleId,
    payload: {
      report_id: reportId,
      module_id: moduleId,
      kind,
      severity,
      title,
      summary,
      expected,
      reporter_id: actor?.id || '',
    },
    client_context: {
      ...clientContext,
      actor,
      created_at: clientContext?.created_at || new Date(now).toISOString(),
    },
  });
  return {
    ok: true,
    report_id: reportId,
    command_id: commandId,
    task_id: '',
    task_status: 'pending_sync',
    status: 'open',
    transport: 'rxdb-webrtc',
  };
}

async function upsertLocalReport(state, report) {
  const raw = state.db?.raw;
  if (!raw) return;
  await prepareReportSync(state.sync);
  const id = report.result?.report_id || `report_${crypto.randomUUID?.() || Date.now()}`;
  const taskId = report.result?.task_id || '';
  const commandId = report.result?.command_id || '';
  const common = {
    id,
    report_id: id,
    module_id: report.module.id || 'ctox',
    kind: report.kind,
    severity: report.severity,
    title: report.title,
    summary: report.summary,
    expected: report.expected,
    status: report.result?.status || 'open',
    reporter_id: state.session?.user?.id || '',
    ctox_command_id: commandId,
    task_id: taskId,
    inbound_channel: report.module.id || 'ctox',
    client_context: report.clientContext,
    created_at_ms: report.now,
    updated_at_ms: report.now,
  };
  await upsertRx(raw.business_module_reports, common);
  await upsertRx(raw.ctox_bug_reports, {
    id,
    title: report.title,
    status: report.result?.status || 'open',
    module: report.module.id || 'ctox',
    inbound_channel: report.module.id || 'ctox',
    severity: report.severity,
    surface: 'business-os',
    description: report.summary,
    evidence: report.clientContext,
    payload: {
      kind: report.kind,
      expected: report.expected,
      ctox_command_id: commandId,
      task_id: taskId,
      change_summary: '',
      rollback_version_id: '',
    },
    created_at_ms: report.now,
    updated_at_ms: report.now,
  });
  await waitForReportSync(state.sync);
}

async function upsertRx(collection, doc) {
  if (!collection) return;
  try {
    await collection.insert(doc);
    return;
  } catch (error) {
    if (!isRxDbConflictError(error)) throw error;
  }
  const existing = await collection.findOne(doc.id).exec();
  if (existing) await existing.patch(doc);
  else await collection.insert(doc);
}

async function prepareReportSync(sync) {
  if (!sync?.startCollection) return;
  await Promise.all([
    sync.startCollection('business_module_reports').then((bridge) => waitForSyncBridgeReady(bridge, 10000)).catch(() => null),
    sync.startCollection('ctox_bug_reports').then((bridge) => waitForSyncBridgeReady(bridge, 10000)).catch(() => null),
  ]);
}

async function waitForReportSync(sync) {
  if (!sync?.startCollection) return;
  await Promise.all([
    sync.startCollection('business_module_reports').then((bridge) => waitForSyncBridgeReady(bridge, 10000)).catch(() => null),
    sync.startCollection('ctox_bug_reports').then((bridge) => waitForSyncBridgeReady(bridge, 10000)).catch(() => null),
  ]);
}

async function waitForSyncBridgeReady(bridge, timeoutMs = 10000) {
  const state = bridge?.state;
  if (!state) return;
  await Promise.race([
    Promise.resolve()
      .then(() => state.awaitInSync?.() || state.awaitInitialReplication?.())
      .catch(() => {}),
    new Promise((resolve) => setTimeout(resolve, timeoutMs)),
  ]);
}

function newId() {
  return globalThis.crypto?.randomUUID?.() || `${Date.now()}_${Math.random().toString(36).slice(2)}`;
}

function isRxDbConflictError(error) {
  const message = String(error?.message || error || '');
  return message.includes('RxDB Error-Code: CONFLICT')
    || message.includes('conflict')
    || message.includes('document already exists')
    || message.includes('Document update conflict');
}

function startMarkup(state) {
  if (state.markupMode !== 'idle') return;
  state.markupMode = 'selecting';
  state.selectionOrigin = null;
  state.selectionRect = null;
  state.strokes = [];
  state.activeStroke = null;
  hideReporterChrome(state);
  renderMarkupOverlay(state);
}

function cancelMarkup(state) {
  state.markupMode = 'idle';
  state.selectionOrigin = null;
  state.selectionRect = null;
  state.strokes = [];
  state.activeStroke = null;
  state.overlay?.remove();
  state.overlay = null;
  showReporterChrome(state);
}

function hideReporterChrome(state) {
  if (state.modal) {
    state.modal.dataset.wasOpen = state.modal.hidden ? '0' : '1';
    state.modal.hidden = true;
    state.modal.style.display = 'none';
  }
  const fab = document.querySelector('[data-ctox-reporter]');
  if (fab) fab.style.display = 'none';
}

function showReporterChrome(state) {
  if (state.modal) {
    state.modal.style.display = '';
    if (state.modal.dataset.wasOpen === '1') state.modal.hidden = false;
    delete state.modal.dataset.wasOpen;
  }
  const fab = document.querySelector('[data-ctox-reporter]');
  if (fab) fab.style.display = '';
}

function renderMarkupOverlay(state) {
  state.overlay?.remove();
  const overlay = document.createElement('div');
  overlay.className = 'ctox-report-markup-overlay';
  overlay.innerHTML = `
    <div class="ctox-report-markup-toolbar" data-toolbar>
      <strong>Bereich auswählen und markieren</strong>
      <span>Ziehe einen Bereich auf. Danach kannst du mit dem Stift darauf zeichnen.</span>
      <div>
        <button type="button" data-toolbar-action="cancel">Abbrechen</button>
        <button type="button" data-toolbar-action="clear" hidden>Löschen</button>
        <button type="button" data-toolbar-action="save" hidden>Übernehmen</button>
      </div>
    </div>
    <div class="ctox-report-markup-selection" data-selection hidden></div>
  `;
  state.overlay = overlay;
  document.body.append(overlay);
  overlay.addEventListener('pointerdown', (event) => onOverlayPointerDown(state, event));
  overlay.addEventListener('pointermove', (event) => onOverlayPointerMove(state, event));
  overlay.addEventListener('pointerup', (event) => onOverlayPointerUp(state, event));
  overlay.querySelector('[data-toolbar]')?.addEventListener('pointerdown', (event) => {
    event.preventDefault();
    event.stopPropagation();
    clearTextSelection();
  });
  overlay.querySelector('[data-toolbar]')?.addEventListener('click', (event) => {
    const action = event.target.closest('[data-toolbar-action]')?.dataset.toolbarAction;
    if (action === 'cancel') cancelMarkup(state);
    if (action === 'clear') {
      state.strokes = [];
      state.activeStroke = null;
      paintSelection(state);
    }
    if (action === 'save') commitMarkup(state);
  });
  paintSelection(state);
}

function onOverlayPointerDown(state, event) {
  event.preventDefault();
  clearTextSelection();
  if (state.markupMode === 'selecting') {
    state.selectionOrigin = { x: event.clientX, y: event.clientY };
    state.selectionRect = { x: event.clientX, y: event.clientY, width: 0, height: 0 };
    state.overlay.setPointerCapture?.(event.pointerId);
    paintSelection(state);
  } else if (state.markupMode === 'drawing') {
    if (!isInsideRect(event.clientX, event.clientY, state.selectionRect)) return;
    state.activeStroke = [relativePoint(state, event)];
    state.overlay.setPointerCapture?.(event.pointerId);
    paintSelection(state);
  }
}

function onOverlayPointerMove(state, event) {
  event.preventDefault();
  if (state.markupMode === 'selecting' && state.selectionOrigin) {
    state.selectionRect = normalizeRect(state.selectionOrigin, { x: event.clientX, y: event.clientY });
    paintSelection(state);
  } else if (state.markupMode === 'drawing' && state.activeStroke) {
    state.activeStroke.push(relativePoint(state, event));
    paintSelection(state);
  }
}

function onOverlayPointerUp(state, event) {
  event.preventDefault();
  clearTextSelection();
  if (state.markupMode === 'selecting' && state.selectionRect) {
    state.overlay.releasePointerCapture?.(event.pointerId);
    if (state.selectionRect.width < 12 || state.selectionRect.height < 12) {
      state.selectionOrigin = null;
      state.selectionRect = null;
      paintSelection(state);
      return;
    }
    state.markupMode = 'drawing';
    paintSelection(state);
  } else if (state.markupMode === 'drawing' && state.activeStroke) {
    state.overlay.releasePointerCapture?.(event.pointerId);
    if (state.activeStroke.length > 1) state.strokes.push(state.activeStroke);
    state.activeStroke = null;
    paintSelection(state);
  }
}

function clearTextSelection() {
  window.getSelection?.().removeAllRanges?.();
}

function paintSelection(state) {
  const selection = state.overlay?.querySelector('[data-selection]');
  if (!selection) return;
  const clear = state.overlay.querySelector('[data-toolbar-action="clear"]');
  const save = state.overlay.querySelector('[data-toolbar-action="save"]');
  if (!state.selectionRect) {
    selection.hidden = true;
    if (clear) clear.hidden = true;
    if (save) save.hidden = true;
    placeMarkupToolbar(state);
    return;
  }
  const rect = state.selectionRect;
  selection.hidden = false;
  selection.style.left = `${rect.x}px`;
  selection.style.top = `${rect.y}px`;
  selection.style.width = `${rect.width}px`;
  selection.style.height = `${rect.height}px`;
  selection.dataset.mode = state.markupMode;
  const allStrokes = state.activeStroke ? [...state.strokes, state.activeStroke] : state.strokes;
  selection.innerHTML = `
    <svg width="100%" height="100%" viewBox="0 0 ${rect.width} ${rect.height}">
      ${allStrokes.map((stroke) => `<polyline points="${stroke.map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(' ')}" fill="none" stroke="#ef4444" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>`).join('')}
    </svg>
  `;
  if (clear) clear.hidden = state.markupMode !== 'drawing' || allStrokes.length === 0;
  if (save) save.hidden = state.markupMode !== 'drawing';
  placeMarkupToolbar(state);
}

function placeMarkupToolbar(state) {
  const toolbar = state.overlay?.querySelector('[data-toolbar]');
  if (!toolbar) return;
  const margin = 12;
  const toolbarRect = toolbar.getBoundingClientRect();
  const toolbarSize = {
    width: Math.min(toolbarRect.width || toolbar.offsetWidth || 0, window.innerWidth - margin * 2),
    height: toolbarRect.height || toolbar.offsetHeight || 0,
  };
  const setPosition = (x, y) => {
    toolbar.style.left = `${Math.round(x)}px`;
    toolbar.style.top = `${Math.round(y)}px`;
    toolbar.style.transform = 'none';
  };
  const fallback = { x: (window.innerWidth - toolbarSize.width) / 2, y: margin };
  const rect = state.selectionRect;
  if (!rect || !toolbarSize.width || !toolbarSize.height) {
    setPosition(fallback.x, fallback.y);
    return;
  }
  const maxX = Math.max(margin, window.innerWidth - toolbarSize.width - margin);
  const maxY = Math.max(margin, window.innerHeight - toolbarSize.height - margin);
  const candidates = [
    { x: fallback.x, y: margin },
    { x: rect.x + rect.width / 2 - toolbarSize.width / 2, y: rect.y - toolbarSize.height - margin },
    { x: rect.x + rect.width / 2 - toolbarSize.width / 2, y: rect.y + rect.height + margin },
    { x: fallback.x, y: window.innerHeight - toolbarSize.height - margin },
  ].map((item) => ({ x: clamp(item.x, margin, maxX), y: clamp(item.y, margin, maxY) }));
  const blocked = { x: rect.x - margin, y: rect.y - margin, width: rect.width + margin * 2, height: rect.height + margin * 2 };
  const placed = candidates.find((item) => !rectsIntersect({ ...item, width: toolbarSize.width, height: toolbarSize.height }, blocked));
  setPosition((placed || candidates[0]).x, (placed || candidates[0]).y);
}

async function commitMarkup(state) {
  if (state.markupMode !== 'drawing' || !state.selectionRect || state.savingMarkup) return;
  state.savingMarkup = true;
  const rect = { ...state.selectionRect };
  const finalStrokes = state.activeStroke ? [...state.strokes, state.activeStroke] : [...state.strokes];
  const markupSvgDataUrl = buildSvgDataUrl(rect, finalStrokes);
  state.markupMode = 'idle';
  if (state.overlay) {
    state.overlay.style.visibility = 'hidden';
    state.overlay.style.pointerEvents = 'none';
  }
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  try {
    const screenDataUrl = await captureScreenRegion(rect).catch(() => null);
    const domDataUrl = screenDataUrl ? null : await captureDomRegion(rect).catch(() => null);
    const screenshotDataUrl = screenDataUrl || domDataUrl;
    const compositeDataUrl = screenshotDataUrl
      ? await buildCompositeDataUrl(rect, finalStrokes, screenshotDataUrl).catch(() => markupSvgDataUrl)
      : markupSvgDataUrl;
    state.attachment = {
      rect,
      strokes: finalStrokes,
      screenshotDataUrl,
      markupSvgDataUrl,
      compositeDataUrl,
      captureMode: screenDataUrl ? 'screen' : domDataUrl ? 'dom' : 'markup-only',
      capturedAt: new Date().toISOString(),
    };
    syncAttachmentPreview(state);
  } finally {
    state.overlay?.remove();
    state.overlay = null;
    state.selectionOrigin = null;
    state.selectionRect = null;
    state.strokes = [];
    state.activeStroke = null;
    state.savingMarkup = false;
    showReporterChrome(state);
  }
}

function syncAttachmentPreview(state) {
  const wrap = state.modal?.querySelector('[data-attachment]');
  const img = state.modal?.querySelector('[data-attachment-img]');
  const label = state.modal?.querySelector('[data-attachment-label]');
  if (!wrap || !img || !label) return;
  if (!state.attachment) {
    wrap.hidden = true;
    img.removeAttribute('src');
    return;
  }
  wrap.hidden = false;
  img.src = state.attachment.compositeDataUrl;
  label.textContent = state.attachment.captureMode === 'markup-only'
    ? 'Markup gespeichert, Screenshot nicht verfuegbar'
    : 'Screenshot mit Markup';
}

async function captureScreenRegion(rect) {
  const chromeCapture = await captureVisibleTabPng();
  if (chromeCapture) {
    const image = await loadImage(chromeCapture);
    const dpr = window.devicePixelRatio || 1;
    const expectedW = window.innerWidth * dpr;
    const expectedH = window.innerHeight * dpr;
    const matches = Math.abs(image.naturalWidth - expectedW) / Math.max(1, expectedW) < 0.2
      && Math.abs(image.naturalHeight - expectedH) / Math.max(1, expectedH) < 0.4;
    if (matches) return cropImageDataUrl(image, rect, dpr, dpr);
  }
  if (!navigator.mediaDevices?.getDisplayMedia) return null;
  let stream;
  try {
    stream = await navigator.mediaDevices.getDisplayMedia({ video: { displaySurface: 'browser' }, audio: false });
  } catch {
    return null;
  }
  try {
    const video = document.createElement('video');
    video.muted = true;
    video.playsInline = true;
    video.srcObject = stream;
    await video.play();
    await waitForVideoFrame(video);
    const scaleX = Math.max(1, video.videoWidth) / window.innerWidth;
    const scaleY = Math.max(1, video.videoHeight) / window.innerHeight;
    return cropImageDataUrl(video, rect, scaleX, scaleY, video.videoWidth, video.videoHeight);
  } finally {
    stream?.getTracks?.().forEach((track) => track.stop());
  }
}

function captureVisibleTabPng() {
  const tabs = globalThis.chrome?.tabs;
  const runtime = globalThis.chrome?.runtime;
  if (!tabs?.captureVisibleTab) return Promise.resolve(null);
  return new Promise((resolve) => {
    try {
      tabs.captureVisibleTab({ format: 'png' }, (dataUrl) => {
        if (runtime?.lastError) return resolve(null);
        resolve(dataUrl || null);
      });
    } catch {
      resolve(null);
    }
  });
}

function cropImageDataUrl(source, rect, scaleX, scaleY, sourceWidth = source.naturalWidth, sourceHeight = source.naturalHeight) {
  const sx = Math.max(0, Math.round(rect.x * scaleX));
  const sy = Math.max(0, Math.round(rect.y * scaleY));
  const sw = Math.max(1, Math.min(sourceWidth - sx, Math.round(rect.width * scaleX)));
  const sh = Math.max(1, Math.min(sourceHeight - sy, Math.round(rect.height * scaleY)));
  const canvas = document.createElement('canvas');
  canvas.width = sw;
  canvas.height = sh;
  const ctx = canvas.getContext('2d');
  if (!ctx) return null;
  ctx.drawImage(source, sx, sy, sw, sh, 0, 0, sw, sh);
  return canvas.toDataURL('image/png');
}

async function captureDomRegion(rect) {
  try {
    const width = Math.max(1, Math.round(window.innerWidth));
    const height = Math.max(1, Math.round(window.innerHeight));
    const clone = document.documentElement.cloneNode(true);
    clone.querySelectorAll('script, .ctox-report-markup-overlay, .ctox-report-backdrop, [data-ctox-reporter]').forEach((node) => node.remove());
    const styleEl = document.createElement('style');
    styleEl.textContent = collectStyleText();
    clone.querySelector('head')?.append(styleEl);
    clone.setAttribute('style', `${clone.getAttribute('style') || ''};width:${width}px;min-height:${height}px;`);
    const serialized = new XMLSerializer().serializeToString(clone);
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}"><foreignObject width="100%" height="100%">${serialized}</foreignObject></svg>`;
    const image = await loadImage(`data:image/svg+xml;base64,${btoa(unescape(encodeURIComponent(svg)))}`);
    return cropImageDataUrl(image, rect, 1, 1, width, height);
  } catch {
    return null;
  }
}

function buildSvgDataUrl(rect, strokeList) {
  const polylines = strokeList.map((stroke) => {
    const points = stroke.map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(' ');
    return `<polyline points="${points}" fill="none" stroke="#ef4444" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"/>`;
  }).join('');
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${rect.width}" height="${rect.height}" viewBox="0 0 ${rect.width} ${rect.height}"><rect width="100%" height="100%" fill="rgba(239,68,68,0.08)" stroke="#ef4444" stroke-width="2"/>${polylines}</svg>`;
  return `data:image/svg+xml;base64,${btoa(unescape(encodeURIComponent(svg)))}`;
}

async function buildCompositeDataUrl(rect, strokeList, screenshotDataUrl) {
  const image = await loadImage(screenshotDataUrl);
  const canvas = document.createElement('canvas');
  canvas.width = Math.max(1, image.naturalWidth || Math.round(rect.width));
  canvas.height = Math.max(1, image.naturalHeight || Math.round(rect.height));
  const ctx = canvas.getContext('2d');
  if (!ctx) return screenshotDataUrl;
  ctx.drawImage(image, 0, 0, canvas.width, canvas.height);
  ctx.strokeStyle = '#ef4444';
  ctx.lineWidth = Math.max(3, Math.round(canvas.width / Math.max(120, rect.width) * 4));
  ctx.lineCap = 'round';
  ctx.lineJoin = 'round';
  const scaleX = canvas.width / rect.width;
  const scaleY = canvas.height / rect.height;
  for (const stroke of strokeList) {
    if (stroke.length < 2) continue;
    ctx.beginPath();
    ctx.moveTo(stroke[0].x * scaleX, stroke[0].y * scaleY);
    stroke.slice(1).forEach((p) => ctx.lineTo(p.x * scaleX, p.y * scaleY));
    ctx.stroke();
  }
  return canvas.toDataURL('image/png');
}

function collectStyleText() {
  return Array.from(document.styleSheets).map((sheet) => {
    try {
      return Array.from(sheet.cssRules).map((rule) => rule.cssText).join('\n');
    } catch {
      return '';
    }
  }).join('\n');
}

function relativePoint(state, event) {
  const rect = state.selectionRect;
  return {
    x: Math.max(0, Math.min(rect.width, event.clientX - rect.x)),
    y: Math.max(0, Math.min(rect.height, event.clientY - rect.y)),
  };
}

function normalizeRect(start, end) {
  return {
    x: Math.min(start.x, end.x),
    y: Math.min(start.y, end.y),
    width: Math.abs(end.x - start.x),
    height: Math.abs(end.y - start.y),
  };
}

function isInsideRect(x, y, rect) {
  return rect && x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height;
}

function rectsIntersect(a, b) {
  return a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y;
}

function clamp(value, min, max) {
  return Math.max(min, Math.min(max, value));
}

function loadImage(src) {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error('Image load failed'));
    image.src = src;
  });
}

function waitForVideoFrame(video) {
  if (typeof video.requestVideoFrameCallback === 'function') {
    return new Promise((resolve) => video.requestVideoFrameCallback(resolve));
  }
  return new Promise((resolve) => setTimeout(resolve, 160));
}

function bugIconSvg() {
  return `
    <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true" focusable="false">
      <path d="M12 2.5a3.5 3.5 0 0 1 3.5 3.5h-7A3.5 3.5 0 0 1 12 2.5Zm-7 8a1 1 0 0 1 1-1h1.2A6 6 0 0 1 8 9V8h8v1a6 6 0 0 1 .8.5H18a1 1 0 1 1 0 2h-1.06A6 6 0 0 1 17 13h2a1 1 0 1 1 0 2h-2a6 6 0 0 1-.27 1.37L18.7 17.3a1 1 0 1 1-1.4 1.4l-1.7-1.7A6 6 0 0 1 13 18.94V12h-2v6.94A6 6 0 0 1 8.4 17l-1.7 1.7a1 1 0 1 1-1.4-1.4l1.97-1.96A6 6 0 0 1 7 15H5a1 1 0 1 1 0-2h2a6 6 0 0 1 .06-1.5H6a1 1 0 0 1-1-1Z" fill="currentColor"/>
    </svg>`;
}

function screenIconSvg() {
  return `
    <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true" focusable="false">
      <path d="M4 5.5A2.5 2.5 0 0 1 6.5 3h11A2.5 2.5 0 0 1 20 5.5v8A2.5 2.5 0 0 1 17.5 16H13v2h3a1 1 0 1 1 0 2H8a1 1 0 1 1 0-2h3v-2H6.5A2.5 2.5 0 0 1 4 13.5v-8Zm2.5-.5a.5.5 0 0 0-.5.5v8a.5.5 0 0 0 .5.5h11a.5.5 0 0 0 .5-.5v-8a.5.5 0 0 0-.5-.5h-11Z" fill="currentColor"/>
    </svg>`;
}

function installReporterStyles() {
  if (document.getElementById(REPORTER_STYLE_ID)) return;
  const style = document.createElement('style');
  style.id = REPORTER_STYLE_ID;
  style.textContent = `
    .ctox-report-fab {
      position: fixed;
      right: 18px;
      bottom: 18px;
      z-index: 40;
      display: inline-flex;
      align-items: center;
      justify-content: center;
      width: 44px;
      height: 44px;
      border: 1px solid rgba(239, 68, 68, .45);
      background: #20252b;
      color: #f2f5f7;
      border-radius: 7px;
      padding: 0;
      font: 700 12px/1.1 ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      box-shadow: 0 12px 32px rgba(0, 0, 0, .35);
      cursor: pointer;
    }
    .ctox-report-fab svg { color: #ef4444; flex: 0 0 auto; }
    .ctox-report-fab.bug-crawled-away svg {
      opacity: 0 !important;
      visibility: hidden !important;
      display: none !important;
    }
    .ctox-bug-actor {
      position: fixed;
      z-index: 999999;
      pointer-events: auto;
      cursor: pointer;
      width: 44px;
      height: 44px;
      display: inline-flex;
      align-items: center;
      justify-content: center;
      background: transparent;
      color: #ef4444;
      transition: none;
    }
    .ctox-report-backdrop {
      position: fixed;
      inset: 0;
      z-index: 80;
      display: grid;
      place-items: center;
      background: rgba(5, 8, 12, .62);
      padding: 18px;
    }
    .ctox-report-backdrop[hidden] { display: none !important; }
    .ctox-report-dialog {
      width: min(720px, calc(100vw - 32px));
      max-height: calc(100vh - 36px);
      display: grid;
      gap: 14px;
      overflow: auto;
      background: #181c21;
      color: #e5e9ee;
      border: 1px solid rgba(112, 131, 151, .32);
      border-radius: 8px;
      padding: 18px;
      box-shadow: 0 20px 60px rgba(0, 0, 0, .42);
      font: 13px/1.35 ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }
    .ctox-report-dialog header,
    .ctox-report-dialog footer,
    .ctox-report-grid {
      display: flex;
      gap: 12px;
      align-items: center;
      justify-content: space-between;
    }
    .ctox-report-dialog header span,
    .ctox-report-dialog label span,
    .ctox-report-dialog footer span {
      display: block;
      color: #9aa4af;
      font-size: 12px;
    }
    .ctox-report-dialog label {
      display: grid;
      gap: 6px;
    }
    .ctox-report-grid label {
      flex: 1;
    }
    .ctox-report-dialog input,
    .ctox-report-dialog textarea,
    .ctox-report-dialog select {
      width: 100%;
      box-sizing: border-box;
      border: 1px solid rgba(133, 148, 163, .34);
      border-radius: 6px;
      background: #101318;
      color: #edf1f5;
      padding: 9px 10px;
      font: inherit;
    }
    .ctox-report-dialog input:focus,
    .ctox-report-dialog textarea:focus,
    .ctox-report-dialog select:focus {
      outline: 2px solid rgba(57, 140, 196, .7);
      outline-offset: 2px;
      border-color: rgba(57, 140, 196, .82);
    }
    .ctox-report-dialog button,
    .ctox-report-markup-toolbar button {
      border: 0;
      border-radius: 6px;
      background: #596a78;
      color: #f5f7f9;
      padding: 8px 11px;
      font: inherit;
      cursor: pointer;
    }
    .ctox-report-dialog header button {
      background: transparent;
      color: #a9b1ba;
      padding: 4px 7px;
    }
    .ctox-report-actions {
      display: flex;
      gap: 8px;
      flex-wrap: wrap;
    }
    .ctox-report-secondary {
      display: inline-flex;
      align-items: center;
      gap: 7px;
      border: 1px solid rgba(133, 148, 163, .24) !important;
      background: #20252b !important;
    }
    .ctox-report-attachment {
      display: grid;
      gap: 8px;
      border: 1px dashed rgba(133, 148, 163, .38);
      border-radius: 8px;
      padding: 8px;
    }
    .ctox-report-attachment[hidden] { display: none !important; }
    .ctox-report-attachment > div {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 10px;
      color: #9aa4af;
      font-size: 12px;
    }
    .ctox-report-attachment button {
      background: transparent;
      color: #9bd0f5;
      padding: 2px 4px;
    }
    .ctox-report-attachment img {
      max-width: 100%;
      max-height: 260px;
      object-fit: contain;
      border-radius: 6px;
      background: #0a0d11;
    }
    .ctox-report-markup-overlay {
      position: fixed;
      inset: 0;
      z-index: 2147483647;
      background: rgba(8, 12, 18, .18);
      cursor: crosshair;
      touch-action: none;
      user-select: none;
      -webkit-user-select: none;
    }
    .ctox-report-markup-overlay * {
      user-select: none;
      -webkit-user-select: none;
    }
    .ctox-report-markup-toolbar {
      position: fixed;
      top: 12px;
      left: 50%;
      z-index: 2;
      transform: translateX(-50%);
      display: flex;
      align-items: center;
      gap: 12px;
      max-width: calc(100vw - 24px);
      padding: 8px 12px;
      border: 1px solid rgba(255, 255, 255, .1);
      border-radius: 8px;
      background: #141a20;
      color: #e7ecf2;
      box-shadow: 0 12px 26px rgba(0, 0, 0, .4);
      cursor: default;
    }
    .ctox-report-markup-toolbar span {
      max-width: 300px;
      color: #a3afbd;
      font-size: 12px;
    }
    .ctox-report-markup-toolbar div {
      display: flex;
      gap: 6px;
    }
    .ctox-report-markup-selection {
      position: absolute;
      z-index: 1;
      box-sizing: border-box;
      border: 2px solid #ef4444;
      background: rgba(239, 68, 68, .08);
      pointer-events: auto;
    }
    .ctox-report-markup-selection[data-mode="drawing"] {
      cursor: crosshair;
    }
  `;
  document.head.append(style);
}

function escapeHtml(value) {
  return String(value ?? '').replace(/[&<>"']/g, (char) => ({
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#39;',
  }[char]));
}
