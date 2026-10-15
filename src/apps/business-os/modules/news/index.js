import { loadModuleMessages } from '../../shared/i18n.js';
import { renderHtmlIfChanged } from '../../shared/stable-dom.js';
import {
  CATEGORIES,
  COMMUNITY_PRESET,
  ISSUE_ACTIVE_STATUSES,
  LAB_PRESET,
  SOURCE_KINDS,
  authSourceFor,
  buildAdapterCommand,
  buildAuthCommand,
  buildGenerateCommand,
  digestActive,
  digestOf,
  withDigest,
  issueWindow,
  buildScheduleCommand,
  categoryCounts,
  compactCount,
  domainOf,
  issueDateKey,
  issueLabel,
  issueNodeLabel,
  issuesForTopic,
  mergeIntoAccountSource,
  migrateLegacyRedditSources,
  monogram,
  newId,
  newSource,
  normalizeTopic,
  postsForIssue,
  presetSources,
  redditPresetSource,
  relativeTime,
  toneBucket,
  topicsForBand,
  trendsForIssue,
  validateTopic,
} from './core/news-model.mjs';

const COLLECTIONS = Object.freeze({
  topics: 'news_topics',
  sources: 'news_sources',
  issues: 'news_issues',
  posts: 'news_posts',
});
const RENDER_DEBOUNCE_MS = 60;

const labels = {
  de: {
    topicsTitle: 'Themen',
    trendTitle: 'Im Trend',
    noTopicSelected: 'Kein Thema ausgewählt',
    emptyTopicsTitle: 'Lege dein erstes Thema an',
    emptyTopicsBody: 'Ein Thema bündelt Quellen und einen Rhythmus zu einem eigenen Feed.',
    createTopic: 'Thema anlegen',
    noTopicsInView: 'Keine Themen in dieser Ansicht.',
    emptyIssueTitle: 'Noch keine Ausgabe',
    emptyIssueBody: 'CTOX recherchiert die Quellen und schreibt daraus den Feed.',
    generateFirst: 'Erste Ausgabe erzeugen',
    issueRunning: 'Ausgabe wird erstellt',
    issueRunningBody: 'CTOX liest gerade die Quellen. Der Feed erscheint hier, sobald er fertig ist.',
    issueFailed: 'Ausgabe fehlgeschlagen',
    retry: 'Erneut erzeugen',
    noPostsInView: 'Keine Beiträge in dieser Ansicht.',
    showMore: 'Mehr anzeigen',
    showLess: 'Weniger anzeigen',
    sources: 'Quellen',
    discussions: 'Diskussionen',
    hype: 'Hype',
    save: 'Merken',
    saved: 'Gemerkt',
    copyLink: 'Link kopieren',
    linkCopied: 'Link kopiert',
    editorial: 'CTOX Redaktion',
    topicsFooter: '{0} Themen',
    postsFooter: '{0} Beiträge · {1}',
    newTopic: 'Neues Thema',
    titleRequired: 'Gib dem Thema einen Namen.',
    deadlineRequired: 'Wähle einen Stichtag.',
    saveFailed: 'Speichern fehlgeschlagen',
    drawerUndo: 'Rückgängig',
    drawerUndone: 'Rückgängig gemacht',
    createTopic: 'Thema anlegen',
    savedSourceOn: '„{0}“ aktiviert',
    savedSourceOff: '„{0}“ deaktiviert',
    savedSourceRemoved: '„{0}“ entfernt',
    savedSourcesAdded: 'Quellen hinzugefügt',
    savedTopic: 'Gespeichert',
    commandFailed: 'Auftrag konnte nicht an CTOX übergeben werden',
    commandQueued: 'Auftrag an CTOX übergeben',
    deleteConfirm: 'Nochmal klicken: „{0}“ mit allen Ausgaben löschen',
    deleteTopic: 'Thema löschen',
    groupLab: 'Labs',
    groupX: 'X',
    groupReddit: 'Reddit',
    groupYoutube: 'YouTube',
    groupNewsletter: 'Newsletter',
    groupCommunity: 'Communities',
    addLabPreset: 'Alle KI-Labs übernehmen',
    addCommunityPreset: 'Communities übernehmen',
    addRedditPreset: 'KI-Subreddits übernehmen',
    sourceAdapter: 'Scraper einrichten',
    sourceConnect: 'Konto verbinden',
    sourceLogin: 'Im Browser anmelden',
    sourceRemove: 'Quelle entfernen',
    sourceActive: 'Quelle aktiv',
    statusNeedsAuth: 'Anmeldung nötig',
    statusFailed: 'Fehler',
    statusRepairing: 'wird repariert',
    statusPending: 'in Arbeit',
    statusAuthRequested: 'Anmeldung offen',
    saveTopicFirst: 'Speichere das Thema, bevor du Quellen einrichtest.',
    placeholderLab: 'https://www.example.com/blog',
    placeholderX: 'Liste, Suche oder @account',
    placeholderYoutube: 'https://www.youtube.com/@kanal',
    placeholderNewsletter: 'absender@newsletter.com',
    placeholderCommunity: 'https://news.ycombinator.com/',
    placeholderReddit: 'r/subreddit',
    showAsList: 'Als Liste anzeigen',
    showAsCards: 'Als Karten anzeigen',
    importDone: '{0} Themen importiert',
    importFailed: 'Import fehlgeschlagen',
    daily: 'Täglich {0}',
    weekly: '{1} {0}',
    deadline: 'Stichtag {0}',
    manual: 'Manuell',
    digestLabel: 'Wochenzusammenfassung',
    digestHint: 'Fasst die Tagesausgaben der Woche zu Top-Meldungen und Trends zusammen, ohne neue Recherche.',
    digestWeekday: 'Wochentag',
    digestTime: 'Uhrzeit',
    digestSavedOn: 'Wochenzusammenfassung {0} {1} gespeichert',
    digestSavedOff: 'Wochenzusammenfassung ausgeschaltet',
    undo: 'Rückgängig',
    undone: 'Rückgängig gemacht',
    digestCadence: '{0} · Woche {1}',
  },
  en: {
    topicsTitle: 'Topics',
    trendTitle: 'Trending',
    noTopicSelected: 'No topic selected',
    emptyTopicsTitle: 'Create your first topic',
    emptyTopicsBody: 'A topic bundles sources and a cadence into its own feed.',
    createTopic: 'Create topic',
    noTopicsInView: 'No topics in this view.',
    emptyIssueTitle: 'No issue yet',
    emptyIssueBody: 'CTOX researches the sources and writes the feed from them.',
    generateFirst: 'Create first issue',
    issueRunning: 'Issue in progress',
    issueRunningBody: 'CTOX is reading the sources. The feed appears here when it is done.',
    issueFailed: 'Issue failed',
    retry: 'Generate again',
    noPostsInView: 'No posts in this view.',
    showMore: 'Show more',
    showLess: 'Show less',
    sources: 'Sources',
    discussions: 'Discussions',
    hype: 'Hype',
    save: 'Save',
    saved: 'Saved',
    copyLink: 'Copy link',
    linkCopied: 'Link copied',
    editorial: 'CTOX Editorial',
    topicsFooter: '{0} topics',
    postsFooter: '{0} posts · {1}',
    newTopic: 'New topic',
    titleRequired: 'Give the topic a name.',
    deadlineRequired: 'Pick a deadline.',
    saveFailed: 'Saving failed',
    drawerUndo: 'Undo',
    drawerUndone: 'Undone',
    createTopic: 'Create topic',
    savedSourceOn: '“{0}” enabled',
    savedSourceOff: '“{0}” disabled',
    savedSourceRemoved: '“{0}” removed',
    savedSourcesAdded: 'Sources added',
    savedTopic: 'Saved',
    commandFailed: 'Could not hand the task to CTOX',
    commandQueued: 'Task handed to CTOX',
    deleteConfirm: 'Click again: delete “{0}” with all issues',
    deleteTopic: 'Delete topic',
    groupLab: 'Labs',
    groupX: 'X',
    groupReddit: 'Reddit',
    groupYoutube: 'YouTube',
    groupNewsletter: 'Newsletters',
    groupCommunity: 'Communities',
    addLabPreset: 'Add all AI labs',
    addCommunityPreset: 'Add communities',
    addRedditPreset: 'Add AI subreddits',
    sourceAdapter: 'Set up scraper',
    sourceConnect: 'Connect account',
    sourceLogin: 'Sign in in the browser',
    sourceRemove: 'Remove source',
    sourceActive: 'Source active',
    statusNeedsAuth: 'Sign-in needed',
    statusFailed: 'Error',
    statusRepairing: 'repairing',
    statusPending: 'in progress',
    statusAuthRequested: 'sign-in open',
    saveTopicFirst: 'Save the topic before setting up sources.',
    placeholderLab: 'https://www.example.com/blog',
    placeholderX: 'List, search, or @account',
    placeholderYoutube: 'https://www.youtube.com/@channel',
    placeholderNewsletter: 'sender@newsletter.com',
    placeholderCommunity: 'https://news.ycombinator.com/',
    placeholderReddit: 'r/subreddit',
    showAsList: 'Show as list',
    showAsCards: 'Show as cards',
    importDone: '{0} topics imported',
    importFailed: 'Import failed',
    daily: 'Daily {0}',
    weekly: '{1} {0}',
    deadline: 'Deadline {0}',
    manual: 'Manual',
    digestLabel: 'Weekly digest',
    digestHint: "Sums up the week's daily issues into top stories and trends, without new research.",
    digestWeekday: 'Weekday',
    digestTime: 'Time',
    digestSavedOn: 'Weekly digest {0} {1} saved',
    digestSavedOff: 'Weekly digest turned off',
    undo: 'Undo',
    undone: 'Undone',
    digestCadence: '{0} · week {1}',
  },
};

