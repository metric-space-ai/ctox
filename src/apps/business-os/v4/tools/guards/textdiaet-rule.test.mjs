// node --test src/apps/business-os/v4/tools/guards/textdiaet-rule.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import { scanSource, isInfoboxClass, looksLikeProse, ratchet, isAppSource } from './textdiaet-rule.mjs';

const LONG = 'Hier können Sie Ihre Einstellungen bearbeiten und anschließend alle Änderungen dauerhaft speichern lassen.';
const rules = (rel, src) => scanSource(rel, src).map((h) => h.rule);

test('Infobox-Klassen: Suffixe zaehlen, Zustandsmodifikatoren und Attributnamen nicht', () => {
  for (const c of ['mail-notice', 'nn-lock-hint', 'ctox-callout', 'x-info-box', 'settings-warning', 'top-banner', 'notice']) assert.ok(isInfoboxClass(c), c);
  for (const c of ['is-warning', 'has-alert', 'data-hint', 'aria-hint', 'notices', 'hint-text', 'callout']) assert.ok(!isInfoboxClass(c), c);
});

test('Infobox in HTML-Attribut, JS-Template und Klassenliste; nicht in CSS-Abfragen', () => {
  assert.deepEqual(rules('modules/a/index.html', '<div class="box app-notice">x</div>'), ['infobox']);
  assert.deepEqual(rules('modules/a/index.js', 'el.innerHTML = `<p class="app-hint">${x}</p>`;'), ['infobox']);
  assert.deepEqual(rules('modules/a/index.js', "node.className = 'app-alert';"), ['infobox']);
  assert.deepEqual(rules('modules/a/index.js', "if (n.classList.contains('app-alert')) {}"), []);
  assert.deepEqual(rules('modules/a/index.js', "n.setAttribute('data-app-hint', '');"), []);
  assert.deepEqual(rules('modules/a/index.js', "b.className = 'ctox-badge is-warning';"), []);
});

test('Hilfesatz > 80 Zeichen: Text zaehlt, Zone/Fehler/Tooltip/Attribut/Freigabe nicht', () => {
  assert.deepEqual(rules('modules/a/index.html', `<p>${LONG}</p>`), ['help-text']);
  assert.deepEqual(rules('modules/a/index.js', `p.textContent = '${LONG}';`), ['help-text']);
  assert.deepEqual(rules('modules/a/index.js', `api.status.report({ text: '${LONG}' });`), []);
  assert.deepEqual(rules('modules/a/index.js', `throw new Error('${LONG}');`), []);
  assert.deepEqual(rules('modules/a/index.js', `b.title = '${LONG}';`), []);
  assert.deepEqual(rules('modules/a/index.html', `<button title="${LONG}">x</button>`), []);
  assert.deepEqual(rules('modules/a/index.js', `// v4-allow: text\nconst seed = '${LONG}';`), []);
  assert.deepEqual(rules('modules/a/index.js', 'x.innerHTML = `<input value="${v}" placeholder="kurz" aria-describedby="a-very-long-identifier-error" data-x="yy" />`;'), []);
});

test('Locales: Werte zaehlen, Fehler-/Tooltip-/Beschreibungsschluessel nicht', () => {
  const src = JSON.stringify({ description: LONG, intro: LONG, saveError: LONG, field: { tooltip: LONG } }, null, 2);
  const hits = scanSource('modules/a/locales/de.json', src);
  assert.deepEqual(hits.map((h) => h.key), ['intro']);
});

test('Prosa-Erkennung ignoriert Code, URLs und kurze Texte', () => {
  assert.ok(looksLikeProse(LONG));
  assert.ok(!looksLikeProse('Kurz und knapp.'));
  assert.ok(!looksLikeProse('https://example.com/a/very/long/path/that/goes/on/and/on/for/more/than/eighty/characters'));
  assert.ok(!looksLikeProse('display: flex; gap: 12px; padding: 4px 8px; border-radius: 6px; color: var(--x); font-size: 13px;'));
});

test('Nur App-Quellen ohne Tests, Handler und Vendor', () => {
  assert.ok(isAppSource('modules/mail/index.js'));
  assert.ok(isAppSource('modules/mail/locales/de.json'));
  assert.ok(!isAppSource('modules/mail/tests/v4-api.test.mjs'));
  assert.ok(!isAppSource('modules/mail/handlers/send.js'));
  assert.ok(!isAppSource('modules/mail/module.json'));
  assert.ok(!isAppSource('shared/x.js'));
});

test('Ratchet: ueber Baseline meldet, darunter ist Senkung', () => {
  const res = { counts: { 'modules/a/index.js': { infobox: 2, 'help-text': 1 } } };
  const r = ratchet(res, { 'modules/a/index.js': { infobox: 1, 'help-text': 3 } });
  assert.deepEqual(r.over.map((o) => o.rule), ['infobox']);
  assert.deepEqual(r.lowered.map((o) => o.rule), ['help-text']);
  assert.equal(ratchet({ counts: { 'modules/new/index.js': { infobox: 1 } } }, {}).over.length, 1);
});

test('Tooltip ueber verschachtelte Aufrufe im Template-Attribut zaehlt nicht', () => {
  const src = "x.innerHTML = `<select title=\"${escapeHtml(state.t('hintKey', 'CTOX nutzt den Skill als Wissens-Hub und liest die verknüpften Originalquellen für Nachweise.'))}\"></select>`;";
  assert.deepEqual(rules('modules/a/index.js', src), []);
});
