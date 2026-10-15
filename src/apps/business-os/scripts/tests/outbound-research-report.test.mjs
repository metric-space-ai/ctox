import test from 'node:test';
import assert from 'node:assert/strict';
import { berichtAusBefehl, berichtHtml } from '../../customer-modules/outbound-lead-generation/research-report.mjs';

test('the final report comes from the research command result', () => {
  assert.equal(berichtAusBefehl({ result: { outbound_text: '**Ananda GmbH — Recherche abgeschlossen**' } }), '**Ananda GmbH — Recherche abgeschlossen**');
  assert.equal(berichtAusBefehl({ result: { user_message: 'Kontaktpass nachgeholt' } }), 'Kontaktpass nachgeholt');
  assert.equal(berichtAusBefehl({ result: {} }), '');
  assert.equal(berichtAusBefehl(null), '');
});

test('the report renders paragraphs, lists and bold text without raw HTML', () => {
  const html = berichtHtml('**Neu belegt**\n- Mitarbeiter: 1.155\n- <script>x</script>\n\nUmsatz offen');
  assert.equal(html, '<p><strong>Neu belegt</strong></p><ul><li>Mitarbeiter: 1.155</li><li>&lt;script&gt;x&lt;/script&gt;</li></ul><p>Umsatz offen</p>');
});
