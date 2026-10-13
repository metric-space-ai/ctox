const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function fixture(file, input, response) {
  const calls = [];
  const output = [];
  const module = { exports: {} };
  const requireMock = (name) => {
    assert.equal(name, 'child_process');
    return { execFileSync: (bin, args, options) => {
      calls.push({ bin, args, source: options.input });
      return JSON.stringify(typeof response === 'function' ? response(args) : response);
    } };
  };
  requireMock.main = {};
  const context = { module, require: requireMock, URL, process: {
    env: { CTOX_SCRAPE_INPUT_JSON: JSON.stringify(input) },
    stdout: { write: (value) => output.push(value) }, stderr: { write: () => {} },
  } };
  vm.createContext(context);
  const source = fs.readFileSync(path.join(__dirname, file), 'utf8');
  vm.runInContext(source + '\nglobalThis.probe = { main, capture: typeof serpPage === "function" ? serpPage : browserPage };', context);
  return { calls, output, probe: context.probe };
}

const input = { company: 'Example GmbH', country: 'DE', owner_user_id: 'owner-fixture', task_id: 'command-fixture' };
for (const [file, sourceId] of [
  ['google-de-authenticated.js', 'google.de'],
  ['google-maps-authenticated.js', 'maps.google.com'],
]) {
  test(`${sourceId}: authenticated capture forwards native owner/task binding`, () => {
    const f = fixture(file, input, { ok: true, session_id: 'session-fixture', authenticated_login: { status: 'completed' }, url: 'https://www.google.de/search', results: [] });
    const result = f.probe.capture(input.company, input.country);
    assert.equal(result.ok, true);
    assert.equal(f.calls.length, 1);
    const call = f.calls[0];
    assert.deepEqual(Array.from(call.args.slice(0, 3)), ['business-os', 'web-stack', 'authenticated-automation']);
    const value = (flag) => call.args[call.args.indexOf(flag) + 1];
    assert.equal(value('--source-id'), sourceId);
    assert.equal(value('--credential-ref'), 'ctox-secret://credentials/GOOGLE_BROWSER_LOGIN');
    assert.equal(value('--owner-user-id'), input.owner_user_id);
    assert.equal(value('--task-id'), input.task_id);
    assert.ok(call.source.includes('page.'));
    assert.equal((result.result || result).session_id, 'session-fixture');
  });

  test(`${sourceId}: missing identity never starts an anonymous provider call`, () => {
    for (const key of ['owner_user_id', 'task_id']) {
      const incomplete = { ...input };
      delete incomplete[key];
      const f = fixture(file, incomplete, { ok: true });
      assert.equal(f.probe.capture(input.company, input.country).ok, false);
      assert.equal(f.calls.length, 0);
    }
  });

  test(`${sourceId}: incomplete login/session remains authorization_required`, () => {
    for (const response of [
      { ok: false },
      { ok: true, authenticated_login: { status: 'completed' } },
      { ok: true, session_id: 'session-fixture', authenticated_login: { status: 'mfa_required' } },
    ]) {
      const f = fixture(file, input, response);
      f.probe.main();
      assert.equal(JSON.parse(f.output.join('')).failure_mode, 'authorization_required');
      assert.ok(f.calls.every((call) => !(call.args[0] === 'web' && ['search', 'browser-automation'].includes(call.args[1]))));
    }
  });
}

test('Google empty authenticated result does not reference an absent search response', () => {
  const f = fixture('google-de-authenticated.js', input, { ok: true, session_id: 'session-fixture', authenticated_login: { status: 'completed' }, results: [], url: 'https://www.google.de/search', page_text_excerpt: '' });
  f.probe.main();
  assert.equal(JSON.parse(f.output.join('')).failure_mode, 'portal_drift');
  assert.equal(f.calls.length, 1);
});

test('Maps never substitutes a previous company when input is missing', () => {
  const f = fixture('google-maps-authenticated.js', {}, { ok: true });
  f.probe.main();
  assert.equal(JSON.parse(f.output.join('')).failure_mode, 'invalid_input');
  assert.equal(f.calls.length, 0);
});
