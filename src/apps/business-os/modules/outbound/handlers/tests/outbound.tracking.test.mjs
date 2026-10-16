// Mail-Tracking als App-Logik (Kern §16.35 Nr. 3, Strang TRACK). Gleichheit mit der Rust-Referenz an konkreten
// Fällen (store_outbound_commands.rs:6188-6337, server.rs:340-376/4024-4053, store.rs:16592-16606) und
// Invarianten TRK1–TRK5 mit Mutationsbeleg (tests/mutation-proof.mjs, TRACKING_MUTATIONS).
import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import { engagement, message } from './_fixtures.mjs';
import sendHandle from '../outbound.message.send_approved.js';
import registerHandle from '../outbound.tracking.register.js';
import eventHandle from '../outbound.tracking.event.js';
import { messageRevision } from '../_lib/outbound.js';
import { TRACKING, TRACKING_PIXEL_GIF, prepareTrackedHtml, parseTrackingPath, ingestResponse, pixelResponse } from '../_lib/tracking.js';

const BASE = 'https://ctox.example.test/ingest/outbound/mail-tracking';
// sha256 der 43 GIF-Bytes = ref, den ctx.files.put liefert (Attrappe wie der Kern: inhaltsadressiert).
const PIXEL_REF = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', new Uint8Array(TRACKING_PIXEL_GIF))), (b) => b.toString(16).padStart(2, '0')).join('');
const PIXEL = { status: 200, headers: { 'content-type': 'image/gif', 'cache-control': 'no-store, max-age=0' }, body_ref: { ref: PIXEL_REF } };
const ENDPOINT = { id: 'mail_tracking', path: 'mail-tracking', command_type: 'outbound.tracking.event', ingest_path: '/ingest/outbound/mail-tracking', base_url: BASE, status: 'registered', registered_at_ms: 1, updated_at_ms: 1 };
const HTML = '<html><body><p>Hallo <a href="https://acme.test/angebot?x=1">Angebot</a> und <A HREF = \'HTTP://acme.test/b\'>B</A>, '
  + '<a href="mailto:a@b.test">Mail</a>, <a href="/relativ">rel</a></p></BODY></html>';

function approvedData(overrides = {}, extra = {}) {
  const msg = message({ approval_status: 'approved', send_status: 'approved_not_sent', ...overrides });
  msg.revision_id = messageRevision(msg);
  return {
    outbound_messages: { msg_1: msg },
    outbound_engagements: { eng_1: engagement() },
    outbound_approvals: { ap: { id: 'ap', message_id: 'msg_1', revision_id: msg.revision_id, decision: 'approved' } },
    ...extra,
  };
}

function sendCtx({ html = HTML, payload = {}, endpoint = ENDPOINT, register = null } = {}) {
  const sent = [];
  const caps = { 'mail.send': async (args) => { sent.push(args); return { message_id: 'q-1', status: 'queued' }; } };
  if (register) caps['net.ingest.webhook.register'] = register;
  const t = createTestCtx({
    now: 1_000,
    caps,
    data: approvedData({ body_html: html, payload }, endpoint ? { outbound_tracking_endpoints: { mail_tracking: endpoint } } : {}),
  });
  return { t, sent };
}

const send = (t) => sendHandle(cmd('outbound.message.send_approved', { message_id: 'msg_1' }), t.ctx);
const hitPath = (t, subPath, extra = {}) => eventHandle(cmd('outbound.tracking.event', { event_id: 'event_id' in extra ? extra.event_id : `ev_${subPath}`, method: 'GET', sub_path: subPath, headers: { 'User-Agent': 'agent/1' }, ...extra.payload }, { id: extra.command_id ?? 'cmd_ev' }), t.ctx);

// ---------------------------------------------------------------------------------------------- Versand (Rust-Gleichheit)

