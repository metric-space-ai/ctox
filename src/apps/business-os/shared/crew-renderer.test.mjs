import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { crewCreatureHtml } from './business-chat.js';
import {
  normalizeCrewAppearance,
  renderCrewCreature,
  CREW_CREATURE_BASE_CSS,
  CREW_CREATURE_CSS,
} from './crew-renderer.js';

const fixture = JSON.parse(readFileSync(new URL('./fixtures/crew-renderer-baseline.json', import.meta.url)));
const hash = value => createHash('sha256').update(value).digest('hex');

test('crew renderer and legacy adapter match all 360 pre-extraction reference renders', () => {
  assert.equal(fixture.sourceCommit, 'e00ecbeb131808797aafb04fad5862a8ea17aa4c');
  assert.equal(Object.keys(fixture.sha256ByCase).length, 360);
  for (const [key, expected] of Object.entries(fixture.sha256ByCase)) {
    const [shape, taskState, placement, phase] = key.split('/');
    const appearance = { name: 'Mira', shape, color: '#1685ee' };
    const chat = {
      crewKey: 'member:fixture-42', crewIdentity: appearance,
      executionProgress: {
        phase, percent: 82,
        steps: [{ position: 1, label: 'Review', status: 'completed' }],
        activity_turns: { total: 6, thinking: 4, tools: 2, last_kind: 'thinking' },
        updated_at_ms: 1720000000000,
      },
    };
    const mode = taskState === 'running'
      ? (phase === 'review' ? 'review' : 'working')
      : ['idle', 'queued', 'scheduled', 'success'].includes(taskState) ? 'sleeping' : taskState;
    const rendered = renderCrewCreature({
      appearance, animationKey: chat.crewKey, taskState, mode, placement,
      progressPercent: 82, activity: { total: 6, lastKind: 'thinking', updatedAt: 1720000000000 },
    });
    // Main now carries the normalized appearance in one data attribute for
    // procedural motion. Verify that addition and retain every historical
    // byte-equivalence assertion for the rest of the extracted renderer.
    for (const [label, html] of [['pure renderer', rendered], ['chat adapter', crewCreatureHtml(chat, taskState, placement)]]) {
      const identities = [...html.matchAll(/ data-crew-identity="([^"]*)"/g)];
      assert.equal(identities.length, 1, `${label} identity count: ${key}`);
      assert.deepEqual(JSON.parse(identities[0][1].replaceAll('&quot;', '"')), appearance, `${label} identity: ${key}`);
      assert.equal(hash(html.replace(identities[0][0], '')), expected, `${label}: ${key}`);
    }
  }
});

test('crew appearance preserves explicit names/colors/shapes and existing neutral fallbacks', () => {
  const neutral = { name: 'Crew', color: '#7d7f84', shape: 'round' };
  assert.deepEqual(normalizeCrewAppearance(), neutral);
  assert.deepEqual(normalizeCrewAppearance({ name: '', shape: 'triangle', color: '#abcdef' }), neutral);
  assert.deepEqual(normalizeCrewAppearance({ name: '  Mira  ', shape: 'triangle', color: '#AbCdEf' }), { name: 'Mira', shape: 'triangle', color: '#AbCdEf' });
  assert.deepEqual(normalizeCrewAppearance({ name: 'Mira', shape: 'unrecognized', color: 'red;position:fixed' }), { ...neutral, name: 'Mira' });
  const a = renderCrewCreature({ appearance: neutral, animationKey: 'a', taskState: 'idle', mode: 'sleeping' });
  const b = renderCrewCreature({ appearance: neutral, animationKey: 'b', taskState: 'idle', mode: 'sleeping' });
  assert.match(a, /is-round/);
  assert.match(b, /is-round/);
  assert.match(a, /--crew-color:#7d7f84/);
  assert.match(b, /--crew-color:#7d7f84/);
});

test('crew renderer escapes caller values without interpreting them as markup or identity', () => {
  const malicious = '\" onmouseover=\"alert(1)';
  const html = renderCrewCreature({
    appearance: { name: malicious, color: '#7d7f84', shape: 'round' },
    animationKey: malicious, taskState: 'idle', mode: 'sleeping',
    activity: { total: malicious, lastKind: malicious, updatedAt: malicious },
  });
  assert.doesNotMatch(html, / onmouseover="/);
  assert.match(html, /data-activity-turns="&quot; onmouseover=&quot;alert\(1\)"/);
  assert.match(html, /--crew-color:#7d7f84/);
});

test('crew base CSS is byte-equivalent and standalone styles retain reduced-motion rules', () => {
  assert.equal(hash(CREW_CREATURE_BASE_CSS), fixture.baseCssSha256);
  assert.ok(CREW_CREATURE_CSS.startsWith(CREW_CREATURE_BASE_CSS));
  assert.match(CREW_CREATURE_CSS, /prefers-reduced-motion: reduce/);
  assert.match(CREW_CREATURE_CSS, /animation: none !important/);
  assert.match(CREW_CREATURE_CSS, /transition: none !important/);
});