const ICONS = {
  reply: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 5h16v11H9l-5 4z"/></svg>',
  link: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M10 14a4 4 0 0 0 6 0l3-3a4 4 0 0 0-6-6l-1 1M14 10a4 4 0 0 0-6 0l-3 3a4 4 0 0 0 6 6l1-1"/></svg>',
  flame: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3c1 4 5 5 5 10a5 5 0 0 1-10 0c0-3 2-4 2-6 2 1 3 2 3 4 1-2 0-5 0-8z"/></svg>',
  bookmark: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 3h12v18l-6-4-6 4z"/></svg>',
  share: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3v12M8 7l4-4 4 4M5 13v7h14v-7"/></svg>',
  wand: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 20 15 9M14 4v2M19 9h2M17.5 5.5l1.5-1.5M12 7l5 5"/></svg>',
  key: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="8" cy="15" r="4"/><path d="M11 12l9-9M17 6l3 3"/></svg>',
  trash: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3"/></svg>',
  plus: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 5v14M5 12h14"/></svg>',
  verified: '<svg viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M12 2l2.4 2.1 3.2-.3.9 3.1 2.8 1.6-1.2 3 1.2 3-2.8 1.6-.9 3.1-3.2-.3L12 22l-2.4-2.1-3.2.3-.9-3.1-2.8-1.6 1.2-3-1.2-3 2.8-1.6.9-3.1 3.2.3z"/><path d="M8 12l3 3 5-6" fill="none" stroke="var(--surface)" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>',
};

const state = {
  ctx: null,
  lang: 'de',
  messages: labels.de,
  topics: [],
  sources: [],
  issues: [],
  posts: [],
  selectedTopicId: '',
  selectedIssueId: '',
  leftGrammar: { search: '', band: 'active', filters: {} },
  centerGrammar: { search: '', band: 'all', filters: {} },
  leftView: 'cards',
  centerView: 'cards',
  expandedPosts: new Set(),
  drawer: null,
  renderTimer: 0,
  subscriptions: [],
  disposed: false,
};
const els = {};

export async function mount(ctx) {
  state.ctx = ctx;
  state.disposed = false;
  state.lang = ctx.locale === 'en' ? 'en' : 'de';
  state.messages = await loadModuleMessages(import.meta.url, state.lang, labels);
  ensureStyles();
  ctx.host.innerHTML = await loadModuleMarkup();
  bindElements(ctx.host);
  wireEvents();
  await loadAll();
  subscribeAll();
  render();
  return unmount;
}

function unmount() {
  state.disposed = true;
  clearTimeout(state.renderTimer);
  for (const sub of state.subscriptions) {
    try { sub?.unsubscribe?.(); } catch {}
  }
  state.subscriptions = [];
}

function ensureStyles() {
  // The stylesheet carries the module's own cache-buster so fresh JS never
  // binds against a stale cached sheet.
  const version = String(import.meta.url).split('?v=')[1] || '';
  const href = new URL('./index.css', import.meta.url).pathname + (version ? `?v=${version}` : '');
  if (document.querySelector(`link[href="${href}"]`)) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  document.head.append(link);
}

async function loadModuleMarkup() {
  const version = String(import.meta.url).split('?v=')[1] || '';
  const markupHref = new URL('./index.html', import.meta.url).pathname + (version ? `?v=${version}` : '');
  const html = await fetch(markupHref).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}

function t(key, ...args) {
  const template = state.messages?.[key] ?? labels[state.lang]?.[key] ?? labels.de[key] ?? key;
  return args.reduce((text, value, index) => text.replaceAll(`{${index}}`, String(value)), String(template));
}

