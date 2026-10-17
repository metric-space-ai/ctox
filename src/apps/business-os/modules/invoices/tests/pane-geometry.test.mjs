// Geometrie-Labor G1/G4 (Shell v4): Positionstabelle folgt der eigenen Pane, Pane-Titel brechen um statt Ellipse.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const css = await readFile(new URL('../index.css', import.meta.url), 'utf8');

test('G1: the detail pane is a size container and the line table stacks by pane width, not window width', () => {
  assert.match(css, /\.invoices-main \{ container: invoices-pane \/ inline-size; \}/);
  assert.match(css, /@container invoices-pane \(max-width: 760px\) \{\s*\[data-app="invoices"\] \.invoices-lines-table \{ min-width: 0; \}/);
  assert.doesNotMatch(css, /@media[^{]*(?:min|max)-width/);
});

test('G4: pane titles wrap at word boundaries, the title row grows, no ellipsis', () => {
  const title = css.match(/\[data-app="invoices"\] \.invoices-module \.ctox-pane-title \{([^}]*)\}/)?.[1] ?? '';
  assert.match(title, /white-space: normal/);
  assert.match(title, /text-overflow: clip/);
  assert.doesNotMatch(title, /ellipsis|nowrap/);
  const row = css.match(/\[data-app="invoices"\] \.invoices-module \.ctox-pane-title-row \{([^}]*)\}/)?.[1] ?? '';
  assert.match(row, /height: auto/);
  assert.match(css, /\.ctox-pane-title-row \{\s*flex-wrap: wrap;/);
});
