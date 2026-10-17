import { renderHtmlIfChanged } from 'ctox:shared/stable-dom.js';
import { autoWirePaneGrammar } from 'ctox:shared/pane-grammar.js';
import { applyPaneIconLabels } from 'ctox:shared/pane-icon-labels.js';
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
  buildScheduleCommand,
  categoryCounts,
  digestActive,
  digestOf,
  withDigest,
  compactCount,
  domainOf,
  issueDateKey,
  issueLabel,
  issueNodeLabel,
  issueWindow,
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
    title: 'News',
    topicsTitle: 'Themen',
    trendTitle: 'Im Trend',
    createTopic: 'Thema anlegen',
    issueFailed: 'Ausgabe fehlgeschlagen',
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
    digestLabel: 'Wochenzusammenfassung',
    digestHint: 'Fasst die Tagesausgaben der Woche zu Top-Meldungen und Trends zusammen, ohne neue Recherche.',
    digestWeekday: 'Wochentag',
    digestTime: 'Uhrzeit',
    digestCadence: '{0} · Woche {1}',
    commandFailed: 'Auftrag konnte nicht an CTOX übergeben werden',
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
  },
  en: {
    title: 'News',
    topicsTitle: 'Topics',
    trendTitle: 'Trending',
    createTopic: 'Create topic',
    issueFailed: 'Issue failed',
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
    digestLabel: 'Weekly digest',
    digestHint: "Sums up the week's daily issues into top stories and trends, without new research.",
    digestWeekday: 'Weekday',
    digestTime: 'Time',
    digestCadence: '{0} · week {1}',
    commandFailed: 'Could not hand the task to CTOX',
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
  api: null,
  root: null,
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
  disposed: false,
  commandWatches: new Map(),
  drawerUndoTimer: 0,
};
const els = {};

function langFrom(locale) {
  return String(locale || '').toLowerCase().startsWith('en') ? 'en' : 'de';
}

// Eingebettete labels sind der Rückfall; api.shell.t('news.<key>') übersteuert (Schlüsselraum offen, OFFEN-B).
function messagesFor(api, lang) {
  const base = { ...labels.de, ...labels[lang] };
  for (const key of Object.keys(base)) {
    const scoped = `news.${key}`;
    const value = api?.shell?.t?.(scoped);
    if (typeof value === 'string' && value && value !== scoped) base[key] = value;
  }
  return base;
}

function resetState() {
  Object.assign(state, {
    topics: [], sources: [], issues: [], posts: [], selectedTopicId: '', selectedIssueId: '',
    leftGrammar: { search: '', band: 'active', filters: {} }, centerGrammar: { search: '', band: 'all', filters: {} },
    leftView: 'cards', centerView: 'cards', expandedPosts: new Set(), drawer: null, renderTimer: 0, disposed: false,
    commandWatches: new Map(), drawerUndoTimer: 0,
  });
  for (const key of Object.keys(els)) delete els[key];
}

export async function mount(root, api, { restored } = {}) {
  resetState();
  // Abos gehören dieser Montage (PORTING.md §4 Falle 1).
  const disposers = [];
  const watches = state.commandWatches;
  state.api = api;
  state.root = root;
  if (!root.dataset.app) root.dataset.app = api.app?.id || 'news';
  state.lang = langFrom(api.shell.locale.get());
  state.messages = messagesFor(api, state.lang);
  // CSS lädt die Shell aus dem Manifest; index.html liefert der Service Worker versioniert (§16.15).
  root.innerHTML = await loadModuleMarkup();
  bindElements(root);
  wireEvents();
  autoWirePaneGrammar(root);
  // Icon + Wort (workjet#801): Kurzwörter der Kopfknöpfe; der Rahmen blendet sie aus, wo der Kopf zu eng ist.
  disposers.push(applyPaneIconLabels(root, state.lang).dispose);
  api.shell.setTitle(t('title'));
  const pendingRestore = restored && typeof restored === 'object' && restored.v === 1 ? restored : null;
  applyRestoredView(pendingRestore);
  let drawerRestore = pendingRestore?.drawer || null;
  // Live-Abos statt RxDB-Einmal-Lesen + Observable + Neuladen.
  for (const [key, name] of Object.entries(COLLECTIONS)) {
    disposers.push(api.query(name, {}).subscribe((rows) => {
      state[key] = Array.isArray(rows) ? rows : [];
      scheduleRender();
      if (drawerRestore && restoreDrawer(drawerRestore)) drawerRestore = null;
    }));
  }
  disposers.push(api.shell.locale.subscribe((locale) => {
    const lang = langFrom(locale);
    if (lang === state.lang) return;
    state.lang = lang;
    state.messages = messagesFor(api, lang);
    applyPaneIconLabels(root, lang);
    scheduleRender();
  }));
  return {
    unmount() {
      clearTimeout(state.renderTimer);
      clearTimeout(state.drawerUndoTimer);
      for (const off of disposers.splice(0)) {
        try { off(); } catch (error) { console.warn('[news] unsubscribe failed', error); }
      }
      for (const off of watches.values()) { try { off(); } catch {} }
      watches.clear();
      root.replaceChildren();
      if (state.root === root) {
        state.disposed = true;
        state.api = null;
        state.root = null;
      }
    },
    snapshot: newsSnapshot,
  };
}