function esc(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

function safeHref(url) {
  const value = String(url || '').trim();
  return /^https?:\/\//i.test(value) ? value : '';
}

function bindElements(host) {
  els.host = host;
  els.root = host.querySelector('[data-news-root]');
  els.leftPane = host.querySelector('.news-left');
  els.centerPane = host.querySelector('.news-center');
  els.topicList = host.querySelector('[data-topic-list]');
  els.feed = host.querySelector('[data-feed]');
  els.feedTitle = host.querySelector('[data-feed-title]');
  els.trends = host.querySelector('[data-trends]');
  els.drawer = host.querySelector('[data-topic-drawer]');
  els.scrim = host.querySelector('[data-drawer-scrim]');
  els.form = host.querySelector('[data-topic-form]');
  els.drawerTitle = host.querySelector('[data-drawer-title]');
  els.sourceGroups = host.querySelector('[data-source-groups]');
  els.newSourceKind = host.querySelector('[data-new-source-kind]');
  els.newSourceValue = host.querySelector('[data-new-source-value]');
}

function collection(name) {
  return state.ctx?.db?.collection?.(name) || null;
}

async function readCollection(name) {
  const handle = collection(name);
  if (!handle) return [];
  const docs = await handle.find().exec();
  return docs.map((doc) => (typeof doc?.toJSON === 'function' ? doc.toJSON() : doc)).filter((doc) => doc && !doc._deleted);
}

async function loadAll() {
  const [topics, sources, issues, posts] = await Promise.all([
    readCollection(COLLECTIONS.topics),
    readCollection(COLLECTIONS.sources),
    readCollection(COLLECTIONS.issues),
    readCollection(COLLECTIONS.posts),
  ]);
  state.topics = topics;
  state.sources = sources;
  state.issues = issues;
  state.posts = posts;
  reconcileSelection();
}

function subscribeAll() {
  for (const [key, name] of Object.entries(COLLECTIONS)) {
    const handle = collection(name);
    const sub = handle?.$?.subscribe?.(() => scheduleReload(key));
    if (sub) state.subscriptions.push(sub);
  }
}

const pendingReloads = new Set();
function scheduleReload(key) {
  pendingReloads.add(key);
  clearTimeout(state.renderTimer);
  state.renderTimer = setTimeout(async () => {
    if (state.disposed) return;
    const keys = [...pendingReloads];
    pendingReloads.clear();
    await Promise.all(keys.map(async (k) => { state[k] = await readCollection(COLLECTIONS[k]); }));
    reconcileSelection();
    render();
  }, RENDER_DEBOUNCE_MS);
}

function reconcileSelection() {
  if (!state.topics.some((topic) => topic.id === state.selectedTopicId)) {
    const first = topicsForBand(state.topics, state.leftGrammar)[0] || state.topics[0];
    state.selectedTopicId = first?.id || '';
    state.selectedIssueId = '';
  }
  const issues = issuesForTopic(state.issues, state.selectedTopicId);
  if (!issues.some((issue) => issue.id === state.selectedIssueId)) {
    state.selectedIssueId = issues[0]?.id || '';
  }
}

function selectedTopic() {
  return state.topics.find((topic) => topic.id === state.selectedTopicId) || null;
}

function selectedIssue() {
  return state.issues.find((issue) => issue.id === state.selectedIssueId) || null;
}

function wireEvents() {
  els.leftPane?.addEventListener('ctox-pane-grammar-change', (event) => {
    state.leftGrammar = { ...state.leftGrammar, ...event.detail, view: state.leftView };
    renderLeft();
  });
  els.centerPane?.addEventListener('ctox-pane-grammar-change', (event) => {
    state.centerGrammar = { ...state.centerGrammar, ...event.detail, view: state.centerView };
    renderCenter();
  });
  els.host.querySelectorAll('[data-news-view-toggle]').forEach((button) => {
    button.addEventListener('click', () => toggleView(button));
  });
  els.host.addEventListener('click', onClick);
  els.form?.addEventListener('submit', (event) => { event.preventDefault(); saveTopic(); });
  els.form?.querySelector('[name="cadence"]')?.addEventListener('change', syncCadenceFields);
  // Instant save: every committed field change of a saved topic persists.
  els.form?.addEventListener('change', (event) => {
    if (event.target.closest?.('[data-digest-input]')) { onDigestChange(); return; }
    if (event.target?.dataset?.action === 'toggle-source') return;
    if (event.target === els.newSourceKind || event.target === els.newSourceValue) return;
    if (event.target?.closest?.('[data-digest-undo], [data-panel="digest"]')) return;
    instantSave(t('savedTopic'));
  });
  els.newSourceKind?.addEventListener('change', syncSourcePlaceholder);
  els.newSourceValue?.addEventListener('keydown', (event) => {
    if (event.key === 'Enter') { event.preventDefault(); addDraftSource(); }
  });
}

function toggleView(button) {
  const side = button.dataset.newsViewToggle;
  const next = button.dataset.newsView === 'cards' ? 'list' : 'cards';
  button.dataset.newsView = next;
  const label = next === 'cards' ? t('showAsList') : t('showAsCards');
  button.setAttribute('aria-label', label);
  button.setAttribute('title', label);
  const pane = side === 'left' ? els.leftPane : els.centerPane;
  if (pane) pane.dataset.pgDefaultView = next;
  if (side === 'left') { state.leftView = next; renderLeft(); } else { state.centerView = next; renderCenter(); }
}

function onClick(event) {
  const target = event.target.closest('[data-action], [data-topic-id], [data-issue-id], [data-trend-post]');
  if (!target || !els.host.contains(target)) return;
  const action = target.dataset.action;
  if (!action) {
    if (target.dataset.issueId) return selectIssue(target.dataset.issueTopic, target.dataset.issueId);
    if (target.dataset.topicId) return selectTopic(target.dataset.topicId);
    if (target.dataset.trendPost) return scrollToPost(target.dataset.trendPost);
    return;
  }
  const handlers = {
    'create-topic': () => openDrawer(null),
    'import-topics': importTopics,
    'export-topics': exportTopics,
    'configure-topic': () => openDrawer(selectedTopic()),
    'generate-issue': generateIssue,
    'open-issue-task': () => focusIssueTask(selectedIssue()),
    'mobile-back': () => els.root?.classList.remove('is-detail'),
    'close-drawer': closeDrawer,
    'save-topic': saveTopic,
    'drawer-undo': undoDrawerChange,
    'digest-undo': undoDigest,
    'delete-topic': deleteTopic,
    'drawer-tab': () => setDrawerTab(target.dataset.tab),
    'add-source': addDraftSource,
    'add-lab-preset': () => addPreset('lab'),
    'add-community-preset': () => addPreset('community'),
    'add-reddit-preset': () => addPreset('reddit'),
    'toggle-source': () => toggleDraftSource(target.dataset.sourceId),
    'remove-source': () => removeDraftSource(target.dataset.sourceId),
    'source-adapter': () => runSourceCommand(target.dataset.sourceId, 'adapter'),
    'source-auth': () => runSourceCommand(target.dataset.sourceId, 'auth'),
    'toggle-category': () => {
      target.setAttribute('aria-pressed', String(target.getAttribute('aria-pressed') !== 'true'));
      instantSave(t('savedTopic'));
    },
    'expand-post': () => toggleExpand(target.dataset.postId),
    'bookmark-post': () => toggleBookmark(target.dataset.postId),
    'copy-post-link': () => copyPostLink(target.dataset.postId),
  };
  const handler = handlers[action];
  if (handler) {
    // A checkbox must keep its native toggle; everything else is a button.
    if (target.type !== 'checkbox') event.preventDefault();
    handler();
  }
}

function selectTopic(topicId) {
  state.selectedTopicId = topicId;
  state.selectedIssueId = issuesForTopic(state.issues, topicId)[0]?.id || '';
  state.expandedPosts.clear();
  els.root?.classList.add('is-detail');
  render();
}

function selectIssue(topicId, issueId) {
  state.selectedTopicId = topicId;
  state.selectedIssueId = issueId;
  state.expandedPosts.clear();
  els.root?.classList.add('is-detail');
  render();
}

function render() {
  if (state.disposed) return;
  renderLeft();
  renderCenter();
  renderTrends();
}

function cadenceLabel(topic) {
  const base = baseCadenceLabel(topic);
  if (!digestActive(topic)) return base;
  const digest = digestOf(topic);
  return t('digestCadence', base, `${weekdayShort(digest.weekday)} ${digest.run_time}`);
}

function weekdayShort(weekday) {
  return new Date(2024, 0, 7 + (Number(weekday) || 0)).toLocaleDateString(state.lang === 'en' ? 'en-US' : 'de-DE', { weekday: 'short' }).replace('.', '');
}

function weekdayLong(weekday) {
  return new Date(2024, 0, 7 + (Number(weekday) || 0)).toLocaleDateString(state.lang === 'en' ? 'en-US' : 'de-DE', { weekday: 'long' });
}

function baseCadenceLabel(topic) {
  if (topic.cadence === 'daily') return t('daily', topic.run_time);
  if (topic.cadence === 'weekly') {
    const day = new Date(2024, 0, 7 + (Number(topic.weekday) || 0)).toLocaleDateString(state.lang === 'en' ? 'en-US' : 'de-DE', { weekday: 'short' }).replace('.', '');
    return t('weekly', topic.run_time, day);
  }
  if (topic.cadence === 'deadline') return t('deadline', topic.deadline_date ? issueLabel(topic.deadline_date, state.lang) : '');
  return t('manual');
}

function issueStatusDot(issue) {
  if (ISSUE_ACTIVE_STATUSES.includes(issue.status)) return '<span class="news-dot is-running" aria-hidden="true"></span>';
  if (issue.status === 'failed') return '<span class="news-dot is-failed" aria-hidden="true"></span>';
  return '';
}

function renderLeft() {
  if (!els.topicList) return;
  const counts = {
    active: state.topics.filter((topic) => topic.enabled !== false).length,
    paused: state.topics.filter((topic) => topic.enabled === false).length,
  };
  els.leftPane?.__ctoxPaneGrammar?.setCounts?.(counts);
  setCountsFallback(els.leftPane, counts);
  const topics = topicsForBand(state.topics, state.leftGrammar);
  setFooter(els.leftPane, t('topicsFooter', topics.length));
  if (!state.topics.length) {
    renderHtmlIfChanged(els.topicList, `
      <div class="ctox-empty news-empty">
        <strong>${esc(t('emptyTopicsTitle'))}</strong>
        <span>${esc(t('emptyTopicsBody'))}</span>
        <button class="ctox-button is-primary" type="button" data-action="create-topic">${esc(t('createTopic'))}</button>
      </div>`);
    return;
  }
  if (!topics.length) {
    renderHtmlIfChanged(els.topicList, `<div class="ctox-empty"><span>${esc(t('noTopicsInView'))}</span></div>`);
    return;
  }
  const compact = state.leftView === 'list';
  const html = topics.map((topic) => {
    const selected = topic.id === state.selectedTopicId;
    const issues = selected ? issuesForTopic(state.issues, topic.id) : [];
    const issueRows = issues.map((issue) => {
      const label = issueNodeLabel(issue, issues, state.lang);
      return `
      <button type="button" class="news-issue-node${issue.id === state.selectedIssueId ? ' is-selected' : ''}"
        data-issue-id="${esc(issue.id)}" data-issue-topic="${esc(topic.id)}"
        data-context-record-id="${esc(issue.id)}" data-context-record-type="news_issue"
        data-context-label="${esc(`${topic.title} · ${label}`)}">
        <span>${esc(label)}</span>${issueStatusDot(issue)}
      </button>`;
    }).join('');
    return `
      <div class="news-topic-group${selected ? ' is-open' : ''}">
        <button type="button" class="news-topic${selected ? ' is-selected' : ''}${compact ? ' is-compact' : ''}"
          data-topic-id="${esc(topic.id)}"
          data-context-record-id="${esc(topic.id)}" data-context-record-type="news_topic" data-context-label="${esc(topic.title)}">
          <span class="news-topic-mark" data-tone="${toneBucket(topic.id)}">${esc(monogram(topic.title))}</span>
          <span class="news-topic-text">
            <strong>${esc(topic.title)}</strong>
            ${compact ? '' : `<small>${esc(cadenceLabel(topic))}</small>`}
          </span>
        </button>
        ${issueRows ? `<div class="news-issue-list">${issueRows}</div>` : ''}
      </div>`;
  }).join('');
  renderHtmlIfChanged(els.topicList, html);
}

function setCountsFallback(pane, counts) {
  if (pane?.__ctoxPaneGrammar) return;
  for (const [key, value] of Object.entries(counts)) {
    const node = pane?.querySelector(`[data-pg-count="${key}"]`);
    if (node) node.textContent = ` (${value})`;
  }
}

function setFooter(pane, text) {
  const node = pane?.querySelector('[data-pg-footer]');
  if (node) node.textContent = text;
}

function renderCenter() {
  if (!els.feed) return;
  const topic = selectedTopic();
  const issue = selectedIssue();
  els.feedTitle.textContent = topic ? (issue ? `${topic.title} · ${issueLabel(issue.issue_date, state.lang)}` : topic.title) : t('noTopicSelected');
  setDisabled('configure-topic', !topic);
  setDisabled('generate-issue', !topic || Boolean(issue && ISSUE_ACTIVE_STATUSES.includes(issue.status)));
  const taskButton = els.host.querySelector('[data-action="open-issue-task"]');
  if (taskButton) taskButton.hidden = !(issue && (issue.task_id || issue.command_id) && issue.status !== 'ready');

  const counts = categoryCounts(state.posts, issue?.id);
  els.centerPane?.__ctoxPaneGrammar?.setCounts?.(counts);
  setCountsFallback(els.centerPane, counts);

  if (!topic) {
    setFooter(els.centerPane, '');
    renderHtmlIfChanged(els.feed, '<div class="ctox-empty"></div>');
    return;
  }
  if (!issue) {
    setFooter(els.centerPane, '');
    renderHtmlIfChanged(els.feed, `
      <div class="ctox-empty news-empty">
        <strong>${esc(t('emptyIssueTitle'))}</strong>
        <span>${esc(t('emptyIssueBody'))}</span>
        <button class="ctox-button is-primary" type="button" data-action="generate-issue">${esc(t('generateFirst'))}</button>
      </div>`);
    return;
  }
  const posts = postsForIssue(state.posts, issue.id, state.centerGrammar);
  setFooter(els.centerPane, t('postsFooter', posts.length, issueLabel(issue.issue_date, state.lang)));
  if (!counts.all && ISSUE_ACTIVE_STATUSES.includes(issue.status)) {
    renderHtmlIfChanged(els.feed, `
      <div class="ctox-empty news-empty">
        <span class="news-spinner" aria-hidden="true"></span>
        <strong>${esc(t('issueRunning'))}</strong>
        <span>${esc(t('issueRunningBody'))}</span>
      </div>`);
    return;
  }
  if (!counts.all && issue.status === 'failed') {
    renderHtmlIfChanged(els.feed, `
      <div class="ctox-empty news-empty">
        <strong>${esc(t('issueFailed'))}</strong>
        ${issue.error ? `<span>${esc(issue.error)}</span>` : ''}
        <button class="ctox-button is-primary" type="button" data-action="generate-issue">${esc(t('retry'))}</button>
      </div>`);
    return;
  }
  if (!posts.length) {
    renderHtmlIfChanged(els.feed, `<div class="ctox-empty"><span>${esc(t('noPostsInView'))}</span></div>`);
    return;
  }
  const html = state.centerView === 'list'
    ? `<div class="news-rows">${posts.map(renderPostRow).join('')}</div>`
    : `<div class="news-column">${posts.map(renderPostThread).join('')}</div>`;
  renderHtmlIfChanged(els.feed, html);
}

function setDisabled(action, disabled) {
  const button = els.host.querySelector(`.news-center [data-action="${action}"]`);
  if (button) button.disabled = disabled;
}

function avatar(name, seed) {
  return `<span class="news-avatar" data-tone="${toneBucket(seed || name)}">${esc(monogram(name))}</span>`;
}

function authorHead(post, { editorial = false } = {}) {
  const name = editorial ? t('editorial') : (post.author_name || domainOf(post.sources?.[0]?.url) || 'CTOX');
  const handle = editorial ? '@ctox' : (post.author_handle ? `@${String(post.author_handle).replace(/^@/, '')}` : '');
  const verified = !editorial && post.author_kind === 'lab' ? `<span class="news-verified">${ICONS.verified}</span>` : '';
  const time = editorial ? '' : relativeTime(post.published_at_ms, state.lang);
  return `<div class="news-post-head"><strong>${esc(name)}</strong>${verified}<span>${esc([handle, time].filter(Boolean).join(' · '))}</span></div>`;
}

function renderLinkCard(card) {
  const href = safeHref(card?.url);
  if (!href) return '';
  const image = safeHref(card.image_url);
  return `
    <a class="news-card" href="${esc(href)}" target="_blank" rel="noopener noreferrer">
      ${image ? `<span class="news-card-media"><img src="${esc(image)}" alt="" loading="lazy" referrerpolicy="no-referrer"></span>` : ''}
      <span class="news-card-caption">
        <small>${esc(domainOf(href))}</small>
        ${card.title ? `<span>${esc(card.title)}</span>` : ''}
      </span>
    </a>`;
}

function renderQuote(quote) {
  if (!quote?.text) return '';
  const href = safeHref(quote.url);
  const head = `<div class="news-post-head"><strong>${esc(quote.source || domainOf(href))}</strong>${quote.meta ? `<span>${esc(quote.meta)}</span>` : ''}</div>`;
  const body = `<p>${esc(quote.text)}</p>`;
  return href
    ? `<a class="news-quote" href="${esc(href)}" target="_blank" rel="noopener noreferrer">${head}${body}</a>`
    : `<div class="news-quote">${head}${body}</div>`;
}

function postSources(post) {
  return (Array.isArray(post.sources) ? post.sources : []).filter((source) => safeHref(source?.url));
}

function renderActions(post) {
  const saved = post.bookmarked === true;
  return `
    <div class="news-actions">
      <span title="${esc(t('discussions'))}">${ICONS.reply}${post.discussion_count ? esc(compactCount(post.discussion_count, state.lang)) : ''}</span>
      <span class="news-hype" title="${esc(t('hype'))}">${ICONS.flame}${Math.round(Number(post.hype_score) || 0)}</span>
      <button type="button" class="news-action${saved ? ' is-on' : ''}" data-action="bookmark-post" data-post-id="${esc(post.id)}" aria-pressed="${saved}" aria-label="${esc(saved ? t('saved') : t('save'))}" title="${esc(saved ? t('saved') : t('save'))}">${ICONS.bookmark}</button>
      <button type="button" class="news-action" data-action="copy-post-link" data-post-id="${esc(post.id)}" aria-label="${esc(t('copyLink'))}" title="${esc(t('copyLink'))}">${ICONS.share}</button>
    </div>`;
}

function paragraphs(text) {
  return String(text || '')
    .split(/\n{2,}/)
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => `<p>${esc(part).replaceAll('\n', '<br>')}</p>`)
    .join('');
}

