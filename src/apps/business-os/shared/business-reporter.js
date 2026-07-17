const REPORTER_STYLE_ID = 'ctox-business-reporter-style';
const REPORT_DISPATCH_TIMEOUT_MS = 25000;

let reporterState = null;
let fabButton = null;
let bugActor = null;

let eggState = {
  state: 'sleeping', // 'sleeping' | 'awakening' | 'crawling' | 'startled' | 'scurrying'
  x: 0,
  y: 0,
  angle: 0,
  speed: 0,
  targetSpeed: 0,
  animationFrameId: null,
  currentTarget: null,
  pauseUntil: 0,
  startleUntil: 0,
  wakeUpStartTime: 0,
  scurryStartTime: 0,
  scurryStartPos: null,
  scurryStartAngle: 0,
  lastTime: 0,
  pointerX: -1e4,
  pointerY: -1e4,
};
let idleTimeout = null;
const IDLE_TIME = 300000; // 5 minutes of inactivity
let idleDelay = IDLE_TIME;
const CRUISE_SPEED_MIN = 55; // px/s — a calm stroll
const CRUISE_SPEED_MAX = 95;
const STARTLE_SPEED = 220;
const STARTLE_RADIUS = 140; // cursor closer than this spooks a crawling bug
const FLEE_RADIUS = 60; // cursor this close sends it home

function setBugMotionClasses({ walking, pausing }) {
  if (!bugActor) return;
  bugActor.classList.toggle('is-walking', Boolean(walking));
  bugActor.classList.toggle('is-pausing', Boolean(pausing));
}

function shouldEnableIdleAnimation() {
  return !globalThis.ctoxBusinessOsDesktop;
}

function reporterCopy() {
  const en = document.documentElement.lang === 'en';
  return en
    ? {
      fabLabel: 'An app is never finished',
      fabTitle: 'An app is never finished — tell CTOX what should improve.',
      fabAria: 'Send feedback to CTOX: report a bug or feature',
      tagline: 'An app is never finished. Your report goes straight into the CTOX work queue.',
    }
    : {
      fabLabel: 'Eine App ist nie fertig',
      fabTitle: 'Eine App ist nie fertig — sag CTOX, was besser werden soll.',
      fabAria: 'Feedback an CTOX senden: Bug oder Feature melden',
      tagline: 'Eine App ist nie fertig. Dein Hinweis geht direkt in die CTOX-Arbeitsschlange.',
    };
}

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
    idleTimeout = setTimeout(startEasterEgg, idleDelay);
    return;
  }
  if (reporterState && (reporterState.modal || reporterState.markupMode !== 'idle')) {
    idleTimeout = setTimeout(startEasterEgg, idleDelay);
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
  eggState.angle = -45; // facing up-left, away from its corner home
  eggState.speed = 0;
  eggState.targetSpeed = 0;
  eggState.startleUntil = 0;
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
  bugActor.style.transform = `rotate(${eggState.angle}deg)`;
  bugActor.classList.add('is-appearing');
  setBugMotionClasses({ walking: false, pausing: false });
  requestAnimationFrame(() => bugActor?.classList.remove('is-appearing'));

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

  idleTimeout = setTimeout(startEasterEgg, idleDelay);
}

