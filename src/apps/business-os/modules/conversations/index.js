import { autoWirePaneGrammar } from 'ctox:shared/pane-grammar.js';

// This module reads the canonical CTOX channel projection (mirror of the
// communication_* tables in runtime/ctox.sqlite3). Per-channel threads are the
// source-of-truth shape; the contact-centric list and channel-tabbed detail
// view bucket them client-side by participant_keys_json.

const SUPPORTED_CHANNELS = [
  'whatsapp',
  'email',
  'jami',
  'teams',
  'meeting',
  'slack',
  'discord',
  'telegram',
  'matrix',
  'mattermost',
  'zulip',
  'google_chat',
];
const OUTBOUND_REPLY_CLASSIFICATIONS = [
  'positive',
  'negative',
  'objection',
  'out_of_office',
  'referral',
  'unsubscribe',
  'unclear',
];
const CHANNEL_LABEL_FALLBACK = {
  whatsapp: 'WhatsApp',
  email: 'E-Mail',
  physical_letter: 'Brief',
  jami: 'Jami',
  teams: 'MS Teams',
  meeting: 'Meeting',
  slack: 'Slack',
  discord: 'Discord',
  telegram: 'Telegram',
  matrix: 'Matrix',
  mattermost: 'Mattermost',
  zulip: 'Zulip',
  google_chat: 'Google Chat',
};
const ALL_CHANNELS_TAB = '__all__';
const MESSAGE_PAGE_SIZE = 200;
const DAY_MS = 86400000;
const ACCOUNT_HEALTH_WARN_MS = 24 * 60 * 60 * 1000;
const ACCOUNT_HEALTH_BAD_MS = 7 * 24 * 60 * 60 * 1000;

const FALLBACK_LABELS = {
  de: {
    moduleTitle: 'Konversationen',
    moduleSubtitle: 'Posteingang',
    footerEntries: 'Konversationen',
    importInvalid: 'Ungültige JSON-Datei.',
    importEmpty: 'Keine Konversationen in der Datei.',
    importedLocal: '{count} Konversationen geladen (nur lokal)',
    channelAll: 'Alle Kanäle',
    channelWhatsapp: 'WhatsApp',
    channelEmail: 'E-Mail',
    channelPhysicalLetter: 'Brief',
    channelJami: 'Jami',
    channelTeams: 'MS Teams',
    channelMeeting: 'Meeting',
    channelSlack: 'Slack',
    channelDiscord: 'Discord',
    channelTelegram: 'Telegram',
    channelMatrix: 'Matrix',
    channelMattermost: 'Mattermost',
    channelZulip: 'Zulip',
    channelGoogleChat: 'Google Chat',
    searchPlaceholder: 'Suchen…',
    cardLabelChannels: 'Aktive Kanäle',
    cardLabelOutbound: 'Outbound',
    cardLabelStats: 'Aktivität',
    cardLabelAccount: 'Konto',
    cardLabelAccounts: 'Konten',
    cardLabelFolder: 'E-Mail-Ordner',
    statMessages: 'Nachrichten',
    statFirst: 'Erste',
    statLast: 'Letzte',
    statAccount: 'Konto',
    statTrust: 'Vertrauen',
    directionInbound: 'Eingang',
    directionOutbound: 'Ausgang',
    directionAny: 'Alle Richtungen',
    dateAny: 'Alle Zeiträume',
    dateToday: 'Heute',
    date7d: 'Letzte 7 Tage',
    date30d: 'Letzte 30 Tage',
    accountAll: 'Alle Konten',
    statusSent: 'gesendet',
    statusDelivered: 'zugestellt',
    statusRead: 'gelesen',
    statusReceived: 'eingegangen',
    statusFailed: 'fehlgeschlagen',
    statusQueued: 'in Warteschlange', // v4-allow: label (Nachrichtenstatus 'queued' aus den Daten)
    todayLabel: 'Heute',
    yesterdayLabel: 'Gestern',
    unknownContact: 'Unbekannter Kontakt',
    threadCountSuffix: 'Threads',
    folderInbox: 'Posteingang',
    folderSent: 'Gesendet',
    folderArchive: 'Archiv',
    folderSpam: 'Spam',
    folderDrafts: 'Entwürfe',
    showHtml: 'HTML anzeigen',
    showPlain: 'Text anzeigen',
    detailsLabel: 'Details',
    detailMessageKey: 'Message-Key',
    detailRemoteId: 'Remote-ID',
    detailObservedAt: 'CTOX beobachtet',
    detailSeen: 'Gesehen',
    detailMetadata: 'Metadata',
    seenYes: 'ja',
    seenNo: 'nein',
    loadOlder: 'Ältere Nachrichten laden',
    olderTotal: 'von',
    taskBadgeLabel: 'Aufgabe',
    workBadgeLabel: 'Arbeitsauftrag',
    detailRouteStatus: 'Queue-Status',
    detailWorkId: 'Work-ID',
    routeQueued: 'in Queue',
    routeLeased: 'gelistet',
    routeRunning: 'läuft',
    routeDone: 'fertig',
    routeHandled: 'erledigt',
    routeFailed: 'fehlgeschlagen',
    routeCancelled: 'abgebrochen',
    routeBlocked: 'blockiert',
    healthOk: 'OK',
    healthWarn: 'Warnung',
    healthBad: 'Fehler',
    healthInboundLabel: 'Letzter Eingang',
    healthOutboundLabel: 'Letzter Ausgang',
    healthNever: 'nie',
    recipientsTo: 'An',
    recipientsCc: 'Cc',
    recipientsBcc: 'Bcc',
    trustHigh: 'Hoch',
    trustMedium: 'Mittel',
    trustLow: 'Niedrig',
    trustUnknown: 'Unbekannt',
    outboundOpenCampaign: 'In Outbound öffnen',
    outboundApprove: 'Freigeben',
    outboundReject: 'Ablehnen',
    outboundAwaitingApproval: 'wartet auf Freigabe', // v4-allow: label (Freigabestatus aus den Daten)
    outboundReplyLabel: 'Antwort',
    outboundReplyClassify: 'Klassifizieren',
    outboundReplyClassification: 'Reply-Klasse',
    outboundReplyMatched: 'Antwort automatisch zugeordnet',
  },
  en: {
    moduleTitle: 'Conversations',
    moduleSubtitle: 'Audit from CTOX’s perspective',
    footerEntries: 'conversations',
    importInvalid: 'Invalid JSON file.',
    importEmpty: 'No conversations in the file.',
    importedLocal: '{count} conversations loaded (local only)',
    channelAll: 'All channels',
    channelWhatsapp: 'WhatsApp',
    channelEmail: 'Email',
    channelJami: 'Jami',
    channelTeams: 'MS Teams',
    channelMeeting: 'Meeting',
    channelSlack: 'Slack',
    channelDiscord: 'Discord',
    channelTelegram: 'Telegram',
    channelMatrix: 'Matrix',
    channelMattermost: 'Mattermost',
    channelZulip: 'Zulip',
    channelGoogleChat: 'Google Chat',
    searchPlaceholder: 'Contact/text/account',
    cardLabelChannels: 'Active channels',
    cardLabelOutbound: 'Outbound',
    cardLabelStats: 'Activity',
    cardLabelAccount: 'CTOX account',
    cardLabelAccounts: 'CTOX accounts',
    cardLabelFolder: 'Email folders',
    statMessages: 'Messages',
    statFirst: 'First',
    statLast: 'Last',
    statAccount: 'CTOX account',
    statTrust: 'Trust',
    directionInbound: 'Inbound',
    directionOutbound: 'Outbound',
    directionAny: 'All directions',
    dateAny: 'All time',
    dateToday: 'Today',
    date7d: 'Last 7 days',
    date30d: 'Last 30 days',
    accountAll: 'All accounts',
    statusSent: 'sent',
    statusDelivered: 'delivered',
    statusRead: 'read',
    statusReceived: 'received',
    statusFailed: 'failed',
    statusQueued: 'queued',
    todayLabel: 'Today',
    yesterdayLabel: 'Yesterday',
    unknownContact: 'Unknown contact',
    threadCountSuffix: 'threads',
    folderInbox: 'Inbox',
    folderSent: 'Sent',
    folderArchive: 'Archive',
    folderSpam: 'Spam',
    folderDrafts: 'Drafts',
    showHtml: 'Show HTML',
    showPlain: 'Show plain text',
    detailsLabel: 'Details',
    detailMessageKey: 'Message key',
    detailRemoteId: 'Remote ID',
    detailObservedAt: 'CTOX observed',
    detailSeen: 'Seen',
    detailMetadata: 'Metadata',
    seenYes: 'yes',
    seenNo: 'no',
    loadOlder: 'Load older messages',
    olderTotal: 'of',
    taskBadgeLabel: 'Task',
    workBadgeLabel: 'Work item',
    detailRouteStatus: 'Queue status',
    detailWorkId: 'Work ID',
    routeQueued: 'queued',
    routeLeased: 'leased',
    routeRunning: 'running',
    routeDone: 'done',
    routeHandled: 'handled',
    routeFailed: 'failed',
    routeCancelled: 'cancelled',
    routeBlocked: 'blocked',
    healthOk: 'OK',
    healthWarn: 'Warning',
    healthBad: 'Error',
    healthInboundLabel: 'Last inbound',
    healthOutboundLabel: 'Last outbound',
    healthNever: 'never',
    recipientsTo: 'To',
    recipientsCc: 'Cc',
    recipientsBcc: 'Bcc',
    trustHigh: 'High',
    trustMedium: 'Medium',
    trustLow: 'Low',
    trustUnknown: 'Unknown',
    outboundOpenCampaign: 'Open in Outbound',
    outboundApprove: 'Approve',
    outboundReject: 'Reject',
    outboundAwaitingApproval: 'awaiting approval',
    outboundReplyLabel: 'Reply',
    outboundReplyClassify: 'Classify',
    outboundReplyClassification: 'Reply class',
    outboundReplyMatched: 'Reply matched automatically',
  },
};

// Snapshot (Live-Remount, PORTING §2): Ansicht, Auswahl und offene Eingaben (Suche, Filter, Reply-Klasse).
const SNAPSHOT_VIEW_KEYS = [
  'channel',
  'account',
  'direction',
  'dateRange',
  'search',
  'listView',
  'userCollapsedContext',
  'selectedBucketKey',
  'selectedChannel',
  'timelineLimit',
  'highlightedMessageKey',
];
const SNAPSHOT_VERSION = 1;