test('Versand: Links umgeschrieben wie Rust-Regex (nur absolute http(s) in href, Groß/klein egal), Pixel vor </body>', async () => {
  const { t, sent } = sendCtx();
  await send(t);
  const html = sent[0].html;
  const tokens = t.all(TRACKING.tokens);
  const clicks = tokens.filter((x) => x.event_type === 'clicked');
  const opens = tokens.filter((x) => x.event_type === 'opened');
  assert.equal(clicks.length, 2);
  assert.equal(opens.length, 1);
  assert.deepEqual(clicks.map((c) => c.target_url).sort(), ['HTTP://acme.test/b', 'https://acme.test/angebot?x=1']);
  for (const c of clicks) {
    assert.ok(html.includes(`href="${BASE}/c/${c.token}"`), 'Klick-Link zeigt auf den net.ingest-Pfad (immer doppelte Anführungszeichen)');
    assert.equal(c.message_id, 'msg_1');
    assert.match(c.token, /^[0-9a-f]{32}$/);
  }
  assert.ok(html.includes('href="mailto:a@b.test"'), 'mailto bleibt');
  assert.ok(html.includes('href="/relativ"'), 'relative Links bleiben');
  assert.ok(!html.includes('acme.test/angebot'), 'Originalziel steht nicht mehr in der Mail');
  const pixel = `<img src="${BASE}/o/${opens[0].token}.gif" width="1" height="1" alt="" style="display:block;width:1px;height:1px;border:0" />`;
  assert.ok(html.includes(`${pixel}</BODY>`), 'Pixel steht direkt vor dem letzten </body> (ASCII-Groß/klein egal)');
  const doc = t.doc('outbound_messages', 'msg_1');
  assert.equal(doc.payload.tracking_enabled, true);
  assert.equal(doc.payload.tracking_status, 'armed');
  assert.equal(doc.payload.tracking_open_token, opens[0].token);
  assert.equal(doc.payload.tracking_click_links, 2);
  assert.equal(doc.body_html, HTML, 'die Nachricht selbst behält das ungetrackte HTML');
  assert.equal(t.doc('communication_messages', doc.communication_message_key).body_html, HTML);
  assert.equal(t.calls.caps.filter((c) => c.name === 'net.ingest.webhook.register').length, 0, 'registrierter Endpunkt: kein Kernaufruf');
  assert.deepEqual(Object.keys(sent[0]).sort(), ['account_id', 'html', 'idempotency_key', 'subject', 'text', 'to'], 'mail.send ohne tracking-Feld');
});

test('Versand: ohne </body> wird das Pixel angehängt; ohne Links 0 Klick-Tokens', async () => {
  const t = createTestCtx({ now: 5 });
  const msg = { id: 'm', campaign_id: 'camp', payload: {} };
  const out = await prepareTrackedHtml(t.ctx, cmd('x'), msg, '<p>ohne Links</p>', `${BASE}/`);
  assert.match(out, new RegExp(`^<p>ohne Links</p><img src="${BASE}/o/[0-9a-f]{32}\\.gif"`));
  assert.equal(msg.payload.tracking_click_links, 0);
  const [token] = t.all(TRACKING.tokens);
  assert.equal(token.campaign_id, 'camp');
  assert.equal(token.created_at_ms, 5);
});

test('Versand: Nur-Text-Nachricht – kein Tracking, keine Tokens, kein Stempel (Rust :6116)', async () => {
  const { t, sent } = sendCtx({ html: '' });
  await send(t);
  assert.equal(sent[0].html, '');
  assert.equal(t.all(TRACKING.tokens).length, 0);
  assert.equal(t.doc('outbound_messages', 'msg_1').payload.tracking_enabled, undefined);
});

