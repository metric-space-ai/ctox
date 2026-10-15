import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { hebePruefdienstRegelAn } from '../../customer-modules/outbound-lead-generation/policy-migration.mjs';

const altText = 'Die gebildete Adresse über den Adapter experte-de prüfen, bei unklarem Ergebnis einmal über mailtester-com. Nur ein Ergebnis „gültig“ belegt die Adresse. Jede Adresse höchstens einmal je Prüfdienst; ist ein Dienst blockiert, das Feld als prüfbedürftig mit diesem Grund melden.';

test('a stored workflow falls back to mailtester when experte-de is blocked', () => {
  const neu = hebePruefdienstRegelAn(altText);
  assert.match(neu, /wenn experte-de blockiert oder nicht erreichbar ist, einmal über mailtester-com\./);
  assert.match(neu, /ist ein Dienst blockiert, den anderen nehmen, und erst wenn beide blockiert sind/);
  assert.doesNotMatch(neu, /ist ein Dienst blockiert, das Feld als prüfbedürftig/);
  assert.equal(hebePruefdienstRegelAn(neu), neu);
});

test('an own wording of the owner stays untouched', () => {
  const eigen = 'E-Mails nur über mailtester-com prüfen.';
  assert.equal(hebePruefdienstRegelAn(eigen), eigen);
});

test('the default workflow already carries the fallback', () => {
  const source = readFileSync(new URL('../../customer-modules/outbound-lead-generation/index.js', import.meta.url), 'utf8');
  assert.ok(source.includes('wenn experte-de blockiert oder nicht erreichbar ist, einmal über mailtester-com.'));
  assert.ok(!source.includes('bei unklarem Ergebnis einmal über mailtester-com. Nur'));
});