function scheduleRender() {
  clearTimeout(state.renderTimer);
  state.renderTimer = setTimeout(() => {
    if (state.disposed || !state.api) return;
    reconcileSelection();
    render();
  }, RENDER_DEBOUNCE_MS);
}

// Snapshot für den Live-Remount (§10.2a Nr. 1): Auswahl, Ansichten, Grammatik und offener Themen-Drawer samt
// ungespeicherter Eingaben und Quellen-Entwürfe.
function newsSnapshot() {
  const snap = {
    v: 1,
    selectedTopicId: state.selectedTopicId,
    selectedIssueId: state.selectedIssueId,
    leftGrammar: structuredClone(state.leftGrammar),
    centerGrammar: structuredClone(state.centerGrammar),
    leftView: state.leftView,
    centerView: state.centerView,
    expandedPosts: [...state.expandedPosts],
    feedScrollTop: els.feed?.scrollTop || 0,
  };
  if (state.drawer && els.drawer && !els.drawer.hidden) {
    const values = {};
    for (const el of els.form?.elements || []) {
      if (!el.name) continue;
      if (el.type === 'checkbox') { values[el.name] = values[el.name] || []; if (el.checked) values[el.name].push(el.value); }
      else if (el.type === 'radio') { if (el.checked) values[el.name] = el.value; }
      else values[el.name] = el.value;
    }
    snap.drawer = {
      persisted: state.drawer.persisted,
      topicId: state.drawer.topic?.id || '',
      tab: state.drawer.tab,
      sources: structuredClone(state.drawer.sources),
      removed: [...state.drawer.removed],
      digest: digestOf(state.drawer.topic),
      values,
    };
  }
  return snap;
}

function applyRestoredView(restored) {
  if (!restored) return;
  if (typeof restored.selectedTopicId === 'string') state.selectedTopicId = restored.selectedTopicId;
  if (typeof restored.selectedIssueId === 'string') state.selectedIssueId = restored.selectedIssueId;
  if (restored.leftGrammar && typeof restored.leftGrammar === 'object') state.leftGrammar = { ...state.leftGrammar, ...restored.leftGrammar };
  if (restored.centerGrammar && typeof restored.centerGrammar === 'object') state.centerGrammar = { ...state.centerGrammar, ...restored.centerGrammar };
  if (restored.leftView === 'list' || restored.leftView === 'cards') state.leftView = restored.leftView;
  if (restored.centerView === 'list' || restored.centerView === 'cards') state.centerView = restored.centerView;
  if (Array.isArray(restored.expandedPosts)) state.expandedPosts = new Set(restored.expandedPosts.filter((id) => typeof id === 'string'));
  const leftSearch = els.leftPane?.querySelector('[data-pg-search]');
  if (leftSearch) leftSearch.value = state.leftGrammar.search || '';
  const centerSearch = els.centerPane?.querySelector('[data-pg-search]');
  if (centerSearch) centerSearch.value = state.centerGrammar.search || '';
  for (const [pane, grammar] of [[els.leftPane, state.leftGrammar], [els.centerPane, state.centerGrammar]]) {
    pane?.querySelectorAll('[data-pg-band]').forEach((tab) => {
      tab.setAttribute('aria-selected', String(tab.dataset.pgBand === grammar.band));
      tab.classList.toggle('is-active', tab.dataset.pgBand === grammar.band);
    });
  }
}

