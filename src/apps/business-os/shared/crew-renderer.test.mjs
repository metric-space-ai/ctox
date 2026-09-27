import test from 'node:test';
import assert from 'node:assert/strict';
import { crewCreatureHtml } from './business-chat.js';
import {
  CREW_ARCHETYPES,
  crewGenome,
  normalizeCrewAppearance,
  renderCrewCreature,
  CREW_CREATURE_BASE_CSS,
  CREW_CREATURE_CSS,
} from './crew-renderer.js';

const SEEDS = [
  { id: 'crew-milo', name: 'Milo', shape: 'round', color: '#1685ee' },
  { id: 'crew-nori', name: 'Nori', shape: 'square', color: '#00aa9a' },
  { id: 'crew-lumi', name: 'Lumi', shape: 'triangle', color: '#7d7f84' },
  { id: 'crew-pico', name: 'Pico', shape: 'blob', color: '#7c6df2' },
];

const pathPoints = (d) => [...d.matchAll(/-?\d+(?:\.\d+)?/g)].map(Number);

test('a genome is deterministic per member and independent of where the creature is shown', () => {
  for (const seed of SEEDS) {
    assert.deepEqual(crewGenome(seed), crewGenome({ ...seed }));
    const map = renderCrewCreature({ appearance: seed, animationKey: seed.id, taskState: 'idle', mode: 'sleeping', placement: 'map' });
    const dock = renderCrewCreature({ appearance: seed, animationKey: 'cmd-123', taskState: 'running', mode: 'working', placement: 'dock' });
    const body = (html) => html.match(/<g class="ctox-crew-body"><path d="([^"]+)"/)[1];
    assert.equal(body(map), body(dock), `${seed.name} keeps one body everywhere`);
    assert.equal(body(map), crewGenome(seed).path);
  }
});

test('the member id seeds the genome; without it the name does, and renaming an id-less look is stable per name', () => {
  const withId = crewGenome({ id: 'crew-x', name: 'Xa', shape: 'round', color: '#1685ee' });
  const renamed = crewGenome({ id: 'crew-x', name: 'Xb', shape: 'round', color: '#1685ee' });
  assert.equal(withId.path, renamed.path, 'renaming a member does not change its body');
  const byName = crewGenome({ name: 'Xa', shape: 'round', color: '#1685ee' });
  assert.equal(byName.path, crewGenome({ name: 'Xa', shape: 'round', color: '#1685ee' }).path);
});

test('members of the same archetype and colour are never twins', () => {
  for (const archetype of CREW_ARCHETYPES) {
    const bodies = new Set();
    const tones = new Set();
    for (let index = 0; index < 24; index += 1) {
      const genome = crewGenome({ id: `crew-${archetype}-${index}`, name: `M${index}`, shape: archetype, color: '#7d7f84' });
      bodies.add(genome.path);
      tones.add(genome.tone.fill);
    }
    assert.equal(bodies.size, 24, `${archetype}: 24 distinct bodies`);
    assert.ok(tones.size >= 12, `${archetype}: tones vary (${tones.size})`);
  }
});

test('every generated body stands on the ground inside the frame with the face on the body', () => {
  for (const archetype of CREW_ARCHETYPES) {
    for (let index = 0; index < 60; index += 1) {
      const genome = crewGenome({ id: `frame-${archetype}-${index}`, name: 'F', shape: archetype, color: '#1685ee' });
      const numbers = pathPoints(genome.path);
      const xs = numbers.filter((_, i) => i % 2 === 0);
      const ys = numbers.filter((_, i) => i % 2 === 1);
      assert.ok(Math.min(...xs) >= 1 && Math.max(...xs) <= 63, `${archetype} ${index} x within frame`);
      assert.ok(Math.min(...ys) >= 3 && Math.max(...ys) <= 60, `${archetype} ${index} y within frame`);
      assert.equal(genome.box.top + genome.box.height, 58, 'stands on the ground line');
      const { left, right } = genome.eyes;
      assert.ok(right[0] - left[0] >= 11, 'eyes never collide');
      for (const [x, y] of [left, right]) {
        assert.ok(x > genome.box.left + 3 && x < genome.box.left + genome.box.width - 3, 'eyes on the body (x)');
        assert.ok(y > genome.box.top + 4 && y < 58 - 6, 'eyes on the body (y)');
      }
      const [tempo, amplitude, irregularity] = [genome.motion.tempo, genome.motion.amplitude, genome.motion.irregularity];
      assert.ok(tempo >= 0.82 && tempo <= 1.22 && amplitude >= 0.8 && amplitude <= 1.2 && irregularity >= 0.15 && irregularity <= 0.85);
    }
  }
});