// One news item is one post: intro, optional long form expanded in place,
// the primary source as link card, one quoted discussion voice, and the
// remaining sources as plain domain links (deduplicated, no repeated titles).
function extraSourceDomains(post) {
  const primary = domainOf(safeHref(post.link_card?.url));
  const seen = new Set(primary ? [primary] : []);
  const links = [];
  for (const source of postSources(post)) {
    const domain = domainOf(source.url);
    if (!domain || seen.has(domain)) continue;
    seen.add(domain);
    links.push({ domain, url: safeHref(source.url) });
  }
  return links;
}

function renderPostThread(post) {
  const expanded = state.expandedPosts.has(post.id);
  const hasBody = Boolean(String(post.body || '').trim());
  const domains = extraSourceDomains(post);
  const body = hasBody && expanded
    ? `<div class="news-body">${paragraphs(post.body)}</div>`
    : '';
  const more = hasBody
    ? `<button type="button" class="news-more" data-action="expand-post" data-post-id="${esc(post.id)}">${esc(expanded ? t('showLess') : t('showMore'))}</button>`
    : '';
  const sources = domains.length
    ? `<div class="news-sources">${domains.map((link) => `<a href="${esc(link.url)}" target="_blank" rel="noopener noreferrer">${esc(link.domain)}</a>`).join('<span aria-hidden="true">·</span>')}</div>`
    : '';
  return `
    <article class="news-thread" data-post-id="${esc(post.id)}"
      data-context-record-id="${esc(post.id)}" data-context-record-type="news_post" data-context-label="${esc(post.intro)}">
      <div class="news-post">
        <div class="news-rail">${avatar(post.author_name || 'CTOX', post.author_handle || post.author_name)}</div>
        <div class="news-post-main">
          ${authorHead(post)}
          <p class="news-text">${esc(post.intro)} ${more}</p>
          ${body}
          ${renderLinkCard(post.link_card)}
          ${renderQuote(post.quote)}
          ${sources}
          ${renderActions(post)}
        </div>
      </div>
    </article>`;
}

