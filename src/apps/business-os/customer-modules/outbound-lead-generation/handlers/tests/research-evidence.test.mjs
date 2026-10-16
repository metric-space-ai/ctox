// Teilschritt 1 research_writeback: Belegregeln, gespiegelt an den Rust-Tests
// person_research_command.rs:3729 two_pages_of_one_provider_are_one_source und
// person_research_gap_closure.rs:7083 an_email_quote_must_name_the_exact_address.
import test from 'node:test';
import assert from 'node:assert/strict';
import {
  requiredIndependentSources, fieldAcceptsSingleAuthoritativeSource, independentResearchEvidenceCount,
  researchEvidenceProvider, researchEvidenceSourceKey, quoteBacksValue, numbersIn, matchingResearchEvidenceCount,
  deduplicateResearchEvidence, crmAwareProviderCount,
} from '../_lib/research-evidence.js';

test('OWNER-REGEL: required sources exactly as in Rust (1 for every field)', () => {
  for (const field of ['firma_name', 'umsatz', 'mitarbeiter', 'person_email', 'firma_domain', 'custom_x']) {
    assert.equal(requiredIndependentSources(field), 1);
  }
  assert.equal(fieldAcceptsSingleAuthoritativeSource(' person_xing '), true);
  assert.equal(fieldAcceptsSingleAuthoritativeSource('umsatz'), false);
});

test('two pages of one provider are one source (Rust :3729)', () => {
  const ev = (url) => ({ field_key: 'firma_ort', source_id: 'northdata.de', source_url: url });
  assert.equal(independentResearchEvidenceCount([ev('https://www.northdata.de/Akemi'), ev('https://www.northdata.com/AKEMI/HRB%201834')], 'firma_ort'), 1);
  assert.equal(independentResearchEvidenceCount([ev('https://www.northdata.de/Akemi'), ev('https://akemi.de/impressum')], 'firma_ort'), 2);
  assert.equal(independentResearchEvidenceCount([ev('https://de.linkedin.com/company/akemi'), ev('https://linkedin.com/company/akemi')], 'firma_ort'), 1);
  assert.equal(independentResearchEvidenceCount([ev('northdata.de/Akemi'), ev('https://www.northdata.de/Akemi/2')], 'firma_ort'), 1);
});

test('provider, CRM scheme and source_id fallback', () => {
  assert.equal(researchEvidenceProvider('www.www.Shop.Example.co.uk'), 'example');
  assert.equal(researchEvidenceProvider('co.uk'), 'co.uk');
  assert.equal(researchEvidenceProvider('localhost'), 'localhost');
  assert.equal(researchEvidenceSourceKey({ source_url: 'Sellify://company/42' }), 'sellify');
  assert.equal(researchEvidenceSourceKey({ url: 'mailto:a@b.de', source_id: 'Bundesanzeiger.de' }), 'bundesanzeiger');
  assert.equal(researchEvidenceSourceKey({ source_id: ' X ' }), 'x');
  assert.equal(researchEvidenceSourceKey({}), null);
  // Sellify alone proves nothing, with a second provider it counts.
  const crm = { field_key: 'umsatz', source_url: 'sellify://company/1' };
  assert.equal(independentResearchEvidenceCount([crm], 'umsatz'), 0);
  assert.equal(independentResearchEvidenceCount([crm, { field: 'umsatz', url: 'https://northdata.de/x' }], 'umsatz'), 2);
  assert.equal(crmAwareProviderCount(0, []), 0);
});

test('an e-mail quote must name the exact address (Rust :7083)', () => {
  const bnt = 'info(at)bnt-chemicals.de; firmeneigenes Adressmuster vorname.nachname@bnt-chemicals.de: Norman Quandt, Birgit Hessler, Sina Helfer, Grit Hartmann';
  assert.equal(quoteBacksValue('person_email', 'robert.suesse@bnt-chemicals.de', bnt), false);
  assert.equal(quoteBacksValue('person_email', 'norman.quandt@bnt-chemicals.de', 'Norman Quandt, Vertrieb, E-Mail: norman.quandt(at)bnt-chemicals.de'), true);
  assert.equal(quoteBacksValue('person_email', 'Grit.Hartmann@bnt-chemicals.de', 'grit.hartmann [at] bnt-chemicals [dot] de'), true);
  assert.equal(quoteBacksValue('person_vorname', 'Robert', bnt), false);
  assert.equal(quoteBacksValue('person_titel', 'Herr', bnt), true);
  assert.equal(quoteBacksValue('person_titel', 'Frau', bnt), true);
  assert.equal(quoteBacksValue('person_titel', 'Prof.', bnt), false);
  assert.equal(quoteBacksValue('person_vorname', 'Norman', bnt), true);
  assert.equal(quoteBacksValue('person_nachname', 'Lovelace', 'Ada Lovelacee'), false);
});

test('quantity quotes: notation, scale, ranges do not count', () => {
  assert.deepEqual(numbersIn('1.500 und 50,87 sowie 50.870.000 / 1,500').map((n) => n[0]), [1500, 50.87, 50870000, 1500]);
  assert.equal(quoteBacksValue('mitarbeiter', '1500', 'rund 1.500 Beschäftigte'), true);
  assert.equal(quoteBacksValue('umsatz', '50870000', 'Umsatz 50,87 Mio. EUR'), true);
  assert.equal(quoteBacksValue('mitarbeiter', '50', '11-100 Mitarbeiter'), false);
  assert.equal(quoteBacksValue('mitarbeiter', '100', '11 bis 100 Mitarbeiter'), false);
  assert.equal(quoteBacksValue('mitarbeiter', '120', 'keine Angabe'), false);
  assert.equal(quoteBacksValue('mitarbeiter', 'viele', 'keine Angabe'), true);
});

test('matching evidence binds value, person and quote; dedupe key', () => {
  const ev = [
    { field_key: 'person_email', value: 'A@x.de', person_key: 'p1', quote: 'Mail: a@x.de', source_url: 'https://x.de/team' },
    { field_key: 'person_email', value: 'a@x.de', person_key: 'p2', quote: 'Mail: a@x.de', source_url: 'https://y.de/' },
    { field_key: 'person_email', value: 'a@x.de', person_key: 'p1', quote: 'kein Treffer', source_url: 'https://z.de/' },
  ];
  assert.equal(matchingResearchEvidenceCount(ev, 'person_email', { value: ' a@X.de ', person_key: 'p1' }, false), 1);
  assert.equal(matchingResearchEvidenceCount(ev, 'person_email', { value: 'a@x.de' }, false), 0);
  assert.equal(matchingResearchEvidenceCount([{ field_key: 'mitarbeiter', value: 12, quote: '12 MA', url: 'https://a.de' }], 'mitarbeiter', { value: 12 }, false), 1);
  assert.equal(matchingResearchEvidenceCount(ev, 'firma_name', {}, true), 0);
  const deduped = deduplicateResearchEvidence([
    { field: 'umsatz', url: 'https://A.de', value: 'X', quote: 'q' },
    { field_key: 'umsatz', source_url: 'https://a.de', value: 'x', quote: 'Q' },
    {},
    { field_key: 'umsatz', value: null },
  ]);
  assert.equal(deduped.length, 2);
});
