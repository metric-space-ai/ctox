import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import './_ctox-shared.mjs';
import { sha256Hex } from '../_lib/sha256.js';

// _lib/news.js laedt ctox:shared/prompt-template.js → erst nach der Aufloesungs-Registrierung importieren.
const { accountAuthSource, extractJsonObject, generatePayload, parseIssueReply, parseSourceReply, validRecordId } = await import('../_lib/news.js');

const reply = `Fertig.
\`\`\`json
{"schema":"ctox.news.issue.v1","summary":"Zwei Releases",
 "posts":[
  {"category":"model","author_name":"OpenAI","author_handle":"@openai","author_kind":"lab",
   "intro":"Neues Modell","body":"Lang","hype_score":140,"discussion_count":2400,
   "published_at":"2026-10-13T05:00:00Z",
   "link_card":{"url":"https://openai.com/x","title":"T","image_url":"javascript:alert(1)"},
   "sources":[{"label":"Blog","url":"https://openai.com/x"},{"label":"bad","url":"ftp://x"}]},
  {"category":"weird","intro":"Ohne Quelle","sources":[]},
  {"intro":"","sources":[{"url":"https://a.example"}]}
 ],
 "trends":[{"title":"Agent-Sandboxing","discussion_count":860},{"title":""}],
 "source_report":[{"source_id":"source_x","status":"needs_auth","note":"login wall"},{"source_id":"../x","status":"ok"},{"source_id":"source_y","status":"great"}]}
\`\`\``;

test('Rust-Test issue_reply_keeps_only_sourced_posts_and_clamps_values', () => {
  const parsed = parseIssueReply(reply);
  assert.equal(parsed.posts.length, 1);
  assert.equal(parsed.dropped, 2);
  const post = parsed.posts[0];
  assert.equal(post.hype_score, 100);
  assert.equal(post.author_handle, 'openai');
  assert.equal(post.sources.length, 1);
  assert.equal(post.link_card.image_url, '');
  assert.equal(post.published_at_ms, 1_791_867_600_000);
  assert.equal(parsed.trends.length, 1);
  assert.equal(parsed.summary, 'Zwei Releases');
  assert.deepEqual(parsed.sourceReport, [['source_x', 'needs_auth', 'login wall']]);
});

test('Rust-Test issue_reply_rejects_wrong_schema_and_empty_feeds', () => {
  for (const text of ['{"schema":"other","posts":[]}', '{"posts":[{"intro":"x","sources":[]}]}', 'kein json', '  ']) {
    assert.throws(() => parseIssueReply(text), (e) => e.code === 'invalid');
  }
});

test('Rust-Test record_ids_are_restricted', () => {
  assert.equal(validRecordId('topic_63ce100b-5c0e-46f4'), true);
  assert.equal(validRecordId('../etc'), false);
  assert.equal(validRecordId(''), false);
  assert.equal(validRecordId('x'.repeat(221)), false);
});

test('Rust-Test account_connection_reply_maps_onto_the_source_row', () => {
  assert.deepEqual(
    parseSourceReply('Verbunden. {"schema":"ctox.news.source.v1","source_id":"source_1","status":"connected","account":"ctox_news","note":"x"}'),
    { status: 'ok', account: 'ctox_news', note: '' },
  );
  const owner = parseSourceReply('{"schema":"ctox.news.source.v1","status":"needs_owner","account":"a b","note":"CAPTCHA im Browser lösen"}');
  assert.equal(owner.status, 'needs_auth');
  assert.equal(owner.account, '');
  assert.equal(owner.note, 'CAPTCHA im Browser lösen');
  assert.equal(parseSourceReply('{"schema":"ctox.news.source.v1","status":"weird"}').status, 'failed');
  assert.equal(parseSourceReply('Scraper läuft.').status, 'ok');
  assert.equal(accountAuthSource('reddit'), 'reddit.com');
  assert.equal(accountAuthSource('lab'), undefined);
});

test('generatePayload (Planer, generate_payload :446): Schnappschuss wie Rust, Anweisung kommt aus prompts/issue.md', () => {
  const t = { id: 'topic_1', title: 'KI', prompt: '', max_posts: 5 };
  const reddit = { id: 'source_1', kind: 'reddit', label: 'Reddit', items: ['r/LocalLLaMA'] };
  const lab = { id: 'source_2', kind: 'lab', label: 'OpenAI' };
  const p = generatePayload(t, [reddit, lab], 'issue_1', '2026-10-14');
  assert.equal(p.record_snapshot.sources[0].auth_source, 'reddit.com');
  assert.equal(p.record_snapshot.sources[1].url, null);
  assert.equal(p.instruction, undefined);
  assert.equal(p.required_skills, undefined);
  // Rust-Test scheduled_issue_names_its_command_for_signed_in_sources: tests/news.issue.generate.test.mjs
});

test('extractJsonObject findet das erste ausgeglichene Objekt, auch mit } in Strings', () => {
  assert.deepEqual(extractJsonObject('vorher {"a":"x}y","b":{"c":1}} nachher {"d":2}'), { a: 'x}y', b: { c: 1 } });
  assert.equal(extractJsonObject('[1,2]'), undefined);
});

test('sha256Hex entspricht node:crypto', () => {
  for (const s of ['', 'task::2026-10-14T07:00:00+02:00', 'Ünïcödé 🚀']) {
    assert.equal(sha256Hex(s), createHash('sha256').update(s).digest('hex'));
  }
});
