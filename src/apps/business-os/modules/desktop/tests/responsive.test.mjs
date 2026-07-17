import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const css = readFileSync(new URL('../index.css', import.meta.url), 'utf8');

assert.match(css, /\.desktop-module\s*\{[\s\S]*container-type:\s*inline-size/);
assert.match(css, /@container \(max-width: 1320px\)\s*\{[\s\S]*\.desktop-widget-container\s*\{[\s\S]*display:\s*none/);
assert.match(css, /\.desktop-hero-widget\s*\{[\s\S]*width:\s*248px/);
assert.doesNotMatch(css, /\.desktop-hero-widget\s*\{[\s\S]*width:\s*334px/);

console.log('ok - desktop status widget cannot overlap icons at compact widths');
