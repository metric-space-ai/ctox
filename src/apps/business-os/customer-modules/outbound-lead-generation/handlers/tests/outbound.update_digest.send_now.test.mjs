import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import handle, { explicitSinceMs } from '../outbound.update_digest.send_now.js';
import { createHash } from 'node:crypto';
import { DIGEST_POLICY_SUMMARY, POLICY_REPORT_REVIEWS, recordAndSendPolicyReportEmail } from '../_lib/policy-report.js';
import {
  parseConfig, parseHhmm, parseRecipients, isPlausibleEmail, shortReason, localLabel, foldRegistryRuns, sourceTargetKey,
  registryProblem, sourceProblem, buildReport, markAsCorrection,
} from '../_lib/update-digest.js';

// 2026-10-15 07:00 UTC = Do 15.10. 09:00 Europe/Berlin
const NOW = Date.UTC(2026, 9, 15, 7, 0);
const HOUR = 3600_000;
const leads = {
  l1: { id: 'l1', name: 'ACME GmbH', campaign: 'Sellify: Nord', research_status: 'completed', field_status: { a: { status: 'verified' }, b: { status: 'verified' }, c: { status: 'action_required' } }, contacts: [{}, {}], payload: { research_finished_at_ms: NOW - HOUR, sellify_finished_at_ms: NOW - HOUR }, sellify_status: 'completed' },
  l2: { id: 'l2', name: 'Beta AG', campaign: 'Messe', research_status: 'needs_review', field_status: { a: { status: 'action_required' } } },
  l3: { id: 'l3', name: 'Gamma KG', campaign: 'Messe', research_status: 'running', research_updated_at_ms: NOW - 3 * HOUR },
  l4: { id: 'l4', name: 'Delta', campaign: 'Messe', research_status: 'failed', research_updated_at_ms: NOW - HOUR, research_error: 'Quelle antwortet nicht.\nstack' },
  l5: { id: 'l5', name: 'UITEST-x', research_status: 'completed' },
  l6: { id: 'l6', name: 'Gone', is_deleted: true, research_status: 'completed' },
};
const sources = {
  'bundesanzeiger.de': { id: 'bundesanzeiger.de', label: 'Bundesanzeiger', enabled: true },
  'xing.com': { id: 'xing.com', label: 'XING', enabled: true },
  sellify: { id: 'sellify', enabled: true },
};
const policyDoc = { id: 'outbound_update_digest_v1', update_digest: { enabled: true, recipients: 'a@x.de; B@X.DE, bad', weekdays: [1, 2], time: '09:00', timezone: 'Europe/Berlin' } };
const caps = (extra = {}) => ({
  'config.get': async ({ key }) => ({ value: key === 'CTO_EMAIL_ADDRESS' ? ' CTO@Firma.de ' : null }),
  'mail.send': async () => ({ message_id: 'm-1', status: 'accepted' }),
  ...extra,
});
// Versandzustand im eigenen Statusdatensatz (Strang CAP3A; vorher runtime_state – laut CAP-3 #831 kein Core-Primitiv).
const STATUS_ID = 'outbound_update_digest_status_v1';
const data = (state = null) => ({
  outbound_lead_generation_leads: leads,
  outbound_lead_generation_sources: sources,
  outbound_lead_generation_research_policies: {
    outbound_update_digest_v1: policyDoc,
    ...(state ? { [STATUS_ID]: { id: STATUS_ID, status: 'digest_status', ...state } } : {}),
  },
  // Scrape-Registry als App-Collections (Strang OLGSCR; vorher Core-SQLite hinter einer Capability).
  outbound_lead_generation_scrape_targets: { 'bundesanzeiger-de': { id: 'bundesanzeiger-de', target_id: 'tb', target_key: 'bundesanzeiger-de' } },
  outbound_lead_generation_scrape_runs: {
    r1: { id: 'r1', run_id: 'r1', target_id: 'tb', status: 'succeeded', started_at: '2026-10-14T05:00:00Z', result: {} },
    r2: { id: 'r2', run_id: 'r2', target_id: 'tb', status: 'provider_account_inactive', started_at: '2026-10-15T05:00:00Z', result: { detail: 'Customer is not active' } },
  },
});

