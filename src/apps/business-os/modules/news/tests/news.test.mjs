import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

import {
  LAB_PRESET,
  NEWS_ISSUE_GENERATE,
  NEWS_TOPIC_SCHEDULE,
  buildAdapterCommand,
  buildAuthCommand,
  buildGenerateCommand,
  buildScheduleCommand,
  categoryCounts,
  compactCount,
  cronForTopic,
  issueLabel,
  issuesForTopic,
  mergeIntoAccountSource,
  newSource,
  normalizeTopic,
  postsForIssue,
  presetSources,
  topicsForBand,
  trendsForIssue,
  validateTopic,
} from '../core/news-model.mjs';
import { collections } from '../schema.js';

const moduleDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');

test('cadence maps to a five-field cron expression', () => {
  assert.equal(cronForTopic({ cadence: 'daily', run_time: '07:05' }), '5 7 * * *');
  assert.equal(cronForTopic({ cadence: 'weekly', run_time: '18:30', weekday: 1 }), '30 18 * * 1');
  assert.equal(cronForTopic({ cadence: 'deadline', run_time: '09:00', deadline_date: '2026-11-01' }), '0 9 1 11 *');
  assert.equal(cronForTopic({ cadence: 'deadline', run_time: '09:00', deadline_date: '' }), '');
  assert.equal(cronForTopic({ cadence: 'manual', run_time: '09:00' }), '');
});

test('topic normalization clamps values and validates the essentials', () => {
  const topic = normalizeTopic({ title: '  KI  ', cadence: 'nope', max_posts: 500, categories: ['model', 'bogus'] }, 1000);
  assert.equal(topic.title, 'KI');
  assert.equal(topic.cadence, 'daily');
  assert.equal(topic.max_posts, 40);
  assert.deepEqual(topic.categories, ['model']);
  assert.equal(topic.cron_expr, '0 7 * * *');
  assert.equal(topic.created_at_ms, 1000);
  assert.equal(validateTopic(normalizeTopic({ title: '' })), 'titleRequired');
  assert.equal(validateTopic(normalizeTopic({ title: 'A', cadence: 'deadline' })), 'deadlineRequired');
  assert.equal(validateTopic(topic), '');
});

test('topic records satisfy the declared schema shape', () => {
  const schema = collections.news_topics;
  const topic = normalizeTopic({ title: 'Lab-Releases' });
  for (const key of schema.required) assert.ok(key in topic, `topic misses required ${key}`);
  for (const [key, def] of Object.entries(schema.properties)) {
    if (!(key in topic)) continue;
    const actual = Array.isArray(topic[key]) ? 'array' : typeof topic[key];
    assert.equal(actual, def.type, `${key} should be ${def.type}`);
  }
  const source = newSource(topic.id, 'lab', 'OpenAI', 'https://openai.com/news/');
  for (const key of collections.news_sources.required) assert.ok(key in source, `source misses required ${key}`);
});

test('collections.schema.json and schema.js agree', async () => {
  const json = JSON.parse(await readFile(resolve(moduleDir, 'collections.schema.json'), 'utf8'));
  assert.deepEqual(Object.keys(json.collections).sort(), Object.keys(collections).sort());
  for (const [name, schema] of Object.entries(collections)) {
    assert.deepEqual(json.collections[name], JSON.parse(JSON.stringify(schema)), `${name} drifted`);
  }
});

test('sources: presets skip known urls and account sources merge selectors', () => {
  const existing = [newSource('t1', 'lab', 'OpenAI', LAB_PRESET[0][1])];
  const added = presetSources('t1', LAB_PRESET, 'lab', existing);
  assert.equal(added.length, LAB_PRESET.length - 1);
  assert.ok(added.every((source) => source.adapter_kind === 'scrape_target' && source.target_key.startsWith('news-')));

  const x = newSource('t1', 'x', '', 'Liste AI Labs');
  assert.equal(x.status, 'needs_auth');
  assert.equal(x.auth_mode, 'browser_session');
  assert.deepEqual(x.items, ['Liste AI Labs']);
  const merged = mergeIntoAccountSource([x], 't1', 'x', 'Suche new model');
  assert.deepEqual(merged.items, ['Liste AI Labs', 'Suche new model']);
  assert.equal(mergeIntoAccountSource([x], 't1', 'lab', 'https://a.example'), null);
});

