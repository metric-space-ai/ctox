import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { fileURLToPath } from 'node:url';

import { build } from 'esbuild';

// Bundle the browser module exactly as the shell would load it, then import
// the pure test hooks. This proves the module's logic evaluates without error
// and honours the write-only contract; the server side is covered by the Rust
// guard test ctox_secret_put_keeps_value_out_of_command_record_and_lists_metadata.
const bundled = await build({
  entryPoints: [fileURLToPath(new URL('./index.js', import.meta.url))],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  write: false,
});
const [{ text: bundledSource }] = bundled.outputFiles;
const { __credentialsTestHooks: hooks } = await import(
  `data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`
);

test('credential key validation matches the server UPPER_SNAKE_CASE rule', () => {
  for (const ok of ['OPENAI_API_KEY', 'A', 'A1_B', 'CTO_IOT_MQTT_PASSWORD']) {
    assert.ok(hooks.KEY_RE.test(ok), `expected valid: ${ok}`);
  }
  for (const bad of ['', 'lower', '1ABC', 'HAS-DASH', 'HAS SPACE', '_LEADING']) {
    assert.ok(!hooks.KEY_RE.test(bad), `expected invalid: ${bad}`);
  }
  assert.ok(!hooks.KEY_RE.test('X'.repeat(65)), 'over 64 chars must be rejected');
});

test('rowHtml renders metadata and never emits a secret value', () => {
  const setRow = hooks.rowHtml(
    { name: 'OPENAI_API_KEY', description: 'OpenAI API key', is_set: true, updated_at: '2026-06-18T00:00:00Z' },
    false,
  );
  assert.match(setRow, /OPENAI_API_KEY/);
  assert.match(setRow, /data-action="delete"/); // remove offered when set
  assert.match(setRow, /type="password"/); // the value input is masked
  assert.doesNotMatch(setRow, /value="/); // no value attribute is ever rendered

  const unsetRow = hooks.rowHtml({ name: 'ANTHROPIC_API_KEY', description: '', is_set: false }, false);
  assert.match(unsetRow, /ANTHROPIC_API_KEY/);
  assert.doesNotMatch(unsetRow, /data-action="delete"/); // nothing to remove when unset
});

test('dispatched command targets the credentials module and a secret command type', () => {
  const doc = hooks.buildCommandDoc('ctox.secret.put', { name: 'OPENAI_API_KEY', value: 'x' }, 'cmd_test');
  assert.equal(doc.id, 'cmd_test');
  assert.equal(doc.module, 'credentials');
  assert.equal(doc.command_type, 'ctox.secret.put');
  assert.equal(doc.record_id, 'OPENAI_API_KEY');
  assert.equal(doc.inbound_channel, 'business_os.credentials');
});