function animLoop(timestamp) {
  if (eggState.state === 'sleeping' || !fabButton || !bugActor) return;

  if (eggState.state === 'awakening') {
    const elapsed = timestamp - eggState.wakeUpStartTime;
    if (elapsed < 700) {
      // Stretch: a slow antenna sweep instead of frantic shaking.
      const stretch = Math.sin(elapsed / 700 * Math.PI) * 6;
      bugActor.style.transform = `rotate(${eggState.angle + stretch}deg)`;
      bugActor.classList.add('is-pausing');
      eggState.animationFrameId = requestAnimationFrame(animLoop);
      return;
    } else {
      eggState.state = 'crawling';
      eggState.currentTarget = getNextTarget();
      eggState.pauseUntil = 0;
      eggState.targetSpeed = CRUISE_SPEED_MIN + Math.random() * (CRUISE_SPEED_MAX - CRUISE_SPEED_MIN);
      eggState.lastTime = timestamp;
      bugActor.classList.remove('is-pausing');
    }
  }

  if (eggState.state === 'crawling' || eggState.state === 'startled') {
    if (!eggState.lastTime) eggState.lastTime = timestamp;
    const dt = Math.min((timestamp - eggState.lastTime) / 1000, 0.1);
    eggState.lastTime = timestamp;

    // A crawling bug notices a nearby cursor and darts away from it.
    const pdx = eggState.x + 13 - eggState.pointerX;
    const pdy = eggState.y + 13 - eggState.pointerY;
    const pointerDist = Math.hypot(pdx, pdy);
    if (eggState.state === 'crawling' && pointerDist < STARTLE_RADIUS) {
      eggState.state = 'startled';
      eggState.startleUntil = timestamp + 380 + Math.random() * 240;
      eggState.pauseUntil = 0;
      const away = Math.atan2(pdy, pdx);
      eggState.currentTarget = {
        x: Math.max(30, Math.min(window.innerWidth - 74, eggState.x + Math.cos(away) * 260)),
        y: Math.max(30, Math.min(window.innerHeight - 74, eggState.y + Math.sin(away) * 260)),
      };
    }
    if (eggState.state === 'startled' && timestamp > eggState.startleUntil && pointerDist > STARTLE_RADIUS) {
      eggState.state = 'crawling';
      eggState.targetSpeed = CRUISE_SPEED_MIN + Math.random() * (CRUISE_SPEED_MAX - CRUISE_SPEED_MIN);
    }

    if (eggState.state === 'crawling' && timestamp < eggState.pauseUntil) {
      // Resting: decelerate to zero, twitch antennae, glance around.
      eggState.speed = Math.max(0, eggState.speed - 300 * dt);
      setBugMotionClasses({ walking: false, pausing: true });
      const lookAngle = eggState.angle + Math.sin(timestamp * 0.004) * 7;
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

    if (distance < 12) {
      if (eggState.state === 'startled') {
        eggState.state = 'crawling';
      }
      eggState.pauseUntil = timestamp + 900 + Math.random() * 1900;
      eggState.currentTarget = getNextTarget();
      eggState.targetSpeed = CRUISE_SPEED_MIN + Math.random() * (CRUISE_SPEED_MAX - CRUISE_SPEED_MIN);
    } else {
      // Accelerate/decelerate toward the situational cruise speed; darting
      // when startled, easing out as it approaches a waypoint.
      const desired = eggState.state === 'startled'
        ? STARTLE_SPEED
        : Math.min(eggState.targetSpeed, Math.max(24, distance * 1.4));
      const accel = eggState.state === 'startled' ? 900 : 220;
      eggState.speed += Math.max(-accel * dt, Math.min(accel * dt, desired - eggState.speed));

      // Steer: heading eases toward the waypoint, so paths bend naturally
      // instead of snapping onto straight rails. A slow sway adds wander.
      const targetAngleDeg = Math.atan2(dy, dx) * 180 / Math.PI + 90;
      const steer = eggState.state === 'startled' ? 0.28 : 0.06;
      eggState.angle = interpolateAngle(eggState.angle, targetAngleDeg, steer);
      const sway = eggState.state === 'startled' ? 0 : Math.sin(timestamp * 0.0011) * 14;
      const headingRad = (eggState.angle + sway - 90) * Math.PI / 180;

      eggState.x += Math.cos(headingRad) * eggState.speed * dt;
      eggState.y += Math.sin(headingRad) * eggState.speed * dt;

      bugActor.style.left = `${eggState.x}px`;
      bugActor.style.top = `${eggState.y}px`;
      bugActor.style.transform = `rotate(${eggState.angle + sway}deg)`;
      setBugMotionClasses({ walking: eggState.speed > 8, pausing: false });
      bugActor.style.setProperty('--bug-gait-ms', `${Math.max(120, Math.round(26000 / Math.max(eggState.speed, 30)))}ms`);
    }

    eggState.animationFrameId = requestAnimationFrame(animLoop);
    return;
  }

  if (eggState.state === 'scurrying') {
    // Sprint home on foot — same locomotion, higher speed — instead of the
    // old teleport-glide. Reads as fleeing, not as a canceled animation.
    if (!eggState.lastTime) eggState.lastTime = timestamp;
    const dt = Math.min((timestamp - eggState.lastTime) / 1000, 0.1);
    eggState.lastTime = timestamp;

    let homeX = window.innerWidth - 62;
    let homeY = window.innerHeight - 62;
    if (fabButton) {
      const rect = fabButton.getBoundingClientRect();
      homeX = rect.left;
      homeY = rect.top;
    }

    const dx = homeX - eggState.x;
    const dy = homeY - eggState.y;
    const distance = Math.hypot(dx, dy);
    eggState.speed = Math.min(eggState.speed + 1200 * dt, 340);
    const homeAngleDeg = Math.atan2(dy, dx) * 180 / Math.PI + 90;
    eggState.angle = interpolateAngle(eggState.angle, homeAngleDeg, 0.35);
    const headingRad = (eggState.angle - 90) * Math.PI / 180;
    const step = Math.min(eggState.speed * dt, distance);
    eggState.x += Math.cos(headingRad) * step;
    eggState.y += Math.sin(headingRad) * step;

    bugActor.style.left = `${eggState.x}px`;
    bugActor.style.top = `${eggState.y}px`;
    bugActor.style.transform = `rotate(${eggState.angle}deg)`;
    setBugMotionClasses({ walking: true, pausing: false });
    bugActor.style.setProperty('--bug-gait-ms', '110ms');

    if (distance < 14) {
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

      idleTimeout = setTimeout(startEasterEgg, idleDelay);
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
  idleMs = IDLE_TIME,
}) {
  if (!session?.authenticated || document.querySelector('[data-ctox-reporter]')) return;
  idleDelay = Math.max(1000, Number(idleMs) || IDLE_TIME);
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

  const copy = reporterCopy();
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'ctox-report-fab';
  button.dataset.ctoxReporter = 'true';
  button.setAttribute('aria-label', copy.fabAria);
  button.title = copy.fabTitle;
  button.innerHTML = `${bugIconSvg()}<span class="ctox-report-fab-label">${escapeHtml(copy.fabLabel)}</span>`;
  button.addEventListener('click', () => openReporterDialog(reporterState));
  document.body.append(button);

  fabButton = button;

  if (!shouldEnableIdleAnimation()) return;

  const handleActivity = (event) => {
    const target = event.target && typeof event.target.closest === 'function'
      ? event.target
      : null;
    if (target && (target.closest('.ctox-report-fab') || target.closest('.ctox-bug-actor'))) {
      if (eggState.state !== 'sleeping') {
        stopEasterEggInstantly();
      }
      return;
    }
    // Pointer movement alone no longer panics the bug into despawning: it
    // tracks the cursor, sidesteps when it comes near (startle logic in the
    // anim loop), and only flees home when the cursor gets really close.
    // Real work signals (click, typing, scroll, touch) still end the stroll.
    if (event.type === 'mousemove' || event.type === 'pointermove') {
      eggState.pointerX = event.clientX ?? eggState.pointerX;
      eggState.pointerY = event.clientY ?? eggState.pointerY;
      if (eggState.state === 'sleeping') {
        resetIdleTimer();
        return;
      }
      const dist = Math.hypot(eggState.x + 13 - eggState.pointerX, eggState.y + 13 - eggState.pointerY);
      if (dist < FLEE_RADIUS) scurryBack();
      return;
    }
    resetIdleTimer();
  };

  function resetIdleTimer() {
    if (idleTimeout) {
      clearTimeout(idleTimeout);
      idleTimeout = null;
    }
    if (eggState.state === 'awakening' || eggState.state === 'crawling' || eggState.state === 'startled') {
      scurryBack();
    } else if (eggState.state === 'sleeping') {
      idleTimeout = setTimeout(startEasterEgg, idleDelay);
    }
  }

  window.addEventListener('mousemove', handleActivity, { passive: true });
  window.addEventListener('mousedown', handleActivity, { passive: true });
  window.addEventListener('keydown', handleActivity, { passive: true });
  window.addEventListener('scroll', handleActivity, { passive: true });
  window.addEventListener('touchstart', handleActivity, { passive: true });
  window.addEventListener('pointermove', handleActivity, { passive: true });

  idleTimeout = setTimeout(startEasterEgg, idleDelay);
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
      <p class="ctox-report-tagline">${escapeHtml(reporterCopy().tagline)}</p>
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
    attachment: reporterAttachmentContext(state.attachment),
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
    status.textContent = reporterStatusText(result);
    setTimeout(() => closeReporterDialog(state), result.task_id ? 900 : 1400);
  } catch (error) {
    submit.disabled = false;
    status.textContent = reporterErrorText(error);
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
  const dispatchResult = await withTimeout(commandBus.dispatch({
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
  }), REPORT_DISPATCH_TIMEOUT_MS, 'Report konnte nicht rechtzeitig an CTOX übergeben werden.');
  const taskId = String(dispatchResult?.task_id || '').trim();
  if (!taskId) {
    throw new Error('CTOX hat fuer den Report keine echte Queue-ID zurueckprojiziert.');
  }
  const status = String(dispatchResult?.status || 'accepted').trim() || 'accepted';
  const taskStatus = String(dispatchResult?.task_status || 'queued').trim() || 'queued';
  return {
    ok: dispatchResult?.ok !== false,
    report_id: reportId,
    command_id: dispatchResult?.command_id || commandId,
    task_id: taskId,
    task_status: taskStatus,
    status,
    transport: dispatchResult?.transport || 'rxdb-webrtc',
  };
}

function reporterAttachmentContext(attachment) {
  if (!attachment) return null;
  const dataUrl = String(attachment.compositeDataUrl || '');
  const mime = dataUrl.match(/^data:([^;]+)/)?.[1] || 'image/png';
  const strokes = Array.isArray(attachment.strokes) ? attachment.strokes : [];
  return {
    rect: attachment.rect || null,
    capture_mode: attachment.captureMode || '',
    captured_at: attachment.capturedAt || '',
    mime,
    has_screenshot: Boolean(dataUrl),
    screenshot_bytes_estimate: dataUrlByteLength(dataUrl),
    stroke_count: strokes.length,
    stroke_points_count: countStrokePoints(strokes),
  };
}

function dataUrlByteLength(dataUrl) {
  const payload = String(dataUrl || '').split(',')[1] || '';
  if (!payload) return 0;
  return Math.floor((payload.length * 3) / 4);
}

function countStrokePoints(strokes) {
  return strokes.reduce((sum, stroke) => sum + (Array.isArray(stroke) ? stroke.length : 0), 0);
}

function reporterStatusText(result) {
  if (result?.task_id) return 'Als CTOX Task angelegt.';
  return 'Report wurde nicht als CTOX Task bestaetigt.';
}

function reporterErrorText(error) {
  const message = String(error?.message || error || '').trim();
  return message || 'Report konnte nicht gesendet werden.';
}

async function withTimeout(promise, timeoutMs, message) {
  return Promise.race([
    Promise.resolve(promise),
    new Promise((_, reject) => setTimeout(() => reject(new Error(message)), timeoutMs)),
  ]);
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
  // Top-down beetle drawn as a real little creature: two antennae, six legs
  // in two gait groups, head, pronotum and seamed elytra. Line work follows
  // the shell icon language (round caps, currentColor), so it themes.
  return `
    <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true" focusable="false" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round">
      <g class="ctox-bug-antennae">
        <path d="M10.9 6.1c-.4-.7-.9-1.2-1.5-1.5" />
        <path d="M13.1 6.1c.4-.7.9-1.2 1.5-1.5" />
      </g>
      <g class="ctox-bug-legs ctox-bug-legs-a">
        <path d="M8.2 10.2 6.7 9.3" />
        <path d="M7.8 13.1H6" />
        <path d="m8.3 15.9-1.4 1.2" />
      </g>
      <g class="ctox-bug-legs ctox-bug-legs-b">
        <path d="m15.8 10.2 1.5-.9" />
        <path d="M16.2 13.1H18" />
        <path d="m15.7 15.9 1.4 1.2" />
      </g>
      <g class="ctox-bug-body">
        <circle cx="12" cy="7.4" r="1.5" fill="currentColor" stroke="none" opacity=".9" />
        <path d="M12 8.6c2.4 0 4 1.7 4 4.3 0 2.9-1.7 5-4 5s-4-2.1-4-5c0-2.6 1.6-4.3 4-4.3Z" fill="currentColor" stroke="none" opacity=".8" />
        <path d="M12 8.6c2.4 0 4 1.7 4 4.3 0 2.9-1.7 5-4 5s-4-2.1-4-5c0-2.6 1.6-4.3 4-4.3Z" />
        <path d="M12 8.8v8.9" stroke-width=".9" opacity=".5" />
      </g>
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
    /* The reporter FAB is the standing reminder that an app is never
       finished. It rests as a quiet glass dot in the shell's chat-dock
       language, breathes a soft pulse ring, and unfolds its thought on
       hover. Loud alarm-red is reserved for real danger, not feedback. */
    .ctox-report-fab {
      position: fixed;
      right: 18px;
      bottom: 18px;
      z-index: 40;
      display: inline-flex;
      align-items: center;
      justify-content: flex-start;
      gap: 0;
      height: 40px;
      min-width: 40px;
      max-width: 40px;
      padding: 0 10px;
      overflow: hidden;
      border: 1px solid color-mix(in srgb, var(--line, #3a4149) 55%, transparent);
      background: color-mix(in srgb, var(--surface, #171a1d) 78%, transparent);
      backdrop-filter: blur(16px) saturate(150%);
      -webkit-backdrop-filter: blur(16px) saturate(150%);
      color: var(--muted, #9ba4aa);
      border-radius: 999px;
      font: 650 12px/1.1 ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      box-shadow:
        0 8px 24px rgba(0, 0, 0, .18),
        0 1px 0 rgba(255, 255, 255, .06) inset;
      cursor: pointer;
      transition:
        max-width 260ms cubic-bezier(0.25, 0.8, 0.25, 1),
        color 160ms ease,
        border-color 160ms ease,
        background-color 160ms ease,
        box-shadow 200ms ease;
    }
    .ctox-report-fab::before {
      content: '';
      position: absolute;
      inset: -1px;
      border-radius: inherit;
      pointer-events: none;
      border: 1px solid color-mix(in srgb, var(--accent, #72b8aa) 65%, transparent);
      opacity: 0;
      animation: ctox-report-breathe 9s ease-out infinite;
    }
    @keyframes ctox-report-breathe {
      0%, 88%, 100% { opacity: 0; transform: scale(1); }
      90% { opacity: .8; transform: scale(1); }
      97% { opacity: 0; transform: scale(1.55); }
    }
    /* A tiny living status dot: the beetle is not an error badge but the
       standing "this app keeps evolving" companion — visibly alive even in
       a still screenshot. */
    .ctox-report-fab::after {
      content: '';
      position: absolute;
      top: 5px;
      right: 6px;
      width: 6px;
      height: 6px;
      border-radius: 999px;
      background: var(--accent, #72b8aa);
      box-shadow: 0 0 8px color-mix(in srgb, var(--accent, #72b8aa) 85%, transparent);
      animation: ctox-report-led 4s ease-in-out infinite;
      pointer-events: none;
    }
    @keyframes ctox-report-led {
      0%, 100% { opacity: .55; }
      50% { opacity: 1; }
    }
    .ctox-report-fab svg {
      flex: 0 0 auto;
      color: color-mix(in srgb, var(--accent, #72b8aa) 60%, var(--muted, #9ba4aa));
      transition: color 160ms ease, transform 200ms ease;
    }
    .ctox-report-fab-label {
      white-space: nowrap;
      opacity: 0;
      margin-left: 0;
      transition: opacity 180ms ease 60ms, margin-left 200ms ease;
    }
    .ctox-report-fab:hover,
    .ctox-report-fab:focus-visible {
      max-width: 280px;
      color: var(--text, #e6e9eb);
      border-color: color-mix(in srgb, var(--accent, #72b8aa) 45%, var(--line, #3a4149));
      background: color-mix(in srgb, var(--surface, #171a1d) 92%, transparent);
      box-shadow:
        0 12px 32px rgba(0, 0, 0, .24),
        0 1px 0 rgba(255, 255, 255, .08) inset;
    }
    .ctox-report-fab:hover svg,
    .ctox-report-fab:focus-visible svg {
      color: var(--accent, #72b8aa);
      transform: rotate(-8deg);
    }
    .ctox-report-fab:hover .ctox-report-fab-label,
    .ctox-report-fab:focus-visible .ctox-report-fab-label {
      opacity: 1;
      margin-left: 8px;
    }
    .ctox-report-fab:focus-visible {
      outline: 2px solid color-mix(in srgb, var(--accent, #72b8aa) 70%, transparent);
      outline-offset: 2px;
    }
    @media (prefers-reduced-motion: reduce) {
      .ctox-report-fab::before, .ctox-report-fab::after { animation: none; }
      .ctox-report-fab, .ctox-report-fab svg, .ctox-report-fab-label { transition: none; }
    }
    .ctox-report-fab.bug-crawled-away svg {
      opacity: 0 !important;
      visibility: hidden !important;
      display: none !important;
    }
    /* The strolling beetle lives BELOW windows/taskbar/topbar (z-index 30):
       a desktop creature that never walks over the user's work. */
    .ctox-bug-actor {
      position: fixed;
      z-index: 30;
      pointer-events: auto;
      cursor: pointer;
      width: 26px;
      height: 26px;
      display: inline-flex;
      align-items: center;
      justify-content: center;
      background: transparent;
      color: var(--accent, #72b8aa);
      opacity: 1;
      transition: opacity 260ms ease;
      filter: drop-shadow(0 1px 2px rgba(0, 0, 0, 0.3));
    }
    .ctox-bug-actor.is-appearing { opacity: 0; }
    .ctox-bug-actor svg { width: 15px; height: 15px; overflow: visible; }
    .ctox-bug-actor .ctox-bug-legs,
    .ctox-bug-actor .ctox-bug-antennae {
      transform-origin: 12px 12px;
    }
    .ctox-bug-actor.is-walking .ctox-bug-legs-a {
      animation: ctox-bug-gait var(--bug-gait-ms, 240ms) ease-in-out infinite;
    }
    .ctox-bug-actor.is-walking .ctox-bug-legs-b {
      animation: ctox-bug-gait var(--bug-gait-ms, 240ms) ease-in-out infinite reverse;
    }
    .ctox-bug-actor.is-walking .ctox-bug-body {
      animation: ctox-bug-bob calc(var(--bug-gait-ms, 240ms) * 2) ease-in-out infinite;
    }
    .ctox-bug-actor.is-pausing .ctox-bug-antennae {
      animation: ctox-bug-twitch 1.6s ease-in-out infinite;
    }
    @keyframes ctox-bug-gait {
      0%, 100% { transform: rotate(3.5deg); }
      50% { transform: rotate(-3.5deg); }
    }
    @keyframes ctox-bug-bob {
      0%, 100% { transform: scale(1); }
      50% { transform: scale(1.03); }
    }
    @keyframes ctox-bug-twitch {
      0%, 62%, 100% { transform: rotate(0deg); }
      70% { transform: rotate(4deg); }
      82% { transform: rotate(-3deg); }
    }
    @media (prefers-reduced-motion: reduce) {
      .ctox-bug-actor .ctox-bug-legs,
      .ctox-bug-actor .ctox-bug-body,
      .ctox-bug-actor .ctox-bug-antennae { animation: none !important; }
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
      background: var(--surface, #181c21);
      color: var(--text, #e5e9ee);
      border: 1px solid var(--line, rgba(112, 131, 151, .32));
      border-radius: 12px;
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
    .ctox-report-tagline {
      margin: -6px 0 0;
      color: var(--muted, #9aa4af);
      font-size: 12px;
      line-height: 1.4;
    }
    .ctox-report-dialog header span,
    .ctox-report-dialog label span,
    .ctox-report-dialog footer span {
      display: block;
      color: var(--muted, #9aa4af);
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
      border: 1px solid var(--line, rgba(133, 148, 163, .34));
      border-radius: 6px;
      background: var(--bg, #101318);
      color: var(--text, #edf1f5);
      padding: 9px 10px;
      font: inherit;
    }
    .ctox-report-dialog input:focus,
    .ctox-report-dialog textarea:focus,
    .ctox-report-dialog select:focus {
      outline: 2px solid color-mix(in srgb, var(--accent, #398cc4) 70%, transparent);
      outline-offset: 2px;
      border-color: var(--accent, rgba(57, 140, 196, .82));
    }
    .ctox-report-dialog button,
    .ctox-report-markup-toolbar button {
      border: 0;
      border-radius: 6px;
      background: var(--accent, #596a78);
      color: var(--accent-foreground, #f5f7f9);
      padding: 8px 11px;
      font: inherit;
      cursor: pointer;
    }
    .ctox-report-dialog header button {
      background: transparent;
      color: var(--muted, #a9b1ba);
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
      border: 1px solid var(--line, rgba(133, 148, 163, .24)) !important;
      background: var(--surface-2, #20252b) !important;
      color: var(--text, #f5f7f9) !important;
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