test('Versand: erster Versand meldet den Pfad bei net.ingest an (einmalig), danach kein Kernaufruf', async () => {
  const calls = [];
  const register = async (args) => { calls.push(args); return { token: 'secret-ingest-token', ingest_url: `${BASE}/` }; };
  const { t, sent } = sendCtx({ endpoint: null, register });
  await send(t);
  // §16.38 Nr. 1/3/4: default_response = Pixel, Altpfade nur über aliases, kein pass_headers (nie Cookie/Authorization).
  assert.deepEqual(calls, [{
    path: 'mail-tracking',
    command: 'outbound.tracking.event',
    default_response: PIXEL,
    aliases: ['/mail/track/o/', '/mail/track/c/'],
  }]);
  assert.equal('pass_headers' in calls[0], false);
  const endpoint = t.doc(TRACKING.endpoints, 'mail_tracking');
  assert.equal(endpoint.base_url, BASE, 'abschließender Schrägstrich entfernt wie Rust trim_end_matches');
  assert.equal(endpoint.ingest_path, '/ingest/outbound/mail-tracking', 'Pfad aus ingest_url abgeleitet');
  assert.equal(endpoint.pixel_ref, PIXEL_REF, 'Pixel einmalig per files.put abgelegt, ref am Endpunkt');
  assert.equal(t.calls.files.length, 1);
  assert.deepEqual(t.calls.files[0].meta, { name: 'outbound-tracking-pixel.gif', mime: 'image/gif' });
  assert.equal(String.fromCharCode(...t.calls.files[0].bytes.slice(0, 6)), 'GIF89a');
  assert.equal(JSON.stringify(endpoint).includes('secret-ingest-token'), false, 'Ingest-Token nicht in synchronisierter Collection');
  assert.ok(sent[0].html.includes(`${BASE}/o/`));
  const again = await registerHandle(cmd('outbound.tracking.register', {}), t.ctx);
  assert.equal(again.idempotent, true);
  assert.equal(calls.length, 1);
  const forced = await registerHandle(cmd('outbound.tracking.register', { force: true }), t.ctx);
  assert.equal(forced.registered, true);
  assert.equal(calls.length, 2);
  assert.equal(t.doc(TRACKING.endpoints, 'mail_tracking').registered_at_ms, endpoint.registered_at_ms);
});

test('Versand: net.ingest nicht verfügbar → ungetrackt senden (Rust: leere tracking_base_url), Versand nicht blockiert', async () => {
  const { t, sent } = sendCtx({ endpoint: null });
  const result = await send(t);
  assert.equal(result.provider_queue_id, 'q-1');
  assert.equal(sent[0].html, HTML);
  assert.equal(t.doc('outbound_messages', 'msg_1').payload.tracking_enabled, false);
  assert.equal(t.all(TRACKING.tokens).length, 0);
});

test('Register: Kern lehnt ab (z. B. rejected{invalid}, Handler fehlt im Manifest) → Fehler, nichts gespeichert; Versand ungetrackt', async () => {
  const reject = async (args) => {
    assert.equal(args.command, 'outbound.tracking.event');
    assert.deepEqual(args.aliases, ['/mail/track/o/', '/mail/track/c/']);
    throw Object.assign(new Error('handler outbound.tracking.event not in manifest'), { code: 'invalid' });
  };
  const t = createTestCtx({ caps: { 'net.ingest.webhook.register': reject } });
  const error = await rejects(registerHandle(cmd('outbound.tracking.register', {}), t.ctx));
  assert.equal(error.code, 'invalid');
  assert.equal(t.doc(TRACKING.endpoints, 'mail_tracking'), null);
  const { t: s, sent } = sendCtx({ endpoint: null, register: reject });
  await send(s);
  assert.equal(sent[0].html, HTML);
  assert.equal(s.doc('outbound_messages', 'msg_1').payload.tracking_enabled, false);
});

test('Register: Basis-URL wie Rust tracking_base_url geprüft (Fehlertexte wörtlich)', async () => {
  const cases = [
    [{ ingest_url: 'https://user:pw@ctox.example.test/i' }, 'tracking_base_url must not contain credentials'],
    [{ ingest_url: '/nur/relativ' }, 'net.ingest returned no absolute ingest URL for mail tracking'],
    [{ ingest_path: 'https://ctox.example.test/i' }, 'net.ingest returned no absolute ingest URL for mail tracking'],
  ];
  for (const [answer, text] of cases) {
    const t = createTestCtx({ caps: { 'net.ingest.webhook.register': async () => answer } });
    const error = await rejects(registerHandle(cmd('outbound.tracking.register', {}), t.ctx));
    assert.equal(error.message, text);
    assert.equal(t.doc(TRACKING.endpoints, 'mail_tracking'), null);
  }
  const t = createTestCtx({ caps: { 'net.ingest.webhook.register': async () => ({ ingest_url: 'https://ctox.example.test/i' }) } });
  const ok = await registerHandle(cmd('outbound.tracking.register', {}), t.ctx);
  assert.equal(ok.endpoint.base_url, 'https://ctox.example.test/i', '§16.38: nur {ingest_url}');
});