// Offener Drawer: erst wieder öffnen, wenn das gespeicherte Thema in den Live-Daten steht (neues Thema sofort).
function restoreDrawer(snap) {
  const topic = snap.persisted ? state.topics.find((item) => item.id === snap.topicId) : null;
  if (snap.persisted && !topic) return false;
  openDrawer(topic);
  if (!state.drawer) return true;
  if (!snap.persisted && snap.topicId) state.drawer.topic.id = snap.topicId;
  if (Array.isArray(snap.sources)) state.drawer.sources = snap.sources;
  if (Array.isArray(snap.removed)) state.drawer.removed = new Set(snap.removed);
  if (snap.digest && typeof snap.digest === 'object') {
    state.drawer.topic = withDigest(state.drawer.topic, snap.digest);
    renderDigestField();
  }
  for (const el of els.form?.elements || []) {
    if (!el.name || !(el.name in (snap.values || {}))) continue;
    const v = snap.values[el.name];
    if (el.type === 'checkbox') el.checked = Array.isArray(v) && v.includes(el.value);
    else if (el.type === 'radio') el.checked = v === el.value;
    else el.value = v;
  }
  syncCadenceFields?.();
  renderSourceGroups();
  if (snap.tab) setDrawerTab(snap.tab);
  return true;
}

