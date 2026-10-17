import assert from 'node:assert/strict';
import test from 'node:test';
import handle from '../kundenpipeline.mail.send.js';
import { cmd, rejects } from './_ctx.mjs';
import { NOW, setup, vorgang } from './_fixtures.mjs';

const mailCaps = (sent) => ({
  'mail.server.config.get': async () => ({ hostname: 'mail.acme-ctox.example' }),
  'mail.send': async (req) => {
    sent.push(req);
    return { message_id: '<m1@mail.acme-ctox.example>', status: 'queued' };
  },
});

test('Bestaetigung: Antwort-Vorschlag, Re:-Betreff, decisions@hostname, Status inArbeit', async () => {
  const sent = [];
  const t = await setup({
    seed: { kundenpipeline_vorgaenge: [vorgang({ triage_json: { antwort_vorschlag: 'Wir kümmern uns.' } })] },
    caps: mailCaps(sent),
  });
  const r = await handle(cmd('kundenpipeline.mail.send', { vorgang_id: 'kpl-v-1' }), t.ctx);
  assert.deepEqual(r, { ok: true, queued: true, to: 'kunde@acme.example', subject: 'Re: Export anpassen' });
  assert.equal(sent.length, 1);
  assert.equal(sent[0].account_id, 'decisions@mail.acme-ctox.example');
  assert.deepEqual(Object.keys(sent[0]).sort(), ['account_id', 'idempotency_key', 'subject', 'text', 'to']);
  assert.deepEqual(sent[0].to, ['kunde@acme.example']);
  assert.equal(sent[0].text, 'Wir kümmern uns.');
  const v = t.doc('kundenpipeline_vorgaenge', 'kpl-v-1');
  assert.equal(v.status, 'inArbeit');
  assert.deepEqual(v.mails_json[0], {
    richtung: 'ausgehend', art: 'bestaetigung', an: 'kunde@acme.example', betreff: 'Re: Export anpassen',
    body: 'Wir kümmern uns.', gesendet_ms: NOW, message_id: '<m1@mail.acme-ctox.example>',
  });
  assert.equal(v.audit_json.at(-1).aktion, 'mail:bestaetigung:gesendet');
});

test('Ergebnis: Run-Zusammenfassung, Betreff mit Re: bleibt, Status abgeschlossen, Hostname-Rueckfall', async () => {
  const sent = [];
  const t = await setup({
    seed: {
      kundenpipeline_vorgaenge: [
        vorgang({ quelle_json: { absender: 'k@x.example', betreff: 'RE: Frage' }, run_json: { zusammenfassung: 'Erledigt.' } }),
      ],
    },
    caps: { 'mail.send': async (req) => (sent.push(req), { message_id: 'm2' }) },
  });
  const r = await handle(cmd('kundenpipeline.mail.send', { vorgang_id: 'kpl-v-1', art: 'ergebnis' }), t.ctx);
  assert.equal(r.subject, 'RE: Frage');
  assert.equal(sent[0].account_id, 'decisions@ctox.local');
  assert.equal(sent[0].text, 'Erledigt.');
  assert.equal(t.doc('kundenpipeline_vorgaenge', 'kpl-v-1').status, 'abgeschlossen');
});

test('Fehlerfaelle: kein Absender, kein Text, Versand nicht verfuegbar schreibt nichts', async () => {
  const t = await setup({
    seed: { kundenpipeline_vorgaenge: [vorgang({ id: 'a', quelle_json: { betreff: 'x' } }), vorgang({ id: 'b' })] },
  });
  await rejects(handle(cmd('kundenpipeline.mail.send', { vorgang_id: 'a', body: 'x' }), t.ctx), 'invalid', /no sender address/);
  await rejects(handle(cmd('kundenpipeline.mail.send', { vorgang_id: 'b' }), t.ctx), 'invalid', /no mail body/);
  await rejects(handle(cmd('kundenpipeline.mail.send', { vorgang_id: 'b', body: 'x' }), t.ctx), 'unavailable');
  assert.equal(t.doc('kundenpipeline_vorgaenge', 'b').status, 'zugeordnet');
});
