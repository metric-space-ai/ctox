// Pure news-app logic: no DOM, no shell. Kept separate so node tests can
// exercise the scheduling, ranking, and command-shaping rules directly.

export const MODULE_ID = 'news';
export const NEWS_SKILL = 'news-aggregation';
export const SCRAPE_SKILL = 'universal-scraping';
export const ISSUE_OUTPUT_CONTRACT = 'ctox.news.issue.v1';
export const CATEGORIES = Object.freeze(['model', 'tool', 'research', 'debate']);
export const CADENCES = Object.freeze(['daily', 'weekly', 'deadline', 'manual']);
export const SOURCE_KINDS = Object.freeze(['lab', 'x', 'youtube', 'newsletter', 'community']);
export const NEWS_ISSUE_GENERATE = 'news.issue.generate';
export const NEWS_TOPIC_SCHEDULE = 'news.topic.schedule';
export const NEWS_SOURCE_ADAPTER = 'news.source.adapter';
export const NEWS_SOURCE_AUTH_ASSIST = 'news.source.auth_assist';
export const ISSUE_ACTIVE_STATUSES = Object.freeze(['queued', 'running']);

// Adapter kind per source kind: how CTOX reaches the source.
const SOURCE_ADAPTER = Object.freeze({
  lab: { adapter_kind: 'scrape_target', auth_mode: 'none' },
  community: { adapter_kind: 'scrape_target', auth_mode: 'none' },
  x: { adapter_kind: 'api_browser', auth_mode: 'browser_session' },
  youtube: { adapter_kind: 'youtube_transcript', auth_mode: 'none' },
  newsletter: { adapter_kind: 'mail', auth_mode: 'none' },
});

export const LAB_PRESET = Object.freeze([
  ['OpenAI', 'https://openai.com/news/'],
  ['Anthropic', 'https://www.anthropic.com/news'],
  ['Google DeepMind', 'https://deepmind.google/discover/blog/'],
  ['Google AI', 'https://blog.google/technology/ai/'],
  ['Meta AI', 'https://ai.meta.com/blog/'],
  ['Mistral AI', 'https://mistral.ai/news'],
  ['xAI', 'https://x.ai/news'],
  ['Microsoft AI', 'https://microsoft.ai/news/'],
  ['Apple ML', 'https://machinelearning.apple.com/'],
  ['Cohere', 'https://cohere.com/blog'],
  ['Ai2', 'https://allenai.org/blog'],
  ['Hugging Face', 'https://huggingface.co/blog'],
  ['DeepSeek', 'https://huggingface.co/deepseek-ai'],
  ['Qwen', 'https://huggingface.co/Qwen'],
  ['Moonshot AI', 'https://huggingface.co/moonshotai'],
  ['Z.ai', 'https://huggingface.co/zai-org'],
]);

export const COMMUNITY_PRESET = Object.freeze([
  ['Hacker News', 'https://news.ycombinator.com/'],
  ['r/LocalLLaMA', 'https://www.reddit.com/r/LocalLLaMA/'],
  ['r/MachineLearning', 'https://www.reddit.com/r/MachineLearning/'],
  ['Hugging Face Trending', 'https://huggingface.co/models?sort=trending'],
  ['GitHub Trending', 'https://github.com/trending'],
  ['arXiv cs.CL', 'https://arxiv.org/list/cs.CL/recent'],
]);

export function newId(prefix) {
  const random = globalThis.crypto?.randomUUID?.() || `${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 10)}`;
  return `${prefix}_${random}`;
}