test('the archetype stays recognisable and the owner colour stays close', () => {
  const hue = (hex) => {
    const v = parseInt(hex.slice(1), 16);
    const [r, g, b] = [(v >> 16) & 255, (v >> 8) & 255, v & 255].map((c) => c / 255);
    const max = Math.max(r, g, b); const min = Math.min(r, g, b);
    if (max === min) return null;
    const d = max - min;
    const h = max === r ? ((g - b) / d + (g < b ? 6 : 0)) : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
    return h * 60;
  };
  for (const seed of SEEDS) {
    const genome = crewGenome(seed);
    const html = renderCrewCreature({ appearance: seed, animationKey: seed.id, taskState: 'idle', mode: 'sleeping' });
    assert.match(html, new RegExp(`is-${seed.shape}`));
    const base = hue(seed.color);
    const shifted = hue(genome.tone.fill);
    if (base === null) assert.equal(shifted, null, `${seed.name}: grey stays grey`);
    else assert.ok(Math.abs(((shifted - base + 540) % 360) - 180) <= 12, `${seed.name}: hue within 12°`);
  }
  // Triangles are triangles: narrow at the top, widest near the ground.
  const tri = crewGenome(SEEDS[2]);
  const numbers = pathPoints(tri.path);
  const points = numbers.reduce((acc, value, i) => (i % 2 ? acc[acc.length - 1].push(value) : acc.push([value]), acc), []);
  const widthNear = (y) => {
    const near = points.filter((p) => Math.abs(p[1] - y) < 2.5).map((p) => p[0]);
    return near.length ? Math.max(...near) - Math.min(...near) : 0;
  };
  assert.ok(widthNear(tri.box.top + tri.box.height * 0.2) < widthNear(58 - 3) * 0.55, 'triangle tapers upward');
});

test('without a member the creature is the neutral ghost, never a member colour or body', () => {
  assert.deepEqual(normalizeCrewAppearance(), { name: 'Crew', color: '#7d7f84', shape: 'round' });
  assert.deepEqual(normalizeCrewAppearance({ name: '', shape: 'triangle', color: '#abcdef' }), { name: 'Crew', color: '#7d7f84', shape: 'round' });
  assert.deepEqual(normalizeCrewAppearance({ id: ' crew-a ', name: '  Mira  ', shape: 'triangle', color: '#AbCdEf' }), { id: 'crew-a', name: 'Mira', shape: 'triangle', color: '#AbCdEf' });
  assert.deepEqual(normalizeCrewAppearance({ name: 'Mira', shape: 'unrecognized', color: 'red;position:fixed' }), { name: 'Mira', color: '#7d7f84', shape: 'round' });
  const ghost = renderCrewCreature({ appearance: null, animationKey: 'a', taskState: 'idle', mode: 'sleeping' });
  assert.match(ghost, /is-neutral/);
  assert.doesNotMatch(ghost, /ctox-crew-shine|ctox-crew-cheeks|--crew-fill/);
  const lumi = renderCrewCreature({ appearance: SEEDS[2], animationKey: 'b', taskState: 'idle', mode: 'sleeping' });
  assert.doesNotMatch(lumi, /is-neutral/, 'a grey member is not the ghost');
  assert.match(CREW_CREATURE_BASE_CSS, /\.is-neutral \.ctox-crew-body \{[^}]*stroke-dasharray/);
});

test('every mode draws its own face', () => {
  const faces = {
    sleeping: /ctox-crew-eyes-sleeping/, failed: /ctox-crew-eyes-x/, review: /ctox-crew-eyes-review/,
    reading: /ctox-crew-eyes-reading/, learning: /ctox-crew-eyes-learning/,
  };
  for (const [mode, pattern] of Object.entries(faces)) {
    const html = renderCrewCreature({ appearance: SEEDS[0], animationKey: 'k', taskState: mode, mode });
    assert.match(html, pattern, mode);
  }
  const working = renderCrewCreature({ appearance: SEEDS[0], animationKey: 'k', taskState: 'running', mode: 'working' });
  assert.doesNotMatch(working, /ctox-crew-eyes-(sleeping|x|review|reading|learning)/);
  assert.match(working, /data-crew-motion="[\d.]+,[\d.]+,[\d.]+,[\d.]+"/);
});

test('the chat adapter renders the same member body as the pure renderer', () => {
  const chat = { crewKey: 'cmd-1', crewIdentity: SEEDS[3], executionProgress: null };
  const html = crewCreatureHtml(chat, 'running', 'window');
  assert.ok(html.includes(crewGenome(SEEDS[3]).path));
  assert.match(html, /is-running is-working is-blob is-window/);
});

test('crew renderer escapes caller values without interpreting them as markup or identity', () => {
  const malicious = '" onmouseover="alert(1)';
  const html = renderCrewCreature({
    appearance: { name: malicious, color: '#7d7f84', shape: 'round' },
    animationKey: malicious, taskState: 'idle', mode: 'sleeping',
    activity: { total: malicious, lastKind: malicious, updatedAt: malicious },
  });
  assert.doesNotMatch(html, / onmouseover="/);
  assert.match(html, /data-activity-turns="&quot; onmouseover=&quot;alert\(1\)"/);
  assert.match(html, /--crew-color:#7d7f84/);
});

test('creature CSS carries no keyframe loops and standalone hosts keep reduced-motion rules', () => {
  assert.doesNotMatch(CREW_CREATURE_BASE_CSS, /@keyframes|animation:/);
  assert.doesNotMatch(CREW_CREATURE_BASE_CSS, /filter:\s*drop-shadow/, 'no per-frame filter on moving bodies');
  assert.ok(CREW_CREATURE_CSS.startsWith(CREW_CREATURE_BASE_CSS));
  assert.match(CREW_CREATURE_CSS, /prefers-reduced-motion: reduce/);
  assert.match(CREW_CREATURE_CSS, /animation: none !important/);
});
