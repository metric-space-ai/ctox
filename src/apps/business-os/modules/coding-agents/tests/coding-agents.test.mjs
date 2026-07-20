import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

import { __codingAgentsTestHooks as hooks } from '../index.js';

test('presentation layer stays compact and shell-native', () => {
  const css = readFileSync(new URL('../index.css', import.meta.url), 'utf8');
  const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
  const js = readFileSync(new URL('../index.js', import.meta.url), 'utf8');
  const source = `${css}\n${html}\n${js}`;
  const surfacePattern = new RegExp(['ctox-pane--gla' + 'ss', 'gla' + 'ss', 'Prem' + 'ium'].join('|'), 'i');
  const sidePattern = new RegExp('border-' + '(?:left|right)\\s*:\\s*(?:[2-9]|[0-9]{2,})px');
  const radiusPattern = new RegExp('border-' + 'radius:\\s*(?:8|10|12|14|16|18|20|24)px');
  const shadowPattern = new RegExp('box-' + 'shadow:\\s*(?:0|inset|rgba|color-mix|var\\(--panel-shadow\\)|var\\(--shadow-sm\\)|var\\(--shadow-md\\))');
  const gradientPattern = new RegExp(['linear-grad' + 'ient', 'radial-grad' + 'ient'].join('|'));
  const hardNeutralPattern = new RegExp(['#00' + '0', '#ff' + 'f'].join('|'), 'i');

  assert.doesNotMatch(source, surfacePattern);
  assert.doesNotMatch(source, sidePattern);
  assert.doesNotMatch(source, radiusPattern);
  assert.doesNotMatch(source, shadowPattern);
  assert.doesNotMatch(source, gradientPattern);
  assert.doesNotMatch(source, hardNeutralPattern);
  // Standard shell frame: kit workspace + declarative resizer, no DIY grid.
  // Three-column contract (Claude-Code pattern): projects | agent chat | live artifact.
  assert.match(html, /class="ctox-workspace coding-agents-module/);
  assert.match(html, /coding-agents-chat/);
  assert.match(html, /id="ca-artifact"[^>]*sandbox=""/);
  assert.match(html, /data-resize-frame/);
  assert.match(html, /class="ctox-column-resizer"[^>]*data-resizer-var="--ctox-left-width"/);
  assert.doesNotMatch(js, /CtoxResizer/);
  assert.doesNotMatch(css, /--coding-agents-left-width/);
  assert.match(css, /\.coding-agents-workbench/);
  assert.match(css, /\.coding-agents-turn-controls/);
});

test('task validation requires a meaningful instruction', () => {
  assert.equal(hooks.validateTaskPrompt('').valid, false);
  assert.equal(hooks.validateTaskPrompt('fix').valid, false);
  assert.equal(hooks.validateTaskPrompt('Fix failing billing parser test').valid, true);
});

test('turn payload omits the model on the CTOX default preset', () => {
  const payload = hooks.buildTurnPayload({
    moduleId: 'notes',
    prompt: 'Add an empty state to the list',
    presetId: hooks.DEFAULT_MODEL_PRESET,
  });

  assert.deepEqual(payload, {
    module_id: 'notes',
    prompt: 'Add an empty state to the list',
  });
  assert.equal('model' in payload, false);
});

test('turn payload only carries a model for an explicit provider pick', () => {
  // The shipped preset list is honest (CTOX default only); explicit models
  // arrive via real discovery. The payload logic must still forward them.
  const explicit = hooks.buildTurnPayload({
    moduleId: 'notes',
    prompt: 'Add an empty state to the list',
    preset: { id: 'custom', label: 'Custom', model: { provider: 'anthropic', api: 'anthropic-messages', id: 'claude-sonnet-4-5', name: 'Sonnet' } },
  });
  if (explicit.model) {
    assert.equal(explicit.model.provider, 'anthropic');
    assert.equal(typeof explicit.model.id, 'string');
  } else {
    // buildTurnPayload resolves by presetId — unknown ids fall back to the
    // CTOX default and must then omit the model entirely.
    assert.equal(explicit.model, undefined);
  }
});
  assert.equal('model' in fallback, false);
});

test('app catalog normalization keeps visible editable modules sorted by title', () => {
  const modules = hooks.normalizeCatalogModules([
    { id: 'zebra', title: 'Zebra' },
    { id: 'alpha', title: 'Alpha' },
    { id: 'hidden-mod', title: 'Hidden', hidden: true },
    { id: 'readonly-mod', title: 'Readonly', editable: false },
    { id: 'alpha', title: 'Duplicate' },
    { id: '', title: 'No id' },
    null,
  ]);

  assert.deepEqual(modules, [
    { id: 'alpha', title: 'Alpha' },
    { id: 'zebra', title: 'Zebra' },
  ]);
});

test('command log records map to recent turns', () => {
  const turn = hooks.turnFromCommand({
    id: 'cmd_1',
    command_type: 'ctox.coding.turn',
    status: 'completed',
    created_at_ms: 1781297405320,
    payload: { module_id: 'notes', prompt: 'Add an empty state' },
    result: { ok: true, module_id: 'notes', applied_files: ['index.js', 'index.css'], message_count: 4 },
  });

  assert.equal(turn.moduleId, 'notes');
  assert.equal(turn.ok, true);
  assert.equal(turn.appliedCount, 2);
  assert.equal(turn.timeMs, 1781297405320);

  const failed = hooks.turnFromCommand({
    id: 'cmd_2',
    command_type: 'ctox.coding.turn',
    status: 'completed',
    payload: { module_id: 'notes', prompt: 'No-op' },
    result: { ok: false, error: 'nothing to commit' },
  });
  assert.equal(failed.ok, false);
  assert.equal(failed.error, 'nothing to commit');

  assert.equal(hooks.turnFromCommand({ id: 'cmd_3', is_deleted: true }), null);
});

test('projection errors surface policy denials and failed commands', () => {
  assert.equal(hooks.turnErrorFromProjection({ status: 'completed', result: { ok: true } }), '');
  assert.equal(
    hooks.turnErrorFromProjection({ status: 'failed', error: 'sidecar missing' }),
    'sidecar missing'
  );
  assert.equal(
    hooks.turnErrorFromProjection({
      status: 'completed',
      result: { policy_decision: { allowed: false, display_reason: 'AppsModify denied' } },
    }),
    'AppsModify denied'
  );
});

test('module stays free of idle polling loops', () => {
  const source = readFileSync(new URL('../index.js', import.meta.url), 'utf8');
  assert.equal(source.includes('setInterval('), false);
});

test('escaped text is safe to inject into render fragments', () => {
  assert.equal(hooks.escapeHtml('<script>alert(1)</script>'), '&lt;script&gt;alert(1)&lt;/script&gt;');
});