export function slugKey(value) {
  return String(value || '')
    .toLowerCase()
    .normalize('NFKD')
    .replace(/[̀-ͯ]/g, '')
    .replace(/^https?:\/\//, '')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 80);
}

function parseTime(runTime) {
  const match = /^(\d{1,2}):(\d{2})$/.exec(String(runTime || '').trim());
  if (!match) return { hour: 7, minute: 0 };
  const hour = Math.min(23, Math.max(0, Number(match[1])));
  const minute = Math.min(59, Math.max(0, Number(match[2])));
  return { hour, minute };
}

// Five-field cron for `ctox schedule`. Manual topics have no schedule; a
// deadline is a one-shot day the backend disables after it fired.
export function cronForTopic(topic) {
  const cadence = CADENCES.includes(topic?.cadence) ? topic.cadence : 'manual';
  const { hour, minute } = parseTime(topic?.run_time);
  if (cadence === 'daily') return `${minute} ${hour} * * *`;
  if (cadence === 'weekly') {
    const weekday = Number.isInteger(Number(topic?.weekday)) ? Math.min(6, Math.max(0, Number(topic.weekday))) : 1;
    return `${minute} ${hour} * * ${weekday}`;
  }
  if (cadence === 'deadline') {
    const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(String(topic?.deadline_date || ''));
    if (!match) return '';
    return `${minute} ${hour} ${Number(match[3])} ${Number(match[2])} *`;
  }
  return '';
}

export function normalizeTopic(input, now = Date.now()) {
  const cadence = CADENCES.includes(input?.cadence) ? input.cadence : 'daily';
  const categories = Array.isArray(input?.categories)
    ? input.categories.filter((value) => CATEGORIES.includes(value))
    : [...CATEGORIES];
  const maxPosts = Math.round(Number(input?.max_posts));
  const topic = {
    id: String(input?.id || newId('topic')),
    title: String(input?.title || '').trim(),
    prompt: String(input?.prompt || '').trim(),
    cadence,
    run_time: /^\d{1,2}:\d{2}$/.test(String(input?.run_time || '')) ? String(input.run_time) : '07:00',
    weekday: Number.isFinite(Number(input?.weekday)) ? Number(input.weekday) : 1,
    deadline_date: String(input?.deadline_date || ''),
    language: input?.language === 'en' ? 'en' : 'de',
    categories: categories.length ? categories : [...CATEGORIES],
    max_posts: Number.isFinite(maxPosts) ? Math.min(40, Math.max(3, maxPosts)) : 12,
    enabled: input?.enabled !== false,
    schedule_task_id: String(input?.schedule_task_id || ''),
    last_issue_at_ms: Number(input?.last_issue_at_ms) || 0,
    sort_order: Number(input?.sort_order) || 0,
    payload: input?.payload && typeof input.payload === 'object' ? input.payload : {},
    created_at_ms: Number(input?.created_at_ms) || now,
    updated_at_ms: now,
  };
  topic.cron_expr = cronForTopic(topic);
  return topic;
}

export function validateTopic(topic) {
  if (!topic.title) return 'titleRequired';
  if (topic.cadence === 'deadline' && !topic.cron_expr) return 'deadlineRequired';
  return '';
}

export function newSource(topicId, kind, label, value, now = Date.now()) {
  const safeKind = SOURCE_KINDS.includes(kind) ? kind : 'lab';
  const adapter = SOURCE_ADAPTER[safeKind];
  const raw = String(value || '').trim();
  const isUrl = /^https?:\/\//i.test(raw);
  return {
    id: newId('source'),
    topic_id: topicId,
    kind: safeKind,
    label: String(label || sourceLabelFromValue(safeKind, raw)).trim() || raw,
    url: isUrl ? raw : '',
    adapter_kind: adapter.adapter_kind,
    target_key: safeKind === 'lab' || safeKind === 'community' ? `news-${slugKey(raw || label)}` : '',
    auth_mode: adapter.auth_mode,
    credential_secret_name: '',
    account_handle: '',
    // Free-form selectors per kind: X lists/searches, YouTube channels,
    // newsletter senders.
    items: isUrl || !raw ? [] : [raw],
    enabled: true,
    status: safeKind === 'x' ? 'needs_auth' : 'new',
    last_error: '',
    last_run_at_ms: 0,
    payload: {},
    created_at_ms: now,
    updated_at_ms: now,
  };
}

export function sourceLabelFromValue(kind, value) {
  const raw = String(value || '').trim();
  if (!raw) return '';
  if (kind === 'x') return 'X';
  if (kind === 'youtube') return 'YouTube';
  if (kind === 'newsletter') return 'Newsletter';
  try {
    const url = new URL(raw);
    return url.hostname.replace(/^www\./, '');
  } catch {
    return raw;
  }
}

// Adding a selector to an existing account-style source (X, YouTube,
// newsletter) extends that source instead of creating a parallel one.
export function mergeIntoAccountSource(sources, topicId, kind, value, now = Date.now()) {
  const raw = String(value || '').trim();
  if (!raw || kind === 'lab' || kind === 'community') return null;
  const existing = sources.find((source) => source.topic_id === topicId && source.kind === kind);
  if (!existing) return null;
  const items = Array.isArray(existing.items) ? existing.items : [];
  if (items.includes(raw)) return existing;
  return { ...existing, items: [...items, raw], updated_at_ms: now };
}

export function presetSources(topicId, preset, kind, existing = [], now = Date.now()) {
  const known = new Set(existing.filter((source) => source.topic_id === topicId).map((source) => source.url));
  return preset
    .filter(([, url]) => !known.has(url))
    .map(([label, url]) => newSource(topicId, kind, label, url, now));
}

export function issueDateKey(date = new Date()) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
}

