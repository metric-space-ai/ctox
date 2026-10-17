// Nachport 9f6ad058a (+ d86fb3ac8): Laufvertrag an jeder news.issue.generate-Anweisung, auch beim Knopf.
import assert from 'node:assert/strict';
import test from 'node:test';
import './_ctox-shared.mjs';
import { cmd } from './_ctx.mjs';
import { NOW } from './_fixtures.mjs';
import { world } from './_sim.mjs';

const { default: generate } = await import('../news.issue.generate.js');
const { selectRunSources, MAX_SOURCES_PER_RUN } = await import('../_lib/news.js');

const issue = { id: 'issue_1', topic_id: 'topic_1', issue_date: '2026-10-14', status: 'queued', payload: {}, created_at_ms: NOW, updated_at_ms: NOW };
const topic = { id: 'topic_1', title: 'KI', prompt: '', cadence: 'daily', language: 'de', categories: ['model'], max_posts: 5 };
const EFFORT = '- Höchstens zwei Abrufe pro Quelle, keine Quelle zweimal lesen; sobald genug Beiträge belegt sind, nicht weiter suchen, sondern das Ergebnis liefern.';

async function instruction(sources, id = 'cmd_run') {
  const w = await world({ seed: { news_issues: [issue] } });
  const c = cmd('news.issue.generate', { topic_id: 'topic_1', issue_id: 'issue_1', issue_date: '2026-10-14', record_snapshot: { topic, sources } }, { id });
  await generate(c, w.attach(c));
  return w.sim.enqueued[0].args.instruction;
}

test('selectRunSources: failed/needs_auth/auth_requested fallen weg, Rest gedeckelt', () => {
  const many = Array.from({ length: MAX_SOURCES_PER_RUN + 2 }, (_, i) => ({ label: `S${i}`, status: 'active' }));
  const { usable, skipped } = selectRunSources([{ label: 'Kaputt', status: 'failed' }, { label: 'Login', status: 'needs_auth' }, { label: 'Offen', status: 'auth_requested' }, ...many]);
  assert.equal(usable.length, MAX_SOURCES_PER_RUN);
  assert.deepEqual(skipped.map((s) => s.reason), ['failed', 'needs_auth', 'auth_requested', 'limit', 'limit']);
});

test('manuelle Ausgabe: Laufvertrag mit Aufwandsgrenze; unbrauchbare Quellen werden genannt und nicht gelesen', async () => {
  const text = await instruction([
    { id: 'source_ok', kind: 'lab', label: 'OpenAI', status: 'active' },
    { id: 'source_bad', kind: 'lab', label: 'Kaputt', status: 'failed' },
    { id: 'source_x', kind: 'x', label: 'X-Feed', status: 'needs_auth', auth_source: 'x.com' },
  ]);
  const lines = text.split('\n');
  assert.ok(lines.includes('Quellen (1): lab:OpenAI.'));
  assert.ok(!lines.some((l) => l.startsWith('Angemeldete Quellen')), 'abgemeldete X-Quelle zaehlt nicht als angemeldet');
  const at = lines.indexOf('News-Laufvertrag (verbindlich):');
  assert.ok(at > 0);
  assert.deepEqual(lines.slice(at + 1, at + 4), ['- Nur diese Quellen lesen: OpenAI.', '- Überspringen: Kaputt (failed); X-Feed (needs_auth).', EFFORT]);
  const context = JSON.parse(text.slice(text.indexOf('{')));
  assert.deepEqual(context.skipped_sources, [{ label: 'Kaputt', reason: 'failed' }, { label: 'X-Feed', reason: 'needs_auth' }]);
});

test('ohne nutzbare Quelle: Websuche; ohne Ausfaelle nur die Aufwandsgrenze', async () => {
  const none = (await instruction([{ id: 's', kind: 'lab', label: 'Kaputt', status: 'failed' }])).split('\n');
  assert.ok(none.includes('Quellen (0): keine konfiguriert, nutze die Websuche.'));
  assert.ok(none.includes('- Keine nutzbare Quelle: nutze die Websuche.'));
  const clean = (await instruction([{ id: 's', kind: 'lab', label: 'OpenAI' }])).split('\n');
  const at = clean.indexOf('News-Laufvertrag (verbindlich):');
  assert.equal(clean[at + 1], EFFORT);
  assert.ok(!clean.some((l) => l.startsWith('- Überspringen')));
});
