// CTOX humanlike.mjs — behavioral primitives for Playwright/Patchright.
//
// Algorithms and default constants are ported from CloakHQ/CloakBrowser's
// open-source `cloakbrowser/human/` Python module (MIT, Copyright CloakHQ,
// https://github.com/CloakHQ/CloakBrowser). The patched-Chromium binary
// is NOT used; this file reimplements only the userland wrapper rules.
//
// These primitives lower the "machine-perfect" signal in mouse/keyboard/
// scroll telemetry that behavioral anti-bot scoring uses. They do NOT
// fix structural CDP leaks (use Patchright for that) and they cannot
// change the TLS fingerprint.
//
// Public API:
//   humanMouseMove(page, from, to, options?)        // Bezier path with wobble + overshoot
//   humanClickAt(page, xy, options?)                // aim delay + hold
//   humanType(locator, text, options?)              // mistype + uniform delay + CDP-trusted shift
//   humanScroll(page, deltaY, options?)             // 3-phase burst-wheel inertia
//   humanClickLocator(page, locator, options?)      // actionability + humanMouse + click
//   ensureActionable(locator, checks, options?)     // pre-action gating

export const DEFAULT_HUMAN_CONFIG = Object.freeze({
  // Mouse
  mouseStepsDivisor: 8,
  mouseMinSteps: 25,
  mouseMaxSteps: 80,
  mouseWobbleMax: 1.5,
  mouseOvershootChance: 0.15,
  mouseOvershootPx: [3, 6],
  mouseBurstSize: [3, 5],
  mouseBurstPauseMs: [8, 18],
  // Keyboard
  typingDelayMs: 70,
  typingDelaySpreadMs: 40,
  typingPauseChance: 0.10,
  typingPauseRangeMs: [400, 1000],
  keyHoldMs: [15, 35],
  shiftDownDelayMs: [30, 70],
  shiftUpDelayMs: [20, 50],
  mistypeChance: 0.02,
  mistypeDelayNoticeMs: [60, 180],
  mistypeDelayCorrectMs: [80, 220],
  // Click
  clickAimDelayInputMs: [60, 140],
  clickAimDelayButtonMs: [80, 200],
  clickHoldInputMs: [40, 100],
  clickHoldButtonMs: [60, 150],
  // Scroll
  scrollAccelDeltaPx: [80, 100],
  scrollCruiseDeltaPx: [80, 130],
  scrollDecelDeltaPx: [60, 90],
  scrollPauseFastMs: [30, 80],
  scrollPauseSlowMs: [80, 200],
  scrollBurstChunkPx: [20, 40],
  scrollBurstStepMs: [8, 20],
  scrollBurstChunks: [3, 5],
  scrollOvershootChance: 0.10,
  scrollOvershootPx: [40, 120],
  scrollSettleDelayMs: [100, 250],
  // Actionability
  actionabilityTimeoutMs: 30_000,
  stabilityTimeoutMs: 5_000,
  pointerCheckTimeoutMs: 5_000,
  backoffMs: [100, 250, 500, 1000],
});

function rand(min, max) {
  return Math.random() * (max - min) + min;
}

function randRange([min, max]) {
  return rand(min, max);
}

