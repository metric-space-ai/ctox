/** Crew creatures: procedurally generated from a genome.
 *
 * A member's look is not drawn by hand. The member id (or, without one, its
 * name) seeds a genome; the owner-chosen archetype (round/blob/square/triangle)
 * and colour are the two genes a person sets, everything else — outline,
 * proportions, face, shade, shine, cheeks, tuft and motion temperament — is
 * derived deterministically, so the same member looks identical everywhere and
 * two members never become twins. Without a member the creature is the neutral
 * "crew" ghost.
 *
 * Pure: no DOM access, timers, network, persistence or randomness. Callers own
 * identity, task state and telemetry. Motion lives in crew-motion.js.
 */

export const CREW_ARCHETYPES = Object.freeze(['round', 'blob', 'square', 'triangle']);

const NEUTRAL_CREW_IDENTITY = Object.freeze({ name: 'Crew', color: '#7d7f84', shape: 'round' });

function crewHash(value) {
  let hash = 2166136261;
  const input = String(value || 'ctox-crew');
  for (let index = 0; index < input.length; index += 1) {
    hash ^= input.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 0;
}

export function normalizeCrewAppearance(explicit) {
  if (explicit && typeof explicit === 'object' && String(explicit.name || '').trim()) {
    const appearance = {
      name: String(explicit.name).trim(),
      color: /^#[0-9a-f]{6}$/i.test(String(explicit.color || '')) ? String(explicit.color) : NEUTRAL_CREW_IDENTITY.color,
      shape: CREW_ARCHETYPES.includes(explicit.shape) ? explicit.shape : NEUTRAL_CREW_IDENTITY.shape,
    };
    const id = String(explicit.id || '').trim();
    if (id) appearance.id = id;
    return appearance;
  }
  return { ...NEUTRAL_CREW_IDENTITY };
}

function isMemberAppearance(explicit) {
  return Boolean(explicit && typeof explicit === 'object' && String(explicit.name || '').trim());
}

// ---- genome ---------------------------------------------------------------

function mulberry32(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const round1 = (value) => Math.round(value * 10) / 10;
const clamp = (value, min, max) => Math.max(min, Math.min(max, value));

function hexToHsl(hex) {
  const value = parseInt(hex.slice(1), 16);
  const r = ((value >> 16) & 255) / 255;
  const g = ((value >> 8) & 255) / 255;
  const b = (value & 255) / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h;
  if (max === r) h = (g - b) / d + (g < b ? 6 : 0);
  else if (max === g) h = (b - r) / d + 2;
  else h = (r - g) / d + 4;
  return [h * 60, s, l];
}

function hslToHex(h, s, l) {
  const hue = ((h % 360) + 360) % 360;
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const x = c * (1 - Math.abs(((hue / 60) % 2) - 1));
  const m = l - c / 2;
  const [r, g, b] = hue < 60 ? [c, x, 0] : hue < 120 ? [x, c, 0] : hue < 180 ? [0, c, x]
    : hue < 240 ? [0, x, c] : hue < 300 ? [x, 0, c] : [c, 0, x];
  const byte = (v) => Math.round(clamp((v + m) * 255, 0, 255)).toString(16).padStart(2, '0');
  return `#${byte(r)}${byte(g)}${byte(b)}`;
}

function mixHex(a, b, amount) {
  const pa = parseInt(a.slice(1), 16);
  const pb = parseInt(b.slice(1), 16);
  const channel = (shift) => Math.round(((pa >> shift) & 255) * (1 - amount) + ((pb >> shift) & 255) * amount);
  return `#${[16, 8, 0].map((shift) => channel(shift).toString(16).padStart(2, '0')).join('')}`;
}

// Radial outline per archetype; r(theta) around the body centre (unit ~1).
function radialProfile(archetype, gene) {
  const wobble = (theta) => gene.wobble2 * Math.cos(2 * theta + gene.phase2) + gene.wobble3 * Math.cos(3 * theta + gene.phase3);
  const superellipse = (theta, n) => (Math.abs(Math.cos(theta)) ** n + Math.abs(Math.sin(theta)) ** n) ** (-1 / n);
  if (archetype === 'blob') {
    return (theta) => 1 + gene.lobeDepth * (Math.abs(Math.cos((gene.lobes * (theta - gene.lobePhase)) / 2)) ** 0.55 - 0.62) + wobble(theta) * 0.4;
  }
  return (theta) => superellipse(theta, gene.exponent) * (1 + wobble(theta));
}

// A rounded polygon in unit space: each corner is a quadratic arc of its own
// radius, each side bulges outward by its own amount.
function roundedPolygonPoints(vertices, radii, bulges, perCorner = 7, perSide = 6) {
  const count = vertices.length;
  const points = [];
  const lerp = (a, b, t) => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
  const cornerStart = [];
  const cornerEnd = [];
  for (let index = 0; index < count; index += 1) {
    const prev = vertices[(index + count - 1) % count];
    const vertex = vertices[index];
    const next = vertices[(index + 1) % count];
    const toPrev = Math.hypot(prev[0] - vertex[0], prev[1] - vertex[1]);
    const toNext = Math.hypot(next[0] - vertex[0], next[1] - vertex[1]);
    cornerStart[index] = lerp(vertex, prev, Math.min(0.45, radii[index] / toPrev));
    cornerEnd[index] = lerp(vertex, next, Math.min(0.45, radii[index] / toNext));
  }
  for (let index = 0; index < count; index += 1) {
    const vertex = vertices[index];
    const a = cornerStart[index];
    const b = cornerEnd[index];
    for (let step = 0; step < perCorner; step += 1) {
      const t = step / perCorner;
      const u = 1 - t;
      points.push([u * u * a[0] + 2 * u * t * vertex[0] + t * t * b[0], u * u * a[1] + 2 * u * t * vertex[1] + t * t * b[1]]);
    }
    const from = b;
    const to = cornerStart[(index + 1) % count];
    const length = Math.hypot(to[0] - from[0], to[1] - from[1]);
    // Outward normal for a clockwise (screen-space) polygon.
    const normal = [(to[1] - from[1]) / length, -(to[0] - from[0]) / length];
    for (let step = 0; step < perSide; step += 1) {
      const t = step / perSide;
      const bulge = Math.sin(Math.PI * t) * bulges[index] * length;
      const base = lerp(from, to, t);
      points.push([base[0] + normal[0] * bulge, base[1] + normal[1] * bulge]);
    }
  }
  return points;
}

const ARCHETYPE_RANGES = {
  round: { width: [44, 52], height: [42, 50], face: [0.4, 0.48], flat: [0.84, 0.95], samples: 40 },
  square: { width: [40, 48], height: [38, 46], face: [0.38, 0.46], flat: [0.9, 1], samples: 44 },
  triangle: { width: [48, 56], height: [42, 50], face: [0.6, 0.66], flat: [1, 1], samples: 0 },
  blob: { width: [46, 54], height: [38, 46], face: [0.44, 0.52], flat: [0.74, 0.86], samples: 64 },
};

function closedCatmullRomPath(points) {
  const count = points.length;
  const at = (index) => points[(index + count) % count];
  let d = `M${round1(points[0][0])} ${round1(points[0][1])}`;
  for (let index = 0; index < count; index += 1) {
    const p0 = at(index - 1);
    const p1 = at(index);
    const p2 = at(index + 1);
    const p3 = at(index + 2);
    const c1 = [p1[0] + (p2[0] - p0[0]) / 6, p1[1] + (p2[1] - p0[1]) / 6];
    const c2 = [p2[0] - (p3[0] - p1[0]) / 6, p2[1] - (p3[1] - p1[1]) / 6];
    d += `C${round1(c1[0])} ${round1(c1[1])} ${round1(c2[0])} ${round1(c2[1])} ${round1(p2[0])} ${round1(p2[1])}`;
  }
  return `${d}Z`;
}

// Width of the closed outline at height y (max crossing distance).
function widthAt(points, y) {
  const xs = [];
  for (let index = 0; index < points.length; index += 1) {
    const [x1, y1] = points[index];
    const [x2, y2] = points[(index + 1) % points.length];
    if ((y1 <= y && y2 > y) || (y2 <= y && y1 > y)) xs.push(x1 + ((y - y1) / (y2 - y1)) * (x2 - x1));
  }
  if (xs.length < 2) return { left: 26, right: 38 };
  return { left: Math.min(...xs), right: Math.max(...xs) };
}

const genomeCache = new Map();

/** Deterministic genome for one member appearance (normalized). */
export function crewGenome(appearanceInput) {
  const appearance = normalizeCrewAppearance(appearanceInput);
  const identityKey = appearance.id || appearance.name;
  const cacheKey = `${identityKey}|${appearance.shape}|${appearance.color}`;
  const cached = genomeCache.get(cacheKey);
  if (cached) return cached;

  const seed = crewHash(`genome:${identityKey}`);
  const rand = mulberry32(seed);
  const between = (min, max) => min + (max - min) * rand();
  const chance = (p) => rand() < p;
  const archetype = appearance.shape;
  const range = ARCHETYPE_RANGES[archetype];

  const gene = {
    exponent: archetype === 'square' ? between(3.1, 4.6) : between(1.9, 2.35),
    corner: between(0.17, 0.25),
    apexTurn: between(-0.1, 0.1),
    lobes: [5, 6, 7][Math.floor(rand() * 3)],
    lobePhase: between(0, Math.PI * 2),
    lobeDepth: between(0.12, 0.17),
    wobble2: between(-0.035, 0.035),
    wobble3: between(-0.03, 0.03),
    phase2: between(0, Math.PI * 2),
    phase3: between(0, Math.PI * 2),
  };
  const width = between(...range.width);
  const height = between(...range.height);
  const flat = between(...range.flat);
  const lean = between(-0.07, 0.07);

  // Sample the profile, flatten the underside, lean, then fit to the box that
  // stands on the ground line (y = 58) centred at x = 32.
  let raw = [];
  if (archetype === 'triangle') {
    const apex = [0.5 + between(-0.09, 0.09), 0];
    const unit = roundedPolygonPoints(
      [apex, [1, 1], [0, 1]],
      [between(0.16, 0.24), between(0.1, 0.16), between(0.1, 0.16)],
      [between(0.015, 0.06), between(0, 0.018), between(0.015, 0.06)],
    );
    raw = unit.map(([x, y]) => [x + (1 - y) * lean * 1.6, y]);
  } else {
    const profile = radialProfile(archetype, gene);
    for (let index = 0; index < range.samples; index += 1) {
      const theta = -Math.PI / 2 + (index / range.samples) * Math.PI * 2;
      const r = profile(theta);
      let x = r * Math.cos(theta);
      let y = r * Math.sin(theta);
      if (y > 0) y *= flat;
      x += -y * lean;
      raw.push([x, y]);
    }
  }
  const minX = Math.min(...raw.map((p) => p[0]));
  const maxX = Math.max(...raw.map((p) => p[0]));
  const minY = Math.min(...raw.map((p) => p[1]));
  const maxY = Math.max(...raw.map((p) => p[1]));
  const ground = 58;
  const top = ground - height;
  const left = 32 - width / 2;
  const points = raw.map(([x, y]) => [
    left + ((x - minX) / (maxX - minX)) * width,
    top + ((y - minY) / (maxY - minY)) * height,
  ]);
  const topPoint = points.reduce((best, point) => (point[1] < best[1] ? point : best), points[0]);

  // Face: eyes sit on the face line, spaced by the body width there.
  const faceY = top + height * between(...range.face);
  const span = widthAt(points, faceY);
  const faceCentre = (span.left + span.right) / 2 + between(-1.6, 2.2);
  const half = Math.max(5.6, (span.right - span.left) * between(0.16, 0.21));
  const eyeSize = between(0.86, 1.14);
  const eyes = {
    left: [round1(faceCentre - half), round1(faceY)],
    right: [round1(faceCentre + half), round1(faceY + between(-1.2, 0.4))],
    size: round1(eyeSize * 100) / 100,
    stroke: round1(clamp(4.9 * eyeSize, 4.3, 5.5)),
    slant: round1(between(2.2, 3.6)),
  };

  // Tone: the owner's colour, individually shifted so two members of the same
  // palette colour stay tell-apart-able; grey stays grey.
  const [h, s, l] = hexToHsl(appearance.color);
  const fill = hslToHex(h + between(-11, 11), clamp(s * between(0.9, 1.08), 0, 1), clamp(l + between(-0.05, 0.045), 0.28, 0.72));
  const [, fs, fl] = hexToHsl(fill);
  const shade = hslToHex(h, clamp(fs * 1.05, 0, 1), clamp(fl - 0.17, 0.12, 0.6));
  const cheek = mixHex(fill, '#ff6f8e', 0.55);

  const shine = {
    cx: round1(left + width * (archetype === 'triangle' ? 0.38 : 0.3)),
    cy: round1(top + height * (archetype === 'triangle' ? 0.42 : 0.24)),
    rx: round1(width * between(0.08, 0.12)),
    ry: round1(height * between(0.05, 0.075)),
  };
  const cheeks = chance(0.5)
    ? { left: [round1(eyes.left[0] - 2), round1(faceY + 6.5 * eyeSize)], right: [round1(eyes.right[0] + 2), round1(eyes.right[1] + 6.5 * eyeSize)] }
    : null;

  let tuft = null;
  if (archetype !== 'triangle' && chance(0.45)) {
    const [tx, ty] = topPoint;
    const base = [round1(tx + between(-3, 3)), round1(ty + 4)];
    const kind = Math.floor(rand() * 3);
    if (kind === 0) tuft = `M${base[0]} ${base[1]}q${round1(between(-1, 2))} -9 ${round1(between(5, 8))} -10`;
    else if (kind === 1) tuft = `M${base[0] - 2} ${base[1]}q-2 -7 -6 -8M${base[0] + 2} ${base[1]}q2 -8 7 -9`;
    else tuft = `M${base[0]} ${base[1]}l${round1(between(-1.5, 1.5))} -9m0 0h0.1`;
  }

  // Motion temperament, read by crew-motion.js.
  const motion = {
    tempo: round1(between(0.82, 1.22) * 100) / 100,
    amplitude: round1(between(0.8, 1.2) * 100) / 100,
    irregularity: round1(between(0.15, 0.85) * 100) / 100,
    phase: round1(between(0, 1000)) / 1000,
  };

  const genome = {
    seed,
    archetype,
    path: closedCatmullRomPath(points),
    box: { left: round1(left), top: round1(top), width: round1(width), height: round1(height) },
    eyes,
    tone: { fill, shade, cheek },
    shine,
    cheeks,
    tuft,
    tuftDot: tuft && tuft.endsWith('h0.1'),
    motion,
  };
  if (genomeCache.size > 256) genomeCache.clear();
  genomeCache.set(cacheKey, genome);
  return genome;
}

// ---- state ----------------------------------------------------------------

const REVIEW_PHASES = new Set(['review', 'awaiting_review', 'awaiting-review', 'reviewing', 'validating']);

/** Creature mode for a task state (and, while running, its execution phase). */
export function crewModeForTaskState(taskState, phase = '') {
  if (taskState === 'failed') return 'failed';
  if (taskState === 'running') return REVIEW_PHASES.has(String(phase || '').toLowerCase()) ? 'review' : 'working';
  if (['idle', 'queued', 'scheduled', 'success'].includes(taskState)) return 'sleeping';
  // reading / learning are member expressions; blocked and others wait awake.
  return taskState;
}

/** Durable activity telemetry from a raw `execution_progress` document. */
export function crewActivityFromProgress(progress) {
  const turns = progress?.activity_turns || progress?.activityTurns || {};
  const kind = turns.last_kind || turns.lastKind;
  return {
    total: Math.max(0, Number(turns.total) || 0),
    lastKind: kind === 'thinking' || kind === 'tool' ? kind : '',
    updatedAt: Math.max(0, Number(progress?.updated_at_ms ?? progress?.updatedAtMs) || 0),
  };
}

// ---- face -----------------------------------------------------------------

function eyesMarkup(eyes, mode) {
  const s = eyes.size;
  const pair = (draw) => draw(eyes.left[0], eyes.left[1]) + draw(eyes.right[0], eyes.right[1]);
  const n = (value) => round1(value);
  if (mode === 'failed') {
    return `<g class="ctox-crew-eyes-x">${pair((x, y) => `<path d="M${n(x - 4.5 * s)} ${n(y - 4.5 * s)}l${n(9 * s)} ${n(9 * s)}M${n(x + 4.5 * s)} ${n(y - 4.5 * s)}l${n(-9 * s)} ${n(9 * s)}" />`)}</g>`;
  }
  if (mode === 'sleeping') {
    return `<g class="ctox-crew-eyes-sleeping">${pair((x, y) => `<path d="M${n(x - 4 * s)} ${n(y + 1)}q${n(4 * s)} ${n(3.6 * s)} ${n(8 * s)} 0" />`)}</g>`;
  }
  if (mode === 'review') {
    // Scrutinising: narrowed, peering arcs.
    return `<g class="ctox-crew-eyes-review">${pair((x, y) => `<path d="M${n(x - 4.8 * s)} ${n(y + 2)}q${n(4.8 * s)} ${n(-6.5 * s)} ${n(9.6 * s)} 0" />`)}</g>`;
  }
  if (mode === 'reading') {
    // Lowered gaze on a page.
    return `<g class="ctox-crew-eyes-reading">${pair((x, y) => `<path d="M${n(x - 3.8 * s)} ${n(y + 2)}h${n(7.6 * s)}" />`)}</g>`;
  }
  if (mode === 'learning') {
    // Wide, lifted eyes: something just clicked.
    return `<g class="ctox-crew-eyes-learning">${pair((x, y) => `<circle cx="${n(x)}" cy="${n(y - 1)}" r="${n(3.2 * s)}" />`)}</g>`;
  }
  const dx = eyes.slant / 2;
  return pair((x, y) => `<path d="M${n(x - dx * s)} ${n(y - 4 * s)}l${n(eyes.slant * s)} ${n(8 * s)}" />`);
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

function escapeAttr(value) {
  return escapeHtml(value).replace(/`/g, '&#96;');
}

const NEUTRAL_BODY = 'M32 12c13.3 0 23 9 23 22.5S46.2 58 32 58 9 48 9 34.5 18.7 12 32 12Z';
const NEUTRAL_EYES = { left: [25.5, 33], right: [38.5, 33], size: 0.92, stroke: 4.6, slant: 2.6 };
const EYE_STROKE_BY_MODE = { sleeping: 0.78, review: 0.86, reading: 0.9 };
const eyeStroke = (eyes, mode) => round1(eyes.stroke * (EYE_STROKE_BY_MODE[mode] || 1));

export function renderCrewCreature({
  appearance, animationKey, taskState, mode, placement = 'dock',
  progressPercent = 0, activity = { total: 0, lastKind: '', updatedAt: 0 },
}) {
  const member = isMemberAppearance(appearance);
  const crew = normalizeCrewAppearance(appearance);
  const genome = member ? crewGenome(crew) : null;
  const progressAngle = Math.max(0, Math.min(360, Number(progressPercent || 0) * 3.6));
  const telemetry = activity;
  const motionSeed = crewHash(`${animationKey}:${placement}`);
  const motion = genome ? genome.motion : { tempo: 0.9, amplitude: 0.7, irregularity: 0.2, phase: 0 };
  const colorVars = genome
    ? `--crew-color:${escapeAttr(crew.color)};--crew-fill:${genome.tone.fill};--crew-shade:${genome.tone.shade};--crew-cheek:${genome.tone.cheek}`
    : `--crew-color:${escapeAttr(crew.color)}`;
  const figure = genome
    ? `${genome.tuft ? `<path class="ctox-crew-tuft" d="${genome.tuft}" />` : ''}`
      + `<g class="ctox-crew-body"><path d="${genome.path}" /></g>`
      + `<ellipse class="ctox-crew-shine" cx="${genome.shine.cx}" cy="${genome.shine.cy}" rx="${genome.shine.rx}" ry="${genome.shine.ry}" />`
      + (genome.cheeks ? `<g class="ctox-crew-cheeks"><ellipse cx="${genome.cheeks.left[0]}" cy="${genome.cheeks.left[1]}" rx="3.1" ry="1.9" /><ellipse cx="${genome.cheeks.right[0]}" cy="${genome.cheeks.right[1]}" rx="3.1" ry="1.9" /></g>` : '')
      + `<g class="ctox-crew-eyes is-${escapeAttr(mode)}" style="stroke-width:${eyeStroke(genome.eyes, mode)}">${eyesMarkup(genome.eyes, mode)}</g>`
    : `<g class="ctox-crew-body"><path d="${NEUTRAL_BODY}" /></g>`
      + `<g class="ctox-crew-eyes is-${escapeAttr(mode)}" style="stroke-width:${eyeStroke(NEUTRAL_EYES, mode)}">${eyesMarkup(NEUTRAL_EYES, mode)}</g>`;
  return `
    <span class="ctox-crew-creature is-${escapeAttr(taskState)} is-${escapeAttr(mode)} is-${escapeAttr(crew.shape)} is-${escapeAttr(placement)}${member ? '' : ' is-neutral'}" data-crew-mode="${escapeAttr(mode)}" data-crew-identity="${escapeAttr(JSON.stringify(crew))}" data-crew-seed="${motionSeed}" data-crew-key="${escapeAttr(`${animationKey}:${placement}`)}" data-crew-motion="${motion.tempo},${motion.amplitude},${motion.irregularity},${motion.phase}" data-activity-turns="${escapeAttr(telemetry.total)}" data-activity-kind="${escapeAttr(telemetry.lastKind)}" data-activity-updated-at="${escapeAttr(telemetry.updatedAt)}" style="${colorVars};--ctox-progress-angle:${progressAngle}deg" aria-hidden="true">
      <span class="ctox-crew-ground"></span>
      <svg class="ctox-crew-figure" viewBox="0 0 64 64" focusable="false">${figure}</svg>
    </span>
  `;
}

/** The one stylesheet for creatures. Hosts only size the wrapper. */
export const CREW_CREATURE_BASE_CSS = `
    .ctox-crew-creature {
      position: relative;
      display: inline-grid;
      place-items: center;
      width: 100%;
      height: 100%;
      contain: layout style;
    }
    .ctox-crew-creature > .ctox-crew-figure,
    .ctox-crew-creature svg {
      position: relative;
      display: block;
      width: 100%;
      height: 100%;
      overflow: visible;
      transform-origin: 50% 90%;
    }
    .ctox-crew-ground {
      position: absolute;
      left: 18%;
      right: 18%;
      bottom: 2%;
      height: 14%;
      border-radius: 50%;
      background: radial-gradient(closest-side, color-mix(in srgb, var(--crew-color) 42%, transparent), transparent);
      transform-origin: 50% 50%;
      pointer-events: none;
    }
    .ctox-crew-creature.is-badge .ctox-crew-ground,
    .ctox-crew-creature.is-dock .ctox-crew-ground { display: none; }
    .ctox-crew-body {
      fill: var(--crew-fill, var(--crew-color));
    }
    .ctox-crew-tuft {
      fill: var(--crew-shade, var(--crew-color));
      stroke: var(--crew-shade, var(--crew-color));
      stroke-width: 3.2;
      stroke-linecap: round;
      stroke-linejoin: round;
    }
    .ctox-crew-shine {
      fill: #fff;
      opacity: 0.2;
    }
    .ctox-crew-cheeks {
      fill: var(--crew-cheek, transparent);
      opacity: 0.6;
    }
    .ctox-crew-eyes,
    .ctox-crew-eyes-x,
    .ctox-crew-eyes-sleeping,
    .ctox-crew-eyes-review,
    .ctox-crew-eyes-reading,
    .ctox-crew-eyes-learning {
      fill: none;
      stroke: #090a0c;
      stroke-linecap: round;
      stroke-linejoin: round;
    }
    .ctox-crew-eyes {
      stroke-width: 5;
      transform-box: fill-box;
      transform-origin: center;
    }
    .ctox-crew-eyes-learning circle {
      fill: #090a0c;
      stroke: none;
    }
    /* No member yet: the whole crew as a quiet ghost, never mistaken for one. */
    .ctox-crew-creature.is-neutral .ctox-crew-body {
      fill: color-mix(in srgb, var(--crew-color) 26%, transparent);
      stroke: color-mix(in srgb, var(--crew-color) 88%, white 12%);
      stroke-width: 2.4;
      stroke-dasharray: 5 4;
    }
    .ctox-crew-creature.is-neutral .ctox-crew-eyes { stroke: color-mix(in srgb, var(--crew-color) 70%, white 30%); }
    .ctox-crew-creature.is-neutral .ctox-crew-ground { opacity: 0.4; }
    .ctox-crew-creature.is-window {
      width: 38px;
      height: 38px;
      flex: 0 0 38px;
    }
    .ctox-chat-crew-slot { touch-action: none; }`;

/** Standalone hosts also get the reduced-motion behaviour. */
export const CREW_CREATURE_CSS = CREW_CREATURE_BASE_CSS + `
@media (prefers-reduced-motion: reduce) {
  .ctox-crew-creature, .ctox-crew-creature * {
    animation: none !important;
    transition: none !important;
  }
}
`;