// ---------------------------------------------------------------------------------------------- Ereignisse (Rust-Gleichheit)

async function armed() {
  const { t, sent } = sendCtx();
  await send(t);
  const tokens = t.all(TRACKING.tokens);
  return {
    t, sent,
    open: tokens.find((x) => x.event_type === 'opened').token,
    click: tokens.find((x) => x.target_url === 'https://acme.test/angebot?x=1').token,
  };
}

test('Öffnung: 200 GIF ohne Cache, Zähler/erste/letzte Zeit, Status opened, Ereignis mit User-Agent', async () => {
  const { t, open } = await armed();
  t.setNow(2_000);
  const first = await hitPath(t, `o/${open}.gif`, { event_id: 'e1' });
  assert.deepEqual(first.response, PIXEL, 'Pixel als body_ref (files), nicht selbst kodiert');
  const stored = t.calls.files.filter((f) => f.ref === PIXEL_REF);
  assert.equal(stored.length, 1, 'Endpunkt ohne pixel_ref: einmal ablegen');
  t.setNow(3_000);
  await hitPath(t, `o/${open}.gif`, { event_id: 'e2' });
  assert.equal(t.calls.files.length, 1, 'zweite Öffnung: ref vom Endpunkt, kein zweites files.put');
  assert.equal(t.doc(TRACKING.endpoints, 'mail_tracking').pixel_ref, PIXEL_REF);
  const doc = t.doc('outbound_messages', 'msg_1');
  assert.equal(doc.payload.open_count, 2);
  assert.equal(doc.payload.first_opened_at_ms, 2_000);
  assert.equal(doc.payload.last_opened_at_ms, 3_000);
  assert.equal(doc.payload.tracking_status, 'opened');
  assert.equal(doc.updated_at_ms, 3_000);
  assert.equal(doc.payload.click_count, undefined);
  assert.deepEqual(t.doc(TRACKING.events, 'e1'), { id: 'e1', token: open, message_id: 'msg_1', campaign_id: 'camp_1', event_type: 'opened', occurred_at_ms: 2_000, user_agent: 'agent/1' });
  assert.equal(first.message_id, 'msg_1');
  assert.equal(first.occurred_at_ms, 2_000);
});

test('Klick: 302 auf das gespeicherte Ziel, click_count, Status clicked; vom Kern per alias zugestellter Altpfad gleichwertig', async () => {
  const { t, click } = await armed();
  t.setNow(4_000);
  const hit = await hitPath(t, `c/${click}`, { event_id: 'k1' });
  assert.deepEqual(hit.response, { status: 302, headers: { location: 'https://acme.test/angebot?x=1', 'cache-control': 'no-store, max-age=0' } });
  assert.equal(hit.target_url, 'https://acme.test/angebot?x=1');
  // §16.38 Nr. 4: alias '/mail/track/c/' – der Handler liest nur die letzten zwei Segmente, ob sub_path oder path.
  const legacy = await eventHandle(cmd('outbound.tracking.event', { event_id: 'k2', method: 'GET', path: `/mail/track/c/${click}`, sub_path: click }), t.ctx);
  assert.equal(legacy.response.status, 302);
  const doc = t.doc('outbound_messages', 'msg_1');
  assert.equal(doc.payload.click_count, 2);
  assert.equal(doc.payload.first_clicked_at_ms, 4_000);
  assert.equal(doc.payload.tracking_status, 'clicked');
});

