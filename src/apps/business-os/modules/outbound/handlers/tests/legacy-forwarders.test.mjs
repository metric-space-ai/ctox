import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { createTestCtx, cmd } from './_ctx.mjs';
import './ctox-shared-loader.mjs'; // ctox:shared/ wie der Service Worker (Handler mit prompt-template.js)

const dir = fileURLToPath(new URL('..', import.meta.url));
const handlerFiles = readdirSync(dir).filter((name) => name.endsWith('.js'));
const libFiles = readdirSync(new URL('../_lib/', import.meta.url)).filter((name) => name.endsWith('.js'));
const source = (rel) => readFileSync(new URL(rel, import.meta.url), 'utf8');

// Strang OUT3: die vier früheren legacy-Weiterleitungen sind abgelöst – keine ruft mehr ctx.cap.legacy.
test('no handler is a legacy forwarder and no file calls ctx.cap.legacy (OUT3)', () => {
  const legacy = handlerFiles.filter((name) => source(`../${name}`).startsWith('// LEGACY'));
  assert.deepEqual(legacy, []);
  for (const rel of [...handlerFiles.map((n) => `../${n}`), ...libFiles.map((n) => `../_lib/${n}`)]) {
    assert.doesNotMatch(source(rel), /cap\.legacy|cap\.scrape|cap\.research\.person|legacy\[/, rel);
  }
});

const OWNER = 'outbound-lead-generation';
for (const type of ['outbound.research_source.test', 'outbound.research_source.reprobe', 'outbound.update_digest.send_now']) {
  test(`${type} goes unchanged to the owner app via ctx.command (no core capability)`, async () => {
    const { default: handle } = await import(`../${type}.js`);
    const t = createTestCtx({ commandResult: ({ id }) => ({ id, status: 'accepted' }) });
    const payload = { target_key: 'northdata', keep_script: true, probe_input: { company: 'Acme' }, dry_run: false };
    const command = cmd(type, payload, { id: 'cmd_ui_1', record_id: 'rec_1', client_context: { source_module: OWNER, campaign_id: 'c1' } });
    const result = await handle(command, t.ctx);
    assert.deepEqual(t.calls.commands, [{
      id: `cmd_ui_1:${OWNER}`, type, payload,
      options: { id: `cmd_ui_1:${OWNER}`, module: OWNER, record_id: 'rec_1', client_context: { source_module: OWNER, campaign_id: 'c1' } },
    }]);
    assert.deepEqual(result, { ok: true, delegated_to: OWNER, command_type: type, command_id: `cmd_ui_1:${OWNER}`, status: 'accepted' });
    assert.equal(t.calls.caps.length, 0, 'keine Capability');
  });

  test(`${type}: owner app rejects → unavailable with reason`, async () => {
    const { default: handle } = await import(`../${type}.js`);
    const t = createTestCtx({ commandResult: ({ id }) => ({ id, status: 'rejected', reason: 'forbidden' }) });
    await assert.rejects(handle(cmd(type, {}), t.ctx), { code: 'unavailable', message: `${OWNER} rejected ${type}: forbidden` });
  });
}

test('every handler file exports a default async function', async () => {
  for (const name of readdirSync(dir).filter((n) => n.endsWith('.js'))) {
    const mod = await import(`../${name}`);
    assert.equal(typeof mod.default, 'function', name);
  }
});

test('handlers import no Node builtins', () => {
  const files = readdirSync(dir).filter((n) => n.endsWith('.js')).map((n) => `../${n}`)
    .concat(readdirSync(new URL('../_lib/', import.meta.url)).map((n) => `../_lib/${n}`));
  for (const file of files) {
    const source = readFileSync(new URL(file, import.meta.url), 'utf8');
    assert.doesNotMatch(source, /from ['"](node:|fs|path|crypto|child_process|http|https|net|os)['"]/, file);
    assert.doesNotMatch(source, /\brequire\(/, file);
  }
});