function renderPostRow(post) {
  return `
    <button type="button" class="news-row" data-trend-post="${esc(post.id)}"
      data-context-record-id="${esc(post.id)}" data-context-record-type="news_post" data-context-label="${esc(post.intro)}">
      ${avatar(post.author_name || 'CTOX', post.author_handle || post.author_name)}
      <span class="news-row-text"><strong>${esc(post.author_name || '')}</strong> ${esc(post.intro)}</span>
      <span class="news-hype">${ICONS.flame}${Math.round(Number(post.hype_score) || 0)}</span>
    </button>`;
}

function renderTrends() {
  if (!els.trends) return;
  const issue = selectedIssue();
  const trends = issue ? trendsForIssue(issue, state.posts) : [];
  if (!trends.length) {
    renderHtmlIfChanged(els.trends, '');
    return;
  }
  renderHtmlIfChanged(els.trends, `<div class="news-trends">${trends.map((trend) => `
    <button type="button" class="news-trend" ${trend.post_id ? `data-trend-post="${esc(trend.post_id)}"` : ''}
      data-context-record-id="${esc(trend.post_id || trend.title)}" data-context-record-type="news_trend" data-context-label="${esc(trend.title)}">
      <strong>${esc(trend.title)}</strong>
      ${trend.discussion_count ? `<small>${esc(compactCount(trend.discussion_count, state.lang))}</small>` : ''}
    </button>`).join('')}</div>`);
}

function scrollToPost(postId) {
  if (state.centerView === 'list') {
    state.centerView = 'cards';
    const toggle = els.host.querySelector('[data-news-view-toggle="center"]');
    if (toggle) { toggle.dataset.newsView = 'cards'; toggle.setAttribute('aria-label', t('showAsList')); toggle.setAttribute('title', t('showAsList')); }
    renderCenter();
  }
  const node = els.feed?.querySelector(`.news-thread[data-post-id="${CSS.escape(postId)}"]`);
  node?.scrollIntoView({ block: 'start', behavior: 'smooth' });
  node?.classList.add('is-flash');
  setTimeout(() => node?.classList.remove('is-flash'), 1200);
}

function toggleExpand(postId) {
  if (state.expandedPosts.has(postId)) state.expandedPosts.delete(postId);
  else state.expandedPosts.add(postId);
  renderCenter();
}

async function toggleBookmark(postId) {
  const handle = collection(COLLECTIONS.posts);
  const doc = await handle?.findOne(postId).exec();
  if (!doc) return;
  try {
    await doc.incrementalPatch({ bookmarked: doc.bookmarked !== true, updated_at_ms: Date.now() });
  } catch (error) {
    notify('error', t('saveFailed'), error?.message);
  }
}

async function copyPostLink(postId) {
  const post = state.posts.find((item) => item.id === postId);
  const href = safeHref(post?.link_card?.url) || safeHref(postSources(post || {})[0]?.url);
  if (!href) return;
  try {
    await navigator.clipboard.writeText(href);
    notify('success', t('linkCopied'));
  } catch {}
}

function notify(type, title, message = '') {
  try {
    state.ctx?.notifications?.show?.({ type, title, message });
  } catch {}
}

// --- Commands ------------------------------------------------------------

async function dispatch(command) {
  if (typeof state.ctx?.commandBus?.dispatch !== 'function') throw new Error(t('commandFailed'));
  return state.ctx.commandBus.dispatch(command);
}

async function generateIssue() {
  const topic = selectedTopic();
  if (!topic) return;
  const issuesHandle = collection(COLLECTIONS.issues);
  if (!issuesHandle) return notify('error', t('commandFailed'));
  const now = Date.now();
  const previous = issuesForTopic(state.issues, topic.id).find((issue) => issue.status === 'ready') || null;
  const issue = {
    id: newId('issue'),
    topic_id: topic.id,
    title: topic.title,
    issue_date: issueDateKey(new Date(now)),
    status: 'queued',
    command_id: '',
    task_id: '',
    post_count: 0,
    generated_at_ms: 0,
    error: '',
    payload: { trigger: 'manual' },
    created_at_ms: now,
    updated_at_ms: now,
  };
  const freshness = issueWindow(topic, state.issues, state.posts, now);
  const command = buildGenerateCommand(topic, state.sources, issue, previous, freshness);
  issue.command_id = command.command_id;
  try {
    await issuesHandle.insert(issue);
    state.selectedIssueId = issue.id;
      await dispatch(command);
    notify('success', t('commandQueued'));
  } catch (error) {
    const doc = await issuesHandle.findOne(issue.id).exec().catch(() => null);
    await doc?.incrementalPatch({ status: 'failed', error: String(error?.message || error), updated_at_ms: Date.now() }).catch(() => {});
    notify('error', t('commandFailed'), error?.message);
  }
}

function focusIssueTask(issue) {
  if (!issue) return;
  const taskId = issue.task_id || '';
  const commandId = issue.command_id || '';
  if (!taskId && !commandId) return;
  try {
    sessionStorage.setItem('ctox.businessOs.focusTask', JSON.stringify({
      taskId, commandId, taskStatus: issue.status || '', sourceModule: 'news', openDrawer: true,
    }));
  } catch {}
  const params = new URLSearchParams();
  if (taskId) params.set('task_id', taskId);
  if (commandId) params.set('command_id', commandId);
  params.set('source', 'news');
  params.set('drawer', '1');
  location.hash = `#ctox?${params.toString()}`;
}