export function issueLabel(issueDate, lang = 'de', today = new Date()) {
  const todayKey = issueDateKey(today);
  const yesterday = new Date(today);
  yesterday.setDate(today.getDate() - 1);
  if (issueDate === todayKey) return lang === 'en' ? 'Today' : 'Heute';
  if (issueDate === issueDateKey(yesterday)) return lang === 'en' ? 'Yesterday' : 'Gestern';
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(String(issueDate || ''));
  if (!match) return String(issueDate || '');
  const date = new Date(Number(match[1]), Number(match[2]) - 1, Number(match[3]));
  const weekday = date.toLocaleDateString(lang === 'en' ? 'en-US' : 'de-DE', { weekday: 'short' }).replace('.', '');
  return `${weekday} ${match[3]}.${match[2]}.`;
}

export function relativeTime(ms, lang = 'de', now = Date.now()) {
  const value = Number(ms);
  if (!value) return '';
  const minutes = Math.max(0, Math.round((now - value) / 60000));
  if (minutes < 60) return lang === 'en' ? `${Math.max(1, minutes)}m` : `${Math.max(1, minutes)} Min.`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return lang === 'en' ? `${hours}h` : `${hours} Std.`;
  const days = Math.round(hours / 24);
  if (days < 7) return lang === 'en' ? `${days}d` : `${days} T.`;
  return new Date(value).toLocaleDateString(lang === 'en' ? 'en-US' : 'de-DE', { day: 'numeric', month: 'short' });
}

export function compactCount(value, lang = 'de') {
  const number = Math.round(Number(value) || 0);
  if (number < 1000) return String(number);
  const scaled = Math.round(number / 100) / 10;
  const text = lang === 'en' ? String(scaled) : String(scaled).replace('.', ',');
  return lang === 'en' ? `${text}k` : `${text} Tsd.`;
}

export function monogram(name) {
  const words = String(name || '?').replace(/[^\p{L}\p{N}\s]/gu, ' ').trim().split(/\s+/).filter(Boolean);
  if (!words.length) return '?';
  if (words.length === 1) return words[0].slice(0, 2).toUpperCase();
  return (words[0][0] + words[1][0]).toUpperCase();
}

// Stable 0..5 tone bucket so avatars differ without hard-coded colors.
export function toneBucket(value) {
  let hash = 0;
  for (const char of String(value || '')) hash = (hash * 31 + char.codePointAt(0)) >>> 0;
  return hash % 6;
}

export function topicsForBand(topics, grammar = {}) {
  const band = grammar.band || 'active';
  const search = String(grammar.search || '').toLowerCase();
  const cadence = grammar.filters?.cadence || 'all';
  const sort = grammar.filters?.sort || 'recent';
  return topics
    .filter((topic) => (band === 'paused' ? topic.enabled === false : topic.enabled !== false))
    .filter((topic) => cadence === 'all' || topic.cadence === cadence)
    .filter((topic) => !search || `${topic.title} ${topic.prompt}`.toLowerCase().includes(search))
    .sort((a, b) => (sort === 'title'
      ? String(a.title).localeCompare(String(b.title))
      : (Number(b.last_issue_at_ms) || Number(b.updated_at_ms) || 0) - (Number(a.last_issue_at_ms) || Number(a.updated_at_ms) || 0)));
}

