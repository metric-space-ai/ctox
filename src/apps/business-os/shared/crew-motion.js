/** Crew motion: one page-wide engine that keeps every crew creature alive.
 *
 * Every `.ctox-crew-creature` in the document — chat bar, chat windows, the
 * CTOX map, tickets, app badges, drag ghosts — is picked up automatically
 * (MutationObserver) and animated from three layers:
 *
 *   base    a continuous, procedural pose for the creature's state (breathing
 *           in sleep, busy bobbing while working, peering in review, …),
 *           shaped by the genome's temperament (tempo, amplitude, irregularity)
 *   impulse a finite gesture triggered ONLY by durable telemetry: a hop for a
 *           tool turn, a tilt for a thinking turn, a sway in review, a stretch
 *           when waking, a shake when failing
 *   face    blinks and glances on the eyes group
 *
 * Mode changes blend from the last pose instead of cutting. Only visible
 * creatures in a visible tab are animated; `prefers-reduced-motion` stops all
 * of it. The body moves as one element (the figure <svg>), so the per-frame
 * work stays on the compositor; eye moves are rare and short.
 */

import { CREW_CREATURE_CSS } from './crew-renderer.js?v=20260927-crew-genome-v1';

const ENGINE_KEY = '__ctoxCrewMotionEngine';
const STYLE_ID = 'ctox-crew-creature-css';
const CREATURE_SELECTOR = '.ctox-crew-creature';
const CALM_MODES = new Set(['sleeping', 'failed']);
const OPEN_EYE_MODES = new Set(['working', 'review', 'learning', 'waiting']);
const TWO_PI = Math.PI * 2;
const MEMORY_TTL_MS = 10 * 60 * 1000;
const FRESH_EVENT_MS = 8000;
const TRANSITION_MS = 480;

const clamp = (value, min, max) => Math.max(min, Math.min(max, value));
const smooth = (value) => value * value * (3 - 2 * value);
const bump = (p) => (p <= 0 || p >= 1 ? 0 : Math.sin(Math.PI * p));
const wave = (t, period, offset = 0) => Math.sin((TWO_PI * t) / period + offset);
const breath = (t, period, offset = 0) => (1 - Math.cos((TWO_PI * t) / period + offset)) / 2;
// Smooth, non-repeating drift from incommensurate sines.
const drift = (t) => (Math.sin(t) + 0.6 * Math.sin(t * 1.618 + 1.3) + 0.3 * Math.sin(t * 2.718 + 0.4)) / 1.9;

function hashUnit(value, salt) {
  let hash = 2166136261 ^ salt;
  const input = String(value || '');
  for (let index = 0; index < input.length; index += 1) {
    hash ^= input.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0) / 4294967296;
}

function restPose() {
  return { x: 0, y: 0, r: 0, sx: 1, sy: 1, gs: 1, go: 1, ex: 0, ey: 0, eb: 1 };
}

function mixPose(a, b, k) {
  const out = {};
  for (const key of Object.keys(a)) out[key] = a[key] + (b[key] - a[key]) * k;
  return out;
}

function normalizeMode(value) {
  const mode = String(value || '').toLowerCase();
  if (['sleeping', 'working', 'review', 'reading', 'learning', 'failed'].includes(mode)) return mode;
  if (mode === 'running') return 'working';
  return 'waiting';
}

// ---- base poses ------------------------------------------------------------