test('Ereignis: voller Pfad statt sub_path genügt; Nicht-GET → 404 ohne Schreiben', async () => {
  const { t, open } = await armed();
  const hit = await eventHandle(cmd('outbound.tracking.event', { event_id: 'p1', method: 'GET', path: `/ingest/outbound/mail-tracking/o/${open}.gif` }), t.ctx);
  assert.equal(hit.response.status, 200);
  const before = t.calls.writes.length;
  const post = await eventHandle(cmd('outbound.tracking.event', { event_id: 'p2', method: 'POST', sub_path: `o/${open}.gif` }), t.ctx);
  assert.deepEqual(post.response.status, 404);
  assert.equal(t.calls.writes.length, before);
});

test('Ereignis: Nachricht fehlt → Ereignis trotzdem gezählt und Antwort geliefert (Rust if let Ok)', async () => {
  const { t, open } = await armed();
  await t.ctx.remove('outbound_messages', 'msg_1');
  const hit = await hitPath(t, `o/${open}.gif`, { event_id: 'x1' });
  assert.equal(hit.response.status, 200);
  assert.ok(t.doc(TRACKING.events, 'x1'));
});

test('parseTrackingPath: .gif Pflicht für Öffnungen, Query wird ignoriert', () => {
  assert.deepEqual(parseTrackingPath('o/abc.gif?x=1'), { expected: 'opened', token: 'abc' });
  assert.equal(parseTrackingPath('o/abc'), null);
  assert.deepEqual(parseTrackingPath('/c/abc'), { expected: 'clicked', token: 'abc' });
  assert.equal(parseTrackingPath('x/abc'), null);
  assert.deepEqual(parseTrackingPath('/mail/track/o/abc.gif'), { expected: 'opened', token: 'abc' }, 'gleiche Regel, keine Altpfad-Sonderung');
  assert.equal(parseTrackingPath('abc.gif'), null);
});

test('§16.38 Nr. 2: body/body_ref im Eingang ändern nichts und werden nicht gespeichert; Header nur user-agent gelesen', async () => {
  const { t, open } = await armed();
  const plain = await hitPath(t, `o/${open}.gif`, { event_id: 'b1' });
  const withBody = await hitPath(t, `o/${open}.gif`, { event_id: 'b2', payload: { body: 'x'.repeat(1000), body_ref: { ref: 'sha256-abc' }, query: { a: '1' }, received_at_ms: 9 } });
  assert.deepEqual(withBody.response, plain.response);
  const stored = t.doc(TRACKING.events, 'b2');
  assert.deepEqual(Object.keys(stored).sort(), ['campaign_id', 'event_type', 'id', 'message_id', 'occurred_at_ms', 'token', 'user_agent']);
  assert.equal(JSON.stringify(t.all(TRACKING.events)).includes('sha256-abc'), false);
});

// ---------------------------------------------------------------------------------------------- Invarianten (Mutationsbeleg)

test('TRK1 opt-out: payload.tracking_enabled=false sendet ohne Pixel, ohne Link-Umschreibung, ohne Tokens', async () => {
  const { t, sent } = sendCtx({ payload: { tracking_enabled: false } });
  await send(t);
  assert.equal(sent[0].html, HTML);
  assert.equal(t.all(TRACKING.tokens).length, 0);
  assert.equal(t.doc('outbound_messages', 'msg_1').payload.tracking_enabled, false);
});

test('TRK2 ein erneut zugestelltes Ereignis (gleiche event_id) zählt nicht doppelt, Antwort bleibt gleich', async () => {
  const { t, open, click } = await armed();
  const a = await hitPath(t, `o/${open}.gif`, { event_id: 'dup' });
  const b = await hitPath(t, `o/${open}.gif`, { event_id: 'dup' });
  assert.deepEqual(b.response, a.response);
  assert.equal(b.duplicate, true);
  assert.equal(t.doc('outbound_messages', 'msg_1').payload.open_count, 1);
  const c1 = await hitPath(t, `c/${click}`, { event_id: undefined, command_id: 'same-cmd' });
  const c2 = await hitPath(t, `c/${click}`, { event_id: undefined, command_id: 'same-cmd' });
  assert.equal(c2.response.headers.Location, c1.response.headers.Location);
  assert.equal(t.doc('outbound_messages', 'msg_1').payload.click_count, 1, 'ohne event_id dient die Befehls-id als Ereignis-id');
  assert.equal(t.all(TRACKING.events).length, 2);
});