// App-API 4.0 (§10.2a Nr. 1): mount(root, api, {restored}) → {unmount, snapshot}. Daten nur über
// api.query/get/patch (live, lokal, vollständig), Befehle über api.command ohne Warten, Zustand nur in die
// Zustandszone (api.status.report). CSS lädt die Shell aus dem Manifest (styles), Spaltenbreiten verdrahtet und
// speichert die Shell (v4/shell/column-resizers.js) über data-resizer-var im Markup.
export async function mount(root, api, { restored } = {}) {
  if (!root.dataset.app) root.dataset.app = api.app?.id || 'conversations';
  root.innerHTML = await loadModuleMarkup();

  const frame = root.querySelector('[data-conv-root]');
  if (!frame) throw new Error('conversations: root missing after fragment mount');

  // Third column = context pane (participant/record context). It is hidden by
  // default and auto-reveals on selection: visible = hasSelection &&
  // !userCollapsed (design-guide "Progressive Disclosure", outbound idiom). The
  // toggle lives in the CENTER detail header so it only appears once a
  // conversation is selected — i.e. once there is something to reveal.
  const toggleContextEl = frame.querySelector('[data-conv-toggle-context]');

  // Abos und Listener gehören dieser Montage (PORTING §4 Falle 1).
  const disposers = [];
  let disposed = false;

  let lang = langFrom(api.shell?.locale?.get?.());
  let messages = messagesFor(api, lang);
  const t = (key, fallback) => messages[key] ?? fallback ?? key;

  applyStaticLabels(frame, t);
  api.shell.setTitle(t('moduleTitle', 'Konversationen'));

  const refs = collectRefs(frame);

  const view = {
    channel: 'all',
    account: '',
    direction: 'any',
    dateRange: 'any',
    search: '',
    listView: 'cards',
    userCollapsedContext: false,
    importedThreads: [],
    importedMessagesByThread: new Map(),
    threads: [],
    threadsLoaded: false,
    accountsById: new Map(),
    buckets: [],
    selectedBucketKey: null,
    selectedChannel: ALL_CHANNELS_TAB,
    bucketMessages: [],
    timelineMessages: [],
    timelineTotal: 0,
    timelineLimit: MESSAGE_PAGE_SIZE,
    deepLink: parseConversationDeepLink({}),
    highlightedMessageKey: '',
    // Outbound-Verknüpfung der Auswahl (Deep Link); null = aus Thread-/Nachrichten-Metadaten der Auswahl ableiten.
    outboundLink: null,
    outboundReplyMatchKeys: new Set(),
    classificationDraft: null,
    pendingSelection: null,
    pendingTimelineScrollTop: 0,
  };

  const restoredView = applyRestoredView(isPlainObject(restored) ? restored : null);

  // The left column chrome (search / shard-list toggle / filter tray / reset /
  // active-dot / channel band) is wired by the shared pane grammar from the
  // data-pg-* markup. The module re-renders on the bubbling grammar-change
  // event. Selecting a band/filter/search is an intentional reset (rebuilding
  // the list is expected there); selecting a ROW stays an in-place class flip
  // (see markActiveBucket).
  autoWirePaneGrammar(frame);
  listen(frame, 'ctox-pane-grammar-change', onLeftGrammarChange);

  // Wire the context-pane reveal toggle (auto-reveal model). Collapsing only
  // sets userCollapsed; re-selecting a conversation clears it.
  if (toggleContextEl) {
    listen(toggleContextEl, 'click', () => {
      view.userCollapsedContext = !view.userCollapsedContext;
      applyContextReveal();
    });
  }

  // Card/list switch: one button, module-owned (see syncViewToggle).
  if (refs.viewToggle) listen(refs.viewToggle, 'click', toggleListView);
  syncViewToggle();

  const importButton = frame.querySelector('[data-action="import"]');
  const exportButton = frame.querySelector('[data-action="export"]');
  if (importButton) listen(importButton, 'click', importConversations);
  if (exportButton) listen(exportButton, 'click', exportConversations);

  // ----- live data (R3): one subscription per collection; the first emission is the current local state -----

  const outboundRows = { outbound_messages: [], outbound_approvals: [] };
  let outboundSubscribed = false;
  const liveDocs = new Map();
  let timelineUnsubscribe = null;
  let timelineSubscriptionKey = '';
  let refreshQueued = false;

  rebuildBuckets();
  populateAccountFilter();
  renderList();

  subscribeCollection('communication_accounts', {}, (rows) => {
    view.accountsById = new Map(
      rows
        .filter((account) => account && account.is_deleted !== true)
        .map((account) => [account.account_key || account.id, account]),
    );
    populateAccountFilter();
    if (view.selectedBucketKey) renderRightPane();
  });

  subscribeCollection('communication_threads', { sort: [{ last_message_at: 'desc' }] }, (rows) => {
    view.threads = rows.slice().sort((a, b) => compareIsoDesc(a.last_message_at, b.last_message_at));
    view.threadsLoaded = true;
    rebuildBuckets();
    populateAccountFilter();
    applyConversationDeepLink();
    renderList();
    if (view.selectedBucketKey) renderDetail();
  });

  disposers.push(api.shell.locale.subscribe((locale) => {
    const next = langFrom(locale);
    if (next === lang) return;
    lang = next;
    messages = messagesFor(api, lang);
    applyStaticLabels(frame, t);
    api.shell.setTitle(t('moduleTitle', 'Konversationen'));
    syncViewToggle();
    populateAccountFilter();
    renderList();
    if (view.selectedBucketKey) renderDetail();
  }));

  // Deep links (v3: #conversations?…) arrive as route params (R13).
  disposers.push(api.shell.onRoute((event) => {
    view.deepLink = parseConversationDeepLink(event?.params || {});
    view.highlightedMessageKey = view.deepLink.message_key || '';
    applyConversationDeepLink();
    renderList();
    if (view.selectedBucketKey) renderDetail();
  }));

  if (restoredView?.deepLinkPending) applyConversationDeepLink();

  return {
    unmount() {
      disposed = true;
      timelineUnsubscribe?.();
      timelineUnsubscribe = null;
      for (const dispose of disposers.splice(0)) {
        try { dispose?.(); } catch (error) { console.error('[conversations] cleanup:', error); }
      }
      liveDocs.clear();
      root.replaceChildren();
    },
    snapshot,
  };

  // ----- mount helpers -----

  function listen(target, type, handler, options) {
    target.addEventListener(type, handler, options);
    disposers.push(() => target.removeEventListener(type, handler, options));
  }

  function subscribeCollection(collection, query, onRows) {
    try {
      const off = api.query(collection, query).subscribe((rows) => {
        if (disposed) return;
        api.status.clear(`conversations.data.${collection}`);
        onRows(Array.isArray(rows) ? rows : []);
      });
      disposers.push(off);
      return off;
    } catch (error) {
      api.status.report({ level: 'error', code: `conversations.data.${collection}`, text: `Conversations: ${collection} – ${errorToText(error)}`, ref: collection });
      return () => {};
    }
  }

  // Live single document (api.get) with one subscription per (collection, id) and mount.
  function liveDoc(collection, id) {
    if (!id) return null;
    const key = `${collection}\n${id}`;
    let entry = liveDocs.get(key);
    if (!entry) {
      entry = { value: null };
      liveDocs.set(key, entry);
      try {
        const doc = api.get(collection, id);
        entry.value = doc.get();
        disposers.push(doc.subscribe((value) => {
          if (disposed || value === entry.value) return;
          entry.value = value;
          scheduleRefresh();
        }));
      } catch (error) {
        api.status.report({ level: 'error', code: `conversations.data.${collection}`, text: `Conversations: ${collection} – ${errorToText(error)}`, ref: collection });
      }
    }
    return entry.value;
  }

  function ensureOutboundData() {
    if (outboundSubscribed) return;
    outboundSubscribed = true;
    for (const collection of Object.keys(outboundRows)) {
      subscribeCollection(collection, {}, (rows) => {
        outboundRows[collection] = rows;
        scheduleRefresh();
      });
    }
  }

  // Coalesced re-render after outbound/deep-link data changes (no polling, no retry timers).
  function scheduleRefresh() {
    if (refreshQueued) return;
    refreshQueued = true;
    queueMicrotask(() => {
      refreshQueued = false;
      if (disposed) return;
      const pending = view.deepLink && !view.deepLink.resolved && hasDeepLinkTarget(view.deepLink);
      if (pending) {
        applyConversationDeepLink();
        if (view.deepLink.resolved) {
          renderList();
          renderDetail();
          return;
        }
      }
      if (view.selectedBucketKey) renderRightPane();
    });
  }

  function snapshot() {
    const snap = { v: SNAPSHOT_VERSION };
    for (const key of SNAPSHOT_VIEW_KEYS) snap[key] = view[key];
    const searchInput = frame.querySelector('[data-pg-search]');
    snap.searchText = searchInput ? String(searchInput.value || '') : '';
    snap.deepLink = { ...(view.deepLink || {}) };
    snap.outboundLink = view.outboundLink ? { ...view.outboundLink } : null;
    snap.classificationDraft = view.classificationDraft ? { ...view.classificationDraft } : null;
    snap.timelineScrollTop = refs.timeline?.scrollTop || 0;
    return snap;
  }

  // Ansicht sofort setzen; die Auswahl erst übernehmen, wenn sie in den Live-Daten steht (erste Emission kann leer sein).
  function applyRestoredView(snap) {
    if (!snap || snap.v !== SNAPSHOT_VERSION) return null;
    const initial = { channel: 'all', account: '', direction: 'any', dateRange: 'any', search: '', listView: 'cards', userCollapsedContext: false, selectedChannel: ALL_CHANNELS_TAB, timelineLimit: MESSAGE_PAGE_SIZE, highlightedMessageKey: '' };
    for (const [key, value] of Object.entries(initial)) {
      if (typeof snap[key] === typeof value) view[key] = snap[key];
    }
    if (typeof snap.selectedBucketKey === 'string' && snap.selectedBucketKey) view.pendingSelection = snap.selectedBucketKey;
    if (isPlainObject(snap.deepLink)) view.deepLink = { ...parseConversationDeepLink({}), ...snap.deepLink };
    if (isPlainObject(snap.outboundLink)) view.outboundLink = { ...snap.outboundLink };
    if (isPlainObject(snap.classificationDraft)) view.classificationDraft = { ...snap.classificationDraft };
    const searchInput = frame.querySelector('[data-pg-search]');
    if (searchInput && typeof snap.searchText === 'string') searchInput.value = snap.searchText;
    for (const [selector, value] of [
      ['[data-conv-direction-filter]', view.direction],
      ['[data-conv-date-filter]', view.dateRange],
    ]) {
      const select = frame.querySelector(selector);
      if (select) select.value = value;
    }
    setActiveBandTab(view.channel);
    view.pendingTimelineScrollTop = Number(snap.timelineScrollTop) || 0;
    return { deepLinkPending: Boolean(view.deepLink && !view.deepLink.resolved && hasDeepLinkTarget(view.deepLink)) };
  }

  // ----- timeline (live query of the selected bucket's threads) -----

  function ensureTimelineSubscription(bucket) {
    const keys = bucket ? [...bucket.threadKeys].filter(Boolean).sort() : [];
    const subscriptionKey = keys.join('\n');
    if (subscriptionKey === timelineSubscriptionKey && (timelineUnsubscribe || !keys.length)) return;
    timelineUnsubscribe?.();
    timelineUnsubscribe = null;
    timelineSubscriptionKey = subscriptionKey;
    view.bucketMessages = [];
    if (!keys.length) return;
    let initial = true;
    try {
      timelineUnsubscribe = api.query('communication_messages', { selector: { thread_key: { $in: keys } } }).subscribe((rows) => {
        if (disposed) return;
        api.status.clear('conversations.data.communication_messages');
        view.bucketMessages = Array.isArray(rows) ? rows : [];
        if (initial) return;
        refreshTimelineForActiveBucket();
      });
    } catch (error) {
      api.status.report({ level: 'error', code: 'conversations.data.communication_messages', text: `Conversations: communication_messages – ${errorToText(error)}`, ref: 'communication_messages' });
    }
    initial = false;
  }

  function computeTimelineForBucket(bucket) {
    if (!bucket) {
      view.timelineMessages = [];
      view.timelineTotal = 0;
      return view.timelineMessages;
    }
    let all = [];
    // Local-only imported snapshot messages (offline audit review) come first.
    for (const key of bucket.threadKeys) {
      const imported = view.importedMessagesByThread.get(key);
      if (imported?.length) all = all.concat(imported);
    }
    all = all.concat(view.bucketMessages);
    all = applyMessageFilters(all);
    all.sort((a, b) => compareIsoDesc(a.external_created_at, b.external_created_at));
    view.timelineTotal = all.length;
    const trimmed = all.slice(0, view.timelineLimit);
    trimmed.reverse();
    view.timelineMessages = trimmed;
    return view.timelineMessages;
  }

  function applyMessageFilters(rows) {
    const cutoff = dateCutoff(view.dateRange);
    return rows.filter((msg) => {
      if (view.selectedChannel !== ALL_CHANNELS_TAB && msg.channel !== view.selectedChannel) return false;
      if (view.direction !== 'any' && msg.direction !== view.direction) return false;
      if (cutoff && isoToMs(msg.external_created_at) < cutoff) return false;
      return true;
    });
  }

  function refreshTimelineForActiveBucket() {
    const bucket = view.buckets.find((b) => b.key === view.selectedBucketKey);
    if (!bucket) return;
    computeTimelineForBucket(bucket);
    renderTimeline();
    renderRightPane();
  }

  // ----- deep link (route params) -----

  function applyConversationDeepLink() {
    const link = view.deepLink;
    if (!link || link.resolved || !hasDeepLinkTarget(link)) return;
    if (hasOutboundDeepLink(link)) {
      view.outboundLink = link;
      ensureOutboundData();
    }
    if (!link.filtersApplied) {
      link.filtersApplied = true;
      if (link.channel && (SUPPORTED_CHANNELS.includes(link.channel) || link.channel === 'all')) {
        view.channel = link.channel === 'all' ? 'all' : link.channel;
        setActiveBandTab(view.channel);
      }
      if (link.account_key) {
        view.account = link.account_key;
        if (refs.accountFilter) refs.accountFilter.value = link.account_key;
      }
    }
    if (link.message_key) {
      const message = liveDoc('communication_messages', link.message_key);
      if (message) {
        link.thread_key ||= message.thread_key || '';
        link.account_key ||= message.account_key || '';
        link.channel ||= message.channel || '';
        view.highlightedMessageKey = message.message_key || link.message_key;
      }
    }
    const bucket = findBucketForDeepLink(link);
    if (bucket) {
      view.selectedBucketKey = bucket.key;
      view.pendingSelection = null;
      view.selectedChannel = link.channel && bucket.channels.has(link.channel) ? link.channel : ALL_CHANNELS_TAB;
      view.timelineLimit = MESSAGE_PAGE_SIZE;
      link.resolved = true;
      api.status.clear('conversations.route.not_found');
      return;
    }
    // Kein stiller Rückgriff (R13): solange das Ziel in den lokalen Daten fehlt, steht es in der Zustandszone;
    // jede weitere Live-Emission versucht es erneut.
    if (view.threadsLoaded && (link.thread_key || link.message_key || link.account_key)) {
      api.status.report({ level: 'warn', code: 'conversations.route.not_found', text: `Conversations: Ziel nicht gefunden (${link.message_key || link.thread_key || link.account_key})`, ref: link.message_key || link.thread_key || link.account_key });
    }
  }

  function hasOutboundDeepLink(link = {}) {
    return Boolean(link.outbound_message_id || link.engagement_id || link.campaign_id || link.thread_key || link.message_key);
  }

  function hasDeepLinkTarget(link = {}) {
    return Boolean(hasOutboundDeepLink(link) || link.account_key || link.channel);
  }

  // Outbound context from the live outbound collections (foreign, read-only; manifest foreign_collections).
  function loadOutboundContextForLink(link = {}) {
    ensureOutboundData();
    const outboundMessages = outboundRows.outbound_messages;
    let outboundMessage = null;
    if (link.outbound_message_id) {
      outboundMessage = outboundMessages.find((message) => message.id === link.outbound_message_id) || null;
    }
    if (!outboundMessage && (link.message_key || link.thread_key || link.engagement_id)) {
      outboundMessage = outboundMessages.find((message) => (
        (link.message_key && (message.communication_message_key === link.message_key || message.payload?.communication_message_key === link.message_key))
        || (link.thread_key && (message.thread_key === link.thread_key || message.payload?.thread_key === link.thread_key))
        || (link.engagement_id && message.engagement_id === link.engagement_id)
      )) || null;
    }
    const engagementId = link.engagement_id || outboundMessage?.engagement_id || '';
    const engagement = engagementId ? liveDoc('outbound_engagements', engagementId) : null;
    const campaignId = link.campaign_id || outboundMessage?.campaign_id || engagement?.campaign_id || '';
    const campaign = campaignId ? liveDoc('outbound_campaigns', campaignId) : null;
    let pipeline = null;
    let contact = null;
    if (engagement?.pipeline_id) {
      pipeline = liveDoc('outbound_pipeline_items', engagement.pipeline_id);
      contact = contactForOutboundEngagement(pipeline, engagement);
    }
    let approvals = [];
    if (outboundMessage?.id) {
      approvals = outboundRows.outbound_approvals.filter((item) => item.message_id === outboundMessage.id);
      approvals.sort((a, b) => (Number(b.updated_at_ms) || 0) - (Number(a.updated_at_ms) || 0));
    }
    if (!outboundMessage && !engagement && !campaign) return null;
    return { campaign, pipeline, contact, engagement, message: outboundMessage, approvals };
  }

  function findBucketForDeepLink(link = {}) {
    return view.buckets.find((bucket) => link.thread_key && bucket.threadKeys.has(link.thread_key))
      || view.buckets.find((bucket) => link.account_key && bucket.accountKeys.has(link.account_key))
      || view.buckets.find((bucket) => bucket.threads.some((thread) => metadataMatchesOutboundLink(thread.metadata_json, link)))
      || null;
  }

  // ----- bucketing -----

  function rebuildBuckets() {
    const byKey = new Map();
    // Imported threads (in-memory import snapshot) are merged with the live
    // projection so they appear in the list without being persisted.
    for (const thread of [...view.threads, ...view.importedThreads]) {
      const participants = participantsOf(thread);
      const key = bucketKeyFor(participants);
      let bucket = byKey.get(key);
      if (!bucket) {
        bucket = {
          key,
          participants,
          displayName: deriveBucketDisplay(participants),
          threads: [],
          threadKeys: new Set(),
          channels: new Set(),
          accountKeys: new Set(),
          lastMessageAt: '',
          messageCount: 0,
          unreadCount: 0,
          subjects: new Set(),
        };
        byKey.set(key, bucket);
      }
      bucket.threads.push(thread);
      bucket.threadKeys.add(thread.thread_key);
      bucket.channels.add(thread.channel);
      if (thread.account_key) bucket.accountKeys.add(thread.account_key);
      bucket.messageCount += Number(thread.message_count) || 0;
      bucket.unreadCount += Number(thread.unread_count) || 0;
      if (compareIsoDesc(thread.last_message_at, bucket.lastMessageAt) < 0) {
        bucket.lastMessageAt = thread.last_message_at;
      }
      if (thread.subject) bucket.subjects.add(thread.subject);
    }
    view.buckets = [...byKey.values()].sort((a, b) => compareIsoDesc(a.lastMessageAt, b.lastMessageAt));
  }

  function populateAccountFilter() {
    const select = refs.accountFilter;
    if (!select) return;
    const current = select.value || view.account;
    const accounts = [...view.accountsById.values()].sort((a, b) => {
      const ca = (a.channel || '').localeCompare(b.channel || '');
      if (ca !== 0) return ca;
      return (a.address || '').localeCompare(b.address || '');
    });
    select.replaceChildren();
    const allOpt = document.createElement('option');
    allOpt.value = '';
    allOpt.textContent = t('accountAll', 'Alle Accounts');
    select.appendChild(allOpt);
    for (const account of accounts) {
      const opt = document.createElement('option');
      opt.value = account.account_key;
      opt.textContent = `${labelForChannel(account.channel, t)} · ${account.address || account.account_key}`;
      select.appendChild(opt);
    }
    if (current && [...accounts.map((a) => a.account_key)].includes(current)) {
      select.value = current;
    } else if (current && current === view.account) {
      select.value = view.account;
    } else {
      select.value = '';
      view.account = '';
    }
    select.disabled = accounts.length === 0;
  }

  // ----- renderers -----

  function onLeftGrammarChange(event) {
    const pane = leftPane();
    const state = event?.detail || pane?.__ctoxPaneGrammar?.state?.() || {};
    const filters = state.filters || {};
    view.search = String(state.search || '').trim().toLowerCase();
    // The single view toggle is module-owned (operator directive 31.08.2026);
    // the grammar reports it back through the pane's data-pg-default-view,
    // which syncViewToggle() keeps in sync. Any other value is a stale echo.
    if (state.view === 'list' || state.view === 'cards') view.listView = state.view;
    view.channel = state.band || 'all';
    view.account = filters.account || '';
    const nextDirection = filters.direction || 'any';
    const nextDate = filters.dateRange || 'any';
    const timelineFilterChanged = nextDirection !== view.direction || nextDate !== view.dateRange;
    view.direction = nextDirection;
    view.dateRange = nextDate;
    renderList();
    if (timelineFilterChanged && view.selectedBucketKey) refreshTimelineForActiveBucket();
  }

  function leftPane() {
    return frame.querySelector('.conv-left');
  }

  // ONE button, an action rather than a state: it always shows the view it
  // switches TO. The pane's data-pg-default-view carries the current view for
  // the shell-wired grammar, which has no [data-pg-view] control to read here.
  function syncViewToggle() {
    const button = refs.viewToggle;
    const pane = leftPane();
    const isList = view.listView === 'list';
    if (pane) pane.dataset.pgDefaultView = isList ? 'list' : 'cards';
    if (!button) return;
    const label = isList
      ? t('viewShowCards', 'Als Karten anzeigen')
      : t('viewShowList', 'Als Liste anzeigen');
    button.dataset.convView = isList ? 'list' : 'cards';
    button.setAttribute('aria-label', label);
    button.setAttribute('title', label);
    // aria-pressed is deliberately absent: the control is not a state.
    button.removeAttribute('aria-pressed');
    for (const icon of button.querySelectorAll('[data-conv-view-icon]')) {
      // Show the target view's icon, hide the current one.
      icon.hidden = icon.dataset.convViewIcon === (isList ? 'list' : 'cards');
    }
  }

  function toggleListView() {
    view.listView = view.listView === 'list' ? 'cards' : 'list';
    syncViewToggle();
    renderList();
  }

  function writeBandCounts(counts) {
    const pg = leftPane()?.__ctoxPaneGrammar;
    if (pg?.setCounts) { pg.setCounts(counts); return; }
    for (const [key, value] of Object.entries(counts || {})) {
      const node = frame.querySelector(`[data-pg-count="${key}"]`);
      if (node) node.textContent = ` (${value})`;
    }
  }

  function writeFooter(text) {
    const pg = leftPane()?.__ctoxPaneGrammar;
    if (pg?.setFooter) { pg.setFooter(text); return; }
    if (refs.footer) refs.footer.textContent = text || '';
  }

  function setActiveBandTab(channel) {
    for (const tab of refs.channelBand?.querySelectorAll('[data-pg-band]') || []) {
      const active = tab.dataset.pgBand === channel;
      tab.classList.toggle('is-active', active);
      tab.setAttribute('aria-selected', active ? 'true' : 'false');
    }
  }

  function renderChannelBand() {
    if (!refs.channelBand) return;
    // Band counts reflect every filter EXCEPT channel (the band is the channel
    // selector); zeros are rendered, never hidden.
    const base = filterBuckets(view.buckets, view, t, { ignoreChannel: true });
    writeBandCounts(channelBandCounts(base));
    // A channel is a real view only when it appears in the full dataset. Tabs
    // for channels that never appear are hidden; the band itself needs >= 2 real
    // channels, otherwise the count lives in the footer (grammar fallback).
    const fullCounts = channelBandCounts(view.buckets);
    const presentChannels = SUPPORTED_CHANNELS.filter((channel) => Number(fullCounts[channel]) > 0);
    refs.channelBand.hidden = presentChannels.length < 2;
    for (const tab of refs.channelBand.querySelectorAll('[data-pg-band]')) {
      const channel = tab.dataset.pgBand;
      tab.hidden = channel !== 'all' && !presentChannels.includes(channel);
    }
    if (view.channel !== 'all' && !presentChannels.includes(view.channel)) {
      view.channel = 'all';
      setActiveBandTab('all');
    }
  }

  function applyContextReveal() {
    const hasSelection = Boolean(view.selectedBucketKey);
    const visible = conversationContextVisible(hasSelection, view.userCollapsedContext);
    frame.classList.toggle('is-context-hidden', !visible);
    if (toggleContextEl) {
      toggleContextEl.setAttribute('aria-pressed', String(visible));
      toggleContextEl.setAttribute('aria-label', visible ? 'Kontext ausblenden' : 'Kontext einblenden');
    }
  }

  function footerScopeLabel() {
    if (view.channel && view.channel !== 'all') return labelForChannel(view.channel, t);
    return t('channelAll', 'Alle Channels');
  }

  function renderList() {
    const filtered = filterBuckets(view.buckets, view, t);
    refs.threadList.replaceChildren();
    refs.threadList.classList.toggle('is-list-view', view.listView === 'list');
    renderChannelBand();
    // Textdiät Regel 5: kein Zähler-Fuß; Zähler stehen an den Kanal-Reitern.
    writeFooter('');
    if (!filtered.length) {
      // Leere Liste rendert nichts (R11); die Fußzeile nennt die Anzahl.
      if (view.pendingSelection && !view.threadsLoaded) return;
      if (view.selectedBucketKey) {
        view.selectedBucketKey = null;
        view.selectedChannel = ALL_CHANNELS_TAB;
        renderDetail();
      } else {
        renderRightPane();
        applyContextReveal();
      }
      return;
    }

    for (const bucket of filtered) {
      refs.threadList.appendChild(buildBucketItem(bucket));
    }

    if (view.selectedBucketKey && !filtered.some((b) => b.key === view.selectedBucketKey)) {
      view.selectedBucketKey = null;
      view.selectedChannel = ALL_CHANNELS_TAB;
      renderDetail();
      return;
    }
    if (!view.selectedBucketKey && view.pendingSelection) {
      // snapshot/restored: Auswahl übernehmen, sobald sie in den Live-Daten steht.
      const pending = view.pendingSelection;
      if (filtered.some((b) => b.key === pending)) {
        view.pendingSelection = null;
        view.selectedBucketKey = pending;
        markActiveBucket();
        applyContextReveal();
        renderDetail();
        return;
      }
      if (view.threadsLoaded) view.pendingSelection = null;
    }
    if (!view.selectedBucketKey) {
      selectBucket(filtered[0].key);
    } else {
      markActiveBucket();
    }
  }

  function exportConversations() {
    const rows = filterBuckets(view.buckets, view, t);
    downloadJson(buildConversationsExport(rows, view.timelineMessages, Date.now()), 'conversations.json');
  }

  function downloadJson(payload, filename) {
    let url = '';
    try {
      const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
      url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = filename;
      a.rel = 'noopener';
      frame.appendChild(a);
      a.click();
      a.remove?.();
    } catch (error) {
      console.error('[conversations] export failed:', error);
    } finally {
      if (url) setTimeout(() => { try { URL.revokeObjectURL(url); } catch {} }, 4000);
    }
  }

  function importConversations() {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'application/json,.json';
    input.addEventListener('change', async () => {
      const file = input.files && input.files[0];
      if (!file) return;
      let parsed;
      try {
        parsed = JSON.parse(await file.text());
      } catch {
        api.status.report({ level: 'error', code: 'conversations.import', text: t('importInvalid', 'Ungültige JSON-Datei.') });
        return;
      }
      const { threads, messagesByThread } = parseConversationsImport(parsed);
      if (!threads.length) {
        api.status.report({ level: 'warn', code: 'conversations.import', text: t('importEmpty', 'Keine Konversationen in der Datei.') });
        return;
      }
      // Local-only overlay: imported conversations are shown for offline review
      // and never persisted (this is a read-only audit projection synced from
      // CTOX-Core, not a browser authoring path).
      view.importedThreads = threads;
      view.importedMessagesByThread = messagesByThread;
      rebuildBuckets();
      populateAccountFilter();
      renderList();
      if (view.selectedBucketKey) renderDetail();
      api.status.report({ level: 'info', code: 'conversations.import', text: t('importedLocal', '{count} Konversationen geladen (nur lokal)').replace('{count}', String(threads.length)) });
    });
    input.click();
  }

  function buildBucketItem(bucket) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'ctox-list-item conv-thread-item';
    btn.dataset.bucketKey = bucket.key;
    for (const [name, value] of Object.entries(conversationRecordContext(bucket))) {
      btn.setAttribute(name, value);
    }
    btn.setAttribute('role', 'option');
    if (view.selectedBucketKey === bucket.key) btn.classList.add('is-active', 'is-selected');

    const avatar = document.createElement('span');
    avatar.className = 'ctox-avatar';
    avatar.textContent = avatarGlyphFor(bucket.displayName);
    btn.appendChild(avatar);

    const newestThread = newestThreadOf(bucket);
    const body = document.createElement('div');
    body.className = 'conv-thread-body';
    // One markup, two densities (operator directive 31.08.2026): the SHARD view
    // shows title + preview + a meta line built from fields the module already
    // holds (channel dots, message count, CTOX account). The LIST view keeps
    // exactly the title row — CSS drops preview and meta, nothing is re-rendered.
    body.innerHTML = `
      <div class="conv-thread-top">
        <span class="conv-thread-name" data-v4-allow-ellipsis></span>
        <span class="conv-thread-time"></span>
      </div>
      <div class="conv-thread-preview">
        <span class="conv-thread-dir"></span>
        <span></span>
      </div>
      <div class="conv-thread-meta">
        <span class="conv-thread-channels"></span>
      </div>
    `;
    // FIX11 (G4): Teilnehmerliste einzeilig wie in Mail; der volle Text steht im Tooltip.
    const nameEl = body.querySelector('.conv-thread-name');
    nameEl.textContent = bucket.displayName;
    nameEl.title = bucket.displayName;
    body.querySelector('.conv-thread-time').textContent = formatTimeShort(bucket.lastMessageAt);
    const dir = body.querySelector('.conv-thread-dir');
    dir.textContent = bucket.channels.size > 1 ? `${bucket.channels.size}×` : '·';
    dir.title = `${bucket.channels.size} ${t('threadCountSuffix', 'Threads')}`;
    const subjEl = body.querySelector('.conv-thread-preview span:last-child');
    subjEl.textContent = newestThread?.subject || '';
    if (subjEl.textContent) { subjEl.title = subjEl.textContent; subjEl.setAttribute('data-v4-allow-ellipsis', ''); }
    const metaRow = body.querySelector('.conv-thread-meta');
    const channelsRow = body.querySelector('.conv-thread-channels');
    for (const channel of [...bucket.channels]) {
      const dot = document.createElement('span');
      dot.className = 'conv-channel-dot';
      dot.dataset.dot = channel;
      dot.title = labelForChannel(channel, t);
      channelsRow.appendChild(dot);
    }
    for (const text of bucketMetaFacts(bucket)) {
      const fact = document.createElement('span');
      fact.className = 'conv-thread-meta-fact';
      fact.textContent = text;
      metaRow.appendChild(fact);
    }
    if (!metaRow.querySelector('.conv-thread-meta-fact') && !bucket.channels.size) metaRow.hidden = true;
    if (bucket.unreadCount > 0) {
      const badge = document.createElement('span');
      badge.className = 'conv-unread-badge';
      badge.textContent = String(bucket.unreadCount);
      body.querySelector('.conv-thread-top').insertBefore(badge, body.querySelector('.conv-thread-time'));
    }
    btn.appendChild(body);
    btn.addEventListener('click', () => selectBucket(bucket.key));
    return btn;
  }

  // Meta facts for the shard view's second/third line. Only fields the module
  // already loads — no new query, no new collection.
  function bucketMetaFacts(bucket) {
    const facts = [];
    if (bucket.messageCount > 0) {
      facts.push(`${bucket.messageCount} ${t('statMessages', 'Nachrichten')}`);
    }
    const accountKey = [...(bucket.accountKeys || [])][0];
    if (accountKey) {
      const account = view.accountsById.get(accountKey);
      facts.push(account?.address || accountKey);
    }
    return facts;
  }

  function selectBucket(bucketKey) {
    if (view.selectedBucketKey === bucketKey) {
      markActiveBucket();
      return;
    }
    view.selectedBucketKey = bucketKey;
    view.selectedChannel = ALL_CHANNELS_TAB;
    view.timelineLimit = MESSAGE_PAGE_SIZE;
    view.outboundLink = null;
    view.highlightedMessageKey = '';
    view.userCollapsedContext = false;
    markActiveBucket();
    applyContextReveal();
    renderDetail();
  }

  function markActiveBucket() {
    for (const node of refs.threadList.querySelectorAll('.conv-thread-item')) {
      // is-active is the module hook; is-selected drives the kit list-item style.
      const active = node.dataset.bucketKey === view.selectedBucketKey;
      node.classList.toggle('is-active', active);
      node.classList.toggle('is-selected', active);
    }
  }

  function renderDetail() {
    applyContextReveal();
    const bucket = view.buckets.find((b) => b.key === view.selectedBucketKey);
    if (!bucket) {
      ensureTimelineSubscription(null);
      refs.detailHeader.hidden = true;
      refs.channelTabs.hidden = true;
      refs.channelTabs.replaceChildren();
      refs.timeline.replaceChildren();
      refs.rightDetail.hidden = true;
      return;
    }
    refs.detailHeader.hidden = false;
    refs.detailAvatar.textContent = avatarGlyphFor(bucket.displayName);
    refs.detailName.textContent = bucket.displayName;
    // FIX11 (G4): Teilnehmerliste im Kopf einzeilig wie in Mail; der volle Text steht im Tooltip.
    refs.detailName.title = bucket.displayName;
    refs.detailHandle.textContent = formatParticipantHandles(bucket.participants);

    renderChannelTabs(bucket);

    ensureTimelineSubscription(bucket);
    computeTimelineForBucket(bucket);
    renderTimeline();
    renderRightPane();
    if (view.pendingTimelineScrollTop && view.timelineMessages.length) {
      refs.timeline.scrollTop = view.pendingTimelineScrollTop;
      view.pendingTimelineScrollTop = 0;
    }
  }

  function renderChannelTabs(bucket) {
    refs.channelTabs.replaceChildren();
    const channels = [...bucket.channels];
    if (channels.length <= 1) {
      view.selectedChannel = channels[0] || ALL_CHANNELS_TAB;
      refs.channelTabs.hidden = true;
      return;
    }
    refs.channelTabs.hidden = false;

    const tabs = [ALL_CHANNELS_TAB, ...channels];
    const messageCountByChannel = countMessagesByChannel(bucket);

    for (const channel of tabs) {
      const tab = document.createElement('button');
      tab.type = 'button';
      tab.className = 'ctox-pane-tab';
      tab.setAttribute('role', 'tab');
      tab.dataset.channel = channel;
      if (channel === view.selectedChannel) {
        tab.classList.add('is-active');
        tab.setAttribute('aria-selected', 'true');
      }
      if (channel === ALL_CHANNELS_TAB) {
        tab.innerHTML = `<span></span><span class="ctox-chip-count"></span>`;
        tab.querySelector('span:first-child').textContent = t('channelAll', 'Alle Channels');
        tab.querySelector('.ctox-chip-count').textContent = String(bucket.messageCount || sumValues(messageCountByChannel));
      } else {
        tab.innerHTML = `
          <span class="conv-channel-dot" data-dot="${channel}"></span>
          <span></span>
          <span class="ctox-chip-count"></span>
        `;
        tab.querySelectorAll('span')[1].textContent = labelForChannel(channel, t);
        tab.querySelector('.ctox-chip-count').textContent = String(messageCountByChannel.get(channel) || 0);
      }
      tab.addEventListener('click', () => {
        view.selectedChannel = channel;
        view.timelineLimit = MESSAGE_PAGE_SIZE;
        for (const other of refs.channelTabs.querySelectorAll('.ctox-pane-tab')) {
          other.classList.toggle('is-active', other === tab);
          other.setAttribute('aria-selected', other === tab ? 'true' : 'false');
        }
        refreshTimelineForActiveBucket();
      });
      refs.channelTabs.appendChild(tab);
    }
  }

  function renderTimeline() {
    refs.timeline.replaceChildren();
    // Keine Nachrichten: die Timeline bleibt leer (R11).
    if (!view.timelineMessages.length) return;
    if (view.timelineTotal > view.timelineMessages.length) {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'ctox-button ctox-button--ghost conv-load-older';
      btn.textContent = `${t('loadOlder', 'Ältere laden')} (${view.timelineMessages.length} ${t('olderTotal', 'von')} ${view.timelineTotal})`;
      btn.addEventListener('click', () => {
        view.timelineLimit += MESSAGE_PAGE_SIZE;
        refreshTimelineForActiveBucket();
      });
      refs.timeline.appendChild(btn);
    }
    let lastDay = '';
    for (const msg of view.timelineMessages) {
      const dayKey = dayKeyFor(msg.external_created_at);
      if (dayKey !== lastDay) {
        const divider = document.createElement('div');
        divider.className = 'conv-day-divider';
        divider.textContent = formatDayLabel(msg.external_created_at, t);
        refs.timeline.appendChild(divider);
        lastDay = dayKey;
      }
      refs.timeline.appendChild(buildMessageBubble(msg));
    }
    const highlighted = Array.from(refs.timeline.querySelectorAll('.conv-message'))
      .find((node) => node.dataset.messageKey === view.highlightedMessageKey);
    if (highlighted) {
      highlighted.scrollIntoView({ block: 'center' });
    } else if (
      refs.timeline.scrollHeight - refs.timeline.scrollTop - refs.timeline.clientHeight <= 24
    ) {
      // Jump to the newest message only when the reader was already at the
      // bottom; otherwise their position in the history is preserved.
      refs.timeline.scrollTop = refs.timeline.scrollHeight;
    }
  }

  function buildMessageBubble(msg) {
    const el = document.createElement('article');
    el.className = 'conv-message';
    if (view.highlightedMessageKey && msg.message_key === view.highlightedMessageKey) {
      el.classList.add('is-highlighted');
      el.setAttribute('aria-current', 'true');
    }
    el.dataset.channel = msg.channel || '';
    el.dataset.direction = msg.direction || 'inbound';
    el.dataset.status = msg.status || '';
    el.dataset.seen = String(Number(msg.seen) || 0);
    el.dataset.messageKey = msg.message_key || '';
    for (const [name, value] of Object.entries(messageRecordContext(msg))) {
      el.setAttribute(name, value);
    }

    const meta = document.createElement('div');
    meta.className = 'conv-message-meta';
    meta.innerHTML = `
      <span class="conv-channel-dot" data-dot="${msg.channel || ''}"></span>
      <span class="conv-message-channel"></span>
      <span class="conv-message-sender"></span>
      <span class="conv-message-folder"></span>
      <span class="conv-message-time"></span>
    `;
    meta.querySelector('.conv-message-channel').textContent = labelForChannel(msg.channel, t);
    const sender = meta.querySelector('.conv-message-sender');
    sender.textContent = msg.sender_display
      ? `· ${msg.sender_display}`
      : (msg.sender_address ? `· ${msg.sender_address}` : '');
    const folderEl = meta.querySelector('.conv-message-folder');
    if (msg.folder_hint) {
      folderEl.textContent = `· ${labelForFolder(msg.folder_hint, t)}`;
    }
    meta.querySelector('.conv-message-time').textContent = formatTimeShort(msg.external_created_at);
    el.appendChild(meta);

    if (msg.subject && msg.channel === 'email') {
      const subject = document.createElement('div');
      subject.className = 'conv-message-subject';
      subject.textContent = msg.subject;
      el.appendChild(subject);
    }

    if (msg.channel === 'email') {
      el.appendChild(buildRecipientsBlock(msg));
    }

    const bodyText = msg.body_text || msg.preview || '';
    const hasHtml = !!msg.body_html;
    if (bodyText || hasHtml) {
      const bodyContainer = document.createElement('div');
      bodyContainer.className = 'conv-message-body-container';

      const plain = document.createElement('div');
      plain.className = 'conv-message-body';
      plain.textContent = bodyText;
      bodyContainer.appendChild(plain);

      if (hasHtml && msg.channel === 'email') {
        const toggle = document.createElement('button');
        toggle.type = 'button';
        toggle.className = 'ctox-button ctox-button--sm ctox-button--ghost conv-message-html-toggle';
        toggle.textContent = t('showHtml', 'HTML anzeigen');
        let htmlVisible = false;
        let iframe = null;
        toggle.addEventListener('click', (event) => {
          event.preventDefault();
          event.stopPropagation();
          htmlVisible = !htmlVisible;
          if (htmlVisible) {
            if (!iframe) {
              iframe = document.createElement('iframe');
              iframe.className = 'conv-message-html-frame';
              iframe.setAttribute('sandbox', '');
              iframe.setAttribute('referrerpolicy', 'no-referrer');
              iframe.setAttribute('loading', 'lazy');
              iframe.srcdoc = msg.body_html;
              bodyContainer.appendChild(iframe);
            } else {
              iframe.hidden = false;
            }
            plain.hidden = true;
            toggle.textContent = t('showPlain', 'Text anzeigen');
          } else {
            if (iframe) iframe.hidden = true;
            plain.hidden = false;
            toggle.textContent = t('showHtml', 'HTML anzeigen');
          }
        });
        bodyContainer.appendChild(toggle);
      }

      el.appendChild(bodyContainer);
    }

    if (Number(msg.has_attachments) > 0) {
      const wrap = document.createElement('div');
      wrap.className = 'conv-message-attachments';
      const pill = document.createElement('button');
      pill.type = 'button';
      pill.className = 'ctox-button ctox-button--sm conv-attachment';
      pill.textContent = `📎 ${msg.raw_payload_ref || ''}`.trim() || '📎';
      pill.title = msg.raw_payload_ref || '';
      pill.addEventListener('click', (event) => {
        event.preventDefault();
        event.stopPropagation();
        openAttachment(msg);
      });
      wrap.appendChild(pill);
      el.appendChild(wrap);
    }

    if (msg.status) {
      const status = document.createElement('span');
      const variant = badgeVariantForMessageStatus(msg.status);
      status.className = `ctox-badge conv-message-status${variant ? ` ${variant}` : ''}`;
      status.innerHTML = `<span class="conv-message-status-icon"></span><span></span>`;
      status.querySelector('.conv-message-status-icon').textContent = iconForStatus(msg.status);
      status.querySelectorAll('span')[1].textContent = labelForStatus(msg.status, t);
      el.appendChild(status);
    }

    const taskBadge = buildTaskBadge(msg);
    if (taskBadge) el.appendChild(taskBadge);

    el.appendChild(buildDetailsBlock(msg));

    return el;
  }

  function buildTaskBadge(msg) {
    // Inbound messages create queue tasks keyed by message_key (per QueueTaskView).
    // Outbound messages may carry a ticket_self_work_id once the harness run started.
    // route_status, ticket_self_work_id, and work_id are projected from CTOX-Core
    // (communication_routing_state + queue join) into the message doc as extras.
    const isInbound = msg.direction === 'inbound';
    const taskId = isInbound ? msg.message_key : '';
    const workId = msg.ticket_self_work_id || msg.work_id || '';
    if (!taskId && !workId) return null;

    const wrap = document.createElement('div');
    wrap.className = 'conv-message-task';
    // Chip = Nutzerbegriff + Titel, nie die Kennung (Textdiät Regel 4). Titel aus der Core-Projektion, sonst der
    // Betreff/Anfang der Nachricht, aus der die Aufgabe entstand. Die Kennungen stehen weiter unter „Details“.
    const fallbackTitle = compactChipTitle(msg.subject || msg.preview || msg.body_text || '');

    if (taskId) {
      wrap.appendChild(buildTaskChip({
        kind: 'task',
        label: t('taskBadgeLabel', 'Aufgabe'),
        title: compactChipTitle(msg.task_title) || fallbackTitle,
        routeStatus: msg.route_status || '',
        onOpen: () => navigateToFlowview({ taskId, sourceModule: 'conversations' }),
      }));
    }
    if (workId) {
      wrap.appendChild(buildTaskChip({
        kind: 'work',
        label: t('workBadgeLabel', 'Arbeitsauftrag'),
        title: compactChipTitle(msg.work_title || msg.ticket_title) || fallbackTitle,
        routeStatus: msg.route_status || '',
        onOpen: () => navigateToFlowview({ workId, taskId: taskId || msg.message_key, sourceModule: 'conversations' }),
      }));
    }
    return wrap;
  }

  function compactChipTitle(value) {
    const text = String(value || '').replace(/\s+/g, ' ').trim();
    return text.length > 48 ? `${text.slice(0, 47)}…` : text;
  }

  function buildTaskChip({ kind, label, title, routeStatus, onOpen }) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'ctox-chip conv-message-task-chip';
    btn.dataset.kind = kind;
    if (routeStatus) btn.dataset.routeStatus = routeStatus;
    btn.title = `${label}${title ? `: ${title}` : ''}${routeStatus ? ` · ${labelForRouteStatus(routeStatus, t)}` : ''}`;
    btn.innerHTML = `
      <span class="conv-message-task-chip-kind"></span>
      <span class="conv-message-task-chip-title"></span>
      <span class="ctox-badge conv-message-task-chip-status"></span>
      <span class="conv-message-task-chip-jump">↗</span>
    `;
    btn.querySelector('.conv-message-task-chip-kind').textContent = label;
    const titleEl = btn.querySelector('.conv-message-task-chip-title');
    if (title) titleEl.textContent = title; else titleEl.remove();
    const statusEl = btn.querySelector('.conv-message-task-chip-status');
    if (routeStatus) {
      statusEl.textContent = labelForRouteStatus(routeStatus, t);
      statusEl.dataset.routeStatus = routeStatus;
      const variant = badgeVariantForRouteStatus(routeStatus);
      if (variant) statusEl.classList.add(variant);
    } else {
      statusEl.hidden = true;
    }
    btn.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
      onOpen();
    });
    return btn;
  }

  function navigateToFlowview({ taskId = '', workId = '', sourceModule = 'conversations' } = {}) {
    if (!taskId && !workId) return;
    const params = {};
    if (taskId) params.task_id = taskId;
    if (workId) params.work_id = workId;
    if (sourceModule) params.source = sourceModule;
    api.shell.navigate('ctox', params);
  }

  function buildRecipientsBlock(msg) {
    const wrap = document.createElement('div');
    wrap.className = 'conv-message-recipients';
    const lines = [];
    const to = Array.isArray(msg.recipient_addresses_json) ? msg.recipient_addresses_json : [];
    const cc = Array.isArray(msg.cc_addresses_json) ? msg.cc_addresses_json : [];
    const bcc = Array.isArray(msg.bcc_addresses_json) ? msg.bcc_addresses_json : [];
    if (to.length) lines.push(`${t('recipientsTo', 'An')}: ${to.join(', ')}`);
    if (cc.length) lines.push(`${t('recipientsCc', 'Cc')}: ${cc.join(', ')}`);
    if (bcc.length) lines.push(`${t('recipientsBcc', 'Bcc')}: ${bcc.join(', ')}`);
    wrap.textContent = lines.join(' · ');
    if (!lines.length) wrap.hidden = true;
    return wrap;
  }

  function buildDetailsBlock(msg) {
    const details = document.createElement('details');
    details.className = 'conv-message-details';
    const summary = document.createElement('summary');
    summary.className = 'conv-message-details-summary';
    summary.textContent = t('detailsLabel', 'Details');
    details.appendChild(summary);

    const dl = document.createElement('dl');
    dl.className = 'conv-message-details-body';
    appendDetailRow(dl, t('detailMessageKey', 'Message-Key'), msg.message_key);
    if (msg.remote_id) appendDetailRow(dl, t('detailRemoteId', 'Remote-ID'), msg.remote_id);
    if (msg.route_status) appendDetailRow(dl, t('detailRouteStatus', 'Queue-Status'), labelForRouteStatus(msg.route_status, t));
    if (msg.ticket_self_work_id) appendDetailRow(dl, t('detailWorkId', 'Work-ID'), msg.ticket_self_work_id);
    else if (msg.work_id) appendDetailRow(dl, t('detailWorkId', 'Work-ID'), msg.work_id);
    if (msg.observed_at) appendDetailRow(dl, t('detailObservedAt', 'CTOX beobachtet'), formatDateShort(msg.observed_at));
    appendDetailRow(dl, t('detailSeen', 'Gesehen'), Number(msg.seen) > 0 ? t('seenYes', 'ja') : t('seenNo', 'nein'));
    if (msg.account_key) {
      const account = view.accountsById.get(msg.account_key);
      appendDetailRow(dl, t('statAccount', 'CTOX-Account'), account?.address || msg.account_key);
    }
    if (msg.trust_level) {
      appendDetailRow(dl, t('statTrust', 'Trust'), labelForTrust(msg.trust_level, t));
    }
    if (msg.metadata_json && Object.keys(msg.metadata_json).length) {
      const dt = document.createElement('dt');
      dt.textContent = t('detailMetadata', 'Metadata');
      const dd = document.createElement('dd');
      const pre = document.createElement('pre');
      pre.className = 'conv-message-details-meta';
      try {
        pre.textContent = JSON.stringify(msg.metadata_json, null, 2);
      } catch {
        pre.textContent = String(msg.metadata_json);
      }
      dd.appendChild(pre);
      dl.appendChild(dt);
      dl.appendChild(dd);
    }
    details.appendChild(dl);
    return details;
  }

  function appendDetailRow(dl, label, value) {
    if (!value) return;
    const dt = document.createElement('dt');
    dt.textContent = label;
    const dd = document.createElement('dd');
    dd.textContent = String(value);
    dl.appendChild(dt);
    dl.appendChild(dd);
  }

  function openAttachment(msg) {
    const ref = msg.raw_payload_ref || '';
    // v3 öffnete den file-viewer mit {args}; dieselben Argumente gehen jetzt als Route-Parameter (R6/R13).
    api.shell.navigate('file-viewer', {
      messageKey: msg.message_key,
      channel: msg.channel,
      ref,
      metadata: msg.metadata_json || {},
    });
  }

  function renderRightPane() {
    const bucket = view.buckets.find((b) => b.key === view.selectedBucketKey);
    if (!bucket) {
      refs.rightDetail.hidden = true;
      return;
    }
    refs.rightDetail.hidden = false;

    refs.channelsBody.replaceChildren();
    const messageCountByChannel = countMessagesByChannel(bucket);
    for (const channel of [...bucket.channels]) {
      const row = document.createElement('div');
      row.className = 'conv-channel-row';
      row.innerHTML = `
        <span class="conv-channel-row-name">
          <span class="conv-channel-dot" data-dot="${channel}"></span>
          <span></span>
        </span>
        <span class="conv-channel-row-count"></span>
      `;
      row.querySelector('.conv-channel-row-name span:last-child').textContent = labelForChannel(channel, t);
      row.querySelector('.conv-channel-row-count').textContent = String(messageCountByChannel.get(channel) || 0);
      refs.channelsBody.appendChild(row);
    }

    refs.statMessages.textContent = String(bucket.messageCount || view.timelineMessages.length || 0);
    const sorted = view.timelineMessages;
    const firstIso = sorted[0]?.external_created_at;
    const lastIso = sorted[sorted.length - 1]?.external_created_at || bucket.lastMessageAt;
    refs.statFirst.textContent = firstIso ? formatDateShort(firstIso) : '—';
    refs.statLast.textContent = lastIso ? formatDateShort(lastIso) : '—';

    if (refs.statAccount) {
      const labels = [...bucket.accountKeys].map((key) => {
        const acc = view.accountsById.get(key);
        return acc?.address || key;
      });
      refs.statAccount.textContent = labels.length ? labels.join(', ') : '—';
    }
    if (refs.statTrust) {
      const trusts = new Set();
      for (const msg of sorted) {
        if (msg.trust_level) trusts.add(msg.trust_level);
      }
      refs.statTrust.textContent = trusts.size
        ? [...trusts].map((tt) => labelForTrust(tt, t)).join(', ')
        : '—';
    }

    renderAccountsCard(bucket);
    renderOutboundCard(bucket);
    renderFolderCard(bucket);
  }

  function renderOutboundCard(bucket) {
    if (!refs.outboundCard || !refs.outboundBody) return;
    const context = currentOutboundContext(bucket);
    if (!context) {
      refs.outboundCard.hidden = true;
      refs.outboundBody.replaceChildren();
      return;
    }
    refs.outboundCard.hidden = false;
    refs.outboundBody.replaceChildren();

    const campaign = context.campaign || {};
    const engagement = context.engagement || {};
    const message = context.message || {};
    const latestApproval = context.approvals?.[0] || null;
    const leadName = context.contact?.name
      || context.contact?.full_name
      || engagement.payload?.contact_name
      || message.recipient_email
      || formatParticipantHandles(bucket.participants);
    const companyName = context.pipeline?.company_name || engagement.payload?.company_name || campaign.name || '';

    const summary = document.createElement('div');
    summary.className = 'conv-outbound-summary';
    summary.innerHTML = `
      <strong></strong>
      <span></span>
      <span></span>
    `;
    summary.querySelector('strong').textContent = campaign.name || t('outboundCampaignFallback', 'Outbound Campaign');
    summary.querySelectorAll('span')[0].textContent = [companyName, leadName].filter(Boolean).join(' · ') || message.recipient_email || '';
    summary.querySelectorAll('span')[1].textContent = outboundStatusLine({
      engagement,
      message,
      latestApproval,
    });
    refs.outboundBody.appendChild(summary);

    const rows = outboundFactRows({ message, engagement, latestApproval, leadName, t });
    if (rows.length) {
      const dl = document.createElement('dl');
      dl.className = 'ctox-fields conv-outbound-facts';
      for (const [label, value] of rows) {
        const dt = document.createElement('dt');
        dt.textContent = label;
        const dd = document.createElement('dd');
        dd.textContent = String(value);
        dl.appendChild(dt);
        dl.appendChild(dd);
      }
      refs.outboundBody.appendChild(dl);
    }

    const actions = document.createElement('div');
    actions.className = 'conv-outbound-actions';
    const openBtn = document.createElement('button');
    openBtn.type = 'button';
    openBtn.className = 'ctox-button';
    openBtn.dataset.action = 'conv-outbound-open';
    openBtn.textContent = t('outboundOpenCampaign', 'In Outbound öffnen');
    openBtn.addEventListener('click', () => openOutboundContext(context));
    actions.appendChild(openBtn);

    if (canApproveOutboundMessage(message)) {
      const approveBtn = document.createElement('button');
      approveBtn.type = 'button';
      approveBtn.className = 'ctox-button is-primary';
      approveBtn.dataset.action = 'conv-outbound-approve';
      approveBtn.dataset.messageId = message.id;
      approveBtn.textContent = t('outboundApprove', 'Freigeben');
      approveBtn.addEventListener('click', () => approveOutboundMessageFromConversations(message.id, context));
      actions.appendChild(approveBtn);

      const rejectBtn = document.createElement('button');
      rejectBtn.type = 'button';
      rejectBtn.className = 'ctox-button';
      rejectBtn.dataset.action = 'conv-outbound-reject';
      rejectBtn.dataset.messageId = message.id;
      rejectBtn.textContent = t('outboundReject', 'Ablehnen');
      rejectBtn.addEventListener('click', () => rejectOutboundMessageFromConversations(message.id, context));
      actions.appendChild(rejectBtn);
    }
    refs.outboundBody.appendChild(actions);

    const inboundReply = inboundReplyForContext(context, bucket);
    if (engagement.id && inboundReply) {
      maybeAutoMatchOutboundReply(context, inboundReply);
      refs.outboundBody.appendChild(buildReplyClassificationControl(context, inboundReply));
    }
  }

  function buildReplyClassificationControl(context, inboundReply) {
    const wrap = document.createElement('div');
    wrap.className = 'conv-outbound-reply-classifier';
    const current = context.engagement?.payload?.reply_classification || 'unclear';
    wrap.innerHTML = `
      <label>
        <span></span>
        <select class="ctox-select"></select>
      </label>
      <button type="button" class="ctox-button is-primary"></button>
      <small></small>
    `;
    wrap.querySelector('label span').textContent = t('outboundReplyClassification', 'Reply-Klasse');
    const select = wrap.querySelector('select');
    for (const option of OUTBOUND_REPLY_CLASSIFICATIONS) {
      const opt = document.createElement('option');
      opt.value = option;
      opt.textContent = labelForReplyClassification(option);
      select.appendChild(opt);
    }
    select.value = OUTBOUND_REPLY_CLASSIFICATIONS.includes(current) ? current : 'unclear';
    // Offene Eingabe (Reply-Klasse vor dem Klick) überlebt Re-Render und Remount (snapshot).
    const draft = view.classificationDraft;
    if (draft?.engagementId === context.engagement.id && OUTBOUND_REPLY_CLASSIFICATIONS.includes(draft.value)) select.value = draft.value;
    select.addEventListener('change', () => {
      view.classificationDraft = { engagementId: context.engagement.id, value: select.value };
    });
    wrap.querySelector('button').textContent = t('outboundReplyClassify', 'Klassifizieren');
    wrap.querySelector('button').dataset.action = 'conv-outbound-reply-classify';
    wrap.querySelector('button').dataset.engagementId = context.engagement.id;
    // Antwort mit Betreff bzw. Absender statt Nachrichten-Kennung (Textdiät Regel 4).
    const replyTitle = String(inboundReply.subject || inboundReply.sender_display || inboundReply.sender_address || inboundReply.preview || '').replace(/\s+/g, ' ').trim();
    wrap.querySelector('small').textContent = replyTitle
      ? `${t('outboundReplyLabel', 'Antwort')}: ${replyTitle.length > 60 ? `${replyTitle.slice(0, 59)}…` : replyTitle}`
      : t('outboundReplyLabel', 'Antwort');
    wrap.querySelector('button').addEventListener('click', () => {
      view.classificationDraft = null;
      classifyOutboundReply({
        engagementId: context.engagement.id,
        replyMessageKey: inboundReply.message_key || '',
        classification: select.value,
      });
    });
    return wrap;
  }

  // v3 hielt den Outbound-Kontext im Zustand bzw. lud ihn asynchron aus den Metadaten der Auswahl nach; v4 rechnet
  // ihn bei jedem Render aus den Live-Abos (outbound_*), Änderungen vom Core kommen gepusht.
  function currentOutboundContext(bucket) {
    if (!bucket) return null;
    const loadedContext = outboundContextFromBucket(bucket);
    const fallbackContext = outboundContextFromDeepLinkFallback(bucket);
    return mergeOutboundContexts(loadedContext, fallbackContext);
  }

  function outboundContextFromBucket(bucket) {
    if (view.outboundLink) return loadOutboundContextForLink(view.outboundLink);
    if (!bucket) return null;
    const threadContext = bucket.threads
      .map((thread) => outboundLinkFromMetadata(thread.metadata_json))
      .find(Boolean);
    const messageContext = view.timelineMessages
      .map((message) => outboundLinkFromMetadata(message.metadata_json))
      .find(Boolean);
    const link = threadContext || messageContext;
    if (!link) return null;
    return loadOutboundContextForLink(link);
  }

  function outboundContextFromDeepLinkFallback(bucket) {
    const link = view.deepLink || {};
    if (!link.outbound_message_id && !link.engagement_id && !link.campaign_id) return null;
    const message = view.timelineMessages.find((item) => (
      (link.message_key && item.message_key === link.message_key)
      || (link.thread_key && item.thread_key === link.thread_key)
      || metadataMatchesOutboundLink(item.metadata_json, link)
    ));
    if (!message) return null;
    const metadataLink = outboundLinkFromMetadata(message.metadata_json) || {};
    const outboundMessageId = link.outbound_message_id || metadataLink.outbound_message_id || '';
    if (!outboundMessageId) return null;
    const recipients = Array.isArray(message.recipient_addresses_json)
      ? message.recipient_addresses_json
      : [];
    const recipient = recipients[0] || '';
    const approvalStatus = firstMetadataValue(message.metadata_json, ['approval_status']) || 'awaiting_approval';
    const sendStatus = firstMetadataValue(message.metadata_json, ['send_status']) || 'awaiting_approval';
    return {
      campaign: {
        id: link.campaign_id || metadataLink.campaign_id || '',
        name: t('outboundCampaignFallback', 'Outbound Campaign'),
      },
      engagement: {
        id: link.engagement_id || metadataLink.engagement_id || '',
        campaign_id: link.campaign_id || metadataLink.campaign_id || '',
        payload: {
          contact_name: formatParticipantHandles(bucket?.participants || []),
          contact_email: recipient,
          channel: message.channel || link.channel || metadataLink.channel || '',
        },
      },
      message: {
        id: outboundMessageId,
        engagement_id: link.engagement_id || metadataLink.engagement_id || '',
        campaign_id: link.campaign_id || metadataLink.campaign_id || '',
        channel: message.channel || link.channel || metadataLink.channel || 'email',
        subject: message.subject || '',
        body_text: message.body_text || message.preview || '',
        recipient_email: recipient,
        approval_status: approvalStatus,
        send_status: sendStatus,
        communication_message_key: message.message_key || link.message_key || '',
        communication_account_key: message.account_key || link.account_key || '',
        thread_key: message.thread_key || link.thread_key || '',
      },
      approvals: [],
      fallback: 'communication_message_deep_link',
    };
  }

  function mergeOutboundContexts(primary, fallback) {
    if (!primary) return fallback || null;
    if (!fallback) return primary;
    return {
      ...fallback,
      ...primary,
      campaign: { ...(fallback.campaign || {}), ...(primary.campaign || {}) },
      pipeline: primary.pipeline || fallback.pipeline || null,
      contact: primary.contact || fallback.contact || null,
      engagement: { ...(fallback.engagement || {}), ...(primary.engagement || {}) },
      message: { ...(fallback.message || {}), ...(primary.message || {}) },
      approvals: primary.approvals?.length ? primary.approvals : (fallback.approvals || []),
    };
  }

  function outboundStatusLine({ engagement = {}, message = {}, latestApproval = null } = {}) {
    const parts = [];
    const approval = outboundApprovalLabel(message.approval_status || latestApproval?.decision || '', t);
    if (approval) parts.push(approval);
    const send = outboundSendLabel(message.send_status || '', t);
    if (send) parts.push(send);
    const engagementState = outboundEngagementLabel(engagement.status || '', t);
    if (engagementState) parts.push(engagementState);
    return parts.join(' · ') || 'Outbound-Kontext';
  }

  function outboundFactRows({ message = {}, engagement = {}, latestApproval = null, leadName = '', t } = {}) {
    return [
      [t('outboundFactChannel', 'Kanal'), labelForChannel(message.channel || engagement.payload?.channel || 'email', t)],
      [t('outboundFactRecipient', 'Empfaenger'), message.recipient_email || leadName],
      [t('outboundFactSubject', 'Betreff'), message.subject || ''],
      [t('outboundFactApproval', 'Freigabe'), outboundApprovalLabel(message.approval_status || latestApproval?.decision || '', t)],
      [t('outboundFactNextStep', 'Naechste Aktion'), outboundNextStepLabel(message, t)],
    ].filter(([, value]) => value);
  }

  function outboundApprovalLabel(status, t) {
    switch (String(status || '').toLowerCase()) {
      case 'awaiting_approval':
        return t('outboundApprovalAwaiting', 'Wartet auf Freigabe'); // v4-allow: label (approval_status der Nachricht)
      case 'approved':
        return t('outboundApprovalApproved', 'Freigegeben');
      case 'rejected':
        return t('outboundApprovalRejected', 'Abgelehnt');
      case 'changes_requested':
        return t('outboundApprovalChangesRequested', 'Aenderungen angefordert');
      default:
        return '';
    }
  }

  function outboundSendLabel(status, t) {
    switch (String(status || '').toLowerCase()) {
      case 'awaiting_approval':
        return '';
      case 'approved_not_sent':
        return t('outboundSendReady', 'Bereit fuer Versand');
      case 'queued_for_provider':
        return t('outboundSendQueued', 'Versand eingereiht');
      case 'sent':
        return t('outboundSendSent', 'Gesendet');
      case 'letter_exported':
        return t('outboundLetterExported', 'Brief exportiert');
      case 'manual_sent':
        return t('outboundLetterSent', 'Brief als verschickt markiert');
      default:
        return '';
    }
  }

  function outboundEngagementLabel(status, t) {
    switch (String(status || '').toLowerCase()) {
      case 'reply_received':
        return t('outboundReplyReceived', 'Antwort eingegangen');
      case 'meeting_booked':
        return t('outboundMeetingBooked', 'Termin gebucht');
      case 'closed':
        return t('outboundClosed', 'Abgeschlossen');
      default:
        return '';
    }
  }

  function outboundNextStepLabel(message = {}, t) {
    const approval = String(message.approval_status || '').toLowerCase();
    const send = String(message.send_status || '').toLowerCase();
    if (approval === 'awaiting_approval') return t('outboundNextReview', 'Nachricht pruefen und freigeben');
    if (approval === 'approved' && send === 'approved_not_sent') return t('outboundNextSend', 'In Outbound versenden');
    if (send === 'queued_for_provider') return t('outboundNextWaitProvider', 'Auf Versandbestaetigung warten');
    if (send === 'sent') return t('outboundNextWaitReply', 'Auf Antwort warten');
    return '';
  }

  function canApproveOutboundMessage(message = {}) {
    if (!message.id) return false;
    if (message.approval_status !== 'awaiting_approval') return false;
    if (message.send_status && message.send_status !== 'awaiting_approval') return false;
    if (!message.subject && message.channel !== 'physical_letter') return false;
    if (!message.body_text && !message.body_html) return false;
    if (message.channel === 'physical_letter') return Boolean(message.recipient_address_text || message.payload?.recipient_address_text);
    return Boolean(message.recipient_email);
  }

  function inboundReplyForContext(context = {}, bucket = null) {
    const replyMessageId = context.engagement?.payload?.reply_message_id || '';
    if (replyMessageId) {
      const existing = view.timelineMessages.find((message) => message.message_key === replyMessageId);
      if (existing) return existing;
    }
    const outboundThreadKey = context.message?.thread_key || context.message?.payload?.thread_key || '';
    const candidates = view.timelineMessages
      .filter((message) => message.direction === 'inbound')
      .filter((message) => !outboundThreadKey || message.thread_key === outboundThreadKey || bucket?.threadKeys?.has(message.thread_key));
    return candidates.sort((a, b) => compareIsoDesc(a.external_created_at, b.external_created_at))[0] || null;
  }

  function openOutboundContext(context = {}) {
    const campaignId = context.campaign?.id || context.message?.campaign_id || context.engagement?.campaign_id || '';
    const engagementId = context.engagement?.id || context.message?.engagement_id || '';
    const messageId = context.message?.id || '';
    const params = new URLSearchParams();
    if (campaignId) params.set('campaign_id', campaignId);
    if (engagementId) params.set('engagement_id', engagementId);
    if (messageId) params.set('message_id', messageId);
    api.shell.navigate('outbound', Object.fromEntries(params));
  }

  // ----- outbound commands (R5): api.command without waiting; failures go to the status zone -----
  // Die vier Befehlstypen gehören outbound (modules/outbound/handlers/, V4-HANDLERS.md). Die Handler
  // outbound.message.approve/reject schreiben Entscheidung (outbound_messages) und Audit (outbound_approvals) selbst;
  // das lokale Upsert der UI (v3 materializeOutboundDecisionLocally) entfällt, die Änderung kommt gepusht.

  function approveOutboundMessageFromConversations(messageId, context = null) {
    sendOutboundCommand('outbound.message.approve', messageId, { message_id: messageId }, {
      onDone: () => applyOutboundDecisionToCommunicationMessage(messageId, 'approved', context),
    });
  }

  function rejectOutboundMessageFromConversations(messageId, context = null) {
    sendOutboundCommand('outbound.message.reject', messageId, {
      message_id: messageId,
      comment: 'Rejected from Conversations',
    }, {
      onDone: () => applyOutboundDecisionToCommunicationMessage(messageId, 'rejected', context),
    });
  }

  function classifyOutboundReply({ engagementId, replyMessageKey, classification }) {
    sendOutboundCommand('outbound.reply.classify', engagementId, {
      engagement_id: engagementId,
      classification,
      reply_message_id: replyMessageKey,
    });
  }

  function maybeAutoMatchOutboundReply(context = {}, inboundReply = {}) {
    const engagementId = context.engagement?.id || context.message?.engagement_id || '';
    const replyMessageKey = inboundReply.message_key || '';
    if (!engagementId || !replyMessageKey) return;
    if (context.engagement?.payload?.reply_message_id === replyMessageKey) return;
    const dedupeKey = `${engagementId}:${replyMessageKey}`;
    if (view.outboundReplyMatchKeys.has(dedupeKey)) return;
    view.outboundReplyMatchKeys.add(dedupeKey);
    sendOutboundCommand('outbound.reply.match', engagementId, {
      engagement_id: engagementId,
      outbound_message_id: context.message?.id || '',
      reply_message_id: replyMessageKey,
      classification: context.engagement?.payload?.reply_classification || 'unclear',
    }, {
      onFailed: () => view.outboundReplyMatchKeys.delete(dedupeKey),
    });
  }

  function sendOutboundCommand(type, recordId, payload, { onDone, onFailed } = {}) {
    const code = `conversations.command.${type}`;
    let handle;
    try {
      handle = api.command(type, {
        ...payload,
        record_id: recordId || '',
        client_context: {
          source_module: 'conversations',
          inbound_channel: 'business_os.conversations',
          deep_link: plainDeepLink(view.deepLink),
        },
      });
    } catch (error) {
      api.status.report({ level: 'error', code, text: `Outbound ${type}: ${errorToText(error)}` });
      onFailed?.(error);
      return null;
    }
    let ended = false;
    let off = null;
    const stop = () => {
      ended = true;
      if (off) {
        off();
        const index = disposers.indexOf(off);
        if (index >= 0) disposers.splice(index, 1);
        off = null;
      }
    };
    off = handle.status.subscribe((state) => {
      if (ended || disposed) return;
      if (state?.status === 'failed') {
        api.status.report({ level: 'error', code, text: `Outbound ${type}: ${state.error || 'failed'}`, ref: handle.id });
        stop();
        onFailed?.(state);
      } else if (state?.status === 'done') {
        api.status.clear(code);
        stop();
        onDone?.(state);
      }
    });
    if (ended) {
      off?.();
      off = null;
    } else {
      disposers.push(off);
    }
    handle.local?.catch?.((error) => {
      if (disposed) return;
      api.status.report({ level: 'error', code, text: `Outbound ${type}: ${errorToText(error)}`, ref: handle.id });
      stop();
      onFailed?.(error);
    });
    return handle;
  }

  // Projektion der Entscheidung in die eigene Kommunikationsnachricht (communication_messages.metadata_json),
  // unverändert aus v3; jetzt nach `done` des Befehls statt nach dem lokalen Upsert, über api.patch (Dokument-id =
  // message_key, §16.28).
  function applyOutboundDecisionToCommunicationMessage(messageId, decision, renderedContext = null) {
    const bucket = view.buckets.find((b) => b.key === view.selectedBucketKey) || null;
    const liveContext = currentOutboundContext(bucket);
    const message = outboundRows.outbound_messages.find((item) => item.id === messageId)
      || (liveContext?.message?.id === messageId ? liveContext.message : null)
      || renderedContext?.message
      || {};
    const communicationMessageKey = cleanParam(
      message.communication_message_key
        || message.payload?.communication_message_key
        || view.deepLink?.message_key
        || ''
    );
    if (!communicationMessageKey) return;
    let current = null;
    try {
      current = api.get('communication_messages', communicationMessageKey).get();
    } catch (error) {
      api.status.report({ level: 'error', code: 'conversations.data.communication_messages', text: `Conversations: communication_messages – ${errorToText(error)}`, ref: communicationMessageKey });
      return;
    }
    if (!current) return;
    const approved = decision === 'approved';
    const context = liveContext || renderedContext || {};
    const metadata = {
      ...(current.metadata_json && typeof current.metadata_json === 'object' ? current.metadata_json : {}),
      approval_status: approved ? 'approved' : 'rejected',
      send_status: approved ? 'approved_not_sent' : 'cancelled',
      outbound_message_id: cleanParam(message.id || messageId),
      outbound_engagement_id: cleanParam(message.engagement_id || context.engagement?.id || ''),
      outbound_campaign_id: cleanParam(message.campaign_id || context.campaign?.id || ''),
      communication_message_key: communicationMessageKey,
    };
    const handle = api.patch('communication_messages', communicationMessageKey, {
      status: current.status || 'draft',
      folder_hint: current.folder_hint || 'drafts',
      metadata_json: metadata,
      updated_at: new Date().toISOString(),
    });
    handle.local?.catch?.((error) => {
      api.status.report({ level: 'error', code: 'conversations.write.communication_messages', text: `Conversations: communication_messages – ${errorToText(error)}`, ref: communicationMessageKey });
    });
  }

  function renderAccountsCard(bucket) {
    if (!refs.accountsBody) return;
    refs.accountsBody.replaceChildren();
    const accounts = [...bucket.accountKeys]
      .map((key) => view.accountsById.get(key))
      .filter(Boolean);
    if (!accounts.length) {
      const empty = document.createElement('span');
      empty.textContent = '—';
      refs.accountsBody.appendChild(empty);
      return;
    }
    for (const account of accounts) {
      refs.accountsBody.appendChild(buildAccountRow(account));
    }
  }

  function buildAccountRow(account) {
    const row = document.createElement('div');
    row.className = 'conv-account-row';
    const health = healthStateFor(account, t);
    row.innerHTML = `
      <span class="conv-account-health" data-state="${health.state}" title="${health.tooltip}"></span>
      <div class="conv-account-meta">
        <strong></strong>
        <div class="conv-account-meta-line"></div>
      </div>
    `;
    const strong = row.querySelector('strong');
    strong.textContent = `${labelForChannel(account.channel, t)} · ${account.address || account.account_key}`;
    // Textdiät Regel 4: Anbieter-Rohwert (standard/email/internal) nur als Tooltip.
    // FIX11 (G4): lange Adressen kuerzen einzeilig; der Tooltip traegt die volle Zeile (plus Anbieter).
    strong.title = account.provider ? `${strong.textContent} (${account.provider})` : strong.textContent;
    strong.setAttribute('data-v4-allow-ellipsis', '');
    const lines = [null, row.querySelector('.conv-account-meta-line')];
    lines[1].innerHTML = '';
    const inboundSpan = document.createElement('span');
    inboundSpan.textContent = `${t('healthInboundLabel', 'Letzter Eingang')}: ${formatRelativeOrNever(account.last_inbound_ok_at, t)}`;
    const outboundSpan = document.createElement('span');
    outboundSpan.textContent = `${t('healthOutboundLabel', 'Letzter Ausgang')}: ${formatRelativeOrNever(account.last_outbound_ok_at, t)}`;
    lines[1].appendChild(inboundSpan);
    lines[1].appendChild(outboundSpan);
    return row;
  }

  function renderFolderCard(bucket) {
    if (!refs.folderCard || !refs.folderBody) return;
    const hasEmail = bucket.channels.has('email');
    const folderCounts = new Map();
    for (const msg of view.timelineMessages) {
      if (msg.channel !== 'email') continue;
      const folder = msg.folder_hint || 'inbox';
      folderCounts.set(folder, (folderCounts.get(folder) || 0) + 1);
    }
    if (!hasEmail || folderCounts.size === 0) {
      refs.folderCard.hidden = true;
      return;
    }
    refs.folderCard.hidden = false;
    refs.folderBody.replaceChildren();
    for (const [folder, count] of folderCounts) {
      const row = document.createElement('div');
      row.className = 'conv-folder-row';
      row.innerHTML = `<span></span><span class="conv-folder-row-count"></span>`;
      row.querySelector('span:first-child').textContent = labelForFolder(folder, t);
      row.querySelector('.conv-folder-row-count').textContent = String(count);
      refs.folderBody.appendChild(row);
    }
  }

  function countMessagesByChannel(bucket) {
    const counts = new Map();
    for (const thread of bucket.threads) {
      counts.set(thread.channel, (counts.get(thread.channel) || 0) + (Number(thread.message_count) || 0));
    }
    return counts;
  }

  function newestThreadOf(bucket) {
    return [...bucket.threads].sort((a, b) => compareIsoDesc(a.last_message_at, b.last_message_at))[0];
  }

}

