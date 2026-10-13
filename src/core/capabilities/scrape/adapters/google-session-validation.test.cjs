const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const vm = require('node:vm');

test('migration hashes bind both reviewed script artifacts and their current production predecessors', () => {
  const manifest = JSON.parse(fs.readFileSync(path.join(__dirname, 'google-session-migration.json'), 'utf8'));
  assert.equal(manifest.automatic_install, false);
  const predecessors = {
    'google-de': [25, '5c39e1fbcf763e02d03b7c2703719af40255ef380bb0f1207fa8d83d03abb3e4'],
    'maps-google-com': [31, 'fb49fae926e20d9730fa61be46beff4d9a26cbd975b65c1da02031d961015cd6'],
  };
  assert.equal(manifest.updates.length, 2);
  for (const update of manifest.updates) {
    assert.deepEqual([update.expected_current_revision, update.expected_current_script_sha256], predecessors[update.target_key]);
    const body = fs.readFileSync(path.join(__dirname, path.basename(update.source_path)), 'utf8');
    assert.equal(crypto.createHash('sha256').update(body.trim()).digest('hex'), update.script_sha256);
  }
});

for (const file of ['google-de-authenticated.js', 'google-maps-authenticated.js']) {
  test(`${file}: login success never converts a Google access challenge into success or no_match`, () => {
    const output = [];
    const module = { exports: {} };
    const requireMock = (name) => {
      assert.equal(name, 'child_process');
      return { execFileSync: () => JSON.stringify({
        ok: true, session_id: 'session-fixture', authenticated_login: { status: 'completed' },
        url: 'https://www.google.com/sorry/', title: 'Access challenge',
        body_text: 'Our systems have detected unusual traffic. CAPTCHA.',
        page_text_excerpt: 'Our systems have detected unusual traffic. CAPTCHA.', results: [],
      }) };
    };
    requireMock.main = {};
    const context = { module, require: requireMock, URL, process: {
      env: { CTOX_SCRAPE_INPUT_JSON: JSON.stringify({ company: 'Example GmbH', country: 'DE', owner_user_id: 'owner-fixture', task_id: 'command-fixture' }) },
      stdout: { write: (v) => output.push(v) }, stderr: { write: () => {} },
    } };
    vm.createContext(context);
    vm.runInContext(fs.readFileSync(path.join(__dirname, file), 'utf8') + '\nmain();', context);
    const result = JSON.parse(output.join(''));
    assert.equal(result.failure_mode, 'blocked');
    assert.deepEqual(result.records, []);
  });
}
