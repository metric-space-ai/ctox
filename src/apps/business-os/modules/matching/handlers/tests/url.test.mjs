import test from 'node:test';
import assert from 'node:assert/strict';
import { originUrl } from '../_lib/job.js';

test('origin_url fallback without WHATWG URL (bare vm context)', () => {
  const saved = globalThis.URL;
  try {
    globalThis.URL = undefined;
    assert.equal(originUrl('https://Jobs.Example.org/a?id=1'), 'https://jobs.example.org');
    assert.equal(originUrl('kein url'), null);
  } finally {
    globalThis.URL = saved;
  }
});
