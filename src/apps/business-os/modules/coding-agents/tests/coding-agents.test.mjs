import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

// Strang AGENT: die App ist die Übersicht aller Agent-Läufe (der Agent sitzt als Seitenleiste in der Code-Ansicht).
// Eigenständig lauffähig ohne DOM: index.js lädt Markup erst in mount().
const { arrangeRuns } = await import('../index.js');

test('presentation layer stays compact and shell-native', () => {
  const css = readFileSync(new URL('../index.css', import.meta.url), 'utf8');
  const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
  const js = readFileSync(new URL('../index.js', import.meta.url), 'utf8');
  const source = `${css}\n${html}\n${js}`;
  const surfacePattern = new RegExp(['ctox-pane--gla' + 'ss', 'gla' + 'ss', 'Prem' + 'ium'].join('|'), 'i');
  const sidePattern = new RegExp('border-' + '(?:left|right)\\s*:\\s*(?:[2-9]|[0-9]{2,})px');
  const radiusPattern = new RegExp('border-' + 'radius:\\s*(?:8|10|12|14|16|18|20|24)px');
  const shadowPattern = new RegExp('box-' + 'shadow:\\s*(?:0|inset|rgba|color-mix|var\\(--panel-shadow\\)|var\\(--shadow-sm\\)|var\\(--shadow-md\\))');
  const gradientPattern = new RegExp(['linear-grad' + 'ient', 'radial-grad' + 'ient'].join('|'));
  const hardNeutralPattern = new RegExp(['#00' + '0', '#ff' + 'f'].join('|'), 'i');
  assert.doesNotMatch(source, surfacePattern);
  assert.doesNotMatch(source, sidePattern);
  assert.doesNotMatch(source, radiusPattern);
  assert.doesNotMatch(source, shadowPattern);
  assert.doesNotMatch(source, gradientPattern);
  assert.doesNotMatch(source, hardNeutralPattern);
  // M-04: Standard-Pane-Kopf, kein eigenes Icon/Abstandshalter im Kopf (das App-Icon oben links gehört der Shell).
  assert.match(html, /class="ctox-pane-header ctox-pane-band"/);
  assert.doesNotMatch(html, /rail-mark|rail-spacer|<svg|<img/);
  // Kein eigener Agent-Lauf mehr in dieser App (M-05): kein ctox.coding.turn aus der UI.
  assert.doesNotMatch(js, /api\.command\(/);
  assert.match(js, /api\.shell\.openSource\(run\.app_id\)/);
});

test('arrangeRuns: laufende zuerst, dann neueste; Filter nach App und Suche (Titel, ID, Auftrag)', () => {
  const turns = [
    { id: 'a', app_id: 'notes', state: 'done', created_at_ms: 30, messages: [{ role: 'user', text: 'Titel kürzen' }] },
    { id: 'b', app_id: 'invoices', state: 'running', created_at_ms: 10, messages: [{ role: 'user', text: 'Summe' }] },
    { id: 'c', app_id: 'notes', state: 'conflict', created_at_ms: 40, messages: [{ role: 'user', text: 'Datum' }] },
    { id: 'd', app_id: 'notes', state: 'done', created_at_ms: 50, _deleted: true },
    { id: '', app_id: 'notes' },
  ];
  const titles = new Map([['notes', 'Notizen'], ['invoices', 'Rechnungen']]);
  assert.deepEqual(arrangeRuns(turns).map((t) => t.id), ['b', 'c', 'a']);
  assert.deepEqual(arrangeRuns(turns, { app: 'notes' }).map((t) => t.id), ['c', 'a']);
  assert.deepEqual(arrangeRuns(turns, { search: 'rechn', titles }).map((t) => t.id), ['b']);
  assert.deepEqual(arrangeRuns(turns, { search: 'KÜRZEN' }).map((t) => t.id), ['a']);
  assert.deepEqual(arrangeRuns(null), []);
});
