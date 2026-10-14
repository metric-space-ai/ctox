const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');

test('migration hashes bind both reviewed script artifacts and their production predecessors', () => {
  const manifest = JSON.parse(fs.readFileSync(path.join(__dirname, 'google-session-migration.json'), 'utf8'));
  assert.equal(manifest.automatic_install, false);
  const predecessors = {
    'google-de': [32, 'd7a2e2ba0c6213e2fbe6a7e480a15963343788ac4a609e8256e5d35a3f5a17dd'],
    'maps-google-com': [32, 'b4f393be07f4586c12e8239cc7b8787a9e70e70a6f2ec6dbbc9c2fac4d1c3ffa'],
  };
  assert.equal(manifest.updates.length, 2);
  for (const update of manifest.updates) {
    assert.deepEqual([update.expected_current_revision, update.expected_current_script_sha256], predecessors[update.target_key]);
    const body = fs.readFileSync(path.join(__dirname, path.basename(update.source_path)), 'utf8');
    assert.equal(crypto.createHash('sha256').update(body.trim()).digest('hex'), update.script_sha256);
  }
});

test('neither adapter drives a Google browser session for the SERP any more', () => {
  for (const file of ['google-de-authenticated.js', 'google-maps-authenticated.js']) {
    const source = fs.readFileSync(path.join(__dirname, file), 'utf8');
    assert.ok(!source.includes('authenticated-automation'), `${file} still starts an authenticated Google session`);
    assert.ok(!source.includes('GOOGLE_BROWSER_LOGIN'), `${file} still references the Google login`);
    assert.ok(source.includes('api.brightdata.com'), `${file} does not use the Bright Data SERP API`);
    assert.ok(!/["']Accept["']\s*:/.test(source), `${file} sends an Accept header (Bright Data then answers 200 with an empty body)`);
  }
});
