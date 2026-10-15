import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

const css = readFileSync(new URL('../../customer-modules/outbound-lead-generation/index.css', import.meta.url), 'utf8');

// A td with display:grid/flex/block leaves the table row; its bottom border then
// sits at its own height and the row separators no longer line up.
test('lead table cells stay table cells so row separators line up', () => {
  const offending = [...css.matchAll(/([^{}]+)\{([^}]*)\}/g)]
    .filter(([, selector, body]) => selector.split(',').some((part) => /\btd(?::[\w()-]+)*\s*$/.test(part.trim()))
      && /display\s*:\s*(grid|flex|block)\b/.test(body))
    .map(([, selector]) => selector.trim());
  assert.deepEqual(offending, []);
});