async function runSourceCommand(sourceId, kind) {
  const topic = state.drawer?.topic;
  const source = state.drawer?.sources.find((item) => item.id === sourceId);
  if (!topic || !source) return;
  if (!state.drawer.persisted || !state.sources.some((item) => item.id === sourceId)) {
    notify('error', t('saveTopicFirst'));
    return;
  }
  const command = kind === 'auth' ? buildAuthCommand(topic, source) : buildAdapterCommand(topic, source);
  try {
    const handle = collection(COLLECTIONS.sources);
    const doc = await handle?.findOne(sourceId).exec();
    await doc?.incrementalPatch({ status: 'pending', last_error: '', payload: { ...(doc.payload || {}), last_command_id: command.command_id }, updated_at_ms: Date.now() });
    source.status = 'pending';
    renderSourceGroups();
    await dispatch(command);
    notify('success', t('commandQueued'));
  } catch (error) {
    notify('error', t('commandFailed'), error?.message);
  }
}

// --- Topic drawer --------------------------------------------------------

function openDrawer(topic) {
  const persisted = Boolean(topic);
  const draft = persisted ? { ...topic } : normalizeTopic({ title: '', cadence: 'daily', run_time: '07:00' });
  state.drawer = {
    topic: draft,
    persisted,
    sources: state.sources.filter((source) => source.topic_id === draft.id).map((source) => ({ ...source })),
    removed: new Set(),
    tab: 'topic',
  };
  const migrated = migrateLegacyRedditSources(state.drawer.sources, draft.id);
  state.drawer.sources = migrated.sources;
  migrated.removedIds.forEach((id) => state.drawer.removed.add(id));
  state.drawer.saved = drawerSnapshot();
  els.drawerTitle.textContent = persisted ? draft.title : t('newTopic');
  syncDrawerFoot();
  hideDrawerUndo();
  fillDrawerForm(draft);
  const deleteButton = els.host.querySelector('[data-action="delete-topic"]');
  deleteButton.hidden = !persisted;
  deleteButton.classList.remove('is-danger');
  deleteButton.setAttribute('title', t('deleteTopic'));
  deleteButton.setAttribute('aria-label', t('deleteTopic'));
  renderDigestField();
  syncCadenceFields();
  syncSourcePlaceholder();
  setDrawerTab('topic');
  renderSourceGroups();
  els.scrim.hidden = false;
  els.drawer.hidden = false;
  form.elements.title.focus();
}

function fillDrawerForm(draft) {
  const form = els.form;
  form.elements.title.value = draft.title || '';
  form.elements.prompt.value = draft.prompt || '';
  form.elements.cadence.value = draft.cadence || 'daily';
  form.elements.run_time.value = draft.run_time || '07:00';
  form.elements.weekday.value = String(draft.weekday ?? 1);
  form.elements.deadline_date.value = draft.deadline_date || '';
  form.elements.enabled.checked = draft.enabled !== false;
  form.elements.language.value = draft.language || 'de';
  form.elements.max_posts.value = String(draft.max_posts || 12);
  els.drawer.querySelectorAll('[data-category]').forEach((chip) => {
    chip.setAttribute('aria-pressed', String((draft.categories || CATEGORIES).includes(chip.dataset.category)));
  });
  syncCadenceFields();
}

function closeDrawer() {
  hideDrawerUndo();
  state.drawer = null;
  if (els.drawer) els.drawer.hidden = true;
  if (els.scrim) els.scrim.hidden = true;
}

function setDrawerTab(tab) {
  if (!state.drawer) return;
  state.drawer.tab = tab;
  els.drawer.querySelectorAll('[data-action="drawer-tab"]').forEach((button) => {
    const active = button.dataset.tab === tab;
    button.setAttribute('aria-selected', String(active));
    button.classList.toggle('is-active', active);
  });
  els.drawer.querySelectorAll('[data-panel]').forEach((panel) => { panel.hidden = panel.dataset.panel !== tab; });
}

function syncCadenceFields() {
  const cadence = els.form?.elements.cadence.value;
  const visible = {
    time: cadence !== 'manual',
    weekday: cadence === 'weekly',
    deadline: cadence === 'deadline',
  };
  els.form?.querySelectorAll('[data-cadence-field]').forEach((field) => { field.hidden = !visible[field.dataset.cadenceField]; });
  const digestField = els.form?.querySelector('[data-digest-field]');
  if (digestField) digestField.hidden = cadence !== 'daily';
}

// --- Weekly digest (instant save + undo) ----------------------------------

const DIGEST_UNDO_MS = 8000;

function renderDigestField() {
  const host = els.form?.querySelector('[data-digest-field]');
  if (!host || !state.drawer) return;
  const digest = digestOf(state.drawer.topic);
  const days = [1, 2, 3, 4, 5, 6, 0]
    .map((day) => `<option value="${day}"${day === digest.weekday ? ' selected' : ''}>${esc(weekdayLong(day))}</option>`)
    .join('');
  host.innerHTML = `
    <label class="news-check"><input type="checkbox" data-digest-input="enabled"${digest.enabled ? ' checked' : ''} /> <span>${esc(t('digestLabel'))}</span></label>
    <p class="news-digest-hint">${esc(t('digestHint'))}</p>
    <div class="news-field-grid" data-digest-schedule${digest.enabled ? '' : ' hidden'}>
      <div>
        <label class="ctox-field-label" for="news-digest-weekday">${esc(t('digestWeekday'))}</label>
        <select class="ctox-select" id="news-digest-weekday" data-digest-input="weekday">${days}</select>
      </div>
      <div>
        <label class="ctox-field-label" for="news-digest-time">${esc(t('digestTime'))}</label>
        <input class="ctox-input" id="news-digest-time" type="time" data-digest-input="run_time" value="${esc(digest.run_time)}" />
      </div>
    </div>
    <div class="news-undo" data-digest-undo hidden>
      <span data-digest-undo-text></span>
      <button class="ctox-button" type="button" data-action="digest-undo">${esc(t('undo'))}</button>
    </div>`;
}

function readDigestField() {
  const host = els.form?.querySelector('[data-digest-field]');
  return {
    enabled: Boolean(host?.querySelector('[data-digest-input="enabled"]')?.checked),
    weekday: Number(host?.querySelector('[data-digest-input="weekday"]')?.value ?? 5),
    run_time: host?.querySelector('[data-digest-input="run_time"]')?.value || '17:00',
  };
}

async function onDigestChange() {
  if (!state.drawer) return;
  const previous = digestOf(state.drawer.topic);
  const next = readDigestField();
  if (JSON.stringify(previous) === JSON.stringify(digestOf({ payload: { digest: next } }))) return;
  const saved = await applyDigest(next);
  if (!saved) return;
  showDigestUndo(previous, next);
}

// Persisted topics save at once; a new topic keeps it in the draft until
// the topic itself is created.
async function applyDigest(digest) {
  const drawer = state.drawer;
  if (!drawer) return false;
  drawer.topic = withDigest(drawer.topic, digest);
  if (drawer.persisted) {
    const topics = collection(COLLECTIONS.topics);
    const stored = state.topics.find((item) => item.id === drawer.topic.id);
    if (!topics || !stored) { notify('error', t('saveFailed')); return false; }
    const topic = withDigest(stored, digest);
    try {
      await topics.upsert(topic);
    } catch (error) {
      notify('error', t('saveFailed'), error?.message);
      return false;
    }
    dispatch(buildScheduleCommand(topic, { digest: true })).catch((error) => notify('error', t('commandFailed'), error?.message));
  }
  const schedule = els.form?.querySelector('[data-digest-schedule]');
  if (schedule) schedule.hidden = !digestOf(drawer.topic).enabled;
  return true;
}

function showDigestUndo(previous, next) {
  const bar = els.form?.querySelector('[data-digest-undo]');
  if (!bar || !state.drawer) return;
  const text = next.enabled
    ? t('digestSavedOn', weekdayLong(next.weekday), next.run_time)
    : t('digestSavedOff');
  bar.querySelector('[data-digest-undo-text]').textContent = text;
  bar.hidden = false;
  state.drawer.digestUndo = previous;
  clearTimeout(state.digestUndoTimer);
  state.digestUndoTimer = setTimeout(() => {
    bar.hidden = true;
    if (state.drawer) state.drawer.digestUndo = null;
  }, DIGEST_UNDO_MS);
}