// ----- helpers (pure / DOM-agnostic) -----

function collectRefs(root) {
  return {
    threadList: root.querySelector('[data-conv-thread-list]'),
    channelBand: root.querySelector('[data-conv-channel-band]'),
    viewToggle: root.querySelector('[data-conv-view-toggle]'),
    footer: root.querySelector('[data-conv-footer]'),
    accountFilter: root.querySelector('[data-conv-account-filter]'),
    detailHeader: root.querySelector('[data-conv-detail-header]'),
    detailAvatar: root.querySelector('[data-conv-detail-avatar]'),
    detailName: root.querySelector('[data-conv-detail-name]'),
    detailHandle: root.querySelector('[data-conv-detail-handle]'),
    channelTabs: root.querySelector('[data-conv-channel-tabs]'),
    timeline: root.querySelector('[data-conv-timeline]'),
    rightDetail: root.querySelector('[data-conv-right-detail]'),
    outboundCard: root.querySelector('[data-conv-outbound-card]'),
    outboundBody: root.querySelector('[data-conv-outbound-card-body]'),
    channelsBody: root.querySelector('[data-conv-channels-card-body]'),
    accountsBody: root.querySelector('[data-conv-accounts-card-body]'),
    folderCard: root.querySelector('[data-conv-folder-card]'),
    folderBody: root.querySelector('[data-conv-folder-card-body]'),
    statMessages: root.querySelector('[data-conv-stat-messages]'),
    statFirst: root.querySelector('[data-conv-stat-first]'),
    statLast: root.querySelector('[data-conv-stat-last]'),
    statAccount: root.querySelector('[data-conv-stat-account]'),
    statTrust: root.querySelector('[data-conv-stat-trust]'),
  };
}