test('config parsing follows Rust (ref :80-160)', () => {
  assert.equal(parseHhmm(' +7 : 05 '), 425);
  assert.equal(parseHhmm('24:00'), null);
  assert.deepEqual(parseRecipients('a@x.de; B@X.DE, bad,a@x.de'), ['a@x.de', 'b@x.de']);
  assert.equal(isPlausibleEmail('a@.de'), false);
  assert.equal(isPlausibleEmail('a b@x.de'), false);
  const c = parseConfig({ update_digest: { weekdays: [3, 1, 3, 9], time: '7:30', timezone: 'Mars/Base' } });
  assert.deepEqual(c.weekdays, [1, 3]);
  assert.equal(c.timezone, 'Europe/Berlin');
  assert.equal(c.enabled, false);
  assert.equal(parseConfig({ time: 'x' }), null);
});

test('helpers: short reason, labels, registry fold and problems', () => {
  assert.equal(shortReason('Quelle antwortet nicht...|x'), 'Quelle antwortet nicht');
  assert.equal(shortReason('Fehler in runtime/ctox.sqlite3'), 'technischer Fehler, Details in der App');
  assert.equal(shortReason('x'.repeat(120)).length, 111);
  assert.equal(localLabel('Europe/Berlin', NOW, true), 'Do 15.10. 09:00');
  assert.equal(localLabel('Europe/Berlin', NOW, false), '15.10.');
  const runs = foldRegistryRuns({ last: [{ target_key: 'k', status: 'blocked', started_at: 'nope', result_json: '{"failure_mode":"blocked","detail":" x "}' }], ok: [] });
  assert.deepEqual(runs.get('k'), { status: 'blocked', at_ms: 0, detail: 'x · blocked', last_ok_ms: 0 });
  assert.equal(sourceTargetKey({ id: '..Bundes--anzeiger.de' }), 'bundes-anzeiger-de');
  assert.match(registryProblem({ status: 'failed', detail: 'API token missing' }), /^Keine Zugangsdaten hinterlegt/);
  assert.equal(sourceProblem({ auth_status: 'credential_missing' }, null), 'Zugangsdaten fehlen');
  assert.equal(sourceProblem({}, { status: 'test_failed', last_error: 'Adapter-Abgleich lief' }), null);
  assert.equal(explicitSinceMs({ since_ms: NOW - 32 * 24 * HOUR }, NOW), null);
  assert.equal(explicitSinceMs({ since_ms: NOW - HOUR }, NOW), NOW - HOUR);
});

