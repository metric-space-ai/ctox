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
  issueWindow,
  digestOf,
  withDigest,
  digestActive,
  cronForDigest,
  issueNodeLabel,
  normalizeStoryUrl,
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
const moduleDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');
// v4 (§16.13): einzige Schemaquelle ist collections.schema.json; schema.js ist gelöscht.
const { collections } = JSON.parse(await readFile(resolve(moduleDir, 'collections.schema.json'), 'utf8'));

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

test('collections.schema.json is the single schema source (schema.js removed)', async () => {
  await assert.rejects(readFile(resolve(moduleDir, 'schema.js'), 'utf8'));
  assert.deepEqual(Object.keys(collections).sort(), ['news_issues', 'news_posts', 'news_sources', 'news_topics']);
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
  assert.equal(generate.payload.required_skills, undefined, 'skill is chosen by the handler (news/news-aggregation)');
  assert.equal(generate.payload.issue_id, 'issue_1');
  assert.equal(generate.payload.thread_key, 'business-os/news/topic_1');
  assert.equal(generate.payload.record_snapshot.sources.length, 1, 'only enabled sources of this topic');
  assert.equal(generate.payload.instruction, undefined, 'prompt lives in prompts/issue.md');
  assert.equal(generate.payload.record_snapshot.topic.prompt, topic.prompt);

  const schedule = buildScheduleCommand(topic);
  assert.equal(schedule.command_type, NEWS_TOPIC_SCHEDULE);
  assert.equal(schedule.payload.cron_expr, '0 7 * * *');
  assert.equal(schedule.payload.enabled, true);
  const removed = buildScheduleCommand(topic, { deleted: true });
  assert.equal(removed.payload.enabled, false);
  assert.equal(removed.payload.cron_expr, '');

  const adapter = buildAdapterCommand(topic, sources[0]);
  assert.equal(adapter.payload.instruction, undefined, 'prompt lives in prompts/source-adapter.md');
  assert.ok(adapter.payload.record_snapshot.source);
  const auth = buildAuthCommand(topic, newSource('topic_1', 'x', '', 'Home'));
  assert.equal(auth.payload.target_url, 'https://x.com/home');
});

test('reddit and x read through a CTOX account, never an API', async () => {
  const model = await import('../core/news-model.mjs');
  const reddit = model.newSource('topic_a', 'reddit', '', 'https://www.reddit.com/r/LocalLLaMA/top/');
  assert.deepEqual(reddit.items, ['r/LocalLLaMA']);
  assert.equal(reddit.status, 'needs_auth');
  assert.equal(reddit.adapter_kind, 'browser_session');
  assert.equal(model.newSource('topic_a', 'x', '', 'Home').adapter_kind, 'browser_session');
  assert.equal(model.redditItem('not a sub!'), '');
  assert.equal(model.redditItem('MachineLearning'), 'r/MachineLearning');
  const merged = model.redditPresetSource([reddit], 'topic_a');
  assert.equal(merged.id, reddit.id);
  assert.deepEqual(merged.items, ['r/LocalLLaMA', 'r/MachineLearning', 'r/singularity']);
  assert.ok(!model.COMMUNITY_PRESET.some(([, url]) => url.includes('reddit.com')));

  const connect = model.buildAdapterCommand({ id: 'topic_a' }, reddit);
  assert.equal(connect.command_type, model.NEWS_SOURCE_ADAPTER);
  assert.equal(connect.payload.auth_source, 'reddit.com');
  assert.equal(connect.payload.instruction, undefined, 'prompt lives in prompts/source-connect.md');
  assert.equal(connect.payload.output_contract, model.SOURCE_REPLY_CONTRACT);
  assert.equal(model.buildAuthCommand({ id: 'topic_a' }, reddit).payload.target_url, 'https://www.reddit.com/');

  const topic = model.normalizeTopic({ id: 'topic_a', title: 'KI' });
  const generate = model.buildGenerateCommand(topic, [reddit], { id: 'issue_a', issue_date: '2026-10-14' }, null);
  // --task-id line: handler test news.issue.generate.test.mjs (prompts/issue.md, uses the kernel command id)
  assert.equal(generate.payload.record_snapshot.sources[0].auth_source, 'reddit.com');
});

test('a failed run replaced on the same day leaves the issue list; same-day runs show their time', async () => {
  const model = await import('../core/news-model.mjs');
  const at = (h) => new Date(2026, 9, 14, h, 0).getTime();
  const issues = [
    { id: 'a', topic_id: 't', issue_date: '2026-10-14', status: 'failed', created_at_ms: at(7) },
    { id: 'b', topic_id: 't', issue_date: '2026-10-14', status: 'ready', created_at_ms: at(3) },
    { id: 'c', topic_id: 't', issue_date: '2026-10-14', status: 'queued', created_at_ms: at(17) },
    { id: 'd', topic_id: 't', issue_date: '2026-10-13', status: 'failed', created_at_ms: at(1) },
  ];
  const listed = model.issuesForTopic(issues, 't');
  assert.deepEqual(listed.map((issue) => issue.id), ['c', 'b', 'd']);
  const today = new Date(2026, 9, 14, 18, 0);
  assert.equal(model.issueNodeLabel(listed[0], listed, 'de', today), 'Heute · 17:00');
  assert.equal(model.issueNodeLabel(listed[2], listed, 'de', today), 'Gestern');
});

