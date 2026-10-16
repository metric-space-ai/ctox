// #772 (main 1c269d63a, Skill outbound-lead-generation-research §5): Personen brauchen zwei Quellen,
// Geschlecht und Titel sind Pflichtfelder. Je Regel ein Test gegen die v4-Handler-Logik (_lib/research.js).
import test from 'node:test';
import assert from 'node:assert/strict';
import { contactsFromResearch, contactFieldKeys } from '../_lib/research.js';

const cand = (value, url, extra = {}) => ({ value, source_url: url, ...extra });

test('#772 Regel 1: Name + Funktion von zwei Anbietern → verified; ein Profil allein → single source', () => {
  const research = { fields: {
    person_vorname: { candidates: [cand('Anna', 'https://www.linkedin.com/in/anna-meier'), cand('Anna', 'https://www.xing.com/profile/Anna_Meier'), cand('Bob', 'https://www.linkedin.com/in/bob-k')] },
    person_nachname: { candidates: [cand('Meier', 'https://www.linkedin.com/in/anna-meier'), cand('Meier', 'https://www.xing.com/profile/Anna_Meier'), cand('Kraus', 'https://www.linkedin.com/in/bob-k')] },
    person_funktion: { candidates: [cand('CEO', 'https://www.linkedin.com/in/anna-meier'), cand('CEO', 'https://www.xing.com/profile/Anna_Meier'), cand('CTO', 'https://www.linkedin.com/in/bob-k')] },
  } };
  const contacts = contactsFromResearch('pipe', 'co', 'ACME', research);
  const anna = contacts.filter((c) => c.contact_name === 'Anna Meier');
  assert.ok(anna.length >= 1);
  for (const c of anna) {
    assert.equal(c.person_source_status, 'verified');
    assert.deepEqual(c.person_sources, ['linkedin.com', 'xing.com']);
  }
  const bob = contacts.find((c) => c.contact_name === 'Bob Kraus');
  assert.equal(bob.person_source_status, 'single_source');
  assert.equal(bob.person_source_reason, 'single source: linkedin.com');
});

test('#772 Regel 1: zwei Hosts desselben Anbieters zählen als eine Quelle (Anbieter, nicht Hosts)', () => {
  const research = { fields: {
    person_vorname: { candidates: [cand('Eva', 'https://de.linkedin.com/in/eva', { provider: 'linkedin' }), cand('Eva', 'https://www.linkedin.com/in/eva-x', { provider: 'linkedin' })] },
    person_nachname: { candidates: [cand('Roth', 'https://de.linkedin.com/in/eva', { provider: 'linkedin' }), cand('Roth', 'https://www.linkedin.com/in/eva-x', { provider: 'linkedin' })] },
  } };
  const [eva] = contactsFromResearch('pipe', 'co', 'ACME', research);
  assert.equal(eva.person_source_status, 'single_source');
  assert.deepEqual(eva.person_sources, ['linkedin']);
});

test('#772 Regel 2: jede zurückgeschriebene Person trägt person_geschlecht und person_titel; Personenfelder fordern sie an', () => {
  const research = { fields: {
    person_vorname: { candidates: [cand('Jan', 'https://acme.example/team')] },
    person_nachname: { candidates: [cand('Berg', 'https://acme.example/team')] },
    person_titel: { candidates: [cand('Dr.', 'https://acme.example/team')] },
  } };
  const [jan] = contactsFromResearch('pipe', 'co', 'ACME', research);
  assert.equal(jan.person_titel, 'Dr.');
  assert.equal(jan.title, 'Dr.');
  assert.equal(jan.person_geschlecht, '', 'fehlendes Geschlecht ist leer, nie abwesend');
  assert.ok('gender' in jan);
  assert.ok(contactFieldKeys(['contact.people']).includes('person_geschlecht'));
  assert.ok(contactFieldKeys(['contact.people']).includes('person_titel'));
  assert.ok(contactFieldKeys([]).includes('person_titel'));
  assert.ok(!contactFieldKeys(['contact.email']).includes('person_titel'), 'nur wenn Personen (Name) recherchiert werden');
});