function randInt([min, max]) {
  return Math.floor(rand(min, max + 1));
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function bezier(t, p0, p1, p2, p3) {
  const u = 1 - t;
  const uu = u * u;
  const uuu = uu * u;
  const tt = t * t;
  const ttt = tt * t;
  return uuu * p0 + 3 * uu * t * p1 + 3 * u * tt * p2 + ttt * p3;
}

function easeInOut(t) {
  return t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;
}

function randomControlPoints(p0, p3) {
  const dx = p3.x - p0.x;
  const dy = p3.y - p0.y;
  const dist = Math.hypot(dx, dy) || 1;
  const perpX = -dy / dist;
  const perpY = dx / dist;
  const maxOffset = 0.3 * dist;
  const off1 = rand(-maxOffset, maxOffset);
  const off2 = rand(-maxOffset, maxOffset);
  return [
    { x: p0.x + 0.25 * dx + perpX * off1, y: p0.y + 0.25 * dy + perpY * off1 },
    { x: p0.x + 0.75 * dx + perpX * off2, y: p0.y + 0.75 * dy + perpY * off2 },
  ];
}

// ─────────────────────────────────────────────────────────────────────────────
// Mouse
// ─────────────────────────────────────────────────────────────────────────────

export async function humanMouseMove(page, from, to, options = {}) {
  const cfg = { ...DEFAULT_HUMAN_CONFIG, ...options };
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const dist = Math.hypot(dx, dy);
  const steps = Math.max(
    cfg.mouseMinSteps,
    Math.min(cfg.mouseMaxSteps, Math.round(dist / cfg.mouseStepsDivisor)),
  );
  const [p1, p2] = randomControlPoints(from, to);
  const burstSize = randInt(cfg.mouseBurstSize);
  let stepsSinceBurst = 0;

  for (let i = 1; i <= steps; i += 1) {
    const tLin = i / steps;
    const t = easeInOut(tLin);
    const x =
      bezier(t, from.x, p1.x, p2.x, to.x) +
      Math.sin(tLin * Math.PI) * rand(-cfg.mouseWobbleMax, cfg.mouseWobbleMax);
    const y =
      bezier(t, from.y, p1.y, p2.y, to.y) +
      Math.sin(tLin * Math.PI) * rand(-cfg.mouseWobbleMax, cfg.mouseWobbleMax);
    await page.mouse.move(x, y);
    stepsSinceBurst += 1;
    if (stepsSinceBurst >= burstSize) {
      await sleep(randRange(cfg.mouseBurstPauseMs));
      stepsSinceBurst = 0;
    }
  }

  // Overshoot + correction
  if (Math.random() < cfg.mouseOvershootChance && dist > 0) {
    const overshoot = randRange(cfg.mouseOvershootPx);
    const ux = dx / dist;
    const uy = dy / dist;
    await page.mouse.move(to.x + ux * overshoot, to.y + uy * overshoot);
    await sleep(randRange(cfg.mouseBurstPauseMs));
    await page.mouse.move(to.x, to.y);
  } else {
    await page.mouse.move(to.x, to.y);
  }
}

export async function humanClickAt(page, xy, options = {}) {
  const cfg = { ...DEFAULT_HUMAN_CONFIG, ...options };
  const kind = options.kind === 'input' ? 'input' : 'button';
  const aim = kind === 'input' ? cfg.clickAimDelayInputMs : cfg.clickAimDelayButtonMs;
  const hold = kind === 'input' ? cfg.clickHoldInputMs : cfg.clickHoldButtonMs;
  await sleep(randRange(aim));
  await page.mouse.down({ button: 'left' });
  await sleep(randRange(hold));
  await page.mouse.up({ button: 'left' });
}

// ─────────────────────────────────────────────────────────────────────────────
// Keyboard
// ─────────────────────────────────────────────────────────────────────────────

const NEARBY_KEYS = {
  a: ['s', 'q', 'w', 'z'], b: ['v', 'g', 'h', 'n'], c: ['x', 'd', 'f', 'v'],
  d: ['s', 'e', 'r', 'f', 'c', 'x'], e: ['w', 's', 'd', 'r'], f: ['d', 'r', 't', 'g', 'v', 'c'],
  g: ['f', 't', 'y', 'h', 'b', 'v'], h: ['g', 'y', 'u', 'j', 'n', 'b'],
  i: ['u', 'j', 'k', 'o'], j: ['h', 'u', 'i', 'k', 'm', 'n'], k: ['j', 'i', 'o', 'l', 'm'],
  l: ['k', 'o', 'p'], m: ['n', 'j', 'k'], n: ['b', 'h', 'j', 'm'],
  o: ['i', 'k', 'l', 'p'], p: ['o', 'l'], q: ['w', 'a'], r: ['e', 'd', 'f', 't'],
  s: ['a', 'w', 'e', 'd', 'x', 'z'], t: ['r', 'f', 'g', 'y'], u: ['y', 'h', 'j', 'i'],
  v: ['c', 'f', 'g', 'b'], w: ['q', 'a', 's', 'e'], x: ['z', 's', 'd', 'c'],
  y: ['t', 'g', 'h', 'u'], z: ['a', 's', 'x'],
  '1': ['2', 'q'], '2': ['1', '3', 'q', 'w'], '3': ['2', '4', 'w', 'e'],
  '4': ['3', '5', 'e', 'r'], '5': ['4', '6', 'r', 't'], '6': ['5', '7', 't', 'y'],
  '7': ['6', '8', 'y', 'u'], '8': ['7', '9', 'u', 'i'], '9': ['8', '0', 'i', 'o'],
  '0': ['9', 'o', 'p'],
};

function nearbyKey(ch) {
  const lower = ch.toLowerCase();
  const candidates = NEARBY_KEYS[lower];
  if (!candidates || candidates.length === 0) return null;
  const pick = candidates[Math.floor(Math.random() * candidates.length)];
  return ch === lower ? pick : pick.toUpperCase();
}

function isPrintableSingleKey(ch) {
  // ASCII printable range — Playwright's keyboard.press accepts these
  // directly. Anything else (CJK, emoji, RTL combinings) goes through
  // type(), which routes via Input.insertText with isTrusted=true.
  const code = ch.codePointAt(0);
  return code !== undefined && code >= 0x20 && code <= 0x7e;
}

export async function humanType(locator, text, options = {}) {
  const cfg = { ...DEFAULT_HUMAN_CONFIG, ...options };
  const page = locator.page ? locator.page() : options.page;
  if (!page) throw new Error('humanType requires a locator or options.page');
  await locator.focus();

  for (const ch of text) {
    if (/[A-Za-z0-9]/.test(ch) && Math.random() < cfg.mistypeChance) {
      const wrong = nearbyKey(ch);
      if (wrong) {
        await page.keyboard.press(wrong, { delay: randRange(cfg.keyHoldMs) });
        await sleep(randRange(cfg.mistypeDelayNoticeMs));
        await page.keyboard.press('Backspace', { delay: randRange(cfg.keyHoldMs) });
        await sleep(randRange(cfg.mistypeDelayCorrectMs));
      }
    }

    if (isPrintableSingleKey(ch)) {
      await page.keyboard.press(ch, { delay: randRange(cfg.keyHoldMs) });
    } else {
      await page.keyboard.type(ch);
    }

    if (Math.random() < cfg.typingPauseChance) {
      await sleep(randRange(cfg.typingPauseRangeMs));
    } else {
      const base = cfg.typingDelayMs;
      const spread = cfg.typingDelaySpreadMs;
      const delay = Math.max(10, base + (Math.random() - 0.5) * 2 * spread);
      await sleep(delay);
    }
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Scroll — burst-wheel inertia + 3-phase motion + overshoot
// ─────────────────────────────────────────────────────────────────────────────

async function burstWheel(page, deltaY, cfg) {
  const chunks = randInt(cfg.scrollBurstChunks);
  const direction = Math.sign(deltaY) || 1;
  const abs = Math.abs(deltaY);
  const chunkBudgets = [];
  let remaining = abs;
  for (let i = 0; i < chunks - 1; i += 1) {
    const c = Math.min(remaining, randRange(cfg.scrollBurstChunkPx));
    chunkBudgets.push(c);
    remaining -= c;
  }
  if (remaining > 0) chunkBudgets.push(remaining);
  for (const c of chunkBudgets) {
    await page.mouse.wheel(0, direction * c);
    await sleep(randRange(cfg.scrollBurstStepMs));
  }
}

export async function humanScroll(page, deltaY, options = {}) {
  const cfg = { ...DEFAULT_HUMAN_CONFIG, ...options };
  const direction = Math.sign(deltaY) || 1;
  let remaining = Math.abs(deltaY);
  if (remaining === 0) return;

  // Plan 3 phases: accel (first ~20%), cruise (middle 60%), decel (last 20%).
  // CloakBrowser uses fixed step counts; we adapt to remaining distance.
  const accelBudget = Math.floor(remaining * 0.2);
  const decelBudget = Math.floor(remaining * 0.2);
  const cruiseBudget = remaining - accelBudget - decelBudget;

  async function runPhase(budget, deltaRange, pauseRange) {
    let left = budget;
    while (left > 0) {
      const step = Math.min(left, randRange(deltaRange));
      const variance = step * (Math.random() - 0.5);
      await burstWheel(page, direction * (step + variance), cfg);
      await sleep(randRange(pauseRange));
      left -= step;
    }
  }

  await runPhase(accelBudget, cfg.scrollAccelDeltaPx, cfg.scrollPauseSlowMs);
  await runPhase(cruiseBudget, cfg.scrollCruiseDeltaPx, cfg.scrollPauseFastMs);
  await runPhase(decelBudget, cfg.scrollDecelDeltaPx, cfg.scrollPauseSlowMs);

  if (Math.random() < cfg.scrollOvershootChance) {
    const overshoot = randRange(cfg.scrollOvershootPx);
    await burstWheel(page, direction * overshoot, cfg);
    await sleep(randRange(cfg.scrollSettleDelayMs));
    const corrections = randInt([1, 2]);
    for (let i = 0; i < corrections; i += 1) {
      await burstWheel(page, -direction * (overshoot / corrections), cfg);
      await sleep(randRange(cfg.scrollSettleDelayMs));
    }
  }
  await sleep(randRange(cfg.scrollSettleDelayMs));
}

// ─────────────────────────────────────────────────────────────────────────────
// Actionability — pre-action gating from cloakbrowser/human/actionability.py
// ─────────────────────────────────────────────────────────────────────────────

export const ACTIONABILITY_CHECKS = Object.freeze({
  click: ['attached', 'visible', 'enabled', 'pointerEvents'],
  hover: ['attached', 'visible', 'pointerEvents'],
  input: ['attached', 'visible', 'enabled', 'editable', 'pointerEvents'],
  focus: ['attached', 'visible', 'enabled'],
});

async function backoffSleep(attempt, cfg) {
  const idx = Math.min(attempt, cfg.backoffMs.length - 1);
  await sleep(cfg.backoffMs[idx]);
}

async function checkOnce(locator, check) {
  switch (check) {
    case 'attached': {
      const count = await locator.count();
      return count >= 1;
    }
    case 'visible':
      return locator.isVisible();
    case 'enabled':
      return locator.isEnabled();
    case 'editable':
      return locator.isEditable();
    case 'pointerEvents': {
      const box = await locator.boundingBox();
      if (!box) return false;
      const cx = box.x + box.width / 2;
      const cy = box.y + box.height / 2;
      const handle = await locator.elementHandle();
      if (!handle) return false;
      try {
        return await locator.page().evaluate(
          ([el, x, y]) => {
            const hit = document.elementFromPoint(x, y);
            if (!hit) return false;
            return hit === el || el.contains(hit);
          },
          [handle, cx, cy],
        );
      } finally {
        await handle.dispose();
      }
    }
    default:
      return true;
  }
}

export async function ensureActionable(locator, checks, options = {}) {
  const cfg = { ...DEFAULT_HUMAN_CONFIG, ...options };
  const deadline = Date.now() + cfg.actionabilityTimeoutMs;
  let attempt = 0;
  while (Date.now() < deadline) {
    let allOk = true;
    for (const check of checks) {
      const ok = await checkOnce(locator, check);
      if (!ok) {
        allOk = false;
        break;
      }
    }
    if (allOk) return;
    await backoffSleep(attempt, cfg);
    attempt += 1;
  }
  throw new Error(`ensureActionable: timed out for checks ${checks.join(',')}`);
}

async function ensureStable(locator, options = {}) {
  const cfg = { ...DEFAULT_HUMAN_CONFIG, ...options };
  const deadline = Date.now() + cfg.stabilityTimeoutMs;
  let prev = await locator.boundingBox();
  while (Date.now() < deadline) {
    await sleep(100);
    const next = await locator.boundingBox();
    if (prev && next &&
        prev.x === next.x && prev.y === next.y &&
        prev.width === next.width && prev.height === next.height) {
      return next;
    }
    prev = next;
  }
  throw new Error('ensureStable: bounding box did not settle');
}

// ─────────────────────────────────────────────────────────────────────────────
// High-level click — actionability → human move → human click
// ─────────────────────────────────────────────────────────────────────────────

export async function humanClickLocator(page, locator, options = {}) {
  await ensureActionable(locator, ACTIONABILITY_CHECKS.click, options);
  const box = await ensureStable(locator, options);
  const targetX = box.x + box.width * rand(0.35, 0.65);
  const targetY = box.y + box.height * rand(0.35, 0.65);
  const start = options.from || { x: targetX - rand(80, 240), y: targetY - rand(40, 140) };
  await humanMouseMove(page, start, { x: targetX, y: targetY }, options);
  await humanClickAt(page, { x: targetX, y: targetY }, options);
}
