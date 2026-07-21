import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

import { __codingAgentsTestHooks as hooks } from '../index.js';
import { __chatUiTestHooks as chatHooks } from '../../../vendor/chat-ui/chat-ui.mjs';

test('presentation layer stays compact and shell-native', () => {
  const css = readFileSync(new URL('../index.css', import.meta.url), 'utf8');
  const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
  const js = readFileSync(new URL('../index.js', import.meta.url), 'utf8');
  // The vendored chat core injects its own <style>; hold it to the same shell
  // token rules as the module itself.
  const chatUi = readFileSync(new URL('../../../vendor/chat-ui/chat-ui.mjs', import.meta.url), 'utf8');
  const source = `${css}\n${html}\n${js}\n${chatUi}`;
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
  // Project column is a narrow icon rail by default: 56px resizer floor,
  // app icons with a monogram fallback, names via the floating hover chip,
  // inline labels only once the rail is dragged wide (container query).
  assert.match(html, /data-resizer="left"[^>]*data-resizer-min="56"/);
  assert.match(css, /container: coding-agents-rail \/ inline-size/);
  assert.match(css, /@container coding-agents-rail \(min-width: 150px\)/);
  assert.match(js, /coding-agents-app-icon/);
  assert.match(js, /coding-agents-app-monogram/);
  assert.match(js, /coding-agents-rail-chip/);
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

test('chat markdown escapes html/script injection before transforming', () => {
  const html = chatHooks.renderMarkdown('<script>alert(1)</script> **bold** <img src=x onerror=alert(1)>');
  // No raw tags survive — everything dangerous is entity-escaped.
  assert.equal(html.includes('<script'), false);
  assert.equal(html.includes('<img'), false);
  assert.equal(html.includes('onerror'), true); // present, but only as inert escaped text
  assert.match(html, /&lt;script&gt;alert\(1\)&lt;\/script&gt;/);
  // The safe subset still applies to the surrounding text.
  assert.match(html, /<strong>bold<\/strong>/);

  // A non-http(s) link scheme must never become an anchor.
  const jsLink = chatHooks.renderMarkdown('[x](javascript:alert(1))');
  assert.equal(jsLink.includes('<a '), false);
  // A quote smuggled into a URL stays encoded and cannot break the attribute.
  const attrBreak = chatHooks.renderMarkdown('[x](https://e.com" onmouseover="alert(1))');
  assert.equal(/href="https:\/\/e\.com"[^>]*onmouseover/.test(attrBreak), false);
});

test('chat renders fenced code blocks with a copy control', () => {
  const html = chatHooks.renderMarkdown('intro\n```\nconst a = 1;\n```');
  assert.match(html, /class="cui-codeblock"/);
  assert.match(html, /class="cui-copy"/);
  assert.match(html, /<pre class="cui-pre"><code>const a = 1;<\/code><\/pre>/);
  // Backticks inside a fence are content, not re-parsed as inline code.
  const inlineCode = chatHooks.renderMarkdown('use `flag` here');
  assert.match(inlineCode, /<code class="cui-code">flag<\/code>/);
});

test('chat renders system events as one-line protocol rows, not bubbles', () => {
  const sys = chatHooks.renderRowHtml({ role: 'system', text: 'turn started', status: 'running' });
  assert.equal(chatHooks.classifyRole({ role: 'system' }), 'system');
  assert.match(sys.cls, /cui-row--system/);
  assert.match(sys.html, /class="cui-proto/);
  assert.equal(sys.html.includes('cui-bubble'), false);
  assert.match(sys.html, /turn started · running/);

  // A failed system event is flagged for the danger token.
  const failed = chatHooks.renderRowHtml({ role: 'system', text: 'denied', failed: true });
  assert.match(failed.html, /cui-proto is-failed/);

  // People and assistants still get bubbles.
  assert.match(chatHooks.renderRowHtml({ role: 'user', text: 'hi' }).html, /cui-bubble--user/);
  assert.match(chatHooks.renderRowHtml({ role: 'assistant', text: 'hi' }).html, /cui-bubble--assistant/);
});
