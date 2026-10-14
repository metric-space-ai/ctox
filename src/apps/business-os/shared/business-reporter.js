import { withTimeout } from './async-timeout.js';

const REPORTER_STYLE_ID = 'ctox-business-reporter-style';
// The first save of a session has to register the report schemas before the
// first document can be written. That cold start used to run inside the very
// first submit and looked like a hang, so the deadlines are generous and the
// dialog retries once on its own before it bothers the user.
const REPORTER_PREPARE_TIMEOUT_MS = 30000;
const REPORTER_SUBMIT_TIMEOUT_MS = 45000;
const REPORTER_RETRY_DELAY_MS = 600;
// Screen capture can stall forever: a display picker that is never answered
// leaves its promise pending, and with it the capture layer over the desktop.
// Every capture therefore runs against a deadline and falls back to the plain
// markup image.
const REPORTER_CAPTURE_TIMEOUT_MS = 45000;
const REPORTER_TOAST_MS = 4200;
const DRAFT_FIELDS = Object.freeze(['kind', 'severity', 'title', 'summary', 'expected']);
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
      fabAria: 'Save feedback: report a bug or feature',
      tagline: 'An app is never finished. Save the report here first, then manage or delegate it from Bugs & Features.',
      saving: 'Saving to Bugs & Features...',
      retrying: 'Saving takes longer than usual. Trying again...',
      savedToast: 'Report saved. You can manage or delegate it in Bugs & Features.',
      keepOpenHint: 'Your text is kept. Use the X in the header to close this.',
      failedTimeout: 'Saving took too long. Your text is kept — please send again.',
    }
    : {
      fabLabel: 'Eine App ist nie fertig',
      fabTitle: 'Eine App ist nie fertig — sag CTOX, was besser werden soll.',
      fabAria: 'Feedback speichern: Bug oder Feature melden',
      tagline: 'Eine App ist nie fertig. Speichere den Hinweis zuerst hier und verwalte oder delegiere ihn danach in Bugs & Features.',
      saving: 'Speichere in Bugs & Features...',
      retrying: 'Das Speichern dauert länger als sonst. Neuer Versuch läuft...',
      savedToast: 'Report gespeichert. Du kannst ihn in Bugs & Features verwalten oder delegieren.',
      keepOpenHint: 'Dein Text bleibt erhalten. Zum Schließen bitte das X oben rechts benutzen.',
      failedTimeout: 'Speichern hat zu lange gedauert. Dein Text bleibt erhalten – bitte noch einmal senden.',
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
  db = null,
  sync = null,
  ensureReportCollections = null,
  idleMs = IDLE_TIME,
  captureTimeoutMs = REPORTER_CAPTURE_TIMEOUT_MS,
}) {
  if (!session?.authenticated || document.querySelector('[data-ctox-reporter]')) return;
  idleDelay = Math.max(1000, Number(idleMs) || IDLE_TIME);
  installReporterStyles();
  reporterState = {
    session,
    getActiveModule,
    db,
    sync,
    ensureReportCollections,
    modal: null,
    overlay: null,
    attachment: null,
    markupMode: 'idle',
    selectionOrigin: null,
    selectionRect: null,
    strokes: [],
    activeStroke: null,
    savingMarkup: false,
    // The draft is the source of truth for what the user typed. It outlives
    // the dialog element, so neither the markup overlay nor an accidental
    // teardown of the backdrop can destroy the text.
    draft: createEmptyDraft(),
    dialogOpen: false,
    submitting: false,
    pendingReportIdentity: null,
    collectionsReady: null,
    overlayKeyHandler: null,
    captureTimeoutMs: Math.max(1000, Number(captureTimeoutMs) || REPORTER_CAPTURE_TIMEOUT_MS),
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

export function resolveBusinessReporterModule({
  activeModule = null,
  modules = [],
  windowManager = null,
} = {}) {
  const focusedWindow = windowManager?.listWindows?.()
    ?.find((entry) => entry?.isFocused && entry?.state !== 'minimized');
  const ownerId = String(focusedWindow?.ownerId || '');
  const focusedModuleId = ownerId.replace(/^(?:desktop-app|module):/, '');
  if (focusedModuleId && focusedModuleId !== ownerId) {
    const catalogModule = modules.find((entry) => entry?.id === focusedModuleId);
    if (catalogModule) return catalogModule;
    return {
      id: focusedModuleId,
      title: String(focusedWindow?.title || focusedModuleId).trim() || focusedModuleId,
    };
  }
  return activeModule || { id: 'ctox', title: 'CTOX' };
}

function createEmptyDraft() {
  return { kind: 'bug', severity: 'medium', title: '', summary: '', expected: '' };
}

function reporterForm(state) {
  const modal = state.modal;
  if (!modal || (typeof modal.isConnected === 'boolean' && !modal.isConnected)) return null;
  return modal.querySelector?.('[data-report-form]') || null;
}

function isBlankValue(value) {
  return String(value ?? '').trim() === '';
}

/**
 * Read what is currently in the dialog back into the durable draft. Called on
 * every input event and before anything hides, replaces or removes the
 * dialog, so the text survives every transition.
 */
function captureDraft(state) {
  const form = reporterForm(state);
  if (!form) return state.draft;
  for (const field of DRAFT_FIELDS) {
    const control = form.querySelector?.(`[name="${field}"]`);
    if (!control) continue;
    const value = String(control.value ?? '');
    if (field === 'kind' || field === 'severity') {
      if (value) state.draft[field] = value;
    } else {
      state.draft[field] = value;
    }
  }
  return state.draft;
}

/**
 * The only write path into the dialog fields, and it is deliberately
 * fill-empty-only: a value coming from anywhere but the keyboard may seed an
 * empty field, never replace text the user has already written.
 */
function applyDraftToForm(form, draft) {
  if (!form || !draft) return;
  for (const field of DRAFT_FIELDS) {
    const control = form.querySelector?.(`[name="${field}"]`);
    if (!control) continue;
    const incoming = String(draft[field] ?? '');
    if (!incoming) continue;
    if (field === 'kind' || field === 'severity') {
      control.value = incoming;
      continue;
    }
    if (!isBlankValue(control.value)) continue;
    control.value = incoming;
  }
}

/**
 * Supported entry point for machine-generated context (module hints, captured
 * error text, …). It can only seed fields that are still empty; user input is
 * never replaced.
 */
export function prefillBusinessReporterDraft(values = {}, state = reporterState) {
  if (!state) return null;
  captureDraft(state);
  for (const field of DRAFT_FIELDS) {
    const incoming = values[field];
    if (incoming === undefined || incoming === null) continue;
    const text = String(incoming);
    if (!text) continue;
    if (field === 'kind' || field === 'severity') {
      state.draft[field] = text;
      continue;
    }
    if (!isBlankValue(state.draft[field])) continue;
    state.draft[field] = text;
  }
  applyDraftToForm(reporterForm(state), state.draft);
  return { ...state.draft };
}

function draftHasContent(state) {
  captureDraft(state);
  return !isBlankValue(state.draft.title)
    || !isBlankValue(state.draft.summary)
    || !isBlankValue(state.draft.expected)
    || Boolean(state.attachment);
}

/**
 * Registering the report schemas is the cold start that made the very first
 * submit look like a hang. Warming it once when the dialog opens — and
 * reusing that promise for the submit — keeps the first save as fast as the
 * second one.
 */
function warmReportCollections(state) {
  if (typeof state.ensureReportCollections !== 'function') return Promise.resolve();
  if (!state.collectionsReady) {
    state.collectionsReady = Promise.resolve()
      .then(() => state.ensureReportCollections())
      .catch((error) => {
        state.collectionsReady = null;
        throw error;
      });
  }
  return state.collectionsReady;
}

function showReporterToast(message, tone = 'success') {
  if (!message || typeof document === 'undefined' || !document.body) return;
  const toast = document.createElement('div');
  toast.className = 'ctox-report-toast';
  toast.dataset.tone = tone;
  toast.setAttribute('role', 'status');
  toast.setAttribute('aria-live', 'polite');
  toast.textContent = message;
  document.body.append(toast);
  requestAnimationFrame(() => toast.classList.add('is-visible'));
  setTimeout(() => {
    toast.classList.remove('is-visible');
    setTimeout(() => toast.remove(), 260);
  }, REPORTER_TOAST_MS);
}

function setReporterStatus(state, text) {
  const status = reporterForm(state)?.querySelector?.('[data-status]');
  if (status) status.textContent = text || '';
}

function openReporterDialog(state) {
  if (!state) return;
  // A second open must never wipe a dialog that is already collecting text.
  if (state.modal && (typeof state.modal.isConnected !== 'boolean' || state.modal.isConnected)) {
    state.dialogOpen = true;
    state.modal.hidden = false;
    state.modal.style.display = '';
    state.modal.querySelector('input[name="title"]')?.focus();
    return;
  }
  const module = state.getActiveModule?.() || { id: 'ctox', title: 'CTOX' };
  const backdrop = document.createElement('div');
  backdrop.className = 'ctox-report-backdrop';
  backdrop.innerHTML = `
    <form class="ctox-report-dialog" data-report-form>
      <header>
        <div>
          <strong>Bug oder Feature erfassen</strong>
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
        <textarea name="expected" rows="3" placeholder="Was sollte stattdessen passieren?"></textarea>
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
        <button type="submit">In Bugs & Features speichern</button>
      </footer>
    </form>
  `;
  state.modal = backdrop;
  state.dialogOpen = true;
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
  // Keep the durable draft in step with every keystroke.
  const form = backdrop.querySelector('[data-report-form]');
  form?.addEventListener('input', () => captureDraft(state));
  form?.addEventListener('change', () => captureDraft(state));
  // The dialog closes on explicit intent only. A backdrop click counts as
  // intent when nothing is written yet; once there is text, a stray click
  // that merely ends on the backdrop (text selection drags do this) must not
  // throw the report away.
  let backdropPointerDown = false;
  backdrop.addEventListener('pointerdown', (event) => {
    backdropPointerDown = event.target === backdrop;
  });
  backdrop.addEventListener('click', (event) => {
    const startedOnBackdrop = backdropPointerDown;
    backdropPointerDown = false;
    if (event.target !== backdrop || !startedOnBackdrop) return;
    requestCloseReporterDialog(state);
  });
  backdrop.addEventListener('keydown', (event) => {
    if (event.key !== 'Escape' || state.markupMode !== 'idle') return;
    event.stopPropagation();
    requestCloseReporterDialog(state);
  });
  form?.addEventListener('submit', async (event) => {
    event.preventDefault();
    await submitReport(state, module, event.currentTarget);
  });
  document.body.append(backdrop);
  applyDraftToForm(form, state.draft);
  syncAttachmentPreview(state);
  // Pay the schema cold start while the user is still typing.
  warmReportCollections(state).catch(() => {});
  backdrop.querySelector('input[name="title"]')?.focus();
}

/**
 * Soft close for the implicit gestures (backdrop click, Escape). With text in
 * the form it refuses and says how to close on purpose; the X button and a
 * successful save call closeReporterDialog directly.
 */
function requestCloseReporterDialog(state) {
  if (state.submitting) return;
  if (draftHasContent(state)) {
    setReporterStatus(state, reporterCopy().keepOpenHint);
    return;
  }
  closeReporterDialog(state);
}

function closeReporterDialog(state, { keepDraft = true } = {}) {
  captureDraft(state);
  state.dialogOpen = false;
  destroyMarkupOverlay(state);
  state.modal?.remove();
  state.modal = null;
  if (!keepDraft) {
    state.draft = createEmptyDraft();
    state.attachment = null;
    state.pendingReportIdentity = null;
  }
}

async function submitReport(state, module, form) {
  if (state.submitting) return;
  const copy = reporterCopy();
  const status = form.querySelector('[data-status]');
  const submit = form.querySelector('button[type="submit"]');
  captureDraft(state);
  const now = Date.now();
  const title = state.draft.title.trim() || 'Business OS report';
  const summary = state.draft.summary.trim();
  const expected = state.draft.expected.trim();
  const kind = state.draft.kind || 'bug';
  const severity = state.draft.severity || 'medium';
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
  // A retried submit reuses the same report id, so the second attempt updates
  // the same record instead of creating a duplicate.
  const identity = state.pendingReportIdentity || {
    reportId: `report_${newId()}`,
  };
  state.pendingReportIdentity = identity;
  state.submitting = true;
  submit.disabled = true;
  if (status) status.textContent = copy.saving;

  const attempt = () => withTimeout(
    () => saveBusinessReportLocally({
      db: state.db,
      sync: state.sync,
      session: state.session,
      module,
      kind,
      severity,
      title,
      summary,
      expected,
      clientContext,
      now,
      reportId: identity.reportId,
    }),
    REPORTER_SUBMIT_TIMEOUT_MS,
    { message: 'business report save exceeded its deadline' },
  );

  try {
    await withTimeout(
      () => warmReportCollections(state),
      REPORTER_PREPARE_TIMEOUT_MS,
      { message: 'business report store preparation exceeded its deadline' },
    );
    let result;
    try {
      result = await attempt();
    } catch (firstError) {
      // Silent second try: cold bridges and a still-registering store are the
      // known first-submit failure, and the draft stays untouched meanwhile.
      console.warn('[business-reporter] first save attempt failed, retrying once', firstError);
      if (status) status.textContent = copy.retrying;
      state.collectionsReady = null;
      await new Promise((resolve) => setTimeout(resolve, REPORTER_RETRY_DELAY_MS));
      await warmReportCollections(state).catch(() => {});
      result = await attempt();
    }
    notifyReportsUpdated(result.report_id || identity.reportId, module.id || '');
    if (status) status.textContent = reporterStatusText(result);
    // Confirmed: only now may the draft be dropped and the dialog closed.
    closeReporterDialog(state, { keepDraft: false });
    showReporterToast(copy.savedToast, 'success');
  } catch (error) {
    // Draft, attachment and report id survive so the user can simply send again.
    submit.disabled = false;
    if (status) status.textContent = reporterErrorText(error);
  } finally {
    state.submitting = false;
    // No capture overlay may outlive a submit — success, failure or timeout.
    destroyMarkupOverlay(state);
  }
}

export async function saveBusinessReportLocally({
  db,
  sync = null,
  session,
  module,
  kind = 'bug',
  severity = 'medium',
  title = 'Business OS report',
  summary = '',
  expected = '',
  clientContext = {},
  now = Date.now(),
  reportId = '',
}) {
  const resolvedReportId = String(reportId || '').trim() || `report_${newId()}`;
  const result = {
    ok: true,
    report_id: resolvedReportId,
    command_id: '',
    task_id: '',
    task_status: 'not_delegated',
    status: 'open',
    report_status: 'open',
    delivery_status: 'not_delegated',
    transport: 'rxdb-webrtc',
  };
  await persistLocalBusinessReport({
    db,
    sync,
    session,
    report: {
      result,
      module,
      kind,
      severity,
      title,
      summary,
      expected,
      clientContext,
      now,
    },
  });
  return result;
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
  return result?.report_id
    ? 'In Bugs & Features gespeichert. Dort kannst du den Report verwalten oder delegieren.'
    : 'Report konnte nicht gespeichert werden.';
}

function reporterErrorText(error) {
  console.warn('[business-reporter] send failed', error);
  if (error?.name === 'OperationTimeoutError') return reporterCopy().failedTimeout;
  return document.documentElement.lang === 'en' ? 'Sending did not work.' : 'Senden hat nicht geklappt.';
}

export async function persistLocalBusinessReport({ db, sync = null, session = null, report }) {
  const moduleReports = db?.collection?.('business_module_reports')
    || db?.raw?.business_module_reports
    || null;
  const bugReports = db?.collection?.('ctox_bug_reports')
    || db?.raw?.ctox_bug_reports
    || null;
  if (!moduleReports || !bugReports) {
    throw new Error('Bugs & Features ist noch nicht bereit. Bitte erneut senden.');
  }
  // Report visibility is local-first. Starting or catching up the WebRTC
  // bridges must never block the write: delegation is an explicit later
  // action from Bugs & Features, not part of this persistence step.
  const id = report.result?.report_id || `report_${crypto.randomUUID?.() || Date.now()}`;
  const taskId = report.result?.task_id || '';
  const commandId = report.result?.command_id || '';
  const reportStatus = report.result?.report_status || 'open';
  const deliveryStatus = report.result?.delivery_status
    || (taskId ? 'accepted' : (commandId ? 'pending_sync' : 'not_delegated'));
  const clientContext = {
    ...(report.clientContext || {}),
    report_delivery: {
      status: deliveryStatus,
      command_id: commandId,
      task_id: taskId,
    },
  };
  const common = {
    id,
    report_id: id,
    module_id: report.module.id || 'ctox',
    kind: report.kind,
    severity: report.severity,
    title: report.title,
    summary: report.summary,
    expected: report.expected,
    status: reportStatus,
    reporter_id: session?.user?.id || '',
    ctox_command_id: commandId,
    task_id: taskId,
    inbound_channel: report.module.id || 'ctox',
    client_context: clientContext,
    created_at_ms: report.now,
    updated_at_ms: report.now,
  };
  await upsertRx(moduleReports, common);
  await upsertRx(bugReports, {
    id,
    title: report.title,
    status: reportStatus,
    module: report.module.id || 'ctox',
    inbound_channel: report.module.id || 'ctox',
    severity: report.severity,
    surface: 'business-os',
    description: report.summary,
    evidence: clientContext,
    payload: {
      kind: report.kind,
      expected: report.expected,
      ctox_command_id: commandId,
      task_id: taskId,
      delivery_status: deliveryStatus,
      change_summary: '',
      rollback_version_id: '',
    },
    created_at_ms: report.now,
    updated_at_ms: report.now,
  });
  void waitForReportSync(sync);
  return true;
}

function notifyReportsUpdated(reportId, moduleId) {
  window.dispatchEvent(new CustomEvent('ctox-business-os-reports-updated', {
    detail: { reportId: reportId || '', moduleId: moduleId || '' },
  }));
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
  // Freeze what is typed before the dialog goes out of sight: from here on
  // the draft, not the DOM, is what carries the report.
  captureDraft(state);
  state.markupMode = 'selecting';
  state.selectionOrigin = null;
  state.selectionRect = null;
  state.strokes = [];
  state.activeStroke = null;
  hideReporterChrome(state);
  renderMarkupOverlay(state);
}

function cancelMarkup(state) {
  destroyMarkupOverlay(state);
}

/**
 * Single teardown for the capture overlay. It also sweeps stray overlay nodes
 * so no capture layer can be left sitting over the desktop, and it always
 * brings the reporter back.
 */
function destroyMarkupOverlay(state) {
  state.markupMode = 'idle';
  state.selectionOrigin = null;
  state.selectionRect = null;
  state.strokes = [];
  state.activeStroke = null;
  state.savingMarkup = false;
  if (state.overlayKeyHandler) {
    document.removeEventListener('keydown', state.overlayKeyHandler, true);
    state.overlayKeyHandler = null;
  }
  state.overlay?.remove();
  state.overlay = null;
  document.querySelectorAll?.('.ctox-report-markup-overlay')?.forEach?.((node) => node.remove());
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

/**
 * Bring the reporter back after a capture. If the dialog element did not
 * survive the round trip, it is rebuilt from the draft — with the text and
 * the fresh attachment — instead of leaving the user with nothing.
 */
function showReporterChrome(state) {
  const fab = document.querySelector('[data-ctox-reporter]');
  if (fab) fab.style.display = '';
  const modalAlive = state.modal
    && (typeof state.modal.isConnected !== 'boolean' || state.modal.isConnected);
  if (modalAlive) {
    state.modal.style.display = '';
    if (state.modal.dataset.wasOpen === '1') state.modal.hidden = false;
    delete state.modal.dataset.wasOpen;
    applyDraftToForm(reporterForm(state), state.draft);
    syncAttachmentPreview(state);
    return;
  }
  state.modal = null;
  if (!state.dialogOpen) return;
  openReporterDialog(state);
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
  // Escape must abort the capture, never leave the layer stranded.
  state.overlayKeyHandler = (event) => {
    if (event.key !== 'Escape') return;
    event.preventDefault();
    event.stopPropagation();
    cancelMarkup(state);
  };
  document.addEventListener('keydown', state.overlayKeyHandler, true);
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
  const deadline = state.captureTimeoutMs || REPORTER_CAPTURE_TIMEOUT_MS;
  // The outer bound keeps headroom over the picker deadline so the DOM
  // fallback still gets its turn when the picker itself timed out.
  const outerDeadline = deadline + Math.max(2000, Math.round(deadline / 3));
  try {
    const captured = await withTimeout(async () => {
      const screenDataUrl = await captureScreenRegion(rect, deadline).catch(() => null);
      const domDataUrl = screenDataUrl ? null : await captureDomRegion(rect).catch(() => null);
      const screenshotDataUrl = screenDataUrl || domDataUrl;
      const compositeDataUrl = screenshotDataUrl
        ? await buildCompositeDataUrl(rect, finalStrokes, screenshotDataUrl).catch(() => markupSvgDataUrl)
        : markupSvgDataUrl;
      return {
        screenshotDataUrl,
        compositeDataUrl,
        captureMode: screenDataUrl ? 'screen' : domDataUrl ? 'dom' : 'markup-only',
      };
    }, outerDeadline, { message: 'screen capture exceeded its deadline' }).catch((error) => {
      console.warn('[business-reporter] screen capture unavailable, keeping the markup only', error);
      return { screenshotDataUrl: null, compositeDataUrl: markupSvgDataUrl, captureMode: 'markup-only' };
    });
    state.attachment = {
      rect,
      strokes: finalStrokes,
      screenshotDataUrl: captured.screenshotDataUrl,
      markupSvgDataUrl,
      compositeDataUrl: captured.compositeDataUrl,
      captureMode: captured.captureMode,
      capturedAt: new Date().toISOString(),
    };
    syncAttachmentPreview(state);
  } catch (error) {
    console.warn('[business-reporter] markup capture failed', error);
  } finally {
    // The overlay always goes away, and the dialog always comes back with
    // the text that was typed before the capture started.
    destroyMarkupOverlay(state);
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

async function captureScreenRegion(rect, timeoutMs = REPORTER_CAPTURE_TIMEOUT_MS) {
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
  // A picker that is never answered must not hold the capture layer hostage.
  const pending = Promise.resolve()
    .then(() => navigator.mediaDevices.getDisplayMedia({ video: { displaySurface: 'browser' }, audio: false }));
  try {
    stream = await withTimeout(() => pending, timeoutMs, { message: 'display capture picker exceeded its deadline' });
  } catch {
    // If the picker answers after the deadline, release the stream anyway.
    pending.then((late) => late?.getTracks?.().forEach((track) => track.stop())).catch(() => {});
    return null;
  }
  if (!stream) return null;
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
      /* Die Crew-Leiste laeuft ueber die volle Breite und ist 56px hoch (unten
         18px). 26px setzt den 40px hohen Melder mittig in ihr rechtes Ende,
         statt ihn darunter kleben zu lassen. */
      bottom: 26px;
      /* Shell windows live at z-index 50. The standing reporter must remain
         clickable above them while staying below shell menus and dialogs. */
      z-index: 220;
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
      z-index: 280;
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
    /* Confirmation lives above the dialog layer so it is still readable in
       the moment the dialog closes itself after a successful save. */
    .ctox-report-toast {
      position: fixed;
      right: 18px;
      bottom: 72px;
      z-index: 300;
      max-width: min(420px, calc(100vw - 36px));
      padding: 11px 14px;
      border: 1px solid color-mix(in srgb, var(--accent, #72b8aa) 45%, var(--line, #3a4149));
      border-radius: 10px;
      background: color-mix(in srgb, var(--surface, #171a1d) 96%, transparent);
      color: var(--text, #e6e9eb);
      font: 13px/1.4 ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      box-shadow: 0 14px 34px rgba(0, 0, 0, .32);
      opacity: 0;
      transform: translateY(8px);
      transition: opacity 200ms ease, transform 200ms ease;
      pointer-events: none;
    }
    .ctox-report-toast.is-visible {
      opacity: 1;
      transform: translateY(0);
    }
    .ctox-report-toast[data-tone="error"] {
      border-color: rgba(239, 68, 68, .55);
    }
    @media (prefers-reduced-motion: reduce) {
      .ctox-report-toast { transition: none; transform: none; }
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
