import test from 'node:test';
import assert from 'node:assert/strict';
import { parseUrlParts } from '../_lib/util.js';

test('URL fallback without WHATWG URL (bare vm context)', () => {
  const saved = globalThis.URL;
  try {
    globalThis.URL = undefined;
    assert.deepEqual(parseUrlParts('https://Example.org/paper?x=1'), { scheme: 'https', host: 'example.org' });
    assert.deepEqual(parseUrlParts('http://user@host.de:8080/'), { scheme: 'http', host: 'host.de' });
    assert.equal(parseUrlParts('not a url'), null);
    assert.equal(parseUrlParts('https:///nohost')?.host, '');
  } finally {
    globalThis.URL = saved;
  }
});
