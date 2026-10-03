'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

// Exercise the actual browser-evaluated decoder without launching the fixture's
// native process/browser. Both the old and corrected expressions fit this seam.
const source = fs.readFileSync(path.join(__dirname, 'browser_rust_smoke.js'), 'utf8');
const start = source.indexOf('async function waitForFileViaDemandFetch(');
assert.ok(start >= 0);
const expression = source.slice(start).match(/const payload = ([^\n]+);\s*\n\s*lastSeen =/)?.[1];
assert.ok(expression, 'demand payload decoder must be exercised from fixture source');
const decode = demandChunks => vm.runInNewContext(expression, { demandChunks, atob });

test('independent demand frames preserve bytes across padded and unpadded boundaries', () => {
  const bytes = Buffer.from(Array.from({ length: 32771 }, (_, i) => i % 256));
  for (const size of [1, 2, 3, 16384]) {
    const chunks = [];
    for (let offset = 0; offset < bytes.length; offset += size) {
      const key = chunks.length % 2 ? 'bytes_base64' : 'bytesBase64';
      chunks.push({ sequence: chunks.length, [key]: bytes.subarray(offset, offset + size).toString('base64') });
    }
    assert.equal(decode(chunks), bytes.toString('latin1'), `frame size ${size}`);
  }
});

test('invalid frame encoding remains an error rather than silently dropping content', () => {
  assert.throws(() => decode([
    { sequence: 0, bytesBase64: 'YQ==' },
    { sequence: 1, bytesBase64: '$not-base64$' },
  ]), /invalid|encoded|character/i);
});
