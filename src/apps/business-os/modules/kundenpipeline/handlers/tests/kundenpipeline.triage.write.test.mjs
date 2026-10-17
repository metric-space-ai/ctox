import assert from 'node:assert/strict';
import test from 'node:test';
import handle from '../kundenpipeline.triage.write.js';
import { deterministicId } from '../_lib/hub.js';
import { cmd, rejects } from './_ctx.mjs';
import { NOW, setup, vorgang } from './_fixtures.mjs';

test('schreibt triage_json, Status triagiert, Audit und eine Triage-Karte', async () => {
  const t = await setup({ seed: { kundenpipeline_vorgaenge: [vorgang()] } });
  const triage = {
    einordnung: 'arbeit',
    aufwand: 'S',
    antwort_vorschlag: 'Danke, wir passen den Export an.',
    aufgabe: { agent: 'Sol · Completion', beschreibung: 'CSV-Export-Spalten umsortieren.' },
  };
  const r = await handle(cmd('kundenpipeline.triage.write', { vorgang_id: 'kpl-v-1', triage }), t.ctx);
  const expectedId = deterministicId('kpl-e-triage', 'kpl-v-1');
  assert.deepEqual(r, { ok: true, vorgang_id: 'kpl-v-1', decision_id: expectedId });
  const v = t.doc('kundenpipeline_vorgaenge', 'kpl-v-1');
  assert.equal(v.status, 'triagiert');
  assert.deepEqual(v.triage_json, triage);
  assert.deepEqual(v.audit_json.at(-1), { zeit_ms: NOW, aktion: 'triage:vorschlag', akteur: 'triage-agent', kanal: 'system' });
  const d = t.doc('kundenpipeline_entscheidungen', expectedId);
  assert.equal(d.typ, 'triage');
  assert.equal(d.status, 'offen');
  assert.equal(d.titel, 'ACME GmbH');
  assert.deepEqual(d.zeilen_json, [
    '▸ MAIL', 'Bitte die CSV-Spalten umsortieren.', '',
    '▸ ANTWORT-VORSCHLAG', 'Danke, wir passen den Export an.', '',
    '▸ AUFGABE → Sol · Completion', 'CSV-Export-Spalten umsortieren.',
  ]);
  assert.equal(d.owner_user_id, 'user_owner');
});

test('Titel faellt auf Absender zurueck; Pflichtfelder; unbekannter Vorgang', async () => {
  const t = await setup({ seed: { kundenpipeline_vorgaenge: [vorgang({ kunde_name: '' })] } });
  const r = await handle(cmd('kundenpipeline.triage.write', { vorgang_id: 'kpl-v-1', triage: {} }), t.ctx);
  assert.equal(t.doc('kundenpipeline_entscheidungen', r.decision_id).titel, 'kunde@acme.example');
  await rejects(handle(cmd('kundenpipeline.triage.write', { triage: {} }), t.ctx), 'invalid', /vorgang_id/);
  await rejects(handle(cmd('kundenpipeline.triage.write', { vorgang_id: 'kpl-v-1', triage: [] }), t.ctx), 'invalid', /triage object/);
  await rejects(handle(cmd('kundenpipeline.triage.write', { vorgang_id: 'nope', triage: {} }), t.ctx), 'not_found');
});