function applyStaticLabels(root, t) {
  const map = {
    '[data-conv-title]': 'moduleTitle',
    '[data-conv-sub]': 'moduleSubtitle',
    '[data-conv-card-label-channels]': 'cardLabelChannels',
    '[data-conv-card-label-outbound]': 'cardLabelOutbound',
    '[data-conv-card-label-stats]': 'cardLabelStats',
    '[data-conv-card-label-accounts]': 'cardLabelAccounts',
    '[data-conv-card-label-folder]': 'cardLabelFolder',
    '[data-conv-stat-label-messages]': 'statMessages',
    '[data-conv-stat-label-first]': 'statFirst',
    '[data-conv-stat-label-last]': 'statLast',
    '[data-conv-stat-label-account]': 'statAccount',
    '[data-conv-stat-label-trust]': 'statTrust',
  };
  for (const [selector, key] of Object.entries(map)) {
    const el = root.querySelector(selector);
    if (el) el.textContent = t(key);
  }
  // Voller Titel als Tooltip, falls er im schmalen Kopf gekürzt wird.
  const titleEl = root.querySelector('[data-conv-title]');
  if (titleEl) titleEl.title = titleEl.textContent;
  const search = root.querySelector('[data-pg-search]');
  if (search) search.placeholder = t('searchPlaceholder', 'Suche');
  const labelByChannel = {
    all: t('channelAll', 'Alle Kanäle'),
    whatsapp: t('channelWhatsapp', 'WhatsApp'),
    email: t('channelEmail', 'E-Mail'),
    jami: t('channelJami', 'Jami'),
    teams: t('channelTeams', 'MS Teams'),
    meeting: t('channelMeeting', 'Meeting'),
    slack: t('channelSlack', 'Slack'),
    discord: t('channelDiscord', 'Discord'),
    telegram: t('channelTelegram', 'Telegram'),
    matrix: t('channelMatrix', 'Matrix'),
    mattermost: t('channelMattermost', 'Mattermost'),
    zulip: t('channelZulip', 'Zulip'),
    google_chat: t('channelGoogleChat', 'Google Chat'),
  };
  // Channel band tab labels (data-pg-band → data-band-label).
  for (const tab of root.querySelectorAll('[data-pg-band]')) {
    const channel = tab.dataset.pgBand;
    const labelEl = tab.querySelector('[data-band-label]');
    if (labelEl) labelEl.textContent = labelByChannel[channel] || channel;
  }
  const directionLabels = {
    any: t('directionAny', 'Alle Richtungen'),
    inbound: t('directionInbound', 'Eingang'),
    outbound: t('directionOutbound', 'Ausgang'),
  };
  for (const opt of root.querySelectorAll('[data-conv-direction-filter] option')) {
    opt.textContent = directionLabels[opt.value] || opt.textContent;
  }
  const dateLabels = {
    any: t('dateAny', 'Alle Zeiträume'),
    today: t('dateToday', 'Heute'),
    '7d': t('date7d', 'Letzte 7 Tage'),
    '30d': t('date30d', 'Letzte 30 Tage'),
  };
  for (const opt of root.querySelectorAll('[data-conv-date-filter] option')) {
    opt.textContent = dateLabels[opt.value] || opt.textContent;
  }
}

