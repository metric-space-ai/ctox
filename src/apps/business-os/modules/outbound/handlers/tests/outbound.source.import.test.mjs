// outbound.source.import – Firmen-Import über Browser-SCHRITTE (Strang OLG725, §16.48 Nr. 2) statt Skript-Quelltext.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import handle, { germanyOnly } from '../outbound.source.import.js';

// Attrappe nach Vertrag (CAP-3d browser.rs automation_run + §16.48 Nr. 2): deny_unknown_fields, ≤ 32 Schritte, alle
// vorab geprüft, Abbruch beim ersten Fehler, Ergebnis als files-Datensatz der App {session_id, steps:[{op, ok, result|code}]}.
// `built:false` = Kern ohne extract/click/wait (heute): unbekannter Schritt → invalid('$.steps').
const BASE_OPS = ['navigate', 'reload', 'back', 'forward', 'tab_open', 'tab_activate', 'tab_close', 'dialog_respond', 'permission_respond', 'clipboard_copy', 'clipboard_paste', 'clipboard_clear', 'credential_fill', 'nav_state'];
const err = (code, message) => Object.assign(new Error(message), { code });
function browserKernel({ pages = {}, built = true, unavailable = false } = {}) {
  const files = new Map();
  const calls = [];
  const sessions = new Map();
  const validStep = (step) => {
    const keys = Object.keys(step);
    if (keys.some((k) => !['op', 'tab_id', 'params'].includes(k))) return false;
    if (BASE_OPS.includes(step.op)) return step.op !== 'navigate' || /^https?:\/\//.test(step.params?.url ?? '');
    if (!built) return false;
    if (step.op === 'click') return typeof step.params?.selector === 'string' && Object.keys(step.params).length === 1;
    if (step.op === 'wait') return (typeof step.params?.selector === 'string') !== (Number.isInteger(step.params?.ms) && step.params.ms <= 30_000);
    if (step.op === 'extract') {
      const p = step.params ?? {};
      return typeof p.selector === 'string' && Number.isInteger(p.limit) && Object.values(p.fields ?? {}).every((f) => typeof f.selector === 'string' && ((f.text === true) !== (typeof f.attr === 'string')));
    }
    return false;
  };
  return {
    files, calls, sessions,
    'browser.automation.run': async (request) => {
      if (unavailable) throw err('unavailable', 'browser engine not bound');
      if (Object.keys(request).some((k) => !['session_id', 'steps', 'timeout_ms'].includes(k))) throw err('invalid', '$ (deny_unknown_fields)');
      if (!Array.isArray(request.steps) || request.steps.length === 0 || request.steps.length > 32) throw err('invalid', '$.steps');
      if (!request.steps.every(validStep)) throw err('invalid', '$.steps');
      calls.push(request);
      const s = sessions.get(request.session_id) ?? { url: null, letter: 'A' };
      sessions.set(request.session_id, s);
      const out = [];
      for (const step of request.steps) {
        if (step.op === 'navigate') { s.url = step.params.url; s.letter = 'A'; out.push({ op: 'navigate', ok: true, result: {} }); continue; }
        if (step.op === 'wait') { out.push({ op: 'wait', ok: true, result: {} }); continue; }
        if (step.op === 'click') {
          const letter = /data-value="([A-Z])"/.exec(step.params.selector)?.[1];
          if (!pages[s.url]?.[letter]) { out.push({ op: 'click', ok: false, code: 'not_found' }); break; }
          s.letter = letter; out.push({ op: 'click', ok: true, result: {} }); continue;
        }
        const items = (pages[s.url]?.[s.letter] ?? []).slice(0, step.params.limit);
        out.push({ op: 'extract', ok: true, result: { items } });
      }
      const text = JSON.stringify({ session_id: request.session_id, steps: out });
      const ref = createHash('sha256').update(text).digest('hex');
      files.set(ref, text);
      return { session_id: request.session_id, steps: out.map(({ op, ok }) => ({ op, ok })), completed: out.filter((x) => x.ok).length, timed_out: false, artifact: { ref, file_id: `f_${ref.slice(0, 6)}` } };
    },
    'files.get': async ({ ref, ...rest }) => {
      if (Object.keys(rest).length) throw err('invalid', 'deny_unknown_fields');
      if (!files.has(ref)) throw err('not_found', 'file not found');
      return { id: 'f', ref, name: 'x.automation.json', mime: 'application/json', size: files.get(ref).length, owner_app: 'outbound', created_at_ms: 0, text: files.get(ref) };
    },
  };
}

function caps(log, kernel) {
  return {
    'knowledge.query': async ({ argv }) => { log.push(argv); return { ok: true }; },
    'browser.automation.run': kernel['browser.automation.run'],
    'files.get': kernel['files.get'],
    // E58: Import-Protokoll über files.put statt artifact.write.
    'files.put': async (content, meta) => { log.push(['artifact', { meta, json: JSON.parse(content.text) }]); return { id: meta?.id ?? 'f', ref: 'r', done: Promise.resolve() }; },
  };
}

const URL_ = 'https://messe.example/aussteller';
const basePayload = {
  source: { url: URL_, filter_prompt: 'Nur Firmen mit Sitz in Deutschland' },
  writeback_contract: { campaign_id: 'c1', domain: 'outbound', table_key: 'campaign_c1_companies' },
  title: 'Messe',
};
const teaser = (booth, name, event, country, description, href) => ({ text: [booth, name, event, country, description].filter((x) => x != null).join('\n'), href });