export function issuesForTopic(issues, topicId, limit = 6) {
  return issues
    .filter((issue) => issue.topic_id === topicId)
    .sort((a, b) => String(b.issue_date).localeCompare(String(a.issue_date)) || (Number(b.created_at_ms) || 0) - (Number(a.created_at_ms) || 0))
    .slice(0, limit);
}

export function postsForIssue(posts, issueId, grammar = {}) {
  const band = grammar.band || 'all';
  const search = String(grammar.search || '').toLowerCase();
  const sort = grammar.filters?.sort || 'rank';
  const savedOnly = grammar.filters?.saved === 'saved';
  return posts
    .filter((post) => post.issue_id === issueId)
    .filter((post) => band === 'all' || post.category === band)
    .filter((post) => !savedOnly || post.bookmarked === true)
    .filter((post) => !search || `${post.author_name} ${post.intro} ${post.body}`.toLowerCase().includes(search))
    .sort((a, b) => {
      if (sort === 'hype') return (Number(b.hype_score) || 0) - (Number(a.hype_score) || 0);
      if (sort === 'time') return (Number(b.published_at_ms) || 0) - (Number(a.published_at_ms) || 0);
      return (Number(a.rank) || 999) - (Number(b.rank) || 999);
    });
}

export function categoryCounts(posts, issueId) {
  const counts = { all: 0, model: 0, tool: 0, research: 0, debate: 0 };
  for (const post of posts) {
    if (post.issue_id !== issueId) continue;
    counts.all += 1;
    if (post.category in counts) counts[post.category] += 1;
  }
  return counts;
}

// Trends come from the generator (issue.payload.trends); without them the
// most discussed posts of the issue stand in.
export function trendsForIssue(issue, posts, limit = 5) {
  const declared = Array.isArray(issue?.payload?.trends) ? issue.payload.trends : [];
  if (declared.length) {
    return declared.slice(0, limit).map((trend) => ({
      title: String(trend.title || ''),
      category: String(trend.category || ''),
      discussion_count: Number(trend.discussion_count) || 0,
      post_id: String(trend.post_id || ''),
    })).filter((trend) => trend.title);
  }
  return posts
    .filter((post) => post.issue_id === issue?.id)
    .sort((a, b) => (Number(b.discussion_count) || 0) - (Number(a.discussion_count) || 0) || (Number(b.hype_score) || 0) - (Number(a.hype_score) || 0))
    .slice(0, limit)
    .map((post) => ({
      title: firstSentence(post.intro),
      category: post.category || '',
      discussion_count: Number(post.discussion_count) || 0,
      post_id: post.id,
    }));
}

export function firstSentence(text, max = 72) {
  const clean = String(text || '').replace(/\s+/g, ' ').trim();
  const cut = clean.split(/(?<=[.!?])\s/)[0] || clean;
  return cut.length > max ? `${cut.slice(0, max - 1).trimEnd()}…` : cut;
}

export function domainOf(url) {
  try {
    return new URL(url).hostname.replace(/^www\./, '');
  } catch {
    return '';
  }
}

function commandEnvelope({ command_type: commandType, record_id: recordId, payload, sync_collections: syncCollections }) {
  const commandId = newId(`cmd_${commandType.replaceAll('.', '_')}`);
  return {
    id: commandId,
    command_id: commandId,
    module: MODULE_ID,
    command_type: commandType,
    record_id: recordId,
    inbound_channel: 'business_os.news',
    sync_collections: syncCollections,
    payload,
    client_context: { source_module: MODULE_ID, command_path: commandType },
  };
}

export function sourceSnapshot(source) {
  return {
    id: source.id,
    kind: source.kind,
    label: source.label,
    url: source.url,
    adapter_kind: source.adapter_kind,
    target_key: source.target_key,
    auth_mode: source.auth_mode,
    items: Array.isArray(source.items) ? source.items : [],
    status: source.status,
  };
}

export function topicSnapshot(topic) {
  return {
    id: topic.id,
    title: topic.title,
    prompt: topic.prompt,
    cadence: topic.cadence,
    language: topic.language,
    categories: topic.categories,
    max_posts: topic.max_posts,
  };
}