async function undoDigest() {
  const previous = state.drawer?.digestUndo;
  if (!previous) return;
  clearTimeout(state.digestUndoTimer);
  state.drawer.digestUndo = null;
  if (await applyDigest(previous)) {
    renderDigestField();
    notify('success', t('undone'));
  }
}

function syncSourcePlaceholder() {
  const kind = els.newSourceKind?.value || 'lab';
  const key = { lab: 'placeholderLab', x: 'placeholderX', reddit: 'placeholderReddit', youtube: 'placeholderYoutube', newsletter: 'placeholderNewsletter', community: 'placeholderCommunity' }[kind];
  if (els.newSourceValue) els.newSourceValue.placeholder = t(key);
}

function sourceStatusPill(source) {
  const map = { needs_auth: 'statusNeedsAuth', auth_requested: 'statusAuthRequested', failed: 'statusFailed', repairing: 'statusRepairing', pending: 'statusPending' };
  const key = map[source.status];
  if (!key) return '';
  const tone = source.status === 'failed' ? 'is-danger' : source.status === 'pending' || source.status === 'auth_requested' ? '' : 'is-warning';
  return `<span class="news-pill ${tone}" title="${esc(source.last_error || '')}">${esc(t(key))}</span>`;
}

function renderSourceGroups() {
  if (!els.sourceGroups || !state.drawer) return;
  const groups = [
    ['lab', 'groupLab', 'add-lab-preset', 'addLabPreset'],
    ['x', 'groupX', '', ''],
    ['reddit', 'groupReddit', 'add-reddit-preset', 'addRedditPreset'],
    ['youtube', 'groupYoutube', '', ''],
    ['newsletter', 'groupNewsletter', '', ''],
    ['community', 'groupCommunity', 'add-community-preset', 'addCommunityPreset'],
  ];
  els.sourceGroups.innerHTML = groups.map(([kind, labelKey, presetAction, presetLabel]) => {
    const sources = state.drawer.sources.filter((source) => source.kind === kind);
    const preset = presetAction
      ? `<button class="ctox-pane-icon" type="button" data-action="${presetAction}" aria-label="${esc(t(presetLabel))}" title="${esc(t(presetLabel))}">${ICONS.plus}</button>`
      : '';
    if (!sources.length && !preset) return '';
    return `
      <section class="news-source-group">
        <header><strong>${esc(t(labelKey))}</strong>${preset}</header>
        ${sources.map(renderSourceRow).join('')}
      </section>`;
  }).join('');
}

function renderSourceRow(source) {
  const items = Array.isArray(source.items) ? source.items : [];
  const actions = [];
  if (source.kind === 'lab' || source.kind === 'community') {
    actions.push(`<button class="ctox-pane-icon" type="button" data-action="source-adapter" data-source-id="${esc(source.id)}" aria-label="${esc(t('sourceAdapter'))}" title="${esc(t('sourceAdapter'))}">${ICONS.wand}</button>`);
  }
  if (authSourceFor(source)) {
    actions.push(`<button class="ctox-pane-icon" type="button" data-action="source-adapter" data-source-id="${esc(source.id)}" aria-label="${esc(t('sourceConnect'))}" title="${esc(t('sourceConnect'))}">${ICONS.wand}</button>`);
    actions.push(`<button class="ctox-pane-icon" type="button" data-action="source-auth" data-source-id="${esc(source.id)}" aria-label="${esc(t('sourceLogin'))}" title="${esc(t('sourceLogin'))}">${ICONS.key}</button>`);
  }
  actions.push(`<button class="ctox-pane-icon" type="button" data-action="remove-source" data-source-id="${esc(source.id)}" aria-label="${esc(t('sourceRemove'))}" title="${esc(t('sourceRemove'))}">${ICONS.trash}</button>`);
  return `
    <div class="news-source-row" data-context-record-id="${esc(source.id)}" data-context-record-type="news_source" data-context-label="${esc(source.label)}">
      <input type="checkbox" data-action="toggle-source" data-source-id="${esc(source.id)}" ${source.enabled !== false ? 'checked' : ''} aria-label="${esc(t('sourceActive'))}" />
      <span class="news-topic-mark" data-tone="${toneBucket(source.label)}">${esc(monogram(source.label))}</span>
      <span class="news-source-main" title="${esc(source.url || '')}">
        <strong>${esc(source.label)}</strong>${sourceStatusPill(source)}
        ${items.length ? `<span class="news-source-items">${items.map((item) => `<span class="ctox-chip">${esc(item)}</span>`).join('')}</span>` : ''}
      </span>
      <span class="news-source-actions">${actions.join('')}</span>
    </div>`;
}

function addDraftSource() {
  if (!state.drawer) return;
  const kind = SOURCE_KINDS.includes(els.newSourceKind?.value) ? els.newSourceKind.value : 'lab';
  const value = String(els.newSourceValue?.value || '').trim();
  if (!value && kind !== 'x') return;
  const merged = mergeIntoAccountSource(state.drawer.sources, state.drawer.topic.id, kind, value);
  if (merged) {
    state.drawer.sources = state.drawer.sources.map((source) => (source.id === merged.id ? merged : source));
  } else {
    const created = newSource(state.drawer.topic.id, kind, '', value || 'Home');
    if (kind === 'reddit' && !created.items.length) return;
    state.drawer.sources.push(created);
  }
  els.newSourceValue.value = '';
  renderSourceGroups();
  instantSave(t('savedSourcesAdded'));
}

function addPreset(kind) {
  if (!state.drawer) return;
  if (kind === 'reddit') {
    const merged = redditPresetSource(state.drawer.sources, state.drawer.topic.id);
    state.drawer.sources = [...state.drawer.sources.filter((source) => source.id !== merged.id), merged];
    renderSourceGroups();
    instantSave(t('savedSourcesAdded'));
    return;
  }
  const preset = kind === 'lab' ? LAB_PRESET : COMMUNITY_PRESET;
  state.drawer.sources.push(...presetSources(state.drawer.topic.id, preset, kind, state.drawer.sources));
  renderSourceGroups();
  instantSave(t('savedSourcesAdded'));
}

function toggleDraftSource(sourceId) {
  const source = state.drawer?.sources.find((item) => item.id === sourceId);
  if (!source) return;
  source.enabled = source.enabled === false;
  instantSave(t(source.enabled ? 'savedSourceOn' : 'savedSourceOff', source.label));
}

function removeDraftSource(sourceId) {
  if (!state.drawer) return;
  const label = state.drawer.sources.find((source) => source.id === sourceId)?.label || '';
  state.drawer.sources = state.drawer.sources.filter((source) => source.id !== sourceId);
  state.drawer.removed.add(sourceId);
  renderSourceGroups();
  instantSave(t('savedSourceRemoved', label));
}

function readTopicForm() {
  const form = els.form.elements;
  const categories = [...els.drawer.querySelectorAll('[data-category]')]
    .filter((chip) => chip.getAttribute('aria-pressed') === 'true')
    .map((chip) => chip.dataset.category);
  return normalizeTopic({
    ...state.drawer.topic,
    title: form.title.value,
    prompt: form.prompt.value,
    cadence: form.cadence.value,
    run_time: form.run_time.value,
    weekday: Number(form.weekday.value),
    deadline_date: form.deadline_date.value,
    enabled: form.enabled.checked,
    language: form.language.value,
    max_posts: Number(form.max_posts.value),
    categories,
  });
}

async function saveTopic() {
  if (!state.drawer) return;
  const topic = readTopicForm();
  const problem = validateTopic(topic);
  if (problem) {
    setDrawerTab('topic');
    notify('error', t(problem));
    els.form.elements.title.focus();
    return;
  }
  if (!(await persistDrawer(topic))) return;
  closeDrawer();
  state.selectedTopicId = topic.id;
}