function bucketHasChannel(bucket, channel) {
  const channels = bucket.channels;
  if (channels instanceof Set) return channels.has(channel);
  return Array.isArray(channels) && channels.includes(channel);
}

function bucketHasAccount(bucket, account) {
  const keys = bucket.accountKeys;
  if (keys instanceof Set) return keys.has(account);
  return Array.isArray(keys) && keys.includes(account);
}

function filterBuckets(buckets, view, t, { ignoreChannel = false } = {}) {
  const cutoff = dateCutoff(view.dateRange);
  return (Array.isArray(buckets) ? buckets : []).filter((bucket) => {
    if (!ignoreChannel && view.channel && view.channel !== 'all' && !bucketHasChannel(bucket, view.channel)) return false;
    if (view.account && !bucketHasAccount(bucket, view.account)) return false;
    if (cutoff && isoToMs(bucket.lastMessageAt) < cutoff) return false;
    if (view.search) {
      const hay = [
        bucket.displayName,
        ...(bucket.participants || []),
        ...[...(bucket.subjects || [])],
        ...(bucket.threads || []).map((th) => th.account_key),
      ].filter(Boolean).join(' ').toLowerCase();
      if (!hay.includes(view.search)) return false;
    }
    return true;
  });
}

function hasActiveListFilters(view = {}) {
  return Boolean(
    view.search
    || (view.channel && view.channel !== 'all')
    || view.account
    || (view.direction && view.direction !== 'any')
    || (view.dateRange && view.dateRange !== 'any'),
  );
}