test('report content: subject, sections, blockers, stats (ref :569-1288)', () => {
  const registry = foldRegistryRuns({
    last: [{ target_key: 'bundesanzeiger-de', status: 'provider_account_inactive', started_at: '2026-10-15T05:00:00Z', result_json: '{}' }],
    ok: [],
  });
  const r = buildReport(Object.values(leads).filter((l) => !l.is_deleted), Object.values(sources),
    [{ source_id: 'xing.com', status: 'test_portal_drift', updated_at_ms: NOW - HOUR }], registry, NOW - 24 * HOUR, NOW, 'Europe/Berlin');
  assert.equal(r.subject, 'Outbound-Update Do 15.10.2026: 1 Recherche abgeschlossen, 1 an Sellify, 4 Blocker');
  const lines = r.body.split('\n');
  assert.equal(lines[1], 'Zeitraum: Mi 14.10. 09:00 bis Do 15.10. 09:00');
  assert.ok(lines.includes('- Quelle gestört: Bundesanzeiger – Konto beim Anbieter inaktiv (letzter Abruf Do 15.10. 07:00, noch kein erfolgreicher Abruf)'));
  assert.ok(lines.includes('- Quelle gestört: XING – Seitenaufbau geändert, Abruf muss angepasst werden (App-Test 15.10.)'));
  assert.ok(lines.includes('- Sellify: Nord: 1 Leads, 1 recherchiert (1 freigegeben, 0 warten auf Prüfung), 0 in Arbeit, 0 nicht begonnen'));
  assert.ok(lines.indexOf('- Sellify: Nord: 1 Leads, 1 recherchiert (1 freigegeben, 0 warten auf Prüfung), 0 in Arbeit, 0 nicht begonnen') < lines.findIndex((l) => l.startsWith('- Messe:')));
  assert.ok(lines.includes('- ACME GmbH: 2 von 3 Feldern belegt, 2 Ansprechpartner, 1 Feld zu prüfen'));
  assert.ok(lines.includes('- Recherche fehlgeschlagen: Delta – Quelle antwortet nicht'));
  assert.ok(lines.includes('- Recherche hängt: Gamma KG – kein Fortschritt seit Do 15.10. 06:00'));
  assert.ok(lines.includes('4 Leads gesamt: 2 recherchiert (1 freigegeben, 1 warten auf Prüfung), 1 in Arbeit, 0 nicht begonnen, 1 an Sellify übergeben.'));
  assert.equal(r.stats.blockers, 4);
  assert.equal(r.stats.leads_total, 4);
  assert.match(r.html, /^<!DOCTYPE html>/);
  assert.equal((r.html.match(/border-right:1px solid #d8dee4;/g) || []).length, 3);
  markAsCorrection(r, NOW - 24 * HOUR, 'Europe/Berlin');
  assert.match(r.subject, /^Korrigierte Fassung · /);
  assert.match(r.html, /max-width:640px;[^>]*><tr><td style="padding:12px 20px;background:#fff4e5;/);
});

test('dry run returns the preview and writes nothing', async () => {
  const t = createTestCtx({ now: NOW, data: data(), caps: caps() });
  const r = await handle(cmd('outbound.update_digest.send_now', { dry_run: true }), t.ctx);
  assert.equal(r.dry_run, true);
  assert.equal(r.sender, 'cto@firma.de');
  assert.deepEqual(r.recipients, ['a@x.de', 'b@x.de']);
  assert.equal(r.stats.leads_total, 4);
  assert.equal(t.calls.writes.length, 0);
});

test('send to distribution moves the since marker and publishes status (ref :1552)', async () => {
  const t = createTestCtx({
    now: NOW, data: data({ last_sent_at_ms: NOW - 2 * HOUR, last_sent_slot: '2026-10-14' }), caps: caps(),
  });
  const r = await handle(cmd('outbound.update_digest.send_now', {}), t.ctx);
  assert.equal(r.delivery_status, 'accepted');
  const mail = t.calls.caps.find((c) => c.name === 'mail.send').args[0];
  // Kernform caps/mail.rs:129 – genau diese Schlüssel, Absender als account_id.
  assert.deepEqual(Object.keys(mail).sort(), ['account_id', 'html', 'idempotency_key', 'subject', 'text', 'to']);
  assert.equal(mail.account_id, 'cto@firma.de');
  assert.equal(mail.idempotency_key, `outbound-update-test:${NOW}`);
  assert.deepEqual(mail.to, ['a@x.de', 'b@x.de']);
  assert.ok(mail.text.length > 0 && mail.html.length > 0);
  assert.equal(t.calls.caps.filter((c) => c.name === 'config.get').length, 1);
  // App-Einstellung sender_email geht vor die Instanz-Adresse (ref :1350); config.get wird dann nicht gefragt.
  const own = createTestCtx({ now: NOW, data: data(), caps: caps() });
  await handle(cmd('outbound.update_digest.send_now', { update_digest: { ...policyDoc.update_digest, sender_email: 'Vertrieb@Firma.de' } }), own.ctx);
  assert.equal(own.calls.caps.find((c) => c.name === 'mail.send').args[0].account_id, 'vertrieb@firma.de');
  assert.equal(own.calls.caps.filter((c) => c.name === 'config.get').length, 0);
  const status = t.doc('outbound_lead_generation_research_policies', STATUS_ID);
  // Zustand und Status in einem App-Datensatz; kein runtime_state-Aufruf an den Core.
  assert.equal(status.last_sent_at_ms, NOW);
  assert.equal(status.last_sent_slot, '2026-10-14');
  assert.equal(status.last_attempt_at_ms, NOW);
  assert.equal(t.calls.caps.some((c) => c.name.startsWith('runtime_state.')), false);
  assert.equal(status.last_kind, 'test');
  assert.equal(status.last_test_ok, true);
  // explicit recipients and an earlier window → correction only for the distribution
  const t2 = createTestCtx({ now: NOW, data: data({ last_sent_at_ms: NOW - 2 * HOUR }), caps: caps() });
  const r2 = await handle(cmd('outbound.update_digest.send_now', { recipients: ['z@y.de'], since_ms: NOW - 10 * HOUR }), t2.ctx);
  assert.doesNotMatch(r2.subject, /Korrigierte/);
  const r3 = await handle(cmd('outbound.update_digest.send_now', { since_ms: NOW - 10 * HOUR }), t2.ctx);
  assert.match(r3.subject, /^Korrigierte Fassung/);
});

test('failures: no config, no recipients, mail error recorded', async () => {
  const empty = createTestCtx({ now: NOW, caps: caps() });
  assert.match((await rejects(handle(cmd('outbound.update_digest.send_now', {}), empty.ctx))).message, /noch nicht gespeichert/);
  const t = createTestCtx({ now: NOW, data: data(), caps: caps() });
  assert.match((await rejects(handle(cmd('outbound.update_digest.send_now', { update_digest: { time: '09:00' } }), t.ctx))).message, /Keine gültige Empfängeradresse/);
  const bad = createTestCtx({ now: NOW, data: data(), caps: caps({
    'mail.send': async () => { throw Object.assign(new Error('smtp down'), { code: 'unavailable' }); },
  }) });
  const e = await rejects(handle(cmd('outbound.update_digest.send_now', {}), bad.ctx));
  assert.equal(e.message, 'smtp down');
  const badStatus = bad.doc('outbound_lead_generation_research_policies', STATUS_ID);
  assert.equal(badStatus.last_error, 'smtp down');
  assert.equal(badStatus.last_sent_at_ms, 0);
  assert.equal(badStatus.last_test_ok, false);
  const noSender = createTestCtx({ now: NOW, data: data(), caps: caps({ 'config.get': async () => ({ value: null }) }) });
  assert.match((await rejects(handle(cmd('outbound.update_digest.send_now', {}), noSender.ctx))).message, /kein Absender-Postfach/);
});

// ------------------------------------------------------------------ Strang OLG3: Policy-Report-Buchführung als App-Logik
// ref: mission/channels/outbound_review.rs:2364 record_and_send_policy_report_email, :1332 founder_reply_review_digest,
//      :1795 require_unconsumed_founder_reply_review, :1881 mark_founder_reply_review_sent

const sha = (text) => createHash('sha256').update(text, 'utf8').digest('hex');
const pr = (extra = {}) => ({ sender_email: ' CTO@Firma.de ', to: [' A@x.de', 'kaputt', 'b@x.de'], subject: ' Update ', body: '\nHallo Team\nStand heute.\n',
  body_html: '<p>Hallo Team</p>', report_key: 'outbound-update-test:1', policy_summary: DIGEST_POLICY_SUMMARY, ...extra });

test('policy report: the send_now delivery is booked as a consumed policy approval (ref :1442 → :2364)', async () => {
  const t = createTestCtx({ now: NOW, data: data(), caps: caps() });
  await handle(cmd('outbound.update_digest.send_now', {}), t.ctx);
  const rows = t.all(POLICY_REPORT_REVIEWS);
  assert.equal(rows.length, 1);
  const row = rows[0];
  const mail = t.calls.caps.find((c) => c.name === 'mail.send').args[0];
  assert.equal(row.reviewer, 'policy-report');
  assert.equal(row.review_summary, DIGEST_POLICY_SUMMARY);
  assert.equal(row.inbound_message_key, `policy-report:outbound-update-test:${NOW}`);
  assert.equal(row.account_key, 'email:cto@firma.de');
  // Digest exakt wie Rust: sha256(action_json || 0x00 || sha256(trim(body))), action_json in Einfügereihenfolge.
  const actionJson = JSON.stringify({ thread_key: row.inbound_message_key, subject: mail.subject, to: mail.to, cc: [], attachments: [] });
  assert.equal(row.action_json, actionJson);
  assert.equal(row.body_sha256, sha(mail.text.trim()));
  assert.equal(row.action_digest, sha(`${actionJson}\u0000${sha(mail.text.trim())}`));
  assert.equal(row.id, `policy-report:outbound-update-test:${NOW}:${row.action_digest}`);
  assert.equal(row.sent_at, new Date(NOW).toISOString());
  assert.deepEqual(row.send_result, { message_id: 'm-1', status: 'accepted' });
});

test('INV-PR1 a consumed policy approval never sends the same slot and body twice (ref :1795)', async () => {
  const t = createTestCtx({ now: NOW, caps: caps() });
  const first = await recordAndSendPolicyReportEmail(t.ctx, pr());
  assert.equal(first.status, 'accepted');
  const mail = t.calls.caps.find((c) => c.name === 'mail.send').args[0];
  assert.deepEqual([mail.account_id, mail.to, mail.subject, mail.text], ['cto@firma.de', ['a@x.de', 'b@x.de'], 'Update', 'Hallo Team\nStand heute.']);
  const again = await rejects(recordAndSendPolicyReportEmail(t.ctx, pr()));
  assert.equal(again.code, 'conflict');
  assert.match(again.message, /no matching unconsumed review approval/);
  assert.equal(t.calls.caps.filter((c) => c.name === 'mail.send').length, 1);
  // Anderer Text im selben Slot = neue Freigabe.
  await recordAndSendPolicyReportEmail(t.ctx, pr({ body: 'Korrektur' }));
  assert.equal(t.calls.caps.filter((c) => c.name === 'mail.send').length, 2);
  assert.equal(t.all(POLICY_REPORT_REVIEWS).length, 2);
});

test('INV-PR2 the body gate rejects headers and internal language before any approval or send (ref :659)', async () => {
  const t = createTestCtx({ now: NOW, caps: caps() });
  const header = await rejects(recordAndSendPolicyReportEmail(t.ctx, pr({ body: 'Betreff: Update\nHallo' })));
  assert.equal(header.message, 'founder/owner outbound email failed communication review because addressing or subject headers were placed in the message body: Betreff: Update');
  const leak = await rejects(recordAndSendPolicyReportEmail(t.ctx, pr({ body: 'Siehe runtime/ctox.sqlite3' })));
  assert.equal(leak.message, 'founder/owner outbound email failed communication review due to internal-language leakage: runtime/ctox.sqlite3, sqlite');
  assert.equal(t.all(POLICY_REPORT_REVIEWS).length, 0);
  // HTML-Prüfung nach der Freigabe: Freigabe bleibt unverbraucht, kein Versand.
  const html = await rejects(recordAndSendPolicyReportEmail(t.ctx, pr({ body_html: '<p>queue: 3</p>' })));
  assert.match(html.message, /internal-language leakage: queue:/);
  assert.equal(t.all(POLICY_REPORT_REVIEWS)[0].sent_at, null);
  assert.equal(t.calls.caps.filter((c) => c.name === 'mail.send').length, 0);
});

test('policy report: input checks keep the Rust texts; a failed send leaves the approval unconsumed', async () => {
  const t = createTestCtx({ now: NOW, caps: caps({ 'mail.send': async () => { throw Object.assign(new Error('smtp down'), { code: 'unavailable' }); } }) });
  assert.equal((await rejects(recordAndSendPolicyReportEmail(t.ctx, pr({ sender_email: 'nobody' })))).message, 'policy report needs a sender mailbox address');
  assert.equal((await rejects(recordAndSendPolicyReportEmail(t.ctx, pr({ to: ['kaputt'] })))).message, 'policy report needs at least one recipient');
  assert.equal((await rejects(recordAndSendPolicyReportEmail(t.ctx, pr({ subject: '  ' })))).message, 'policy report needs subject and body');
  assert.equal((await rejects(recordAndSendPolicyReportEmail(t.ctx, pr()))).message, 'smtp down');
  const [row] = t.all(POLICY_REPORT_REVIEWS);
  assert.equal(row.sent_at, null);
  assert.deepEqual(row.send_result, {});
});