export function buildIssueInstruction(topic, sources, previousIssue) {
  const enabled = sources.filter((source) => source.enabled !== false);
  const lines = [
    `Erstelle die News-Ausgabe für das Thema "${topic.title}" mit dem Skill ${NEWS_SKILL}.`,
    topic.prompt ? `Fokus: ${topic.prompt}` : '',
    `Sprache: ${topic.language === 'en' ? 'Englisch' : 'Deutsch'}. Höchstens ${topic.max_posts} Beiträge, Kategorien: ${topic.categories.join(', ')}.`,
    `Quellen (${enabled.length}): ${enabled.map((source) => `${source.kind}:${source.label}`).join('; ') || 'keine konfiguriert, nutze die Websuche'}.`,
    previousIssue ? `Vorherige Ausgabe ${previousIssue.issue_date}: wiederhole nur Meldungen mit neuer Entwicklung.` : '',
    `Liefere das Ergebnis im Vertrag ${ISSUE_OUTPUT_CONTRACT}; jeder Beitrag braucht mindestens eine Quelle mit Link.`,
  ];
  return lines.filter(Boolean).join('\n');
}

export function buildGenerateCommand(topic, sources, issue, previousIssue) {
  const topicSources = sources.filter((source) => source.topic_id === topic.id);
  return commandEnvelope({
    command_type: NEWS_ISSUE_GENERATE,
    record_id: topic.id,
    payload: {
    topic_id: topic.id,
    issue_id: issue.id,
    issue_date: issue.issue_date,
    instruction: buildIssueInstruction(topic, topicSources, previousIssue),
    required_skills: [NEWS_SKILL],
    thread_key: `business-os/news/${topic.id}`,
    output_contract: ISSUE_OUTPUT_CONTRACT,
    previous_issue_id: previousIssue?.id || '',
    record_snapshot: {
      topic: topicSnapshot(topic),
      sources: topicSources.filter((source) => source.enabled !== false).map(sourceSnapshot),
    },
  },
    sync_collections: ['news_issues', 'news_posts'],
  });
}

function localTimezone() {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || 'Europe/Berlin';
  } catch {
    return 'Europe/Berlin';
  }
}

export function buildScheduleCommand(topic, { deleted = false } = {}) {
  return commandEnvelope({
    command_type: NEWS_TOPIC_SCHEDULE,
    record_id: topic.id,
    payload: {
    topic_id: topic.id,
    title: topic.title,
    cadence: topic.cadence,
    cron_expr: deleted ? '' : topic.cron_expr,
    enabled: deleted ? false : topic.enabled !== false && Boolean(topic.cron_expr),
    deleted,
    deadline_date: topic.deadline_date || '',
    timezone: localTimezone(),
    record_snapshot: { topic: topicSnapshot(topic) },
  },
    sync_collections: ['news_topics'],
  });
}

export function buildAdapterCommand(topic, source) {
  return commandEnvelope({
    command_type: NEWS_SOURCE_ADAPTER,
    record_id: source.id,
    payload: {
    topic_id: topic?.id || source.topic_id,
    source_id: source.id,
    instruction: [
      `Richte für die News-Quelle "${source.label}" (${source.url || source.items?.join(', ') || source.kind}) einen wiederverwendbaren Scraper mit dem Skill ${SCRAPE_SKILL} ein.`,
      `target_key: ${source.target_key}. Extrahiere je Meldung Titel, Datum, URL und Kurztext; teste den Lauf einmal.`,
    ].join('\n'),
    required_skills: [SCRAPE_SKILL],
    thread_key: `business-os/news/${source.topic_id}/sources`,
    record_snapshot: { source: sourceSnapshot(source) },
  },
    sync_collections: ['news_sources'],
  });
}

export function buildAuthCommand(topic, source) {
  return commandEnvelope({
    command_type: NEWS_SOURCE_AUTH_ASSIST,
    record_id: source.id,
    payload: {
    topic_id: topic?.id || source.topic_id,
    source_id: source.id,
    target_url: source.kind === 'x' ? 'https://x.com/home' : source.url,
    record_snapshot: { source: sourceSnapshot(source) },
  },
    sync_collections: ['news_sources'],
  });
}