// Write the drawer state (topic + sources) and re-dispatch the schedule when
// it changed. Shared by "Thema anlegen", instant save and undo.
async function persistDrawer(topic, sourceList = state.drawer?.sources || [], removed = state.drawer?.removed || new Set()) {
  const topics = collection(COLLECTIONS.topics);
  const sources = collection(COLLECTIONS.sources);
  if (!topics || !sources) {
    notify('error', t('saveFailed'));
    return false;
  }
  const now = Date.now();
  const previous = state.topics.find((item) => item.id === topic.id);
  try {
    await topics.upsert(topic);
    for (const source of sourceList) {
      await sources.upsert({ ...source, topic_id: topic.id, updated_at_ms: now });
    }
    for (const sourceId of removed) {
      const doc = await sources.findOne(sourceId).exec();
      await doc?.remove();
    }
  } catch (error) {
    notify('error', t('saveFailed'), error?.message);
    return false;
  }
  const scheduleChanged = !previous
    || previous.cron_expr !== topic.cron_expr
    || previous.enabled !== topic.enabled
    || previous.title !== topic.title;
  if (scheduleChanged) {
    dispatch(buildScheduleCommand(topic)).catch((error) => notify('error', t('commandFailed'), error?.message));
  }
  // The digest follows the topic: only a daily, active topic keeps it.
  if (digestActive(topic) !== Boolean(previous && digestActive(previous)) || (digestActive(topic) && !previous)) {
    dispatch(buildScheduleCommand(topic, { digest: true })).catch((error) => notify('error', t('commandFailed'), error?.message));
  }
  return true;
}

// --- Instant save + undo (8 s), same pattern as the weekly digest ----------

const DRAWER_UNDO_MS = 8000;

function syncDrawerFoot() {
  const foot = els.drawer?.querySelector('[data-drawer-create-foot]');
  if (!foot) return;
  foot.hidden = Boolean(state.drawer?.persisted);
  const create = foot.querySelector('[data-action="save-topic"]');
  if (create) create.textContent = t('createTopic');
  const undo = els.drawer?.querySelector('[data-action="drawer-undo"]');
  if (undo) undo.textContent = t('drawerUndo');
}

function drawerSnapshot() {
  return {
    topic: { ...state.drawer.topic },
    sources: state.drawer.sources.map((source) => ({ ...source })),
  };
}

async function instantSave(message) {
  const drawer = state.drawer;
  if (!drawer?.persisted) return; // a new topic is created once, with its name
  const topic = readTopicForm();
  if (validateTopic(topic)) return; // keep editing; nothing half-valid is stored
  const before = drawer.saved || drawerSnapshot();
  const removed = new Set(drawer.removed);
  if (!(await persistDrawer(topic, drawer.sources, removed))) return;
  if (state.drawer !== drawer) return;
  drawer.removed.clear();
  drawer.topic = topic;
  drawer.saved = drawerSnapshot();
  els.drawerTitle.textContent = topic.title;
  showDrawerUndo(message, before);
}

function showDrawerUndo(message, before) {
  const bar = els.drawer?.querySelector('[data-drawer-undo]');
  if (!bar || !state.drawer) return;
  bar.querySelector('[data-drawer-undo-text]').textContent = message;
  bar.querySelector('[data-action="drawer-undo"]').hidden = false;
  bar.hidden = false;
  // Undo returns to the state before the first change of this burst.
  if (!state.drawer.undo) state.drawer.undo = before;
  clearTimeout(state.drawerUndoTimer);
  state.drawerUndoTimer = setTimeout(hideDrawerUndo, DRAWER_UNDO_MS);
}

function hideDrawerUndo() {
  clearTimeout(state.drawerUndoTimer);
  const bar = els.drawer?.querySelector('[data-drawer-undo]');
  if (bar) bar.hidden = true;
  if (state.drawer) state.drawer.undo = null;
}

async function undoDrawerChange() {
  const drawer = state.drawer;
  const snapshot = drawer?.undo;
  if (!snapshot) return;
  const keep = new Set(snapshot.sources.map((source) => source.id));
  const added = new Set(drawer.sources.filter((source) => !keep.has(source.id)).map((source) => source.id));
  if (!(await persistDrawer(snapshot.topic, snapshot.sources, added))) return;
  if (state.drawer !== drawer) return;
  clearTimeout(state.drawerUndoTimer);
  drawer.topic = { ...snapshot.topic };
  drawer.sources = snapshot.sources.map((source) => ({ ...source }));
  drawer.saved = drawerSnapshot();
  drawer.undo = null;
  drawer.removed.clear();
  fillDrawerForm(drawer.topic);
  renderSourceGroups();
  const bar = els.drawer?.querySelector('[data-drawer-undo]');
  if (bar) {
    bar.querySelector('[data-drawer-undo-text]').textContent = t('drawerUndone');
    bar.querySelector('[data-action="drawer-undo"]').hidden = true;
    state.drawerUndoTimer = setTimeout(hideDrawerUndo, 2500);
  }
}

async function deleteTopic() {
  const topic = state.drawer?.topic;
  if (!topic || !state.drawer.persisted) return;
  const button = els.host.querySelector('[data-action="delete-topic"]');
  // Two-step confirm on the button itself: no shell-level dialog.
  if (!state.drawer.confirmDelete) {
    state.drawer.confirmDelete = true;
    button?.classList.add('is-danger');
    button?.setAttribute('title', t('deleteConfirm', topic.title));
    button?.setAttribute('aria-label', t('deleteConfirm', topic.title));
    return;
  }
  try {
    await dispatch(buildScheduleCommand(topic, { deleted: true }));
    if (digestOf(topic).enabled) await dispatch(buildScheduleCommand(topic, { deleted: true, digest: true }));
  } catch (error) {
    notify('error', t('commandFailed'), error?.message);
    return;
  }
  const removeWhere = async (name, predicate) => {
    const handle = collection(name);
    if (!handle) return;
    const docs = await handle.find().exec();
    for (const doc of docs) {
      if (predicate(doc)) await doc.remove();
    }
  };
  await removeWhere(COLLECTIONS.posts, (doc) => doc.topic_id === topic.id);
  await removeWhere(COLLECTIONS.issues, (doc) => doc.topic_id === topic.id);
  await removeWhere(COLLECTIONS.sources, (doc) => doc.topic_id === topic.id);
  await removeWhere(COLLECTIONS.topics, (doc) => doc.id === topic.id);
  closeDrawer();
}

// --- Import / export -------------------------------------------------------

function exportTopics() {
  const data = {
    format: 'ctox.news.topics.v1',
    topics: state.topics,
    sources: state.sources,
  };
  const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
  const link = document.createElement('a');
  link.href = URL.createObjectURL(blob);
  link.download = `news-topics-${issueDateKey()}.json`;
  link.click();
  setTimeout(() => URL.revokeObjectURL(link.href), 1000);
}

function importTopics() {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = 'application/json,.json';
  input.addEventListener('change', async () => {
    const file = input.files?.[0];
    if (!file) return;
    try {
      const data = JSON.parse(await file.text());
      const topics = collection(COLLECTIONS.topics);
      const sources = collection(COLLECTIONS.sources);
      const idMap = new Map();
      const now = Date.now();
      for (const raw of Array.isArray(data.topics) ? data.topics : []) {
        const topic = normalizeTopic({ ...raw, id: newId('topic'), schedule_task_id: '' }, now);
        idMap.set(raw.id, topic.id);
        await topics.upsert(topic);
        dispatch(buildScheduleCommand(topic)).catch(() => {});
        if (digestActive(topic)) dispatch(buildScheduleCommand(topic, { digest: true })).catch(() => {});
      }
      for (const raw of Array.isArray(data.sources) ? data.sources : []) {
        const topicId = idMap.get(raw.topic_id);
        if (!topicId) continue;
        await sources.upsert({ ...raw, id: newId('source'), topic_id: topicId, status: authSourceFor(raw) ? 'needs_auth' : 'new', created_at_ms: now, updated_at_ms: now });
      }
      notify('success', t('importDone', idMap.size));
    } catch (error) {
      notify('error', t('importFailed'), error?.message);
    }
  });
  input.click();
}
