import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const root = dirname(fileURLToPath(import.meta.url));
const html = readFileSync(join(root, 'index.html'), 'utf8');
const css = readFileSync(join(root, 'index.css'), 'utf8');
const js = readFileSync(join(root, 'index.js'), 'utf8');

test('public booking surface uses Workjet product identity and local assets', () => {
  assert.match(html, /<title>Termin buchen \| Workjet<\/title>/);
  assert.match(html, /\/ui-contract\/v1\/workjet-ui-contract\.css/);
  assert.doesNotMatch(html, /fonts\.googleapis|fonts\.gstatic|Business OS<\/strong>/);
  assert.doesNotMatch(html, /CTOX Desktop|CTOX Mobile|T3 Code|Workjet Alpha/);
});

test('public booking surface follows the shared theme and category contract', () => {
  assert.match(css, /--app-accent:\s*var\(--workjet-category-productivity-accent\)/);
  assert.match(css, /background:\s*var\(--workjet-surface-canvas\)/);
  assert.match(css, /color:\s*var\(--workjet-text-primary\)/);
  assert.match(css, /@media \(max-width: 560px\)/);
  assert.match(css, /@media \(max-width: 380px\)/);
  assert.match(css, /@media \(prefers-reduced-motion: reduce\)/);
  assert.doesNotMatch(css, /radial-gradient|backdrop-filter|rgba\(79,\s*70,\s*229/);
});

test('public booking surface provides German and English without exposing server errors', () => {
  assert.match(js, /de:\s*Object\.freeze/);
  assert.match(js, /en:\s*Object\.freeze/);
  assert.match(js, /new Intl\.DateTimeFormat\(locale/);
  assert.match(js, /data-i18n/);
  assert.doesNotMatch(js, /\.innerHTML\s*=/);
  assert.doesNotMatch(js, /\balert\s*\(/);
  assert.doesNotMatch(js, /err\.error|error\.message/);
  assert.match(js, /slotsRetryMs = Math\.min\(slotsRetryMs \* 2, 60_000\)/);
  assert.match(js, /attempt >= 1/);
});

test('public booking surface keeps secrets out of URLs and logs', () => {
  assert.match(js, /hold_token:\s*state\.activeHold\.token/);
  assert.doesNotMatch(js, /console\.(?:log|info|warn|error)/);
  assert.doesNotMatch(js, /[?&](?:token|hold_token)=/);
  assert.match(js, /encodeURIComponent\(state\.slug\)/);
});

test('public booking interactions expose accessible names and live status', () => {
  assert.match(html, /role="alert" aria-live="assertive"/);
  assert.match(html, /data-i18n-aria-label="previousMonth"/);
  assert.match(html, /data-i18n-aria-label="nextMonth"/);
  assert.match(html, /role="grid" aria-label="Kalender"/);
  assert.match(html, /autocomplete="name"/);
  assert.match(html, /autocomplete="email"/);
});