test('TRK3 Token und Pfad müssen zusammenpassen: Öffnungs-Token auf Klickpfad (und umgekehrt) → 404, nichts gezählt', async () => {
  const { t, open, click } = await armed();
  const before = t.calls.writes.length;
  const a = await hitPath(t, `c/${open}`, { event_id: 'm1' });
  const b = await hitPath(t, `o/${click}.gif`, { event_id: 'm2' });
  assert.deepEqual(a.response, { status: 404, headers: { 'content-type': 'text/plain; charset=utf-8' }, body: 'tracking token not found' });
  assert.equal(b.response.status, 404);
  assert.equal(t.all(TRACKING.events).length, 0);
  assert.equal(t.doc('outbound_messages', 'msg_1').payload.open_count, undefined);
  assert.equal(t.calls.writes.length, before);
});

test('TRK4 Weiterleitung nur auf gespeicherte HTTP(S)-Ziele (kein offener Redirect)', async () => {
  const t = createTestCtx({ data: { [TRACKING.tokens]: {
    js: { id: 'js', token: 'js', message_id: 'msg_x', campaign_id: '', event_type: 'clicked', target_url: 'javascript:alert(1)', created_at_ms: 1 },
    empty: { id: 'empty', token: 'empty', message_id: 'msg_x', campaign_id: '', event_type: 'clicked', target_url: '', created_at_ms: 1 },
  } } });
  const bad = await hitPath(t, 'c/js', { event_id: 'r1' });
  assert.equal(bad.response.status, 404, '§16.38 Nr. 3: kein 5xx; Rust antwortete 500');
  assert.equal(bad.response.headers.location, undefined);
  assert.equal(bad.response.body, 'stored mail tracking target is not HTTP(S)');
  const empty = await hitPath(t, 'c/empty', { event_id: 'r2' });
  assert.deepEqual(empty.response.body, 'tracking target not found');
  assert.equal(empty.response.status, 404);
});

test('TRK5 unbekanntes Token → 404 ohne jede Schreiboperation', async () => {
  const { t } = await armed();
  const before = t.calls.writes.length;
  const hit = await hitPath(t, 'o/unbekannt.gif', { event_id: 'u1' });
  assert.equal(hit.response.status, 404);
  assert.equal(hit.response.body, 'tracking token not found');
  assert.equal(t.calls.writes.length, before);
});

test('TRK6 Antwortgrenzen §16.38 Nr. 3: Status-Whitelist, Header-Whitelist, absolute Location, Körper ≤ 64 KiB', () => {
  assert.deepEqual(ingestResponse(pixelResponse('abc')), pixelResponse('abc'));
  assert.equal(ingestResponse({ status: 200, headers: {}, body_ref: { ref: '' } }).status, 404, 'body_ref nur mit ref');
  assert.equal(ingestResponse({ status: 200, headers: {}, body_ref: 'abc' }).status, 404);
  for (const status of [200, 204, 301, 308, 400, 410, 429]) assert.equal(ingestResponse({ status, headers: { location: 'https://a.test/' } }).status, status);
  for (const status of [100, 300, 309, 411, 500, 503, undefined]) assert.equal(ingestResponse({ status, headers: {} }).status, 404, String(status));
  assert.deepEqual(ingestResponse({ status: 200, headers: { 'Set-Cookie': 'a=1', 'Referrer-Policy': 'no-referrer', 'Content-Type': 'text/plain', location: 'https://a.test/' }, body: 'ok' }),
    { status: 200, headers: { 'content-type': 'text/plain' }, body: 'ok' });
  assert.equal(ingestResponse({ status: 302, headers: { location: '/relativ' } }).status, 404);
  assert.equal(ingestResponse({ status: 302, headers: { location: 'javascript:alert(1)' } }).status, 404);
  assert.equal(ingestResponse({ status: 200, headers: {}, body: 'x'.repeat(64 * 1024) }).status, 200);
  assert.equal(ingestResponse({ status: 200, headers: {}, body: 'x'.repeat(64 * 1024 + 1) }).status, 404);
});