async function loadModuleMarkup() {
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
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

// Schreiben nur über die App-API; `local`-Ablehnung → Zustandszone (Core-Ablehnungen meldet der Kernel).
function write(op, name, idOrDoc, patch) {
  const api = state.api;
  if (!api) return Promise.resolve(false);
  let handle;
  try {
    handle = op === 'put' ? api.put(name, idOrDoc) : op === 'patch' ? api.patch(name, idOrDoc, patch) : api.remove(name, idOrDoc);
  } catch (error) {
    notify('error', t('saveFailed'), error?.message, `write.${name}`);
    return Promise.resolve(false);
  }
  return handle.local.then(() => true, (error) => { notify('error', t('saveFailed'), error?.message, `write.${name}`); return false; });
}

function readOne(name, id) {
  return id && state.api ? state.api.get(name, id).get() : null;
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

// Angezeigte Ausgabe (FIX9a): Solange die gewählte Ausgabe noch erstellt wird und keine Beiträge hat, zeigt der
// Hauptbereich die letzte fertige Ausgabe desselben Themas. Der Erstellstatus steht nur in der Zone und als Punkt
// am Ausgabeknoten.
function displayedIssue() {
  const issue = selectedIssue();
  if (!issue || !ISSUE_ACTIVE_STATUSES.includes(issue.status)) return issue;
  if (state.posts.some((post) => post.issue_id === issue.id)) return issue;
  const ready = issuesForTopic(state.issues, issue.topic_id, Infinity)
    .find((other) => other.id !== issue.id && !ISSUE_ACTIVE_STATUSES.includes(other.status) && other.status !== 'failed'
      && state.posts.some((post) => post.issue_id === other.id));
  return ready || issue;
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
  // Sofort speichern (9f6ad058a): jede abgeschlossene Feldänderung eines gespeicherten Themas wird geschrieben.
  els.form?.addEventListener('change', (event) => {
    if (event.target?.closest?.('[data-digest-field]')) { onDigestChange(); return; }
    if (event.target?.dataset?.action === 'toggle-source') return;
    if (event.target === els.newSourceKind || event.target === els.newSourceValue) return;
    instantSave();
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
      instantSave();
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
  return t('digestCadence', base, `${weekdayName(digest.weekday, 'short')} ${digest.run_time}`);
}

function weekdayName(weekday, style = 'long') {
  const name = new Date(2024, 0, 7 + (Number(weekday) || 0)).toLocaleDateString(state.lang === 'en' ? 'en-US' : 'de-DE', { weekday: style });
  return style === 'short' ? name.replace('.', '') : name;
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
  // Leere Liste rendert nichts (PORTING.md R11); „Thema anlegen“ steht im Kopf der Spalte.
  if (!topics.length) {
    renderHtmlIfChanged(els.topicList, '');
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

// Laufende bzw. fehlgeschlagene Ausgabe der Auswahl → Zustandszone (vorher Leerzustand im Feed).
function reportIssueState(issue) {
  const api = state.api;
  if (!api) return;
  if (issue && ISSUE_ACTIVE_STATUSES.includes(issue.status)) {
    api.status.report({ level: 'info', code: 'news.issue.running', text: state.lang === 'en' ? 'Issue in progress' : 'Ausgabe wird erstellt', ref: issue.id });
  } else api.status.clear('news.issue.running');
  if (issue && issue.status === 'failed') {
    api.status.report({ level: 'error', code: 'news.issue.failed', text: issue.error ? `${t('issueFailed')}: ${issue.error}` : t('issueFailed'), ref: issue.id });
  } else api.status.clear('news.issue.failed');
}

function renderCenter() {
  if (!els.feed) return;
  const topic = selectedTopic();
  const running = selectedIssue();
  const issue = displayedIssue();
  els.feedTitle.textContent = topic ? (issue ? `${topic.title} · ${issueLabel(issue.issue_date, state.lang)}` : topic.title) : '';
  setDisabled('configure-topic', !topic);
  setDisabled('generate-issue', !topic || Boolean(running && ISSUE_ACTIVE_STATUSES.includes(running.status)));
  const taskButton = els.host.querySelector('[data-action="open-issue-task"]');
  if (taskButton) taskButton.hidden = !(running && (running.task_id || running.command_id) && running.status !== 'ready');

  const counts = categoryCounts(state.posts, issue?.id);
  els.centerPane?.__ctoxPaneGrammar?.setCounts?.(counts);
  setCountsFallback(els.centerPane, counts);
  hideEmptyCategoryTabs(counts);

  reportIssueState(running);
  if (!topic || !issue) {
    // Ohne Thema/Ausgabe bleibt der Feed leer; „Ausgabe erzeugen“ steht im Kopf des Feeds.
    setFooter(els.centerPane, '');
    renderHtmlIfChanged(els.feed, '');
    return;
  }
  const posts = postsForIssue(state.posts, issue.id, state.centerGrammar);
  if (!posts.length) {
    renderHtmlIfChanged(els.feed, '');
    return;
  }
  const html = state.centerView === 'list'
    ? `<div class="news-rows">${posts.map(renderPostRow).join('')}</div>`
    : `<div class="news-column">${posts.map(renderPostThread).join('')}</div>`;
  renderHtmlIfChanged(els.feed, html);
}

// Leere Kategoriereiter (0) ausblenden (FIX9a); „Alle“ bleibt. Steht die Auswahl auf einem leeren Reiter, gilt „Alle“.
function hideEmptyCategoryTabs(counts) {
  const tabs = [...(els.centerPane?.querySelectorAll('[data-pg-band]') || [])];
  let fallback = false;
  for (const tab of tabs) {
    const band = tab.dataset.pgBand;
    const empty = band !== 'all' && !(counts[band] > 0);
    tab.hidden = empty;
    if (empty && tab.getAttribute('aria-selected') === 'true') fallback = true;
  }
  if (fallback) tabs.find((tab) => tab.dataset.pgBand === 'all')?.click();
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
  const issue = displayedIssue();
  const trends = issue ? trendsForIssue(issue, state.posts) : [];
  // Leere Trend-Spalte ausblenden (FIX9a): Spalte samt Teiler weg, der Feed nimmt die Breite.
  els.root?.classList.toggle('is-no-trends', !trends.length);
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
  const doc = readOne(COLLECTIONS.posts, postId);
  if (!doc) return;
  await write('patch', COLLECTIONS.posts, postId, { bookmarked: doc.bookmarked !== true, updated_at_ms: Date.now() });
}

async function copyPostLink(postId) {
  const post = state.posts.find((item) => item.id === postId);
  const href = safeHref(post?.link_card?.url) || safeHref(postSources(post || {})[0]?.url);
  if (!href) return;
  try {
    await navigator.clipboard.writeText(href);
    notify('info', t('linkCopied'), '', 'link.copied');
  } catch {}
}

// Meldungen nur in die Zustandszone (Anf. 27); stabiler Code je Fall, damit `clear` greift. v4-allow: text (Zonen-Code, keine Klasse)
function notify(type, title, message = '', code = 'notice') {
  const api = state.api;
  if (!api) return;
  const level = type === 'error' ? 'error' : type === 'warning' ? 'warn' : 'info';
  api.status.report({ level, code: `news.${code}`, text: message ? `${title}: ${message}` : String(title || '') });
}

// --- Commands ------------------------------------------------------------

// Befehle über api.command, nie warten (R5). Umschlag aus core/news-model.mjs → Typ + Payload (+ record_id,
// client_context); Status live: failed → Zone (+ optionaler Folgeschritt), done → Zone räumen.
function dispatch(envelope, { onFailed } = {}) {
  const api = state.api;
  if (!api) return null;
  const type = envelope.command_type;
  const code = `news.command.${type}`;
  let handle;
  try {
    handle = api.command(type, {
      ...(envelope.payload || {}),
      record_id: envelope.record_id,
      client_context: envelope.client_context,
      sync_collections: envelope.sync_collections,
    });
  } catch (error) {
    api.status.report({ level: 'error', code, text: `${t('commandFailed')}: ${error?.message || error}` });
    onFailed?.(error?.message || String(error));
    return null;
  }
  handle.local.catch((error) => {
    api.status.report({ level: 'error', code, text: `${t('commandFailed')}: ${error?.message || error}`, ref: handle.id });
    onFailed?.(error?.message || String(error));
  });
  const watches = state.commandWatches;
  const off = handle.status.subscribe((st) => {
    if (st?.status === 'failed') {
      api.status.report({ level: 'error', code, text: `${t('commandFailed')}: ${st.error || ''}`.replace(/: $/, ''), ref: handle.id });
      onFailed?.(st.error || '');
    } else if (st?.status === 'done') {
      api.status.clear(code);
    } else return;
    off?.();
    watches.delete(handle.id);
  });
  watches.set(handle.id, off);
  return handle;
}

async function generateIssue() {
  const topic = selectedTopic();
  if (!topic || !state.api) return;
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
  const envelope = buildGenerateCommand(topic, state.sources, issue, previous, freshness);
  // Ausgabe lokal anlegen, dann Befehl senden (ohne Warten); die Befehls-ID vergibt der Kernel (OFFEN-2).
  issue.command_id = envelope.command_id;
  const stored = write('put', COLLECTIONS.issues, issue);
  state.selectedIssueId = issue.id;
  const handle = dispatch(envelope, {
    onFailed: (message) => write('patch', COLLECTIONS.issues, issue.id, { status: 'failed', error: String(message || t('commandFailed')), updated_at_ms: Date.now() }),
  });
  if (handle) stored.then((ok) => { if (ok) write('patch', COLLECTIONS.issues, issue.id, { command_id: handle.id, updated_at_ms: Date.now() }); });
  scheduleRender();
}

function focusIssueTask(issue) {
  if (!issue) return;
  const taskId = issue.task_id || '';
  const commandId = issue.command_id || '';
  if (!taskId && !commandId) return;
  // Sprung in die CTOX-App über die Shell (R13) statt sessionStorage + location.hash.
  state.api?.shell.navigate('ctox', {
    task_id: taskId, command_id: commandId, task_status: issue.status || '', source: 'news', drawer: true,
  });
}

async function runSourceCommand(sourceId, kind) {
  const topic = state.drawer?.topic;
  const source = state.drawer?.sources.find((item) => item.id === sourceId);
  if (!topic || !source) return;
  if (!state.drawer.persisted || !state.sources.some((item) => item.id === sourceId)) {
    notify('error', t('saveTopicFirst'), '', 'source.unsaved');
    return;
  }
  const envelope = kind === 'auth' ? buildAuthCommand(topic, source) : buildAdapterCommand(topic, source);
  const doc = readOne(COLLECTIONS.sources, sourceId);
  if (doc) {
    write('patch', COLLECTIONS.sources, sourceId, { status: 'pending', last_error: '', payload: { ...(doc.payload || {}), last_command_id: envelope.command_id }, updated_at_ms: Date.now() });
  }
  source.status = 'pending';
  renderSourceGroups();
  dispatch(envelope);
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
  syncSourcePlaceholder();
  setDrawerTab('topic');
  renderSourceGroups();
  els.scrim.hidden = false;
  els.drawer.hidden = false;
  els.form.elements.title.focus();
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
  renderDigestField();
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

// --- Wochenzusammenfassung (#800): Einstellung im Themen-Dialog, gespeichert wie jedes Feld (sofort + Rückgängig) ---

function renderDigestField() {
  const host = els.form?.querySelector('[data-digest-field]');
  if (!host || !state.drawer) return;
  const digest = digestOf(state.drawer.topic);
  host.querySelectorAll('[data-digest-text]').forEach((el) => { el.textContent = t(el.dataset.digestText); });
  host.querySelector('[data-digest-hint]')?.setAttribute('title', t('digestHint'));
  host.querySelectorAll('[data-digest-input="weekday"] option').forEach((option) => { option.textContent = weekdayName(option.value); });
  host.querySelector('[data-digest-input="enabled"]').checked = digest.enabled;
  host.querySelector('[data-digest-input="weekday"]').value = String(digest.weekday);
  host.querySelector('[data-digest-input="run_time"]').value = digest.run_time;
  host.querySelector('[data-digest-schedule]').hidden = !digest.enabled;
}

function readDigestField() {
  const host = els.form?.querySelector('[data-digest-field]');
  return {
    enabled: Boolean(host?.querySelector('[data-digest-input="enabled"]')?.checked),
    weekday: Number(host?.querySelector('[data-digest-input="weekday"]')?.value ?? 5),
    run_time: host?.querySelector('[data-digest-input="run_time"]')?.value || '17:00',
  };
}

// Neues Thema: bleibt im Entwurf bis „Thema anlegen“; gespeichertes Thema: sofort, mit Rückgängig.
function onDigestChange() {
  const drawer = state.drawer;
  if (!drawer) return;
  const next = readDigestField();
  if (JSON.stringify(digestOf(drawer.topic)) === JSON.stringify(digestOf({ payload: { digest: next } }))) return;
  drawer.topic = withDigest(drawer.topic, next);
  const schedule = els.form?.querySelector('[data-digest-schedule]');
  if (schedule) schedule.hidden = !next.enabled;
  instantSave();
}

function syncSourcePlaceholder() {
  const kind = els.newSourceKind?.value || 'lab';
  const key = { lab: 'placeholderLab', x: 'placeholderX', reddit: 'placeholderReddit', youtube: 'placeholderYoutube', newsletter: 'placeholderNewsletter', community: 'placeholderCommunity' }[kind];
  if (els.newSourceValue) els.newSourceValue.placeholder = t(key);
}

function sourceStatusPill(source) {
  // `pending` (Einrichtung läuft) ist der Zustand des laufenden Befehls und steht in der Zustandszone (news.command.*).
  const map = { needs_auth: 'statusNeedsAuth', auth_requested: 'statusAuthRequested', failed: 'statusFailed', repairing: 'statusRepairing' };
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
  instantSave();
}

function addPreset(kind) {
  if (!state.drawer) return;
  if (kind === 'reddit') {
    const merged = redditPresetSource(state.drawer.sources, state.drawer.topic.id);
    state.drawer.sources = [...state.drawer.sources.filter((source) => source.id !== merged.id), merged];
    renderSourceGroups();
    instantSave();
    return;
  }
  const preset = kind === 'lab' ? LAB_PRESET : COMMUNITY_PRESET;
  state.drawer.sources.push(...presetSources(state.drawer.topic.id, preset, kind, state.drawer.sources));
  renderSourceGroups();
  instantSave();
}

function toggleDraftSource(sourceId) {
  const source = state.drawer?.sources.find((item) => item.id === sourceId);
  if (!source) return;
  source.enabled = source.enabled === false;
  instantSave();
}

function removeDraftSource(sourceId) {
  if (!state.drawer) return;
  state.drawer.sources = state.drawer.sources.filter((source) => source.id !== sourceId);
  state.drawer.removed.add(sourceId);
  renderSourceGroups();
  instantSave();
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
    notify('error', t(problem), '', 'topic.invalid');
    els.form.elements.title.focus();
    return;
  }
  state.api?.status.clear('news.topic.invalid');
  if (!(await persistDrawer(topic))) return;
  closeDrawer();
  state.selectedTopicId = topic.id;
  scheduleRender();
}

// Thema + Quellen schreiben und den Zeitplan neu senden, wenn er sich geändert hat. Gemeinsam für „Thema anlegen“,
// Sofort-Speichern und Rückgängig (9f6ad058a). Nur api.put/remove, Befehl über api.command (dispatch).
async function persistDrawer(topic, sourceList = state.drawer?.sources || [], removed = state.drawer?.removed || new Set()) {
  const now = Date.now();
  const previous = state.topics.find((item) => item.id === topic.id);
  const writes = [write('put', COLLECTIONS.topics, topic)];
  for (const source of sourceList) {
    writes.push(write('put', COLLECTIONS.sources, { ...source, topic_id: topic.id, updated_at_ms: now }));
  }
  for (const sourceId of removed) {
    if (readOne(COLLECTIONS.sources, sourceId)) writes.push(write('remove', COLLECTIONS.sources, sourceId));
  }
  if (!(await Promise.all(writes)).every(Boolean)) return false;
  const scheduleChanged = !previous
    || previous.cron_expr !== topic.cron_expr
    || previous.enabled !== topic.enabled
    || previous.title !== topic.title;
  if (scheduleChanged) dispatch(buildScheduleCommand(topic));
  // Der Wochen-Termin folgt dem Thema: nur ein tägliches, aktives Thema behält ihn.
  const digestChanged = previous
    ? digestActive(previous) !== digestActive(topic) || (digestActive(topic) && JSON.stringify(digestOf(previous)) !== JSON.stringify(digestOf(topic)))
    : digestActive(topic);
  if (digestChanged) dispatch(buildScheduleCommand(topic, { digest: true }));
  return true;
}

// --- Sofort speichern + Rückgängig (8 s) ------------------------------------

const DRAWER_UNDO_MS = 8000;

function syncDrawerFoot() {
  const foot = els.drawer?.querySelector('[data-drawer-create-foot]');
  if (foot) foot.hidden = Boolean(state.drawer?.persisted);
  const create = foot?.querySelector('[data-action="save-topic"]');
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

// Ein neues Thema entsteht einmal mit „Thema anlegen“; Halbgültiges wird nicht gespeichert (Fehler → Zone).
async function instantSave() {
  const drawer = state.drawer;
  if (!drawer?.persisted) return;
  const topic = readTopicForm();
  const problem = validateTopic(topic);
  if (problem) {
    notify('error', t(problem), '', 'topic.invalid');
    return;
  }
  state.api?.status.clear('news.topic.invalid');
  const before = drawer.saved || drawerSnapshot();
  const removed = new Set(drawer.removed);
  if (!(await persistDrawer(topic, drawer.sources, removed))) return;
  if (state.drawer !== drawer) return;
  drawer.removed.clear();
  drawer.topic = topic;
  drawer.saved = drawerSnapshot();
  els.drawerTitle.textContent = topic.title;
  showDrawerUndo(before);
}

function showDrawerUndo(before) {
  const bar = els.drawer?.querySelector('[data-drawer-undo]');
  if (!bar || !state.drawer) return;
  bar.hidden = false;
  // Rückgängig führt auf den Stand vor der ersten Änderung dieser Folge zurück.
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
  drawer.topic = { ...snapshot.topic };
  drawer.sources = snapshot.sources.map((source) => ({ ...source }));
  drawer.saved = drawerSnapshot();
  drawer.removed.clear();
  hideDrawerUndo();
  els.drawerTitle.textContent = drawer.topic.title;
  fillDrawerForm(drawer.topic);
  renderSourceGroups();
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
  // Zeitplan löschen als Befehl (ohne Warten; Fehler → Zone), Datensätze lokal entfernen.
  dispatch(buildScheduleCommand(topic, { deleted: true }));
  if (digestOf(topic).enabled) dispatch(buildScheduleCommand(topic, { deleted: true, digest: true }));
  const removeWhere = async (name, predicate) => {
    for (const doc of state.api ? state.api.query(name, {}).get() : []) {
      if (predicate(doc)) await write('remove', name, doc.id);
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
      const idMap = new Map();
      const now = Date.now();
      for (const raw of Array.isArray(data.topics) ? data.topics : []) {
        const topic = normalizeTopic({ ...raw, id: newId('topic'), schedule_task_id: '' }, now);
        idMap.set(raw.id, topic.id);
        await write('put', COLLECTIONS.topics, topic);
        dispatch(buildScheduleCommand(topic));
        if (digestActive(topic)) dispatch(buildScheduleCommand(topic, { digest: true }));
      }
      for (const raw of Array.isArray(data.sources) ? data.sources : []) {
        const topicId = idMap.get(raw.topic_id);
        if (!topicId) continue;
        await write('put', COLLECTIONS.sources, { ...raw, id: newId('source'), topic_id: topicId, status: authSourceFor(raw) ? 'needs_auth' : 'new', created_at_ms: now, updated_at_ms: now });
      }
      state.api?.status.clear('news.import');
      notify('info', t('importDone', idMap.size), '', 'import.done');
    } catch (error) {
      notify('error', t('importFailed'), error?.message, 'import');
    }
  });
  input.click();
}