function errorToText(error) {
  if (!error) return '';
  if (typeof error === 'string') return error;
  return error.message || String(error);
}

// Route-Parameter (api.shell.onRoute, v3: #conversations?…) → Deep-Link-Zustand; Parameter-Semantik unverändert.
function parseConversationDeepLink(params = {}) {
  const get = (key) => {
    if (!params || typeof params !== 'object') return '';
    if (typeof params.get === 'function') return params.get(key);
    const value = params[key];
    return value == null || typeof value === 'object' ? '' : value;
  };
  return {
    campaign_id: cleanParam(get('campaign_id')),
    engagement_id: cleanParam(get('engagement_id')),
    outbound_message_id: cleanParam(get('outbound_message_id') || get('message_id')),
    thread_key: cleanParam(get('thread_key')),
    message_key: cleanParam(get('message_key')),
    account_key: cleanParam(get('account_key')),
    channel: normalizeConversationChannel(get('channel')),
    filtersApplied: false,
    resolved: false,
  };
}

function plainDeepLink(link = {}) {
  const out = {};
  for (const key of ['campaign_id', 'engagement_id', 'outbound_message_id', 'thread_key', 'message_key', 'account_key', 'channel']) {
    if (link?.[key]) out[key] = String(link[key]);
  }
  return out;
}

