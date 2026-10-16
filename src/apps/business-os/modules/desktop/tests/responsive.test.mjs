import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const css = readFileSync(new URL('../index.css', import.meta.url), 'utf8');
const js = readFileSync(new URL('../index.js', import.meta.url), 'utf8');
const dragJs = readFileSync(new URL('../iconDrag.js', import.meta.url), 'utf8');

assert.match(css, /\.desktop-module\s*\{[\s\S]*container-type:\s*inline-size/);
assert.match(css, /@container \(max-width: 1320px\)\s*\{[\s\S]*\.desktop-widget-container\s*\{[\s\S]*display:\s*none/);
assert.match(css, /\.desktop-hero-widget\s*\{[\s\S]*width:\s*248px/);
assert.doesNotMatch(css, /\.desktop-hero-widget\s*\{[\s\S]*width:\s*334px/);

const selectedIconRule = css.match(/\.desktop-icon\.selected\s*\{([^}]*)\}/u)?.[1] ?? '';
assert.match(selectedIconRule, /background:\s*transparent/u);
assert.match(selectedIconRule, /box-shadow:\s*none/u);
assert.doesNotMatch(selectedIconRule, /border-color/u);

const glyphRule = css.match(/\.desktop-icon \.desktop-icon-glyph\s*\{([^}]*)\}/u)?.[1] ?? '';
assert.match(glyphRule, /background:\s*transparent\s*!important/u);
assert.match(glyphRule, /border:\s*0\s*!important/u);
assert.match(glyphRule, /box-shadow:\s*none\s*!important/u);
assert.doesNotMatch(
  css,
  /:root\[data-shell-style="ctox"\] \.desktop-icon(?::hover)? \.desktop-icon-glyph/u,
  'a shell theme must not reintroduce a second tile around app artwork',
);

const selectedLabelRule = css.match(/\.desktop-icon\.selected \.desktop-icon-label\s*\{([^}]*)\}/u)?.[1] ?? '';
assert.match(selectedLabelRule, /background:\s*transparent/u);
assert.match(css, /--desktop-icon-cell-w:\s*120px/u);
assert.match(css, /--desktop-icon-cell-h:\s*140px/u);
// Kompaktraster folgt der Fensterbreite (Container), nicht dem Viewport (Geometrie-Labor G6).
assert.match(css, /@container \(max-width: 767px\)[\s\S]*--desktop-icon-cell-h:\s*132px/u);
assert.doesNotMatch(css, /@media[^{]*(?:min|max)-width/u);
assert.match(css, /\.desktop-icon:focus-visible \.desktop-icon-glyph/u);
assert.match(js, /const DEFAULT_GRID = \{ cellW: 120, cellH: 140,/u);
assert.match(js, /const COMPACT_GRID = \{ cellW: 88, cellH: 132,/u);
assert.match(js, /width:\s*112,[\s\S]*height:\s*136,/u, 'drag bounds use the rendered desktop icon size');
assert.match(js, /compactWidth:\s*88,[\s\S]*compactHeight:\s*128,/u);
assert.match(js, /return 'CTOX Backend';/u);
assert.match(js, /el\.setAttribute\('role', 'button'\)/u);
assert.match(js, /el\.setAttribute\('aria-label', label\)/u);
assert.match(js, /const columns = Math\.max\(1, Math\.floor\(usableWidth \/ grid\.cellW\)\)/u);
assert.match(js, /x: grid\.offset \+ \(index % columns\) \* grid\.cellW/u);
// v4: Zeilen-Migration per localStorage-Flag entfällt; Kollisionen verteilt normalizeIconLayoutIfNeeded je Montage.
assert.match(js, /function normalizeIconLayoutIfNeeded\(\)/u);
assert.match(js, /const position = clampIconPosition\(doc, doc, currentGrid\(\)\)/u);
assert.doesNotMatch(js, /const rows = Math\.max\(1, Math\.floor\(usableHeight \/ grid\.cellH\)\)/u);
assert.match(js, /onReorder: reorderIcons/u);
assert.match(js, /dataset\.workjetMobileHost === 'true'\) return/u);
assert.match(js, /api\.patch\('desktop_icons', id, \{ sort_index: sortIndex/u);
assert.match(dragJs, /TOUCH_REORDER_HOLD_MS = 360/u);
assert.match(dragJs, /addEventListener\('touchstart', onTouchStart/u);
assert.match(dragJs, /addEventListener\('touchmove', onTouchMove, \{ passive: false \}\)/u);
assert.match(dragJs, /\.desktop-icon\[data-icon-id\]/u);

console.log('ok - desktop icons keep one unboxed surface and responsive row-major layout');