test('feed filtering, ranking, counts and trends', () => {
  const posts = [
    { id: 'p1', issue_id: 'i1', category: 'model', rank: 2, hype_score: 90, discussion_count: 10, intro: 'Neues Modell. Mehr.', published_at_ms: 1 },
    { id: 'p2', issue_id: 'i1', category: 'tool', rank: 1, hype_score: 50, discussion_count: 900, intro: 'SDK', published_at_ms: 3, bookmarked: true },
    { id: 'p3', issue_id: 'i2', category: 'model', rank: 1, intro: 'other' },
  ];
  assert.deepEqual(postsForIssue(posts, 'i1').map((p) => p.id), ['p2', 'p1']);
  assert.deepEqual(postsForIssue(posts, 'i1', { filters: { sort: 'hype' } }).map((p) => p.id), ['p1', 'p2']);
  assert.deepEqual(postsForIssue(posts, 'i1', { band: 'model' }).map((p) => p.id), ['p1']);
  assert.deepEqual(postsForIssue(posts, 'i1', { filters: { saved: 'saved' } }).map((p) => p.id), ['p2']);
  assert.deepEqual(categoryCounts(posts, 'i1'), { all: 2, model: 1, tool: 1, research: 0, debate: 0 });
  assert.deepEqual(trendsForIssue({ id: 'i1' }, posts).map((t) => t.post_id), ['p2', 'p1']);
  assert.equal(trendsForIssue({ id: 'i1', payload: { trends: [{ title: 'Agent-Sandboxing', discussion_count: 860 }] } }, posts)[0].title, 'Agent-Sandboxing');
});

test('topics and issues are ordered for the left column', () => {
  const topics = [
    { id: 'a', title: 'B', enabled: true, cadence: 'daily', last_issue_at_ms: 5 },
    { id: 'b', title: 'A', enabled: false, cadence: 'weekly', last_issue_at_ms: 9 },
    { id: 'c', title: 'C', enabled: true, cadence: 'weekly', last_issue_at_ms: 7 },
  ];
  assert.deepEqual(topicsForBand(topics, { band: 'active' }).map((t) => t.id), ['c', 'a']);
  assert.deepEqual(topicsForBand(topics, { band: 'paused' }).map((t) => t.id), ['b']);
  assert.deepEqual(topicsForBand(topics, { band: 'active', filters: { cadence: 'daily' } }).map((t) => t.id), ['a']);
  const issues = [
    { id: 'x', topic_id: 'a', issue_date: '2026-10-12' },
    { id: 'y', topic_id: 'a', issue_date: '2026-10-13' },
    { id: 'z', topic_id: 'c', issue_date: '2026-10-14' },
  ];
  assert.deepEqual(issuesForTopic(issues, 'a').map((i) => i.id), ['y', 'x']);
});

test('labels for issues and counts', () => {
  const today = new Date(2026, 9, 13);
  assert.equal(issueLabel('2026-10-13', 'de', today), 'Heute');
  assert.equal(issueLabel('2026-10-12', 'de', today), 'Gestern');
  assert.match(issueLabel('2026-10-10', 'de', today), /10\.10\.$/);
  assert.equal(compactCount(2400, 'de'), '2,4 Tsd.');
  assert.equal(compactCount(860, 'de'), '860');
});

test('commands carry canonical type, record id, skill and record snapshot', () => {
  const topic = normalizeTopic({ id: 'topic_1', title: 'Lab-Releases', prompt: 'Neue Modelle' });
  const sources = [
    newSource('topic_1', 'lab', 'OpenAI', 'https://openai.com/news/'),
    { ...newSource('topic_1', 'youtube', '', 'https://www.youtube.com/@x'), enabled: false },
    newSource('other', 'lab', 'Fremd', 'https://example.com'),
  ];
  const generate = buildGenerateCommand(topic, sources, { id: 'issue_1', issue_date: '2026-10-13' }, null);
  assert.equal(generate.command_type, NEWS_ISSUE_GENERATE);
  assert.equal(generate.module, 'news');
  assert.equal(generate.record_id, 'topic_1');
  assert.equal(generate.command_id, generate.id);
  assert.equal('type' in generate, false);
  assert.deepEqual(generate.payload.required_skills, ['news-aggregation']);
  assert.equal(generate.payload.issue_id, 'issue_1');
  assert.equal(generate.payload.thread_key, 'business-os/news/topic_1');
  assert.equal(generate.payload.record_snapshot.sources.length, 1, 'only enabled sources of this topic');
  assert.match(generate.payload.instruction, /Lab-Releases/);

  const schedule = buildScheduleCommand(topic);
  assert.equal(schedule.command_type, NEWS_TOPIC_SCHEDULE);
  assert.equal(schedule.payload.cron_expr, '0 7 * * *');
  assert.equal(schedule.payload.enabled, true);
  const removed = buildScheduleCommand(topic, { deleted: true });
  assert.equal(removed.payload.enabled, false);
  assert.equal(removed.payload.cron_expr, '');

  const adapter = buildAdapterCommand(topic, sources[0]);
  assert.deepEqual(adapter.payload.required_skills, ['universal-scraping']);
  assert.ok(adapter.payload.record_snapshot.source);
  const auth = buildAuthCommand(topic, newSource('topic_1', 'x', '', 'Home'));
  assert.equal(auth.payload.target_url, 'https://x.com/home');
});