function metadataMatchesOutboundLink(metadata, link = {}) {
  if (!metadata || typeof metadata !== 'object') return false;
  const values = new Set();
  collectMetadataValues(metadata, values);
  return Boolean(
    (link.campaign_id && values.has(link.campaign_id))
    || (link.engagement_id && values.has(link.engagement_id))
    || (link.outbound_message_id && values.has(link.outbound_message_id)),
  );
}

function outboundLinkFromMetadata(metadata) {
  if (!metadata || typeof metadata !== 'object') return null;
  const link = {
    campaign_id: firstMetadataValue(metadata, ['campaign_id', 'outbound_campaign_id']),
    engagement_id: firstMetadataValue(metadata, ['engagement_id', 'outbound_engagement_id']),
    outbound_message_id: firstMetadataValue(metadata, ['outbound_message_id', 'message_id']),
    thread_key: firstMetadataValue(metadata, ['thread_key', 'communication_thread_key']),
    message_key: firstMetadataValue(metadata, ['message_key', 'communication_message_key']),
    account_key: firstMetadataValue(metadata, ['account_key', 'communication_account_key']),
    channel: normalizeConversationChannel(firstMetadataValue(metadata, ['channel'])),
    applied: false,
    resolved: false,
  };
  return Object.values(link).some(Boolean) ? link : null;
}

function firstMetadataValue(value, keys) {
  if (!value || typeof value !== 'object') return '';
  if (Array.isArray(value)) {
    for (const item of value) {
      const nested = firstMetadataValue(item, keys);
      if (nested) return nested;
    }
    return '';
  }
  for (const key of keys) {
    if (value[key] != null && typeof value[key] !== 'object') return cleanParam(value[key]);
  }
  for (const item of Object.values(value)) {
    const nested = firstMetadataValue(item, keys);
    if (nested) return nested;
  }
  return '';
}

function collectMetadataValues(value, out) {
  if (value == null) return;
  if (typeof value === 'string' || typeof value === 'number') {
    out.add(String(value));
    return;
  }
  if (Array.isArray(value)) {
    value.forEach((item) => collectMetadataValues(item, out));
    return;
  }
  if (typeof value === 'object') {
    Object.values(value).forEach((item) => collectMetadataValues(item, out));
  }
}

function cleanParam(value) {
  return String(value || '').trim();
}

function normalizeConversationChannel(value) {
  const channel = cleanParam(value);
  if (channel === 'physical_letter') return 'email';
  return SUPPORTED_CHANNELS.includes(channel) || channel === 'all' ? channel : '';
}

function contactForOutboundEngagement(pipeline, engagement) {
  const contacts = Array.isArray(pipeline?.contacts) ? pipeline.contacts : [];
  if (!contacts.length || !engagement) return null;
  const contactId = cleanParam(engagement.contact_id);
  const email = cleanParam(engagement.payload?.contact_email || '').toLowerCase();
  return contacts.find((contact) => {
    const ids = [contact.id, contact.contact_id, contact.outbound_contact_id, contact.key]
      .map(cleanParam)
      .filter(Boolean);
    return contactId && ids.includes(contactId);
  }) || contacts.find((contact) => {
    const candidate = cleanParam(contact.email || contact.email_address).toLowerCase();
    return email && candidate === email;
  }) || null;
}

function dateCutoff(range) {
  const now = Date.now();
  if (range === 'today') {
    const start = new Date();
    start.setHours(0, 0, 0, 0);
    return start.getTime();
  }
  if (range === '7d') return now - 7 * DAY_MS;
  if (range === '30d') return now - 30 * DAY_MS;
  return 0;
}

function participantsOf(thread) {
  const raw = Array.isArray(thread?.participant_keys_json) ? thread.participant_keys_json : [];
  return raw
    .map((entry) => (typeof entry === 'string' ? entry : entry?.key || entry?.address || ''))
    .filter(Boolean);
}

function bucketKeyFor(participants) {
  if (!participants.length) return '__unknown__';
  const normalized = participants
    .map((p) => String(p).trim().toLowerCase())
    .filter(Boolean)
    .sort();
  return normalized.join('|') || '__unknown__';
}

function deriveBucketDisplay(participants) {
  if (!participants.length) return '';
  const cleaned = participants.map((p) => String(p).trim()).filter(Boolean);
  if (cleaned.length <= 2) return cleaned.join(', ');
  return `${cleaned[0]} +${cleaned.length - 1}`;
}

function formatParticipantHandles(participants) {
  return participants.join(', ');
}

function sumValues(map) {
  let total = 0;
  for (const value of map.values()) total += value;
  return total;
}