function basePose(mode, t, genes) {
  const A = genes.amplitude;
  const irr = genes.irregularity;
  const T = t * genes.tempo + genes.phase * 37;
  const pose = restPose();
  if (mode === 'sleeping') {
    const b = breath(T, 4.6);
    pose.sx = 1 + 0.024 * A * b;
    pose.sy = 1 - 0.034 * A * b;
    pose.y = 0.5 * A * b;
    pose.r = 0.8 * irr * drift(T * 0.18);
    pose.gs = 1 + 0.05 * b;
    return pose;
  }
  if (mode === 'failed') {
    pose.sx = 1.05;
    pose.sy = 0.91;
    pose.r = -3.5;
    pose.y = 0.6;
    const b = breath(T, 5.4);
    pose.sy -= 0.012 * b;
    return pose;
  }
  if (mode === 'working') {
    const bob = breath(T, 0.96);
    pose.y = -1.5 * A * bob;
    pose.sy = 1 + 0.04 * A * wave(T, 0.96, Math.PI / 2);
    pose.sx = 1 - 0.55 * (pose.sy - 1);
    pose.r = 3.2 * A * wave(T, 1.9) * (0.75 + 0.25 * drift(T * 0.7));
    pose.x = 1.1 * A * drift(T * 0.9) * (0.5 + irr);
    pose.gs = 1 - 0.1 * bob;
    return pose;
  }
  if (mode === 'review') {
    const lean = wave(T, 3.1);
    pose.r = 4.2 * A * lean;
    pose.x = 1.2 * A * lean;
    pose.y = -0.8 * breath(T, 3.1);
    pose.sy = 1 + 0.018 * wave(T, 1.55);
    pose.sx = 1 - 0.5 * (pose.sy - 1);
    pose.ex = 2.2 * wave(T, 2.6);
    return pose;
  }
  if (mode === 'reading') {
    const line = (T / 1.8) % 1;
    pose.ex = line < 0.85 ? -2 + (4 * line) / 0.85 : 2 - (4 * (line - 0.85)) / 0.15;
    pose.ey = 0.4;
    pose.y = 0.4 * breath(T, 1.8);
    pose.r = 1.4 * wave(T, 3.6);
    pose.sy = 1 - 0.012 * breath(T, 3.6);
    return pose;
  }
  if (mode === 'learning') {
    const b = breath(T, 2.2);
    pose.y = -2 * A * b;
    pose.r = 2.6 * A * wave(T, 2.2, 1);
    pose.sy = 1 + 0.03 * b;
    pose.sx = 1 - 0.015 * b;
    pose.ey = -0.8;
    pose.gs = 1 - 0.14 * b;
    pose.go = 1 + 0.5 * b;
    return pose;
  }
  // waiting / awake idle: calm breathing, looking around.
  const b = breath(T, 3.4);
  pose.sx = 1 + 0.014 * A * b;
  pose.sy = 1 - 0.02 * A * b;
  pose.r = 1.4 * A * drift(T * 0.35);
  pose.x = 0.4 * drift(T * 0.5 + 2);
  return pose;
}

// ---- impulses (durable telemetry only) --------------------------------------

const IMPULSES = {
  tool: { duration: 780, apply(p, pose, dir, A) {
    const takeoff = p < 0.2 ? bump(p / 0.2) : 0;
    const air = p >= 0.18 && p <= 0.8 ? bump((p - 0.18) / 0.62) : 0;
    const land = p > 0.78 ? bump((p - 0.78) / 0.22) : 0;
    pose.y -= 8.5 * A * air;
    pose.sy += 0.09 * air - 0.13 * takeoff - 0.11 * land;
    pose.sx -= 0.05 * air - 0.09 * takeoff - 0.08 * land;
    pose.r += dir * 5 * air;
    pose.gs -= 0.4 * air;
    pose.go -= 0.35 * air;
  } },
  thinking: { duration: 1350, apply(p, pose, dir, A) {
    const e = bump(p);
    pose.r += dir * 10 * A * e;
    pose.y -= 1.6 * e;
    pose.ex += dir * 1.8 * e;
    pose.ey -= 1.6 * e;
  } },
  review: { duration: 1600, apply(p, pose, dir, A) {
    const e = bump(p);
    pose.x += dir * 3.4 * A * e;
    pose.r += dir * 7 * Math.sin(TWO_PI * p) * e;
    pose.ex += dir * 2.4 * e;
  } },
  wake: { duration: 720, apply(p, pose) {
    const e = bump(p);
    pose.sy += 0.15 * e;
    pose.sx -= 0.07 * e;
    pose.y -= 2.4 * e;
  } },
  oops: { duration: 820, apply(p, pose) {
    const decay = 1 - p;
    pose.x += 2.6 * Math.sin(18 * Math.PI * p) * decay;
    pose.r += 5 * Math.sin(18 * Math.PI * p) * decay;
  } },
  cheer: { duration: 900, apply(p, pose) {
    const e = bump(p);
    pose.y -= 5 * e;
    pose.sy += 0.08 * e;
    pose.gs -= 0.3 * e;
  } },
};

function transitionImpulse(from, to) {
  if (to === 'failed') return 'oops';
  if (from === 'sleeping' && to !== 'sleeping') return 'wake';
  if (to === 'learning') return 'cheer';
  return '';
}

// ---- engine -----------------------------------------------------------------