test('imports companies from extract steps with letter pagination (click + wait), skips missing letters', async () => {
  const log = [];
  const kernel = browserKernel({ pages: { [URL_]: {
    A: [teaser('Halle 1', 'Acme GmbH', 'Messe 26', 'Deutschland', 'Pumpen', 'https://www.acme.de/'), teaser('Halle 2', 'Gamma SA', 'Messe 26', 'Frankreich', 'x', 'https://gamma.fr/')],
    C: [teaser('Halle 3', 'Beta AG', 'Messe 26', 'Germany', 'Ventile', 'https://beta.de/')],
    // B fehlt → click scheitert, B wird übersprungen; M liegt im zweiten Aufruf (10 Buchstaben je Aufruf).
    M: [teaser('Halle 4', 'Mu KG', 'Messe 26', 'Deutschland', '', 'https://mu.de/'), teaser('Halle 3', 'Beta AG', 'Messe 26', 'Germany', 'Ventile', 'https://beta.de/')],
  } } });
  const t = createTestCtx({ now: 77, caps: caps(log, kernel) });
  const result = await handle(cmd('outbound.source.import', basePayload, { id: 'imp1', record_id: 'src1' }), t.ctx);
  assert.equal(result.records_count, 3, 'Acme, Beta, Mu – Gamma (Frankreich) gefiltert, Beta doppelt');
  const [open, ...pages] = kernel.calls;
  assert.deepEqual(open.steps.map((s) => s.op), ['navigate', 'wait', 'extract']);
  assert.equal(open.steps[0].params.url, URL_);
  assert.equal(open.steps[2].params.selector, 'a.teaser');
  assert.ok(kernel.calls.every((c) => c.session_id === open.session_id && c.timeout_ms === 180000 && c.steps.length <= 32));
  assert.ok(kernel.calls.every((c) => !('source' in c)), 'kein Skript-Quelltext mehr');
  assert.deepEqual(pages[0].steps.slice(0, 3).map((s) => s.op), ['click', 'wait', 'extract']);
  assert.match(pages[0].steps[0].params.selector, /data-value="B"/);
  assert.match(pages[1].steps[0].params.selector, /data-value="C"/, 'nach fehlendem B geht es mit C weiter');
  const append = log.find((a) => a[1] === 'append' && a[5] === 'campaign_c1_companies');
  const written = JSON.parse(append[append.indexOf('--rows') + 1]);
  assert.deepEqual(written.map((r) => r.company_name), ['Acme GmbH', 'Beta AG', 'Mu KG']);
  assert.equal(written[0].domain, 'acme.de');
  assert.equal(written[0].booth, 'Halle 1');
  assert.equal(written[0].event, 'Messe 26');
  assert.equal(written[0].country, 'Deutschland');
  assert.equal(written[0].source_id, 'src1');
  const artifact = log.find((e) => e[0] === 'artifact')[1];
  assert.equal(artifact.meta.name, 'outbound_companies.json');
  assert.equal(artifact.meta.mime, 'application/json');
  assert.equal(artifact.meta.id, 'outbound_companies-imp1');
  assert.equal(artifact.json.records_count, 3);
  assert.ok(artifact.json.automation.skipped_letters.includes('B'));
});

test('Kern ohne extract/click/wait (invalid) oder ohne Browser (unavailable): saubere Meldung unavailable, nichts geschrieben', async () => {
  for (const kernel of [browserKernel({ built: false }), browserKernel({ unavailable: true })]) {
    const log = [];
    const t = createTestCtx({ caps: caps(log, kernel) });
    const e = await rejects(handle(cmd('outbound.source.import', basePayload, { id: 'imp2' }), t.ctx));
    assert.equal(e.code, 'unavailable');
    assert.match(e.message, /^Firmen-Import nicht möglich: die Browser-Automatisierung des Kerns nimmt die Schritte \(navigate\/wait\/click\/extract\) noch nicht an \((invalid|unavailable): /);
    assert.equal(log.some((a) => a[1] === 'append' || a[0] === 'artifact'), false);
  }
});

test('guards, germany detection and empty page', async () => {
  assert.equal(germanyOnly({ definition: { country: 'DE' } }), true);
  assert.equal(germanyOnly({ filter_prompt: 'Österreich' }), false);
  const t = createTestCtx({ caps: caps([], browserKernel()) });
  assert.equal((await rejects(handle(cmd('outbound.source.import', { source: {} }), t.ctx))).code, 'invalid');
  assert.equal((await rejects(handle(cmd('outbound.source.import', basePayload), t.ctx))).code, 'not_found');
  const ohneDe = createTestCtx({ caps: caps([], browserKernel({ pages: { [URL_]: { A: [teaser('H', 'Gamma SA', 'M', 'Frankreich', '', 'https://g.fr/')] } } })) });
  const all = await handle(cmd('outbound.source.import', { ...basePayload, source: { url: URL_ }, filter_prompt: 'alle' }, { id: 'imp3' }), ohneDe.ctx).catch((e) => e);
  assert.equal(all.records_count, 1, 'ohne Deutschland-Filter bleibt Gamma');
});