function labelForChannel(channel, t) {
  if (!channel) return '';
  const suffix = String(channel)
    .split('_')
    .filter(Boolean)
    .map((part) => `${part[0].toUpperCase()}${part.slice(1)}`)
    .join('');
  return t(`channel${suffix}`, CHANNEL_LABEL_FALLBACK[channel] || channel);
}

function labelForFolder(folder, t) {
  if (folder === 'inbox') return t('folderInbox', 'Posteingang');
  if (folder === 'sent') return t('folderSent', 'Gesendet');
  if (folder === 'archive') return t('folderArchive', 'Archiv');
  if (folder === 'spam') return t('folderSpam', 'Spam');
  if (folder === 'drafts') return t('folderDrafts', 'Entwürfe');
  return folder || '';
}

function labelForStatus(status, t) {
  switch (status) {
    case 'sent': return t('statusSent', 'gesendet');
    case 'delivered': return t('statusDelivered', 'zugestellt');
    case 'read': return t('statusRead', 'gelesen');
    case 'received': return t('statusReceived', 'eingegangen');
    case 'failed': return t('statusFailed', 'fehlgeschlagen');
    case 'queued': return t('statusQueued', 'in Warteschlange'); // v4-allow: label (Nachrichtenstatus)
    case 'done': return t('statusDone', 'erledigt');
    default: return status || '';
  }
}

function iconForStatus(status) {
  switch (status) {
    case 'sent': return '✓';
    case 'delivered': return '✓✓';
    case 'read': return '✓✓';
    case 'received': return '↓';
    case 'failed': return '⚠';
    case 'queued': return '⏳';
    default: return '·';
  }
}

// Kit badge variants (.ctox-badge.is-*) for message send state.
function badgeVariantForMessageStatus(status) {
  switch (status) {
    case 'failed': return 'is-danger';
    case 'delivered':
    case 'read': return 'is-info';
    default: return '';
  }
}

// Kit badge variants for queue route state on task/work chips.
function badgeVariantForRouteStatus(status) {
  switch (String(status || '').toLowerCase()) {
    case 'queued':
    case 'pending':
    case 'accepted': return 'is-warning';
    case 'leased':
    case 'running':
    case 'working': return 'is-info';
    case 'done':
    case 'completed':
    case 'handled': return 'is-success';
    case 'failed':
    case 'blocked':
    case 'stale_missing_native':
    case 'cancelled':
    case 'canceled': return 'is-danger';
    default: return '';
  }
}

function labelForRouteStatus(status, t) {
  switch (String(status || '').toLowerCase()) {
    case 'pending':
    case 'accepted':
    case 'queued': return t('routeQueued', 'in Queue');
    case 'leased': return t('routeLeased', 'gelistet');
    case 'running':
    case 'working': return t('routeRunning', 'läuft');
    case 'done':
    case 'completed': return t('routeDone', 'fertig');
    case 'handled': return t('routeHandled', 'erledigt');
    case 'failed': return t('routeFailed', 'fehlgeschlagen');
    case 'cancelled':
    case 'canceled': return t('routeCancelled', 'abgebrochen');
    case 'blocked':
    case 'stale_missing_native': return t('routeBlocked', 'blockiert');
    default: return status || '';
  }
}

function labelForTrust(level, t) {
  switch (level) {
    case 'high': return t('trustHigh', 'Hoch');
    case 'medium': return t('trustMedium', 'Mittel');
    case 'low': return t('trustLow', 'Niedrig');
    case 'unknown': return t('trustUnknown', 'Unbekannt');
    default: return level || '';
  }
}

function labelForReplyClassification(value) {
  switch (value) {
    case 'positive': return 'positiv';
    case 'negative': return 'negativ';
    case 'objection': return 'Einwand';
    case 'out_of_office': return 'Out of office';
    case 'referral': return 'Weiterleitung';
    case 'unsubscribe': return 'Unsubscribe';
    case 'unclear': return 'unklar';
    default: return value || '';
  }
}

function avatarGlyphFor(name) {
  const trimmed = String(name || '').trim();
  if (!trimmed) return '?';
  const parts = trimmed.split(/\s+/);
  const initials = parts.length >= 2
    ? `${parts[0][0] || ''}${parts[parts.length - 1][0] || ''}`
    : trimmed.slice(0, 2);
  return initials.toUpperCase();
}

function healthStateFor(account, t = (_key, fallback) => fallback) {
  const adapterStatus = accountAdapterStatus(account);
  const authState = String(adapterStatus.auth_state || '').toLowerCase();
  const syncState = String(adapterStatus.sync_state || '').toLowerCase();
  const rateLimitedUntil = Number(adapterStatus.rate_limited_until_ms || 0);
  if (authState === 'failed' || authState === 'deauthorized') {
    return { state: 'bad', tooltip: adapterHealthTooltip(adapterStatus, t) };
  }
  if (syncState === 'failed') {
    return { state: 'bad', tooltip: adapterHealthTooltip(adapterStatus, t) };
  }
  if (rateLimitedUntil > Date.now()) {
    return { state: 'warn', tooltip: adapterHealthTooltip(adapterStatus, t) };
  }
  if (adapterStatus.last_success_at_ms) {
    const age = Date.now() - Number(adapterStatus.last_success_at_ms);
    if (age < ACCOUNT_HEALTH_WARN_MS) {
      return { state: 'ok', tooltip: adapterHealthTooltip(adapterStatus, t) };
    }
  }
  const latest = Math.max(isoToMs(account?.last_inbound_ok_at), isoToMs(account?.last_outbound_ok_at));
  if (!latest) return { state: 'bad', tooltip: 'never' };
  const age = Date.now() - latest;
  if (age < ACCOUNT_HEALTH_WARN_MS) return { state: 'ok', tooltip: '' };
  if (age < ACCOUNT_HEALTH_BAD_MS) return { state: 'warn', tooltip: '' };
  return { state: 'bad', tooltip: '' };
}

function accountAdapterStatus(account) {
  let profile = account?.profile_json || {};
  if (typeof profile === 'string') {
    try {
      profile = JSON.parse(profile);
    } catch {
      profile = {};
    }
  }
  return profile.adapterStatus || profile.adapter_status || {};
}

function adapterHealthTooltip(adapterStatus, t) {
  const kind = String(adapterStatus?.provider_error_kind || '').toLowerCase();
  if (kind && kind !== 'none') {
    if (kind === 'deauthorized') return t('healthAuthFailed', 'Auth fehlgeschlagen');
    if (kind === 'missing_scope') return t('healthMissingScope', 'Scopes fehlen');
    if (kind === 'missing_intent') return t('healthMissingIntent', 'Discord Intent fehlt');
    if (kind === 'missing_permission') return t('healthMissingPermission', 'Berechtigung fehlt');
    if (kind === 'rate_limited') return t('healthRateLimited', 'Rate Limit aktiv');
    return kind;
  }
  if (adapterStatus?.last_operation) {
    return `${adapterStatus.last_operation}: ${adapterStatus.sync_state || adapterStatus.auth_state || 'ok'}`;
  }
  return '';
}

function formatRelativeOrNever(iso, t) {
  if (!iso) return t('healthNever', 'nie');
  return formatDateShort(iso);
}

function compareIsoAsc(a, b) {
  const ta = isoToMs(a);
  const tb = isoToMs(b);
  return ta - tb;
}

function compareIsoDesc(a, b) {
  const ta = isoToMs(a);
  const tb = isoToMs(b);
  return tb - ta;
}

function isoToMs(value) {
  if (!value) return 0;
  const t = Date.parse(value);
  return Number.isFinite(t) ? t : 0;
}

function formatTimeShort(iso) {
  const ms = isoToMs(iso);
  if (!ms) return '';
  const date = new Date(ms);
  const now = new Date();
  const sameDay = date.toDateString() === now.toDateString();
  if (sameDay) {
    return `${pad(date.getHours())}:${pad(date.getMinutes())}`;
  }
  const sameYear = date.getFullYear() === now.getFullYear();
  if (sameYear) {
    return `${pad(date.getDate())}.${pad(date.getMonth() + 1)}.`;
  }
  return `${pad(date.getDate())}.${pad(date.getMonth() + 1)}.${date.getFullYear()}`;
}

function formatDateShort(iso) {
  const ms = isoToMs(iso);
  if (!ms) return '—';
  const date = new Date(ms);
  return `${pad(date.getDate())}.${pad(date.getMonth() + 1)}.${date.getFullYear()} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

function dayKeyFor(iso) {
  const ms = isoToMs(iso);
  if (!ms) return '';
  const date = new Date(ms);
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

function formatDayLabel(iso, t) {
  const ms = isoToMs(iso);
  if (!ms) return '';
  const date = new Date(ms);
  const now = new Date();
  const today = now.toDateString();
  const yesterday = new Date(now.getTime() - 86400000).toDateString();
  if (date.toDateString() === today) return t('todayLabel', 'Heute');
  if (date.toDateString() === yesterday) return t('yesterdayLabel', 'Gestern');
  return `${pad(date.getDate())}.${pad(date.getMonth() + 1)}.${date.getFullYear()}`;
}

function pad(value) {
  return String(value).padStart(2, '0');
}

// Markup relativ und ohne ?v= (§16.15: der Service Worker liefert die Artefaktdatei versioniert). CSS lädt die Shell
// aus dem Manifest (styles).
async function loadModuleMarkup() {
  const response = await fetch(new URL('./index.html', import.meta.url));
  return response.text();
}

function langFrom(locale) {
  return String(locale || '').toLowerCase().startsWith('en') ? 'en' : 'de';
}

// api.shell.t ist live; die eingebetteten Labels sind der Rückfall, solange der Schlüsselraum von t() offen ist
// (app-api.d.ts OFFEN-B). Schlüsselraum `conversations.<key>`.
function messagesFor(api, lang) {
  const base = { ...FALLBACK_LABELS.de, ...(FALLBACK_LABELS[lang] || {}) };
  const translate = api?.shell?.t;
  if (typeof translate !== 'function') return base;
  for (const key of Object.keys(base)) {
    const scoped = `conversations.${key}`;
    const value = translate.call(api.shell, scoped);
    if (typeof value === 'string' && value && value !== scoped) base[key] = value;
  }
  return base;
}

function isPlainObject(value) {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function conversationRecordContext(bucket = {}) {
  const id = String(bucket.key || '');
  return {
    'data-context-record-id': id,
    'data-context-record-type': 'conversation',
    'data-context-label': String(bucket.displayName || id || 'Conversation'),
  };
}

function messageRecordContext(message = {}) {
  const id = String(message.message_key || message.id || message.external_message_id || message.external_created_at || '');
  const rawLabel = message.subject || message.sender_display || message.sender_address || message.preview || message.body_text || `${message.channel || 'Conversation'} message`;
  const label = String(rawLabel).replace(/\s+/g, ' ').trim().slice(0, 160);
  return {
    'data-context-record-id': id,
    'data-context-record-type': 'conversation_message',
    'data-context-label': label || id || 'Conversation message',
  };
}

// Auto-reveal model (design-guide "Progressive Disclosure", outbound idiom):
// the context pane is shown only when a conversation is selected and the user
// has not collapsed it.
function conversationContextVisible(hasSelection, userCollapsed) {
  return Boolean(hasSelection) && !userCollapsed;
}

// Counted channel band: `all` = bucket count; every supported channel gets its
// own count with zeros included (never hidden), so the band reads honestly.
function channelBandCounts(buckets) {
  const rows = Array.isArray(buckets) ? buckets : [];
  const counts = { all: rows.length };
  for (const channel of SUPPORTED_CHANNELS) counts[channel] = 0;
  for (const bucket of rows) {
    const channels = bucket.channels instanceof Set ? [...bucket.channels] : (bucket.channels || []);
    for (const channel of channels) {
      if (Object.prototype.hasOwnProperty.call(counts, channel)) counts[channel] += 1;
    }
  }
  return counts;
}

// Export = JSON of the visible conversations (honest and small — the actual
// thread records plus whatever timeline messages are loaded locally).
function buildConversationsExport(buckets, messages, nowMs) {
  const rows = Array.isArray(buckets) ? buckets : [];
  const byThread = {};
  for (const message of Array.isArray(messages) ? messages : []) {
    const key = message && message.thread_key;
    if (!key) continue;
    (byThread[key] ||= []).push(message);
  }
  return {
    kind: 'ctox-conversations-export',
    exported_at_ms: Number(nowMs) || 0,
    conversations: rows.map((bucket) => ({
      key: bucket.key,
      display_name: bucket.displayName,
      participants: [...(bucket.participants || [])],
      channels: bucket.channels instanceof Set ? [...bucket.channels] : [...(bucket.channels || [])],
      account_keys: bucket.accountKeys instanceof Set ? [...bucket.accountKeys] : [...(bucket.accountKeys || [])],
      threads: (bucket.threads || []).map((thread) => ({ ...thread })),
    })),
    messages: byThread,
  };
}

// Import → a LOCAL-ONLY review overlay (never persisted). Accepts an exported
// snapshot ({ conversations: [...], messages: {...} }) or a bare conversations
// array; keeps only threads with a thread_key.
function parseConversationsImport(raw) {
  const src = raw && typeof raw === 'object' ? raw : {};
  const conversations = Array.isArray(raw)
    ? raw
    : (Array.isArray(src.conversations) ? src.conversations : []);
  const threads = [];
  for (const conversation of conversations) {
    if (!conversation || typeof conversation !== 'object') continue;
    const list = Array.isArray(conversation.threads) ? conversation.threads : [];
    for (const thread of list) {
      if (thread && typeof thread === 'object' && thread.thread_key) {
        threads.push({ ...thread, __imported: true });
      }
    }
  }
  const messagesByThread = new Map();
  const rawMessages = src.messages && typeof src.messages === 'object' ? src.messages : {};
  for (const [threadKey, list] of Object.entries(rawMessages)) {
    if (!Array.isArray(list)) continue;
    messagesByThread.set(
      threadKey,
      list.filter((message) => message && typeof message === 'object').map((message) => ({ ...message, __imported: true })),
    );
  }
  return { threads, messagesByThread };
}

export const __conversationsTestHooks = {
  filterBuckets,
  hasActiveListFilters,
  parseConversationDeepLink,
  conversationContextVisible,
  conversationRecordContext,
  messageRecordContext,
  channelBandCounts,
  buildConversationsExport,
  parseConversationsImport,
};