function createEngine() {
  const actors = new Map();
  const memory = new Map();
  let frame = 0;
  let lastFrameAt = 0;
  let observer = null;
  let intersection = null;
  let reduced = false;
  let started = false;

  const reducedQuery = typeof window.matchMedia === 'function' ? window.matchMedia('(prefers-reduced-motion: reduce)') : null;
  reduced = Boolean(reducedQuery?.matches);

  function readGenes(node) {
    const [tempo, amplitude, irregularity, phase] = String(node.dataset.crewMotion || '').split(',').map(Number);
    return {
      tempo: Number.isFinite(tempo) && tempo > 0 ? tempo : 1,
      amplitude: Number.isFinite(amplitude) && amplitude > 0 ? amplitude : 1,
      irregularity: Number.isFinite(irregularity) ? irregularity : 0.5,
      phase: Number.isFinite(phase) ? phase : 0,
    };
  }

  function actorKey(node) {
    return String(node.dataset.crewKey || node.dataset.crewSeed || '');
  }

  function scheduleIn(actor, now) {
    const unit = hashUnit(actor.key, actor.salt++);
    actor.nextBlinkAt = now + 2200 + unit * 3800;
    actor.nextGlanceAt = now + 1200 + hashUnit(actor.key, actor.salt++) * 3200;
  }

  function attach(node) {
    if (actors.has(node) || !node.isConnected) return;
    const figure = node.querySelector(':scope > .ctox-crew-figure') || node.querySelector('svg');
    if (!figure) return;
    const now = performance.now();
    const key = actorKey(node);
    const remembered = memory.get(key);
    const mode = normalizeMode(node.dataset.crewMode);
    const turns = Math.max(0, Number(node.dataset.activityTurns) || 0);
    const actor = {
      node,
      figure,
      ground: node.querySelector(':scope > .ctox-crew-ground'),
      eyes: figure.querySelector('.ctox-crew-eyes'),
      key,
      salt: 1,
      genes: readGenes(node),
      mode,
      turns,
      impulses: [],
      fromPose: null,
      transitionAt: 0,
      lastPose: restPose(),
      lastWritten: '',
      lastEyes: '',
      lastGround: '',
      visible: false,
      unit: 0,
      glance: { x: 0, y: 0, at: 0 },
      blinkAt: 0,
      // WebKit misplaces composited layers inside foreignObject: no layer hint there.
      layerHint: !node.closest('foreignObject'),
    };
    scheduleIn(actor, now);
    if (remembered && now - remembered.at < MEMORY_TTL_MS) {
      actor.lastPose = remembered.pose;
      if (remembered.mode !== mode) {
        actor.fromPose = remembered.pose;
        actor.transitionAt = now;
        const gesture = transitionImpulse(remembered.mode, mode);
        if (gesture) addImpulse(actor, gesture, now);
      }
      if (turns > remembered.turns) addImpulse(actor, impulseForTurn(node, mode), now);
    } else if (turns > 0 && Date.now() - (Number(node.dataset.activityUpdatedAt) || 0) <= FRESH_EVENT_MS) {
      addImpulse(actor, impulseForTurn(node, mode), now);
    }
    actors.set(node, actor);
    intersection?.observe(node);
  }

  function detach(actor) {
    actors.delete(actor.node);
    intersection?.unobserve(actor.node);
    if (actor.key) memory.set(actor.key, { mode: actor.mode, turns: actor.turns, pose: actor.lastPose, at: performance.now() });
    clearStyles(actor);
  }

  function clearStyles(actor) {
    actor.figure.style.transform = '';
    actor.figure.style.willChange = '';
    if (actor.ground) {
      actor.ground.style.transform = '';
      actor.ground.style.opacity = '';
    }
    if (actor.eyes) actor.eyes.style.transform = '';
    actor.lastWritten = '';
    actor.lastEyes = '';
    actor.lastGround = '';
  }

  function impulseForTurn(node, mode) {
    if (mode === 'review') return 'review';
    return node.dataset.activityKind === 'thinking' ? 'thinking' : 'tool';
  }

  function addImpulse(actor, name, now) {
    const spec = IMPULSES[name];
    if (!spec) return;
    const dir = hashUnit(actor.key, actor.salt++) > 0.5 ? 1 : -1;
    actor.impulses = actor.impulses.filter((impulse) => impulse.name !== name).slice(-1);
    actor.impulses.push({ name, spec, startAt: now, duration: spec.duration / Math.sqrt(actor.genes.tempo), dir });
  }

  // Telemetry or mode changed on an existing node (in-place updates).
  function refresh(node) {
    const actor = actors.get(node);
    if (!actor) {
      attach(node);
      return;
    }
    const now = performance.now();
    const mode = normalizeMode(node.dataset.crewMode);
    if (mode !== actor.mode) {
      actor.fromPose = actor.lastPose;
      actor.transitionAt = now;
      const gesture = transitionImpulse(actor.mode, mode);
      actor.mode = mode;
      actor.eyes = actor.figure.querySelector('.ctox-crew-eyes');
      if (gesture) addImpulse(actor, gesture, now);
    }
    const turns = Math.max(0, Number(node.dataset.activityTurns) || 0);
    if (turns > actor.turns && (mode === 'working' || mode === 'review')) addImpulse(actor, impulseForTurn(node, mode), now);
    actor.turns = turns;
    ensureLoop();
  }

  function scan(root) {
    if (!root) return;
    if (root.nodeType === 1 && root.matches?.(CREATURE_SELECTOR)) attach(root);
    root.querySelectorAll?.(CREATURE_SELECTOR).forEach((node) => refresh(node));
    ensureLoop();
  }

  function composePose(actor, now) {
    const t = now / 1000;
    let pose = basePose(actor.mode, t, actor.genes);
    if (actor.fromPose) {
      const k = clamp((now - actor.transitionAt) / TRANSITION_MS, 0, 1);
      if (k >= 1) actor.fromPose = null;
      else pose = mixPose(actor.fromPose, pose, smooth(k));
    }
    actor.impulses = actor.impulses.filter((impulse) => now - impulse.startAt < impulse.duration);
    for (const impulse of actor.impulses) {
      impulse.spec.apply(clamp((now - impulse.startAt) / impulse.duration, 0, 1), pose, impulse.dir, actor.genes.amplitude);
    }
    // Face: blinks and glances for open eyes.
    if (OPEN_EYE_MODES.has(actor.mode)) {
      if (now >= actor.nextBlinkAt) {
        actor.blinkAt = now;
        actor.nextBlinkAt = now + 2400 + hashUnit(actor.key, actor.salt++) * 4200;
      }
      const blinkP = (now - actor.blinkAt) / 150;
      if (blinkP >= 0 && blinkP < 1) pose.eb = 1 - 0.88 * bump(blinkP);
      if (now >= actor.nextGlanceAt && actor.mode !== 'review') {
        const busy = actor.mode === 'working';
        actor.glance = {
          x: (hashUnit(actor.key, actor.salt++) - 0.5) * (busy ? 3.6 : 4.4),
          y: busy ? 0.6 + hashUnit(actor.key, actor.salt++) * 0.8 : (hashUnit(actor.key, actor.salt++) - 0.5) * 1.6,
          at: now,
        };
        actor.nextGlanceAt = now + (busy ? 900 : 1800) + hashUnit(actor.key, actor.salt++) * (busy ? 1900 : 3600);
      }
      const settle = smooth(clamp((now - actor.glance.at) / 180, 0, 1));
      pose.ex += actor.glance.x * settle;
      pose.ey += actor.glance.y * settle;
    }
    return pose;
  }

  function write(actor, pose) {
    const unit = actor.unit || (actor.figure.getBoundingClientRect().width / 64) || 0.5;
    actor.unit = unit;
    const f = (value, digits = 2) => value.toFixed(digits);
    const body = `translate(${f(pose.x * unit)}px, ${f(pose.y * unit)}px) rotate(${f(pose.r)}deg) scale(${f(pose.sx, 3)}, ${f(pose.sy, 3)})`;
    if (body !== actor.lastWritten) {
      actor.figure.style.transform = body;
      actor.lastWritten = body;
    }
    if (actor.ground) {
      const ground = `${f(clamp(pose.gs, 0.4, 1.3), 3)}|${f(clamp(pose.go, 0.2, 1.6), 2)}`;
      if (ground !== actor.lastGround) {
        const [scale, opacity] = ground.split('|');
        actor.ground.style.transform = `scale(${scale})`;
        actor.ground.style.opacity = opacity;
        actor.lastGround = ground;
      }
    }
    if (actor.eyes) {
      // Eye moves repaint the SVG; quantise so a held glance costs nothing.
      const eyes = `translate(${f(pose.ex, 1)}px, ${f(pose.ey, 1)}px) scale(1, ${f(pose.eb, 2)})`;
      if (eyes !== actor.lastEyes) {
        actor.eyes.style.transform = eyes;
        actor.lastEyes = eyes;
      }
    }
    actor.lastPose = pose;
  }

  function tick(now) {
    frame = 0;
    if (reduced || document.hidden) return;
    let animated = 0;
    let lively = false;
    for (const actor of actors.values()) {
      if (!actor.node.isConnected) {
        detach(actor);
        continue;
      }
      if (!actor.visible) continue;
      animated += 1;
      if (!CALM_MODES.has(actor.mode) || actor.impulses.length || actor.fromPose) lively = true;
    }
    if (!animated) return;
    // Calm scenes (only sleepers) run at ~30 fps, lively ones at ~60 fps.
    const interval = lively ? 15 : 32;
    if (now - lastFrameAt >= interval) {
      lastFrameAt = now;
      for (const actor of actors.values()) {
        if (!actor.visible) continue;
        write(actor, composePose(actor, now));
      }
    }
    frame = window.requestAnimationFrame(tick);
  }

  function ensureLoop() {
    if (frame || reduced || document.hidden || !started) return;
    frame = window.requestAnimationFrame(tick);
  }

  function pruneMemory() {
    const now = performance.now();
    for (const [key, entry] of memory) if (now - entry.at > MEMORY_TTL_MS) memory.delete(key);
  }

  function start(root = document) {
    if (started) {
      scan(root.nodeType ? root : document);
      return;
    }
    started = true;
    // The one creature stylesheet, first in <head> so hosts can size wrappers.
    if (!document.getElementById(STYLE_ID)) {
      const style = document.createElement('style');
      style.id = STYLE_ID;
      style.textContent = CREW_CREATURE_CSS;
      (document.head || document.documentElement).prepend(style);
    }
    if (typeof IntersectionObserver === 'function') {
      intersection = new IntersectionObserver((entries) => {
        for (const entry of entries) {
          const actor = actors.get(entry.target);
          if (!actor) continue;
          actor.visible = entry.isIntersecting;
          if (entry.boundingClientRect.width) actor.unit = entry.boundingClientRect.width / 64;
          if (actor.visible && !reduced && actor.layerHint) actor.figure.style.willChange = 'transform';
          else if (!actor.visible) actor.figure.style.willChange = '';
        }
        ensureLoop();
      }, { rootMargin: '32px' });
    }
    observer = new MutationObserver((records) => {
      for (const record of records) {
        if (record.type === 'attributes') {
          const node = record.target;
          if (node.matches?.(CREATURE_SELECTOR)) refresh(node);
          continue;
        }
        for (const node of record.addedNodes) {
          if (node.nodeType !== 1) continue;
          if (node.matches(CREATURE_SELECTOR)) attach(node);
          else if (node.firstElementChild) node.querySelectorAll(CREATURE_SELECTOR).forEach(attach);
        }
      }
      ensureLoop();
    });
    observer.observe(document.documentElement, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ['data-activity-turns', 'data-crew-mode'],
    });
    document.addEventListener('visibilitychange', () => {
      if (!document.hidden) ensureLoop();
    });
    reducedQuery?.addEventListener?.('change', (event) => {
      reduced = event.matches;
      if (reduced) for (const actor of actors.values()) clearStyles(actor);
      else ensureLoop();
    });
    window.setInterval(pruneMemory, 60 * 1000);
    document.querySelectorAll(CREATURE_SELECTOR).forEach(attach);
    if (!intersection) for (const actor of actors.values()) actor.visible = true;
    ensureLoop();
  }

  function snapshot() {
    return Array.from(actors.values()).map((actor) => ({
      node: actor.node,
      key: actor.key,
      mode: actor.mode,
      visible: actor.visible,
      turns: actor.turns,
      impulses: actor.impulses.map((impulse) => impulse.name),
      transform: actor.lastWritten,
    }));
  }

  return { start, scan, refresh, snapshot, get running() { return Boolean(frame); }, get reduced() { return reduced; } };
}

function engine() {
  if (typeof window === 'undefined' || typeof document === 'undefined') return null;
  // Needs a real browser: DOM observation and animation frames.
  if (typeof MutationObserver !== 'function' || typeof window.requestAnimationFrame !== 'function' || !document.documentElement) return null;
  // One engine per page, even if this module is loaded under several URLs.
  if (!window[ENGINE_KEY]) window[ENGINE_KEY] = createEngine();
  return window[ENGINE_KEY];
}

/** Start the page-wide engine (idempotent). */
export function startCrewMotion(root = document) {
  engine()?.start(root);
}

/** Pick up creatures and telemetry changes below root right now. */
export function syncCrewMotion(root = document) {
  const instance = engine();
  if (!instance) return;
  instance.start(root);
}

/** Debug/acceptance view of the engine state. */
export function crewMotionSnapshot() {
  return engine()?.snapshot() || [];
}

export const __crewMotionInternals = { basePose, IMPULSES, normalizeMode, transitionImpulse };