test('legacy subreddit scrape targets fold into one Reddit account source', async () => {
  const model = await import('../core/news-model.mjs');
  const hn = model.newSource('t', 'community', 'Hacker News', 'https://news.ycombinator.com/');
  const a = model.newSource('t', 'community', 'r/LocalLLaMA', 'https://www.reddit.com/r/LocalLLaMA/');
  const b = model.newSource('t', 'community', 'r/MachineLearning', 'https://www.reddit.com/r/MachineLearning/');
  const { sources, removedIds } = model.migrateLegacyRedditSources([hn, a, b], 't');
  assert.deepEqual(removedIds, [a.id, b.id]);
  const reddit = sources.find((source) => source.kind === 'reddit');
  assert.deepEqual(reddit.items, ['r/LocalLLaMA', 'r/MachineLearning']);
  assert.equal(reddit.status, 'needs_auth');
  assert.ok(sources.includes(hn));
  const again = model.migrateLegacyRedditSources(sources, 't');
  assert.deepEqual(again.removedIds, []);
});

test('issue window runs since the previous issue and lists covered links', () => {
  const now = Date.parse('2026-10-15T07:00:00Z');
  const topic = { id: 't1', cadence: 'daily' };
  const issues = [
    { id: 'i1', topic_id: 't1', status: 'ready', generated_at_ms: now - 9 * 3_600_000 },
    { id: 'i2', topic_id: 't1', status: 'failed', generated_at_ms: now - 3_600_000 },
  ];
  const posts = [{ issue_id: 'i1', link_card: { url: 'https://www.Cohere.com/blog/new/' },
    sources: [{ url: 'https://news.ycombinator.com/item?id=1', kind: 'discussion' }] }];
  const window = issueWindow(topic, issues, posts, now);
  assert.equal(window.startMs, now - 9 * 3_600_000);
  assert.equal(window.previousIssueId, 'i1');
  assert.deepEqual(window.coveredUrls, ['cohere.com/blog/new']);
  assert.equal(issueWindow(topic, [], [], now).startMs, now - 24 * 3_600_000);
  assert.equal(issueWindow({ id: 't1', cadence: 'weekly' }, [], [], now).startMs, now - 7 * 24 * 3_600_000);
  assert.equal(normalizeStoryUrl('https://x.ai/news#a'), 'x.ai/news');
  const command = buildGenerateCommand({ ...normalizeTopic({ id: 't1', title: 'KI', cadence: 'daily' }) }, [], { id: 'i3', issue_date: '2026-10-15' }, issues[0], window);
  assert.equal(command.payload.window_start_ms, window.startMs);
  assert.equal(command.payload.window_start, new Date(window.startMs).toISOString());
  assert.equal(command.payload.previous_issue_id, 'i1');
  assert.deepEqual(command.payload.covered_urls, ['cohere.com/blog/new']);
  // v4: die Anweisung (inkl. Zeitfenster-Zeile) baut der Handler aus prompts/issue.md.
  assert.equal(command.payload.instruction, undefined);
  assert.equal(buildGenerateCommand(normalizeTopic({ id: 't1', title: 'KI' }), [], { id: 'i4', issue_date: '2026-10-15' }, null).payload.window_start_ms, undefined);
});

test('weekly digest rides on a daily topic with its own schedule', () => {
  const topic = normalizeTopic({ id: 't1', title: 'KI', cadence: 'daily', run_time: '07:00' });
  assert.deepEqual(digestOf(topic), { enabled: false, weekday: 5, run_time: '17:00' });
  const on = withDigest(topic, { enabled: true, weekday: 0, run_time: '18:30' });
  assert.equal(digestActive(on), true);
  assert.equal(cronForDigest(on), '30 18 * * 0');
  assert.equal(digestActive({ ...on, cadence: 'weekly' }), false);
  assert.equal(digestActive({ ...on, enabled: false }), false);
  const command = buildScheduleCommand(on, { digest: true });
  assert.equal(command.payload.mode, 'digest');
  assert.equal(command.payload.cron_expr, '30 18 * * 0');
  assert.equal(command.payload.enabled, true);
  assert.equal(buildScheduleCommand(on).payload.cron_expr, '0 7 * * *');
  assert.equal(buildScheduleCommand(on).payload.mode, undefined);
  const off = buildScheduleCommand(withDigest(on, { enabled: false }), { digest: true });
  assert.equal(off.payload.enabled, false);
  const gone = buildScheduleCommand(on, { digest: true, deleted: true });
  assert.deepEqual([gone.payload.mode, gone.payload.cron_expr, gone.payload.enabled, gone.payload.deleted], ['digest', '', false, true]);
  const today = new Date('2026-10-16T12:00:00Z');
  const digestIssue = { issue_date: '2026-10-16', payload: { kind: 'weekly_digest' } };
  assert.match(issueNodeLabel(digestIssue, [digestIssue], 'de', today), /^Woche · /);
  assert.match(issueNodeLabel(digestIssue, [digestIssue], 'en', today), /^Week · /);
  assert.doesNotMatch(issueNodeLabel({ issue_date: '2026-10-16', payload: {} }, [], 'de', today), /Woche/);
});
